mod assets;
mod engine;
mod measure;
mod state;
mod ui;
use gpui_kit::{
    component::{Theme, ThemeMode},
    *,
};
use picsie_core::model::{Document, demo_document};
use std::path::PathBuf;
fn open_editor(document: Document, path: Option<PathBuf>, cx: &mut App) -> anyhow::Result<()> {
    gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1280.), px(860.)),
                cx,
            ))),
            window_min_size: Some(size(px(960.), px(640.))),
            titlebar: Some(TitlebarOptions {
                title: Some("Picsie".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
        |window, cx| cx.new(|cx| ui::Desktop::new(document, path, window, cx)),
    )?;
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.iter().any(|arg| arg == "--measure") {
        return measure::run();
    }
    let path = arguments
        .iter()
        .position(|arg| arg == "--open")
        .and_then(|i| arguments.get(i + 1))
        .map(PathBuf::from);
    let document = match &path {
        Some(path) => picsie_core::files::open_project(path)?,
        None => demo_document(),
    };
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            Theme::update(cx, |theme| {
                // Palette from src/ui/controls.tsx, including native Kit popup surfaces.
                theme.radius = px(5.);
                theme.colors.background = rgb(0x1b1d23).into();
                theme.colors.foreground = rgb(0xe9eaf0).into();
                theme.colors.border = rgb(0x343843).into();
                theme.colors.input = rgb(0x343843).into();
                theme.colors.muted_foreground = rgb(0x979faf).into();
                theme.colors.popover = rgb(0x22252d).into();
                theme.colors.popover_foreground = rgb(0xe9eaf0).into();
                theme.colors.list = rgb(0x22252d).into();
                theme.colors.list_active = rgb(0x363e58).into();
                theme.colors.list_active_border = rgb(0x363e58).into();
                theme.colors.list_hover = rgb(0x3a4160).into();
                theme.colors.accent = rgb(0xa5b4fc).into();
                theme.colors.accent_foreground = rgb(0x171b2c).into();
                theme.colors.ring = rgb(0xa5b4fc).into();
                theme.colors.slider_bar = rgb(0xa5b4fc).into();
                theme.colors.slider_thumb = rgb(0xedf0fc).into();
            });
            ui::menus::install(cx);
            open_editor(document, path, cx).expect("open Picsie window");
        });
    Ok(())
}
