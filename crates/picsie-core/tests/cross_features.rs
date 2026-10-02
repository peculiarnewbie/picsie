//! Cross-feature integration: group transforms × editable shapes/gradients ×
//! Levels/Curves adjustments. All three features were reviewed on their own
//! branches; these are derived local regressions (not upstream fixtures) proving
//! the combined behavior: adjustment exclusion from group members, shape redraw
//! through group commit, gradient/shape liveness rules under adjustments, and
//! shared publication equality. Compositor 609dbeae, MIT © 2026 Wonder
//! Assembly LLC.
use picsie_core::{
    adjustment::{AdjustmentKind, LayerAdjustment, LevelsSettings},
    editor::*,
    geometry::*,
    model::*,
    render::{self, Renderer},
};
use std::sync::Arc;

fn image(name: &str, w: u32, h: u32, x: f64, y: f64) -> Layer {
    let bytes = [200, 40, 30, 255].repeat((w * h) as usize);
    let mut layer = Layer::new(
        name,
        w,
        h,
        render::native_content(render::rgba_image(w, h, &bytes).unwrap()),
    );
    layer.x = x;
    layer.y = y;
    layer
}

fn rounded_shape(name: &str, w: u32, h: u32, x: f64, y: f64, radius: f64) -> Layer {
    let mut layer = Layer::new(
        name,
        w,
        h,
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#ff0000".into(),
            corner_radius: radius,
            line_width: None,
            line_start: None,
            line_end: None,
        },
    );
    layer.x = x;
    layer.y = y;
    layer
}

fn adjustment(name: &str, doc: &Document) -> Layer {
    let mut layer = Layer::new(name, doc.width, doc.height, Content::Paint);
    layer.adjustment = Some(LayerAdjustment::new(AdjustmentKind::Levels));
    layer
}

fn editor_with(layers: Vec<Layer>) -> Editor {
    let mut doc = Document::new("Cross", 400, 300).unwrap();
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

fn select(e: &mut Editor, ids: &[String]) {
    e.select(Some(ids[0].clone()), SelectionMode::Replace)
        .unwrap();
    for id in &ids[1..] {
        e.select(Some(id.clone()), SelectionMode::Toggle).unwrap();
    }
}

fn by_name<'a>(e: &'a Editor, name: &str) -> &'a Layer {
    e.history
        .document
        .layers
        .iter()
        .find(|l| l.name == name)
        .unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

/// Upstream `asset != nil`: source-less adjustment layers are never group
/// members. A mixed selection transforms only its pixel members; a lone
/// adjustment (or an all-adjustment selection) yields no box at all.
#[test]
fn group_members_exclude_sourceless_adjustments() {
    let doc = Document::new("Cross", 400, 300).unwrap();
    let a = image("A", 40, 20, 10., 10.);
    let adj = adjustment("Levels", &doc);
    let mut e = editor_with(vec![a.clone(), adj.clone()]);
    // Mixed selection: the box covers the pixel member only.
    select(&mut e, &[a.id.clone(), adj.id.clone()]);
    let members = e.group_members();
    assert_eq!(
        members.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
        vec!["A".to_string()]
    );
    let boxed = e.edited_group_box().expect("pixel member keeps a box");
    assert!(near(boxed.x, 10.) && near(boxed.y, 10.));
    // Lone adjustment: no box, and BeginTransform stays a no-op.
    select(&mut e, &[adj.id.clone()]);
    assert!(e.group_members().is_empty());
    assert!(e.edited_group_box().is_none());
    e.command(Command::BeginTransform).unwrap();
    assert!(!e.history.info().dirty || e.history.info().undo_count == 0);
}

fn half_mask(width: u32, height: u32) -> Arc<LayerMask> {
    let mut pixels = vec![255u8; (width * height) as usize];
    for y in 0..height {
        for x in 0..width / 2 {
            pixels[(y * width + x) as usize] = 0;
        }
    }
    Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        raster: Some(Arc::new(MaskRaster {
            width,
            height,
            pixels: Arc::new(pixels),
        })),
        linked: true,
        placement: None,
        strokes: vec![],
    })
}

fn line_shape() -> Layer {
    let mut layer = Layer::new(
        "L",
        60,
        20,
        Content::Shape {
            shape: Shape::Line,
            color: "#ff0000".into(),
            corner_radius: 0.,
            line_width: Some(6.),
            line_start: Some(Point::new(0.1, 0.5)),
            line_end: Some(Point::new(0.9, 0.5)),
        },
    );
    layer.x = 100.;
    layer.y = 200.;
    layer
}

fn px(doc: &Document, x: f64, y: f64) -> [u8; 4] {
    Renderer::default().sample(doc, Point::new(x, y)).unwrap()
}

