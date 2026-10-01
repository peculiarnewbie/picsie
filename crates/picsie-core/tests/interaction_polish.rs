//! Fixtures translated from SelectionTests, TransformTests and DistortTests
//! in Compositor 609dbeae. MIT © 2026 Wonder Assembly LLC. Local regressions labeled.
use picsie_core::{
    crop::{self, CropDrag, CropRect, DragMode},
    editor::*,
    feedback::CursorHint,
    geometry::{self, Viewport},
    model::*,
    pixel_selection::{PixelSelection, PixelSelectionMode},
    render::{self, Renderer},
};
use skia_safe::{self as sk, PathBuilder};
use std::sync::Arc;
fn image(w: u32, h: u32) -> Layer {
    let bytes = [255, 0, 0, 255].repeat((w * h) as usize);
    Layer::new(
        "Red",
        w,
        h,
        render::native_content(render::rgba_image(w, h, &bytes).unwrap()),
    )
}
fn editor(w: u32, h: u32, layer: Layer) -> Editor {
    let mut doc = Document::new("Interaction", w, h).unwrap();
    doc.layers.push(layer);
    let mut e = Editor::new(doc).unwrap();
    e.viewport = Viewport {
        width: w as f64,
        height: h as f64,
        zoom: 1.,
        pan: Point::default(),
    };
    e
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
fn path() -> sk::Path {
    let mut p = PathBuilder::new();
    p.move_to((0., 0.))
        .line_to((100., 0.))
        .line_to((0., 100.))
        .close();
    p.detach()
}
fn shape() -> [Point; 4] {
    [
        Point::new(10., 10.),
        Point::new(60., 10.),
        Point::new(30., 30.),
        Point::new(10., 30.),
    ]
}
fn rgba(e: &Editor, x: usize, y: usize) -> [u8; 4] {
    let image = Renderer::default()
        .render(&e.history.document)
        .unwrap()
        .image_snapshot();
    render::rgba_pixels(&image).unwrap()[(y * e.history.document.width as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}
// SelectionTests.antialiasingControlsEdgeCoverage.
#[test]
fn source_antialiasing_controls_diagonal_coverage() {
    let smooth = PixelSelection::from_path_with_smoothing(100, 100, path(), 0., true).unwrap();
    assert!((0..100).any(|x| (1..255).contains(&smooth.at(x, 99 - x))));
    let hard = PixelSelection::from_path_with_smoothing(100, 100, path(), 0., false).unwrap();
    assert!((0..100).all(|x| [0, 255].contains(&hard.at(x, 99 - x))));
    assert!(!hard.translated(Point::new(1., 0.)).unwrap().antialiased);
    assert!(!hard.inverted().unwrap().antialiased);
}
// SelectionTests.cursorBadgeFollowsModifiersButKeepsAnOutlinesStartingMode.
#[test]
fn source_modifiers_freeze_the_draft_without_changing_default() {
    let mut e = editor(100, 100, image(100, 100));
    e.command(Command::SetTool { tool: Tool::Lasso }).unwrap();
    let feedback = e.selection_feedback().unwrap();
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let both = Modifiers {
        shift: true,
        alt: true,
        ..Default::default()
    };
    assert_eq!(feedback.effective_mode(shift), PixelSelectionMode::Add);
    assert_eq!(feedback.effective_mode(both), PixelSelectionMode::Subtract);
    pointer(&mut e, Phase::Down, 10., 10., shift);
    assert_eq!(
        e.selection_feedback().unwrap().effective_mode(both),
        PixelSelectionMode::Add
    );
    assert_eq!(e.selection_mode, PixelSelectionMode::Replace);
    e.command(Command::CancelGesture).unwrap();
}
// Source Selection.swift canMoveSelection uses the winding path, not its bounding rectangle.
#[test]
fn source_move_cursor_respects_irregular_outline_and_wand_mode() {
    let mut e = editor(100, 100, image(100, 100));
    e.history.pixel_selection = Some(PixelSelection::from_path(100, 100, path(), 0.).unwrap());
    e.command(Command::SetTool { tool: Tool::Lasso }).unwrap();
    let f = e.selection_feedback().unwrap();
    assert_eq!(
        f.at_view(Point::new(25., 25.), Default::default()),
        CursorHint::Move
    );
    assert_eq!(
        f.at_view(Point::new(75., 75.), Default::default()),
        CursorHint::Crosshair
    );
    e.command(Command::SetTool { tool: Tool::Wand }).unwrap();
    let f = e.selection_feedback().unwrap();
    assert_eq!(
        f.at_view(Point::new(25., 25.), Default::default()),
        CursorHint::Crosshair
    );
    assert_eq!(
        f.at_view(
            Point::new(25., 25.),
            Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            }
        ),
        CursorHint::Copy
    );
}
// TransformTests.scalePercentSetsBothSidesAboutTheCenter; blank baseline is the
// pre-edit displayed size. Free resize default is the user's explicit adaptation.
#[test]
fn source_numeric_ratio_and_percent_are_one_cancelable_transaction() {
    let mut layer = image(20, 10);
    layer.x = 30.;
    layer.y = 20.;
    layer.scale_x = 2.;
    layer.scale_y = 3.;
    let mut e = editor(100, 80, layer);
    let before = e.history.document.clone();
    let center = geometry::center(e.selected().unwrap());
    assert_eq!(e.transform_scale_percent(), 200.);
    e.command(Command::SetTransformField {
        field: TransformField::ScalePercent,
        value: 150.,
    })
    .unwrap();
    assert_eq!(
        (e.selected().unwrap().scale_x, e.selected().unwrap().scale_y),
        (1.5, 1.5)
    );
    assert_eq!(geometry::center(e.selected().unwrap()), center);
    e.command(Command::SetTransformRatio { locked: true })
        .unwrap();
    assert!(e.transform_active());
    e.command(Command::SetTransformField {
        field: TransformField::Width,
        value: 60.,
    })
    .unwrap();
    assert_eq!(
        e.selected().unwrap().height as f64 * e.selected().unwrap().scale_y,
        30.
    );
    e.command(Command::CancelTransform).unwrap();
    assert_eq!(e.history.document, before);
    let count = e.history.info().undo_count;
    e.command(Command::SetTransformField {
        field: TransformField::X,
        value: 10.,
    })
    .unwrap();
    e.command(Command::SetTransformField {
        field: TransformField::Rotation,
        value: 372.5,
    })
    .unwrap();
    assert_eq!(e.selected().unwrap().rotation, 12.5);
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(e.history.info().undo_count, count + 1);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.history.document, before);
}
#[test]
fn source_blank_scale_baseline_does_not_compound() {
    let mut layer = Layer::new("Blank", 20, 10, Content::Paint);
    layer.scale_x = 2.;
    layer.scale_y = 3.;
    let mut e = editor(100, 80, layer);
    for value in [200., 150., 200.] {
        e.command(Command::SetTransformField {
            field: TransformField::ScalePercent,
            value,
        })
        .unwrap();
    }
    let l = e.selected().unwrap();
    assert_eq!((l.scale_x, l.scale_y), (4., 6.));
    assert_eq!(e.transform_scale_percent(), 200.);
}
// TransformTests.moveRotateAndShiftConstraints: Shift restricts an in-flight move to one axis.
#[test]
fn source_shift_constrains_move_after_pointer_down() {
    let mut e = editor(100, 100, image(100, 100));
    pointer(&mut e, Phase::Down, 50., 50., Default::default());
    pointer(
        &mut e,
        Phase::Up,
        80.,
        60.,
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(
        (e.selected().unwrap().x, e.selected().unwrap().y),
        (30., 0.)
    );
}
// CropSnap.apply (source implementation; source test suite doesn't cover symmetric snapping).
#[test]
fn local_symmetric_crop_snap_keeps_center() {
    let original = CropRect {
        x: 10.,
        y: 10.,
        width: 60.,
        height: 40.,
    };
    let drag = CropDrag {
        start: Point::new(70., 50.),
        original,
        mode: DragMode::Resize(4),
    };
    let p = Point::new(96., 66.);
    let next = drag.updated(p, None, true);
    let snapped = crop::snap(next, drag, p, None, true, (&[100.], &[70.]), 6.);
    assert_eq!(
        (
            snapped.x + snapped.width / 2.,
            snapped.y + snapped.height / 2.
        ),
        (40., 30.)
    );
    assert_eq!(
        (snapped.x + snapped.width, snapped.y + snapped.height),
        (100., 70.)
    );
}
// DistortTests.distortingWarpsTheLayerIntoTheShapeAsOneUndoStep.
#[test]
fn source_distortion_is_persistent_supports_folding_and_commits_one_undo() {
    let mut layer = image(20, 20);
    layer.x = 10.;
    layer.y = 10.;
    let mut e = editor(100, 60, layer);
    let before = e.history.document.clone();
    let count = e.history.info().undo_count;
    let c = shape();
    e.command(Command::DistortLayer {
        corners: [c[0], c[2], c[1], c[3]],
    })
    .unwrap();
    assert!(e.transform_active());
    assert_eq!(e.distortion_corners().unwrap()[1], c[2]);
    e.command(Command::DistortLayer { corners: c }).unwrap();
    e.command(Command::CommitTransform).unwrap();
    assert!(e.distortion_corners().is_none());
    assert_eq!(e.history.info().undo_count, count + 1);
    let l = e.selected().unwrap();
    assert_eq!(
        (l.x, l.y, l.width, l.height, l.rotation),
        (10., 10., 50, 20, 0.)
    );
    assert_eq!(rgba(&e, 50, 12)[3], 255);
    assert_eq!(rgba(&e, 15, 25)[3], 255);
    assert_eq!(rgba(&e, 50, 28)[3], 0);
    assert_eq!(rgba(&e, 80, 12)[3], 0);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.history.document, before);
}
// DistortTests.distortedLayerIsTrimmedToItsVisiblePixels.
#[test]
fn source_distortion_apply_trims_transparent_margins() {
    let mut bytes = vec![0u8; 40 * 20 * 4];
    for y in 5..15 {
        for x in 15..25 {
            bytes[(y * 40 + x) * 4..][..4].copy_from_slice(&[255, 0, 0, 255]);
        }
    }
    let mut layer = Layer::new(
        "Cutout",
        40,
        20,
        render::native_content(render::rgba_image(40, 20, &bytes).unwrap()),
    );
    layer.x = 10.;
    layer.y = 10.;
    let mut e = editor(100, 60, layer);
    let c = [
        Point::new(10., 10.),
        Point::new(60., 10.),
        Point::new(50., 30.),
        Point::new(10., 30.),
    ];
    e.command(Command::DistortLayer { corners: c }).unwrap();
    e.command(Command::CommitTransform).unwrap();
    let l = e.selected().unwrap();
    assert!(l.width < 20 && l.height <= 12);
    assert!(l.x >= 20. && l.y >= 14.);
    assert_eq!(rgba(&e, 30, 20)[3], 255);
}
// Local linked/unlinked mask regression for the cross-grid placement adapter.
#[test]
fn local_distortion_preserves_unlinked_mask_world_position_and_cancel() {
    let mut layer = image(20, 20);
    layer.x = 10.;
    layer.y = 10.;
    layer.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        linked: false,
        placement: None,
        strokes: vec![],
        raster: Some(Arc::new(MaskRaster {
            width: 2,
            height: 2,
            pixels: Arc::new(vec![0, 255, 0, 255]),
        })),
    }));
    let old = MaskPlacement::of(&layer).as_layer(&layer);
    let mut e = editor(100, 60, layer);
    let before = e.history.document.clone();
    e.command(Command::DistortLayer { corners: shape() })
        .unwrap();
    let l = e.selected().unwrap();
    let placed = l.mask.as_ref().unwrap().placement.unwrap().as_layer(l);
    assert_eq!(geometry::corners(&placed), geometry::corners(&old));
    e.command(Command::CancelTransform).unwrap();
    assert_eq!(e.history.document, before);
}
// Local off-canvas preview regression against EditorCanvas.renderBounds' union.
#[test]
fn local_expanded_crop_reveals_retained_pixels_and_transparency() {
    let mut layer = image(100, 60);
    layer.x = -10.;
    let mut e = editor(100, 60, layer);
    e.viewport.width = 160.;
    e.viewport.height = 100.;
    let mut r = Renderer::default();
    let crop = CropRect {
        x: -20.,
        y: -10.,
        width: 140.,
        height: 80.,
    };
    let image = r
        .preview(
            &e.history.document,
            &e.viewport,
            &[],
            false,
            None,
            Some(crop),
            None,
            None,
        )
        .unwrap()
        .image_snapshot();
    let pixels = render::rgba_pixels(&image).unwrap();
    let at = |x: usize, y: usize| &pixels[(y * 160 + x) * 4..][..4];
    assert_eq!(at(25, 40), [255, 0, 0, 255]); // document x=-5, retained source
    assert!(at(15, 40)[0] >= 70); // document x=-15, checkerboard inside expanded crop
    assert_eq!(e.history.document.width, 100);
}

