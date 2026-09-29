//! Native menus mirror the QuickGUI shell; focused Kit inputs own clipboard actions.
use super::*;
use gpui_kit::component::input as text_input;
gpui_kit::actions!(
    picsie,
    [
        New, Open, OpenComp, Import, Save, SaveAs, SaveComp, ExportPng, ExportJpeg, Close, Quit,
        UndoCanvas, RedoCanvas, AllPixels, Deselect, Inverse, Fill, CanvasSize, Paint, Gradient,
        Clipping, Duplicate, Raise, Lower, Fit, Actual, ZoomIn, ZoomOut
    ]
);
#[derive(Default)]
struct Quitting(bool);
impl Global for Quitting {}
pub fn cancel_quit(cx: &mut App) {
    cx.set_global(Quitting(false));
}
pub fn request_quit(window: &mut Window, cx: &mut App) {
    cx.set_global(Quitting(true));
    window.dispatch_action(Box::new(Close), cx);
}
pub fn install(cx: &mut App) {
    cx.set_global(Quitting(false));
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        } else if cx.global::<Quitting>().0 {
            let next = cx.windows()[0];
            let _ = next.update(cx, |_, window, cx| {
                window.activate_window();
                window.dispatch_action(Box::new(Close), cx);
            });
        }
    })
    .detach();
    cx.set_menus([
        Menu::new("Picsie").items([MenuItem::action("Quit Picsie", Quit)]),
        Menu::new("File").items([
            MenuItem::action("New…", New),
            MenuItem::action("Open Project…", Open),
            MenuItem::action("Open Compositor Package…", OpenComp),
            MenuItem::action("Import Image…", Import),
            MenuItem::separator(),
            MenuItem::action("Save Project", Save),
            MenuItem::action("Save Project As…", SaveAs),
            MenuItem::action("Save Compositor Package…", SaveComp),
            MenuItem::action("Export PNG…", ExportPng),
            MenuItem::action("Export JPEG…", ExportJpeg),
            MenuItem::action("Close Window", Close),
        ]),
        Menu::new("Edit").items([
            MenuItem::action("Undo Canvas Edit", UndoCanvas),
            MenuItem::action("Redo Canvas Edit", RedoCanvas),
            MenuItem::separator(),
            MenuItem::action("Cut", text_input::Cut),
            MenuItem::action("Copy", text_input::Copy),
            MenuItem::action("Paste", text_input::Paste),
            MenuItem::action("Select All", text_input::SelectAll),
        ]),
        Menu::new("Select").items([
            MenuItem::action("All Pixels", AllPixels),
            MenuItem::action("Deselect", Deselect),
            MenuItem::action("Inverse", Inverse),
            MenuItem::action("Fill with Foreground", Fill),
        ]),
        Menu::new("Image").items([MenuItem::action("Canvas Size…", CanvasSize)]),
        Menu::new("Layer").items([
            MenuItem::action("New Paint Layer", Paint),
            MenuItem::action("Create / Release Clipping Mask", Clipping),
            MenuItem::action("New Gradient Layer", Gradient),
            MenuItem::action("Duplicate Layer", Duplicate),
            MenuItem::action("Raise Layer", Raise),
            MenuItem::action("Lower Layer", Lower),
        ]),
        Menu::new("View").items([
            MenuItem::action("Fit Canvas", Fit),
            MenuItem::action("Actual Size", Actual),
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
        ]),
    ]);
}
pub(super) fn bind(element: Div, cx: &Context<Desktop>) -> Div {
    element
        .on_action(cx.listener(|this, _: &text_input::SelectAll, window, cx| {
            if this.modal.is_none() && window.focused_input(cx).is_none() {
                let pixels = this
                    .state
                    .as_ref()
                    .is_some_and(|s| matches!(s.tool, Tool::Marquee | Tool::Lasso));
                this.act(
                    Action::Command(if pixels {
                        Command::SelectAllPixels
                    } else {
                        Command::SelectAll
                    }),
                    window,
                    cx,
                );
            }
        }))
        .on_action(cx.listener(|this, _: &New, window, cx| {
            if this.modal.is_none() {
                this.act(Action::New, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Open, window, cx| {
            if this.modal.is_none() {
                this.act(Action::File(FileAction::Open), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &OpenComp, window, cx| {
            if this.modal.is_none() {
                this.act(Action::File(FileAction::OpenComp), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Import, window, cx| {
            if this.modal.is_none() {
                this.act(Action::File(FileAction::Import), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Save, window, cx| {
            if this.modal.is_none() {
                this.act(Action::File(FileAction::Save), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &SaveAs, window, cx| {
            if this.modal.is_none() {
                this.act(Action::File(FileAction::SaveAs), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &SaveComp, window, cx| {
            if this.modal.is_none() {
                this.act(Action::File(FileAction::SaveComp), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &ExportPng, window, cx| {
            if this.modal.is_none() {
                this.act(Action::File(FileAction::ExportPng), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &ExportJpeg, window, cx| {
            if this.modal.is_none() {
                this.act(Action::File(FileAction::ExportJpeg), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Close, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Close, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &CanvasSize, window, cx| {
            if this.modal.is_none() {
                this.act(Action::CanvasSize, window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &UndoCanvas, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::Undo), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &RedoCanvas, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::Redo), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &AllPixels, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::SelectAllPixels), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Deselect, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::DeselectPixels), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Inverse, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::InvertSelection), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Fill, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::FillSelection), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Paint, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::AddPaintLayer), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Gradient, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::AddGradient), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Clipping, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::ToggleClippingMask), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Duplicate, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::Duplicate), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Raise, window, cx| {
            if this.modal.is_none() {
                this.act(
                    Action::Command(Command::Reorder { direction: 1 }),
                    window,
                    cx,
                );
            }
        }))
        .on_action(cx.listener(|this, _: &Lower, window, cx| {
            if this.modal.is_none() {
                this.act(
                    Action::Command(Command::Reorder { direction: -1 }),
                    window,
                    cx,
                );
            }
        }))
        .on_action(cx.listener(|this, _: &Fit, window, cx| {
            if this.modal.is_none() {
                this.act(Action::Command(Command::Fit), window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &Actual, window, cx| {
            if this.modal.is_none() {
                this.act(
                    Action::Command(Command::Zoom {
                        zoom: 1.,
                        point: None,
                    }),
                    window,
                    cx,
                );
            }
        }))
        .on_action(cx.listener(|this, _: &Quit, window, cx| {
            if !this.busy && this.modal.is_none() {
                request_quit(window, cx);
            }
        }))
        .on_action(cx.listener(|this, _: &ZoomIn, _, _| {
            if !this.busy && this.modal.is_none() {
                this.engine.request(Operation::ZoomBy {
                    factor: 1.25,
                    point: None,
                });
            }
        }))
        .on_action(cx.listener(|this, _: &ZoomOut, _, _| {
            if !this.busy && this.modal.is_none() {
                this.engine.request(Operation::ZoomBy {
                    factor: 0.8,
                    point: None,
                });
            }
        }))
}
