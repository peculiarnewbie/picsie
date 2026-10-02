//! Shape tool and interactive gradient fixtures translated from the pinned Compositor
//! suite, plus local regressions for the Picsie adaptation. Upstream MIT © 2026
//! Wonder Assembly LLC; source mapping in docs/compositor-port.md.
//!
//! Translated (adapted) fixtures:
//! - ShapeToolTests.swift: rectangle fill as one undo, ellipse circle from center,
//!   click/Escape/tool-switch makes nothing, rounded-rectangle radius and pill clamp.
//! - GradientTests.swift: foreground-to-background commit, radial symmetry,
//!   reverse/opacity, foreground-to-transparent over paint, cancel/click/undo,
//!   redrag replacement, mask coverage, palette refresh.
//! Local engine differences: procedural shape layers (no baked raster), straight-alpha
//! pixel reads instead of premultiplied CoreGraphics bytes, and typed Apply/Cancel
//! commands instead of SwiftUI buttons. Additional local tests below the translated
//! block cover old-project compatibility, package round-trips, cycling, clamps,
//! history resolution, and the retained prototype gradient layer.
use picsie_core::{editor::*, geometry::*, history::*, model::*, render::*};
use serde_json::json;
use std::sync::Arc;

fn doc_with(width: u32, height: u32, layers: Vec<Layer>) -> Document {
    let mut d = Document::new("Test", width, height).unwrap();
    d.layers = layers;
    d
}
fn editor(width: u32, height: u32, layers: Vec<Layer>) -> Editor {
    let mut e = Editor::new(doc_with(width, height, layers)).unwrap();
    e.viewport = Viewport {
        width: width as f64,
        height: height as f64,
        zoom: 1.,
        pan: Point::default(),
    };
    e.display_scale = 1.;
    e
}
fn paint_layer(name: &str, width: u32, height: u32) -> Layer {
    Layer::new(name, width, height, Content::Paint)
}
fn cmd(e: &mut Editor, v: serde_json::Value) {
    e.command(serde_json::from_value(v).unwrap()).unwrap();
}
fn pointer(e: &mut Editor, phase: Phase, x: f64, y: f64) {
    pointer_mod(e, phase, x, y, Modifiers::default());
}
fn pointer_mod(e: &mut Editor, phase: Phase, x: f64, y: f64, modifiers: Modifiers) {
    e.pointer(PointerSample {
        phase,
        point: Point::new(x, y),
        modifiers,
    })
    .unwrap();
}
fn drag(e: &mut Editor, from: (f64, f64), to: (f64, f64), modifiers: Modifiers) {
    pointer_mod(e, Phase::Down, from.0, from.1, modifiers);
    pointer_mod(e, Phase::Move, to.0, to.1, modifiers);
    pointer_mod(e, Phase::Up, to.0, to.1, modifiers);
}
fn pix(d: &Document, x: f64, y: f64) -> [u8; 4] {
    Renderer::default().sample(d, Point::new(x, y)).unwrap()
}
fn overlap_pixels(doc: &Document, x0: u32, y0: u32, w: u32, h: u32) -> Vec<u8> {
    let mut image = Renderer::default().render(doc).unwrap();
    let all = picsie_core::render::rgba_pixels(&image.image_snapshot()).unwrap();
    let stride = doc.width as usize * 4;
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for y in y0..y0 + h {
        out.extend_from_slice(&all[y as usize * stride + x0 as usize * 4..][..w as usize * 4]);
    }
    out
}
fn shape_session() -> Editor {
    let mut e = editor(100, 80, vec![paint_layer("Layer 1", 100, 80)]);
    cmd(&mut e, json!({"type":"setTool","tool":"rectangle"}));
    cmd(&mut e, json!({"type":"setColor","color":"#ff0000"}));
    e
}
fn gradient_session(width: u32, height: u32) -> Editor {
    let mut e = editor(width, height, vec![paint_layer("Layer 1", width, height)]);
    cmd(&mut e, json!({"type":"setTool","tool":"gradient"}));
    cmd(
        &mut e,
        json!({"type":"setGradientStyle","style":"foreground-to-background"}),
    );
    e
}
fn gradient_drag(e: &mut Editor, from: (f64, f64), to: (f64, f64)) {
    drag(e, from, to, Modifiers::default());
}
fn near(value: u8, target: u8, tolerance: u8) -> bool {
    value.abs_diff(target) <= tolerance
}

// ShapeToolTests.rectangleFillsANewLayerWithTheForegroundColorAsOneUndoStep.
#[test]
fn translated_rectangle_fill_is_one_undo_and_keeps_selection() {
    let mut e = shape_session();
    cmd(&mut e, json!({"type":"selectAllPixels"}));
    let count = e.history.info().undo_count;
    drag(&mut e, (10., 10.), (40., 30.), Modifiers::default());
    let doc = &e.history.document;
    assert_eq!(
        doc.layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["Layer 1", "Rectangle 1"]
    );
    assert_eq!(e.history.info().undo_count, count + 1);
    let active = e.selected().unwrap();
    assert_eq!(active.name, "Rectangle 1");
    assert_eq!((active.x, active.y), (10., 10.));
    assert_eq!((active.width, active.height), (30, 20));
    assert!(e.history.pixel_selection.is_some());
    assert_eq!(pix(&e.history.document, 25., 20.), [255, 0, 0, 255]);
    assert_eq!(pix(&e.history.document, 9., 20.)[3], 0);
    assert_eq!(pix(&e.history.document, 40., 20.)[3], 0);
    drag(&mut e, (60., 10.), (70., 20.), Modifiers::default());
    assert_eq!(e.selected().unwrap().name, "Rectangle 2");
    cmd(&mut e, json!({"type":"undo"}));
    cmd(&mut e, json!({"type":"undo"}));
    assert_eq!(
        e.history
            .document
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["Layer 1"]
    );
}