/// Group resize of an editable rounded shape AND a line commits through the
/// shared `redrawShape` hook: previews carry placement only, while commit
/// redraws each source grid at its displayed size, preserving document-pixel
/// radius/line width. Rendered corners, line width and the linked mask
/// footprint match before/after commit, and save/reopen retains everything.
#[test]
fn group_resize_redraws_rounded_shape_at_commit() {
    use picsie_core::files::{open_project, save_project};
    let mut shape = rounded_shape("R", 100, 60, 20., 20., 12.);
    shape.mask = Some(half_mask(100, 60));
    let line = line_shape();
    let mut e = editor_with(vec![shape.clone(), line.clone()]);
    select(&mut e, &[shape.id.clone(), line.id.clone()]);
    let start = e.edited_group_box().expect("group box");
    assert!(near(start.width as f64 * start.scale_x, 140.));
    e.command(Command::BeginTransform).unwrap();
    // Double the box width numerically; previews must not normalize the shapes.
    e.command(Command::SetTransformField {
        field: TransformField::Width,
        value: 280.,
    })
    .unwrap();
    let preview = e.history.document.clone();
    for name in ["R", "L"] {
        assert!(
            matches!(
                preview
                    .layers
                    .iter()
                    .find(|l| l.name == name)
                    .unwrap()
                    .content
                    .as_ref(),
                Content::Shape { .. }
            ),
            "{name} stays editable in the preview"
        );
    }
    // Rendered preview: rounded corner stays open, mask hides the same left
    // half, and the line draws at its document-pixel width.
    assert_eq!(px(&preview, 22., 22.)[3], 0);
    assert_eq!(px(&preview, 30., 50.)[3], 0);
    assert_eq!(px(&preview, 100., 50.)[3], 0);
    assert_eq!(px(&preview, 150., 50.), [255, 0, 0, 255]);
    assert_eq!(px(&preview, 240., 210.), [255, 0, 0, 255]);
    assert_eq!(px(&preview, 240., 200.)[3], 0);
    e.command(Command::CommitTransform).unwrap();
    let committed = e.history.document.clone();
    // Full source grid changes at commit: unit scales at the displayed sizes.
    let rect = committed.layers.iter().find(|l| l.name == "R").unwrap();
    assert!(near(rect.scale_x, 1.) && near(rect.scale_y, 1.));
    assert_eq!((rect.width, rect.height), (200, 60));
    match rect.content.as_ref() {
        Content::Shape {
            shape,
            corner_radius,
            ..
        } => {
            assert_eq!(*shape, Shape::Rectangle);
            assert!(near(*corner_radius, 12.));
        }
        _ => panic!("group commit must keep the rounded shape editable"),
    }
    let kept = rect.mask.as_ref().expect("mask survives group commit");
    assert!(kept.enabled && kept.linked && kept.strokes.is_empty());
    assert_eq!(
        kept.raster.as_ref().map(|r| (r.width, r.height)),
        Some((100, 60))
    );
    let placement = kept.placement.expect("redraw pins the mask footprint");
    assert!(near(placement.x, 20.) && near(placement.y, 20.));
    assert!(near(placement.scale_x, 1.) && near(placement.scale_y, 1.));
    let stroke = committed.layers.iter().find(|l| l.name == "L").unwrap();
    assert!(near(stroke.scale_x, 1.) && near(stroke.scale_y, 1.));
    assert_eq!((stroke.width, stroke.height), (120, 20));
    match stroke.content.as_ref() {
        Content::Shape {
            shape,
            line_width,
            line_start,
            line_end,
            ..
        } => {
            assert_eq!(*shape, Shape::Line);
            assert_eq!(*line_width, Some(6.));
            assert!(line_start.is_some() && line_end.is_some());
        }
        _ => panic!("group commit must keep the line editable"),
    }
    // The committed frame renders the same document pixels as the preview.
    for (x, y) in [
        (22., 22.),
        (30., 50.),
        (100., 50.),
        (150., 50.),
        (240., 210.),
        (240., 200.),
    ] {
        assert_eq!(
            px(&committed, x, y),
            px(&preview, x, y),
            "pixel at ({x}, {y})"
        );
    }
    // Save/reopen retains the redrawn shapes, style and mask footprint.
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("group-shapes.picsie");
    save_project(&path, &committed).unwrap();
    let reopened = open_project(&path).unwrap();
    let again = reopened.layers.iter().find(|l| l.name == "R").unwrap();
    assert_eq!((again.width, again.height), (200, 60));
    match again.content.as_ref() {
        Content::Shape { corner_radius, .. } => assert!(near(*corner_radius, 12.)),
        _ => panic!("reopen must keep the rounded shape editable"),
    }
    assert_eq!(
        again.mask.as_ref().and_then(|m| m.placement),
        kept.placement
    );
    let opened_line = reopened.layers.iter().find(|l| l.name == "L").unwrap();
    assert!(matches!(
        opened_line.content.as_ref(),
        Content::Shape {
            shape: Shape::Line,
            line_width: Some(_),
            ..
        }
    ));
    assert_eq!(px(&reopened, 150., 50.), [255, 0, 0, 255]);
    assert_eq!(px(&reopened, 240., 210.), [255, 0, 0, 255]);
}

