//! TransformGroup port: one transform box for a folder or several selected layers.
//! Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
//!
//! Source-citation honesty: the pinned `CompositorTests/TransformTests.swift` and
//! `GroupTests.swift` contain NO multi-layer transform fixtures, so the
//! `translated_*` tests below are checks derived from the pinned algorithms —
//! `LayerTransform.following`/`placing` and `mirrored` in
//! `Compositor/Document/LayerTransform.swift` and `LayerFlip.swift`, member rules
//! in `EditorSession.swift` (`transformsAsGroup`, `groupTransformMembers`,
//! `groupTransformBox`) — not executions of upstream fixtures. Each such test names
//! the source function it exercises. Everything else is a labeled local regression
//! through the public command API.
use picsie_core::{
    editor::*,
    files::{open_project, save_project},
    geometry::{self, Viewport},
    model::*,
    render::{self, Renderer},
};
use std::sync::Arc;

fn image(name: &str, w: u32, h: u32) -> Layer {
    let bytes = [200, 40, 30, 255].repeat((w * h) as usize);
    Layer::new(
        name,
        w,
        h,
        render::native_content(render::rgba_image(w, h, &bytes).unwrap()),
    )
}

fn folder(name: &str, doc: &Document) -> Layer {
    let mut folder = Layer::new(name, doc.width, doc.height, Content::Group);
    folder.x = 0.;
    folder.y = 0.;
    folder
}

fn mask_linked() -> Arc<LayerMask> {
    Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        raster: Some(Arc::new(MaskRaster {
            width: 4,
            height: 4,
            pixels: Arc::new(vec![255; 16]),
        })),
        linked: true,
        placement: None,
        strokes: vec![],
    })
}

fn editor_with(layers: Vec<Layer>) -> Editor {
    let mut doc = Document::new("Group", 400, 300).unwrap();
    doc.layers = layers;
    let mut e = Editor::new(doc).unwrap();
    e.viewport = Viewport {
        width: 400.,
        height: 300.,
        zoom: 1.,
        pan: Point::default(),
    };
    e.tool = Tool::Move;
    e
}

fn two_layers() -> (Layer, Layer) {
    let mut a = image("A", 40, 20);
    a.x = 10.;
    a.y = 10.;
    let mut b = image("B", 20, 40);
    b.x = 100.;
    b.y = 40.;
    (a, b)
}

fn place(e: &mut Editor, layer: &Layer) {
    let mut doc = e.history.document.clone();
    doc.replace(layer.clone());
    e.edit("test setup", doc, None).unwrap();
}

fn by_name<'a>(e: &'a Editor, name: &str) -> &'a Layer {
    e.history
        .document
        .layers
        .iter()
        .find(|l| l.name == name)
        .unwrap()
}

fn center(l: &Layer) -> Point {
    geometry::center(l)
}

fn select_all(e: &mut Editor, ids: &[String]) {
    e.select(Some(ids[0].clone()), SelectionMode::Replace)
        .unwrap();
    for id in &ids[1..] {
        e.select(Some(id.clone()), SelectionMode::Toggle).unwrap();
    }
}

fn pointer(e: &mut Editor, phase: Phase, x: f64, y: f64, modifiers: Modifiers) {
    e.command(Command::Pointer {
        samples: vec![PointerSample {
            phase,
            point: Point::new(x, y),
            modifiers,
        }],
    })
    .unwrap();
}

fn box_of(e: &Editor) -> Layer {
    e.edited_group_box().expect("group box")
}

fn displayed(l: &Layer) -> (f64, f64) {
    (l.width as f64 * l.scale_x, l.height as f64 * l.scale_y)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

// Translated `LayerTransform.following`: a plain box move carries members exactly,
// including rotated and flipped children.
#[test]
fn translated_plain_move_carries_members_exactly() {
    let (mut a, mut b) = two_layers();
    a.rotation = 30.;
    b.flip_x = true;
    b.flip_y = true;
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    // The rotated box starts left/above A's origin; move it by (+20, +15).
    let start = box_of(&e);
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: start.x + 20.,
    })
    .unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::Y,
        value: start.y + 15.,
    })
    .unwrap();
    // Box origin moved by (+20, +15): every member shifts by the same delta.
    let moved_a = by_name(&e, "A").clone();
    let moved_b = by_name(&e, "B").clone();
    assert!(near(moved_a.x - a.x, 20.) && near(moved_a.y - a.y, 15.));
    assert!(near(moved_b.x - b.x, 20.) && near(moved_b.y - b.y, 15.));
    assert_eq!(moved_a.rotation, 30.);
    assert!(moved_b.flip_x && moved_b.flip_y);
    assert_eq!(displayed(&moved_a), displayed(&a));
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
    assert_eq!(e.history.info().undo_label, "Transform Layers");
}

