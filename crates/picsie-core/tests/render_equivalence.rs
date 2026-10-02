//! Local optimization regressions, not additional translated Compositor fixtures.
use picsie_core::{model::*, render::*};
use std::sync::Arc;

#[test]
fn native_bgra_matches_conversion_for_alpha_format_padding_and_color_space() {
    use skia_safe::{AlphaType, Color, ColorSpace, ColorType, ImageInfo, surfaces};
    for alpha in [0, 63, 128, 254, 255] {
        for color_type in [ColorType::BGRA8888, ColorType::RGBA8888] {
            for padding in [0, 16] {
                for color_space in [None, Some(ColorSpace::new_srgb_linear())] {
                    let info = ImageInfo::new((17, 13), color_type, AlphaType::Premul, color_space);
                    let row_bytes = 17 * 4 + padding;
                    let mut storage = vec![0; row_bytes * 13];
                    let mut s =
                        surfaces::wrap_pixels(&info, &mut storage, row_bytes, None).unwrap();
                    s.canvas().clear(Color::from_argb(alpha, 231, 93, 27));
                    let desired =
                        ImageInfo::new((17, 13), ColorType::BGRA8888, AlphaType::Unpremul, None);
                    let mut expected = vec![0; 17 * 13 * 4];
                    assert!(s.read_pixels(&desired, &mut expected, 17 * 4, (0, 0)));
                    let actual = bgra_pixels(&mut s).unwrap();
                    assert_eq!(
                        actual, expected,
                        "alpha={alpha}, format={color_type:?}, padding={padding}"
                    );
                    if alpha == 255 && info.color_space().is_none() {
                        assert_eq!(&actual[..4], &[27, 93, 231, 255]);
                    }
                }
            }
        }
    }
}

fn assert_neutral_matches_identity(doc: &Document) {
    let fast = Renderer::default().render(doc).unwrap().image_snapshot();
    let mut reference = doc.clone();
    for layer in &mut reference.layers {
        layer.brightness = 1. + f64::EPSILON;
    }
    let reference = Renderer::default()
        .render(&reference)
        .unwrap()
        .image_snapshot();
    let fast = rgba_pixels(&fast).unwrap();
    let reference = rgba_pixels(&reference).unwrap();
    let first_difference = fast.iter().zip(&reference).position(|(a, b)| a != b);
    assert!(
        first_difference.is_none(),
        "first differing byte: {first_difference:?}; layers: {:?}",
        doc.layers
    );
}

#[test]
fn opaque_fills_preserve_exact_pixels_and_fractional_draws_keep_the_old_path() {
    for content in [
        Content::Gradient {
            from: "#123456".into(),
            to: "#efd531ff".into(),
        },
        Content::Gradient {
            from: "#12345680".into(),
            to: "#efd531".into(),
        },
        Content::shape(Shape::Rectangle, "#38a7d9".into()),
        Content::shape(Shape::Rectangle, "#38a7d990".into()),
    ] {
        for x in [-7., 0., 13., 13.5] {
            for opacity in [1., 0.63] {
                for transformed in [false, true] {
                    let mut doc = Document::new("Fill equivalence", 64, 64).unwrap();
                    doc.layers.push(Layer::new(
                        "Backdrop",
                        64,
                        64,
                        Content::Gradient {
                            from: "#aa148977".into(),
                            to: "#13f981ba".into(),
                        },
                    ));
                    let mut layer = Layer::new("Fill", 31, 27, content.clone());
                    layer.x = x;
                    layer.y = 5.;
                    layer.opacity = opacity;
                    if transformed {
                        layer.rotation = 31.;
                        layer.scale_x = 0.8;
                        layer.flip_x = true;
                    }
                    doc.layers.push(layer);
                    assert_neutral_matches_identity(&doc);
                }
            }
        }
    }
}