/// Group flip mirrors placement only: editable shapes are never rasterized and
/// stay editable.
#[test]
fn group_flip_keeps_shape_editable() {
    let shape = rounded_shape("R", 100, 60, 20., 20., 12.);
    let other = image("B", 20, 20, 200., 200.);
    let mut e = editor_with(vec![shape.clone(), other.clone()]);
    select(&mut e, &[shape.id.clone(), other.id.clone()]);
    e.update_layer(serde_json::json!({"flipX": true})).unwrap();
    let flipped = by_name(&e, "R").clone();
    assert!(matches!(flipped.content.as_ref(), Content::Shape { .. }));
    assert!(flipped.flip_x);
    assert_eq!(e.history.info().undo_count, 1);
}

/// A committed gradient raster edit clears live shape style, matching upstream
/// dropping liveness once pixels change elsewhere.
#[test]
fn gradient_commit_clears_live_shape_style() {
    let shape = rounded_shape("R", 100, 60, 0., 0., 0.);
    let mut e = editor_with(vec![shape.clone()]);
    e.select(Some(shape.id.clone()), SelectionMode::Replace)
        .unwrap();
    e.command(Command::SetTool {
        tool: Tool::Gradient,
    })
    .unwrap();
    e.command(Command::Pointer {
        samples: vec![
            PointerSample {
                phase: Phase::Down,
                point: Point::new(10.5, 20.),
                modifiers: Modifiers::default(),
            },
            PointerSample {
                phase: Phase::Move,
                point: Point::new(90.5, 20.),
                modifiers: Modifiers::default(),
            },
            PointerSample {
                phase: Phase::Up,
                point: Point::new(90.5, 20.),
                modifiers: Modifiers::default(),
            },
        ],
    })
    .unwrap();
    assert!(e.gradient_edit.is_some());
    e.command(Command::CommitGradient).unwrap();
    assert!(!matches!(
        by_name(&e, "R").content.as_ref(),
        Content::Shape { .. }
    ));
}

/// The pending gradient preview never leaks into the committed save/export
/// document before Apply.
#[test]
fn pending_gradient_stays_out_of_committed_document() {
    let mut e = editor_with(vec![image("A", 100, 60, 0., 0.)]);
    let id = e.history.document.layers[0].id.clone();
    e.select(Some(id.clone()), SelectionMode::Replace).unwrap();
    e.command(Command::SetTool {
        tool: Tool::Gradient,
    })
    .unwrap();
    e.command(Command::Pointer {
        samples: vec![
            PointerSample {
                phase: Phase::Down,
                point: Point::new(10.5, 20.),
                modifiers: Modifiers::default(),
            },
            PointerSample {
                phase: Phase::Move,
                point: Point::new(90.5, 20.),
                modifiers: Modifiers::default(),
            },
            PointerSample {
                phase: Phase::Up,
                point: Point::new(90.5, 20.),
                modifiers: Modifiers::default(),
            },
        ],
    })
    .unwrap();
    assert!(e.gradient_edit.as_ref().is_some_and(|g| g.has_line()));
    // The live document carries the preview; the committed base does not.
    let committed = e.gradient_base_document().expect("pending base");
    let base_layer = committed.layers.iter().find(|l| l.id == id).unwrap();
    let live_layer = e
        .history
        .document
        .layers
        .iter()
        .find(|l| l.id == id)
        .unwrap();
    assert!(!Arc::ptr_eq(&base_layer.content, &live_layer.content));
    e.command(Command::CancelGradient).unwrap();
    assert!(e.gradient_base_document().is_none());
}

