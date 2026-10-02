//! Selected Compositor fixtures, MIT © 2026 Wonder Assembly LLC, pinned 609dbeae.
//! Upstream fixtures and local regressions are named separately. See docs/compositor-port.md.
use picsie_core::{
    brush::{BrushStroke, Settings},
    editor::*,
    files::*,
    geometry::*,
    model::*,
    render::*,
};
use std::sync::Arc;
fn session(w: u32, h: u32, layers: Vec<Layer>) -> Editor {
    let mut d = Document::new("Fixture", w, h).unwrap();
    d.version = 2;
    d.layers = layers;
    let mut e = Editor::new(d).unwrap();
    e.viewport = Viewport {
        width: w as f64,
        height: h as f64,
        zoom: 1.,
        pan: Point::default(),
    };
    e
}
fn cmd(e: &mut Editor, c: Command) {
    e.command(c).unwrap();
}
fn pointer(e: &mut Editor, phase: Phase, x: f64, y: f64) {
    e.pointer(PointerSample {
        phase,
        point: Point::new(x, y),
        modifiers: Modifiers::default(),
    })
    .unwrap();
}
fn square(e: &mut Editor, x: f64, y: f64, size: f64) {
    cmd(
        e,
        Command::SetTool {
            tool: Tool::Marquee,
        },
    );
    pointer(e, Phase::Down, x, y);
    pointer(e, Phase::Up, x + size, y + size);
}
fn shape(w: u32, h: u32, color: &str) -> Layer {
    Layer::new(
        "Color",
        w,
        h,
        Content::shape(Shape::Rectangle, color.into()),
    )
}
fn mask(values: &[u8], w: u32, h: u32) -> Arc<LayerMask> {
    Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        raster: Some(Arc::new(MaskRaster {
            width: w,
            height: h,
            pixels: Arc::new(values.to_vec()),
        })),
        linked: true,
        placement: None,
        strokes: vec![],
    })
}
fn pixels(d: &Document) -> Vec<u8> {
    rgba_pixels(&Renderer::default().render(d).unwrap().image_snapshot()).unwrap()
}
fn alphas(d: &Document) -> Vec<u8> {
    pixels(d).chunks_exact(4).map(|p| p[3]).collect()
}
fn probe(d: &Document, x: u32, y: u32) -> [u8; 4] {
    let data = pixels(d);
    data[((y * d.width + x) * 4) as usize..((y * d.width + x) * 4 + 4) as usize]
        .try_into()
        .unwrap()
}
fn brush(w: u32, h: u32, diameter: f64, hardness: f64, opacity: f64) -> BrushStroke {
    let d = Document::new("Brush", w, h).unwrap();
    BrushStroke::new(
        Layer::new("Paint", w, h, Content::Paint),
        &d,
        None,
        false,
        Settings {
            diameter,
            hardness,
            opacity,
            smoothing: 0.,
            erase: false,
            color: [255, 0, 0],
        },
        1.,
    )
    .unwrap()
}
fn brush_doc(stroke: &BrushStroke, w: u32, h: u32) -> Document {
    let mut d = Document::new("Brush", w, h).unwrap();
    d.layers.push(stroke.snapshot().unwrap());
    d
}

