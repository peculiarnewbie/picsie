//! Local layer-appearance and mask workload benchmark, not a translated
//! upstream test or production engine. Uses existing editor commands only.
//!
//! Compositor 609dbeae semantic reference (functionality only, adapted to the
//! Rust command model):
//! - `Compositor/Document/LayerAppearance.swift`: `beginOpacityEdit`,
//!   `finishOpacityEdit`, `setLayerOpacity`, `previewBlendMode`,
//!   `setLayerBlendMode`, `cycleBlendMode`.
//! - `Compositor/Document/EditorSession.swift`: `toggleLayerVisibility`,
//!   `beginVisibilitySwipe`, `setVisibilityInSwipe`, `endVisibilitySwipe`.
//! - `Compositor/Document/LayerMask.swift`: `addMask`, `toggleLayerMask`
//!   (enable/disable), `deleteLayerMask`, mask brush painting.
//! GIMP 3.2.6 (`e101dd19`) is the secondary functionality reference:
//! `app/core/gimplayer.c` (`gimp_layer_set_opacity`, `gimp_layer_set_mode`),
//! `app/core/gimplayermask.c` and `app/pdb/layer-cmds.c`
//! (`gimp-layer-add-mask`, `gimp-layer-remove-mask`,
//! `gimp-layer-get/set-apply-mask`, `gimp-layer-set-composite-space`).
//!
//! Paired GIMP workloads live in `scripts/perf/gimp-feature-workload.py` and
//! are orchestrated by `scripts/compare-feature-performance.py`.
//!
//! Selection evolution workloads (`SELECTION_CASES`) reuse this scaffold with
//! a segmented two-square fixture and a split command/coverage/render/read
//! boundary: `coverage_ms` materializes `render::selection_coverage` (the
//! selected-coverage availability proof), separately from the end-to-end
//! document RGBA. The unchanged document composite is never treated as the
//! selection output check; coverage is materialized and validated every
//! sample, the first measured sample's `.selcov` channel dump per
//! case/size/trial is written, and the trial-1 dumps are compared across
//! applications with the `compare-sel` subcommand.
//!
//! Compositor 609dbeae selection reference (functionality only):
//! - `Compositor/Document/Selection.swift`: `DocumentSelection.coverage()`,
//!   `coverageBounds`, `clip`, `resizeSelection` (expand/contract band),
//!   invert as canvas-minus-outline, `applySelection` add/subtract.
//! - `Compositor/Document/MagicWand.swift` + `WandPixels.c`: point-sample,
//!   per-channel tolerance match on premultiplied pixels, contiguous flood vs
//!   full-canvas match, exact pixel-edge outlining.
//! - `Compositor/Document/EditorSession.swift` thumbnail selection: layer
//!   alpha silhouette as the outline source (`SelectLayerPixels`).
//! GIMP e101dd19 secondary reference: `app/core/gimpchannel-select.c`
//! (`gimp_channel_select_fuzzy`, `gimp_channel_select_by_color`,
//! `gimp_channel_select_alpha`), `app/pdb/selection-cmds.c`
//! (`gimp-selection-invert/grow/shrink`), `app/pdb/image-select-cmds.c`
//! (`gimp-image-select-color/contiguous-color/item`).
//!
//! Subcommands: `prepare`, `run`, `compare` (test-only RGBA comparator) and
//! `compare-sel` (test-only single-channel selection-coverage comparator);
//! benchmark verification, not production renderers.
//!
//! Recent-features workloads (`RECENT_CASES`, batch 3) reuse this scaffold
//! with modest matched fixtures and the same split command/render/read
//! boundary: group resize/rotate preserve the folder hierarchy and carry
//! every member from its original placement in one undo
//! (`editor/group_transform.rs`, Compositor 609dbeae `LayerTransform.swift`
//! `TransformGroup`/`following`/`placing` via `EditorSession.swift`
//! `groupTransformMembers`/`groupTransformBox`); linear image/mask gradients
//! use matched black-to-white endpoints across the layer footprint
//! (`gradient.rs`, Compositor 609dbeae `Gradient.swift` linear shape,
//! foreground-to-background commit as one undo); Levels/Curves time a live
//! `UpdateAdjustment*` preview inside the open adjustment transaction
//! (`editor/adjustments.rs`, Compositor 609dbeae `Levels.swift`
//! `updateLevels` / `Curves.swift`). GIMP 3.2.6 (`e101dd19`) controls:
//! `gimp-item-transform-scale/rotate` on a group layer (bakes resampling
//! into the children — labeled, not equal-quality parity with Picsie's live
//! transforms), `gimp-drawable-edit-gradient-fill` LINEAR on the layer and
//! mask drawables, and non-destructive `gimp:levels` / `gimp:curves`
//! drawable filters (`Gimp.DrawableFilter.new` + config + `append_filter` +
//! `update`, the endorsed replacement for the deprecated
//! `gimp-drawable-levels/curves-spline`). Mask gradients dump real mask
//! coverage (`.maskcov`) compared with the single-channel comparator.
use anyhow::{Result, ensure};
use picsie_core::{
    adjustment::{AdjustmentKind, CurvePoint, CurvesSettings, LevelsSettings},
    editor::{
        Command, Editor, Modifiers, PaintTarget, Phase, PointerSample, SelectionMode, Tool,
        TransformField,
    },
    files,
    geometry::Viewport,
    gradient::GradientStyle,
    model::{Blend, Content, Document, Layer, LayerMask, MaskMode, MaskRaster, Point},
    pixel_selection::MarqueeKind,
    render::{self, Renderer},
    wand::WandSettings,
};
use serde_json::json;
use std::{fs, hint::black_box, io::Read, path::Path, sync::Arc, time::Instant};

/// Layer-appearance and mask workloads. `opacity-preview` exercises the live
/// slider-drag preview without committing; it has no GIMP equivalent and is
/// Picsie-only. All other cases pair with a public GIMP API workload.
const CASES: [&str; 7] = [
    "opacity-preview",
    "opacity-commit",
    "visibility-toggle",
    "blend-multiply",
    "blend-screen",
    "mask-disabled",
    "mask-paint",
];
/// Selection evolution workloads. Input shape, antialiasing, sampling scope,
/// tolerance and alpha semantics are matched with the GIMP driver:
/// axis-aligned center-half rectangle (AA-neutral) for invert/expand/contract;
/// point-sample tolerance-0 wand on the selected layer only (contiguous flood
/// vs full-canvas match); layer-alpha silhouette for `select-layer-alpha`.
/// Existing fill/clear/feather cohorts already cover those behaviors and are
/// not duplicated here.
const SELECTION_CASES: [&str; 6] = [
    "select-inverse",
    "select-expand5",
    "select-contract5",
    "wand-contiguous",
    "wand-noncontiguous",
    "select-layer-alpha",
];
/// Recent-features workloads (batch 3). Group transforms preserve the folder
/// hierarchy and member geometry in one undo; gradients use matched
/// black-to-white linear endpoints; Levels/Curves time one live preview
/// update inside the open adjustment transaction (no commit in the timer).
const RECENT_CASES: [&str; 6] = [
    "group-resize",
    "group-rotate",
    "gradient-image-linear",
    "gradient-mask-linear",
    "levels-update",
    "curves-update",
];
const SIZES: [(u32, u32); 2] = [(1200, 800), (3600, 2400)];

/// Shared matched fixture constants. The GIMP workload builds the same layers
/// procedurally: same canvas sizes, colors, middle-layer rect and mask split.
const TOP: [u8; 4] = [200, 80, 80, 255]; // #c85050 full-canvas selected layer.
const MIDDLE: [u8; 4] = [63, 127, 191, 255]; // #3f7fbf offset rect layer.
const BOTTOM: [u8; 4] = [224, 208, 64, 255]; // #e0d040 full-canvas base.
const OPACITY: f64 = 0.35;
/// 35% top over bottom-only corner: approximately [216, 163, 70].
const CORNER_OPACITY: [u8; 4] = [216, 163, 70, 255];
/// Multiply top over bottom: [176, 65, 20]. Screen top over bottom: [248, 223, 124].
const CORNER_MULTIPLY: [u8; 4] = [176, 65, 20, 255];
const CORNER_SCREEN: [u8; 4] = [248, 223, 124, 255];
/// Mid-gray base for gradient/adjustment fixtures (#808080). Levels
/// black=64 maps it to ~85; the curves mid lift maps it to 192.
const GRAY: [u8; 4] = [128, 128, 128, 255];
/// Deterministic Levels control: input black 64 on the RGB composite range.
const LEVELS_BLACK: f64 = 64.;
/// Deterministic Curves control: RGB composite mid lift 128 -> 192.
const CURVES_MID: (f64, f64) = (128., 192.);

