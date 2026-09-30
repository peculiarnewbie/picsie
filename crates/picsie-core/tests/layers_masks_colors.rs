//! Fixtures translated from ColorPickerTests, BlendShortcutTests, LayerAppearanceTests,
//! SelectionTests, LayerMaskTests, MaskTransformTests, DistortTests (Compositor 609dbeae).
//! MIT © 2026 Wonder Assembly LLC. Extra cross-grid/alpha/transaction regressions labeled local.
use picsie_core::{
    editor::*,
    geometry::Viewport,
    model::*,
    pixel_selection::{PixelSelection, PixelSelectionMode},
    render::{self, Renderer},
};
use std::sync::Arc;
fn layer(w: u32, h: u32, color: &str) -> Layer {
    Layer::new(
        "Color",
        w,
        h,
        Content::Shape {
            shape: Shape::Rectangle,
            color: color.into(),
        },
    )
}
fn editor() -> Editor {
    let mut d = Document::new("Polish", 100, 100).unwrap();
    d.layers.push(layer(100, 100, "#ff0000"));
    let mut e = Editor::new(d).unwrap();
    e.viewport = Viewport {
        width: 100.,
        height: 100.,
        zoom: 1.,
        pan: Point::default(),
    };
    e
}
fn mask(pixels: Vec<u8>, w: u32, h: u32) -> Arc<LayerMask> {
    Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        raster: Some(Arc::new(MaskRaster {
            width: w,
            height: h,
            pixels: Arc::new(pixels),
        })),
        linked: true,
        placement: None,
        strokes: vec![],
    })
}
#[test]
fn compositor_palette_has_separate_background_and_mask_colors() {
    let mut e = editor();
    assert_eq!(e.color, "#000000");
    assert_eq!(e.background_color, "#ffffff");
    e.command(Command::SetPaletteColor {
        color: "#112233".into(),
        background: false,
    })
    .unwrap();
    e.command(Command::SetPaletteColor {
        color: "#aabbcc".into(),
        background: true,
    })
    .unwrap();
    e.command(Command::SwapPaletteColors).unwrap();
    assert_eq!(e.color, "#aabbcc");
    assert_eq!(e.background_color, "#112233");
    e.command(Command::AddMask {
        base: MaskMode::Reveal,
    })
    .unwrap();
    let original = e.color.clone();
    e.command(Command::ResetPaletteColors).unwrap();
    assert_eq!(e.mask_mode, MaskMode::Hide);
    e.command(Command::SetPaletteColor {
        color: "#000000".into(),
        background: true,
    })
    .unwrap();
    assert_eq!(e.mask_mode, MaskMode::Reveal);
    assert_eq!(e.color, original);
    e.command(Command::SetPaintTarget {
        target: PaintTarget::Content,
    })
    .unwrap();
    e.command(Command::ResetPaletteColors).unwrap();
    assert_eq!(e.color, "#000000");
    assert_eq!(e.background_color, "#ffffff");
}
#[test]
fn compositor_palette_only_changes_open_text_and_picker_commit_keeps_draft_identity() {
    let mut e = editor();
    e.command(Command::SetTool { tool: Tool::Text }).unwrap();
    e.command(Command::SetTextColor {
        color: "#00ff00".into(),
        draft_id: None,
    })
    .unwrap();
    assert_eq!(e.color, "#00ff00");
    assert_eq!(e.history.info().undo_count, 0);
    e.command(Command::Pointer {
        samples: vec![
            PointerSample {
                phase: Phase::Down,
                point: Point::new(10., 10.),
                modifiers: Modifiers::default(),
            },
            PointerSample {
                phase: Phase::Up,
                point: Point::new(10., 10.),
                modifiers: Modifiers::default(),
            },
        ],
    })
    .unwrap();
    assert!(e.text_editing());
    let id = e.selected_id().unwrap().to_owned();
    e.command(Command::SetPaletteColor {
        color: "#123456".into(),
        background: false,
    })
    .unwrap();
    assert!(e.text_editing());
    assert!(
        matches!(e.selected().unwrap().content.as_ref(),Content::Text{color,..} if color=="#123456")
    );
    e.command(Command::SetTextColor {
        color: "#ffffff".into(),
        draft_id: Some("stale".into()),
    })
    .unwrap();
    assert_eq!(e.color, "#123456");
    e.command(Command::SetTextColor {
        color: "#ffffff".into(),
        draft_id: Some(id),
    })
    .unwrap();
    assert_eq!(e.color, "#ffffff");
}
#[test]
fn compositor_blend_order_wraps_and_undoes() {
    let mut e = editor();
    e.command(Command::CycleBlendMode { forward: true })
        .unwrap();
    assert_eq!(e.selected().unwrap().blend, Blend::Darken);
    e.command(Command::CycleBlendMode { forward: false })
        .unwrap();
    e.command(Command::CycleBlendMode { forward: false })
        .unwrap();
    assert_eq!(e.selected().unwrap().blend, Blend::Luminosity);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.selected().unwrap().blend, Blend::SourceOver);
    assert_eq!(Blend::ALL.len(), 24);
}
#[test]
fn compositor_known_srgb_blend_pixels_and_local_extra_modes() {
    let mut d = Document::new("Blend", 4, 4).unwrap();
    d.layers = vec![layer(4, 4, "#666666"), layer(4, 4, "#cccccc")];
    let cases = [
        (Blend::SourceOver, 0.8),
        (Blend::Multiply, 0.32),
        (Blend::Screen, 0.88),
        (Blend::Overlay, 0.64),
        (Blend::Darken, 0.4),
        (Blend::Lighten, 0.8),
        (Blend::Difference, 0.4),
        (Blend::ColorDodge, 1.),
        (Blend::ColorBurn, 0.25),
        (Blend::LinearBurn, 0.2),
        (Blend::LinearDodge, 1.),
        (Blend::VividLight, 1.),
        (Blend::LinearLight, 1.),
        (Blend::PinLight, 0.6),
        (Blend::HardMix, 1.),
        (Blend::Subtract, 0.),
        (Blend::Divide, 0.5),
    ];
    for (mode, value) in cases {
        d.layers[1].blend = mode;
        let image = Renderer::default().render(&d).unwrap().image_snapshot();
        let pixels = render::rgba_pixels(&image).unwrap();
        assert!(
            (pixels[0] as f64 / 255. - value).abs() < 0.02,
            "{mode:?}: {} expected {value}",
            pixels[0]
        );
        assert_eq!(pixels[3], 255);
    }
    // Local: soft source alpha must interpolate the blended result, never harden edges.
    d.layers[1].blend = Blend::ColorDodge;
    d.layers[1].opacity = 0.5;
    let p = render::rgba_pixels(&Renderer::default().render(&d).unwrap().image_snapshot()).unwrap();
    assert!((p[0] as i32 - 179).abs() <= 2);
}
#[test]
fn compositor_visibility_swipe_is_one_undo_without_changing_selection() {
    let mut e = editor();
    e.command(Command::AddPaintLayer).unwrap();
    let ids = e
        .history
        .document
        .layers
        .iter()
        .map(|l| l.id.clone())
        .collect::<Vec<_>>();
    let selection = e.selection.clone();
    let n = e.history.info().undo_count;
    e.command(Command::BeginVisibilitySwipe { id: ids[0].clone() })
        .unwrap();
    e.command(Command::SwipeVisibility { id: ids[1].clone() })
        .unwrap();
    e.command(Command::SwipeVisibility { id: ids[0].clone() })
        .unwrap();
    e.command(Command::EndVisibilitySwipe).unwrap();
    assert!(e.history.document.layers.iter().all(|l| !l.visible));
    assert_eq!(e.selection, selection);
    assert_eq!(e.history.info().undo_count, n + 1);
    e.command(Command::Undo).unwrap();
    assert!(e.history.document.layers.iter().all(|l| l.visible));
}
#[test]
fn compositor_mask_thumbnail_black_hole_add_subtract_and_layer_alpha() {
    let mut e = editor();
    let id = e.selected_id().unwrap().to_owned();
    let mut pixels = vec![255; 10000];
    for y in 30..70 {
        for x in 20..60 {
            pixels[y * 100 + x] = 0;
        }
    }
    for y in 40..50 {
        for x in 30..40 {
            pixels[y * 100 + x] = 255;
        }
    }
    e.history.document.layers[0].mask = Some(mask(pixels, 100, 100));
    e.command(Command::LoadThumbnailSelection {
        id: id.clone(),
        mask: true,
        mode: PixelSelectionMode::Replace,
    })
    .unwrap();
    let sel = e.history.pixel_selection.as_ref().unwrap();
    assert_eq!(sel.at(25, 35), 255);
    assert_eq!(sel.at(35, 45), 0);
    assert_eq!(sel.at(59, 69), 255);
    assert_eq!(sel.at(60, 70), 0);
    e.history.pixel_selection = Some(PixelSelection::all(100, 100).unwrap());
    e.command(Command::LoadThumbnailSelection {
        id: id.clone(),
        mask: true,
        mode: PixelSelectionMode::Subtract,
    })
    .unwrap();
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(25, 35), 0);
    e.command(Command::LoadThumbnailSelection {
        id,
        mask: false,
        mode: PixelSelectionMode::Replace,
    })
    .unwrap();
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(25, 35), 255); // Mask ignored.
}
#[test]
fn local_mask_copy_preserves_document_size_across_different_grids_and_undo() {
    let mut e = editor();
    let source = e.selected_id().unwrap().to_owned();
    e.history.document.layers[0].mask = Some(mask(vec![0, 255, 255, 0], 2, 2));
    e.add_layers(vec![layer(50, 25, "#0000ff")]).unwrap();
    let target = e.selected_id().unwrap().to_owned();
    let n = e.history.info().undo_count;
    e.command(Command::CopyMask {
        source_id: source.clone(),
        target_id: target.clone(),
    })
    .unwrap();
    let target_layer = e.selected().unwrap();
    let placed = target_layer
        .mask
        .as_ref()
        .unwrap()
        .placement
        .unwrap()
        .as_layer(target_layer);
    assert_eq!(placed.width as f64 * placed.scale_x, 100.);
    assert_eq!(placed.height as f64 * placed.scale_y, 100.);
    assert_eq!(e.paint_target, PaintTarget::Mask);
    assert_eq!(e.history.info().undo_count, n + 1);
    e.command(Command::Undo).unwrap();
    assert!(
        e.history
            .document
            .layers
            .iter()
            .find(|l| l.id == target)
            .unwrap()
            .mask
            .is_none()
    );
    assert!(e.history.document.layers[0].mask.is_some());
}
#[test]
fn compositor_mask_transform_handles_nudge_and_cancel_leave_pixels_intact() {
    let mut e = editor();
    e.history.document.layers[0].mask = Some(mask(vec![0, 255, 255, 0], 2, 2));
    let pixels = e.selected().unwrap().mask.as_ref().unwrap().raster.clone();
    e.command(Command::ToggleMaskLink).unwrap();
    e.command(Command::SetPaintTarget {
        target: PaintTarget::Mask,
    })
    .unwrap();
    e.command(Command::BeginTransform).unwrap();
    e.command(Command::Nudge {
        delta: Point::new(10., 5.),
    })
    .unwrap();
    assert_eq!(e.selected().unwrap().x, 0.);
    assert_eq!(e.independent_mask_layer().unwrap().x, 10.);
    assert_eq!(
        e.cursor_map().handles.unwrap().points[0],
        Point::new(10., 5.)
    );
    assert_eq!(e.selected().unwrap().mask.as_ref().unwrap().raster, pixels);
    e.command(Command::CancelTransform).unwrap();
    assert!(
        e.selected()
            .unwrap()
            .mask
            .as_ref()
            .unwrap()
            .placement
            .is_none()
    );
}
#[test]
fn compositor_distortion_geometry_convex_folded_and_degenerate() {
    let c = [
        Point::new(10., 10.),
        Point::new(60., 10.),
        Point::new(30., 30.),
        Point::new(10., 30.),
    ];
    let map = picsie_core::distort::homography(&c);
    for (p, corner) in [(0., 0.), (1., 0.), (1., 1.), (0., 1.)].into_iter().zip(c) {
        let mapped = map.map_point(p);
        assert!(
            (mapped.x as f64 - corner.x).abs() < 1e-5 && (mapped.y as f64 - corner.y).abs() < 1e-5
        );
    }
    assert!(picsie_core::distort::convex(&c));
    assert!(picsie_core::distort::usable(&[c[0], c[2], c[1], c[3]]));
    assert!(!picsie_core::distort::convex(&[c[0], c[2], c[1], c[3]]));
    assert!(!picsie_core::distort::usable(&[c[0], c[0], c[2], c[3]]));
}
#[test]
fn local_mask_distortion_preview_cancel_apply_and_one_undo() {
    let mut e = editor();
    let mut pixels = vec![255; 10000];
    for y in 20..80 {
        for x in 20..80 {
            pixels[y * 100 + x] = 0;
        }
    }
    e.history.document.layers[0].mask = Some(mask(pixels, 100, 100));
    e.command(Command::ToggleMaskLink).unwrap();
    e.command(Command::SetPaintTarget {
        target: PaintTarget::Mask,
    })
    .unwrap();
    let before = e.history.document.clone();
    let n = e.history.info().undo_count;
    let c = [
        Point::new(10., 10.),
        Point::new(90., 0.),
        Point::new(70., 90.),
        Point::new(0., 100.),
    ];
    e.command(Command::DistortMask { corners: c }).unwrap();
    assert!(e.transform_active());
    assert_eq!(e.mask_distortion_corners(), Some(c));
    e.command(Command::CancelTransform).unwrap();
    assert_eq!(e.history.document, before);
    e.command(Command::DistortMask { corners: c }).unwrap();
    e.command(Command::CommitTransform).unwrap();
    assert!(!e.transform_active());
    assert_eq!(e.history.info().undo_count, n + 1);
    assert_eq!(e.selected().unwrap().x, 0.);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.history.document, before);
}
#[test]
fn local_duplicate_drop_is_one_undo_and_moves_copy_only() {
    let mut e = editor();
    e.command(Command::AddPaintLayer).unwrap();
    let target = e.selected_id().unwrap().to_owned();
    let original = e.history.document.layers[0].id.clone();
    e.command(Command::Select {
        id: Some(original.clone()),
        mode: SelectionMode::Replace,
    })
    .unwrap();
    let n = e.history.info().undo_count;
    e.command(Command::DuplicateTo {
        target_id: target,
        side: Side::Above,
        into: false,
    })
    .unwrap();
    assert_eq!(e.history.document.layers.len(), 3);
    assert_ne!(e.selected_id(), Some(original.as_str()));
    assert_eq!(e.history.info().undo_count, n + 1);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.history.document.layers.len(), 2);
}