// Translated `placing`: growing the box about its left edge scales member offsets
// and displayed sizes proportionally from their original placements.
#[test]
fn translated_resize_keeps_member_box_fractions() {
    let (a, b) = two_layers();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    // Box is (10, 10, 110, 70); doubling W keeps x and scales offsets by two.
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::Width,
        value: 220.,
    })
    .unwrap();
    let grown = box_of(&e);
    assert!(near(grown.x, 10.) && near(displayed(&grown).0, 220.));
    let moved_b = by_name(&e, "B").clone();
    assert!(near(moved_b.x, 190.) && near(moved_b.y, 40.));
    assert!(near(displayed(&moved_b).0, 40.));
    // A width-only edit leaves heights alone without the ratio lock.
    assert!(near(displayed(&moved_b).1, 40.));
    let moved_a = by_name(&e, "A").clone();
    assert!(near(moved_a.x, 10.));
    e.command(Command::CancelTransform).unwrap();
    assert_eq!(by_name(&e, "A").x, a.x);
    assert_eq!(by_name(&e, "B").x, b.x);
    assert_eq!(e.history.info().undo_count, 0);
}

// Translated `LayerFlip.mirrored` via `flipLayers`: members mirror across the box
// middle, toggling the axis flip and negating rotation.
#[test]
fn translated_flip_mirrors_members_across_box_middle() {
    let (mut a, b) = two_layers();
    a.rotation = 30.;
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    // Mirror across the measured box middle (A's rotation widens the box).
    let axis = geometry::center(&box_of(&e)).x;
    e.command(Command::UpdateLayer {
        patch: serde_json::json!({"flipX": true}),
    })
    .unwrap();
    let flipped_a = by_name(&e, "A").clone();
    let flipped_b = by_name(&e, "B").clone();
    assert!(near(center(&flipped_a).x, 2. * axis - center(&a).x));
    assert!(near(center(&flipped_b).x, 2. * axis - center(&b).x));
    assert!(flipped_a.flip_x);
    assert!(flipped_b.flip_x);
    assert!(near(flipped_a.rotation, -30.));
    assert_eq!(e.history.info().undo_count, 1);
    assert_eq!(e.history.info().undo_label, "Flip Horizontal");
    e.command(Command::Undo).unwrap();
    assert_eq!(by_name(&e, "A").x, a.x);
}

// Translated member rules (`groupTransformMembers`): nested folders contribute their
// descendants, and selecting a parent together with its child transforms it once.
#[test]
fn translated_members_cover_nested_selection_once() {
    let doc = Document::new("Group", 400, 300).unwrap();
    let outer = folder("Outer", &doc);
    let mut inner = folder("Inner", &doc);
    inner.parent_id = Some(outer.id.clone());
    let mut a = image("A", 40, 20);
    a.parent_id = Some(inner.id.clone());
    a.x = 10.;
    a.y = 10.;
    let mut b = image("B", 20, 40);
    b.parent_id = Some(outer.id.clone());
    b.x = 100.;
    b.y = 40.;
    let mut e = editor_with(vec![outer.clone(), inner.clone(), a.clone(), b.clone()]);
    // Folder alone reaches through the nested folder.
    e.select(Some(outer.id.clone()), SelectionMode::Replace)
        .unwrap();
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: 20.,
    })
    .unwrap();
    assert!(near(by_name(&e, "A").x - a.x, 10.));
    assert!(near(by_name(&e, "B").x - b.x, 10.));
    e.command(Command::CancelTransform).unwrap();
    // Parent plus child selected: the child still moves exactly once.
    e.select(Some(outer.id.clone()), SelectionMode::Replace)
        .unwrap();
    e.select(Some(a.id.clone()), SelectionMode::Toggle).unwrap();
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: 20.,
    })
    .unwrap();
    assert!(near(by_name(&e, "A").x - a.x, 10.));
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
}