fn solid_layer(name: &str, width: u32, height: u32, color: [u8; 4]) -> Result<Layer> {
    let pixels = color.repeat(width as usize * height as usize);
    Ok(Layer::new(
        name,
        width,
        height,
        render::native_content(render::rgba_image(width, height, &pixels)?),
    ))
}

fn fixture(width: u32, height: u32, masked: bool) -> Result<Document> {
    let mut doc = Document::new("Performance features", width, height)?;
    doc.layers
        .push(solid_layer("Bottom", width, height, BOTTOM)?);
    let (mw, mh) = (width * 3 / 4, height * 3 / 4);
    let mut middle = solid_layer("Middle", mw, mh, MIDDLE)?;
    middle.x = width as f64 / 8.;
    middle.y = height as f64 / 8.;
    doc.layers.push(middle);
    let mut top = solid_layer("Top", width, height, TOP)?;
    if masked {
        // Left half revealed, right half hidden: disabling the mask or
        // painting the revealed side both change the composite.
        let mut coverage = vec![0u8; width as usize * height as usize];
        for y in 0..height as usize {
            for x in 0..width as usize / 2 {
                coverage[y * width as usize + x] = 255;
            }
        }
        top.mask = Some(Arc::new(LayerMask {
            enabled: true,
            base: MaskMode::Reveal,
            raster: Some(Arc::new(MaskRaster {
                width,
                height,
                pixels: Arc::new(coverage),
            })),
            linked: true,
            placement: None,
            strokes: vec![],
        }));
    }
    doc.layers.push(top);
    Ok(doc)
}

fn masked(case: &str) -> bool {
    case == "mask-disabled" || case == "mask-paint"
}

fn is_selection_case(case: &str) -> bool {
    SELECTION_CASES.contains(&case)
}

fn is_recent_case(case: &str) -> bool {
    RECENT_CASES.contains(&case)
}

/// Modest matched group fixture: opaque base plus a folder carrying two
/// raster members (red 1/3 x 1/4 at the 1/12 offset, blue 1/6 x 1/3 at
/// 2/3 x 1/3). Group transforms must carry both members from their
/// original placements — never a pre-flattened image.
fn group_fixture(width: u32, height: u32) -> Result<Document> {
    let mut doc = Document::new("Performance groups", width, height)?;
    doc.layers
        .push(solid_layer("Bottom", width, height, BOTTOM)?);
    let mut folder = Layer::new("Group", width, height, Content::Group);
    folder.x = 0.;
    folder.y = 0.;
    let folder_id = folder.id.clone();
    doc.layers.push(folder);
    let (aw, ah) = (width / 3, height / 4);
    let mut a = solid_layer("A", aw, ah, TOP)?;
    a.x = width as f64 / 12.;
    a.y = height as f64 / 12.;
    a.parent_id = Some(folder_id.clone());
    doc.layers.push(a);
    let (bw, bh) = (width / 6, height / 3);
    let mut b = solid_layer("B", bw, bh, MIDDLE)?;
    b.x = width as f64 * 2. / 3.;
    b.y = height as f64 / 3.;
    b.parent_id = Some(folder_id);
    doc.layers.push(b);
    doc.validate()?;
    Ok(doc)
}

/// Single mid-gray layer for the linear image gradient (black-to-white
/// across the full width at mid-height).
fn gradient_fixture(width: u32, height: u32) -> Result<Document> {
    let mut doc = Document::new("Performance gradient", width, height)?;
    doc.layers.push(solid_layer("Base", width, height, GRAY)?);
    doc.validate()?;
    Ok(doc)
}

/// Opaque base plus a full-canvas red layer behind a solid white Reveal
/// mask: the mask gradient modulates real coverage left to right.
fn mask_gradient_fixture(width: u32, height: u32) -> Result<Document> {
    let mut doc = Document::new("Performance mask gradient", width, height)?;
    doc.layers
        .push(solid_layer("Bottom", width, height, BOTTOM)?);
    let mut top = solid_layer("Top", width, height, TOP)?;
    top.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        raster: Some(Arc::new(MaskRaster {
            width,
            height,
            pixels: Arc::new(vec![255u8; width as usize * height as usize]),
        })),
        linked: true,
        placement: None,
        strokes: vec![],
    }));
    doc.layers.push(top);
    doc.validate()?;
    Ok(doc)
}

