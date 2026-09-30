//! Selected translated fixtures from Compositor GuideTests, TransformPressTests and TypeToolTests
//! at 609dbeae. MIT © 2026 Wonder Assembly LLC. Additional local regressions are labeled.
use picsie_core::{
    editor::{Command, Editor},
    geometry::Viewport,
    model::*,
    placement::*,
    text::{self, TextAlignment, TextLayout},
};
use serde_json::json;
fn command(e: &mut Editor, v: serde_json::Value) {
    e.command(serde_json::from_value(v).unwrap()).unwrap();
}
fn editor() -> Editor {
    let mut e = Editor::new(Document::new("Layout", 400, 300).unwrap()).unwrap();
    e.viewport = Viewport {
        width: 400.,
        height: 300.,
        zoom: 1.,
        pan: Point::default(),
    };
    e
}
fn pointer(e: &mut Editor, phase: &str, x: f64, y: f64, control: bool) {
    command(
        e,
        json!({"type":"pointer","samples":[{"phase":phase,"point":{"x":x,"y":y},"modifiers":{"control":control}}]}),
    );
}
fn add(e: &mut Editor, axis: &str, pos: f64) {
    command(e, json!({"type":"addGuide","axis":axis,"position":pos}));
}
fn text(e: &mut Editor, x: f64, y: f64, content: &str) {
    command(e, json!({"type":"setTool","tool":"text"}));
    pointer(e, "down", x, y, false);
    pointer(e, "up", x, y, false);
    command(e, json!({"type":"updateText","patch":{"text":content}}));
}
fn content(e: &Editor) -> &str {
    match e.selected().unwrap().content.as_ref() {
        Content::Text { text, .. } => text,
        _ => panic!("not text"),
    }
}
#[test]
fn upstream_new_guides_undo_clear_and_lock() {
    let mut e = editor();
    add(&mut e, "vertical", 40.);
    add(&mut e, "horizontal", 25.);
    assert_eq!(e.history.document.guides.len(), 2);
    command(&mut e, json!({"type":"undo"}));
    assert_eq!(e.history.document.guides[0].position, 40.);
    let mut options = e.view_options.clone();
    options.lock_guides = true;
    command(&mut e, json!({"type":"setViewOptions","options":options}));
    add(&mut e, "horizontal", 10.);
    assert_eq!(e.history.document.guides.len(), 1);
    pointer(&mut e, "down", 40., 20., false);
    assert!(!e.guide_drag_active());
    command(&mut e, json!({"type":"clearGuides"}));
    assert!(e.history.document.guides.is_empty());
    command(&mut e, json!({"type":"undo"}));
    assert_eq!(e.history.document.guides.len(), 1);
}
#[test]
fn upstream_guide_drag_move_delete_cancel_one_history() {
    let mut e = editor();
    add(&mut e, "vertical", 40.);
    let before = e.history.info().undo_count;
    pointer(&mut e, "down", 40., 20., false);
    pointer(&mut e, "move", 70., 20., false);
    assert_eq!(e.displayed_guides()[0].position, 70.);
    assert_eq!(e.history.document.guides[0].position, 40.);
    pointer(&mut e, "up", 70., 20., false);
    command(&mut e, json!({"type":"finishGuide","delete":false}));
    assert_eq!(e.history.document.guides[0].position, 70.);
    assert_eq!(e.history.info().undo_count, before + 1);
    pointer(&mut e, "down", 70., 20., false);
    command(&mut e, json!({"type":"finishGuide","delete":true}));
    assert!(e.history.document.guides.is_empty());
    command(&mut e, json!({"type":"undo"}));
    assert_eq!(e.history.document.guides[0].position, 70.);
    pointer(&mut e, "down", 70., 20., false);
    pointer(&mut e, "move", 90., 20., false);
    command(&mut e, json!({"type":"cancelGesture"}));
    assert_eq!(e.history.document.guides[0].position, 70.);
}
#[test]
fn upstream_project_guides_roundtrip_and_legacy_rejection() {
    let mut e = editor();
    add(&mut e, "vertical", 16.);
    add(&mut e, "horizontal", 12.);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("guides.comp");
    picsie_core::comp::save(&path, &e.history.document).unwrap();
    let reopened = picsie_core::comp::open(&path).unwrap();
    assert_eq!(reopened.guides, e.history.document.guides);
    let file = path.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    manifest["version"] = 7.into();
    std::fs::write(file, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(picsie_core::comp::open(&path).is_err());
}
#[test]
fn upstream_canvas_and_image_size_move_guides() {
    let mut doc = Document::new("Guides", 100, 50).unwrap();
    doc.guides = vec![
        CanvasGuide {
            id: id(),
            axis: GuideAxis::Vertical,
            position: 20.,
        },
        CanvasGuide {
            id: id(),
            axis: GuideAxis::Horizontal,
            position: 10.,
        },
    ];
    let expanded = picsie_core::canvas_size::resize_canvas(
        &doc,
        &picsie_core::canvas_size::CanvasSizeOptions {
            width: 140,
            height: 80,
            anchor: 8,
            fill: None,
        },
    )
    .unwrap();
    assert_eq!(
        expanded
            .guides
            .iter()
            .map(|g| g.position)
            .collect::<Vec<_>>(),
        vec![60., 40.]
    );
    let scaled = picsie_core::image_size::resize(
        &doc,
        &picsie_core::image_size::ImageSizeOptions {
            width: 200,
            height: 100,
            resolution: 72.,
            sampling: Sampling::High,
        },
    )
    .unwrap();
    assert_eq!(
        scaled.guides.iter().map(|g| g.position).collect::<Vec<_>>(),
        vec![40., 20.]
    );
}
#[test]
fn upstream_snap_targets_follow_view_menu() {
    let mut e = editor();
    let mut l = Layer::new(
        "Red",
        100,
        60,
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#ff0000".into(),
        },
    );
    l.x = 150.;
    l.y = 120.;
    e.history.document.layers.push(l);
    let (xs, ys) = targets(&e.history.document, &e.view_options, &[], false);
    assert_eq!(xs, vec![0., 400., 150., 250.]);
    assert_eq!(ys, vec![0., 300., 120., 180.]);
    e.view_options.snap_layers = false;
    e.view_options.snap_bounds = false;
    add(&mut e, "vertical", 33.);
    assert_eq!(
        targets(&e.history.document, &e.view_options, &[], false).0,
        vec![33.]
    );
    e.view_options.guides = false;
    assert!(
        targets(&e.history.document, &e.view_options, &[], false)
            .0
            .is_empty()
    );
    e.view_options.grid = true;
    e.view_options.snap_grid = true;
    assert!(
        targets(&e.history.document, &e.view_options, &[], false)
            .0
            .contains(&8.)
    );
    e.view_options.snap = false;
    assert!(
        targets(&e.history.document, &e.view_options, &[], true)
            .0
            .is_empty()
    );
}
#[test]
fn upstream_grid_and_ruler_steps() {
    assert_eq!(
        grid_lines(64.),
        vec![0., 8., 16., 24., 32., 40., 48., 56., 64.]
    );
    assert_eq!(ruler_step(1.), 100.);
    assert_eq!(ruler_step(8.), 10.);
    let (offset, target) = nearest(&[10., 20., 30.], &[34., 7.], 10.);
    assert_eq!((offset, target), (-3., Some(7.)));
}
#[test]
fn upstream_active_layer_moves_outside_bounds_control_drags_freely() {
    let mut e = editor();
    let mut l = Layer::new(
        "Red",
        100,
        100,
        Content::Shape {
            shape: Shape::Rectangle,
            color: "#ff0000".into(),
        },
    );
    l.x = 150.;
    l.y = 100.;
    let id = l.id.clone();
    e.history.document.layers.push(l);
    command(&mut e, json!({"type":"select","id":id,"mode":"replace"}));
    pointer(&mut e, "down", 20., 20., true);
    pointer(&mut e, "up", 40., 30., true);
    assert_eq!(
        (e.selected().unwrap().x, e.selected().unwrap().y),
        (170., 110.)
    );
    command(&mut e, json!({"type":"undo"}));
    pointer(&mut e, "down", 20., 20., false);
    pointer(&mut e, "up", 28., 27., false);
    assert_eq!(
        (e.selected().unwrap().x, e.selected().unwrap().y),
        (150., 100.)
    );
}
#[test]
fn upstream_create_edit_cancel_and_undo() {
    let mut e = editor();
    text(&mut e, 30., 40., "Hello\nCompositor");
    command(&mut e, json!({"type":"updateText","patch":{"fontSize":48}}));
    command(&mut e, json!({"type":"commitText"}));
    assert_eq!(e.history.info().undo_count, 1);
    assert_eq!(
        (e.selected().unwrap().x, e.selected().unwrap().y),
        (30., 40.)
    );
    let id = e.selected_id().unwrap().to_owned();
    command(&mut e, json!({"type":"editText","id":id}));
    command(
        &mut e,
        json!({"type":"updateText","patch":{"text":"Changed"}}),
    );
    command(&mut e, json!({"type":"cancelText"}));
    assert_eq!(content(&e), "Hello\nCompositor");
    assert_eq!(e.history.info().undo_count, 1);
    command(&mut e, json!({"type":"editText","id":id}));
    command(
        &mut e,
        json!({"type":"updateText","patch":{"text":"Changed"}}),
    );
    command(&mut e, json!({"type":"commitText"}));
    command(&mut e, json!({"type":"undo"}));
    assert_eq!(content(&e), "Hello\nCompositor");
    command(&mut e, json!({"type":"undo"}));
    assert!(e.history.document.layers.is_empty());
    command(&mut e, json!({"type":"redo"}));
    assert_eq!(content(&e), "Hello\nCompositor");
}
#[test]
fn upstream_point_growth_preserves_rotated_corner_and_scale() {
    let mut e = editor();
    text(&mut e, 20., 20., "Text");
    command(&mut e, json!({"type":"commitText"}));
    command(
        &mut e,
        json!({"type":"updateLayer","patch":{"rotation":30,"scaleX":2,"flipX":true}}),
    );
    let old = e.selected().unwrap().clone();
    let anchor = picsie_core::geometry::bounds_point(&old, Point::default());
    let id = old.id.clone();
    command(&mut e, json!({"type":"editText","id":id}));
    command(
        &mut e,
        json!({"type":"updateText","patch":{"text":"Longer text"}}),
    );
    command(&mut e, json!({"type":"commitText"}));
    let new = e.selected().unwrap();
    let current = picsie_core::geometry::bounds_point(new, Point::default());
    assert!(anchor.distance(current) < 0.001);
    assert_eq!((new.rotation, new.scale_x, new.flip_x), (30., 2., true));
    assert!(new.width > old.width);
}
#[test]
fn upstream_paragraph_tool_switch_and_empty_discard() {
    let mut e = editor();
    command(&mut e, json!({"type":"setTool","tool":"text"}));
    pointer(&mut e, "down", 40., 60., false);
    pointer(&mut e, "up", 240., 180., false);
    assert_eq!(
        (e.selected().unwrap().width, e.selected().unwrap().height),
        (200, 120)
    );
    command(
        &mut e,
        json!({"type":"updateText","patch":{"text":"Text that wraps inside its paragraph box"}}),
    );
    command(&mut e, json!({"type":"setTool","tool":"brush"}));
    assert!(!e.text_editing());
    assert!(!e.selected().unwrap().text_layout.as_ref().unwrap().point);
    let count = e.history.document.layers.len();
    command(&mut e, json!({"type":"setTool","tool":"text"}));
    pointer(&mut e, "down", 0., 0., false);
    pointer(&mut e, "up", 100., 40., false);
    command(&mut e, json!({"type":"setTool","tool":"brush"}));
    assert_eq!(e.history.document.layers.len(), count);
}
#[test]
fn upstream_text_package_roundtrip_keeps_styles() {
    let mut e = editor();
    text(&mut e, 0., 0., "Café 日本語\nSecond line");
    command(
        &mut e,
        json!({"type":"updateText","patch":{"alignment":"right","tracking":3,"leading":90,"fontName":"monospace"}}),
    );
    command(&mut e, json!({"type":"commitText"}));
    let dir = tempfile::tempdir().unwrap();
    for extension in ["picsie", "comp"] {
        let path = dir.path().join(format!("text.{extension}"));
        picsie_core::files::save_project(&path, &e.history.document).unwrap();
        let reopened = picsie_core::files::open_project(&path).unwrap();
        assert_eq!(
            reopened.layers[0].text_layout,
            e.history.document.layers[0].text_layout
        );
        assert_eq!(
            reopened.layers[0].content,
            e.history.document.layers[0].content
        );
    }
}
#[test]
fn local_text_resize_reflows_without_changing_font_size() {
    let mut e = editor();
    text(
        &mut e,
        40.,
        60.,
        "Paragraph words wrap when the box becomes smaller",
    );
    let layer = e.selected().unwrap().clone();
    let old_width = layer.width;
    let bottom = picsie_core::geometry::bounds_point(&layer, Point::new(1., 1.));
    pointer(&mut e, "down", bottom.x, bottom.y, false);
    pointer(&mut e, "up", bottom.x - 100., bottom.y + 50., false);
    let new = e.selected().unwrap();
    assert!(new.width < old_width);
    assert!(!new.text_layout.as_ref().unwrap().point);
    assert_eq!(new.scale_x, 1.);
    assert_eq!(new.content, layer.content);
    command(&mut e, json!({"type":"cancelText"}));
    assert!(e.history.document.layers.is_empty());
}
#[test]
fn local_invalid_text_draft_is_atomic_and_utf8_cursor_is_validated() {
    let mut e = editor();
    text(&mut e, 10., 10., "Café🙂");
    let before = e.history.document.clone();
    assert!(
        e.command(
            serde_json::from_value(json!({"type":"updateText","patch":{"fontSize":0}})).unwrap()
        )
        .is_err()
    );
    assert_eq!(e.history.document, before);
    assert!(
        e.command(Command::SetTextSelection { anchor: 4, head: 4 })
            .is_err()
    );
    command(
        &mut e,
        json!({"type":"setTextSelection","anchor":5,"head":9}),
    );
    let mut renderer = picsie_core::render::Renderer::default();
    let _ = renderer.export(&e.history.document, false).unwrap();
    assert_eq!(text::byte_at("Café🙂", 6), 9);
}
#[test]
fn local_alignment_tracking_leading_and_transparent_export() {
    let mut e = editor();
    let mut l = Layer::new(
        "Type",
        200,
        160,
        Content::Text {
            text: "TYPE\nTYPE".into(),
            font_size: 32.,
            font_family: FontFamily::SansSerif,
            color: "#ff0000".into(),
        },
    );
    l.text_layout = Some(TextLayout {
        alignment: TextAlignment::Right,
        tracking: 3.,
        leading: 50.,
        ..Default::default()
    });
    e.history.document.layers.push(l.clone());
    let para = text::paragraph(&l, Some(176.));
    assert!(para.get_line_metrics()[0].left > 0.);
    let mut r = picsie_core::render::Renderer::default();
    let im = r.layer_surface(&l).unwrap();
    let bytes = picsie_core::render::rgba_pixels(&im).unwrap();
    assert!(bytes.chunks_exact(4).any(|p| p[3] == 0));
    assert!(
        bytes
            .chunks_exact(4)
            .any(|p| p[0] > 0 && p[1] == 0 && p[2] == 0 && p[3] > 0)
    );
    let current = text::caret(&l, 0);
    let world = picsie_core::geometry::to_world(
        &l,
        Point::new(current.left as f64, current.top as f64 + 5.),
    );
    assert_eq!(text::hit(&l, world), 0);
    assert!(!text::installed_fonts().is_empty());
}

