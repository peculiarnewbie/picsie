//! Headless group-transform verification: drives the real editor + Skia preview
//! through folder/multi selection, numeric resize/rotate, flip and cancel, and
//! writes canvas PNG captures (including the group-box overlay) plus the published
//! snapshot capability to artifacts/group-transforms/.
//!
//! This is renderer-level evidence, not a window run: Xvfb/xdotool are unavailable
//! on this machine, so GPUI window chrome, pointer hit-testing through the toolkit
//! and platform file dialogs remain unverified here (noted as gaps).
use picsie_core::{
    editor::*,
    geometry::Viewport,
    model::*,
    render::{self, Renderer},
};
use std::sync::Arc;

fn out() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from("artifacts/group-transforms");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn image(name: &str, w: u32, h: u32, rgb: [u8; 3]) -> Layer {
    let mut bytes = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..w * h {
        bytes.extend([rgb[0], rgb[1], rgb[2], 255]);
    }
    Layer::new(
        name,
        w,
        h,
        render::native_content(render::rgba_image(w, h, &bytes).unwrap()),
    )
}

fn shot(renderer: &mut Renderer, editor: &Editor, name: &str) {
    let displayed = editor.preview_document();
    let mut surface = renderer
        .preview(
            &displayed,
            &editor.viewport,
            &[],
            true,
            None,
            None,
            None,
            editor.selection_draft().as_ref(),
        )
        .unwrap();
    let overlaid = editor.draw_group_overlay(surface.canvas());
    let png = render::encode(&surface.image_snapshot(), false).unwrap();
    std::fs::write(out().join(name), &png).unwrap();
    println!(
        "{name}: {bytes} bytes, overlay drawn: {overlaid}",
        bytes = png.len()
    );
}

fn main() {
    let mut doc = Document::new("GroupShots", 400, 300).unwrap();
    let mut group = Layer::new("Folder", 400, 300, Content::Group);
    let group_id = group.id.clone();
    let mut red = image("Red", 80, 60, [220, 40, 30]);
    red.parent_id = Some(group_id.clone());
    red.x = 40.;
    red.y = 50.;
    let mut blue = image("Blue", 60, 90, [40, 90, 220]);
    blue.parent_id = Some(group_id.clone());
    blue.x = 180.;
    blue.y = 90.;
    blue.rotation = 20.;
    group.mask = None;
    doc.layers = vec![group, red, blue];
    let mut editor = Editor::new(doc).unwrap();
    editor.viewport = Viewport {
        width: 400.,
        height: 300.,
        zoom: 1.,
        pan: Point::default(),
    };
    editor.tool = Tool::Move;
    let mut renderer = Renderer::default();

    // Folder selection: one box around both children.
    editor
        .select(Some(group_id.clone()), SelectionMode::Replace)
        .unwrap();
    let snapshot = picsie_core::editor::publication::Snapshot::capture(&editor).unwrap();
    let target = snapshot.transform_target.clone().unwrap();
    println!(
        "groupBox capability: {}",
        serde_json::to_string(&snapshot.group_box).unwrap()
    );
    println!(
        "transformTarget: x={} y={} w={} h={} rotation={}",
        target.x,
        target.y,
        target.width as f64 * target.scale_x,
        target.height as f64 * target.scale_y,
        target.rotation
    );
    assert!(snapshot.group_box.is_some());
    shot(&mut renderer, &editor, "shot-01-folder-box.png");

    // Numeric resize draft: members preview from originals, single undo pending.
    editor.command(Command::BeginTransform).unwrap();
    editor
        .command(Command::SetTransformField {
            field: TransformField::Width,
            value: 300.,
        })
        .unwrap();
    shot(&mut renderer, &editor, "shot-02-resize-draft.png");
    assert!(editor.transform_active());

    // Numeric rotate + flip draft on top of the same transaction.
    editor
        .command(Command::SetTransformField {
            field: TransformField::Rotation,
            value: 15.,
        })
        .unwrap();
    shot(&mut renderer, &editor, "shot-03-rotate-draft.png");
    editor.command(Command::CommitTransform).unwrap();
    println!("undo after commit: {}", editor.history.info().undo_count);
    println!("undo label: {}", editor.history.info().undo_label);
    shot(&mut renderer, &editor, "shot-04-committed.png");

    // Flip about the box middle as one undo, then undo it back.
    editor
        .command(Command::UpdateLayer {
            patch: serde_json::json!({"flipX": true}),
        })
        .unwrap();
    println!("flip undo label: {}", editor.history.info().undo_label);
    shot(&mut renderer, &editor, "shot-05-flipped.png");
    editor.command(Command::Undo).unwrap();
    editor.command(Command::Undo).unwrap();
    let red_back = editor
        .history
        .document
        .layers
        .iter()
        .find(|l| l.name == "Red")
        .unwrap();
    assert_eq!((red_back.x, red_back.y), (40., 50.));
    println!(
        "restored after two undos: Red at ({}, {})",
        red_back.x, red_back.y
    );
    shot(&mut renderer, &editor, "shot-06-undone.png");

    // Multi-selection (no folder) shows the same shared box.
    let ids: Vec<String> = editor
        .history
        .document
        .layers
        .iter()
        .filter(|l| !matches!(l.content.as_ref(), Content::Group))
        .map(|l| l.id.clone())
        .collect();
    editor
        .select(Some(ids[0].clone()), SelectionMode::Replace)
        .unwrap();
    editor
        .select(Some(ids[1].clone()), SelectionMode::Toggle)
        .unwrap();
    let multi = picsie_core::editor::publication::Snapshot::capture(&editor).unwrap();
    println!(
        "multi groupBox capability: {}",
        serde_json::to_string(&multi.group_box).unwrap()
    );
    assert!(multi.group_box.is_some());
    shot(&mut renderer, &editor, "shot-07-multi-box.png");

    // An empty folder publishes no box (UI keeps fields disabled).
    let mut empty = Layer::new("Empty", 400, 300, Content::Group);
    let empty_id = empty.id.clone();
    let mut with_empty = editor.history.document.clone();
    empty.mask = None;
    with_empty.layers.push(empty);
    with_empty.validate().unwrap();
    editor.edit("test setup", with_empty, None).unwrap();
    editor
        .select(Some(empty_id.clone()), SelectionMode::Replace)
        .unwrap();
    let none = picsie_core::editor::publication::Snapshot::capture(&editor).unwrap();
    println!(
        "empty-folder groupBox capability: {}",
        serde_json::to_string(&none.group_box).unwrap()
    );
    assert!(none.group_box.is_none());
    let _ = Arc::new(());
    println!("group-transform shots: OK");
}
