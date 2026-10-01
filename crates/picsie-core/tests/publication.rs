//! Local transport regressions, not translated upstream behavior fixtures.
//! Compare the new native publication with the preexisting serialized contract.
use picsie_core::{
    editor::{
        Command, Editor, Tool,
        publication::{Snapshot, SnapshotPublisher},
    },
    model::*,
    render,
};
use serde_json::json;
use std::sync::Arc;

fn matches_legacy(editor: &Editor, publisher: &mut SnapshotPublisher) -> Snapshot {
    let legacy: Snapshot = serde_json::from_value(editor.snapshot()).unwrap();
    let native = publisher.capture(editor).unwrap();
    assert_eq!(
        serde_json::to_value(&native).unwrap(),
        serde_json::to_value(legacy).unwrap()
    );
    native
}

#[test]
fn native_metadata_matches_legacy_without_serializing_pixel_payloads() {
    let mut doc = demo_document();
    let mut layer = Layer::new(
        "Raster",
        17,
        13,
        render::native_content(
            render::rgba_image(17, 13, &[10, 20, 30, 255].repeat(17 * 13)).unwrap(),
        ),
    );
    layer.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        linked: false,
        placement: Some(MaskPlacement::of(&layer)),
        raster: Some(Arc::new(MaskRaster {
            width: 17,
            height: 13,
            pixels: Arc::new(vec![128; 17 * 13]),
        })),
        strokes: vec![Arc::new(MaskStroke {
            mode: MaskMode::Hide,
            size: 3.,
            opacity: 0.5,
            points: vec![Point::new(4., 4.)],
        })],
    }));
    doc.layers.push(layer);
    let mut editor = Editor::new(doc).unwrap();
    let mut publisher = SnapshotPublisher::default();
    let initial = matches_legacy(&editor, &mut publisher);
    let image = initial.document.layers.last().unwrap();
    assert_eq!(image.content, json!({"kind":"image"}));
    assert!(image.mask.as_ref().unwrap().get("raster").is_none());
    assert!(image.mask.as_ref().unwrap().get("strokes").is_none());
    for command in [
        Command::SetTool { tool: Tool::Crop },
        Command::Fit,
        Command::SetTool { tool: Tool::Move },
        Command::SetPaletteColor {
            color: "#abcdef".into(),
            background: false,
        },
        Command::SetSelectionAntialiased { antialiased: false },
        Command::SetTransformRatio { locked: true },
        Command::BeginDistort,
        Command::CancelTransform,
    ] {
        editor.command(command).unwrap();
        matches_legacy(&editor, &mut publisher);
    }
}

#[test]
fn viewport_publication_shares_document_rows_sources_and_controls() {
    let mut editor = Editor::new(demo_document()).unwrap();
    editor
        .command(Command::SetTool { tool: Tool::Hand })
        .unwrap();
    let mut publisher = SnapshotPublisher::default();
    let first = matches_legacy(&editor, &mut publisher);
    for command in [
        Command::Pan {
            delta: Point::new(12., -7.),
        },
        Command::Zoom {
            zoom: 1.3,
            point: Some(Point::new(130., 40.)),
        },
        Command::ResizeViewport {
            width: 900.,
            height: 700.,
        },
        Command::Fit,
    ] {
        editor.command(command).unwrap();
        let next = matches_legacy(&editor, &mut publisher);
        assert!(Arc::ptr_eq(&first.document, &next.document));
        assert!(Arc::ptr_eq(&first.layer_rows, &next.layer_rows));
        assert!(Arc::ptr_eq(&first.mask_source_ids, &next.mask_source_ids));
        assert!(first.same_controls(&next));
        assert_eq!(
            next.cursor_map.origin,
            picsie_core::geometry::canvas_origin(&editor.history.document, &editor.viewport)
        );
    }
    assert_eq!(editor.history.info().undo_count, 0);
}