// Local: hidden, locked and blank paint layers stay out of the box. Upstream has
// no locked field on its layers, so this follows the local move semantics instead:
// Nudge and canvas translation skip locked layers, and group edits must not bypass
// that choice.
#[test]
fn local_hidden_locked_and_blank_stay_out() {
    let doc = Document::new("Group", 400, 300).unwrap();
    let mut group = folder("Folder", &doc);
    let (mut a, mut b) = two_layers();
    a.parent_id = Some(group.id.clone());
    let mut hidden = image("Hidden", 10, 10);
    hidden.parent_id = Some(group.id.clone());
    hidden.visible = false;
    let mut locked = image("Locked", 10, 10);
    locked.parent_id = Some(group.id.clone());
    locked.locked = true;
    locked.x = 200.;
    locked.y = 200.;
    let mut blank = Layer::new("Blank", 400, 300, Content::Paint);
    blank.parent_id = Some(group.id.clone());
    b.parent_id = Some(group.id.clone());
    let mut e = editor_with(vec![
        group.clone(),
        a.clone(),
        b.clone(),
        hidden.clone(),
        locked.clone(),
        blank.clone(),
    ]);
    e.select(Some(group.id.clone()), SelectionMode::Replace)
        .unwrap();
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: box_of(&e).x + 5.,
    })
    .unwrap();
    assert!(near(by_name(&e, "A").x - a.x, 5.));
    assert!(near(by_name(&e, "B").x - b.x, 5.));
    assert_eq!(by_name(&e, "Hidden").x, hidden.x);
    assert_eq!(by_name(&e, "Locked").x, locked.x);
    assert_eq!(by_name(&e, "Blank").x, blank.x);
    // Only A (10,10)-(50,30) and B (100,40)-(120,80) shape the box.
    let grown = box_of(&e);
    let (w, h) = displayed(&grown);
    assert!(near(grown.x, 15.) && near(w, 110.) && near(h, 70.));
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
    assert_eq!(by_name(&e, "Locked").x, locked.x);

    // A hidden folder hides its children from the box as well.
    group.visible = false;
    place(&mut e, &group);
    assert!(e.edited_group_box().is_none());
}

// Local: locking follows the per-layer move semantics consistently. A locked folder
// does not freeze its unlocked children (Nudge moves them too); an unlocked folder
// whose children are all locked offers no box at all.
#[test]
fn local_locked_folder_and_all_locked_selection() {
    let doc = Document::new("Group", 400, 300).unwrap();
    let mut group = folder("Folder", &doc);
    group.locked = true;
    let (mut a, mut b) = two_layers();
    a.parent_id = Some(group.id.clone());
    b.parent_id = Some(group.id.clone());
    let mut e = editor_with(vec![group.clone(), a.clone(), b.clone()]);
    e.select(Some(group.id.clone()), SelectionMode::Replace)
        .unwrap();
    assert!(e.edited_group_box().is_some());
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: box_of(&e).x + 5.,
    })
    .unwrap();
    assert!(near(by_name(&e, "A").x - a.x, 5.));
    assert!(near(by_name(&e, "B").x - b.x, 5.));
    assert_eq!(by_name(&e, "Folder").x, 0.);
    e.command(Command::CancelTransform).unwrap();

    // Every member locked: no box, and BeginTransform stays a no-op.
    let mut locked_a = a.clone();
    locked_a.locked = true;
    let mut locked_b = b.clone();
    locked_b.locked = true;
    place(&mut e, &locked_a);
    place(&mut e, &locked_b);
    assert!(e.edited_group_box().is_none());
    let undo = e.history.info().undo_count;
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: 50.,
    })
    .unwrap();
    assert_eq!(by_name(&e, "A").x, a.x);
    assert_eq!(by_name(&e, "B").x, b.x);
    assert_eq!(e.history.info().undo_count, undo);
}