/// SelectionTests.expandAndContractGrowAndShrinkTheOutline and expandStaysOnCanvasAndContractCanEmptyTheSelection.
#[test]
fn compositor_expand_contract_edges_and_history() {
    let mut e = session(100, 100, vec![]);
    square(&mut e, 40., 40., 20.);
    cmd(&mut e, Command::ExpandSelection { amount: 5 });
    let s = e.history.pixel_selection.as_ref().unwrap();
    let b = s.bounds.as_ref().unwrap();
    assert_eq!((b.x, b.width), (35, 30));
    assert_eq!((s.at(37, 50), s.at(33, 50)), (255, 0));
    cmd(&mut e, Command::ContractSelection { amount: 8 });
    let b = e
        .history
        .pixel_selection
        .as_ref()
        .unwrap()
        .bounds
        .as_ref()
        .unwrap();
    assert_eq!((b.x, b.width), (43, 14));
    cmd(&mut e, Command::Undo);
    assert_eq!(
        e.history
            .pixel_selection
            .as_ref()
            .unwrap()
            .bounds
            .as_ref()
            .unwrap()
            .width,
        30
    );
    cmd(&mut e, Command::SelectAllPixels);
    cmd(&mut e, Command::ExpandSelection { amount: 10 });
    assert_eq!(
        e.history
            .pixel_selection
            .as_ref()
            .unwrap()
            .bounds
            .as_ref()
            .unwrap()
            .width,
        100
    );
    cmd(&mut e, Command::ContractSelection { amount: 10 });
    let s = e.history.pixel_selection.as_ref().unwrap();
    assert_eq!((s.at(5, 50), s.at(50, 50)), (0, 255));
    cmd(&mut e, Command::ContractSelection { amount: 45 });
    assert!(e.history.pixel_selection.as_ref().unwrap().bounds.is_none());
    cmd(&mut e, Command::InvertSelection);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(5, 50), 255);
}
/// SelectionFeatherTests.featherSoftensTheSelectionAndWhatItClips, extended with invert/resize preservation.
#[test]
fn compositor_feathered_fill_and_outline_operations() {
    let mut e = session(
        100,
        100,
        vec![Layer::new("Paint", 100, 100, Content::Paint)],
    );
    square(&mut e, 25., 25., 50.);
    cmd(&mut e, Command::FeatherSelection { amount: 5 });
    cmd(
        &mut e,
        Command::SetColor {
            color: "#ff0000".into(),
        },
    );
    cmd(&mut e, Command::FillSelection);
    let values = alphas(&e.history.document);
    let row = &values[50 * 100..51 * 100];
    assert!(row.iter().filter(|&&v| v > 0 && v < 255).count() >= 4);
    assert_eq!(row[50], 255);
    assert_eq!(row[0], 0);
    cmd(&mut e, Command::Undo);
    assert!(alphas(&e.history.document).iter().all(|&a| a == 0));
    cmd(&mut e, Command::Redo);
    cmd(&mut e, Command::InvertSelection);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().feather, 5.);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(50, 50), 0);
    cmd(&mut e, Command::ExpandSelection { amount: 2 });
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().feather, 5.);
}
/// Local regression: source growth retains transformed source pixels and mask placement.
#[test]
fn local_fill_grows_transformed_layer_without_moving_its_mask() {
    let mut l = shape(10, 10, "#ff0000");
    l.x = 40.;
    l.y = 40.;
    l.rotation = 90.;
    l.flip_x = true;
    l.mask = Some(mask(&[255], 1, 1));
    let old_center = center(&l);
    let mut e = session(100, 100, vec![l]);
    square(&mut e, 10., 10., 10.);
    cmd(
        &mut e,
        Command::SetColor {
            color: "#00ff00".into(),
        },
    );
    cmd(&mut e, Command::FillSelection);
    assert_eq!(probe(&e.history.document, 15, 15), [0, 255, 0, 255]);
    assert_eq!(
        probe(
            &e.history.document,
            old_center.x as u32,
            old_center.y as u32
        ),
        [255, 0, 0, 255]
    );
    assert_eq!(e.selected().unwrap().rotation, 90.);
    let placed = e
        .selected()
        .unwrap()
        .mask
        .as_ref()
        .unwrap()
        .placement
        .unwrap()
        .as_layer(e.selected().unwrap());
    assert!(center(&placed).distance(old_center) < 0.001);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.selected().unwrap().width, 10);
}
/// Local integration: explicit empty coverage is a no-op for every pixel edit; text stays editable.
#[test]
fn local_fill_empty_selection_and_live_text() {
    let text = Layer::new(
        "Text",
        100,
        100,
        Content::Text {
            text: "Hello".into(),
            font_size: 20.,
            font_family: FontFamily::SansSerif,
            color: "#ff0000".into(),
        },
    );
    let mut e = session(100, 100, vec![text]);
    cmd(
        &mut e,
        Command::SetColor {
            color: "#00ff00".into(),
        },
    );
    cmd(&mut e, Command::FillSelection);
    assert!(
        matches!(e.selected().unwrap().content.as_ref(),Content::Text{color,..} if color=="#00ff00")
    );
    cmd(&mut e, Command::SelectAllPixels);
    cmd(&mut e, Command::InvertSelection);
    let before = e.history.document.clone();
    let count = e.history.info().undo_count;
    cmd(&mut e, Command::FillSelection);
    cmd(&mut e, Command::ClearSelectedPixels);
    cmd(&mut e, Command::SetTool { tool: Tool::Brush });
    pointer(&mut e, Phase::Down, 50., 50.);
    pointer(&mut e, Phase::Up, 55., 50.);
    assert_eq!(e.history.document, before);
    assert_eq!(e.history.info().undo_count, count);
    assert_eq!(e.snapshot()["hasPixelSelection"], true);
}
/// BrushTests.opacityCapsTheWholeStrokeEvenWhereItOverlapsItself.
#[test]
fn compositor_brush_opacity_caps_the_entire_stroke() {
    let mut b = brush(200, 80, 20., 1., 0.5);
    for x in [20., 180., 20., 180., 20., 100.] {
        b.input(Point::new(x, 40.)).unwrap();
    }
    b.finish().unwrap();
    let d = brush_doc(&b, 200, 80);
    let p = pixels(&d);
    for x in 20..180 {
        assert!((p[(40 * 200 + x) * 4 + 3] as i32 - 128).abs() <= 1);
    }
    assert_eq!(p[(70 * 200 + 100) * 4 + 3], 0);
}
/// BrushTests.softStrokeBuildsCoverageWhileKeepingItsFeatheredRim and spacedDabsLeaveNoVisibleRippleAlongTheStroke.
#[test]
fn compositor_soft_brush_accumulation_and_even_spacing() {
    let mut dab = brush(200, 80, 40., 0., 1.);
    dab.input(Point::new(100., 40.)).unwrap();
    dab.finish().unwrap();
    let single = probe(&brush_doc(&dab, 200, 80), 100, 50)[3];
    let mut line = brush(200, 80, 40., 0., 1.);
    line.input(Point::new(20., 40.)).unwrap();
    line.input(Point::new(180., 40.)).unwrap();
    line.finish().unwrap();
    assert!(probe(&brush_doc(&line, 200, 80), 100, 50)[3] as i32 > single as i32 + 60);
    for hardness in [0., 0.5, 1.] {
        let mut b = brush(900, 300, 120., hardness, 1.);
        b.input(Point::new(100., 150.)).unwrap();
        b.input(Point::new(800., 150.)).unwrap();
        b.finish().unwrap();
        let p = alphas(&brush_doc(&b, 900, 300));
        for offset in [0, 30, 50] {
            let line = &p[(150 + offset) * 900 + 300..(150 + offset) * 900 + 601];
            assert!(line.iter().max().unwrap() - line.iter().min().unwrap() <= 16);
        }
    }
}
/// BrushTests.sparseMouseSamplesFollowACurveInsteadOfStraightChords and liveStrokeReachesNewestSampleAndTailIsReplacedExactly.
#[test]
fn compositor_brush_curve_and_provisional_tail() {
    let mut b = brush(300, 300, 4., 1., 1.);
    for angle in (0..=180).step_by(30) {
        let r = (angle as f64).to_radians();
        b.input(Point::new(150. + 100. * r.cos(), 150. + 100. * r.sin()))
            .unwrap();
    }
    b.finish().unwrap();
    let d = brush_doc(&b, 300, 300);
    for angle in [45., 75., 105., 135.] {
        let r = f64::to_radians(angle);
        assert!(
            probe(
                &d,
                (150. + 100. * r.cos()) as u32,
                (150. + 100. * r.sin()) as u32
            )[3] > 0
        );
    }
    let mut b = brush(300, 120, 8., 1., 1.);
    for (x, y) in [(20., 60.), (150., 20.), (280., 60.)] {
        b.input(Point::new(x, y)).unwrap();
    }
    assert_eq!(probe(&brush_doc(&b, 300, 120), 278, 60)[3], 255);
    b.finish().unwrap();
    let d = brush_doc(&b, 300, 120);
    assert_eq!(probe(&d, 215, 40)[3], 0);
    assert_eq!(probe(&d, 278, 60)[3], 255);
}
/// BrushTests.paintedBoundsTrimTilePaddingAndKeepSoftEdges (native image storage instead of CGImage).
#[test]
fn compositor_brush_bounds_and_native_project_roundtrip() {
    for hardness in [0., 1.] {
        let mut b = brush(600, 200, 20., hardness, 1.);
        b.input(Point::new(300., 100.)).unwrap();
        let live = b.snapshot().unwrap();
        b.finish().unwrap();
        let final_layer = b.snapshot().unwrap();
        assert!(final_layer.width <= 20 && final_layer.height <= 20);
        assert!(final_layer.x >= 290. && final_layer.y >= 90.);
        assert_eq!(
            rgba_pixels(&Renderer::default().layer_surface(&live).unwrap()).unwrap(),
            rgba_pixels(&Renderer::default().layer_surface(&final_layer).unwrap()).unwrap()
        );
        assert!(matches!(
            final_layer.content.as_ref(),
            Content::Image {
                data: picsie_core::asset::ImageAsset::Tiled(_)
            }
        ));
        let d = brush_doc(&b, 600, 200);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("brush.picsie");
        save_project(&path, &d).unwrap();
        assert_eq!(pixels(&open_project(&path).unwrap()), pixels(&d));
        assert!(!d.metadata().to_string().contains("base64"));
    }
}
/// Local regression for the EditorSession smoothing/finalization path and per-stroke mask cap.
#[test]
fn local_brush_smoothing_mask_cap_and_interrupted_commit() {
    let mut e = session(100, 100, vec![shape(100, 100, "#ff0000")]);
    cmd(
        &mut e,
        Command::AddMask {
            base: MaskMode::Reveal,
        },
    );
    cmd(
        &mut e,
        Command::SetBrush {
            size: 20.,
            opacity: 0.5,
        },
    );
    cmd(
        &mut e,
        Command::SetBrushTip {
            hardness: 1.,
            smoothing: 10.,
        },
    );
    let count = e.history.info().undo_count;
    cmd(&mut e, Command::SetTool { tool: Tool::Brush });
    pointer(&mut e, Phase::Down, 20., 50.);
    pointer(&mut e, Phase::Move, 70., 50.);
    assert_eq!(probe(&e.history.document, 75, 50)[3], 255);
    cmd(&mut e, Command::SetTool { tool: Tool::Move });
    assert_eq!(probe(&e.history.document, 70, 50)[3], 128);
    assert_eq!(e.history.info().undo_count, count + 1);
    cmd(&mut e, Command::Undo);
    assert_eq!(probe(&e.history.document, 50, 50)[3], 255);
    cmd(&mut e, Command::Redo);
    assert_eq!(probe(&e.history.document, 50, 50)[3], 128);
}
/// LayerMaskTests.folderMasksClipEveryLayerInsideAndMultiplyWithTheirOwnMasks.
#[test]
fn compositor_nested_folder_masks_multiply_and_are_pass_through() {
    let mut folder = Layer::new("Folder", 2, 2, Content::Group);
    folder.mask = Some(mask(&[255, 0, 128, 255], 2, 2));
    let mut red = shape(2, 2, "#ff0000");
    red.parent_id = Some(folder.id.clone());
    let mut e = session(2, 2, vec![folder.clone(), red.clone()]);
    assert_eq!(alphas(&e.history.document), [255, 0, 128, 255]);
    red.mask = folder.mask.clone();
    e.history.document.replace(red.clone());
    assert_eq!(alphas(&e.history.document), [255, 0, 64, 255]);
    let mut outer = Layer::new("Outer", 2, 2, Content::Group);
    outer.mask = Some(mask(&[0], 1, 1));
    folder.parent_id = Some(outer.id.clone());
    e.history.document.replace(folder);
    e.history.document.layers.push(outer);
    assert_eq!(alphas(&e.history.document), [0; 4]);
    let mut f = Layer::new("Folder", 2, 2, Content::Group);
    f.mask = Some(mask(&[128], 1, 1));
    let mut a = shape(2, 2, "#ff0000");
    a.parent_id = Some(f.id.clone());
    let mut b = shape(2, 2, "#0000ff");
    b.parent_id = Some(f.id.clone());
    let e = session(2, 2, vec![f, a, b]);
    assert_eq!(alphas(&e.history.document), [192; 4]);
}
/// LayerMaskTests.folderMaskCanBePainted; addMask consumes the selection as upstream specifies.
#[test]
fn compositor_folder_mask_paint_fill_and_selection_history() {
    let folder = Layer::new("Folder", 40, 20, Content::Group);
    let mut red = shape(40, 20, "#ff0000");
    red.parent_id = Some(folder.id.clone());
    let mut e = session(40, 20, vec![red, folder]);
    square(&mut e, 5., 5., 10.);
    let count = e.history.info().undo_count;
    cmd(
        &mut e,
        Command::AddMask {
            base: MaskMode::Reveal,
        },
    );
    assert!(e.history.pixel_selection.is_none());
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(probe(&e.history.document, 10, 10)[3], 0);
    assert_eq!(probe(&e.history.document, 30, 10)[3], 255);
    cmd(&mut e, Command::Undo);
    assert!(e.selected().unwrap().mask.is_none());
    assert!(e.history.pixel_selection.is_some());
    cmd(&mut e, Command::Redo);
    cmd(
        &mut e,
        Command::ResetMask {
            base: MaskMode::Reveal,
        },
    );
    cmd(
        &mut e,
        Command::SetPaintTarget {
            target: PaintTarget::Mask,
        },
    );
    cmd(
        &mut e,
        Command::SetBrush {
            size: 8.,
            opacity: 1.,
        },
    );
    cmd(&mut e, Command::SetTool { tool: Tool::Brush });
    pointer(&mut e, Phase::Down, 10., 10.);
    pointer(&mut e, Phase::Up, 11., 10.);
    assert_eq!(probe(&e.history.document, 10, 10)[3], 0);
    assert_eq!(probe(&e.history.document, 30, 10)[3], 255);
    square(&mut e, 5., 5., 10.);
    cmd(&mut e, Command::ClearSelectedPixels);
    assert_eq!(probe(&e.history.document, 10, 10)[3], 255);
}
fn alpha_layer(values: &[u8]) -> Layer {
    let p: Vec<_> = values.iter().flat_map(|&a| [0, 0, 0, a]).collect();
    Layer::new("Alpha", 2, 2, native_content(rgba_image(2, 2, &p).unwrap()))
}
/// LiveMaskTests clipping stacks preserve base alpha and do not darken soft edges over white.
#[test]
fn compositor_clipping_stack_preserves_soft_alpha_and_colors() {
    let base = alpha_layer(&[255, 128, 32, 0]);
    let red = shape(2, 2, "#ff0000");
    let mut e = session(2, 2, vec![base, red]);
    cmd(&mut e, Command::ToggleClippingMask);
    assert_eq!(alphas(&e.history.document), [255, 128, 32, 0]);
    for p in pixels(&e.history.document).chunks_exact(4).take(3) {
        assert_eq!(&p[..3], &[255, 0, 0]);
    }
    e.history.document.layers[1].opacity = 0.5;
    assert_eq!(alphas(&e.history.document), [255, 128, 32, 0]);
    e.history.document.layers[1].opacity = 1.;
    e.history.document.layers.insert(0, shape(2, 2, "#ffffff"));
    for p in pixels(&e.history.document).chunks_exact(4) {
        assert_eq!(p[0], 255);
        assert_eq!(p[3], 255);
    }
}
/// LiveMaskTests hidden sources still provide alpha, including source opacity and own mask.
#[test]
fn compositor_hidden_live_source_and_chained_masks() {
    let red = shape(2, 2, "#ff0000");
    let mut source = alpha_layer(&[255, 0, 128, 255]);
    source.visible = false;
    source.opacity = 0.5;
    source.mask = Some(mask(&[128], 1, 1));
    let mut e = session(2, 2, vec![red, source.clone()]);
    let target = e.history.document.layers[0].id.clone();
    e.select(Some(target), SelectionMode::Replace).unwrap();
    cmd(
        &mut e,
        Command::LinkMask {
            source_id: source.id.clone(),
        },
    );
    assert_eq!(alphas(&e.history.document), [64, 0, 32, 64]);
    let second = alpha_layer(&[255, 255, 0, 128]);
    e.history.document.layers[1].mask_source_id = Some(second.id.clone());
    e.history.document.layers.push(second);
    e.history.document.layers[2].visible = false;
    assert_eq!(alphas(&e.history.document), [64, 0, 0, 32]);
}
/// LiveMaskTests toggling, detach on reorder, insertion adoption and cycle rejection.
#[test]
fn compositor_clipping_graph_editing_and_validation() {
    let a = shape(2, 2, "#ff0000");
    let b = shape(2, 2, "#00ff00");
    let c = shape(2, 2, "#0000ff");
    let ids = [a.id.clone(), b.id.clone(), c.id.clone()];
    let mut e = session(2, 2, vec![a, b, c]);
    e.select(Some(ids[0].clone()), SelectionMode::Replace)
        .unwrap();
    cmd(&mut e, Command::ToggleClippingMask);
    assert!(!e.history.info().can_undo);
    for id in &ids[1..] {
        e.select(Some(id.clone()), SelectionMode::Replace).unwrap();
        cmd(&mut e, Command::ToggleClippingMask);
    }
    assert!(
        e.history.document.layers[1..]
            .iter()
            .all(|l| l.mask_source_id.as_ref() == Some(&ids[0]))
    );
    e.select(Some(ids[1].clone()), SelectionMode::Replace)
        .unwrap();
    cmd(&mut e, Command::ToggleClippingMask);
    assert!(
        e.history
            .document
            .layers
            .iter()
            .all(|l| l.mask_source_id.is_none())
    );
    cmd(&mut e, Command::Undo);
    e.select(Some(ids[2].clone()), SelectionMode::Replace)
        .unwrap();
    cmd(
        &mut e,
        Command::ReorderTo {
            target_id: ids[0].clone(),
            side: Side::Below,
        },
    );
    assert!(e.history.document.layers[0].mask_source_id.is_none());
    assert_eq!(
        e.history.document.layers[2].mask_source_id.as_ref(),
        Some(&ids[0])
    );
    let before = e.history.document.clone();
    e.select(Some(ids[0].clone()), SelectionMode::Replace)
        .unwrap();
    assert!(
        e.command(Command::LinkMask {
            source_id: ids[1].clone()
        })
        .is_err()
    );
    assert_eq!(e.history.document, before);
    assert!(
        e.command(Command::LinkMask {
            source_id: "missing".into()
        })
        .is_err()
    );
    e.select(Some(ids[2].clone()), SelectionMode::Replace)
        .unwrap();
    cmd(
        &mut e,
        Command::ReorderTo {
            target_id: ids[0].clone(),
            side: Side::Above,
        },
    );
    assert!(
        e.history.document.layers[1..]
            .iter()
            .all(|l| l.mask_source_id.as_ref() == Some(&ids[0]))
    );
}
/// LiveMaskTests source deletion bakes the dependency; local .picsie/.comp and folder-copy regressions.
#[test]
fn compositor_mask_roundtrip_duplicate_and_bake_delete() {
    let mut folder = Layer::new("Folder", 2, 2, Content::Group);
    folder.mask = Some(mask(&[255, 128, 255, 255], 2, 2));
    let mut base = alpha_layer(&[255, 128, 32, 0]);
    base.visible = false;
    base.parent_id = Some(folder.id.clone());
    let mut red = shape(2, 2, "#ff0000");
    red.parent_id = Some(folder.id.clone());
    red.mask_source_id = Some(base.id.clone());
    red.mask = Some(mask(&[128], 1, 1));
    let source = base.id.clone();
    let target = red.id.clone();
    let mut e = session(2, 2, vec![base, red, folder.clone()]);
    let before = pixels(&e.history.document);
    let dir = tempfile::tempdir().unwrap();
    for ext in ["picsie", "comp"] {
        let path = dir.path().join(format!("masked.{ext}"));
        save_project(&path, &e.history.document).unwrap();
        let d = open_project(&path).unwrap();
        assert_eq!(pixels(&d), before);
        assert_eq!(
            d.layers
                .iter()
                .find(|l| l.id == target)
                .unwrap()
                .mask_source_id
                .as_ref(),
            Some(&source)
        );
    }
    cmd(&mut e, Command::Duplicate);
    let copy = e.selected_id().unwrap().to_owned();
    let copied: Vec<_> = e
        .history
        .document
        .layers
        .iter()
        .filter(|l| l.parent_id.as_ref() == Some(&copy))
        .collect();
    assert_eq!(copied.len(), 2);
    let copied_source = copied.iter().find(|l| !l.visible).unwrap();
    assert_eq!(
        copied
            .iter()
            .find(|l| l.visible)
            .unwrap()
            .mask_source_id
            .as_ref(),
        Some(&copied_source.id)
    );
    cmd(&mut e, Command::Undo);
    e.select(Some(source.clone()), SelectionMode::Replace)
        .unwrap();
    cmd(&mut e, Command::Remove);
    assert_eq!(pixels(&e.history.document), before);
    assert!(
        e.history
            .document
            .layers
            .iter()
            .find(|l| l.id == target)
            .unwrap()
            .mask_source_id
            .is_none()
    );
    cmd(&mut e, Command::Undo);
    assert_eq!(pixels(&e.history.document), before);
    assert_eq!(
        e.history
            .document
            .layers
            .iter()
            .find(|l| l.id == target)
            .unwrap()
            .mask_source_id
            .as_ref(),
        Some(&source)
    );
}

