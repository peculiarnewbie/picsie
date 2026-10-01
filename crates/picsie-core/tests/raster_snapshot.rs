//! RasterSnapshotTests.swift, Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
//! Adapted to a 768×512 CPU fixture and Picsie's two persistence adapters.
use picsie_core::{
    asset::ImageAsset,
    editor::*,
    geometry::Viewport,
    model::*,
    raster_snapshot::{RasterPatch, RasterSnapshot},
    render::*,
};
use std::sync::Arc;

fn raster(layer: &Layer) -> Arc<RasterSnapshot> {
    match layer.content.as_ref() {
        Content::Image {
            data: ImageAsset::Tiled(r),
        } => r.clone(),
        _ => panic!("expected a tiled raster"),
    }
}
fn stroke(editor: &mut Editor, x: f64, y: f64) {
    editor
        .command(Command::Pointer {
            samples: vec![
                PointerSample {
                    phase: Phase::Down,
                    point: Point::new(x, y),
                    modifiers: Modifiers::default(),
                },
                PointerSample {
                    phase: Phase::Move,
                    point: Point::new(x + 180., y + 30.),
                    modifiers: Modifiers::default(),
                },
                PointerSample {
                    phase: Phase::Up,
                    point: Point::new(x + 180., y + 30.),
                    modifiers: Modifiers::default(),
                },
            ],
        })
        .unwrap();
}

#[test]
fn compositor_mouse_up_and_next_stroke_never_flatten_document() {
    let mut document = Document::new("Sparse brush", 768, 512).unwrap();
    document
        .layers
        .push(Layer::new("Paint", 768, 512, Content::Paint));
    let mut editor = Editor::new(document).unwrap();
    editor.viewport = Viewport {
        width: 768.,
        height: 512.,
        zoom: 1.,
        pan: Point::default(),
    };
    editor.tool = Tool::Brush;
    editor.brush_size = 80.;
    editor.brush_hardness = 0.;
    editor.brush_opacity = 0.5;
    stroke(&mut editor, 160., 240.);
    let first = raster(&editor.history.document.layers[0]);
    assert!(!first.has_materialized_pixels());
    let frozen = rgba_pixels(
        &Renderer::default()
            .render(&editor.history.document)
            .unwrap()
            .image_snapshot(),
    )
    .unwrap();
    assert!(!first.has_materialized_pixels());
    let count = editor.history.info().undo_count;
    stroke(&mut editor, 300., 260.);
    let second = raster(&editor.history.document.layers[0]);
    assert!(!first.has_materialized_pixels() && !second.has_materialized_pixels());
    assert_eq!(editor.history.info().undo_count, count + 1);
    editor.command(Command::Undo).unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &raster(&editor.history.document.layers[0])
    ));
    assert_eq!(
        frozen,
        rgba_pixels(
            &Renderer::default()
                .render(&editor.history.document)
                .unwrap()
                .image_snapshot()
        )
        .unwrap()
    );
    editor.command(Command::Redo).unwrap();
    assert!(Arc::ptr_eq(
        &second,
        &raster(&editor.history.document.layers[0])
    ));
    assert!(!first.has_materialized_pixels());
    let expected = rgba_pixels(
        &Renderer::default()
            .render(&editor.history.document)
            .unwrap()
            .image_snapshot(),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    for extension in ["picsie", "comp"] {
        let path = directory.path().join(format!("snapshot.{extension}"));
        picsie_core::files::save_project(&path, &editor.history.document).unwrap();
        let loaded = picsie_core::files::open_project(&path).unwrap();
        assert_eq!(
            expected,
            rgba_pixels(
                &Renderer::default()
                    .render(&loaded)
                    .unwrap()
                    .image_snapshot()
            )
            .unwrap()
        );
    }
}