#[test]
fn live_edits_undo_and_direct_mutation_publish_fresh_immutable_metadata() {
    let mut editor = Editor::new(demo_document()).unwrap();
    let mut publisher = SnapshotPublisher::default();
    let first = matches_legacy(&editor, &mut publisher);
    let old_x = first.selected().unwrap().x;
    editor
        .command(Command::BeginPropertyEdit {
            label: "Move".into(),
        })
        .unwrap();
    editor
        .command(Command::UpdateLayer {
            patch: json!({"x":old_x+31.}),
        })
        .unwrap();
    let live = matches_legacy(&editor, &mut publisher);
    assert!(!Arc::ptr_eq(&first.document, &live.document));
    assert!(!first.same_controls(&live));
    assert_eq!(first.selected().unwrap().x, old_x);
    assert_eq!(live.selected().unwrap().x, old_x + 31.);
    editor.command(Command::FinishGesture).unwrap();
    editor.command(Command::Undo).unwrap();
    let restored = matches_legacy(&editor, &mut publisher);
    assert_eq!(restored.selected().unwrap().x, old_x);
    editor.history.document.layers.last_mut().unwrap().locked = true;
    let locked = matches_legacy(&editor, &mut publisher);
    assert!(locked.selected().unwrap().locked);
    assert!(!restored.selected().unwrap().locked);
    let id = editor.selected_id().unwrap().to_owned();
    editor.history.document.layers.retain(|l| l.id != id);
    assert!(matches_legacy(&editor, &mut publisher).layer(&id).is_none());
}

#[test]
fn collapsed_rows_do_not_hide_selected_clipping_capabilities() {
    let mut doc = Document::new("Folder", 100, 100).unwrap();
    let group = Layer::new("Folder", 100, 100, Content::Group);
    let mut base = Layer::new("Base", 50, 50, Content::Paint);
    base.parent_id = Some(group.id.clone());
    let mut clipped = Layer::new("Clipped", 50, 50, Content::Paint);
    clipped.parent_id = Some(group.id.clone());
    clipped.mask_source_id = Some(base.id.clone());
    let group_id = group.id.clone();
    let clipped_id = clipped.id.clone();
    doc.layers = vec![group, base, clipped];
    let mut editor = Editor::new(doc).unwrap();
    let mut publisher = SnapshotPublisher::default();
    let expanded = matches_legacy(&editor, &mut publisher);
    assert!(expanded.can_toggle_clipping);
    editor
        .command(Command::ToggleGroupExpansion { id: group_id })
        .unwrap();
    // Normal collapse selects the folder. A restored/programmatic selection can
    // still target its hidden child; capability queries must not depend on rows.
    let folder = matches_legacy(&editor, &mut publisher);
    assert!(!folder.can_toggle_clipping);
    editor
        .select(
            Some(clipped_id),
            picsie_core::editor::SelectionMode::Replace,
        )
        .unwrap();
    let collapsed = matches_legacy(&editor, &mut publisher);
    assert!(Arc::ptr_eq(&expanded.document, &collapsed.document));
    assert!(!Arc::ptr_eq(&expanded.layer_rows, &collapsed.layer_rows));
    assert_eq!(collapsed.layer_rows.len(), 1);
    assert!(collapsed.can_toggle_clipping);
}

#[test]
fn session_controls_invalidate_without_a_document_edit() {
    let mut editor = Editor::new(demo_document()).unwrap();
    let mut publisher = SnapshotPublisher::default();
    let first = matches_legacy(&editor, &mut publisher);
    editor
        .command(Command::SetPaletteColor {
            color: "#abcdef".into(),
            background: false,
        })
        .unwrap();
    let changed = matches_legacy(&editor, &mut publisher);
    assert!(Arc::ptr_eq(&first.document, &changed.document));
    assert!(!first.same_controls(&changed));
    assert_eq!(first.color, "#000000");
    assert_eq!(changed.color, "#abcdef");
}

#[test]
fn pixel_resource_replacement_invalidates_even_when_metadata_is_identical() {
    let mut doc = Document::new("Pixels", 16, 16).unwrap();
    let mut layer = Layer::new("Mask", 16, 16, Content::Paint);
    layer.mask = Some(Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        linked: true,
        placement: None,
        strokes: vec![],
        raster: Some(Arc::new(MaskRaster {
            width: 16,
            height: 16,
            pixels: Arc::new(vec![255; 256]),
        })),
    }));
    doc.layers.push(layer);
    let mut editor = Editor::new(doc).unwrap();
    let mut publisher = SnapshotPublisher::default();
    let first = matches_legacy(&editor, &mut publisher);
    let mask = Arc::make_mut(editor.history.document.layers[0].mask.as_mut().unwrap());
    let raster = Arc::make_mut(mask.raster.as_mut().unwrap());
    Arc::make_mut(&mut raster.pixels)[0] = 0;
    let next = matches_legacy(&editor, &mut publisher);
    assert!(!Arc::ptr_eq(&first.document, &next.document));
    assert_eq!(
        serde_json::to_value(&first.document).unwrap(),
        serde_json::to_value(&next.document).unwrap()
    );
}