#[test]
fn local_blend_hover_is_transient_and_does_not_mutate_export_or_history() {
    let mut e = editor();
    let before = e.history.document.clone();
    let count = e.history.info().undo_count;
    e.command(Command::PreviewBlendMode {
        id: e.selected_id().map(str::to_owned),
        mode: Some(Blend::VividLight),
    })
    .unwrap();
    assert_eq!(e.preview_document().layers[0].blend, Blend::VividLight);
    assert_eq!(e.history.document, before);
    assert_eq!(e.history.info().undo_count, count);
    e.command(Command::PreviewBlendMode {
        id: None,
        mode: None,
    })
    .unwrap();
    assert_eq!(e.preview_document(), before);
}
#[test]
fn compositor_uniform_and_folded_masks_preserve_background() {
    let mut l = layer(100, 100, "#ff0000");
    l.mask = Some(mask(vec![255], 1, 1));
    let c = [
        Point::new(10., 10.),
        Point::new(90., 10.),
        Point::new(10., 90.),
        Point::new(90., 90.),
    ];
    let (uniform, placement) =
        picsie_core::distort::mask(&l, MaskPlacement::of(&l), &c, None).unwrap();
    assert_eq!((uniform.width, uniform.height), (1, 1));
    assert_eq!(uniform.pixels.as_slice(), &[255]);
    assert_eq!((placement.x, placement.scale_x), (10., 0.8));
    let mut pixels = vec![255; 10000];
    for y in 20..80 {
        for x in 20..80 {
            pixels[y * 100 + x] = 0;
        }
    }
    l.mask = Some(mask(pixels, 100, 100));
    let (folded, _) = picsie_core::distort::mask(&l, MaskPlacement::of(&l), &c, None).unwrap();
    assert!(folded.pixels.iter().any(|v| *v < 10));
    assert_eq!(folded.pixels[0], 255);
    assert!(folded.pixels.iter().any(|v| *v == 255));
}
#[test]
fn local_clipping_boundary_keeps_selection_and_collapsed_range_skips_children() {
    let mut e = editor();
    let first = e.selected_id().unwrap().to_owned();
    e.command(Command::AddPaintLayer).unwrap();
    let top = e.selected_id().unwrap().to_owned();
    e.command(Command::Select {
        id: Some(first.clone()),
        mode: SelectionMode::Replace,
    })
    .unwrap();
    let count = e.history.info().undo_count;
    e.command(Command::ToggleClippingFor { id: top.clone() })
        .unwrap();
    assert_eq!(e.selected_id(), Some(first.as_str()));
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(
        e.history
            .document
            .layers
            .iter()
            .find(|l| l.id == top)
            .unwrap()
            .mask_source_id
            .as_deref(),
        Some(first.as_str())
    );
    e.command(Command::Undo).unwrap();
    e.command(Command::Select {
        id: Some(top.clone()),
        mode: SelectionMode::Toggle,
    })
    .unwrap();
    e.command(Command::GroupSelected).unwrap();
    let folder = e.selected_id().unwrap().to_owned();
    e.command(Command::ToggleGroupExpansion { id: folder.clone() })
        .unwrap();
    e.command(Command::AddPaintLayer).unwrap();
    let outside = e.selected_id().unwrap().to_owned();
    e.command(Command::Select {
        id: Some(folder.clone()),
        mode: SelectionMode::Range,
    })
    .unwrap();
    assert!(e.selection.ids.contains(&folder));
    assert!(e.selection.ids.contains(&outside));
    assert!(!e.selection.ids.contains(&first));
    assert!(!e.selection.ids.contains(&top));
}

