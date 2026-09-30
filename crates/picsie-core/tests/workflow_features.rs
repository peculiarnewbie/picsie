//! Selected pinned Compositor fixtures, MIT © 2026 Wonder Assembly LLC, 609dbeae.
//! `compositor_*` tests translate the named Swift scenarios; `local_*` are regressions.
use picsie_core::{
    editor::*,
    files,
    geometry::Viewport,
    image_size::ImageSizeOptions,
    model::*,
    pixel_selection::*,
    render::*,
    wand::{self, WandSettings},
};
use std::sync::Arc;
fn session(w: u32, h: u32, layers: Vec<Layer>) -> Editor {
    let mut d = Document::new("Fixture", w, h).unwrap();
    d.layers = layers;
    let mut e = Editor::new(d).unwrap();
    e.viewport = Viewport {
        width: w as f64,
        height: h as f64,
        zoom: 1.,
        pan: Point::default(),
    };
    e
}
fn raster(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Layer {
    let pixels = (0..h)
        .flat_map(|y| (0..w).flat_map(move |x| [x, y]))
        .collect::<Vec<_>>();
    let data = pixels
        .chunks_exact(2)
        .flat_map(|p| f(p[0], p[1]))
        .collect::<Vec<_>>();
    Layer::new(
        "Layer 1",
        w,
        h,
        native_content(rgba_image(w, h, &data).unwrap()),
    )
}
fn halves() -> Editor {
    session(
        100,
        40,
        vec![raster(100, 40, |x, _| {
            if x < 50 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        })],
    )
}
fn cmd(e: &mut Editor, c: Command) {
    e.command(c).unwrap();
}
fn select(e: &mut Editor, x: f64, y: f64, w: f64, h: f64) {
    // Upstream tests call beginLasso directly, bypassing the canvas's move hit testing.
    let mut d = SelectionDraft::new(
        Point::new(x, y),
        Some(MarqueeKind::Rectangle),
        PixelSelectionMode::Replace,
    );
    d.drag(Point::new(x + w, y + h), false);
    e.history.pixel_selection = finish(&e.history.document, None, &d).unwrap();
}
fn pixel(e: &Editor, x: u32, y: u32) -> [u8; 4] {
    Renderer::default()
        .sample(&e.history.document, Point::new(x as f64, y as f64))
        .unwrap()
}
fn pointer(e: &mut Editor, phase: Phase, x: f64, y: f64, modifiers: Modifiers) {
    e.pointer(PointerSample {
        phase,
        point: Point::new(x, y),
        modifiers,
    })
    .unwrap();
}
fn bytes(e: &Editor) -> Vec<u8> {
    rgba_pixels(
        &Renderer::default()
            .render(&e.history.document)
            .unwrap()
            .image_snapshot(),
    )
    .unwrap()
}
/// SelectionClipboardTests.copyAndPastePutsPixelsOnANewLayerInPlace / cutLeavesAHoleAndPasteRestoresThePixels.
#[test]
fn compositor_copy_cut_paste_in_place() {
    let mut e = halves();
    let source = e.selected_id().unwrap().to_owned();
    select(&mut e, 40., 10., 20., 20.);
    let copied = e.copy_pixels(false).unwrap().unwrap();
    assert_eq!(copied.origin, Point::new(40., 10.));
    assert_eq!(copied.image.dimensions(), skia_safe::ISize::new(20, 20));
    let count = e.history.info().undo_count;
    e.paste_pixels(copied.image, Some(copied.origin), "Paste")
        .unwrap();
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(e.history.info().undo_label, "Paste");
    assert_ne!(e.selected_id().unwrap(), source);
    assert!(e.history.pixel_selection.is_none());
    assert_eq!(pixel(&e, 45, 15), [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 55, 15), [0, 0, 255, 255]);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.history.document.layers.len(), 1);
    select(&mut e, 10., 10., 10., 10.);
    let copied = e.copy_pixels(false).unwrap().unwrap();
    cmd(&mut e, Command::ClearSelectedPixels);
    assert_eq!(pixel(&e, 15, 15)[3], 0);
    e.paste_pixels(copied.image, Some(copied.origin), "Paste")
        .unwrap();
    assert_eq!(pixel(&e, 15, 15), [255, 0, 0, 255]);
}
/// SelectionClipboardTests.layerViaCopyCopiesTheSelectionOrDuplicatesTheLayer.
#[test]
fn compositor_layer_via_copy_and_duplicate() {
    let mut e = halves();
    let source = e.selected_id().unwrap().to_owned();
    select(&mut e, 60., 0., 10., 40.);
    cmd(&mut e, Command::LayerViaCopy);
    assert_eq!(e.history.info().undo_label, "Layer via Copy");
    assert_eq!(e.selected().unwrap().width, 10);
    assert!(e.history.pixel_selection.is_none());
    cmd(
        &mut e,
        Command::Select {
            id: Some(source),
            mode: SelectionMode::Replace,
        },
    );
    cmd(&mut e, Command::LayerViaCopy);
    assert_eq!(e.history.document.layers.len(), 3);
}
/// SelectionClipboardTests.transformSelectionMovesPixelsAndOutlineAsOneUndo.
#[test]
fn compositor_selection_transform_moves_and_undo_restores() {
    let mut e = halves();
    select(&mut e, 10., 10., 10., 10.);
    let before = e.history.document.clone();
    let outline = e.history.pixel_selection.clone();
    let count = e.history.info().undo_count;
    cmd(&mut e, Command::BeginTransform);
    cmd(
        &mut e,
        Command::UpdateLayer {
            patch: serde_json::json!({"x":70.,"y":20.}),
        },
    );
    assert_eq!(
        e.history
            .pixel_selection
            .as_ref()
            .unwrap()
            .bounds
            .as_ref()
            .unwrap()
            .x,
        70
    );
    cmd(&mut e, Command::CommitTransform);
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(e.history.info().undo_label, "Transform Selection");
    assert_eq!(e.history.document.layers.len(), 1);
    assert_eq!(pixel(&e, 15, 15)[3], 0);
    assert_eq!(pixel(&e, 75, 25), [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 85, 25), [0, 0, 255, 255]);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.history.document, before);
    assert_eq!(e.history.pixel_selection, outline);
}
/// SelectionClipboardTests.escapeRestoresExactlyWithoutAnUndoStep / applyingAnUnchangedTransformLeavesSoftEdgesUntouched.
#[test]
fn compositor_selection_transform_cancel_and_soft_noop() {
    let mut e = halves();
    select(&mut e, 10., 10., 10., 10.);
    cmd(&mut e, Command::FeatherSelection { amount: 6 });
    let before = e.history.document.clone();
    let outline = e.history.pixel_selection.clone();
    let count = e.history.info().undo_count;
    cmd(&mut e, Command::BeginTransform);
    cmd(
        &mut e,
        Command::UpdateLayer {
            patch: serde_json::json!({"x":70.,"y":20.}),
        },
    );
    cmd(&mut e, Command::CancelTransform);
    assert_eq!(e.history.document, before);
    assert_eq!(e.history.pixel_selection, outline);
    assert_eq!(e.history.info().undo_count, count);
    cmd(&mut e, Command::BeginTransform);
    cmd(&mut e, Command::CommitTransform);
    assert_eq!(e.history.document, before);
    assert_eq!(e.history.pixel_selection, outline);
    assert_eq!(e.history.info().undo_count, count);
}
/// SelectionEditTests.cmdDragMovesSelectedPixelsAndOutlineAsOneUndo / duplicatePixelDragPreservesSourceAndUndoesTogether.
#[test]
fn compositor_move_and_duplicate_pixels() {
    for duplicate in [false, true] {
        let mut e = halves();
        select(&mut e, 10., 10., 10., 10.);
        let before = e.history.document.clone();
        let count = e.history.info().undo_count;
        cmd(
            &mut e,
            Command::MovePixels {
                delta: Point::new(60.2, 5.),
                duplicate,
            },
        );
        assert_eq!(pixel(&e, 75, 20), [255, 0, 0, 255]);
        assert_eq!(pixel(&e, 15, 15)[3], if duplicate { 255 } else { 0 });
        assert_eq!(e.history.info().undo_count, count + 1);
        assert_eq!(
            e.history.info().undo_label,
            if duplicate {
                "Duplicate Pixels"
            } else {
                "Move Pixels"
            }
        );
        assert_eq!(
            e.history
                .pixel_selection
                .as_ref()
                .unwrap()
                .bounds
                .as_ref()
                .unwrap()
                .x,
            70
        );
        cmd(&mut e, Command::Undo);
        assert_eq!(e.history.document, before);
    }
}
/// SelectionTests.draggingMovesTheOutlineInWholePixelsAsOneUndo / movingOffCanvasAndBackKeepsTheWholeShape.
#[test]
fn compositor_outline_movement_keeps_shape_off_canvas() {
    let mut e = session(100, 100, vec![]);
    select(&mut e, 10., 10., 20., 20.);
    let before = e.history.pixel_selection.clone();
    let count = e.history.info().undo_count;
    cmd(
        &mut e,
        Command::MoveSelection {
            delta: Point::new(40.2, 40.4),
        },
    );
    assert_eq!(
        e.history
            .pixel_selection
            .as_ref()
            .unwrap()
            .bounds
            .as_ref()
            .unwrap()
            .x,
        50
    );
    assert_eq!(e.history.info().undo_count, count + 1);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.history.pixel_selection, before);
    cmd(
        &mut e,
        Command::MoveSelection {
            delta: Point::new(-25., 0.),
        },
    );
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(0, 20), 255);
    cmd(
        &mut e,
        Command::MoveSelection {
            delta: Point::new(25., 0.),
        },
    );
    assert_eq!(e.history.pixel_selection, before);
}
/// SelectionTests.polygonalCornersCanBeRemovedAndClosed.
#[test]
fn compositor_polygonal_corners_remove_close_and_cancel() {
    let mut e = session(100, 100, vec![]);
    cmd(&mut e, Command::SetTool { tool: Tool::Lasso });
    cmd(
        &mut e,
        Command::SetLassoKind {
            kind: LassoKind::Polygonal,
        },
    );
    for (x, y) in [(10., 10.), (90., 10.), (50., 50.)] {
        pointer(&mut e, Phase::Down, x, y, Modifiers::default());
        pointer(&mut e, Phase::Up, x, y, Modifiers::default());
    }
    cmd(&mut e, Command::RemoveSelectionPoint);
    for (x, y) in [(90., 90.), (10., 90.)] {
        pointer(&mut e, Phase::Down, x, y, Modifiers::default());
        pointer(&mut e, Phase::Up, x, y, Modifiers::default());
    }
    cmd(&mut e, Command::FinishSelection);
    assert_eq!(e.history.info().undo_label, "Polygonal Lasso");
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(80, 80), 255);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(5, 50), 0);
    let before = e.history.pixel_selection.clone();
    pointer(&mut e, Phase::Down, 1., 1., Modifiers::default());
    cmd(&mut e, Command::CancelPixelSelection);
    assert!(e.selection_draft().is_none());
    assert_eq!(e.history.pixel_selection, before);
}
fn selected(path: Option<skia_safe::Path>, w: u32, h: u32) -> Vec<usize> {
    path.map(|p| {
        PixelSelection::from_path(w, h, p, 0.)
            .unwrap()
            .pixels
            .iter()
            .enumerate()
            .filter_map(|(i, v)| (*v >= 128).then_some(i))
            .collect()
    })
    .unwrap_or_default()
}
/// MagicWandTests.contiguousStopsAtOtherColorsWhileNonContiguousFindsEveryMatch.
#[test]
fn compositor_wand_contiguity_and_row_direction() {
    let data = (0..4)
        .flat_map(|_| {
            (0..10).flat_map(|x| {
                if x < 3 || x >= 6 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
        })
        .collect::<Vec<_>>();
    let settings = WandSettings::default();
    let left = (0..4)
        .flat_map(|y| (0..3).map(move |x| y * 10 + x))
        .collect::<Vec<_>>();
    assert_eq!(
        selected(
            wand::select(&data, 10, 4, Point::new(1.5, 2.5), settings).unwrap(),
            10,
            4
        ),
        left
    );
    let matches = selected(
        wand::select(
            &data,
            10,
            4,
            Point::new(1.5, 2.5),
            WandSettings {
                contiguous: false,
                ..settings
            },
        )
        .unwrap(),
        10,
        4,
    );
    assert_eq!(matches.len(), 28);
    let band = (0..3)
        .flat_map(|y| {
            (0..4).flat_map(move |_| {
                if y == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        selected(
            wand::select(&band, 4, 3, Point::new(1., 0.), settings).unwrap(),
            4,
            3
        ),
        vec![0, 1, 2, 3]
    );
    assert!(
        wand::select(&band, 4, 3, Point::new(9., 0.), settings)
            .unwrap()
            .is_none()
    );
}
/// MagicWandTests.toleranceAppliesToEveryChannelIncludingAlpha / sampleSizeAveragesThePixelsAroundTheClick.
#[test]
fn compositor_wand_tolerance_alpha_and_average() {
    let row = [
        [100, 100, 100, 255],
        [132, 100, 100, 255],
        [133, 100, 100, 255],
        [100, 100, 100, 222],
    ]
    .concat();
    for (t, expected) in [(0, vec![0]), (32, vec![0, 1]), (33, vec![0, 1, 2, 3])] {
        assert_eq!(
            selected(
                wand::select(
                    &row,
                    4,
                    1,
                    Point::new(0.5, 0.5),
                    WandSettings {
                        tolerance: t,
                        contiguous: false,
                        ..Default::default()
                    }
                )
                .unwrap(),
                4,
                1
            ),
            expected
        );
    }
    let dot = (0..5)
        .flat_map(|y| {
            (0..5).flat_map(move |x| {
                if x == 2 && y == 2 {
                    [255, 255, 255, 255]
                } else {
                    [0, 0, 0, 255]
                }
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        selected(
            wand::select(
                &dot,
                5,
                5,
                Point::new(2.5, 2.5),
                WandSettings {
                    tolerance: 10,
                    contiguous: false,
                    ..Default::default()
                }
            )
            .unwrap(),
            5,
            5
        ),
        vec![12]
    );
    assert_eq!(
        selected(
            wand::select(
                &dot,
                5,
                5,
                Point::new(2.5, 2.5),
                WandSettings {
                    tolerance: 30,
                    radius: 1,
                    contiguous: false,
                    ..Default::default()
                }
            )
            .unwrap(),
            5,
            5
        ),
        (0..25).filter(|i| *i != 12).collect::<Vec<_>>()
    );
}
/// MagicWandTests.outlinesReproduceTheirPixelsWithHolesAndCornerTouches.
#[test]
fn compositor_wand_holes_and_diagonal_contacts() {
    let mut mask = vec![0; 48];
    for y in 0..3 {
        for x in 0..3 {
            if !(x == 1 && y == 1) {
                mask[y * 8 + x] = 255;
            }
        }
    }
    mask[4 * 8 + 5] = 255;
    mask[5 * 8 + 6] = 255;
    assert_eq!(
        selected(wand::outline(&mask, 8, 6).unwrap(), 8, 6),
        mask.iter()
            .enumerate()
            .filter_map(|(i, v)| (*v != 0).then_some(i))
            .collect::<Vec<_>>()
    );
    assert!(wand::outline(&[0; 4], 2, 2).unwrap().is_none());
}
/// MagicWandTests.theWandReadsTheActiveLayerOrEveryVisibleLayerAndCombinesModes.
#[test]
fn compositor_wand_sampling_and_selection_history() {
    let mut e = session(
        20,
        10,
        vec![raster(20, 10, |x, _| {
            if x < 10 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            }
        })],
    );
    cmd(&mut e, Command::AddPaintLayer);
    cmd(&mut e, Command::SetTool { tool: Tool::Wand });
    pointer(&mut e, Phase::Down, 2., 2., Modifiers::default());
    assert_eq!(
        e.history
            .pixel_selection
            .as_ref()
            .unwrap()
            .pixels
            .iter()
            .filter(|v| **v > 0)
            .count(),
        200
    );
    cmd(
        &mut e,
        Command::SetWand {
            settings: WandSettings {
                sample_all_layers: true,
                ..Default::default()
            },
        },
    );
    pointer(&mut e, Phase::Down, 2., 2., Modifiers::default());
    assert_eq!(
        e.history
            .pixel_selection
            .as_ref()
            .unwrap()
            .pixels
            .iter()
            .filter(|v| **v > 0)
            .count(),
        100
    );
    cmd(
        &mut e,
        Command::SetSelectionMode {
            mode: PixelSelectionMode::Add,
        },
    );
    pointer(&mut e, Phase::Down, 15., 5., Modifiers::default());
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(15, 5), 255);
    cmd(
        &mut e,
        Command::SetSelectionMode {
            mode: PixelSelectionMode::Subtract,
        },
    );
    pointer(&mut e, Phase::Down, 2., 2., Modifiers::default());
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(2, 2), 0);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(2, 2), 255);
}
/// ImageSizeTests.resizePreservesLayerIdentityAndUndoRestoresSource (local equivalent import raster).
#[test]
fn compositor_image_size_preserves_identity_and_undo() {
    let mut e = session(
        64,
        32,
        vec![raster(64, 32, |x, _| {
            if x < 32 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 0]
            }
        })],
    );
    let before = e.history.document.clone();
    let selected = e.selected_id().unwrap().to_owned();
    cmd(
        &mut e,
        Command::ResizeImage {
            options: ImageSizeOptions {
                width: 128,
                height: 96,
                resolution: 300.,
                sampling: Sampling::Nearest,
            },
        },
    );
    assert_eq!(
        (e.history.document.width, e.history.document.height),
        (128, 96)
    );
    assert_eq!(e.history.document.resolution, 300.);
    assert_eq!(e.selected_id(), Some(selected.as_str()));
    assert_eq!(e.selected().unwrap().height, 96);
    assert_eq!(pixel(&e, 0, 0)[0], 255);
    assert_eq!(pixel(&e, 127, 0)[3], 0);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.history.document, before);
    cmd(&mut e, Command::Redo);
    assert_eq!(e.history.document.resolution, 300.);
}
/// ImageSizeTests.resolutionOnlyRetainsPixelsAndSurvivesSaveAndExport.
#[test]
fn compositor_resolution_only_roundtrip_export() {
    let mut e = session(32, 16, vec![raster(32, 16, |_, _| [255, 0, 0, 255])]);
    let layers = e.history.document.layers.clone();
    cmd(
        &mut e,
        Command::ResizeImage {
            options: ImageSizeOptions {
                width: 32,
                height: 16,
                resolution: 300.,
                sampling: Sampling::High,
            },
        },
    );
    assert_eq!(e.history.document.layers, layers);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Size.comp");
    files::save_project(&path, &e.history.document).unwrap();
    assert_eq!(files::open_project(&path).unwrap().resolution, 300.);
    let png = Renderer::default()
        .export(&e.history.document, false)
        .unwrap();
    let reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let density = reader.info().pixel_dims.unwrap();
    assert!((density.xppu as f64 * 0.0254 - 300.).abs() < 1.);
    let jpeg = Renderer::default()
        .export(&e.history.document, true)
        .unwrap();
    assert_eq!(jpeg[13], 1);
    assert_eq!(u16::from_be_bytes([jpeg[14], jpeg[15]]), 300);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.history.document.resolution, 72.);
}
/// ImageSizeTests.rotatedHiddenLayerScalesInDocumentAxesAndInvalidSizeIsRejected.
#[test]
fn compositor_image_size_rotated_hidden_and_atomic_limits() {
    let mut layer = raster(64, 32, |_, _| [255, 0, 0, 255]);
    layer.x = -16.;
    layer.y = 4.;
    layer.rotation = 90.;
    layer.visible = false;
    let mut e = session(64, 32, vec![layer]);
    let before = e.history.document.clone();
    cmd(
        &mut e,
        Command::ResizeImage {
            options: ImageSizeOptions {
                width: 128,
                height: 96,
                resolution: 72.,
                sampling: Sampling::Nearest,
            },
        },
    );
    let layer = e.selected().unwrap();
    assert!(!layer.visible);
    assert_eq!(layer.rotation, 0.);
    assert!((layer.width as i32 - 64).abs() <= 1);
    assert!((layer.height as i32 - 192).abs() <= 1);
    assert!(layer.y < 0.);
    let resized = e.history.document.clone();
    assert!(
        e.command(Command::ResizeImage {
            options: ImageSizeOptions {
                width: 8192,
                height: 8192,
                resolution: 72.,
                sampling: Sampling::High
            }
        })
        .is_err()
    );
    assert_eq!(e.history.document, resized);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.history.document, before);
}
/// Local regressions: merges bake appearance/masks, retain placement and undo; groups and multi-selection route correctly.
#[test]
fn local_merge_down_group_and_selected_layers() {
    for group in [false, true] {
        let mut a = raster(20, 10, |_, _| [255, 0, 0, 255]);
        let mut b = raster(10, 10, |_, _| [0, 0, 255, 255]);
        b.x = 10.;
        b.opacity = 0.5;
        let mut layers = vec![];
        if group {
            let folder = Layer::new("Folder", 20, 10, Content::Group);
            a.parent_id = Some(folder.id.clone());
            b.parent_id = a.parent_id.clone();
            layers.push(folder);
        }
        layers.extend([a, b]);
        let mut e = session(20, 10, layers);
        if group {
            let id = e.history.document.layers[0].id.clone();
            cmd(
                &mut e,
                Command::Select {
                    id: Some(id),
                    mode: SelectionMode::Replace,
                },
            );
        }
        let before = e.history.document.clone();
        let rendered = bytes(&e);
        cmd(&mut e, Command::MergeLayers);
        assert_eq!(e.history.document.layers.len(), 1);
        assert_eq!(bytes(&e), rendered);
        cmd(&mut e, Command::Undo);
        assert_eq!(e.history.document, before);
    }
    let mut e = halves();
    cmd(&mut e, Command::Duplicate);
    cmd(&mut e, Command::SelectAll);
    cmd(&mut e, Command::MergeLayers);
    assert_eq!(e.history.document.layers.len(), 1);
}
/// Local regressions: source transforms/masks, soft edges, external centering, limit failures, clipboard source isolation.
#[test]
fn local_clipboard_masks_feather_centering_and_source_preservation() {
    let mut layer = raster(20, 10, |_, _| [255, 0, 0, 255]);
    layer.x = 20.;
    layer.y = 10.;
    layer.scale_x = 2.;
    layer.opacity = 0.1;
    layer.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        linked: true,
        placement: None,
        strokes: vec![],
        raster: Some(Arc::new(MaskRaster::solid(false))),
    }));
    let mut e = session(100, 40, vec![layer]);
    select(&mut e, 25., 12., 10., 5.);
    let copied = e.copy_pixels(false).unwrap().unwrap();
    assert_eq!(rgba_pixels(&copied.image).unwrap()[..4], [255, 0, 0, 255]);
    let merged = e.copy_pixels(true).unwrap().unwrap();
    assert_eq!(rgba_pixels(&merged.image).unwrap()[3], 0);
    e.paint_target = PaintTarget::Mask;
    let mask = e.copy_pixels(false).unwrap().unwrap();
    assert_eq!(rgba_pixels(&mask.image).unwrap()[..4], [0, 0, 0, 255]);
    e.paste_pixels(copied.image, None, "Paste").unwrap();
    assert_eq!(e.selected().unwrap().x, 45.);
    assert_eq!(e.selected().unwrap().y, 17.);
    select(&mut e, 46., 18., 5., 2.);
    cmd(&mut e, Command::FeatherSelection { amount: 2 });
    let copied = e.copy_pixels(false).unwrap().unwrap();
    assert!(
        rgba_pixels(&copied.image)
            .unwrap()
            .chunks_exact(4)
            .any(|p| p[3] > 0 && p[3] < 255)
    );
}