#[test]
fn neutral_appearance_matches_the_identity_filter_for_alpha_and_transforms() {
    let pixels: Vec<u8> = (0u32..1024)
        .flat_map(|i| [(i * 37) as u8, (i * 73) as u8, (i * 19) as u8, i as u8])
        .collect();
    let source = native_content(rgba_image(32, 32, &pixels).unwrap());
    for backdrop in [false, true] {
        for transformed in [false, true] {
            for blur in [0., 1.5] {
                for sampling in [Sampling::Nearest, Sampling::Smooth, Sampling::High] {
                    for blend in [
                        Blend::SourceOver,
                        Blend::Multiply,
                        Blend::Screen,
                        Blend::Overlay,
                        Blend::Darken,
                        Blend::Lighten,
                        Blend::SoftLight,
                        Blend::HardLight,
                        Blend::Difference,
                        Blend::Exclusion,
                        Blend::ColorDodge,
                        Blend::ColorBurn,
                        Blend::Hue,
                        Blend::Saturation,
                        Blend::Color,
                        Blend::Luminosity,
                    ] {
                        let mut doc = Document::new("Neutral appearance", 64, 64).unwrap();
                        if backdrop {
                            doc.layers.push(Layer::new(
                                "Backdrop",
                                64,
                                64,
                                Content::Gradient {
                                    from: "#132789".into(),
                                    to: "#efd531".into(),
                                },
                            ));
                        }
                        let mut layer = Layer::new("Alpha ramp", 32, 32, source.clone());
                        layer.x = 13.;
                        layer.y = 11.;
                        layer.opacity = 0.63;
                        layer.blend = blend;
                        layer.blur = blur;
                        layer.sampling = sampling;
                        if transformed {
                            layer.rotation = 17.;
                            layer.scale_x = 1.2;
                            layer.scale_y = 0.8;
                            layer.flip_x = true;
                        }
                        doc.layers.push(layer);
                        let fast = Renderer::default().render(&doc).unwrap().image_snapshot();
                        // The prior path always applied a Skia color matrix. A non-neutral
                        // f64 that rounds to 1.0f32 exercises that same identity matrix,
                        // independently of the neutral fast path, with unchanged pixels.
                        for layer in &mut doc.layers {
                            layer.brightness = 1. + f64::EPSILON;
                        }
                        let reference = Renderer::default().render(&doc).unwrap().image_snapshot();
                        let fast = rgba_pixels(&fast).unwrap();
                        let reference = rgba_pixels(&reference).unwrap();
                        let first_difference =
                            fast.iter().zip(&reference).position(|(a, b)| a != b);
                        assert!(
                            first_difference.is_none(),
                            "pixel byte {first_difference:?}: backdrop={backdrop}, transformed={transformed}, blur={blur}, sampling={sampling:?}, blend={blend:?}"
                        );
                    }
                }
            }
        }
    }
}