#[test]
fn local_folder_mask_placement_is_used_for_child_compositing() {
    let mut e = editor();
    e.command(Command::GroupSelected).unwrap();
    let folder = e.selected_id().unwrap().to_owned();
    let mut pixels = vec![255; 10000];
    for y in 30..70 {
        for x in 30..70 {
            pixels[y * 100 + x] = 0;
        }
    }
    e.history
        .document
        .layers
        .iter_mut()
        .find(|l| l.id == folder)
        .unwrap()
        .mask = Some(mask(pixels, 100, 100));
    e.command(Command::SetPaintTarget {
        target: PaintTarget::Mask,
    })
    .unwrap();
    e.command(Command::ToggleMaskLink).unwrap();
    e.command(Command::SetMaskPlacement {
        placement: MaskPlacement {
            x: 10.,
            y: 0.,
            scale_x: 1.,
            scale_y: 1.,
            rotation: 0.,
            flip_x: false,
            flip_y: false,
        },
    })
    .unwrap();
    let image = Renderer::default()
        .render(&e.history.document)
        .unwrap()
        .image_snapshot();
    let pixels = render::rgba_pixels(&image).unwrap();
    assert_eq!(pixels[(50 * 100 + 35) * 4 + 3], 255);
    assert_eq!(pixels[(50 * 100 + 50) * 4 + 3], 0);
}
#[test]
fn compositor_projects_support_more_than_the_prototype_hundred_layers() {
    let mut d = Document::new("Layer limit", 1, 1).unwrap();
    d.layers = (0..101).map(|_| layer(1, 1, "#ff0000")).collect();
    let mut e = Editor::new(d).unwrap();
    e.command(Command::Duplicate).unwrap();
    assert_eq!(e.history.document.layers.len(), 102);
    e.history.document.validate().unwrap();
    e.command(Command::Undo).unwrap();
    assert_eq!(e.history.document.layers.len(), 101);
    assert_eq!(picsie_core::model::MAX_LAYERS, 10_000);
}

