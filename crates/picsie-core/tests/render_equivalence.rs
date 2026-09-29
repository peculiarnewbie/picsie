//! Local optimization regressions, not additional translated Compositor fixtures.
use picsie_core::{model::*, render::*};

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
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#38a7d9".into(),
        },
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#38a7d990".into(),
        },
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
