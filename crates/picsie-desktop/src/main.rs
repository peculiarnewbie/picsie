#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod assets;
mod engine;
mod launch;
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
            // Match the installed Linux desktop entry for window grouping and icons.
            app_id: Some("picsie".into()),
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
            for (document, path) in documents {
                open_editor(document, path, cx).expect("open Picsie window");
            }
        });
    Ok(())
}