#[test]
fn local_all_source_blend_modes_round_trip_through_compositor_packages() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("blends.comp");
    let mut document = Document::new("All source modes", 1, 1).unwrap();
    document.layers = Blend::ALL
        .into_iter()
        .map(|mode| {
            let mut l = layer(1, 1, "#6680cc");
            l.blend = mode;
            l
        })
        .collect();
    picsie_core::comp::save(&path, &document).unwrap();
    let reopened = picsie_core::comp::open(&path).unwrap();
    assert_eq!(
        reopened.layers.iter().map(|l| l.blend).collect::<Vec<_>>(),
        Blend::ALL
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["layers"][0]["blendMode"], "Normal");
    assert_eq!(manifest["layers"][8]["blendMode"], "Linear Dodge (Add)");
}

#[test]
fn compositor_background_fill_preserves_text_and_mask_palette_opposites() {
    let mut e = editor();
    e.command(Command::SetPaletteColor {
        color: "#00ff00".into(),
        background: true,
    })
    .unwrap();
    e.command(Command::FillBackground).unwrap();
    let image = Renderer::default()
        .render(&e.history.document)
        .unwrap()
        .image_snapshot();
    assert_eq!(
        &render::rgba_pixels(&image).unwrap()[..4],
        &[0, 255, 0, 255]
    );
    e.command(Command::AddMask {
        base: MaskMode::Hide,
    })
    .unwrap();
    e.command(Command::ResetPaletteColors).unwrap();
    e.command(Command::FillBackground).unwrap();
    let image = Renderer::default()
        .render(&e.history.document)
        .unwrap()
        .image_snapshot();
    assert_eq!(render::rgba_pixels(&image).unwrap()[3], 255);
    e.command(Command::SetPaintTarget {
        target: PaintTarget::Content,
    })
    .unwrap();
    e.history.document.layers[0].content = Arc::new(Content::Text {
        text: "Editable".into(),
        color: "#ffffff".into(),
        font_size: 16.,
        font_family: FontFamily::SansSerif,
    });
    e.command(Command::FillBackground).unwrap();
    assert!(
        matches!(e.selected().unwrap().content.as_ref(),Content::Text { text, color, .. } if text == "Editable" && color == "#00ff00")
    );
}
#[test]
fn local_mask_chain_keeps_active_layer_and_undo() {
    let mut e = editor();
    let id = e.selected_id().unwrap().to_owned();
    e.command(Command::AddMask {
        base: MaskMode::Reveal,
    })
    .unwrap();
    e.command(Command::AddPaintLayer).unwrap();
    let active = e.selected_id().unwrap().to_owned();
    let count = e.history.info().undo_count;
    e.command(Command::ToggleMaskLinkFor { id: id.clone() })
        .unwrap();
    assert_eq!(e.selected_id(), Some(active.as_str()));
    assert!(!e.history.document.layers[0].mask.as_ref().unwrap().linked);
    assert_eq!(e.history.info().undo_count, count + 1);
    e.command(Command::Undo).unwrap();
    assert!(e.history.document.layers[0].mask.as_ref().unwrap().linked);
}