/// Local pointer regression for Cmd/Ctrl-drag and Option/Alt duplicate; source commands keep native ownership.
#[test]
fn local_pointer_pixel_moves_and_persistent_layer_cancel() {
    for (duplicate, tool) in [Tool::Marquee, Tool::Lasso, Tool::Wand]
        .into_iter()
        .flat_map(|tool| {
            [false, true]
                .into_iter()
                .map(move |duplicate| (duplicate, tool))
        })
    {
        let mut e = halves();
        select(&mut e, 10., 10., 10., 10.);
        cmd(&mut e, Command::SetTool { tool });
        let before = e.history.document.clone();
        let count = e.history.info().undo_count;
        let modifiers = Modifiers {
            control: true,
            alt: duplicate,
            ..Default::default()
        };
        pointer(&mut e, Phase::Down, 15., 15., modifiers);
        pointer(&mut e, Phase::Move, 45.4, 15., modifiers);
        pointer(&mut e, Phase::Up, 75.2, 20., modifiers);
        assert_eq!(pixel(&e, 75, 20), [255, 0, 0, 255]);
        assert_eq!(pixel(&e, 15, 15)[3], if duplicate { 255 } else { 0 });
        assert_eq!(e.history.info().undo_count, count + 1);
        assert_eq!(e.tool, tool);
        cmd(&mut e, Command::Undo);
        assert_eq!(e.history.document, before);
    }
    let mut e = halves();
    let before = e.history.document.clone();
    cmd(&mut e, Command::BeginTransform);
    pointer(&mut e, Phase::Down, 20., 20., Modifiers::default());
    pointer(&mut e, Phase::Up, 30., 25., Modifiers::default());
    cmd(&mut e, Command::CancelTransform);
    assert_eq!(e.history.document, before);
    assert_eq!(e.history.info().undo_count, 0);
}
/// SelectionClipboardTests.scalingAndMovingPastTheLayerEdgeGrowsTheLayer, including its mask growth rule.
#[test]
fn compositor_floating_transform_grows_source_and_reveals_new_mask_area() {
    let mut layer = raster(30, 20, |_, _| [255, 0, 0, 255]);
    layer.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Hide,
        linked: true,
        placement: None,
        strokes: vec![],
        raster: Some(Arc::new(MaskRaster::solid(false))),
    }));
    let mut e = session(100, 40, vec![layer]);
    select(&mut e, 10., 5., 10., 10.);
    let before = e.history.document.clone();
    cmd(&mut e, Command::BeginTransform);
    cmd(
        &mut e,
        Command::UpdateLayer {
            patch: serde_json::json!({"x":50.,"scaleX":2.}),
        },
    );
    cmd(&mut e, Command::CommitTransform);
    assert_eq!(e.selected().unwrap().width, 70);
    assert_eq!(pixel(&e, 60, 10), [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 5, 10)[3], 0);
    cmd(&mut e, Command::Undo);
    assert_eq!(e.history.document, before);
}