// Local adapter checks for linked masks, floating merges and native overlay phases.
#[test]
fn local_distortion_carries_linked_mask_and_floating_pixels() {
    let mut layer = image(20, 20);
    layer.x = 10.;
    layer.y = 10.;
    layer.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        linked: true,
        placement: None,
        strokes: vec![],
        raster: Some(Arc::new(MaskRaster {
            width: 2,
            height: 2,
            pixels: Arc::new(vec![255, 0, 255, 0]),
        })),
    }));
    let mut e = editor(100, 60, layer);
    e.command(Command::DistortLayer { corners: shape() })
        .unwrap();
    e.command(Command::CommitTransform).unwrap();
    assert!(rgba(&e, 15, 12)[3] > 200);
    assert!(rgba(&e, 50, 12)[3] < 20);
    let mut layer = image(20, 20);
    layer.x = 10.;
    layer.y = 10.;
    let mut e = editor(100, 60, layer);
    e.history.pixel_selection = Some(
        PixelSelection::from_path(
            100,
            60,
            sk::Path::rect(sk::Rect::new(10., 10., 30., 30.), None),
            0.,
        )
        .unwrap(),
    );
    let before = e.history.document.clone();
    let count = e.history.info().undo_count;
    e.command(Command::DistortLayer { corners: shape() })
        .unwrap();
    e.command(Command::CommitTransform).unwrap();
    assert_eq!(rgba(&e, 50, 12)[3], 255);
    assert_eq!(rgba(&e, 50, 28)[3], 0);
    assert_eq!(e.history.info().undo_count, count + 1);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.history.document, before);
}
#[test]
fn local_native_outline_phase_moves_cached_vector_dashes() {
    let mut e = editor(100, 100, image(100, 100));
    e.history.pixel_selection = Some(PixelSelection::from_path(100, 100, path(), 0.).unwrap());
    let outline = e.selection_outline().unwrap();
    assert_ne!(outline.dashes(0.), outline.dashes(1.));
    assert_eq!(outline.dashes(0.), outline.dashes(8.));
}

