#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod assets;
mod engine;
mod launch;
mod measure;
mod preferences;
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
            // Match the installed Linux desktop entry for window grouping and icons.
            app_id: Some("picsie".into()),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1280.), px(860.)),
                cx,
            ))),
            window_min_size: Some(size(px(800.), px(520.))),
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
    let paths = match launch::parse(std::env::args_os().skip(1))? {
        launch::Launch::Help => {
            println!(
                "Picsie {}\n\nUsage: picsie [--open PATH] [PROJECT ...]\n\nOpen .picsie, .electropic, or .comp projects, each in its own window.\nWith no project, open the sample composition.\n\n  -h, --help       Show this help\n  -V, --version    Show the version\n  --              Treat remaining arguments as paths",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        launch::Launch::Version => {
            println!("Picsie {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        launch::Launch::Measure { moving } => return measure::run(moving),
        launch::Launch::Projects(paths) => paths,
    };
    let documents = if paths.is_empty() {
        vec![(demo_document(), None)]
    } else {
        paths
            .into_iter()
            .map(|path| {
                picsie_core::files::open_project(&path).map(|document| (document, Some(path)))
            })
            .collect::<anyhow::Result<Vec<_>>>()?
    };
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            cx.set_app_identity("dev.peculiarnewbie.picsie", "Picsie");
            gpui_kit::init(cx);
            // BasePopover binds Space to its trigger. Text inputs must keep typing spaces.
            cx.bind_keys([KeyBinding::new("space", NoAction, Some("Input"))]);
            cx.bind_keys(
                ["up", "down", "shift-up", "shift-down"]
                    .into_iter()
                    .map(|key| KeyBinding::new(key, NoAction, Some("PicsieNumeric > Input"))),
            );
            // Canvas row navigation/spacing uses Rust's displayed paragraph geometry.
            // Suppress the hidden input's logical-line bindings before raw key capture.
            cx.bind_keys(
                [
                    "up",
                    "down",
                    "home",
                    "end",
                    "shift-up",
                    "shift-down",
                    "shift-home",
                    "shift-end",
                    "alt-up",
                    "alt-down",
                    "alt-left",
                    "alt-right",
                    "alt-shift-up",
                    "alt-shift-down",
                    "alt-shift-left",
                    "alt-shift-right",
                ]
                .into_iter()
                .map(|key| KeyBinding::new(key, NoAction, Some("PicsieCanvasText > Input"))),
            );
            Theme::change(ThemeMode::Dark, None, cx);
            Theme::update(cx, |theme| {
                // ContentView neutral dark chrome; native accent adapts across platforms.
                theme.radius = px(5.);
                theme.font_size = px(13.);
                theme.colors.background = rgb(0x242424).into();
                theme.colors.foreground = rgb(0xeeeeee).into();
                theme.colors.border = rgb(0x3a3a3a).into();
                theme.colors.input = rgb(0x3a3a3a).into();
                theme.colors.muted_foreground = rgb(0xa0a0a0).into();
                theme.colors.popover = rgb(0x282828).into();
                theme.colors.popover_foreground = rgb(0xeeeeee).into();
                theme.colors.list = rgb(0x282828).into();
                theme.colors.list_active = rgb(0x364c65).into();
                theme.colors.list_active_border = rgb(0x364c65).into();
                theme.colors.list_hover = rgb(0x404040).into();
                theme.colors.accent = rgb(0x75a7d9).into();
                theme.colors.accent_foreground = rgb(0x151515).into();
                theme.colors.ring = rgb(0x75a7d9).into();
                theme.colors.slider_bar = rgb(0x75a7d9).into();
                theme.colors.slider_thumb = rgb(0xe0e0e0).into();
            });
            ui::menus::install(cx);
            for (document, path) in documents {
                open_editor(document, path, cx).expect("open Picsie window");
            }
        });
    Ok(())
}