/// Local regression for ImageResizer's independent-mask rule and the native placement model.
#[test]
fn local_image_size_retains_independent_mask_pixels() {
    let mut layer = raster(20, 10, |_, _| [255, 0, 0, 255]);
    let mut placement = MaskPlacement::of(&layer);
    placement.x = 4.;
    layer.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Hide,
        linked: false,
        placement: Some(placement),
        strokes: vec![],
        raster: Some(Arc::new(MaskRaster {
            width: 20,
            height: 10,
            pixels: Arc::new(
                (0..200)
                    .map(|i| if i % 20 < 10 { 255 } else { 0 })
                    .collect(),
            ),
        })),
    }));
    let mut e = session(20, 10, vec![layer]);
    let source = e.selected().unwrap().mask.as_ref().unwrap().raster.clone();
    assert_eq!(pixel(&e, 8, 5), [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 16, 5), [0, 0, 0, 0]);
    cmd(
        &mut e,
        Command::ResizeImage {
            options: ImageSizeOptions {
                width: 40,
                height: 20,
                resolution: 72.,
                sampling: Sampling::Nearest,
            },
        },
    );
    let mask = e.selected().unwrap().mask.as_ref().unwrap();
    assert_eq!(mask.raster, source);
    assert!(mask.placement.is_some());
    assert!(!mask.linked);
    assert_eq!(pixel(&e, 16, 10), [255, 0, 0, 255]);
    assert_eq!(pixel(&e, 32, 10), [0, 0, 0, 0]);
}