// SelectionTests.movingOffCanvasAndBackKeepsTheWholeShape / Selection.swift::moveSelection.
#[test]
fn source_outline_can_leave_canvas_and_return_with_smoothing_intact() {
    let mut e = editor(100, 100, image(100, 100));
    let selected = PixelSelection::from_path_with_smoothing(
        100,
        100,
        sk::Path::rect(sk::Rect::new(10., 10., 30., 30.), None),
        0.,
        false,
    )
    .unwrap();
    e.history.pixel_selection = Some(selected.translated(Point::new(-40., 0.)).unwrap());
    assert!(
        e.history
            .pixel_selection
            .as_ref()
            .unwrap()
            .contains(Point::new(-20., 20.))
    );
    assert!(e.selection_outline().is_some());
    assert!(e.can_modify_selection());
    e.command(Command::MoveSelection {
        delta: Point::new(40., 0.),
    })
    .unwrap();
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(20, 20), 255);
    assert!(!e.history.pixel_selection.as_ref().unwrap().antialiased);
}
#[test]
fn source_feather_enables_smoothing_on_a_hard_outline() {
    let mut e = editor(100, 100, image(100, 100));
    e.history.pixel_selection =
        Some(PixelSelection::from_path_with_smoothing(100, 100, path(), 0., false).unwrap());
    e.command(Command::FeatherSelection { amount: 2 }).unwrap();
    let s = e.history.pixel_selection.as_ref().unwrap();
    assert!(!s.antialiased);
    assert_eq!(s.feather, 2.);
    assert!((0..100).any(|x| (1..255).contains(&s.at(x, 99 - x))));
}

// LayerTransform.swift::TransformDrag.corners: Shift restricts corner/edge motion.
#[test]
fn source_shift_constrains_distortion_corner_to_one_axis() {
    let mut layer = image(40, 40);
    layer.x = 20.;
    layer.y = 20.;
    let mut e = editor(100, 100, layer);
    let modifiers = Modifiers {
        control: true,
        shift: true,
        ..Default::default()
    };
    pointer(&mut e, Phase::Down, 20., 20., modifiers);
    pointer(&mut e, Phase::Move, 5., 10., modifiers);
    pointer(&mut e, Phase::Up, 5., 10., modifiers);
    let corners = e.distortion_corners().unwrap();
    assert_eq!(corners[0], Point::new(5., 20.));
    assert_eq!(corners[1], Point::new(60., 20.));
    let handles = e.cursor_map().handles.unwrap();
    assert_eq!(handles.points.len(), 8);
    assert!(!handles.rotation);
    e.command(Command::CancelTransform).unwrap();
    assert_eq!(e.selected().unwrap().x, 20.);
}