#[test]
fn local_legacy_translucent_text_comp_preserves_raster_alpha() {
    let mut e = editor();
    text(&mut e, 10., 10., "Alpha text");
    command(
        &mut e,
        json!({"type":"updateText","patch":{"color":"#ff000080"}}),
    );
    command(&mut e, json!({"type":"commitText"}));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("alpha.comp");
    let mut renderer = picsie_core::render::Renderer::default();
    let before = renderer.export(&e.history.document, false).unwrap();
    picsie_core::comp::save(&path, &e.history.document).unwrap();
    let opened = picsie_core::comp::open(&path).unwrap();
    assert!(matches!(
        opened.layers[0].content.as_ref(),
        Content::Image { .. }
    ));
    let after = renderer.export(&opened, false).unwrap();
    assert_eq!(before, after);
}

#[test]
fn local_empty_text_caret_stays_inside_the_new_box() {
    let mut e = editor();
    text(&mut e, 20., 20., "");
    let layer = e.selected().unwrap();
    let caret = text::caret(layer, 0);
    assert!(caret.top >= 0. && caret.bottom <= layer.height as f32);
}

#[test]
fn local_caret_navigation_uses_wrapped_rendered_rows_and_extends_selection() {
    let mut e = editor();
    command(&mut e, json!({"type":"setTool","tool":"text"}));
    pointer(&mut e, "down", 20., 20., false);
    pointer(&mut e, "up", 220., 220., false);
    command(
        &mut e,
        json!({"type":"updateText","patch":{"text":"First words wrap across several rendered lines. Café stays editable.","fontSize":24,"leading":32}}),
    );
    command(
        &mut e,
        json!({"type":"setTextSelection","anchor":0,"head":0}),
    );
    let before = e.history.info().undo_count;
    let top = e.snapshot()["textCaret"]["y"].as_f64().unwrap();
    command(
        &mut e,
        json!({"type":"moveTextCaret","direction":"down","extend":true}),
    );
    assert_eq!(e.text_selection.0, 0);
    assert!(e.text_selection.1 > 0);
    assert!(e.snapshot()["textCaret"]["y"].as_f64().unwrap() > top + 20.);
    command(
        &mut e,
        json!({"type":"moveTextCaret","direction":"up","extend":false}),
    );
    assert_eq!(e.text_selection, (0, 0));
    command(
        &mut e,
        json!({"type":"moveTextCaret","direction":"lineEnd","extend":false}),
    );
    assert!(e.text_selection.1 > 0 && e.text_selection.1 < content(&e).len());
    assert!((e.snapshot()["textCaret"]["y"].as_f64().unwrap() - top).abs() < 1.);
    command(
        &mut e,
        json!({"type":"moveTextCaret","direction":"lineStart","extend":false}),
    );
    assert_eq!(e.text_selection, (0, 0));
    assert_eq!(e.history.info().undo_count, before);
}