// Local: the box is the upright union of the members' rotated corners.
#[test]
fn local_box_is_upright_union_of_rotated_corners() {
    let (mut a, b) = two_layers();
    a.rotation = 90.;
    let e = editor_with(vec![a.clone(), b.clone()]);
    let mut selecting = e;
    selecting
        .select(Some(a.id.clone()), SelectionMode::Replace)
        .unwrap();
    selecting
        .select(Some(b.id.clone()), SelectionMode::Toggle)
        .unwrap();
    let grown = box_of(&selecting);
    let corners: Vec<Point> = [&a, &b].iter().flat_map(|l| geometry::corners(l)).collect();
    let (min_x, min_y) = corners
        .iter()
        .fold((f64::INFINITY, f64::INFINITY), |(x, y), p| {
            (x.min(p.x), y.min(p.y))
        });
    let (max_x, max_y) = corners
        .iter()
        .fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |(x, y), p| {
            (x.max(p.x), y.max(p.y))
        });
    assert!(near(grown.x, min_x) && near(grown.y, min_y));
    assert!(near(displayed(&grown).0, max_x - min_x));
    assert!(near(displayed(&grown).1, max_y - min_y));
    assert_eq!(grown.rotation, 0.);
}

// Local: dragging the box corner is one gesture and one undo; Escape cancels it.
#[test]
fn local_pointer_resize_is_one_undo_and_cancellable() {
    let (a, b) = two_layers();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    let start = box_of(&e);
    let se = geometry::handles(&start, 1.)
        .into_iter()
        .find(|h| h.0 == "se")
        .unwrap()
        .1;
    let plain = Modifiers::default();
    pointer(&mut e, Phase::Down, se.x, se.y, plain);
    pointer(&mut e, Phase::Move, se.x + 110., se.y + 70., plain);
    let grown = box_of(&e);
    assert!(near(displayed(&grown).0, 220.) && near(displayed(&grown).1, 140.));
    assert!(near(by_name(&e, "B").x, 190.));
    pointer(&mut e, Phase::Up, se.x + 110., se.y + 70., plain);
    assert_eq!(e.history.info().undo_count, 1);
    e.command(Command::Undo).unwrap();
    assert_eq!(by_name(&e, "B").x, b.x);
    e.command(Command::Redo).unwrap();
    assert!(near(by_name(&e, "B").x, 190.));

    // A second drag cancelled with Escape leaves no undo behind.
    pointer(&mut e, Phase::Down, se.x + 110., se.y + 70., plain);
    pointer(&mut e, Phase::Move, se.x + 130., se.y + 90., plain);
    assert!(near(displayed(&box_of(&e)).0, 240.));
    e.command(Command::CancelGesture).unwrap();
    assert!(near(by_name(&e, "B").x, 190.));
    assert_eq!(e.history.info().undo_count, 1);
}

// Local: Shift keeps the box proportions; Alt resizes about the center.
#[test]
fn local_shift_preserves_ratio_and_alt_uses_center() {
    let (a, b) = two_layers();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    let start = box_of(&e);
    let (w, h) = displayed(&start);
    let se = geometry::handles(&start, 1.)
        .into_iter()
        .find(|h| h.0 == "se")
        .unwrap()
        .1;
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    pointer(&mut e, Phase::Down, se.x, se.y, shift);
    pointer(&mut e, Phase::Move, se.x + 110., se.y + 10., shift);
    let grown = box_of(&e);
    let (gw, gh) = displayed(&grown);
    assert!(near(gw / gh, w / h));
    assert!(gw > w && gh > h);
    pointer(&mut e, Phase::Up, se.x + 110., se.y + 10., shift);
    assert_eq!(e.history.info().undo_count, 1);
    e.command(Command::Undo).unwrap();

    let center = geometry::center(&box_of(&e));
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    pointer(&mut e, Phase::Down, se.x, se.y, alt);
    pointer(&mut e, Phase::Move, se.x + 20., se.y + 20., alt);
    let centered = box_of(&e);
    assert!(near(geometry::center(&centered).x, center.x));
    assert!(near(geometry::center(&centered).y, center.y));
    pointer(&mut e, Phase::Up, se.x + 20., se.y + 20., alt);
}

