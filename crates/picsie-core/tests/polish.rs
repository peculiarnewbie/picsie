//! Selected CursorTests / TransformOverlay and CanvasThumbnailTests scenarios,
//! Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC. Local regressions labeled.
use picsie_core::{
    editor::{Command, Editor, Modifiers, PaintTarget, Tool},
    feedback::CursorHint,
    geometry::{self, Viewport},
    model::*,
    render::Renderer,
    thumbnail::Thumbnails,
};
use std::sync::Arc;
fn mask(base: MaskMode) -> Arc<LayerMask> {
    LayerMask {
        enabled: true,
        base,
        raster: None,
        linked: true,
        placement: None,
        strokes: vec![],
    }
    .into()
}
fn editor() -> Editor {
    let mut doc = Document::new("Polish", 400, 200).unwrap();
    let mut layer = Layer::new(
        "Red",
        100,
        100,
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#ff0000".into(),
        },
    );
    layer.x = 100.;
    layer.y = 50.;
    doc.layers.push(layer);
    let mut e = Editor::new(doc).unwrap();
    e.viewport = Viewport {
        width: 400.,
        height: 200.,
        zoom: 1.,
        pan: Point::default(),
    };
    e
}
#[test]
fn local_crop_preview_has_nonprinting_surround_and_thirds() {
    let mut e = Editor::new(Document::new("Crop overlay", 100, 100).unwrap()).unwrap();
    e.viewport = Viewport {
        width: 140.,
        height: 140.,
        zoom: 1.,
        pan: Point::default(),
    };
    e.tool = Tool::Crop;
    let cursor = e.cursor_map();
    assert!(!cursor.crop_active);
    assert_eq!(cursor.crop.unwrap().width, 100.);
    assert_eq!(
        cursor.at(Point::new(25., 0.), Modifiers::default()),
        CursorHint::ResizeVertical
    );
    let mut renderer = Renderer::default();
    let before = picsie_core::render::rgba_pixels(
        &renderer
            .render(&e.history.document)
            .unwrap()
            .image_snapshot(),
    )
    .unwrap();
    let mut preview = |crop| {
        picsie_core::render::rgba_pixels(
            &renderer
                .preview(
                    &e.history.document,
                    &e.viewport,
                    &[],
                    false,
                    None,
                    crop,
                    None,
                    None,
                )
                .unwrap()
                .image_snapshot(),
        )
        .unwrap()
    };
    let plain = preview(None);
    let cropped = preview(Some(picsie_core::crop::CropRect {
        x: 10.,
        y: 10.,
        width: 60.,
        height: 60.,
    }));
    let pixel = |pixels: &[u8], x: usize, y: usize| pixels[(y * 140 + x) * 4];
    assert!(pixel(&cropped, 10, 10) < pixel(&plain, 10, 10) / 2);
    assert_eq!(pixel(&cropped, 40, 40), pixel(&plain, 40, 40));
    assert!(pixel(&cropped, 50, 45) > pixel(&plain, 50, 45) + 20);
    let after = picsie_core::render::rgba_pixels(
        &renderer
            .render(&e.history.document)
            .unwrap()
            .image_snapshot(),
    )
    .unwrap();
    assert_eq!(
        before, after,
        "crop guides never enter exported document pixels"
    );
    assert!(e.crop_rect.is_none());
    assert_eq!(e.history.info().undo_count, 0);
    assert_eq!(
        cursor.at(Point::new(25., 25.), Modifiers::default()),
        CursorHint::Crosshair
    );
    for (phase, point) in [
        (picsie_core::editor::Phase::Down, Point::new(25., 25.)),
        (picsie_core::editor::Phase::Up, Point::new(65., 70.)),
    ] {
        e.pointer(picsie_core::editor::PointerSample {
            phase,
            point: Point::new(point.x + 20., point.y + 20.),
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
        })
        .unwrap();
    }
    let crop = e.crop_rect.unwrap();
    assert_eq!(
        (crop.x, crop.y, crop.width, crop.height),
        (25., 25., 40., 45.)
    );
}
#[test]
fn source_cursor_active_layer_moves_outside_and_alt_offers_copy() {
    let e = editor();
    let map = e.cursor_map();
    assert_eq!(
        map.at(Point::new(20., 20.), Modifiers::default()),
        CursorHint::Move
    );
    assert_eq!(
        map.at(
            Point::new(20., 20.),
            Modifiers {
                alt: true,
                ..Default::default()
            }
        ),
        CursorHint::Copy
    );
    assert_eq!(
        map.at(Point::new(125., 59.), Modifiers::default()),
        CursorHint::ResizeVertical
    );
    assert_eq!(
        map.at(Point::new(150., 22.), Modifiers::default()),
        CursorHint::Rotate
    );
}
#[test]
fn source_hidden_controls_leave_only_moving_until_persistent_transform() {
    let mut e = editor();
    e.view_options.show_controls = false;
    assert_eq!(
        e.cursor_map()
            .at(Point::new(100., 50.), Modifiers::default()),
        CursorHint::Move
    );
    e.command(Command::BeginTransform).unwrap();
    assert_eq!(
        e.cursor_map()
            .at(Point::new(100., 50.), Modifiers::default()),
        CursorHint::ResizeDiagonalDown
    );
}
#[test]
fn source_edges_and_rotated_cursor_use_the_actual_resize_hit() {
    let mut e = editor();
    assert_eq!(
        geometry::hit_handle(e.selected().unwrap(), Point::new(125., 59.), 1.),
        Some("n")
    );
    e.history.document.layers[0].rotation = 45.;
    let layer = e.selected().unwrap();
    let p = geometry::bounds_point(layer, Point::new(0.25, 0.));
    assert_eq!(geometry::hit_handle(layer, p, 1.), Some("n"));
    assert_eq!(
        e.cursor_map().at(p, Modifiers::default()),
        CursorHint::ResizeDiagonalUp
    );
}
#[test]
fn source_ellipse_shift_circle_and_alt_from_center() {
    let mut e = editor();
    e.tool = Tool::Ellipse;
    let modifiers = Modifiers {
        shift: true,
        alt: true,
        ..Default::default()
    };
    for (phase, point) in [
        (picsie_core::editor::Phase::Down, Point::new(50., 40.)),
        (picsie_core::editor::Phase::Up, Point::new(60., 45.)),
    ] {
        e.pointer(picsie_core::editor::PointerSample {
            phase,
            point,
            modifiers,
        })
        .unwrap();
    }
    let l = e.selected().unwrap();
    assert_eq!((l.x, l.y, l.width, l.height), (40., 30., 20, 20));
}
#[test]
fn source_shape_click_and_tool_switch_discard_draft() {
    let mut e = editor();
    e.tool = Tool::Rectangle;
    let before = e.history.document.clone();
    for phase in [
        picsie_core::editor::Phase::Down,
        picsie_core::editor::Phase::Up,
    ] {
        e.pointer(picsie_core::editor::PointerSample {
            phase,
            point: Point::new(20., 20.),
            modifiers: Modifiers::default(),
        })
        .unwrap();
    }
    assert_eq!(e.history.document, before);
    e.pointer(picsie_core::editor::PointerSample {
        phase: picsie_core::editor::Phase::Down,
        point: Point::new(20., 20.),
        modifiers: Modifiers::default(),
    })
    .unwrap();
    e.pointer(picsie_core::editor::PointerSample {
        phase: picsie_core::editor::Phase::Move,
        point: Point::new(50., 50.),
        modifiers: Modifiers::default(),
    })
    .unwrap();
    e.command(Command::SetTool { tool: Tool::Brush }).unwrap();
    assert_eq!(e.history.document, before);
}
#[test]
fn source_opacity_digits_and_quarter_hardness_steps() {
    let mut e = editor();
    e.tool = Tool::Brush;
    e.command(Command::TypeOpacityDigit { digit: 4 }).unwrap();
    assert_eq!(e.brush_opacity, 0.4);
    e.command(Command::TypeOpacityDigit { digit: 5 }).unwrap();
    assert_eq!(e.brush_opacity, 0.45);
    e.command(Command::TypeOpacityDigit { digit: 0 }).unwrap();
    assert_eq!(e.brush_opacity, 1.);
    e.command(Command::TypeOpacityDigit { digit: 5 }).unwrap();
    assert_eq!(e.brush_opacity, 0.05);
    e.brush_hardness = 0.8;
    e.command(Command::StepBrushHardness { increase: false })
        .unwrap();
    assert_eq!(e.brush_hardness, 0.75);
    e.command(Command::StepBrushHardness { increase: true })
        .unwrap();
    assert_eq!(e.brush_hardness, 1.);
    e.tool = Tool::Move;
    e.command(Command::StepBrushHardness { increase: false })
        .unwrap();
    assert_eq!(e.brush_hardness, 1.);
    e.command(Command::TypeOpacityDigit { digit: 3 }).unwrap();
    assert_eq!(e.selected().unwrap().opacity, 0.3);
}
#[test]
fn source_mask_thumbnail_edge_tone_carries_outside_the_layer() {
    let mut e = editor();
    let layer = &mut e.history.document.layers[0];
    layer.mask = Some(mask(MaskMode::Hide));
    let mut cache = Thumbnails::default();
    let mut renderer = Renderer::default();
    let output = cache.update(&e.history.document, &mut renderer).unwrap();
    let thumb = &output
        .iter()
        .find(|(id, _)| id.starts_with("@mask-"))
        .unwrap()
        .1;
    assert_eq!(&thumb.pixels[..4], &[0, 0, 0, 255]);
    let mut pixels = vec![255; 20 * 20];
    for y in 5..15 {
        for x in 5..15 {
            pixels[y * 20 + x] = 0;
        }
    }
    assert_eq!(picsie_core::thumbnail::edge_tone(&pixels, 20, 20), 255);
    Arc::make_mut(e.history.document.layers[0].mask.as_mut().unwrap()).raster =
        Some(Arc::new(MaskRaster {
            width: 20,
            height: 20,
            pixels: Arc::new(pixels),
        }));
    let output = cache.update(&e.history.document, &mut renderer).unwrap();
    let thumb = &output
        .iter()
        .find(|(id, _)| id.starts_with("@mask-"))
        .unwrap()
        .1;
    assert_eq!(&thumb.pixels[..4], &[255, 255, 255, 255]);
    let center = ((18 * thumb.width + 27) * 4) as usize;
    assert!(thumb.pixels[center] < 10);
}
#[test]
fn local_mask_thumbnail_selection_enable_reuses_and_raster_edit_invalidates() {
    let mut e = editor();
    e.history.document.layers[0].mask = Some(mask(MaskMode::Reveal));
    let mut cache = Thumbnails::default();
    let mut renderer = Renderer::default();
    let first = cache
        .update(&e.history.document, &mut renderer)
        .unwrap()
        .remove(1)
        .1;
    e.paint_target = PaintTarget::Mask;
    Arc::make_mut(e.history.document.layers[0].mask.as_mut().unwrap()).enabled = false;
    let again = cache
        .update(&e.history.document, &mut renderer)
        .unwrap()
        .remove(1)
        .1;
    assert!(Arc::ptr_eq(&first, &again));
    Arc::make_mut(e.history.document.layers[0].mask.as_mut().unwrap()).base = MaskMode::Hide;
    let changed = cache
        .update(&e.history.document, &mut renderer)
        .unwrap()
        .remove(1)
        .1;
    assert!(!Arc::ptr_eq(&first, &changed));
    e.history.document.layers[0].mask = None;
    assert_eq!(
        cache
            .update(&e.history.document, &mut renderer)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn local_caret_presentation_never_commits_a_later_gesture() {
    let mut e = editor();
    e.tool = Tool::Text;
    for phase in [
        picsie_core::editor::Phase::Down,
        picsie_core::editor::Phase::Up,
    ] {
        e.pointer(picsie_core::editor::PointerSample {
            phase,
            point: Point::new(20., 20.),
            modifiers: Modifiers::default(),
        })
        .unwrap();
    }
    e.command(Command::UpdateText {
        patch: picsie_core::text::TextPatch {
            text: Some("word next\nlast line".into()),
            ..Default::default()
        },
    })
    .unwrap();
    let count = e.history.info().undo_count;
    e.command(Command::SetTextCaretVisible { visible: false })
        .unwrap();
    assert!(e.text_editing());
    assert_eq!(e.history.info().undo_count, count);
    e.command(Command::CancelText).unwrap();
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::SetTextCaretVisible { visible: true })
        .unwrap();
    assert!(e.transform_active());
}
#[test]
fn local_word_and_paragraph_selection_use_shaped_text_byte_boundaries() {
    let mut e = editor();
    e.tool = Tool::Text;
    for phase in [
        picsie_core::editor::Phase::Down,
        picsie_core::editor::Phase::Up,
    ] {
        e.pointer(picsie_core::editor::PointerSample {
            phase,
            point: Point::new(20., 20.),
            modifiers: Modifiers::default(),
        })
        .unwrap();
    }
    e.command(Command::UpdateText {
        patch: picsie_core::text::TextPatch {
            text: Some("word next\nlast line".into()),
            font_size: Some(20.),
            ..Default::default()
        },
    })
    .unwrap();
    let layer = e.selected().unwrap();
    let rect = picsie_core::text::caret(layer, 2);
    let p = geometry::to_world(layer, Point::new(rect.left as f64, rect.center_y() as f64));
    let (start, end) = picsie_core::text::unit_at(layer, p, false);
    assert_eq!((start, end), (0, 4));
    assert_eq!(picsie_core::text::unit_at(layer, p, true), (0, 10));
}