/// Local regression: transparent replacements, grid shifts and later replacement
/// must preserve immutable old snapshots and keep patch rectangles disjoint.
#[test]
fn transparent_replacements_and_shifted_crops_preserve_immutable_pixels() {
    let base = ImageAsset::Raster(Arc::new(
        rgba_image(320, 200, &[60, 90, 120, 255].repeat(320 * 200)).unwrap(),
    ));
    let red = RasterPatch::new(
        skia_safe::IRect::from_xywh(240, 20, 80, 120),
        Arc::new(rgba_image(80, 120, &[255, 0, 0, 128].repeat(80 * 120)).unwrap()),
    );
    let red_id = red.image.unique_id() as usize;
    let first = Arc::new(
        RasterSnapshot::replacing(
            Some(&base),
            (320, 200),
            vec![red],
            skia_safe::IRect::from_xywh(-20, -10, 360, 220),
        )
        .unwrap(),
    );
    let transparent = RasterPatch::new(
        skia_safe::IRect::from_xywh(280, 50, 40, 40),
        Arc::new(rgba_image(40, 40, &vec![0; 40 * 40 * 4]).unwrap()),
    );
    let second = RasterSnapshot::replacing(
        Some(&ImageAsset::Tiled(first.clone())),
        (360, 220),
        vec![transparent],
        skia_safe::IRect::from_wh(360, 220),
    )
    .unwrap();
    assert!(!first.has_materialized_pixels() && !second.has_materialized_pixels());
    let retained = second
        .storage_parts()
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        retained[&red_id],
        80 * 120 * 4,
        "split patches retain and account for the complete original allocation once"
    );
    for (i, a) in second.patches.iter().enumerate() {
        for b in &second.patches[i + 1..] {
            assert!(skia_safe::IRect::intersect(&a.rect, &b.rect).is_none());
        }
    }
    let old = rgba_pixels(&first.image().unwrap()).unwrap();
    let new = rgba_pixels(&second.image().unwrap()).unwrap();
    let pixel = |p: &[u8], x: usize, y: usize| p[(y * 360 + x) * 4..(y * 360 + x) * 4 + 4].to_vec();
    assert_eq!(pixel(&old, 290, 60), vec![255, 0, 0, 128]);
    assert_eq!(pixel(&new, 290, 60), vec![0, 0, 0, 0]);
    assert_eq!(pixel(&new, 265, 60), vec![255, 0, 0, 128]);
    assert_eq!(pixel(&new, 50, 50), vec![60, 90, 120, 255]);
    assert_eq!(old, rgba_pixels(&first.image().unwrap()).unwrap());
}