// ShapeToolTests.ellipseLeavesItsCornersClearWithShiftCircleAndOptionFromCenter.
#[test]
fn translated_ellipse_circle_from_center_leaves_corners_clear() {
    let mut e = shape_session();
    cmd(&mut e, json!({"type":"cycleShapeKind"}));
    drag(
        &mut e,
        (50., 40.),
        (60., 45.),
        Modifiers {
            shift: true,
            alt: true,
            ..Default::default()
        },
    );
    let active = e.selected().unwrap();
    assert_eq!(active.name, "Ellipse 1");
    assert_eq!(
        (active.x, active.y, active.width, active.height),
        (40., 30., 20, 20)
    );
    assert_eq!(pix(&e.history.document, 50., 40.), [255, 0, 0, 255]);
    assert_eq!(pix(&e.history.document, 40., 30.)[3], 0);
    assert_eq!(pix(&e.history.document, 59., 49.)[3], 0);
}

// ShapeToolTests.aClickEscapeOrToolSwitchMakesNoLayer.
#[test]
fn translated_click_escape_or_tool_switch_makes_no_layer() {
    let mut e = shape_session();
    let count = e.history.info().undo_count;
    pointer(&mut e, Phase::Down, 20., 20.);
    pointer(&mut e, Phase::Up, 20., 20.);
    assert_eq!(e.history.info().undo_count, count);
    pointer(&mut e, Phase::Down, 20., 20.);
    pointer_mod(&mut e, Phase::Move, 50., 50., Modifiers::default());
    cmd(&mut e, json!({"type":"cancelGesture"}));
    assert_eq!(e.history.document.layers.len(), 1);
    pointer(&mut e, Phase::Down, 20., 20.);
    pointer_mod(&mut e, Phase::Move, 50., 50., Modifiers::default());
    cmd(&mut e, json!({"type":"setTool","tool":"brush"}));
    assert_eq!(e.history.info().undo_count, count);
    assert_eq!(e.history.document.layers.len(), 1);
}

// ShapeToolTests.roundedRectanglesFollowTheRadiusAndClampToAPill.
#[test]
fn translated_rounded_rectangles_follow_radius_and_clamp_to_pill() {
    let mut e = shape_session();
    cmd(&mut e, json!({"type":"setShapeCornerRadius","radius":8.}));
    drag(&mut e, (10., 10.), (50., 40.), Modifiers::default());
    cmd(&mut e, json!({"type":"setShapeCornerRadius","radius":500.}));
    drag(&mut e, (55., 50.), (95., 70.), Modifiers::default());
    assert_eq!(pix(&e.history.document, 10., 10.)[3], 0);
    assert_eq!(pix(&e.history.document, 13., 13.)[3], 255);
    assert_eq!(pix(&e.history.document, 30., 10.)[3], 255);
    assert_eq!(pix(&e.history.document, 55., 50.)[3], 0);
    assert_eq!(pix(&e.history.document, 75., 60.), [255, 0, 0, 255]);
    assert_eq!(e.history.document.layers.len(), 3);
    // Ellipses take no radius even when one is set.
    cmd(&mut e, json!({"type":"cycleShapeKind"}));
    cmd(&mut e, json!({"type":"setTool","tool":"ellipse"}));
    drag(&mut e, (5., 5.), (25., 25.), Modifiers::default());
    let ellipse = e.selected().unwrap();
    assert!(matches!(
        ellipse.content.as_ref(),
        Content::Shape {
            corner_radius,
            ..
        } if *corner_radius == 0.
    ));
}