/// Local regression: stack order, rather than selection click order, names a merged result.
#[test]
fn local_merge_selection_order_and_empty_transform() {
    let a = raster(10, 10, |_, _| [255, 0, 0, 255]);
    let mut b = raster(10, 10, |_, _| [0, 0, 255, 255]);
    b.name = "Top".into();
    let mut e = session(20, 10, vec![a, b]);
    let id = e.history.document.layers[0].id.clone();
    cmd(
        &mut e,
        Command::Select {
            id: Some(id),
            mode: SelectionMode::Toggle,
        },
    );
    cmd(&mut e, Command::MergeLayers);
    assert_eq!(e.selected().unwrap().name, "Top");
    let before = e.history.document.clone();
    let count = e.history.info().undo_count;
    select(&mut e, 15., 0., 5., 10.);
    cmd(
        &mut e,
        Command::MovePixels {
            delta: Point::new(1., 0.),
            duplicate: false,
        },
    );
    assert!(!e.transform_active());
    assert_eq!(e.history.document, before);
    assert_eq!(e.history.info().undo_count, count);
}

/// SelectionClipboardTests.lassoShapedSelectionCopiesAndPastes / copyMergedTakesEveryVisibleLayerNotJustTheActiveOne.
#[test]
fn compositor_clipboard_lasso_and_visible_merge() {
    let mut e = halves();
    let mut draft = SelectionDraft::new(Point::new(10., 5.), None, PixelSelectionMode::Replace);
    draft.drag(Point::new(40., 5.), false);
    draft.drag(Point::new(10., 35.), false);
    e.history.pixel_selection = finish(&e.history.document, None, &draft).unwrap();
    let clip = e.copy_pixels(false).unwrap().unwrap();
    assert_eq!(clip.origin, Point::new(10., 5.));
    assert_eq!((clip.image.width(), clip.image.height()), (30, 30));
    let pixels = rgba_pixels(&clip.image).unwrap();
    assert_eq!(
        &pixels[(5 * 30 + 5) * 4..(5 * 30 + 5) * 4 + 4],
        &[255, 0, 0, 255]
    );
    assert_eq!(pixels[(28 * 30 + 28) * 4 + 3], 0);
    e.paste_pixels(clip.image, Some(clip.origin), "Paste")
        .unwrap();
    cmd(&mut e, Command::Undo);
    let mut green = raster(20, 20, |_, _| [0, 255, 0, 255]);
    green.x = 60.;
    green.y = 10.;
    e.import_layers(vec![green]).unwrap();
    cmd(
        &mut e,
        Command::UpdateLayer {
            patch: serde_json::json!({"x":60.,"y":10.}),
        },
    );
    select(&mut e, 55., 5., 30., 30.);
    let active = e.copy_pixels(false).unwrap().unwrap();
    let merged = e.copy_pixels(true).unwrap().unwrap();
    for (clip, outside) in [(active, [0, 0, 0, 0]), (merged, [0, 0, 255, 255])] {
        let pixels = rgba_pixels(&clip.image).unwrap();
        assert_eq!(
            &pixels[(10 * 30 + 10) * 4..(10 * 30 + 10) * 4 + 4],
            &[0, 255, 0, 255]
        );
        assert_eq!(&pixels[(3 * 30 + 3) * 4..(3 * 30 + 3) * 4 + 4], &outside);
    }
    cmd(
        &mut e,
        Command::UpdateLayer {
            patch: serde_json::json!({"visible":false}),
        },
    );
    let merged = e.copy_pixels(true).unwrap().unwrap();
    let pixels = rgba_pixels(&merged.image).unwrap();
    assert_eq!(
        &pixels[(10 * 30 + 10) * 4..(10 * 30 + 10) * 4 + 4],
        &[0, 0, 255, 255]
    );
}