// Local: the rotation stalk carries members around the box center, Shift snaps 15°.
#[test]
fn local_pointer_rotate_pivots_about_box_center() {
    let (a, b) = two_layers();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    let start = box_of(&e);
    let pivot = geometry::center(&start);
    let rotate = geometry::handles(&start, 1.)
        .into_iter()
        .find(|h| h.0 == "rotate")
        .unwrap()
        .1;
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    pointer(&mut e, Phase::Down, rotate.x, rotate.y, shift);
    // Drag to the left of the pivot: roughly +90° clockwise, snapped to 15°.
    pointer(&mut e, Phase::Move, pivot.x - 100., pivot.y, shift);
    let turned = box_of(&e);
    assert!((turned.rotation / 15.).fract().abs() < 0.001);
    assert!(near(turned.rotation.abs(), 90.));
    // Member centers rotate with the box around its old center.
    for (before, name) in [(&a, "A"), (&b, "B")] {
        let rel = (center(before).x - pivot.x, center(before).y - pivot.y);
        let angle = turned.rotation.to_radians();
        let expect = Point::new(
            pivot.x + rel.0 * angle.cos() - rel.1 * angle.sin(),
            pivot.y + rel.0 * angle.sin() + rel.1 * angle.cos(),
        );
        let actual = center(by_name(&e, name));
        assert!(near(actual.x, expect.x) && near(actual.y, expect.y));
    }
    pointer(&mut e, Phase::Up, pivot.x - 100., pivot.y, shift);
    assert_eq!(e.history.info().undo_count, 1);
}

// Local: numeric rotation keeps each member's box-relative placement, including a
// flipped and rotated child.
#[test]
fn local_numeric_rotate_keeps_relative_placement() {
    let (mut a, mut b) = two_layers();
    a.rotation = 30.;
    b.flip_x = true;
    b.rotation = -20.;
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    let before = box_of(&e);
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::Rotation,
        value: 90.,
    })
    .unwrap();
    assert!(near(box_of(&e).rotation, 90.));
    let after = box_of(&e);
    for (original, name) in [(&a, "A"), (&b, "B")] {
        let moved = by_name(&e, name).clone();
        assert!(near(
            moved.rotation,
            (original.rotation + 90. + 180.).rem_euclid(360.) - 180.
        ));
        // Box-relative coordinates of every member corner are invariant under the
        // rotation: the member travels with the box, whatever its own flips are.
        for (c0, c1) in geometry::corners(original)
            .into_iter()
            .zip(geometry::corners(&moved))
        {
            let r0 = geometry::to_local(&before, c0);
            let r1 = geometry::to_local(&after, c1);
            assert!(near(r0.x, r1.x) && near(r0.y, r1.y));
        }
    }
    assert!(by_name(&e, "B").flip_x);
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
}

// Local: linked masks travel with their member; unlinked masks stay document-fixed.
#[test]
fn local_masks_follow_link_state() {
    let (mut a, mut b) = two_layers();
    a.mask = Some(mask_linked());
    let mut unlinked = mask_linked();
    Arc::make_mut(&mut unlinked).linked = false;
    Arc::make_mut(&mut unlinked).placement = Some(MaskPlacement {
        sampling: Sampling::High,
        x: b.x,
        y: b.y,
        scale_x: 1.,
        scale_y: 1.,
        rotation: 0.,
        flip_x: false,
        flip_y: false,
    });
    b.mask = Some(unlinked);
    let frozen = b.mask.as_ref().unwrap().placement.unwrap();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: 30.,
    })
    .unwrap();
    e.command(Command::CommitTransform).unwrap();
    assert!(by_name(&e, "A").mask.as_ref().unwrap().placement.is_none());
    assert_eq!(
        by_name(&e, "B").mask.as_ref().unwrap().placement,
        Some(frozen)
    );
    assert_eq!(e.history.info().undo_count, 1);
}

// Local: an open draft follows arrow nudges and still commits as one undo.
#[test]
fn local_nudge_moves_open_draft_as_one_undo() {
    let (a, b) = two_layers();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::Nudge {
        delta: Point::new(3., -2.),
    })
    .unwrap();
    assert!(near(by_name(&e, "A").x - a.x, 3.));
    assert!(near(by_name(&e, "B").y - b.y, -2.));
    assert_eq!(e.history.info().undo_count, 0);
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
    e.command(Command::Undo).unwrap();
    assert_eq!(by_name(&e, "A").x, a.x);
}