/// Local regression for the hard-tip interior/edge shortcut at arbitrary phases.
/// The independent reference evaluates all four original hypot samples per pixel.
#[test]
fn hard_tip_arbitrary_phases_match_four_sample_reference() {
    use picsie_core::brush::{BrushStroke, Settings};
    for (diameter, point) in [
        (1., Point::new(131.173, 109.827)),
        (4., Point::new(129.9999999, 105.0000001)),
        (14., Point::new(128.125, 128.875)),
        (100., Point::new(131.173, 109.827)),
        (160., Point::new(128.0000001, 128.9999999)),
    ] {
        let mut doc = Document::new("Hard tip", 256, 256).unwrap();
        let mut brush = BrushStroke::new(
            Layer::new("Paint", 256, 256, Content::Paint),
            &doc,
            None,
            false,
            Settings {
                diameter,
                hardness: 1.,
                opacity: 1.,
                smoothing: 0.,
                erase: false,
                color: [255, 0, 0],
            },
            1.,
        )
        .unwrap();
        brush.input(point).unwrap();
        brush.finish().unwrap();
        doc.layers.push(brush.snapshot().unwrap());
        let pixels =
            rgba_pixels(&Renderer::default().render(&doc).unwrap().image_snapshot()).unwrap();
        for y in 0..256 {
            for x in 0..256 {
                let count = [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                    .into_iter()
                    .filter(|&(dx, dy)| {
                        Point::new(x as f64 + dx, y as f64 + dy).distance(point) <= diameter / 2.
                    })
                    .count();
                let alpha = (count as f64 / 4. * 255.).round() as u8;
                assert_eq!(
                    pixels[(y * 256 + x) * 4 + 3],
                    alpha,
                    "diameter {diameter}, pixel {x}/{y}"
                );
            }
        }
    }
}

/// Local regression against the pre-tile complete pixel-edit algorithm, including
/// feathered coverage, translucent pixels and rotated/nonuniform/flipped mapping.
#[test]
fn bounded_fill_and_clear_match_contiguous_reference_for_transforms() {
    for feather in [0., 12.] {
        for transformed in [false, true] {
            let mut doc = Document::new("Bounded edits", 400, 300).unwrap();
            let mut data = vec![];
            for y in 0..180 {
                for x in 0..260 {
                    data.extend_from_slice(&[
                        (x % 256) as u8,
                        (y % 256) as u8,
                        173,
                        if (x + y) % 3 == 0 { 137 } else { 255 },
                    ]);
                }
            }
            let mut layer = Layer::new(
                "Source",
                260,
                180,
                native_content(rgba_image(260, 180, &data).unwrap()),
            );
            layer.x = 27.;
            layer.y = 31.;
            if transformed {
                layer.rotation = 27.;
                layer.scale_x = 0.73;
                layer.scale_y = 1.19;
                layer.flip_x = true;
                layer.x = -13.25;
            }
            doc.layers.push(layer.clone());
            let mut path = skia_safe::PathBuilder::new();
            path.add_oval(skia_safe::Rect::from_xywh(30., 40., 150., 100.), None, None);
            let selection = picsie_core::pixel_selection::PixelSelection::from_path(
                400,
                300,
                path.detach(),
                feather,
            )
            .unwrap();
            let coverage = selection_coverage(&selection).unwrap();
            for clear in [false, true] {
                let reference = if clear {
                    let mut cleared = layer.clone();
                    cleared.content = Arc::new(native_content(
                        clear_selected_pixels(&layer, &selection).unwrap(),
                    ));
                    cleared
                } else {
                    let b = selection.coverage_bounds().unwrap();
                    let (mut filled, dx, dy) = expanded_layer(
                        &layer,
                        skia_safe::Rect::from_xywh(
                            b.x as f32,
                            b.y as f32,
                            b.width as f32,
                            b.height as f32,
                        ),
                    )
                    .unwrap();
                    let source = Renderer::default().layer_surface(&layer).unwrap();
                    let mut output = surface(filled.width, filled.height).unwrap();
                    output
                        .canvas()
                        .draw_image(source, (dx as f32, dy as f32), None);
                    let mut pixels = rgba_pixels(&output.image_snapshot()).unwrap();
                    for y in 0..filled.height {
                        for x in 0..filled.width {
                            let world = picsie_core::geometry::to_world(
                                &filled,
                                Point::new(x as f64 + 0.5, y as f64 + 0.5),
                            );
                            composite_pixel(
                                &mut pixels[((y * filled.width + x) * 4) as usize..][..4],
                                [229, 57, 53],
                                edit_coverage(&doc, Some(&coverage), world) as f64 / 255.,
                                false,
                            );
                        }
                    }
                    filled.content = Arc::new(native_content(
                        rgba_image(filled.width, filled.height, &pixels).unwrap(),
                    ));
                    filled
                };
                let mut editor = Editor::new(doc.clone()).unwrap();
                editor.history.pixel_selection = Some(selection.clone());
                editor.color = "#e53935".into();
                editor
                    .command(if clear {
                        Command::ClearSelectedPixels
                    } else {
                        Command::FillSelection
                    })
                    .unwrap();
                let mut expected = doc.clone();
                expected.layers[0] = reference;
                let pixels = |doc: &Document| {
                    rgba_pixels(&Renderer::default().render(doc).unwrap().image_snapshot()).unwrap()
                };
                assert_eq!(
                    pixels(&editor.history.document),
                    pixels(&expected),
                    "clear {clear}, transformed {transformed}, feather {feather}"
                );
            }
        }
    }
}

#[test]
fn encoded_source_retains_decoded_pixels_and_identity() {
    use base64::Engine as _;
    let image = rgba_image(32, 24, &[60, 90, 120, 137].repeat(32 * 24)).unwrap();
    let encoded = encode(&image, false).unwrap();
    let previous = decode(&encoded).unwrap();
    let asset = ImageAsset::from(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(encoded)
    ));
    let decoded = asset.image().unwrap();
    assert!(
        !decoded.is_lazy_generated(),
        "source pixels remain owned beyond Skia's decoder cache"
    );
    assert_eq!(
        decoded.unique_id(),
        asset.clone().image().unwrap().unique_id()
    );
    assert!(
        rgba_pixels(&previous).unwrap() == rgba_pixels(&decoded).unwrap(),
        "retained decode must match previous lazy PNG pixel contract"
    );
}