#[test]
fn local_reference_property_commands_update_the_same_text_draft() {
    let mut e = editor();
    text(&mut e, 20., 20., "");
    command(
        &mut e,
        json!({"type":"beginPropertyEdit","label":"Edit text"}),
    );
    command(
        &mut e,
        json!({"type":"updateLayer","patch":{"content":{"kind":"text","text":"Reference UI","fontSize":24,"fontFamily":"serif","color":"#123456"}}}),
    );
    assert!(e.text_editing());
    assert_eq!(e.history.info().undo_count, 0);
    assert_eq!(content(&e), "Reference UI");
    assert!(e.selected().unwrap().width > 100);
    command(&mut e, json!({"type":"finishGesture"}));
    assert!(!e.text_editing());
    assert_eq!(e.history.info().undo_count, 1);
    command(&mut e, json!({"type":"undo"}));
    assert!(e.history.document.layers.is_empty());
}

#[test]
fn local_text_overlay_keeps_white_handles_and_overset_corner_when_flipped() {
    let mut e = editor();
    command(&mut e, json!({"type":"setTool","tool":"text"}));
    pointer(&mut e, "down", 50., 50., false);
    pointer(&mut e, "up", 250., 80., false);
    command(
        &mut e,
        json!({"type":"updateText","patch":{"text":"First line\nSecond line\nThird line","fontSize":24}}),
    );
    command(
        &mut e,
        json!({"type":"updateLayer","patch":{"flipX":true,"flipY":true}}),
    );
    let id = e.selected().unwrap().id.clone();
    command(&mut e, json!({"type":"editText","id":id}));
    assert!(text::overset(e.selected().unwrap()));
    let length = content(&e).len();
    command(
        &mut e,
        json!({"type":"setTextSelection","anchor":0,"head":length}),
    );
    let mut surface = picsie_core::render::surface(400, 300).unwrap();
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    e.draw_text_editor(surface.canvas());
    let pixels = picsie_core::render::rgba_pixels(&surface.image_snapshot()).unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * 400 + x) * 4..(y * 400 + x + 1) * 4];
    assert_eq!(pixel(250, 80), &[0, 0, 0, 255]);
    assert_eq!(pixel(248, 78), &[255, 255, 255, 255]);
    assert_eq!(pixel(50, 50), &[255, 255, 255, 255]);
    assert!(
        pixels
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, _)| *i / 400 < 46 || *i / 400 > 84)
            .all(|(_, pixel)| pixel[3] == 0),
        "overset selection escapes the paragraph box"
    );
}