// Local: scale percent grows the box about its center from the frozen baseline.
#[test]
fn local_scale_percent_uses_frozen_baseline() {
    let (a, b) = two_layers();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    let start = box_of(&e);
    let pivot = geometry::center(&start);
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::ScalePercent,
        value: 200.,
    })
    .unwrap();
    let grown = box_of(&e);
    assert!(near(displayed(&grown).0, displayed(&start).0 * 2.));
    assert!(near(geometry::center(&grown).x, pivot.x));
    assert!(near(geometry::center(&grown).y, pivot.y));
    // Typing the same percentage twice does not compound.
    e.command(Command::SetTransformField {
        field: TransformField::ScalePercent,
        value: 200.,
    })
    .unwrap();
    assert!(near(displayed(&box_of(&e)).0, displayed(&start).0 * 2.));
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
}

// Local: a committed group transform survives a project save and reopen.
#[test]
fn local_group_transform_survives_save_reopen() {
    let doc = Document::new("Group", 400, 300).unwrap();
    let group = folder("Folder", &doc);
    let (mut a, mut b) = two_layers();
    a.parent_id = Some(group.id.clone());
    b.parent_id = Some(group.id.clone());
    let mut e = editor_with(vec![group, a.clone(), b.clone()]);
    e.select(Some(a.parent_id.clone().unwrap()), SelectionMode::Replace)
        .unwrap();
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::Rotation,
        value: 45.,
    })
    .unwrap();
    e.command(Command::CommitTransform).unwrap();
    let before = Renderer::default()
        .export(&e.history.document, false)
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("group.comp");
    save_project(&path, &e.history.document).unwrap();
    let reopened = open_project(&path).unwrap();
    assert_eq!(reopened.layers.len(), 3);
    for layer in &reopened.layers {
        if layer.name == "A" || layer.name == "B" {
            let current = by_name(&e, &layer.name);
            assert_eq!(layer.x, current.x);
            assert_eq!(layer.y, current.y);
            assert_eq!(layer.rotation, current.rotation);
            assert_eq!(layer.parent_id, current.parent_id);
        }
    }
    assert_eq!(
        Renderer::default().export(&reopened, false).unwrap(),
        before
    );
}

// Local: deep folder nesting resolves through the borrowed index: a 40-deep chain
// still finds its leaf once and carries it exactly once.
#[test]
fn local_deep_nesting_resolves_through_index() {
    let mut layers = vec![];
    let mut parent: Option<String> = None;
    for depth in 0..40 {
        let mut folder = Layer::new(&format!("Folder {depth}"), 400, 300, Content::Group);
        folder.parent_id = parent.clone();
        parent = Some(folder.id.clone());
        layers.push(folder);
    }
    let mut leaf = image("Leaf", 30, 20);
    leaf.parent_id = parent;
    leaf.x = 50.;
    leaf.y = 60.;
    let outer = layers[0].id.clone();
    layers.push(leaf.clone());
    let mut e = editor_with(layers);
    e.select(Some(outer.clone()), SelectionMode::Replace)
        .unwrap();
    assert_eq!(e.group_members().len(), 1);
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: box_of(&e).x + 5.,
    })
    .unwrap();
    assert!(near(by_name(&e, "Leaf").x - leaf.x, 5.));
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
    // Selecting an intermediate folder with the leaf also carries it once.
    let middle = e.history.document.layers[20].id.clone();
    e.select(Some(middle.clone()), SelectionMode::Replace)
        .unwrap();
    e.select(Some(leaf.id.clone()), SelectionMode::Toggle)
        .unwrap();
    assert_eq!(e.group_members().len(), 1);
}

// Local: a lone layer keeps the single-layer transform path (no group box).
#[test]
fn local_single_layer_keeps_existing_path() {
    let (a, b) = two_layers();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    e.select(Some(a.id.clone()), SelectionMode::Replace)
        .unwrap();
    assert!(e.edited_group_box().is_none());
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: 99.,
    })
    .unwrap();
    assert_eq!(by_name(&e, "A").x, 99.);
    assert_eq!(by_name(&e, "B").x, b.x);
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
}