/// Local integration expansion of the earlier isolated bounded-feather experiment:
/// compare EVERY coverage byte with the original full-canvas Skia/RGBA path.
#[test]
fn bounded_feather_matches_full_canvas_for_edges_holes_inversion_and_ranges() {
    use picsie_core::pixel_selection::{PixelSelection, PixelSelectionMode};
    use skia_safe::{self as sk, Rect};
    for (w, h) in [(37, 23), (896, 640)] {
        for case in [
            "rectangle",
            "edge-ellipse",
            "full",
            "inverted",
            "off-canvas",
            "combined-hole",
            "empty",
        ] {
            let mut path = sk::PathBuilder::new();
            match case {
                "full" => {
                    path.add_rect(Rect::from_wh(w as f32, h as f32), None, None);
                }
                "edge-ellipse" => {
                    path.add_oval(
                        Rect::from_xywh(-3., 0., w as f32 / 4., h as f32 / 3.),
                        None,
                        None,
                    );
                }
                "off-canvas" => {
                    path.add_rect(
                        Rect::from_xywh(-10., -5., w as f32 / 3., h as f32 / 2.),
                        None,
                        None,
                    );
                }
                "empty" => {}
                _ => {
                    path.add_rect(
                        Rect::from_xywh(w as f32 / 4., h as f32 / 4., w as f32 / 3., h as f32 / 3.),
                        None,
                        None,
                    );
                }
            }
            let mut selected = PixelSelection::from_path(w, h, path.detach(), 0.).unwrap();
            if case == "inverted" {
                selected = selected.inverted().unwrap();
            }
            if case == "combined-hole" {
                let mut hole = sk::PathBuilder::new();
                hole.add_oval(
                    Rect::from_xywh(w as f32 / 3., h as f32 / 3., w as f32 / 8., h as f32 / 8.),
                    None,
                    None,
                );
                selected = selected
                    .combined(&hole.detach(), PixelSelectionMode::Subtract)
                    .unwrap();
            }
            for feather in [1., 2., 20., 80., 250.] {
                selected.feather = feather;
                let alpha = sk::ImageInfo::new(
                    (w as i32, h as i32),
                    sk::ColorType::Alpha8,
                    sk::AlphaType::Premul,
                    None,
                );
                let image = sk::images::raster_from_data(
                    &alpha,
                    sk::Data::new_copy(selected.pixels.as_slice()),
                    w as usize,
                )
                .unwrap();
                let mut paint = sk::Paint::default();
                paint.set_image_filter(
                    sk::image_filters::blur(
                        ((feather / 2.) as f32, (feather / 2.) as f32),
                        sk::TileMode::Clamp,
                        None,
                        None,
                    )
                    .unwrap(),
                );
                let mut output = surface(w, h).unwrap();
                output.canvas().draw_image(image, (0., 0.), Some(&paint));
                let full = rgba_pixels(&output.image_snapshot())
                    .unwrap()
                    .chunks_exact(4)
                    .map(|p| p[3])
                    .collect::<Vec<_>>();
                let bounded = selection_coverage(&selected).unwrap();
                let differences = full
                    .iter()
                    .zip(bounded.iter())
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(differences, 0, "{w}x{h} {case} feather {feather}");
            }
        }
    }
}