#[test]
fn local_single_pixel_sampling_matches_full_composite_with_blends_masks_and_alpha() {
    let mut d = Document::new("Sampling", 32, 32).unwrap();
    let bottom = layer(32, 32, "#337faa");
    let mut top = layer(16, 16, "#e43d67");
    top.x = 8.;
    top.y = 8.;
    top.rotation = 12.;
    top.opacity = 0.6;
    let mut pixels = vec![255; 256];
    for y in 5..11 {
        for x in 5..11 {
            pixels[y * 16 + x] = 64;
        }
    }
    top.mask = Some(mask(pixels, 16, 16));
    d.layers = vec![bottom, top];
    for blend in Blend::ALL {
        d.layers[1].blend = blend;
        let mut renderer = Renderer::default();
        let full = render::rgba_pixels(&renderer.render(&d).unwrap().image_snapshot()).unwrap();
        for (x, y) in [(0, 0), (8, 8), (16, 16), (23, 23), (31, 31)] {
            let sample = renderer
                .sample(&d, Point::new(x as f64 + 0.3, y as f64 + 0.8))
                .unwrap();
            assert_eq!(
                &sample,
                &full[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4],
                "{blend:?} at {x},{y}"
            );
        }
        assert!(renderer.sample(&d, Point::new(-1., 0.)).is_err());
    }
}