// Local bounded regression: carrying hundreds of members stays a single indexed
// pass and one undo. Small images keep the test fast; no rendering is involved.
#[test]
fn local_many_members_transform_in_one_pass() {
    let mut layers = vec![];
    for i in 0..300 {
        let mut layer = image(&format!("M{i}"), 2, 2);
        layer.x = (i % 20) as f64 * 10.;
        layer.y = (i / 20) as f64 * 10.;
        layers.push(layer);
    }
    let before: Vec<(String, f64, f64)> = layers.iter().map(|l| (l.id.clone(), l.x, l.y)).collect();
    let mut e = editor_with(layers);
    let ids: Vec<String> = e
        .history
        .document
        .layers
        .iter()
        .map(|l| l.id.clone())
        .collect();
    select_all(&mut e, &ids);
    assert!(e.edited_group_box().is_some());
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: box_of(&e).x + 7.,
    })
    .unwrap();
    for (id, x, y) in &before {
        let layer = e
            .history
            .document
            .layers
            .iter()
            .find(|l| &l.id == id)
            .unwrap();
        assert!(near(layer.x - x, 7.));
        assert!(near(layer.y - y, 0.));
    }
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, 1);
    e.command(Command::Undo).unwrap();
    for (id, x, y) in &before {
        let layer = e
            .history
            .document
            .layers
            .iter()
            .find(|l| &l.id == id)
            .unwrap();
        assert_eq!((layer.x, layer.y), (*x, *y));
    }
}

// Local: the member list is shared across repeated queries and only recomputed
// when the document (by value, via `retained_eq`) or the selection changes.
// Box-only queries never copy members: successive `group_members` calls return
// pointer-identical `Arc`s until an edit, undo or reselection invalidates them.
#[test]
fn local_group_cache_shares_members_until_change() {
    use std::sync::Arc;
    let (a, b) = two_layers();
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    let first = e.group_members();
    assert_eq!(first.len(), 2);
    // Repeated member and box-only queries share without recomputing.
    assert!(Arc::ptr_eq(&first, &e.group_members()));
    let _ = e.edited_group_box();
    let _ = e.edited_group_box();
    assert!(Arc::ptr_eq(&first, &e.group_members()));
    let before = box_of(&e);
    // An edit invalidates by document value and recomputes once.
    let mut moved = a.clone();
    moved.x += 4.;
    place(&mut e, &moved);
    let second = e.group_members();
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(&second, &e.group_members()));
    assert!(near(box_of(&e).x, before.x + 4.));
    // Undo restores the earlier values (recomputed, since values differ again).
    e.command(Command::Undo).unwrap();
    let restored = e.group_members();
    assert!(!Arc::ptr_eq(&second, &restored));
    assert_eq!(restored.len(), 2);
    assert!(near(box_of(&e).x, before.x));
    // Leaving the group selection yields no members at all.
    e.select(Some(a.id.clone()), SelectionMode::Replace)
        .unwrap();
    assert!(e.group_members().is_empty());
    assert!(e.edited_group_box().is_none());
}

// Local: an empty (all-locked or memberless) selection caches its emptiness too.
// Repeated box queries must not rebuild members or re-clone the document:
// successive calls keep sharing the same cached member `Arc`.
#[test]
fn local_empty_group_cache_reuses_shared_members() {
    use std::sync::Arc;
    let (mut a, mut b) = two_layers();
    a.locked = true;
    b.locked = true;
    let mut e = editor_with(vec![a.clone(), b.clone()]);
    select_all(&mut e, &[a.id.clone(), b.id.clone()]);
    assert!(e.edited_group_box().is_none());
    let first = e.group_members();
    assert!(first.is_empty());
    let _ = e.edited_group_box();
    let _ = e.edited_group_box();
    assert!(Arc::ptr_eq(&first, &e.group_members()));
    // An empty folder behaves the same way.
    let doc = Document::new("Group", 400, 300).unwrap();
    let empty = folder("Empty", &doc);
    let mut with_empty = e.history.document.clone();
    with_empty.layers.push(empty.clone());
    e.edit("test setup", with_empty, None).unwrap();
    e.select(Some(empty.id.clone()), SelectionMode::Replace)
        .unwrap();
    assert!(e.edited_group_box().is_none());
    let folder_first = e.group_members();
    let _ = e.edited_group_box();
    let _ = e.edited_group_box();
    assert!(Arc::ptr_eq(&folder_first, &e.group_members()));
}