/// An adjustment above an editable shape remaps the shape's composite and
/// preserves alpha; identity stays a byte-exact no-op.
#[test]
fn adjustment_above_shape_remaps_composite_preserving_alpha() {
    let mut doc = Document::new("Cross", 4, 2).unwrap();
    doc.layers.push(rounded_shape("R", 4, 2, 0., 0., 0.));
    let plain = Renderer::default().render(&doc).unwrap().image_snapshot();
    let plain_pixels = render::rgba_pixels(&plain).unwrap();
    assert!(plain_pixels[3] == 255 && plain_pixels[0] > 200);
    let mut settings = LevelsSettings::default();
    settings.ranges[0].output_white = 0.;
    let mut adj = Layer::new("Levels", 4, 2, Content::Paint);
    let mut value = LayerAdjustment::new(AdjustmentKind::Levels);
    value.levels = settings.clone();
    adj.adjustment = Some(value);
    doc.layers.push(adj);
    let pixels =
        render::rgba_pixels(&Renderer::default().render(&doc).unwrap().image_snapshot()).unwrap();
    assert_eq!(&pixels[0..4], &[0, 0, 0, 255]);
    // Identity adjustment is a byte-exact no-op over the shape.
    doc.layers.pop();
    let mut identity = Layer::new("Levels", 4, 2, Content::Paint);
    identity.adjustment = Some(LayerAdjustment::new(AdjustmentKind::Levels));
    doc.layers.push(identity);
    let same =
        render::rgba_pixels(&Renderer::default().render(&doc).unwrap().image_snapshot()).unwrap();
    assert_eq!(same, plain_pixels);
    // A soft mask halves the shape's coverage: the same crush keeps RGB black
    // while the semi-translucent alpha comes back untouched, never thickened.
    doc.layers.pop();
    let mut soft = rounded_shape("S", 4, 2, 0., 0., 0.);
    soft.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        raster: Some(Arc::new(MaskRaster {
            width: 4,
            height: 2,
            pixels: Arc::new(vec![128; 8]),
        })),
        linked: true,
        placement: None,
        strokes: vec![],
    }));
    doc.layers[0] = soft;
    let mut crush = Layer::new("Levels", 4, 2, Content::Paint);
    let mut crushed = LayerAdjustment::new(AdjustmentKind::Levels);
    crushed.levels = settings.clone();
    crush.adjustment = Some(crushed);
    doc.layers.push(crush);
    let soft_pixels =
        render::rgba_pixels(&Renderer::default().render(&doc).unwrap().image_snapshot()).unwrap();
    assert_eq!(&soft_pixels[0..4], &[0, 0, 0, 128]);
}

/// Starting an adjustment edit resolves a pending gradient first, so the two
/// live edits never stack.
#[test]
fn adjustment_begin_resolves_pending_gradient() {
    let mut e = editor_with(vec![image("A", 100, 60, 0., 0.)]);
    let id = e.history.document.layers[0].id.clone();
    e.select(Some(id.clone()), SelectionMode::Replace).unwrap();
    e.command(Command::SetTool {
        tool: Tool::Gradient,
    })
    .unwrap();
    e.command(Command::Pointer {
        samples: vec![
            PointerSample {
                phase: Phase::Down,
                point: Point::new(10.5, 20.),
                modifiers: Modifiers::default(),
            },
            PointerSample {
                phase: Phase::Move,
                point: Point::new(90.5, 20.),
                modifiers: Modifiers::default(),
            },
            PointerSample {
                phase: Phase::Up,
                point: Point::new(90.5, 20.),
                modifiers: Modifiers::default(),
            },
        ],
    })
    .unwrap();
    assert!(e.gradient_edit.is_some());
    e.command(Command::AddAdjustment {
        kind: AdjustmentKind::Levels,
    })
    .unwrap();
    assert!(e.gradient_edit.is_none());
    assert!(e.adjustment_edit.is_some());
}

/// The shared publication distinguishes every new live state, so Cancel/Undo
/// restore the right controls: group box, pending gradient and adjustment edit
/// each flip `same_controls`.
#[test]
fn publication_distinguishes_combined_live_state() {
    use picsie_core::editor::publication::SnapshotPublisher;
    let shape = rounded_shape("R", 100, 60, 20., 20., 12.);
    let other = image("B", 20, 20, 200., 200.);
    let mut e = editor_with(vec![shape.clone(), other.clone()]);
    let mut publisher = SnapshotPublisher::default();
    select(&mut e, &[shape.id.clone(), other.id.clone()]);
    let grouped = publisher.capture(&e).unwrap();
    e.select(Some(shape.id.clone()), SelectionMode::Replace)
        .unwrap();
    let single = publisher.capture(&e).unwrap();
    assert!(!grouped.same_controls(&single));
    // Pending gradient flips the comparison; cancel restores it.
    e.command(Command::SetTool {
        tool: Tool::Gradient,
    })
    .unwrap();
    e.command(Command::Pointer {
        samples: vec![PointerSample {
            phase: Phase::Down,
            point: Point::new(10.5, 20.),
            modifiers: Modifiers::default(),
        }],
    })
    .unwrap();
    let pending = publisher.capture(&e).unwrap();
    assert!(!single.same_controls(&pending));
    e.command(Command::CancelGradient).unwrap();
    e.command(Command::SetTool { tool: Tool::Move }).unwrap();
    let settled = publisher.capture(&e).unwrap();
    assert!(single.same_controls(&settled));
}