fn assert_cached_preview_matches_full(
    renderer: &mut Renderer,
    doc: &Document,
    viewport: &picsie_core::geometry::Viewport,
    show_handles: bool,
) {
    let selection = vec![doc.layers[0].id.clone()];
    let mut actual = renderer
        .preview(
            doc,
            viewport,
            &selection,
            show_handles,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    // A new renderer has no retained document and must execute the full path.
    let mut expected = Renderer::default()
        .preview(
            doc,
            viewport,
            &selection,
            show_handles,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let actual = rgba_pixels(&actual.image_snapshot()).unwrap();
    let expected = rgba_pixels(&expected.image_snapshot()).unwrap();
    let difference = actual.iter().zip(&expected).position(|(a, b)| a != b);
    assert!(
        difference.is_none(),
        "cached preview diverged at byte {difference:?}: {:?}, viewport={viewport:?}",
        doc.layers
    );
}

#[test]
fn retained_preview_matches_full_after_moves_and_viewport_or_overlay_changes() {
    let pixels: Vec<u8> = (0u32..32 * 29)
        .flat_map(|i| [(i * 37) as u8, (i * 73) as u8, (i * 19) as u8, i as u8])
        .collect();
    for content in [
        Content::shape(Shape::Ellipse, "#6587ff80".into()),
        Content::Gradient {
            from: "#12345680".into(),
            to: "#efd531".into(),
        },
        native_content(rgba_image(32, 29, &pixels).unwrap()),
    ] {
        for background in [false, true] {
            for blend in [
                Blend::SourceOver,
                Blend::Multiply,
                Blend::Screen,
                Blend::Overlay,
                Blend::Darken,
                Blend::Lighten,
                Blend::SoftLight,
                Blend::HardLight,
                Blend::Difference,
                Blend::Exclusion,
                Blend::ColorDodge,
                Blend::ColorBurn,
                Blend::Hue,
                Blend::Saturation,
                Blend::Color,
                Blend::Luminosity,
            ] {
                for sampling in [Sampling::Nearest, Sampling::Smooth, Sampling::High] {
                    let mut doc = Document::new("Damage equivalence", 96, 80).unwrap();
                    let mut moved = Layer::new("Moving", 32, 29, content.clone());
                    moved.blend = blend;
                    moved.sampling = sampling;
                    moved.opacity = 0.63;
                    doc.layers.push(moved);
                    if background {
                        doc.layers.insert(
                            0,
                            Layer::new(
                                "Backdrop",
                                96,
                                80,
                                Content::Gradient {
                                    from: "#aa148977".into(),
                                    to: "#13f981ba".into(),
                                },
                            ),
                        );
                    }
                    let moving_index = usize::from(background);
                    let mut overlap = Layer::new(
                        "Overlap",
                        30,
                        27,
                        Content::shape(Shape::Ellipse, "#ef638280".into()),
                    );
                    overlap.x = 34.;
                    overlap.y = 9.;
                    overlap.blend = Blend::Screen;
                    doc.layers.push(overlap);
                    let mut renderer = Renderer::default();
                    let mut viewport = picsie_core::geometry::Viewport {
                        width: 128.,
                        height: 110.,
                        zoom: 0.73,
                        pan: Point::new(13., 7.),
                    };
                    for (x, y) in [
                        (20., 19.),
                        (21., 19.),
                        (25.5, 9.125),
                        (-15.25, 0.),
                        (-200., 150.),
                        (200., 150.),
                        (20., 19.),
                    ] {
                        doc.layers[moving_index].x = x;
                        doc.layers[moving_index].y = y;
                        assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, false);
                    }
                    // These changes reuse the composite but repaint the viewport/overlay.
                    viewport.pan = Point::new(-6., -10.);
                    viewport.zoom = 1.27;
                    assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, true);
                    // A stationary transformed layer can cross the partial redraw clip.
                    let top = doc.layers.last_mut().unwrap();
                    top.rotation = 17.;
                    top.scale_x = 1.2;
                    top.scale_y = 0.8;
                    assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, false);
                    doc.layers[moving_index].x += 2.5;
                    assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, false);
                }
            }
        }
    }
}

#[test]
fn retained_preview_invalidates_for_content_effects_masks_hierarchy_and_history() {
    let mut doc = demo_document();
    let viewport = picsie_core::geometry::Viewport {
        width: 320.,
        height: 240.,
        zoom: 0.23,
        pan: Point::default(),
    };
    let mut renderer = Renderer::default();
    assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, true);
    let before = doc.clone();
    doc.layers[1].x += 9.25;
    assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, true);
    for change in 0..10 {
        match change {
            0 => doc.layers[1].blur = 2.,
            1 => doc.layers[1].rotation = 17.,
            2 => doc.layers[1].content = Arc::new(Content::Paint),
            3 => {
                doc.layers[1].mask = Some(Arc::new(LayerMask {
                    enabled: true,
                    base: MaskMode::Hide,
                    raster: Some(Arc::new(MaskRaster::solid(false))),
                    linked: false,
                    placement: Some(MaskPlacement::of(&doc.layers[1])),
                    strokes: vec![],
                }));
            }
            4 => doc.layers[1].visible = false,
            5 => doc.layers.swap(1, 2),
            6 => {
                let group = Layer::new("Group", 1, 1, Content::Group);
                doc.layers[1].parent_id = Some(group.id.clone());
                doc.layers.insert(0, group);
            }
            7 => doc.width = 1100,
            8 => {
                doc.layers.remove(1);
            }
            _ => doc = before.clone(), // Undo to retained native source identities.
        };
        assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, true);
    }
    // Live source changes can invalidate another layer even if its own pixels are unchanged.
    doc.layers[2].mask_source_id = Some(doc.layers[1].id.clone());
    assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, true);
    doc.layers[1].x += 11.;
    assert_cached_preview_matches_full(&mut renderer, &doc, &viewport, true);
}