// Local: lines carry editable width/endpoints with Shift 45-degree snapping.
#[test]
fn local_line_width_endpoints_and_shift_snapping() {
    let mut e = shape_session();
    cmd(&mut e, json!({"type":"setTool","tool":"line"}));
    cmd(&mut e, json!({"type":"setShapeLineWidth","width":6.}));
    drag(
        &mut e,
        (10., 10.),
        (40., 15.),
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    let active = e.selected().unwrap().clone();
    let (start, end, width) = match active.content.as_ref() {
        Content::Shape {
            shape: Shape::Line,
            line_start: Some(start),
            line_end: Some(end),
            line_width: Some(width),
            ..
        } => (*start, *end, *width),
        other => panic!("expected an editable line, got {other:?}"),
    };
    assert_eq!(width, 6.);
    // A 9.5-degree drag snaps flat while keeping the dragged length.
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    assert!(dy.abs() < 0.01, "snapped flat: {start:?} {end:?}");
    assert!((dx.hypot(dy) - (30f64.hypot(5.)) / active.width as f64).abs() < 0.05);
    assert_eq!(active.name, "Line 1");
    assert_eq!(pix(&e.history.document, 25., 10.)[0], 255);
    // A click without a drag makes nothing.
    let layers = e.history.document.layers.len();
    pointer(&mut e, Phase::Down, 60., 60.);
    pointer(&mut e, Phase::Up, 60., 60.);
    assert_eq!(e.history.document.layers.len(), layers);
}

// Local: cycling steps through all three source kinds, discarding a draft.
#[test]
fn local_shape_cycling_steps_through_all_kinds() {
    let mut e = shape_session();
    assert_eq!(e.tool, Tool::Rectangle);
    cmd(&mut e, json!({"type":"cycleShapeKind"}));
    assert_eq!(e.tool, Tool::Ellipse);
    pointer(&mut e, Phase::Down, 20., 20.);
    pointer_mod(&mut e, Phase::Move, 50., 50., Modifiers::default());
    cmd(&mut e, json!({"type":"cycleShapeKind"}));
    assert_eq!(e.tool, Tool::Line);
    assert_eq!(e.history.document.layers.len(), 1);
    cmd(&mut e, json!({"type":"cycleShapeKind"}));
    assert_eq!(e.tool, Tool::Rectangle);
}

// Local: radius/width commands clamp to the source 0…5000 / 1…5000 ranges.
#[test]
fn local_shape_setting_clamps_match_source_ranges() {
    let mut e = shape_session();
    cmd(
        &mut e,
        json!({"type":"setShapeCornerRadius","radius":99999.}),
    );
    assert_eq!(e.shape_corner_radius, 5000.);
    cmd(&mut e, json!({"type":"setShapeCornerRadius","radius":-3.}));
    assert_eq!(e.shape_corner_radius, 0.);
    cmd(&mut e, json!({"type":"setShapeLineWidth","width":0.}));
    assert_eq!(e.shape_line_width, 1.);
    assert!(
        e.command(Command::SetShapeLineWidth { width: f64::NAN })
            .is_err()
    );
}

// GradientTests.foregroundToBackgroundFillsCanvasAndCommitsOneUndo.
#[test]
fn translated_gradient_commit_is_one_undo_with_exact_endpoints() {
    let mut e = gradient_session(101, 4);
    let count = e.history.info().undo_count;
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    assert!(e.gradient_edit.is_some());
    assert_eq!(e.history.info().undo_count, count);
    cmd(&mut e, json!({"type":"commitGradient"}));
    assert!(e.gradient_edit.is_none());
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(pix(&e.history.document, 0., 0.), [0, 0, 0, 255]);
    assert_eq!(pix(&e.history.document, 100., 3.), [255, 255, 255, 255]);
    let middle = pix(&e.history.document, 50., 1.);
    assert!(near(middle[0], 128, 2) && middle[1] == middle[0] && middle[3] == 255);
    cmd(&mut e, json!({"type":"undo"}));
    assert!(matches!(
        e.selected().unwrap().content.as_ref(),
        Content::Paint
    ));
}

// GradientTests.radialSpreadsFromStartToRimInEveryDirection.
#[test]
fn translated_radial_gradient_is_symmetric_to_its_rim() {
    let mut e = gradient_session(101, 101);
    // Blank the canvas footprint first: the test document starts transparent.
    cmd(&mut e, json!({"type":"setGradientShape","shape":"radial"}));
    gradient_drag(&mut e, (50.5, 50.5), (90.5, 50.5));
    cmd(&mut e, json!({"type":"commitGradient"}));
    let doc = &e.history.document;
    assert_eq!(pix(doc, 50., 50.), [0, 0, 0, 255]);
    let halfway =
        [(70, 50), (30, 50), (50, 70), (50, 30)].map(|(x, y)| pix(doc, x as f64, y as f64)[0]);
    assert!(
        halfway
            .iter()
            .all(|v| near(*v, 128, 5) && near(*v, halfway[0], 2)),
        "{halfway:?}"
    );
    assert_eq!(pix(doc, 100., 50.), [255, 255, 255, 255]);
    assert_eq!(pix(doc, 0., 0.), [255, 255, 255, 255]);
}

// GradientTests.reverseOpacityAndDirectionFollowSettings.
#[test]
fn translated_reversed_half_opacity_gradient() {
    let mut e = gradient_session(101, 4);
    cmd(&mut e, json!({"type":"setGradientReverse","reversed":true}));
    cmd(&mut e, json!({"type":"setGradientOpacity","opacity":0.5}));
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    let start = pix(&e.history.document, 0., 2.);
    assert!(near(start[3], 128, 2) && start[0] == 255, "{start:?}");
    let end = pix(&e.history.document, 100., 2.);
    assert!(near(end[0], 0, 3) && near(end[3], 128, 2), "{end:?}");
}

// GradientTests.foregroundToTransparentPreservesUnderlyingPixelsAndAlpha.
#[test]
fn translated_transparent_gradient_preserves_underlying_pixels() {
    let mut e = gradient_session(101, 4);
    cmd(&mut e, json!({"type":"setColor","color":"#ff0000"}));
    gradient_drag(&mut e, (100.5, 2.), (101., 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    assert_eq!(pix(&e.history.document, 50., 2.), [255, 0, 0, 255]);
    cmd(&mut e, json!({"type":"setColor","color":"#000000"}));
    cmd(
        &mut e,
        json!({"type":"setGradientStyle","style":"foreground-to-transparent"}),
    );
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    assert_eq!(pix(&e.history.document, 0., 2.), [0, 0, 0, 255]);
    assert_eq!(pix(&e.history.document, 100., 2.), [255, 0, 0, 255]);
    let middle = pix(&e.history.document, 50., 2.);
    assert!(
        near(middle[0], 128, 3) && middle[1] == 0 && middle[3] == 255,
        "{middle:?}"
    );
}

// GradientTests.cancelUndoAndClicksLeaveDocumentUntouched.
#[test]
fn translated_gradient_cancel_clicks_and_undo_leave_document() {
    let mut e = gradient_session(101, 4);
    let before = e.history.document.clone();
    pointer(&mut e, Phase::Down, 10., 2.);
    pointer(&mut e, Phase::Up, 10., 2.);
    assert!(e.gradient_edit.is_none());
    gradient_drag(&mut e, (0., 2.), (100., 2.));
    cmd(&mut e, json!({"type":"cancelGradient"}));
    assert!(e.gradient_edit.is_none());
    assert_eq!(e.history.document, before);
    gradient_drag(&mut e, (0., 2.), (100., 2.));
    cmd(&mut e, json!({"type":"undo"}));
    assert!(e.gradient_edit.is_none());
    assert_eq!(e.history.document, before);
}

// GradientTests.redraggingReplacesPendingLineWithoutAccumulating.
//
// The session-level redrag restarts the pending line; at canvas level a press on
// an endpoint grabs it instead (EditorCanvas.beginGradientDrag), so the second
// press lands clear of the first line's end handle.
#[test]
fn translated_redrag_replaces_pending_line_without_accumulating() {
    let mut e = gradient_session(101, 4);
    gradient_drag(&mut e, (0.5, 2.), (80.5, 2.));
    pointer(&mut e, Phase::Down, 100.5, 2.);
    pointer(&mut e, Phase::Move, 0.5, 2.);
    pointer(&mut e, Phase::Up, 0.5, 2.);
    cmd(&mut e, json!({"type":"commitGradient"}));
    assert_eq!(e.history.info().undo_count, 1);
    assert_eq!(pix(&e.history.document, 0., 2.), [255, 255, 255, 255]);
    assert_eq!(pix(&e.history.document, 100., 2.), [0, 0, 0, 255]);
}

// GradientTests.maskGradientWritesCoverageInsideLayerBounds.
#[test]
fn translated_mask_gradient_writes_coverage_inside_layer_bounds() {
    let mut e = gradient_session(101, 4);
    gradient_drag(&mut e, (0., 2.), (0.6, 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    let id = e.selected_id().unwrap().to_owned();
    e.select(Some(id), SelectionMode::Replace).unwrap();
    cmd(&mut e, json!({"type":"setPaintTarget","target":"mask"}));
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    assert_eq!(pix(&e.history.document, 0., 2.)[3], 0);
    assert_eq!(pix(&e.history.document, 100., 2.)[3], 255);
    assert!(near(pix(&e.history.document, 50., 2.)[3], 128, 2));
}

// GradientTests.paletteChangesUpdatePendingPreview.
#[test]
fn translated_palette_change_refreshes_pending_preview() {
    let mut e = gradient_session(101, 4);
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    cmd(&mut e, json!({"type":"swapPaletteColors"}));
    assert!(e.gradient_edit.is_some());
    assert_eq!(e.color, "#ffffff");
    cmd(&mut e, json!({"type":"cancelGradient"}));
}

// Local: numeric resize redraws the shape at its displayed size (redrawShape),
// preserving document-pixel radius, line width and mask coverage in one undo.
#[test]
fn local_resize_redraw_preserves_radius_width_and_masks() {
    let mut e = shape_session();
    cmd(&mut e, json!({"type":"setShapeCornerRadius","radius":8.}));
    drag(&mut e, (20., 10.), (60., 40.), Modifiers::default());
    let id = e.selected_id().unwrap().to_owned();
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    // Hide the left half of the shape through its mask.
    {
        let layer = e.selected().unwrap().clone();
        let mut mask = layer.mask.as_ref().unwrap().as_ref().clone();
        let mut pixels = vec![255u8; 40 * 30];
        for y in 0..30 {
            for x in 0..20 {
                pixels[y * 40 + x] = 0;
            }
        }
        mask.raster = Some(Arc::new(MaskRaster {
            width: 40,
            height: 30,
            pixels: Arc::new(pixels),
        }));
        let mut doc = e.history.document.clone();
        doc.layers.iter_mut().find(|l| l.id == id).unwrap().mask = Some(Arc::new(mask));
        e.edit("Mask half", doc, None).unwrap();
    }
    assert_eq!(pix(&e.history.document, 22., 20.)[3], 0);
    // Doubling the width redraws at 80px with the radius intact (a stretched
    // 16px radius would cut the checked corner pixel away).
    cmd(&mut e, json!({"type":"updateLayer","patch":{"scaleX":2.0}}));
    let resized = e.selected().unwrap().clone();
    assert_eq!((resized.width, resized.height), (80, 30));
    assert_eq!((resized.scale_x, resized.scale_y), (1., 1.));
    assert_eq!((resized.x, resized.y), (20., 10.));
    let (radius, kind) = match resized.content.as_ref() {
        Content::Shape {
            shape,
            corner_radius,
            ..
        } => (*corner_radius, *shape),
        other => panic!("shape stayed editable: {other:?}"),
    };
    assert_eq!(kind, Shape::Rectangle);
    assert_eq!(radius, 8.);
    assert_eq!(pix(&e.history.document, 96., 14.)[3], 255);
    assert_eq!(pix(&e.history.document, 20., 10.)[3], 0);
    // The mask pins its pre-redraw footprint with a compensated explicit
    // placement (scale halved for the doubled grid); the retained raster keeps
    // covering the same document pixels: doc x<40 hidden, x>=40 shown.
    let mask = resized.mask.as_ref().unwrap();
    assert_eq!(
        (
            mask.raster.as_ref().unwrap().width,
            mask.raster.as_ref().unwrap().height
        ),
        (40, 30)
    );
    let placement = mask.placement.expect("pinned explicit placement");
    assert_eq!((placement.x, placement.y), (20., 10.));
    assert_eq!((placement.scale_x, placement.scale_y), (1., 1.));
    assert_eq!(pix(&e.history.document, 22., 20.)[3], 0);
    assert_eq!(pix(&e.history.document, 60., 20.), [255, 0, 0, 255]);
    // Nonuniform halve keeps the stored radius; drawing clamps it to the pill.
    cmd(&mut e, json!({"type":"updateLayer","patch":{"scaleY":0.5}}));
    let resized = e.selected().unwrap().clone();
    assert_eq!((resized.width, resized.height), (80, 15));
    assert_eq!((resized.scale_x, resized.scale_y), (1., 1.));
    assert!(matches!(
        resized.content.as_ref(),
        Content::Shape { corner_radius, .. } if *corner_radius == 8.
    ));
    assert_eq!(pix(&e.history.document, 60., 20.), [255, 0, 0, 255]);
    cmd(&mut e, json!({"type":"undo"}));
    cmd(&mut e, json!({"type":"undo"}));
    let restored = e.selected().unwrap();
    assert_eq!((restored.width, restored.height), (40, 30));
    assert_eq!((restored.scale_x, restored.scale_y), (1., 1.));
    assert_eq!(pix(&e.history.document, 22., 20.)[3], 0);
}

// Local: a full-white linked mask still covers the resized shape.
#[test]
fn local_full_white_mask_covers_resized_shape() {
    let mut e = shape_session();
    drag(&mut e, (20., 10.), (60., 40.), Modifiers::default());
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    assert_eq!(pix(&e.history.document, 40., 25.), [255, 0, 0, 255]);
    cmd(&mut e, json!({"type":"updateLayer","patch":{"scaleX":2.0}}));
    let resized = e.selected().unwrap();
    assert_eq!((resized.width, resized.scale_x), (80, 1.));
    assert!(resized.mask.as_ref().unwrap().placement.is_some());
    assert_eq!(pix(&e.history.document, 40., 25.), [255, 0, 0, 255]);
    assert_eq!(pix(&e.history.document, 95., 25.), [255, 0, 0, 255]);
    assert_eq!(pix(&e.history.document, 40., 45.)[3], 0);
}

// Local: an independently placed unlinked mask keeps its document footprint.
#[test]
fn local_unlinked_mask_keeps_footprint_across_redraw() {
    let mut e = shape_session();
    drag(&mut e, (20., 10.), (60., 40.), Modifiers::default());
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    {
        let layer = e.selected().unwrap().clone();
        let mut mask = layer.mask.as_ref().unwrap().as_ref().clone();
        mask.linked = false;
        mask.placement = Some(MaskPlacement {
            sampling: Sampling::High,
            x: 20.,
            y: 10.,
            scale_x: 1.,
            scale_y: 1.,
            rotation: 0.,
            flip_x: false,
            flip_y: false,
        });
        let mut pixels = vec![255u8; 40 * 30];
        for y in 0..30 {
            for x in 0..20 {
                pixels[y * 40 + x] = 0;
            }
        }
        mask.raster = Some(Arc::new(MaskRaster {
            width: 40,
            height: 30,
            pixels: Arc::new(pixels),
        }));
        let mut doc = e.history.document.clone();
        doc.layers
            .iter_mut()
            .find(|l| l.id == layer.id)
            .unwrap()
            .mask = Some(Arc::new(mask));
        e.edit("Unlink mask", doc, None).unwrap();
    }
    // Numeric transform fields target the layer once painting returns to content.
    cmd(&mut e, json!({"type":"setPaintTarget","target":"content"}));
    let before = overlap_pixels(&e.history.document, 0, 10, 60, 30);
    cmd(&mut e, json!({"type":"updateLayer","patch":{"scaleX":2.0}}));
    let resized = e.selected().unwrap().clone();
    assert_eq!((resized.width, resized.scale_x), (80, 1.));
    let placement = resized.mask.as_ref().unwrap().placement.unwrap();
    assert_eq!((placement.scale_x, placement.scale_y), (0.5, 1.));
    // The overlap with the pre-resize frame renders byte-identical: the mask
    // footprint did not move while the shape doubled around it.
    assert_eq!(overlap_pixels(&e.history.document, 0, 10, 60, 30), before);
    assert_eq!(pix(&e.history.document, 22., 20.)[3], 0);
    assert_eq!(pix(&e.history.document, 60., 20.), [255, 0, 0, 255]);
}

// Local: legacy mask strokes bake into coverage so the redraw still applies.
#[test]
fn local_legacy_mask_strokes_bake_through_redraw() {
    let mut e = shape_session();
    drag(&mut e, (20., 10.), (60., 40.), Modifiers::default());
    {
        let layer = e.selected().unwrap().clone();
        let mut mask = layer.mask.as_ref().cloned().unwrap_or_else(|| {
            Arc::new(LayerMask {
                enabled: true,
                base: MaskMode::Reveal,
                raster: None,
                linked: true,
                placement: None,
                strokes: vec![],
            })
        });
        let mask = Arc::make_mut(&mut mask);
        mask.raster = None;
        mask.strokes = vec![Arc::new(MaskStroke {
            mode: MaskMode::Reveal,
            size: 6.,
            opacity: 1.,
            points: vec![Point::new(10., 10.), Point::new(30., 20.)],
        })];
        let mut doc = e.history.document.clone();
        doc.version = 1;
        doc.layers
            .iter_mut()
            .find(|l| l.id == layer.id)
            .unwrap()
            .mask = Some(Arc::new(mask.clone()));
        e.edit("Legacy strokes", doc, None).unwrap();
    }
    let before = overlap_pixels(&e.history.document, 0, 10, 60, 30);
    cmd(&mut e, json!({"type":"updateLayer","patch":{"scaleX":2.0}}));
    let resized = e.selected().unwrap().clone();
    assert_eq!((resized.width, resized.scale_x), (80, 1.));
    let mask = resized.mask.as_ref().unwrap();
    assert!(mask.strokes.is_empty());
    assert!(mask.raster.is_some());
    assert_eq!(e.history.document.version, 2);
    // The overlap with the pre-resize frame renders byte-identical: baked
    // coverage kept its footprint while the shape doubled around it.
    assert_eq!(overlap_pixels(&e.history.document, 0, 10, 60, 30), before);
}

// Local: the transform preview stays bounded (2048 long side), reuses its cache,
// and never leaks into full renders; the capped radius still reads correctly.
#[test]
fn local_shape_preview_bounded_cached_and_full_renders_exact() {
    let mut layer = Layer::new(
        "Big",
        3000,
        1500,
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#ff0000".into(),
            corner_radius: 100.,
            line_width: None,
            line_start: None,
            line_end: None,
        },
    );
    layer.scale_x = 2.;
    layer.scale_y = 2.;
    let mut renderer = Renderer::default();
    renderer.set_bounded_shape_preview(true);
    // Displayed 6000x3000 caps to 2048 on the long side.
    let preview = renderer.layer_surface(&layer).unwrap();
    assert_eq!((preview.width(), preview.height()), (2048, 1024));
    let cached = renderer.layer_surface(&layer).unwrap();
    assert_eq!(preview.unique_id(), cached.unique_id());
    // Capped radius reads correctly through the transform: doc (40,40) sits
    // inside the r=100 round (dist 85) but outside a stretched r=200 (dist 226).
    let mut doc = Document::new("Preview", 200, 200).unwrap();
    doc.layers.push(layer.clone());
    let mut pixels = Renderer::default();
    pixels.set_bounded_shape_preview(true);
    assert_eq!(
        pixels.sample(&doc, Point::new(40., 40.)).unwrap(),
        [255, 0, 0, 255]
    );
    // Full renders ignore the preview cache: the stretched source grid draws.
    let mut plain = Renderer::default();
    assert_eq!(plain.layer_surface(&layer).unwrap().width(), 3000);
}

// Local: line width is fixed in document pixels across resizes.
#[test]
fn local_line_width_survives_nonuniform_resize() {
    let mut e = shape_session();
    cmd(&mut e, json!({"type":"setTool","tool":"line"}));
    cmd(&mut e, json!({"type":"setShapeLineWidth","width":6.}));
    drag(&mut e, (10., 40.), (50., 40.), Modifiers::default());
    cmd(&mut e, json!({"type":"updateLayer","patch":{"scaleX":2.0}}));
    let resized = e.selected().unwrap().clone();
    let width = match resized.content.as_ref() {
        Content::Shape {
            shape: Shape::Line,
            line_width: Some(width),
            ..
        } => *width,
        other => panic!("line stayed editable: {other:?}"),
    };
    assert_eq!(width, 6.);
    assert_eq!((resized.scale_x, resized.scale_y), (1., 1.));
    assert_eq!(resized.width, 92);
}

// Local: a 100x100 radius-10 layer at scale 4 previews as 400x400 with radius
// 10 (not 1600x1600), and the committed redraw renders the identical footprint.
#[test]
fn local_preview_matches_commit_footprint_and_radius() {
    let mut layer = Layer::new(
        "Box",
        100,
        100,
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#ff0000".into(),
            corner_radius: 10.,
            line_width: None,
            line_start: None,
            line_end: None,
        },
    );
    layer.x = 50.;
    layer.y = 50.;
    layer.scale_x = 4.;
    layer.scale_y = 4.;
    let mut doc = Document::new("Preview", 500, 500).unwrap();
    doc.layers.push(layer);
    let mut preview_renderer = Renderer::default();
    preview_renderer.set_bounded_shape_preview(true);
    let mut previewed = preview_renderer.render(&doc).unwrap();
    let preview_pixels = picsie_core::render::rgba_pixels(&previewed.image_snapshot()).unwrap();
    let at = |pixels: &[u8], x: u32, y: u32| pixels[(y as usize * 500 + x as usize) * 4 + 3];
    // Inside the r=10 round (dist 8.5), outside a stretched r=40 (dist 51).
    assert_eq!(at(&preview_pixels, 54, 54), 255);
    // Displayed footprint edges, not the double-scaled ones.
    assert_eq!(at(&preview_pixels, 449, 250), 255);
    assert_eq!(at(&preview_pixels, 451, 250), 0);
    // Committing redraws full and renders the same visible pixels.
    let mut e = editor(500, 500, doc.layers.clone());
    e.select(Some(doc.layers[0].id.clone()), SelectionMode::Replace)
        .unwrap();
    cmd(&mut e, json!({"type":"beginTransform"}));
    cmd(&mut e, json!({"type":"commitTransform"}));
    let committed = e.selected().unwrap().clone();
    assert_eq!((committed.width, committed.height), (400, 400));
    assert_eq!((committed.scale_x, committed.scale_y), (1., 1.));
    let mut done = Renderer::default().render(&e.history.document).unwrap();
    assert_eq!(
        picsie_core::render::rgba_pixels(&done.image_snapshot()).unwrap(),
        preview_pixels
    );
}

// Local: a scaled preview keeps own-mask coverage (linked and unlinked) and
// matches the committed footprint.
#[test]
fn local_masked_preview_matches_committed_coverage() {
    for unlinked in [false, true] {
        let mut e = shape_session();
        cmd(&mut e, json!({"type":"setShapeCornerRadius","radius":8.}));
        drag(&mut e, (20., 10.), (60., 40.), Modifiers::default());
        let id = e.selected_id().unwrap().to_owned();
        cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
        {
            let layer = e.selected().unwrap().clone();
            let mut mask = layer.mask.as_ref().unwrap().as_ref().clone();
            let mut pixels = vec![255u8; 40 * 30];
            for y in 0..30 {
                for x in 0..20 {
                    pixels[y * 40 + x] = 0;
                }
            }
            mask.raster = Some(Arc::new(MaskRaster {
                width: 40,
                height: 30,
                pixels: Arc::new(pixels),
            }));
            if unlinked {
                mask.linked = false;
                mask.placement = Some(MaskPlacement {
                    sampling: Sampling::High,
                    x: 20.,
                    y: 10.,
                    scale_x: 1.,
                    scale_y: 1.,
                    rotation: 0.,
                    flip_x: false,
                    flip_y: false,
                });
            }
            let mut doc = e.history.document.clone();
            doc.layers.iter_mut().find(|l| l.id == id).unwrap().mask = Some(Arc::new(mask));
            e.edit("Mask half", doc, None).unwrap();
        }
        // Draft-like scaled layer straight into the preview pipeline.
        let mut draft = e.history.document.clone();
        let shape = draft.layers.iter_mut().find(|l| l.id == id).unwrap();
        shape.scale_x = 2.;
        let mut preview_renderer = Renderer::default();
        preview_renderer.set_bounded_shape_preview(true);
        let mut previewed = preview_renderer.render(&draft).unwrap();
        let preview_pixels = picsie_core::render::rgba_pixels(&previewed.image_snapshot()).unwrap();
        let at = |pixels: &[u8], x: u32, y: u32| pixels[(y as usize * 100 + x as usize) * 4 + 3];
        // Masked left half stays hidden in the preview; right half shows.
        assert_eq!(at(&preview_pixels, 22, 20), 0, "unlinked={unlinked}");
        assert_eq!(at(&preview_pixels, 96, 14), 255, "unlinked={unlinked}");
        // Commit and compare the overlap byte-for-byte.
        cmd(&mut e, json!({"type":"setPaintTarget","target":"content"}));
        cmd(&mut e, json!({"type":"updateLayer","patch":{"scaleX":2.0}}));
        assert_eq!(overlap_pixels(&e.history.document, 0, 10, 60, 30), {
            let mut expected = vec![0u8; 60 * 30 * 4];
            for (i, pixel) in preview_pixels
                .chunks_exact(100 * 4)
                .skip(10)
                .take(30)
                .flat_map(|row| row[..60 * 4].to_vec())
                .enumerate()
            {
                expected[i] = pixel;
            }
            expected
        });
    }
}

// Local: painting, gradients and fills bake a shape to plain pixels.
#[test]
fn local_pixel_edits_remove_shape_editability() {
    let mut e = shape_session();
    drag(&mut e, (10., 10.), (50., 40.), Modifiers::default());
    cmd(&mut e, json!({"type":"setTool","tool":"brush"}));
    cmd(&mut e, json!({"type":"setBrush","size":6,"opacity":1}));
    pointer(&mut e, Phase::Down, 25., 20.);
    pointer(&mut e, Phase::Up, 25., 20.);
    assert!(matches!(
        e.selected().unwrap().content.as_ref(),
        Content::Image { .. }
    ));
    cmd(&mut e, json!({"type":"undo"}));
    assert!(matches!(
        e.selected().unwrap().content.as_ref(),
        Content::Shape { .. }
    ));
    // A gradient commit bakes the same way.
    cmd(&mut e, json!({"type":"setTool","tool":"gradient"}));
    gradient_drag(&mut e, (10., 10.), (50., 10.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    assert!(matches!(
        e.selected().unwrap().content.as_ref(),
        Content::Image { .. }
    ));
    cmd(&mut e, json!({"type":"undo"}));
    // A selection fill bakes as well.
    cmd(&mut e, json!({"type":"setTool","tool":"marquee"}));
    pointer(&mut e, Phase::Down, 15., 15.);
    pointer(&mut e, Phase::Up, 35., 25.);
    cmd(&mut e, json!({"type":"fillSelection"}));
    assert!(matches!(
        e.selected().unwrap().content.as_ref(),
        Content::Image { .. }
    ));
}

// Local: the default gradient style is foreground-to-transparent, composited
// over opaque old pixels through the shader path.
#[test]
fn local_default_transparent_gradient_over_opaque_pixels() {
    let mut e = editor(101, 4, vec![paint_layer("Layer 1", 101, 4)]);
    assert_eq!(
        e.gradient_settings.style,
        picsie_core::gradient::GradientStyle::ForegroundToTransparent
    );
    cmd(&mut e, json!({"type":"setTool","tool":"gradient"}));
    cmd(&mut e, json!({"type":"setColor","color":"#000000"}));
    // Opaque red base first.
    cmd(&mut e, json!({"type":"setColor","color":"#ff0000"}));
    cmd(
        &mut e,
        json!({"type":"setGradientStyle","style":"foreground-to-background"}),
    );
    gradient_drag(&mut e, (100.5, 2.), (101., 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    assert_eq!(pix(&e.history.document, 50., 2.), [255, 0, 0, 255]);
    // Half black over opaque red through the default transparent style.
    cmd(&mut e, json!({"type":"setColor","color":"#000000"}));
    cmd(
        &mut e,
        json!({"type":"setGradientStyle","style":"foreground-to-transparent"}),
    );
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    let middle = pix(&e.history.document, 50., 2.);
    assert!(
        near(middle[0], 128, 3) && middle[1] == 0 && middle[3] == 255,
        "{middle:?}"
    );
    assert_eq!(pix(&e.history.document, 100., 2.), [255, 0, 0, 255]);
}

// Local: a gradient honors an independently placed mask grid.
#[test]
fn local_gradient_maps_through_transformed_mask_placement() {
    let mut e = gradient_session(100, 4);
    gradient_drag(&mut e, (0., 2.), (0.6, 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    cmd(&mut e, json!({"type":"toggleMaskLink"}));
    cmd(&mut e, json!({"type":"setPaintTarget","target":"mask"}));
    // A fresh solid mask has no grid to place; materialize coverage first.
    gradient_drag(&mut e, (0., 2.), (0.6, 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    cmd(
        &mut e,
        json!({"type":"setMaskPlacement","placement":{
            "sampling": "High", "x": 50., "y": 0.,
            "scaleX": 1., "scaleY": 1., "rotation": 0.,
            "flipX": false, "flipY": false,
        }}),
    );
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    cmd(&mut e, json!({"type":"commitGradient"}));
    let layer = e.selected().unwrap();
    let placement = layer.mask.as_ref().unwrap().placement.unwrap();
    assert_eq!(placement.x, 50.);
    // The placed grid starts 50 document pixels right, so raster pixel 0 lands
    // on doc 50: the ramp reads t=(x+50)/100 across the visible overlap.
    assert!(near(pix(&e.history.document, 50., 2.)[3], 128, 3));
    assert!(near(pix(&e.history.document, 75., 2.)[3], 191, 4));
}

// Local: adding a mask applies the pending content gradient in one undo.
#[test]
fn local_target_switch_applies_pending_gradient() {
    let mut e = gradient_session(101, 4);
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    let count = e.history.info().undo_count;
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    assert!(e.gradient_edit.is_none());
    // One undo for the applied gradient plus one for the mask itself.
    assert_eq!(e.history.info().undo_count, count + 2);
    assert!(matches!(
        e.selected().unwrap().content.as_ref(),
        Content::Image { .. }
    ));
    // A pending mask line commits when painting returns to content.
    cmd(&mut e, json!({"type":"setPaintTarget","target":"mask"}));
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    let count = e.history.info().undo_count;
    cmd(&mut e, json!({"type":"setPaintTarget","target":"content"}));
    assert!(e.gradient_edit.is_none());
    assert_eq!(e.history.info().undo_count, count + 1);
}

// Local: switching tools applies the pending gradient in one undo.
#[test]
fn local_tool_switch_applies_pending_gradient() {
    let mut e = gradient_session(101, 4);
    gradient_drag(&mut e, (0.5, 2.), (100.5, 2.));
    let count = e.history.info().undo_count;
    cmd(&mut e, json!({"type":"setTool","tool":"brush"}));
    assert!(e.gradient_edit.is_none());
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(pix(&e.history.document, 100., 2.), [255, 255, 255, 255]);
    cmd(&mut e, json!({"type":"undo"}));
    assert!(matches!(
        e.selected().unwrap().content.as_ref(),
        Content::Paint
    ));
}

// Local: dragging an endpoint moves just that end; Shift snaps the line.
#[test]
fn local_gradient_endpoints_drag_and_shift_snaps() {
    let mut e = gradient_session(101, 101);
    gradient_drag(&mut e, (10., 50.), (90., 50.));
    let edit = e.gradient_edit.clone().unwrap();
    // Grab the end handle well within ten screen points.
    pointer(&mut e, Phase::Down, 88., 50.);
    pointer_mod(&mut e, Phase::Move, 88., 80., Modifiers::default());
    pointer(&mut e, Phase::Up, 88., 80.);
    let moved = e.gradient_edit.clone().unwrap();
    assert!((moved.start.x - edit.start.x).abs() < 0.01);
    assert!((moved.end.y - 80.).abs() < 0.01);
    // Shift snaps the moved end to eighths of a turn around the start.
    pointer(&mut e, Phase::Down, 88., 80.);
    pointer_mod(
        &mut e,
        Phase::Move,
        60.,
        70.,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    let snapped = e.gradient_edit.clone().unwrap();
    // The moved end sits on an eighth of a turn around the start.
    let turns = (snapped.end.y - snapped.start.y).atan2(snapped.end.x - snapped.start.x)
        / std::f64::consts::PI
        * 4.;
    assert!((turns - turns.round()).abs() < 1e-9, "{turns}");
    cmd(&mut e, json!({"type":"cancelGradient"}));
}

// Local: old projects without style fields stay readable with defaults.
#[test]
fn local_legacy_shape_projects_keep_fill_only_defaults() {
    let mut e = editor(100, 80, vec![]);
    let doc: Document = serde_json::from_value(json!({
        "format": "picsie", "version": 2, "id": e.history.document.id.clone(),
        "resolution": 72., "name": "Old", "width": 100, "height": 80,
        "layers": [{
            "id": e.history.document.id.clone(), "name": "Rectangle",
            "visible": true, "locked": false, "width": 30, "height": 20,
            "x": 10., "y": 10., "scaleX": 1., "scaleY": 1., "rotation": 0.,
            "flipX": false, "flipY": false, "opacity": 1., "blend": "source-over",
            "sampling": "High", "brightness": 1., "saturation": 1., "blur": 0.,
            "content": {"kind": "shape", "shape": "rectangle", "color": "#ff0000"},
            "strokes": [],
        }],
        "guides": [],
    }))
    .unwrap();
    doc.validate().unwrap();
    assert!(matches!(
        doc.layers[0].content.as_ref(),
        Content::Shape {
            corner_radius,
            line_width: None,
            line_start: None,
            line_end: None,
            ..
        } if *corner_radius == 0.
    ));
    let _ = &mut e;
}

// Local: package round-trip preserves editable shape styles.
#[test]
fn local_comp_round_trip_preserves_editable_shape_styles() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = shape_session();
    cmd(&mut e, json!({"type":"setShapeCornerRadius","radius":8.}));
    drag(&mut e, (10., 10.), (50., 40.), Modifiers::default());
    cmd(&mut e, json!({"type":"setTool","tool":"line"}));
    drag(&mut e, (60., 10.), (90., 40.), Modifiers::default());
    let path = dir.path().join("shapes.comp");
    picsie_core::comp::save(&path, &e.history.document).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    let shapes: Vec<&serde_json::Value> = manifest["layers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l.get("shape").is_some_and(|s| !s.is_null()))
        .collect();
    assert_eq!(shapes.len(), 2);
    assert_eq!(shapes[0]["shape"]["kind"], "Rectangle");
    assert_eq!(shapes[0]["shape"]["cornerRadius"], 8.);
    assert_eq!(shapes[1]["shape"]["kind"], "Line");
    let reopened = picsie_core::comp::open(&path).unwrap();
    assert!(matches!(
        reopened.layers[1].content.as_ref(),
        Content::Shape {
            shape: Shape::Rectangle,
            corner_radius,
            ..
        } if *corner_radius == 8.
    ));
    assert!(matches!(
        reopened.layers[2].content.as_ref(),
        Content::Shape {
            shape: Shape::Line,
            line_width: Some(_),
            line_start: Some(_),
            line_end: Some(_),
            ..
        }
    ));
    let mut before = Renderer::default().render(&e.history.document).unwrap();
    let mut after = Renderer::default().render(&reopened).unwrap();
    assert_eq!(
        picsie_core::render::rgba_pixels(&before.image_snapshot()).unwrap(),
        picsie_core::render::rgba_pixels(&after.image_snapshot()).unwrap()
    );
}

// Local: unknown shape kinds stay explicitly rejected; translucent shapes rasterize.
#[test]
fn local_comp_rejects_unknown_shapes_and_rasterizes_translucent_ones() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = editor(100, 80, vec![paint_layer("Layer 1", 100, 80)]);
    let mut glass = Layer::new(
        "Glass",
        20,
        20,
        Content::shape(Shape::Rectangle, "#ff000080".into()),
    );
    glass.x = 5.;
    glass.y = 5.;
    e.history.document.layers.push(glass);
    e.history.document.validate().unwrap();
    let path = dir.path().join("glass.comp");
    picsie_core::comp::save(&path, &e.history.document).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert!(
        manifest["layers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l.get("shape").is_none_or(|s| s.is_null()))
    );
    let reopened = picsie_core::comp::open(&path).unwrap();
    assert!(matches!(
        reopened.layers[1].content.as_ref(),
        Content::Image { .. }
    ));
    // An unknown future kind fails the import instead of guessing.
    let mut tampered = manifest;
    tampered["layers"][1]["shape"] =
        json!({"kind":"Star","red":1.,"green":0.,"blue":0.,"cornerRadius":0.});
    std::fs::write(
        path.join("manifest.json"),
        serde_json::to_vec(&tampered).unwrap(),
    )
    .unwrap();
    assert!(picsie_core::comp::open(&path).is_err());
}

// Local: the retained prototype gradient layer still round-trips.
#[test]
fn local_prototype_gradient_layer_stays_compatible() {
    let mut e = editor(100, 80, vec![paint_layer("Layer 1", 100, 80)]);
    cmd(&mut e, json!({"type":"addGradient"}));
    assert!(matches!(
        e.selected().unwrap().content.as_ref(),
        Content::Gradient { .. }
    ));
    assert_eq!(pix(&e.history.document, 5., 5.)[3], 255);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gradient.comp");
    picsie_core::comp::save(&path, &e.history.document).unwrap();
    let reopened = picsie_core::comp::open(&path).unwrap();
    assert_eq!(reopened.layers.len(), 2);
    let json_round = serde_json::to_value(&e.history.document).unwrap();
    let restored: Document = serde_json::from_value(json_round).unwrap();
    assert_eq!(
        restored.layers[1].content,
        e.history.document.layers[1].content
    );
    let _ = Arc::new(0);
}