/// Mid-gray base with a baked white square (side = width/8, same placement
/// as the selection squares) for the Levels/Curves live updates. A single
/// opaque layer keeps both engines' adjustment inputs byte-identical; both
/// tones give meaningful probes under either control.
fn adjustment_fixture(width: u32, height: u32) -> Result<Document> {
    let mut doc = Document::new("Performance adjustments", width, height)?;
    let mut pixels = GRAY.repeat(width as usize * height as usize);
    let side = width / 8;
    let (x0, y0) = (width / 8, height / 2 - side / 2);
    for y in y0..y0 + side {
        for x in x0..x0 + side {
            let offset = ((y * width + x) * 4) as usize;
            pixels[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    }
    doc.layers.push(Layer::new(
        "Base",
        width,
        height,
        render::native_content(render::rgba_image(width, height, &pixels)?),
    ));
    doc.validate()?;
    Ok(doc)
}

fn recent_fixture(case: &str, width: u32, height: u32) -> Result<Document> {
    match case {
        "group-resize" | "group-rotate" => group_fixture(width, height),
        "gradient-image-linear" => gradient_fixture(width, height),
        "gradient-mask-linear" => mask_gradient_fixture(width, height),
        "levels-update" | "curves-update" => adjustment_fixture(width, height),
        _ => anyhow::bail!("Unknown recent workload: {case}"),
    }
}

/// Segmented selection fixture: opaque base plus a transparent selected layer
/// carrying two separated opaque red squares. A solid canvas would make the
/// wand cases meaningless (everything matches); the two disconnected squares
/// separate contiguous from noncontiguous matching, and the surrounding
/// transparency exercises alpha handling on both sides.
fn selection_fixture(width: u32, height: u32) -> Result<Document> {
    let mut doc = Document::new("Performance selections", width, height)?;
    doc.layers
        .push(solid_layer("Bottom", width, height, BOTTOM)?);
    let side = width / 8;
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    for (x0, name) in [(width / 8, "left"), (width - width / 8 - side, "right")] {
        let _ = name;
        let y0 = height / 2 - side / 2;
        for y in y0..y0 + side {
            for x in x0..x0 + side {
                let offset = ((y * width + x) * 4) as usize;
                pixels[offset..offset + 4].copy_from_slice(&TOP);
            }
        }
    }
    doc.layers.push(Layer::new(
        "Top",
        width,
        height,
        render::native_content(render::rgba_image(width, height, &pixels)?),
    ));
    Ok(doc)
}

/// Center-half input rectangle shared by the invert/expand/contract cases.
fn input_rect(doc: &Document) -> (u32, u32, u32, u32) {
    (doc.width / 4, doc.height / 4, doc.width / 2, doc.height / 2)
}

/// Click at the center of the left red square for both wand cases.
fn wand_point(doc: &Document) -> Point {
    Point::new(
        (doc.width / 8 + doc.width / 16) as f64,
        (doc.height / 2) as f64,
    )
}

/// Center of the right red square (untouched by the contiguous wand).
fn right_square_center(doc: &Document) -> (u32, u32) {
    (doc.width - doc.width / 8 - doc.width / 16, doc.height / 2)
}

/// Materialized selection coverage at document resolution (feather is 0 in
/// every case here, so this is the stored coverage). Empty (no) selection
/// materializes as all-zero coverage with no bounds.
fn coverage_of(editor: &Editor) -> Result<Vec<u8>> {
    let doc = &editor.history.document;
    match &editor.history.pixel_selection {
        Some(selection) => {
            ensure!(selection.width == doc.width && selection.height == doc.height);
            Ok(render::selection_coverage(selection)?.to_vec())
        }
        None => Ok(vec![0u8; doc.width as usize * doc.height as usize]),
    }
}

fn coverage_at(coverage: &[u8], doc: &Document, x: u32, y: u32) -> u8 {
    coverage[(y * doc.width + x) as usize]
}

fn selected_count(coverage: &[u8]) -> u64 {
    coverage.iter().filter(|v| **v > 0).count() as u64
}

fn prepare(folder: &Path) -> Result<()> {
    fs::create_dir_all(folder)?;
    for (width, height) in SIZES {
        // Baseline composite (no mask, normal blend, full opacity) for
        // provenance and cross-application fixture matching.
        let doc = fixture(width, height, false)?;
        let image = Renderer::default().render(&doc)?.image_snapshot();
        fs::write(
            folder.join(format!("{width}.png")),
            render::encode(&image, false)?,
        )?;
        files::save_project(&folder.join(format!("{width}.picsie")), &doc)?;
        // Segmented selection fixture baseline (untouched composite) for
        // provenance and cross-application fixture matching.
        let selections = selection_fixture(width, height)?;
        let selection_image = Renderer::default().render(&selections)?.image_snapshot();
        fs::write(
            folder.join(format!("{width}-selections.png")),
            render::encode(&selection_image, false)?,
        )?;
        files::save_project(
            &folder.join(format!("{width}-selections.picsie")),
            &selections,
        )?;
        // Recent-features baselines (untouched composites) for provenance
        // and cross-application fixture matching.
        for (suffix, recent) in [
            ("groups", group_fixture(width, height)?),
            ("gradient", gradient_fixture(width, height)?),
            ("gradient-mask", mask_gradient_fixture(width, height)?),
            ("adjustments", adjustment_fixture(width, height)?),
        ] {
            let image = Renderer::default().render(&recent)?.image_snapshot();
            fs::write(
                folder.join(format!("{width}-{suffix}.png")),
                render::encode(&image, false)?,
            )?;
            files::save_project(&folder.join(format!("{width}-{suffix}.picsie")), &recent)?;
        }
    }
    Ok(())
}

fn setup(doc: &Document, case: &str) -> Result<Editor> {
    let mut editor = Editor::new(doc.clone())?;
    editor.viewport = Viewport {
        width: doc.width as f64,
        height: doc.height as f64,
        zoom: 1.,
        pan: Point::default(),
    };
    if case == "mask-paint" {
        // Untimed setup; the timed section paints one brush gesture only.
        editor.tool = Tool::Brush;
        editor.paint_target = PaintTarget::Mask;
        editor.brush_size = 100.;
        editor.brush_hardness = 1.;
        editor.brush_opacity = 1.;
        editor.brush_smoothing = 0.;
    }
    if matches!(
        case,
        "select-inverse" | "select-expand5" | "select-contract5"
    ) {
        // Untimed input-shape setup: an axis-aligned center-half marquee,
        // which is antialias-neutral on both engines. The timed command is
        // the invert/expand/contract evolution only.
        editor.command(Command::SetTool {
            tool: Tool::Marquee,
        })?;
        editor.command(Command::SetMarqueeKind {
            kind: MarqueeKind::Rectangle,
        })?;
        editor.command(Command::SetSelectionMode {
            mode: picsie_core::pixel_selection::PixelSelectionMode::Replace,
        })?;
        editor.command(Command::SetSelectionAntialiased { antialiased: true })?;
        let (rx, ry, rw, rh) = input_rect(&editor.history.document);
        for (phase, x, y) in [
            (Phase::Down, rx as f64, ry as f64),
            (Phase::Move, (rx + rw) as f64, (ry + rh) as f64),
            (Phase::Up, (rx + rw) as f64, (ry + rh) as f64),
        ] {
            editor.command(Command::Pointer {
                samples: vec![PointerSample {
                    phase,
                    point: Point::new(x, y),
                    modifiers: Modifiers::default(),
                }],
            })?;
        }
        ensure!(
            editor.history.info().undo_count == 1,
            "Input rectangle setup did not commit one selection"
        );
    }
    if case == "wand-contiguous" || case == "wand-noncontiguous" {
        // Untimed wand configuration; the timed section is the click only.
        // Point sample, zero per-channel tolerance on premultiplied pixels,
        // selected layer only (matches GIMP sample-merged off).
        editor.command(Command::SetTool { tool: Tool::Wand })?;
        editor.command(Command::SetWand {
            settings: WandSettings {
                tolerance: 0,
                radius: 0,
                contiguous: case == "wand-contiguous",
                sample_all_layers: false,
            },
        })?;
        editor.command(Command::SetSelectionMode {
            mode: picsie_core::pixel_selection::PixelSelectionMode::Replace,
        })?;
        editor.command(Command::SetSelectionAntialiased { antialiased: true })?;
    }
    if case == "group-resize" || case == "group-rotate" {
        // Untimed group targeting: the folder alone is selected so the box
        // carries both members. The timed section is the resize/rotate edit.
        let folder_id = doc
            .layers
            .iter()
            .find(|l| matches!(l.content.as_ref(), Content::Group))
            .unwrap()
            .id
            .clone();
        editor.tool = Tool::Move;
        editor.select(Some(folder_id), SelectionMode::Replace)?;
    }
    if case == "gradient-image-linear" || case == "gradient-mask-linear" {
        // Untimed gradient configuration: linear black-to-white
        // foreground-to-background; the timed section is the drag + commit.
        // The background is set while the paint target is still Content:
        // in Mask target the same command would flip the mask mode instead.
        editor.command(Command::SetTool {
            tool: Tool::Gradient,
        })?;
        editor.command(Command::SetColor {
            color: "#000000".to_owned(),
        })?;
        editor.command(Command::SetPaletteColor {
            color: "#ffffff".to_owned(),
            background: true,
        })?;
        editor.command(Command::SetGradientStyle {
            style: GradientStyle::ForegroundToBackground,
        })?;
        if case == "gradient-mask-linear" {
            editor.command(Command::SetPaintTarget {
                target: PaintTarget::Mask,
            })?;
        }
    }
    if case == "levels-update" {
        // Untimed adjustment creation: adds the layer and opens the live
        // edit transaction. The timed section is one preview update inside
        // that open transaction (no commit in the timer).
        editor.command(Command::AddAdjustment {
            kind: AdjustmentKind::Levels,
        })?;
        ensure!(
            editor.adjustment_edit.is_some(),
            "Levels setup did not open an adjustment edit"
        );
    }
    if case == "curves-update" {
        editor.command(Command::AddAdjustment {
            kind: AdjustmentKind::Curves,
        })?;
        ensure!(
            editor.adjustment_edit.is_some(),
            "Curves setup did not open an adjustment edit"
        );
    }
    Ok(editor)
}

/// Point inside the middle rect on the revealed (left) half.
fn overlap(doc: &Document) -> (u32, u32) {
    (doc.width / 3, doc.height / 2)
}

/// Point inside the middle rect on the masked (right) half.
fn hidden_side(doc: &Document) -> (u32, u32) {
    (doc.width * 3 / 4, doc.height / 2)
}

/// Bottom-only corner, outside the middle rect: second consistency probe.
fn corner(doc: &Document) -> (u32, u32) {
    (doc.width - 10, 10)
}

fn at(pixels: &[u8], doc: &Document, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * doc.width + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().unwrap()
}

fn perform(editor: &mut Editor, case: &str) -> Result<()> {
    let top_id = editor.selected_id().unwrap().to_owned();
    match case {
        "opacity-preview" => {
            editor.command(Command::BeginPropertyEdit {
                label: "Edit layer opacity".into(),
            })?;
            editor.command(Command::UpdateLayer {
                patch: json!({"opacity": OPACITY}),
            })?;
        }
        "opacity-commit" => {
            editor.command(Command::BeginPropertyEdit {
                label: "Edit layer opacity".into(),
            })?;
            editor.command(Command::UpdateLayer {
                patch: json!({"opacity": OPACITY}),
            })?;
            editor.command(Command::FinishGesture)?;
        }
        "visibility-toggle" => {
            editor.command(Command::BeginVisibilitySwipe { id: top_id })?;
            editor.command(Command::SwipeVisibility {
                id: editor.selected_id().unwrap().to_owned(),
            })?;
            editor.command(Command::EndVisibilitySwipe)?;
        }
        "blend-multiply" => {
            editor.command(Command::UpdateLayer {
                patch: json!({"blend": "multiply"}),
            })?;
        }
        "blend-screen" => {
            editor.command(Command::UpdateLayer {
                patch: json!({"blend": "screen"}),
            })?;
        }
        "mask-disabled" => {
            editor.command(Command::UpdateLayer {
                patch: json!({"mask": {"enabled": false}}),
            })?;
        }
        "mask-paint" => {
            // Same 61-point horizontal trajectory shape as the established
            // brush workload; it crosses the mask reveal/hide boundary.
            let doc = &editor.history.document;
            let (width, height) = (doc.width, doc.height);
            let mut samples: Vec<_> = (0..=60)
                .map(|i| PointerSample {
                    phase: if i == 0 { Phase::Down } else { Phase::Move },
                    point: Point::new(
                        width as f64 / 12. + i as f64 * width as f64 / 120.,
                        height as f64 / 2.,
                    ),
                    modifiers: Modifiers::default(),
                })
                .collect();
            samples.push(PointerSample {
                phase: Phase::Up,
                point: samples.last().unwrap().point,
                modifiers: Modifiers::default(),
            });
            editor.command(Command::Pointer { samples })?;
        }
        "select-inverse" => {
            editor.command(Command::InvertSelection)?;
        }
        "select-expand5" => {
            editor.command(Command::ExpandSelection { amount: 5 })?;
        }
        "select-contract5" => {
            editor.command(Command::ContractSelection { amount: 5 })?;
        }
        "wand-contiguous" | "wand-noncontiguous" => {
            // The timed wand click: one Down sample at the left square, as
            // the translated MagicWand fixture drives it.
            let point = wand_point(&editor.history.document);
            editor.command(Command::Pointer {
                samples: vec![PointerSample {
                    phase: Phase::Down,
                    point,
                    modifiers: Modifiers::default(),
                }],
            })?;
        }
        "select-layer-alpha" => {
            editor.command(Command::SelectLayerPixels)?;
        }
        "group-resize" => {
            // One box edit doubling about its center; every member follows
            // its original placement in a single "Transform Layers" undo.
            editor.command(Command::BeginTransform)?;
            editor.command(Command::SetTransformField {
                field: TransformField::ScalePercent,
                value: 200.,
            })?;
            editor.command(Command::CommitTransform)?;
        }
        "group-rotate" => {
            // One box edit rotating 30 degrees about its center, same
            // single-undo member-carrying contract as resize.
            editor.command(Command::BeginTransform)?;
            editor.command(Command::SetTransformField {
                field: TransformField::Rotation,
                value: 30.,
            })?;
            editor.command(Command::CommitTransform)?;
        }
        "gradient-image-linear" | "gradient-mask-linear" => {
            // Full-width linear drag at mid-height, then one commit undo
            // ("Gradient" on the layer, "Gradient Mask" on the mask).
            let doc = &editor.history.document;
            let (width, height) = (doc.width as f64, doc.height as f64);
            for (phase, x) in [
                (Phase::Down, 0.5),
                (Phase::Move, width - 0.5),
                (Phase::Up, width - 0.5),
            ] {
                editor.command(Command::Pointer {
                    samples: vec![PointerSample {
                        phase,
                        point: Point::new(x, height / 2.),
                        modifiers: Modifiers::default(),
                    }],
                })?;
            }
            editor.command(Command::CommitGradient)?;
        }
        "levels-update" => {
            // One live preview inside the open adjustment transaction: input
            // black 64 on the RGB composite range. No commit in the timer.
            let mut settings = LevelsSettings::default();
            settings.ranges[0].black = LEVELS_BLACK;
            editor.command(Command::UpdateAdjustmentLevels {
                settings,
                preview: true,
            })?;
        }
        "curves-update" => {
            // One live preview inside the open adjustment transaction: RGB
            // composite mid lift 128 -> 192. No commit in the timer.
            let mut settings = CurvesSettings::default();
            settings.channels[0] = vec![
                CurvePoint { x: 0., y: 0. },
                CurvePoint {
                    x: CURVES_MID.0,
                    y: CURVES_MID.1,
                },
                CurvePoint { x: 255., y: 255. },
            ];
            editor.command(Command::UpdateAdjustmentCurves {
                settings,
                preview: true,
            })?;
        }
        _ => anyhow::bail!("Unknown workload: {case}"),
    }
    Ok(())
}

fn near(actual: [u8; 4], expected: [u8; 4], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected.iter())
        .all(|(a, e)| a.abs_diff(*e) <= tolerance)
}

/// Changed-pixel count between two same-size RGBA buffers (untimed
/// validation only): every timed workload must produce meaningful
/// nontrivial output, not just move metadata.
fn changed_count(pixels: &[u8], pristine: &[u8]) -> u64 {
    pixels
        .chunks_exact(4)
        .zip(pristine.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count() as u64
}

fn near_f64(actual: f64, expected: f64, epsilon: f64) -> bool {
    (actual - expected).abs() <= epsilon
}

fn displayed_size(layer: &Layer) -> (f64, f64) {
    (
        layer.width as f64 * layer.scale_x,
        layer.height as f64 * layer.scale_y,
    )
}

/// Real mask coverage at document resolution for the mask-gradient case
/// (the selected-coverage proof for masks, separate from the RGBA
/// composite and from pixel-selection coverage).
fn mask_coverage_of(editor: &Editor, layer_name: &str) -> Result<Vec<u8>> {
    let doc = &editor.history.document;
    let layer = doc
        .layers
        .iter()
        .find(|l| l.name == layer_name)
        .ok_or_else(|| anyhow::anyhow!("{layer_name} layer is missing"))?;
    let raster = layer
        .mask
        .as_ref()
        .and_then(|mask| mask.raster.as_ref())
        .ok_or_else(|| anyhow::anyhow!("{layer_name} mask raster is missing"))?;
    ensure!(
        raster.width == doc.width && raster.height == doc.height,
        "Mask coverage has unexpected dimensions"
    );
    Ok(raster.pixels.to_vec())
}

fn validate(
    editor: &Editor,
    case: &str,
    pixels: &[u8],
    coverage: &[u8],
    pristine: &[u8],
    original: &Document,
) -> Result<()> {
    let doc = &editor.history.document;
    ensure!(doc.width == original.width && doc.height == original.height);
    if is_recent_case(case) {
        // Recent cases have no pixel-selection output. The mask gradient's
        // coverage channel is real mask coverage (materialized once per
        // sample in the timed coverage stage), not selection coverage; every
        // other recent case must produce an empty channel.
        if case == "gradient-mask-linear" {
            ensure!(
                coverage.len() == doc.width as usize * doc.height as usize,
                "Mask coverage has unexpected dimensions"
            );
        } else {
            ensure!(
                coverage.is_empty(),
                "Recent case produced selection coverage"
            );
        }
        ensure!(pixels.len() == pristine.len());
        return validate_recent(editor, case, pixels, coverage, pristine, original);
    }
    if is_selection_case(case) {
        // The document pixels are unchanged by a pure selection evolution;
        // that must NOT read as the selection check. Coverage below is the
        // materialized selection output; the RGBA composite only proves the
        // document still renders.
        ensure!(doc.layers.len() == 2);
        ensure!(pixels.len() == doc.width as usize * doc.height as usize * 4);
        ensure!(
            coverage.len() == doc.width as usize * doc.height as usize,
            "Selection coverage has unexpected dimensions"
        );
        return validate_selection(editor, case, coverage);
    }
    // Layer/mask cases have no selection output; the coverage stage is
    // defined as zero and the buffer must stay empty.
    ensure!(coverage.is_empty(), "Non-selection case produced coverage");
    ensure!(doc.layers.len() == 3);
    ensure!(pixels.len() == doc.width as usize * doc.height as usize * 4);
    let top = doc.layers.iter().find(|l| l.name == "Top").unwrap();
    let (ox, oy) = overlap(doc);
    let (cx, cy) = corner(doc);
    match case {
        "opacity-preview" => {
            ensure!(
                editor.history.info().undo_count == 0,
                "Preview must not commit"
            );
            ensure!((top.opacity - OPACITY).abs() < 0.001);
            let pixel = at(pixels, doc, ox, oy);
            ensure!(pixel != TOP, "Preview did not change the composite");
            // 35% top over middle: approximately [111, 111, 152].
            ensure!(
                near(pixel, [111, 111, 152, 255], 3),
                "Unexpected preview composite: {pixel:?}"
            );
            ensure!(
                near(at(pixels, doc, cx, cy), CORNER_OPACITY, 3),
                "Unexpected preview corner: {:?}",
                at(pixels, doc, cx, cy)
            );
        }
        "opacity-commit" => {
            ensure!(editor.history.info().undo_count == 1);
            ensure!((top.opacity - OPACITY).abs() < 0.001);
            let pixel = at(pixels, doc, ox, oy);
            ensure!(
                near(pixel, [111, 111, 152, 255], 3),
                "Unexpected composite: {pixel:?}"
            );
            ensure!(
                near(at(pixels, doc, cx, cy), CORNER_OPACITY, 3),
                "Unexpected corner: {:?}",
                at(pixels, doc, cx, cy)
            );
        }
        "visibility-toggle" => {
            ensure!(editor.history.info().undo_count == 1);
            ensure!(!top.visible);
            let pixel = at(pixels, doc, ox, oy);
            ensure!(pixel == MIDDLE, "Hidden top still contributes: {pixel:?}");
            ensure!(
                at(pixels, doc, cx, cy) == BOTTOM,
                "Hidden top still contributes at corner"
            );
        }
        "blend-multiply" => {
            ensure!(editor.history.info().undo_count == 1);
            ensure!(top.blend == Blend::Multiply);
            let pixel = at(pixels, doc, ox, oy);
            ensure!(pixel != TOP, "Blend did not change the composite");
            ensure!(
                near(pixel, [49, 40, 60, 255], 3),
                "Unexpected multiply: {pixel:?}"
            );
            ensure!(
                near(at(pixels, doc, cx, cy), CORNER_MULTIPLY, 3),
                "Unexpected multiply corner"
            );
        }
        "blend-screen" => {
            ensure!(editor.history.info().undo_count == 1);
            ensure!(top.blend == Blend::Screen);
            let pixel = at(pixels, doc, ox, oy);
            ensure!(pixel != TOP, "Blend did not change the composite");
            ensure!(
                near(pixel, [214, 167, 211, 255], 3),
                "Unexpected screen: {pixel:?}"
            );
            ensure!(
                near(at(pixels, doc, cx, cy), CORNER_SCREEN, 3),
                "Unexpected screen corner"
            );
        }
        "mask-disabled" => {
            ensure!(editor.history.info().undo_count == 1);
            ensure!(!top.mask.as_ref().unwrap().enabled);
            let pixel = at(pixels, doc, hidden_side(doc).0, hidden_side(doc).1);
            ensure!(pixel == TOP, "Disabled mask still hides: {pixel:?}");
            // The revealed left half is unaffected by disabling.
            ensure!(at(pixels, doc, ox, oy) == TOP, "Revealed side changed");
        }
        "mask-paint" => {
            ensure!(
                editor.history.info().undo_count == 1,
                "Paint must commit one edit"
            );
            // Black mask stroke over the revealed left half hides the top there.
            let (x, y) = (doc.width / 4, doc.height / 2);
            let pixel = at(pixels, doc, x, y);
            ensure!(pixel == MIDDLE, "Mask stroke did not hide: {pixel:?}");
            // Far from the stroke the masked side still shows the middle.
            ensure!(
                at(pixels, doc, hidden_side(doc).0, hidden_side(doc).1) == MIDDLE,
                "Stroke reached outside its region"
            );
        }
        _ => anyhow::bail!("Unknown workload: {case}"),
    }
    Ok(())
}

/// Changed interior/exterior coverage probes plus bounds for the six
/// selection evolution workloads. Every case commits exactly one history
/// entry on top of its untimed input setup.
fn validate_selection(editor: &Editor, case: &str, coverage: &[u8]) -> Result<()> {
    let doc = &editor.history.document;
    let selection = editor.history.pixel_selection.as_ref();
    ensure!(
        selection.is_some(),
        "{case}: timed command left no selection"
    );
    let selection = selection.unwrap();
    ensure!(
        selection.width == doc.width && selection.height == doc.height,
        "{case}: selection dimensions differ from the document"
    );
    let (rx, ry, rw, rh) = input_rect(doc);
    let (cx, cy) = (rx + rw / 2, ry + rh / 2);
    let (ex, ey) = (10, 10);
    match case {
        "select-inverse" => {
            // One setup marquee plus one invert.
            ensure!(editor.history.info().undo_count == 2);
            let bounds = selection.bounds.as_ref().unwrap();
            ensure!(
                bounds.x == 0 && bounds.y == 0,
                "{case}: inverted bounds origin moved: {bounds:?}"
            );
            ensure!(
                bounds.width == doc.width && bounds.height == doc.height,
                "{case}: inverted selection does not span the canvas: {bounds:?}"
            );
            ensure!(
                coverage_at(coverage, doc, cx, cy) == 0,
                "{case}: former interior still selected"
            );
            ensure!(
                coverage_at(coverage, doc, ex, ey) == 255,
                "{case}: former exterior not selected"
            );
        }
        "select-expand5" => {
            ensure!(editor.history.info().undo_count == 2);
            let bounds = selection.bounds.as_ref().unwrap();
            ensure!(
                (bounds.x, bounds.y, bounds.width, bounds.height)
                    == (rx - 5, ry - 5, rw + 10, rh + 10),
                "{case}: expanded bounds diverged: {bounds:?}"
            );
            ensure!(
                coverage_at(coverage, doc, cx, cy) == 255,
                "{case}: interior lost"
            );
            ensure!(
                coverage_at(coverage, doc, rx - 3, ry + rh / 2) == 255,
                "{case}: grown band not selected"
            );
            ensure!(
                coverage_at(coverage, doc, ex, ey) == 0,
                "{case}: exterior reached"
            );
        }
        "select-contract5" => {
            ensure!(editor.history.info().undo_count == 2);
            let bounds = selection.bounds.as_ref().unwrap();
            ensure!(
                (bounds.x, bounds.y, bounds.width, bounds.height)
                    == (rx + 5, ry + 5, rw - 10, rh - 10),
                "{case}: contracted bounds diverged: {bounds:?}"
            );
            ensure!(
                coverage_at(coverage, doc, cx, cy) == 255,
                "{case}: center lost"
            );
            ensure!(
                coverage_at(coverage, doc, rx + 2, ry + rh / 2) == 0,
                "{case}: trimmed edge still selected"
            );
            ensure!(
                coverage_at(coverage, doc, ex, ey) == 0,
                "{case}: exterior reached"
            );
        }
        "wand-contiguous" => {
            ensure!(editor.history.info().undo_count == 1);
            let side = doc.width / 8;
            ensure!(
                selected_count(coverage) == u64::from(side * side),
                "{case}: contiguous wand covered {}, expected one square",
                selected_count(coverage)
            );
            let point = wand_point(doc);
            ensure!(
                coverage_at(coverage, doc, point.x as u32, point.y as u32) == 255,
                "{case}: clicked square not selected"
            );
            let (qx, qy) = right_square_center(doc);
            ensure!(
                coverage_at(coverage, doc, qx, qy) == 0,
                "{case}: disconnected square leaked into contiguous match"
            );
            ensure!(
                coverage_at(coverage, doc, doc.width / 2, doc.height / 2) == 0,
                "{case}: transparent gap selected"
            );
        }
        "wand-noncontiguous" => {
            ensure!(editor.history.info().undo_count == 1);
            let side = doc.width / 8;
            ensure!(
                selected_count(coverage) == 2 * u64::from(side * side),
                "{case}: full-canvas match covered {}, expected both squares",
                selected_count(coverage)
            );
            let point = wand_point(doc);
            ensure!(
                coverage_at(coverage, doc, point.x as u32, point.y as u32) == 255,
                "{case}: clicked square not selected"
            );
            let (qx, qy) = right_square_center(doc);
            ensure!(
                coverage_at(coverage, doc, qx, qy) == 255,
                "{case}: disconnected square missing from full-canvas match"
            );
            ensure!(
                coverage_at(coverage, doc, doc.width / 2, doc.height / 2) == 0,
                "{case}: transparent gap selected"
            );
        }
        "select-layer-alpha" => {
            ensure!(editor.history.info().undo_count == 1);
            let side = doc.width / 8;
            ensure!(
                selected_count(coverage) == 2 * u64::from(side * side),
                "{case}: layer silhouette covered {}, expected both squares",
                selected_count(coverage)
            );
            let point = wand_point(doc);
            ensure!(
                coverage_at(coverage, doc, point.x as u32, point.y as u32) == 255,
                "{case}: left square silhouette missing"
            );
            let (qx, qy) = right_square_center(doc);
            ensure!(
                coverage_at(coverage, doc, qx, qy) == 255,
                "{case}: right square silhouette missing"
            );
            ensure!(
                coverage_at(coverage, doc, doc.width / 2, doc.height / 2) == 0,
                "{case}: transparent gap selected"
            );
        }
        _ => anyhow::bail!("Unknown workload: {case}"),
    }
    Ok(())
}

/// Changed-output plus geometry/history assertions for the six
/// recent-features workloads. `pristine` is the untouched fixture render
/// (one untimed render per case/size, shared across samples); every case
/// must change a nontrivial fraction of it. Mask coverage for
/// `gradient-mask-linear` is validated separately from its own channel.
fn validate_recent(
    editor: &Editor,
    case: &str,
    pixels: &[u8],
    maskcov: &[u8],
    pristine: &[u8],
    original: &Document,
) -> Result<()> {
    let doc = &editor.history.document;
    ensure!(pixels.len() == doc.width as usize * doc.height as usize * 4);
    let changed = changed_count(pixels, pristine);
    let total = doc.width as u64 * doc.height as u64;
    match case {
        "group-resize" => {
            ensure!(doc.layers.len() == 4, "Group members were lost");
            ensure!(editor.history.info().undo_count == 1);
            ensure!(editor.history.info().undo_label == "Transform Layers");
            let folder = doc.layers.iter().find(|l| l.name == "Group").unwrap();
            for name in ["A", "B"] {
                let member = doc.layers.iter().find(|l| l.name == name).unwrap();
                ensure!(
                    member.parent_id.as_deref() == Some(folder.id.as_str()),
                    "{case}: {name} left its folder"
                );
                let before = original.layers.iter().find(|l| l.name == name).unwrap();
                let (w, h) = displayed_size(member);
                let (bw, bh) = displayed_size(before);
                ensure!(
                    near_f64(w, 2. * bw, 1.) && near_f64(h, 2. * bh, 1.),
                    "{case}: {name} did not double: {w}x{h} from {bw}x{bh}"
                );
            }
            // (w/24, h/8) sits left of A's original left edge but inside the
            // doubled box-relative footprint: base before, red after.
            ensure!(
                at(pixels, doc, doc.width / 24, doc.height / 8) == TOP,
                "{case}: doubled member did not cover the probe"
            );
            // (23w/24, h/2) sits right of B's original right edge but inside
            // the doubled box-relative footprint: base before, blue after.
            ensure!(
                at(pixels, doc, doc.width * 23 / 24, doc.height / 2) == MIDDLE,
                "{case}: doubled member did not cover the probe"
            );
            ensure!(
                changed * 1000 >= total,
                "{case}: no nontrivial changed output ({changed}/{total})"
            );
        }
        "group-rotate" => {
            ensure!(doc.layers.len() == 4, "Group members were lost");
            ensure!(editor.history.info().undo_count == 1);
            ensure!(editor.history.info().undo_label == "Transform Layers");
            let folder = doc.layers.iter().find(|l| l.name == "Group").unwrap();
            for name in ["A", "B"] {
                let member = doc.layers.iter().find(|l| l.name == name).unwrap();
                ensure!(
                    member.parent_id.as_deref() == Some(folder.id.as_str()),
                    "{case}: {name} left its folder"
                );
                ensure!(
                    near_f64(member.rotation, 30., 0.5),
                    "{case}: {name} rotation is {}",
                    member.rotation
                );
            }
            ensure!(
                changed * 1000 >= total,
                "{case}: no nontrivial changed output ({changed}/{total})"
            );
        }
        "gradient-image-linear" => {
            ensure!(doc.layers.len() == 1);
            ensure!(editor.history.info().undo_count == 1);
            ensure!(editor.history.info().undo_label == "Gradient");
            let left = at(pixels, doc, 1, doc.height / 2);
            let right = at(pixels, doc, doc.width - 2, doc.height / 2);
            let middle = at(pixels, doc, doc.width / 2, doc.height / 2);
            ensure!(near(left, [0, 0, 0, 255], 4), "{case}: left {left:?}");
            ensure!(
                near(right, [255, 255, 255, 255], 4),
                "{case}: right {right:?}"
            );
            ensure!(
                near(middle, [128, 128, 128, 255], 6)
                    && middle[0] == middle[1]
                    && middle[1] == middle[2],
                "{case}: middle {middle:?}"
            );
            ensure!(
                changed * 10 >= total,
                "{case}: no nontrivial changed output ({changed}/{total})"
            );
        }
        "gradient-mask-linear" => {
            ensure!(doc.layers.len() == 2);
            ensure!(editor.history.info().undo_count == 1);
            ensure!(editor.history.info().undo_label == "Gradient Mask");
            // The timed materialization above, validated here (never
            // re-read inside a timer). Picsie's encoded-space ramp is
            // deterministic at both sizes: edges 0/255, 5% -> 13,
            // middle 128, 95% -> 242. GIMP's linear-light mask ramp
            // differs by engine design (0/1/55/227/255); each side is
            // checked against its own deterministic ramp and the paired
            // comparison quantifies the gap.
            let side = |x: u32, y: u32| maskcov[(y * doc.width + x) as usize];
            ensure!(
                side(1, doc.height / 2) <= 1,
                "{case}: mask start not covered"
            );
            ensure!(
                side(doc.width - 2, doc.height / 2) >= 254,
                "{case}: mask end not revealed"
            );
            let (lx, mx, rx) = (
                side(doc.width * 5 / 100, doc.height / 2),
                side(doc.width / 2, doc.height / 2),
                side(doc.width * 95 / 100, doc.height / 2),
            );
            ensure!(
                lx.abs_diff(13) <= 3,
                "{case}: mask 5% off the encoded ramp: {lx}"
            );
            ensure!(mx.abs_diff(128) <= 2, "{case}: mask middle off ramp: {mx}");
            ensure!(
                rx.abs_diff(242) <= 3,
                "{case}: mask 95% off the encoded ramp: {rx}"
            );
            // Composite follows the coverage: base on the left, red right.
            // Windows match the GIMP driver exactly.
            ensure!(
                near(
                    at(pixels, doc, doc.width * 5 / 100, doc.height / 2),
                    BOTTOM,
                    8
                ),
                "{case}: composite left did not reveal the base"
            );
            ensure!(
                near(
                    at(pixels, doc, doc.width * 95 / 100, doc.height / 2),
                    TOP,
                    16
                ),
                "{case}: composite right did not reveal the top"
            );
            ensure!(
                changed * 100 >= total,
                "{case}: no nontrivial changed output ({changed}/{total})"
            );
        }
        "levels-update" => {
            // One committed AddAdjustment plus the open live edit: the timed
            // update previews without committing a second undo.
            ensure!(doc.layers.len() == 2);
            ensure!(editor.history.info().undo_count == 1);
            let edit = editor.adjustment_edit.as_ref().ok_or_else(|| {
                anyhow::anyhow!("{case}: timed update left no open adjustment edit")
            })?;
            ensure!(
                (edit.working.levels.ranges[0].black - LEVELS_BLACK).abs() < 0.001,
                "{case}: levels input black did not apply"
            );
            // Gray 128 through input black 64: (128-64)/191*255 ~= 85.
            let gray = at(pixels, doc, doc.width / 2, doc.height / 2);
            ensure!(near(gray, [85, 85, 85, 255], 3), "{case}: gray {gray:?}");
            let white = at(pixels, doc, doc.width / 8 + doc.width / 16, doc.height / 2);
            ensure!(
                near(white, [255, 255, 255, 255], 1),
                "{case}: white patch {white:?}"
            );
            ensure!(
                changed * 10 >= total,
                "{case}: no nontrivial changed output ({changed}/{total})"
            );
        }
        "curves-update" => {
            ensure!(doc.layers.len() == 2);
            ensure!(editor.history.info().undo_count == 1);
            let edit = editor.adjustment_edit.as_ref().ok_or_else(|| {
                anyhow::anyhow!("{case}: timed update left no open adjustment edit")
            })?;
            ensure!(
                edit.working.curves.channels[0].len() == 3
                    && edit.working.curves.channels[0][1].x == CURVES_MID.0
                    && edit.working.curves.channels[0][1].y == CURVES_MID.1,
                "{case}: curves mid point did not apply"
            );
            // Gray 128 sits exactly on the lifted knot: 192.
            let gray = at(pixels, doc, doc.width / 2, doc.height / 2);
            ensure!(near(gray, [192, 192, 192, 255], 2), "{case}: gray {gray:?}");
            let white = at(pixels, doc, doc.width / 8 + doc.width / 16, doc.height / 2);
            ensure!(
                near(white, [255, 255, 255, 255], 1),
                "{case}: white patch {white:?}"
            );
            ensure!(
                changed * 10 >= total,
                "{case}: no nontrivial changed output ({changed}/{total})"
            );
        }
        _ => anyhow::bail!("Unknown recent workload: {case}"),
    }
    Ok(())
}
/// Untimed restoration controls, run once per case after the timed loop:
/// Undo must restore the pre-workload state, and cancelling an opacity
/// preview must leave no undo behind. Hard failure here fails the run.
fn restoration_checks(original: &Document, case: &str, width: u32) -> Result<()> {
    if is_selection_case(case) {
        return restoration_checks_selection(original, case, width);
    }
    if is_recent_case(case) {
        return restoration_checks_recent(original, case, width);
    }
    let mut editor = setup(original, case)?;
    perform(&mut editor, case)?;
    if case == "opacity-preview" {
        // Cancellation control: abandoning the drag restores full opacity.
        editor.command(Command::CancelGesture)?;
        let top = editor
            .history
            .document
            .layers
            .iter()
            .find(|l| l.name == "Top")
            .unwrap();
        ensure!(
            (top.opacity - 1.).abs() < 0.001,
            "{width} {case}: cancel did not restore opacity"
        );
        ensure!(
            editor.history.info().undo_count == 0,
            "{width} {case}: cancel left an undo entry"
        );
        return Ok(());
    }
    editor.command(Command::Undo)?;
    ensure!(
        editor.history.info().undo_count == 0,
        "{width} {case}: undo did not consume the edit"
    );
    let top = editor
        .history
        .document
        .layers
        .iter()
        .find(|l| l.name == "Top")
        .unwrap()
        .clone();
    match case {
        "opacity-commit" => ensure!((top.opacity - 1.).abs() < 0.001),
        "visibility-toggle" => ensure!(top.visible),
        "blend-multiply" | "blend-screen" => ensure!(top.blend == Blend::SourceOver),
        "mask-disabled" => ensure!(top.mask.as_ref().unwrap().enabled),
        "mask-paint" => ensure!(top.mask.as_ref().unwrap().strokes.is_empty()),
        _ => anyhow::bail!("Unknown workload: {case}"),
    }
    // Restored pixels must match the untouched fixture render.
    let restored = render::rgba_pixels(
        &Renderer::default()
            .render(&editor.history.document)?
            .image_snapshot(),
    )?;
    let pristine = render::rgba_pixels(&Renderer::default().render(original)?.image_snapshot())?;
    ensure!(
        restored == pristine,
        "{width} {case}: undo did not restore pixels"
    );
    Ok(())
}

/// Untimed selection restoration control: actual history Undo must restore
/// the exact pre-command coverage (the input rectangle for invert/expand/
/// contract, empty for wand/layer-alpha) and the untouched pixels.
fn restoration_checks_selection(original: &Document, case: &str, width: u32) -> Result<()> {
    let mut editor = setup(original, case)?;
    let baseline_undos = editor.history.info().undo_count;
    let before = coverage_of(&editor)?;
    perform(&mut editor, case)?;
    ensure!(
        editor.history.info().undo_count == baseline_undos + 1,
        "{width} {case}: timed command did not commit one undo"
    );
    editor.command(Command::Undo)?;
    ensure!(
        editor.history.info().undo_count == baseline_undos,
        "{width} {case}: undo did not consume the selection edit"
    );
    let restored = coverage_of(&editor)?;
    ensure!(
        restored == before,
        "{width} {case}: undo did not restore selection coverage"
    );
    let restored_pixels = render::rgba_pixels(
        &Renderer::default()
            .render(&editor.history.document)?
            .image_snapshot(),
    )?;
    let pristine = render::rgba_pixels(&Renderer::default().render(original)?.image_snapshot())?;
    ensure!(
        restored_pixels == pristine,
        "{width} {case}: undo did not restore pixels"
    );
    Ok(())
}

/// Untimed recent-features restoration control: actual history Undo (or the
/// adjustment Cancel path, which preserves the property-transaction
/// boundary) must restore the pre-workload state byte-for-byte, including
/// group hierarchy/member geometry and mask coverage.
fn restoration_checks_recent(original: &Document, case: &str, width: u32) -> Result<()> {
    let mut editor = setup(original, case)?;
    if case == "levels-update" || case == "curves-update" {
        // Identity reference with the adjustment layer present: both sides
        // travel the same lookup-table path, so this is exact by
        // construction and also proves the cancel kept the layer.
        let identity = render::rgba_pixels(
            &Renderer::default()
                .render(&editor.history.document)?
                .image_snapshot(),
        )?;
        perform(&mut editor, case)?;
        ensure!(
            editor.adjustment_edit.is_some(),
            "{width} {case}: timed update closed the adjustment edit"
        );
        editor.command(Command::CancelAdjustmentEdit)?;
        ensure!(
            editor.adjustment_edit.is_none(),
            "{width} {case}: cancel left the adjustment edit open"
        );
        let layer = editor
            .history
            .document
            .layers
            .iter()
            .find(|l| l.adjustment.is_some())
            .ok_or_else(|| anyhow::anyhow!("{width} {case}: cancel removed the layer"))?;
        ensure!(
            layer.adjustment.as_ref().unwrap().is_identity(),
            "{width} {case}: cancel did not restore identity settings"
        );
        ensure!(
            editor.history.info().undo_count == 1,
            "{width} {case}: cancel consumed the AddAdjustment undo"
        );
        let restored = render::rgba_pixels(
            &Renderer::default()
                .render(&editor.history.document)?
                .image_snapshot(),
        )?;
        ensure!(
            restored == identity,
            "{width} {case}: cancel did not restore pixels"
        );
        return Ok(());
    }
    perform(&mut editor, case)?;
    editor.command(Command::Undo)?;
    ensure!(
        editor.history.info().undo_count == 0,
        "{width} {case}: undo did not consume the edit"
    );
    if case == "group-resize" || case == "group-rotate" {
        // Hierarchy and member geometry must come back exactly.
        for name in ["Group", "A", "B"] {
            let member = editor
                .history
                .document
                .layers
                .iter()
                .find(|l| l.name == name)
                .ok_or_else(|| anyhow::anyhow!("{width} {case}: {name} missing after undo"))?;
            let before = original.layers.iter().find(|l| l.name == name).unwrap();
            ensure!(
                member.x == before.x
                    && member.y == before.y
                    && member.scale_x == before.scale_x
                    && member.scale_y == before.scale_y
                    && member.rotation == before.rotation
                    && member.parent_id == before.parent_id,
                "{width} {case}: {name} geometry differs after undo"
            );
        }
    }
    if case == "gradient-mask-linear" {
        // Real mask coverage must come back: solid white again.
        let coverage = mask_coverage_of(&editor, "Top")?;
        ensure!(
            coverage.iter().all(|v| *v == 255),
            "{width} {case}: undo did not restore mask coverage"
        );
    }
    let restored = render::rgba_pixels(
        &Renderer::default()
            .render(&editor.history.document)?
            .image_snapshot(),
    )?;
    let pristine = render::rgba_pixels(&Renderer::default().render(original)?.image_snapshot())?;
    ensure!(
        restored == pristine,
        "{width} {case}: undo did not restore pixels"
    );
    Ok(())
}

/// Test-only single-channel selection-coverage comparator with bounded
/// memory: streams both raw grayscale dumps and reports coverage error plus
/// each side's selected-pixel count. Benchmark verification, not a
/// production renderer.
fn compare_command(a: &Path, b: &Path, width: u32, height: u32) -> Result<()> {
    let row_bytes = width as usize * 4;
    let expected = row_bytes * height as usize;
    let mut fa = fs::File::open(a)?;
    let mut fb = fs::File::open(b)?;
    ensure!(
        fa.metadata()?.len() as usize == expected,
        "Left probe has unexpected size"
    );
    ensure!(
        fb.metadata()?.len() as usize == expected,
        "Right probe has unexpected size"
    );
    // 256-row blocks bound extra memory well below one full frame.
    let block_rows = 256usize;
    let mut block_a = vec![0u8; row_bytes * block_rows];
    let mut block_b = vec![0u8; row_bytes * block_rows];
    let mut remaining = height as usize;
    let (mut different, mut max_difference, mut max_alpha_difference) = (0u64, 0u8, 0u8);
    let (mut visible_different, mut max_visible_difference) = (0u64, 0u8);
    while remaining > 0 {
        let rows = remaining.min(block_rows);
        let len = rows * row_bytes;
        fa.read_exact(&mut block_a[..len])?;
        fb.read_exact(&mut block_b[..len])?;
        for offset in (0..len).step_by(4) {
            let (pa, pb) = (&block_a[offset..offset + 4], &block_b[offset..offset + 4]);
            let mut worst = pa[3].abs_diff(pb[3]);
            max_alpha_difference = max_alpha_difference.max(worst);
            if pa[3] != 0 || pb[3] != 0 {
                for c in 0..3 {
                    worst = worst.max(pa[c].abs_diff(pb[c]));
                }
            }
            if worst > 0 {
                different += 1;
                max_difference = max_difference.max(worst);
            }
            let mut visible_worst = pa[3].abs_diff(pb[3]);
            for c in 0..3 {
                let va = (pa[c] as u32 * pa[3] as u32 + 127) / 255;
                let vb = (pb[c] as u32 * pb[3] as u32 + 127) / 255;
                visible_worst = visible_worst.max(va.abs_diff(vb) as u8);
            }
            if visible_worst > 0 {
                visible_different += 1;
                max_visible_difference = max_visible_difference.max(visible_worst);
            }
        }
        remaining -= rows;
    }
    println!(
        "{}",
        json!({"differing_pixels": different, "max_channel_difference": max_difference,
               "max_alpha_difference": max_alpha_difference,
               "differing_premultiplied_pixels": visible_different,
               "max_premultiplied_channel_difference": max_visible_difference})
    );
    Ok(())
}

fn compare_selection_command(a: &Path, b: &Path, width: u32, height: u32) -> Result<()> {
    let row_bytes = width as usize;
    let expected = row_bytes * height as usize;
    let mut fa = fs::File::open(a)?;
    let mut fb = fs::File::open(b)?;
    ensure!(
        fa.metadata()?.len() as usize == expected,
        "Left coverage has unexpected size"
    );
    ensure!(
        fb.metadata()?.len() as usize == expected,
        "Right coverage has unexpected size"
    );
    // 256-row blocks bound extra memory well below one full frame.
    let block_rows = 256usize;
    let mut block_a = vec![0u8; row_bytes * block_rows];
    let mut block_b = vec![0u8; row_bytes * block_rows];
    let mut remaining = height as usize;
    let (mut different, mut max_difference) = (0u64, 0u8);
    let (mut selected_a, mut selected_b) = (0u64, 0u64);
    while remaining > 0 {
        let rows = remaining.min(block_rows);
        let len = rows * row_bytes;
        fa.read_exact(&mut block_a[..len])?;
        fb.read_exact(&mut block_b[..len])?;
        for i in 0..len {
            let (va, vb) = (block_a[i], block_b[i]);
            if va > 0 {
                selected_a += 1;
            }
            if vb > 0 {
                selected_b += 1;
            }
            let gap = va.abs_diff(vb);
            if gap > 0 {
                different += 1;
                max_difference = max_difference.max(gap);
            }
        }
        remaining -= rows;
    }
    println!(
        "{}",
        json!({"differing_pixels": different, "max_coverage_difference": max_difference,
               "picsie_selected_pixels": selected_a, "gimp_selected_pixels": selected_b})
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() >= 3,
        "Usage: performance_features prepare FIXTURES | run OUTPUT WARMUPS SAMPLES [CASE ...] | compare A.rgba B.rgba WIDTH HEIGHT | compare-sel A.selcov B.selcov WIDTH HEIGHT"
    );
    let folder = Path::new(&args[2]);
    if args[1] == "prepare" {
        return prepare(folder);
    }
    if args[1] == "compare" {
        ensure!(
            args.len() == 6,
            "Usage: performance_features compare A.rgba B.rgba WIDTH HEIGHT"
        );
        let width: u32 = args[4].parse()?;
        let height: u32 = args[5].parse()?;
        return compare_command(Path::new(&args[2]), Path::new(&args[3]), width, height);
    }
    if args[1] == "compare-sel" {
        ensure!(
            args.len() == 6,
            "Usage: performance_features compare-sel A.selcov B.selcov WIDTH HEIGHT"
        );
        let width: u32 = args[4].parse()?;
        let height: u32 = args[5].parse()?;
        return compare_selection_command(Path::new(&args[2]), Path::new(&args[3]), width, height);
    }
    ensure!(args[1] == "run");
    // One or more exact case filters; unknown names fail before any timing.
    // The shared dispatch still accepts the older layer/mask and selection
    // workloads alongside the recent-features cases.
    let wanted: Vec<&str> = args.iter().skip(5).map(String::as_str).collect();
    for name in &wanted {
        ensure!(
            CASES.contains(name) || SELECTION_CASES.contains(name) || RECENT_CASES.contains(name),
            "Unknown workload: {name}"
        );
    }
    fs::create_dir_all(folder)?;
    let warmups: usize = args.get(3).map_or(Ok(2), |v| v.parse())?;
    let samples: usize = args.get(4).map_or(Ok(4), |v| v.parse())?;
    ensure!(samples > 0);
    let mut results = vec![];
    for (width, height) in SIZES {
        for case in CASES
            .iter()
            .chain(SELECTION_CASES.iter())
            .chain(RECENT_CASES.iter())
        {
            if !wanted.is_empty() && !wanted.contains(case) {
                continue;
            }
            let original = if is_selection_case(case) {
                selection_fixture(width, height)?
            } else if is_recent_case(case) {
                recent_fixture(case, width, height)?
            } else {
                fixture(width, height, masked(case))?
            };
            // Untimed pristine reference for the changed-output checks (one
            // render per case/size, shared across samples).
            let pristine =
                render::rgba_pixels(&Renderer::default().render(&original)?.image_snapshot())?;
            let mut rows = vec![];
            for i in 0..warmups + samples {
                // Untimed fixture preparation; each sample starts from a fresh
                // editor sharing the immutable original pixels.
                let mut editor = setup(&original, case)?;
                // A fresh cold Renderer per sample; caches are never reused
                // across samples, unlike a warm retained-preview interaction.
                let mut renderer = Renderer::default();
                let started = Instant::now();
                perform(&mut editor, case)?;
                let command_ms = started.elapsed().as_secs_f64() * 1000.;
                // Materialized selection coverage availability: the timed
                // command alone does not prove the coverage was realized.
                // Layer/mask cases have no selection output; their coverage
                // stage is defined as zero to preserve the shared schema.
                let coverage = if is_selection_case(case) {
                    let materialized = coverage_of(&editor)?;
                    let coverage_ms = started.elapsed().as_secs_f64() * 1000. - command_ms;
                    (materialized, coverage_ms)
                } else if *case == "gradient-mask-linear" {
                    // Real mask coverage availability, materialized once per
                    // sample in the coverage stage (mirroring the selection
                    // boundary): the same channel is validated and dumped
                    // below, never re-read inside a timer.
                    let materialized = mask_coverage_of(&editor, "Top")?;
                    let coverage_ms = started.elapsed().as_secs_f64() * 1000. - command_ms;
                    (materialized, coverage_ms)
                } else {
                    (Vec::new(), 0.)
                };
                let output = renderer.render(&editor.history.document)?.image_snapshot();
                let render_ms = started.elapsed().as_secs_f64() * 1000. - command_ms - coverage.1;
                let pixels = render::rgba_pixels(&output)?;
                let read_ms =
                    started.elapsed().as_secs_f64() * 1000. - command_ms - coverage.1 - render_ms;
                let total_ms = started.elapsed().as_secs_f64() * 1000.;
                // Getter validation and pixel assertions stay outside the timer.
                validate(&editor, case, &pixels, &coverage.0, &pristine, &original)?;
                // Only the first measured sample per case/size/trial is
                // dumped; coverage itself is materialized and validated on
                // every sample, and the paired comparison uses trial-1 dumps.
                // The mask gradient dumps its real mask coverage channel
                // (the timed materialization above) for the paired `.maskcov`
                // comparison.
                if i == warmups {
                    fs::write(folder.join(format!("{width}-{case}.rgba")), &pixels)?;
                    if is_selection_case(case) {
                        fs::write(folder.join(format!("{width}-{case}.selcov")), &coverage.0)?;
                    }
                    if *case == "gradient-mask-linear" {
                        fs::write(folder.join(format!("{width}-{case}.maskcov")), &coverage.0)?;
                    }
                }
                black_box(&pixels);
                black_box(&coverage.0);
                if i >= warmups {
                    rows.push(json!({"command_ms":command_ms,"coverage_ms":coverage.1,
                                     "render_ms":render_ms,
                                     "read_ms":read_ms,"total_ms":total_ms}));
                }
            }
            // Untimed restoration controls; failures fail the run.
            restoration_checks(&original, case, width)?;
            results.push(json!({"width":width,"height":height,"case":case,"samples":rows}));
            eprintln!("Completed {width} {case}");
        }
    }
    println!(
        "{}",
        json!({"app":"picsie","scope":"Existing editor command with history (command_ms), plus cold fresh-Renderer composite (render_ms) and complete document RGBA read (read_ms); total_ms is the end-to-end CPU availability boundary. Selection cases additionally time materialized selection-coverage availability every sample (coverage_ms) and dump the first measured sample's .selcov channel per case/size/trial (paired comparison uses the trial-1 dumps); layer/mask cases report coverage_ms 0 and have no selection output. Recent group cases carry every folder member from its original placement in one Transform Layers undo; gradients commit one Gradient/Gradient Mask undo; Levels/Curves time one live preview inside the open adjustment transaction (no commit in the timer) and report coverage_ms 0; the mask gradient additionally materializes its real mask coverage once per sample in coverage_ms (validated and dumped from that same channel, paired comparison uses the trial-1 `.maskcov` dumps). Excludes UI/GPU/file encoding and any warm retained preview. Fresh editor sharing immutable original pixels each sample; preview case holds an open property gesture; setup, getter validation, assertions and untimed undo/cancel restoration checks outside timers.","results":results})
    );
    Ok(())
}
