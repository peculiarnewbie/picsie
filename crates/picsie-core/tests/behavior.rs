//! Behavior fixtures moved from the TypeScript suite. Compositor fixtures are marked below.
//! Upstream MIT © 2026 Wonder Assembly LLC; pinned source mapping in docs/compositor-port.md.
use picsie_core::{
    canvas_size::*, editor::*, files::*, geometry::*, history::*, model::*, render::*,
};
use serde_json::json;
use std::{path::Path, sync::Arc};
fn layer(name: &str) -> Layer {
    Layer::new(
        name,
        80,
        60,
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#ff0000".into(),
        },
    )
}
fn doc(layers: Vec<Layer>) -> Document {
    let mut d = Document::new("Test", 500, 500).unwrap();
    d.layers = layers;
    d
}
fn editor(layers: Vec<Layer>) -> Editor {
    let mut e = Editor::new(doc(layers)).unwrap();
    e.viewport = Viewport {
        width: 500.,
        height: 500.,
        zoom: 1.,
        pan: Point::default(),
    };
    e
}
fn cmd(e: &mut Editor, v: serde_json::Value) {
    e.command(serde_json::from_value(v).unwrap()).unwrap();
}
fn pointer(e: &mut Editor, phase: Phase, x: f64, y: f64) {
    e.pointer(PointerSample {
        phase,
        point: Point::new(x, y),
        modifiers: Modifiers::default(),
    })
    .unwrap();
}
fn pix(d: &Document, x: f64, y: f64) -> [u8; 4] {
    Renderer::default().sample(d, Point::new(x, y)).unwrap()
}
fn near(a: Point, b: Point) {
    assert!(a.distance(b) < 0.00001, "{a:?} != {b:?}");
}
fn moved_layer(name: &str, x: f64) -> Layer {
    let mut l = layer(name);
    l.x = x;
    l.y = 80.;
    l
}
fn undo(e: &mut Editor) {
    cmd(e, json!({"type":"undo"}));
}
fn redo(e: &mut Editor) {
    cmd(e, json!({"type":"redo"}));
}
#[test]
fn validation_rejects_invalid_dimensions_versions_duplicates_assets_and_commands() {
    assert!(Document::new("Test", 0, 20).is_err());
    assert!(Document::new("Test", 8192, 8192).is_err());
    let l = layer("A");
    assert!(doc(vec![l.clone(), l.clone()]).validate().is_err());
    let mut d = doc(vec![l]);
    d.version = 999;
    assert!(parse_project(&serde_json::to_string(&d).unwrap()).is_err());
    d.version = 1;
    d.layers[0].content = Arc::new(Content::Image {
        data: "file:///private/image.png".into(),
    });
    assert!(d.validate().is_err());
    let mut e = editor(vec![layer("A")]);
    let before = e.history.document.clone();
    assert!(
        e.command(Command::UpdateLayer {
            patch: json!({"scaleX":0})
        })
        .is_err()
    );
    assert!(
        e.command(Command::SetBrush {
            size: f64::NAN,
            opacity: 1.
        })
        .is_err()
    );
    assert_eq!(e.history.document, before);
}
#[test]
fn composite_order_alpha_visibility_and_blend_pixels() {
    let red = layer("Red");
    let mut blue = red.clone();
    blue.id = id();
    blue.content = Arc::new(Content::Shape {
        shape: Shape::Rectangle,
        color: "#0000ff".into(),
    });
    blue.opacity = 0.5;
    assert_eq!(
        pix(&doc(vec![red.clone(), blue.clone()]), 10., 10.),
        [127, 0, 128, 255]
    );
    assert_eq!(
        pix(&doc(vec![blue.clone(), red.clone()]), 10., 10.),
        [255, 0, 0, 255]
    );
    assert_eq!(pix(&doc(vec![red.clone()]), 100., 100.), [0; 4]);
    blue.opacity = 1.;
    blue.blend = Blend::Multiply;
    assert_eq!(
        pix(&doc(vec![red.clone(), blue.clone()]), 10., 10.),
        [0, 0, 0, 255]
    );
    blue.blend = Blend::Screen;
    assert_eq!(
        pix(&doc(vec![red.clone(), blue.clone()]), 10., 10.),
        [255, 0, 255, 255]
    );
    blue.visible = false;
    assert_eq!(pix(&doc(vec![blue]), 10., 10.), [0; 4]);
}
#[test]
fn erasing_reveals_only_the_lower_layer() {
    let red = layer("Red");
    let mut blue = red.clone();
    blue.id = id();
    blue.content = Arc::new(Content::Shape {
        shape: Shape::Rectangle,
        color: "#0000ff".into(),
    });
    blue.strokes.push(Arc::new(Stroke {
        mode: StrokeMode::Erase,
        color: "#000000".into(),
        size: 10.,
        opacity: 1.,
        points: vec![Point::new(10., 10.)],
    }));
    assert_eq!(
        pix(&doc(vec![red, blue.clone()]), 10., 10.),
        [255, 0, 0, 255]
    );
    assert_eq!(pix(&doc(vec![blue.clone()]), 10., 10.), [0; 4]);
    assert_eq!(pix(&doc(vec![blue]), 1., 1.), [0, 0, 255, 255]);
}
#[test]
fn filters_affect_pixels_and_blur_extends_bounds() {
    let mut l = layer("Gray");
    l.content = Arc::new(Content::Shape {
        shape: Shape::Rectangle,
        color: "#808080".into(),
    });
    l.brightness = 0.5;
    assert_eq!(pix(&doc(vec![l.clone()]), 10., 10.), [64, 64, 64, 255]);
    l.brightness = 1.;
    l.blur = 3.;
    l.x = 20.;
    l.y = 20.;
    let p = pix(&doc(vec![l]), 18., 30.);
    assert!(p[3] > 0 && p[3] < 255);
}
#[test]
fn inverse_transforms_and_hit_testing_skip_hidden_and_locked() {
    for rotation in [-135., 0., 37., 90.] {
        for flip_x in [false, true] {
            let mut l = layer("A");
            l.x = 30.;
            l.y = 20.;
            l.scale_x = 2.;
            l.scale_y = 0.7;
            l.rotation = rotation;
            l.flip_x = flip_x;
            near(
                to_local(&l, to_world(&l, Point::new(3., 7.))),
                Point::new(3., 7.),
            );
        }
    }
    let a = layer("A");
    let mut b = layer("B");
    let p = Point::new(10., 10.);
    assert_eq!(
        hit_test(&doc(vec![a.clone(), b.clone()]), p).unwrap().id,
        b.id
    );
    b.locked = true;
    assert_eq!(
        hit_test(&doc(vec![a.clone(), b.clone()]), p).unwrap().id,
        a.id
    );
    b.visible = false;
    assert!(hit_test(&doc(vec![b]), p).is_none());
}
#[test]
fn zoom_preserves_anchor() {
    let d = doc(vec![]);
    let v = Viewport {
        width: 400.,
        height: 300.,
        zoom: 0.5,
        pan: Point::new(15., -20.),
    };
    let p = Point::new(135., 74.);
    near(
        to_document(&d, &zoom_at(&d, &v, p, 3.), p),
        to_document(&d, &v, p),
    );
}
#[test]
fn captured_move_is_one_entry_and_noop_click_has_no_history() {
    let l = layer("A");
    let mut e = editor(vec![l.clone()]);
    pointer(&mut e, Phase::Down, 20., 20.);
    pointer(&mut e, Phase::Up, 20., 20.);
    assert!(!e.history.info().can_undo);
    pointer(&mut e, Phase::Down, 20., 20.);
    pointer(&mut e, Phase::Move, 45., 35.);
    pointer(&mut e, Phase::Up, 50., 45.);
    assert_eq!(
        (e.selected().unwrap().x, e.selected().unwrap().y),
        (30., 25.)
    );
    assert_eq!(e.history.info().undo_count, 1);
    undo(&mut e);
    assert_eq!(e.selected().unwrap(), &l);
    redo(&mut e);
    assert_eq!(e.selected().unwrap().x, 30.);
}
#[test]
fn shape_creation_reverse_drag_and_cancellation() {
    let mut e = editor(vec![]);
    cmd(&mut e, json!({"type":"setTool","tool":"ellipse"}));
    pointer(&mut e, Phase::Down, 70., 80.);
    pointer(&mut e, Phase::Up, 20., 30.);
    let l = e.selected().unwrap();
    assert_eq!((l.width, l.height, l.x, l.y), (50, 50, 20., 30.));
    pointer(&mut e, Phase::Down, 5., 5.);
    pointer(&mut e, Phase::Move, 40., 40.);
    pointer(&mut e, Phase::Cancel, 40., 40.);
    assert_eq!(e.history.document.layers.len(), 1);
}
#[test]
fn paint_autocreates_layer_and_records_local_coordinates() {
    let mut e = editor(vec![]);
    cmd(&mut e, json!({"type":"setTool","tool":"brush"}));
    cmd(&mut e, json!({"type":"setColor","color":"#ff0000"}));
    cmd(&mut e, json!({"type":"setBrush","size":6,"opacity":1}));
    pointer(&mut e, Phase::Down, 10., 10.);
    pointer(&mut e, Phase::Move, 20., 10.);
    pointer(&mut e, Phase::Up, 30., 10.);
    assert_eq!(e.selected().unwrap().strokes.len(), 1);
    assert_eq!(pix(&e.history.document, 20., 10.), [255, 0, 0, 255]);
    undo(&mut e);
    assert!(e.history.document.layers.is_empty());
    redo(&mut e);
    assert_eq!(e.history.document.layers.len(), 1);
    let mut l = Layer::new("Paint", 20, 20, Content::Paint);
    l.x = 20.;
    l.y = 20.;
    l.scale_x = 2.;
    l.scale_y = 2.;
    let mut e = editor(vec![l]);
    cmd(&mut e, json!({"type":"setTool","tool":"brush"}));
    pointer(&mut e, Phase::Down, 40., 40.);
    pointer(&mut e, Phase::Up, 40., 40.);
    assert_eq!(
        e.selected().unwrap().strokes[0].points[0],
        Point::new(10., 10.)
    );
}
#[test]
fn locks_refuse_paint_delete_and_property_changes() {
    let mut l = layer("Locked");
    l.locked = true;
    let mut e = editor(vec![l.clone()]);
    cmd(&mut e, json!({"type":"setTool","tool":"brush"}));
    pointer(&mut e, Phase::Down, 10., 10.);
    pointer(&mut e, Phase::Up, 20., 20.);
    cmd(
        &mut e,
        json!({"type":"updateLayer","patch":{"opacity":0.2}}),
    );
    cmd(&mut e, json!({"type":"remove"}));
    assert_eq!(e.history.document.layers, vec![l]);
    cmd(
        &mut e,
        json!({"type":"updateLayer","patch":{"visible":false}}),
    );
    assert!(!e.selected().unwrap().visible);
}
#[test]
fn range_selection_preserves_anchor_and_canvas_shift_toggles() {
    let layers: Vec<_> = (0..4)
        .map(|i| moved_layer(&i.to_string(), 20. + 120. * i as f64))
        .collect();
    let mut e = editor(layers.clone());
    e.select(Some(layers[1].id.clone()), SelectionMode::Replace);
    e.select(Some(layers[3].id.clone()), SelectionMode::Range);
    assert_eq!(e.selection.ids.len(), 3);
    e.select(Some(layers[2].id.clone()), SelectionMode::Range);
    assert_eq!(e.selection.ids.len(), 2);
    e.select(Some(layers[0].id.clone()), SelectionMode::Toggle);
    e.select(Some(layers[1].id.clone()), SelectionMode::Toggle);
    assert_eq!(
        e.selected_layers()
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["0", "2"]
    );
    e.pointer(PointerSample {
        phase: Phase::Down,
        point: Point::new(170., 100.),
        modifiers: Modifiers {
            shift: true,
            alt: false,
        },
    })
    .unwrap();
    assert_eq!(e.selection.ids.len(), 3);
    assert!(!e.history.info().can_undo);
}
#[test]
fn group_moves_preserve_spacing_skip_locks_clamp_and_restore_selection() {
    let a = moved_layer("A", 20.);
    let b = moved_layer("B", 140.);
    let mut l = moved_layer("Locked", 260.);
    l.locked = true;
    let mut e = editor(vec![a.clone(), b.clone(), l.clone()]);
    cmd(&mut e, json!({"type":"selectAll"}));
    pointer(&mut e, Phase::Down, 50., 100.);
    pointer(&mut e, Phase::Move, 65., 115.);
    pointer(&mut e, Phase::Up, 80., 130.);
    assert_eq!(
        e.history
            .document
            .layers
            .iter()
            .map(|l| (l.x, l.y))
            .collect::<Vec<_>>(),
        [(50., 110.), (170., 110.), (260., 80.)]
    );
    assert_eq!(e.selection.ids.len(), 3);
    undo(&mut e);
    assert_eq!(e.history.document.layers, vec![a, b, l]);
    redo(&mut e);
    assert_eq!(e.history.document.layers[1].x, 170.);
    cmd(&mut e, json!({"type":"nudge","delta":{"x":200000,"y":0}}));
    assert_eq!(e.history.document.layers[1].x, 100000.);
    assert_eq!(
        e.history.document.layers[1].x - e.history.document.layers[0].x,
        120.
    );
}
#[test]
fn duplication_is_in_place_ordered_unlocked_and_deletion_skips_locked() {
    let a = moved_layer("A", 20.);
    let mut b = moved_layer("B", 140.);
    b.locked = true;
    let mut e = editor(vec![a.clone(), b.clone()]);
    cmd(&mut e, json!({"type":"selectAll"}));
    cmd(&mut e, json!({"type":"duplicate"}));
    assert_eq!(
        e.history
            .document
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "A copy", "B", "B copy"]
    );
    assert_eq!(
        e.selected_layers().iter().map(|l| l.x).collect::<Vec<_>>(),
        [20., 140.]
    );
    assert!(e.selected_layers().iter().all(|l| !l.locked));
    let copied = e.selection.ids.clone();
    undo(&mut e);
    assert_eq!(e.selection.ids, [a.id.clone(), b.id.clone()]);
    redo(&mut e);
    assert_eq!(e.selection.ids, copied);
    undo(&mut e);
    cmd(&mut e, json!({"type":"selectAll"}));
    cmd(&mut e, json!({"type":"remove"}));
    assert_eq!(e.history.document.layers, [b]);
    undo(&mut e);
    assert_eq!(e.history.document.layers.len(), 2);
}
#[test]
fn reordering_forms_stable_block_and_layer_limit_is_atomic() {
    let layers: Vec<_> = ["A", "B", "C", "D", "E"].iter().map(|n| layer(n)).collect();
    let mut e = editor(layers.clone());
    e.select(Some(layers[1].id.clone()), SelectionMode::Replace);
    e.select(Some(layers[3].id.clone()), SelectionMode::Toggle);
    cmd(
        &mut e,
        json!({"type":"reorderTo","targetId":layers[4].id,"side":"above"}),
    );
    assert_eq!(
        e.history
            .document
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["A", "C", "E", "B", "D"]
    );
    let count = e.history.info().undo_count;
    cmd(&mut e, json!({"type":"reorder","direction":1}));
    assert_eq!(e.history.info().undo_count, count);
    undo(&mut e);
    assert_eq!(e.history.document.layers, layers);
    let mut e = editor((0..100).map(|_| layer("A")).collect());
    let before = e.history.document.clone();
    assert!(e.command(Command::AddPaintLayer).is_err());
    assert!(e.command(Command::Duplicate).is_err());
    assert_eq!(e.history.document, before);
}
// Compositor TransformTests.rotatedResizeKeepsOppositeAnchorAtEveryHandle.
#[test]
fn compositor_all_rotated_handles_preserve_opposite_anchor() {
    let mut l = Layer::new("Transform", 200, 100, Content::Paint);
    l.x = 31.;
    l.y = -19.;
    l.rotation = 37.;
    for (handle, x, y) in HANDLES {
        for preserve in [true, false] {
            for flipped in [true, false] {
                l.flip_x = flipped;
                l.flip_y = flipped;
                let start = bounds_point(&l, Point::new(x, y));
                let n = resize(
                    &l,
                    handle,
                    Point::new(start.x + 34., start.y + 17.),
                    preserve,
                    false,
                );
                near(
                    bounds_point(&l, Point::new(1. - x, 1. - y)),
                    bounds_point(&n, Point::new(1. - x, 1. - y)),
                );
                if preserve {
                    assert!(
                        (n.width as f64 * n.scale_x / (n.height as f64 * n.scale_y) - 2.).abs()
                            < 0.00001
                    );
                }
            }
        }
    }
}
#[test]
fn compositor_projection_center_resize_mirroring_and_rotation() {
    let l = Layer::new("Transform", 100, 50, Content::Paint);
    let r = resize(&l, "se", Point::new(150., 80.), true, false);
    assert_eq!(
        (r.width as f64 * r.scale_x, r.height as f64 * r.scale_y),
        (152., 76.)
    );
    let r = resize(&l, "e", Point::new(125., 25.), false, true);
    near(center(&r), center(&l));
    assert_eq!(r.width as f64 * r.scale_x, 150.);
    let r = resize(&l, "se", Point::new(-50., -25.), false, false);
    assert!(r.flip_x && r.flip_y);
    near(
        bounds_point(&r, Point::new(1., 1.)),
        bounds_point(&l, Point::default()),
    );
    let r = rotate(&l, Point::new(100., 25.), Point::new(50., 75.), false);
    assert_eq!(r.rotation, 90.);
    near(center(&r), center(&l));
    let mut l = l;
    l.rotation = -7.5;
    assert_eq!(
        rotate(&l, Point::new(100., 25.), Point::new(100., 25.), true).rotation,
        -15.
    );
    let mut flipped = l.clone();
    flipped.flip_x = true;
    assert_eq!(handles(&flipped, 1.), handles(&l, 1.));
}
#[test]
fn pointer_rotation_and_resize_cancel_restores_original() {
    let l = layer("A");
    let mut e = editor(vec![l.clone()]);
    let rotation = handles(&l, 1.)[8].1;
    pointer(&mut e, Phase::Down, rotation.x, rotation.y);
    pointer(&mut e, Phase::Move, 100., 30.);
    assert_ne!(e.selected().unwrap().rotation, 0.);
    pointer(&mut e, Phase::Cancel, 100., 30.);
    assert_eq!(e.selected().unwrap(), &l);
    pointer(&mut e, Phase::Down, 80., 60.);
    pointer(&mut e, Phase::Up, 120., 90.);
    assert_eq!(e.selected().unwrap().scale_x, 1.5);
    assert_eq!(e.selected().unwrap().scale_y, 1.5);
}
// Compositor LayerTests.blankLayersAreTransparentAndInsertedAboveSelection.
#[test]
fn compositor_insertion_and_duplication_preserve_assets_and_transforms() {
    let originals: Vec<_> = ["1", "2", "3"].iter().map(|n| layer(n)).collect();
    let mut e = editor(originals.clone());
    e.select(Some(originals[0].id.clone()), SelectionMode::Replace);
    e.add_layers(vec![layer("4")]).unwrap();
    assert_eq!(
        e.history
            .document
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["1", "4", "2", "3"]
    );
    undo(&mut e);
    e.add_layers(vec![layer("Import 1"), layer("Import 2")])
        .unwrap();
    assert_eq!(e.history.document.layers[2].name, "Import 2");
    undo(&mut e);
    cmd(&mut e, json!({"type":"setTool","tool":"rectangle"}));
    pointer(&mut e, Phase::Down, 10., 10.);
    pointer(&mut e, Phase::Up, 30., 30.);
    assert_eq!(e.history.document.layers[1].name, "Rectangle");
    let source = e.selected().unwrap().clone();
    cmd(&mut e, json!({"type":"duplicate"}));
    let copy = e.selected().unwrap();
    assert_ne!(copy.id, source.id);
    assert!(Arc::ptr_eq(&copy.content, &source.content));
    assert_eq!((copy.x, copy.y), (source.x, source.y));
}
#[test]
fn masks_hide_reveal_disable_remove_and_opacity_preserve_original_pixels() {
    let mut e = editor(vec![layer("A")]);
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    cmd(&mut e, json!({"type":"setBrush","size":30,"opacity":1}));
    pointer(&mut e, Phase::Down, 40., 30.);
    pointer(&mut e, Phase::Up, 40., 30.);
    assert_eq!(pix(&e.history.document, 40., 30.), [0; 4]);
    assert!(e.selected().unwrap().strokes.is_empty());
    cmd(&mut e, json!({"type":"setMaskMode","mode":"reveal"}));
    pointer(&mut e, Phase::Down, 40., 30.);
    pointer(&mut e, Phase::Up, 40., 30.);
    assert_eq!(pix(&e.history.document, 40., 30.), [255, 0, 0, 255]);
    undo(&mut e);
    assert_eq!(pix(&e.history.document, 40., 30.), [0; 4]);
    cmd(
        &mut e,
        json!({"type":"updateLayer","patch":{"mask":{"enabled":false}}}),
    );
    assert_eq!(pix(&e.history.document, 40., 30.), [255, 0, 0, 255]);
    cmd(&mut e, json!({"type":"removeMask"}));
    assert!(e.selected().unwrap().mask.is_none());
    assert_eq!(pix(&e.history.document, 40., 30.), [255, 0, 0, 255]);
    let mut e = editor(vec![layer("A")]);
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    cmd(&mut e, json!({"type":"setBrush","size":30,"opacity":0.5}));
    pointer(&mut e, Phase::Down, 40., 30.);
    pointer(&mut e, Phase::Up, 40., 30.);
    assert_eq!(pix(&e.history.document, 40., 30.), [255, 0, 0, 127]);
}
#[test]
fn mask_reset_eraser_cancel_locks_groups_and_extreme_coordinates() {
    let mut e = editor(vec![layer("A")]);
    cmd(&mut e, json!({"type":"addMask","base":"hide"}));
    cmd(&mut e, json!({"type":"setTool","tool":"eraser"}));
    pointer(&mut e, Phase::Down, 40., 30.);
    pointer(&mut e, Phase::Up, 40., 30.);
    assert_eq!(pix(&e.history.document, 40., 30.), [255, 0, 0, 255]);
    cmd(&mut e, json!({"type":"resetMask","base":"hide"}));
    pointer(&mut e, Phase::Down, 40., 30.);
    pointer(&mut e, Phase::Cancel, 40., 30.);
    assert_eq!(pix(&e.history.document, 40., 30.), [0; 4]);
    cmd(
        &mut e,
        json!({"type":"updateLayer","patch":{"locked":true}}),
    );
    let before = e.history.document.clone();
    cmd(&mut e, json!({"type":"resetMask","base":"reveal"}));
    cmd(&mut e, json!({"type":"removeMask"}));
    assert_eq!(e.history.document, before);
    for mask in [true, false] {
        let mut l = layer("Tiny");
        l.x = -100000.;
        l.y = -100000.;
        l.scale_x = 0.01;
        l.scale_y = 0.01;
        let mut e = editor(vec![l]);
        if mask {
            cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
        } else {
            cmd(&mut e, json!({"type":"setTool","tool":"brush"}));
        }
        pointer(&mut e, Phase::Down, 100., 100.);
        pointer(&mut e, Phase::Up, 100., 100.);
        e.history.document.validate().unwrap();
    }
    let mut e = editor(vec![layer("A"), layer("B")]);
    cmd(&mut e, json!({"type":"selectAll"}));
    let before = e.history.document.clone();
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    cmd(&mut e, json!({"type":"setTool","tool":"brush"}));
    pointer(&mut e, Phase::Down, 20., 20.);
    pointer(&mut e, Phase::Up, 30., 30.);
    assert_eq!(e.history.document, before);
}
#[test]
fn mask_duplicate_edits_are_independent_and_roundtrip() {
    let mut e = editor(vec![layer("A")]);
    cmd(&mut e, json!({"type":"addMask","base":"reveal"}));
    pointer(&mut e, Phase::Down, 40., 30.);
    pointer(&mut e, Phase::Up, 40., 30.);
    let original = e.selected().unwrap().clone();
    cmd(&mut e, json!({"type":"duplicate"}));
    assert!(Arc::ptr_eq(
        e.selected().unwrap().mask.as_ref().unwrap(),
        original.mask.as_ref().unwrap()
    ));
    cmd(&mut e, json!({"type":"resetMask","base":"reveal"}));
    assert_eq!(e.history.document.layers[0].mask, original.mask);
    assert_ne!(e.selected().unwrap().mask, original.mask);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mask.picsie");
    save_project(&path, &e.history.document).unwrap();
    let reopened = open_project(&path).unwrap();
    assert_eq!(reopened, e.history.document);
    let mut r = Renderer::default();
    assert_eq!(
        r.export(&reopened, false).unwrap(),
        r.export(&e.history.document, false).unwrap()
    );
}
// Compositor CanvasSizeTests: floor assigns +/- odd spare pixels consistently.
#[test]
fn compositor_canvas_all_anchors_preserve_assets_masks_and_transforms() {
    let mut l = layer("A");
    l.rotation = 37.;
    l.flip_x = true;
    l.locked = true;
    l.scale_x = 1.25;
    l.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Hide,
        strokes: vec![],
    }));
    let d = doc(vec![l.clone()]);
    for anchor in 0..9 {
        for delta in [-5, 5] {
            let n = resize_canvas(
                &d,
                &CanvasSizeOptions {
                    width: (500 + delta) as u32,
                    height: (500 + delta) as u32,
                    anchor,
                    fill: None,
                },
            )
            .unwrap();
            let next = &n.layers[0];
            assert_eq!(next.x, (delta as f64 * (anchor % 3) as f64 / 2.).floor());
            assert_eq!(next.y, (delta as f64 * (anchor / 3) as f64 / 2.).floor());
            assert!(Arc::ptr_eq(&l.content, &next.content));
            assert!(Arc::ptr_eq(
                l.mask.as_ref().unwrap(),
                next.mask.as_ref().unwrap()
            ));
            assert_eq!(next.rotation, l.rotation);
            assert_eq!(next.scale_x, l.scale_x);
        }
    }
}
#[test]
fn compositor_canvas_colored_extension_leaves_old_holes_and_shrink_adds_no_fill() {
    let d = Document::new("Transparent", 4, 4).unwrap();
    let n = resize_canvas(
        &d,
        &CanvasSizeOptions {
            width: 8,
            height: 2,
            anchor: 4,
            fill: Some("#ff0000".into()),
        },
    )
    .unwrap();
    assert_eq!(n.layers.len(), 1);
    assert_eq!(pix(&n, 0., 0.), [255, 0, 0, 255]);
    assert_eq!(pix(&n, 3., 0.), [0; 4]);
    assert_eq!(pix(&n, 7., 0.), [255, 0, 0, 255]);
    let shrink = resize_canvas(
        &d,
        &CanvasSizeOptions {
            width: 2,
            height: 2,
            anchor: 4,
            fill: Some("#ff0000".into()),
        },
    )
    .unwrap();
    assert!(shrink.layers.is_empty());
    let expand = resize_canvas(
        &d,
        &CanvasSizeOptions {
            width: 8,
            height: 8,
            anchor: 4,
            fill: None,
        },
    )
    .unwrap();
    assert!(expand.layers.is_empty());
}
#[test]
fn canvas_size_undo_selection_atomic_validation_and_recoverable_crop() {
    let mut e = editor(vec![layer("A")]);
    let old = e.history.document.clone();
    let selected = e.selection.clone();
    cmd(
        &mut e,
        json!({"type":"resizeCanvas","options":{"width":600,"height":600,"anchor":4,"fill":"#ffffff"}}),
    );
    assert_eq!(e.history.document.layers.len(), 2);
    assert_eq!(e.selection, selected);
    assert_eq!(e.history.info().undo_label, "Canvas Size");
    undo(&mut e);
    assert_eq!(e.history.document, old);
    assert!(
        e.command(Command::ResizeCanvas {
            options: CanvasSizeOptions {
                width: 8192,
                height: 8192,
                anchor: 4,
                fill: None
            }
        })
        .is_err()
    );
    assert_eq!(e.history.document, old);
    cmd(
        &mut e,
        json!({"type":"resizeCanvas","options":{"width":10,"height":10,"anchor":0}}),
    );
    cmd(
        &mut e,
        json!({"type":"resizeCanvas","options":{"width":500,"height":500,"anchor":0}}),
    );
    assert_eq!(pix(&e.history.document, 70., 50.), [255, 0, 0, 255]);
}
#[test]
fn history_tracks_saved_revision_noops_redo_nested_transactions_and_selection() {
    let mut e = editor(vec![layer("A")]);
    let original = e.history.document.clone();
    let selection = e.selection.clone();
    let saved = e.history.revision.clone();
    cmd(
        &mut e,
        json!({"type":"updateLayer","patch":{"name":"Renamed"}}),
    );
    assert!(e.history.info().dirty);
    undo(&mut e);
    assert!(!e.history.info().dirty);
    cmd(&mut e, json!({"type":"zoom","zoom":2}));
    e.edit("Equivalent", e.history.document.clone(), None)
        .unwrap();
    assert!(e.history.info().can_redo);
    assert!(!e.history.info().dirty);
    redo(&mut e);
    e.history.mark_saved(saved);
    assert!(e.history.info().dirty);
    undo(&mut e);
    e.begin_edit("Layer Setup");
    cmd(&mut e, json!({"type":"addPaintLayer"}));
    cmd(&mut e, json!({"type":"addPaintLayer"}));
    assert!(!e.history.info().can_undo);
    undo(&mut e);
    assert_eq!(e.history.document.layers.len(), 3);
    e.end_edit();
    assert_eq!(e.history.info().undo_label, "Layer Setup");
    assert!(!e.history.info().can_redo);
    undo(&mut e);
    assert_eq!(e.history.document, original);
    assert_eq!(e.selection, selection);
    redo(&mut e);
    assert_eq!(e.history.document.layers.len(), 3);
}
#[test]
fn history_unique_asset_budgets_trim_past_and_future() {
    let mut s = surface(64, 32).unwrap();
    let source = Layer::new("Image", 64, 32, png_content(&s.image_snapshot()).unwrap());
    let mut h = History::new(doc(vec![source.clone()]));
    h.entry_limit = 2;
    h.byte_limit = 0;
    for name in ["A", "B", "C"] {
        h.begin("Rename", Selection::default());
        let mut d = h.document.clone();
        d.layers[0].name = name.into();
        h.preview(d);
        h.commit(Selection::default());
    }
    assert_eq!(h.info().undo_count, 2);
    assert_eq!(h.retained_bytes(), 0);
    h.begin("Delete", Selection::default());
    h.preview(doc(vec![]));
    h.commit(Selection::default());
    assert_eq!(h.info().undo_count, 0);
    let mut h = History::new(doc(vec![]));
    h.byte_limit = 0;
    h.begin("Import", Selection::default());
    h.preview(doc(vec![source]));
    h.commit(Selection::default());
    h.undo();
    assert!(!h.info().can_redo);
}
#[test]
fn canceled_gestures_preserve_redo_and_group_range_anchor() {
    let layers = vec![
        moved_layer("A", 20.),
        moved_layer("B", 140.),
        moved_layer("C", 260.),
    ];
    let mut e = editor(layers.clone());
    e.select(Some(layers[0].id.clone()), SelectionMode::Replace);
    e.select(Some(layers[1].id.clone()), SelectionMode::Toggle);
    let before = e.selection.clone();
    cmd(&mut e, json!({"type":"duplicate"}));
    e.select(Some(layers[2].id.clone()), SelectionMode::Replace);
    undo(&mut e);
    assert_eq!(e.selection, before);
    pointer(&mut e, Phase::Down, 50., 100.);
    pointer(&mut e, Phase::Move, 60., 110.);
    pointer(&mut e, Phase::Cancel, 60., 110.);
    assert!(e.history.info().can_redo);
    assert_eq!(e.selection, before);
    e.select(Some(layers[2].id.clone()), SelectionMode::Range);
    assert_eq!(
        e.selection.ids,
        [layers[1].id.clone(), layers[2].id.clone()]
    );
}
#[test]
fn file_import_is_self_contained_and_failed_writes_preserve_original() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("source.png");
    let project = dir.path().join("project.picsie");
    std::fs::write(
        &image,
        Renderer::default()
            .export(&doc(vec![layer("Red")]), false)
            .unwrap(),
    )
    .unwrap();
    let imported = import_image(&image).unwrap();
    let d = doc(vec![imported]);
    save_project(&project, &d).unwrap();
    std::fs::remove_file(&image).unwrap();
    let opened = open_project(&project).unwrap();
    assert_eq!(opened.format, "picsie");
    assert_eq!(pix(&opened, 10., 10.), [255, 0, 0, 255]);
    let original = std::fs::read(&project).unwrap();
    let mut invalid = d;
    invalid.width = 0;
    assert!(save_project(&project, &invalid).is_err());
    assert_eq!(std::fs::read(&project).unwrap(), original);
    let svg = dir.path().join("image.svg");
    std::fs::write(&svg, b"<svg/>").unwrap();
    assert!(import_image(&svg).is_err());
}
#[test]
fn export_png_alpha_jpeg_white_and_preview_isolation() {
    let d = doc(vec![layer("Red")]);
    let mut r = Renderer::default();
    let before = r.export(&d, false).unwrap();
    let png = decode(&before).unwrap();
    let jpeg = decode(&r.export(&d, true).unwrap()).unwrap();
    let read = |image: &skia_safe::Image| {
        let mut p = [0u8; 4];
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Unpremul,
            None,
        );
        assert!(image.read_pixels(
            &info,
            &mut p,
            4,
            (400, 400),
            skia_safe::image::CachingHint::Allow
        ));
        p
    };
    assert_eq!(read(&png), [0; 4]);
    assert_eq!(read(&jpeg), [255; 4]);
    let mut preview = r
        .preview(
            &d,
            &Viewport {
                width: 300.,
                height: 200.,
                zoom: 1.,
                pan: Point::default(),
            },
            &[d.layers[0].id.clone()],
            true,
            None,
        )
        .unwrap();
    let bytes = frame_bytes(&mut preview).unwrap();
    assert_eq!(&bytes[..4], b"II\x2a\x00");
    assert!(bytes.len() > 300 * 200 * 4);
    let mut decoded = tiff::decoder::Decoder::new(std::io::Cursor::new(&bytes)).unwrap();
    assert_eq!(decoded.dimensions().unwrap(), (300, 200));
    assert_eq!(
        decoded.get_tag_u32(tiff::tags::Tag::Compression).unwrap(),
        1
    );
    assert_eq!(r.export(&d, false).unwrap(), before);
}
#[test]
fn real_legacy_projects_open_and_roundtrip_with_no_pixel_payload_in_metadata() {
    for name in ["legacy-editing", "legacy-native"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../tests/fixtures/{name}.electropic"));
        let d = open_project(&path).unwrap();
        assert_eq!(d.format, "electropic");
        let mut r = Renderer::default();
        let bytes = r.export(&d, false).unwrap();
        assert!(bytes.len() > 1000);
        let encoded = serde_json::to_string(&d).unwrap();
        let reopened = parse_project(&encoded).unwrap();
        assert_eq!(r.export(&reopened, false).unwrap(), bytes);
        let state = Editor::new(d).unwrap().snapshot().to_string();
        assert!(
            !state.contains("data:image")
                && !state.contains("\"strokes\"")
                && !state.contains("\"points\"")
        );
    }
}