/// BrushTests.brushStaysCircularOnNonuniformRotatedFlippedLayer, preserving its transform and probes.
#[test]
fn compositor_brush_diameter_is_in_document_space() {
    let mut source = vec![0u8; 100 * 100 * 4];
    for y in 2..8 {
        for x in 2..8 {
            source[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
        }
    }
    let mut layer = Layer::new(
        "Fixture",
        100,
        100,
        native_content(rgba_image(100, 100, &source).unwrap()),
    );
    layer.x = 25.;
    layer.y = -50.;
    layer.scale_x = 0.5;
    layer.scale_y = 2.;
    layer.rotation = 90.;
    layer.flip_x = true;
    layer.flip_y = true;
    layer.sampling = Sampling::Nearest;
    let mut e = session(100, 100, vec![layer]);
    cmd(
        &mut e,
        Command::SetColor {
            color: "#00ff00".into(),
        },
    );
    cmd(
        &mut e,
        Command::SetBrush {
            size: 16.,
            opacity: 1.,
        },
    );
    cmd(&mut e, Command::SetTool { tool: Tool::Brush });
    pointer(&mut e, Phase::Down, 50., 50.);
    let live = pixels(&e.history.document);
    pointer(&mut e, Phase::Up, 50., 50.);
    let result = pixels(&e.history.document);
    for (x, y) in [(50, 50), (54, 50), (50, 54)] {
        assert!(result[(y * 100 + x) * 4 + 1] > 240);
        assert!(live[(y * 100 + x) * 4 + 1] > 240);
    }
    for (x, y) in [(61, 50), (50, 61)] {
        assert_eq!(result[(y * 100 + x) * 4 + 3], 0);
    }
}

/// Local multi-selection adaptation: copies link to the copied base, not the original.
#[test]
fn local_duplicate_multiple_roots_remaps_clipping_sources() {
    let base = shape(2, 2, "#ff0000");
    let mut top = shape(2, 2, "#00ff00");
    top.mask_source_id = Some(base.id.clone());
    let mut e = session(2, 2, vec![base, top]);
    cmd(&mut e, Command::SelectAll);
    cmd(&mut e, Command::Duplicate);
    let copies: Vec<_> = e
        .history
        .document
        .layers
        .iter()
        .filter(|l| e.selection.ids.contains(&l.id))
        .collect();
    assert_eq!(copies.len(), 2);
    let base = copies.iter().find(|l| l.mask_source_id.is_none()).unwrap();
    let top = copies.iter().find(|l| l.mask_source_id.is_some()).unwrap();
    assert_eq!(top.mask_source_id.as_ref(), Some(&base.id));
    e.history.document.validate().unwrap();
}

/// Local regression of LiveMaskBaker's inverse target grid: deletion retains off-canvas pixels.
#[test]
fn local_delete_source_keeps_recoverable_off_canvas_coverage() {
    let mut base = shape(40, 40, "#000000");
    base.x = -10.;
    base.y = -10.;
    base.visible = false;
    base.opacity = 0.5;
    let mut top = shape(40, 40, "#ff0000");
    top.x = -10.;
    top.y = -10.;
    top.mask_source_id = Some(base.id.clone());
    let source = base.id.clone();
    let target = top.id.clone();
    let mut e = session(20, 20, vec![base, top]);
    e.select(Some(source), SelectionMode::Replace).unwrap();
    cmd(&mut e, Command::Remove);
    e.select(Some(target), SelectionMode::Replace).unwrap();
    cmd(
        &mut e,
        Command::Nudge {
            delta: Point::new(10., 10.),
        },
    );
    assert_eq!(probe(&e.history.document, 0, 0), [255, 0, 0, 128]);
}
