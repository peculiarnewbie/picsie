//! Forms follow src/ui/canvas-size*.ts and Compositor's CanvasSizeSheet / ColorPickerSheet.
//! MIT © 2026 Wonder Assembly LLC. Only transient form calculations live here.
mod color;
mod image;
use super::*;
use color::{Hsb, parse_hex};
use gpui_kit::component::checkbox::Checkbox;
use image::ImageDraft;
use picsie_core::canvas_size::CanvasSizeOptions;
#[derive(Clone)]
pub(super) enum ColorTarget {
    Foreground,
    Background,
    Text,
    Layer,
}
#[derive(Clone)]
pub(super) enum Modal {
    New,
    Canvas(CanvasDraft),
    Image(ImageDraft),
    Color(ColorDraft),
    SelectionAmount(&'static str),
    Close,
}
impl Modal {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::SelectionAmount(_) => "selection-amount",
            Self::Canvas(_) => "canvas-size",
            Self::Image(_) => "image-size",
            Self::Color(_) => "color",
            Self::Close => "close",
        }
    }
}
#[derive(Clone)]
pub(super) struct CanvasDraft {
    original: [f64; 2],
    size: [f64; 2],
    relative: bool,
    locked: bool,
    percent: bool,
    anchor: u8,
    fill: String,
    custom: String,
}
impl CanvasDraft {
    fn display(&self, i: usize) -> f64 {
        let n = self.size[i] - if self.relative { self.original[i] } else { 0. };
        if self.percent {
            n / self.original[i] * 100.
        } else {
            n
        }
    }
    fn set(&mut self, i: usize, n: f64) {
        self.size[i] = if self.percent {
            n * self.original[i] / 100.
        } else {
            n
        } + if self.relative { self.original[i] } else { 0. };
        if self.locked {
            self.size[1 - i] = self.size[i] * self.original[1 - i] / self.original[i];
        }
    }
    fn valid(&self) -> bool {
        self.size
            .iter()
            .all(|n| n.is_finite() && (1. ..=8192.).contains(&n.round()))
            && self.size[0].round() * self.size[1].round() <= 24_000_000.
    }
}
#[derive(Clone)]
pub(super) struct ColorDraft {
    pub(super) target: ColorTarget,
    title: String,
    pub(super) hsb: Hsb,
    original: String,
    pub(super) text_id: Option<String>,
    drag: Option<&'static str>,
}
pub(super) const COLOR_PANEL_WIDTH: f32 = 540.;
pub(super) const COLOR_PANEL_HEIGHT: f32 = 340.;
const ANCHORS: [&str; 9] = [
    "Top left",
    "Top center",
    "Top right",
    "Middle left",
    "Center",
    "Middle right",
    "Bottom left",
    "Bottom center",
    "Bottom right",
];
impl Desktop {
    pub(super) fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal = Some(Modal::New);
        self.notice.clear();
        self.fields["new-name"].update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }
    pub(super) fn open_canvas_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = &self.state else { return };
        let size = [state.document.width as f64, state.document.height as f64];
        self.send(Command::FinishGesture);
        self.modal = Some(Modal::Canvas(CanvasDraft {
            original: size,
            size,
            relative: false,
            locked: false,
            percent: false,
            anchor: 4,
            fill: "transparent".into(),
            custom: "#ffffff".into(),
        }));
        self.notice.clear();
        self.sync_canvas_fields(window, cx);
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }
    pub(super) fn open_color(
        &mut self,
        target: ColorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(state) = &self.state else { return };
        if state.paint_target == "mask"
            && matches!(target, ColorTarget::Foreground | ColorTarget::Background)
        {
            return;
        }
        let (title, color) = match target {
            ColorTarget::Foreground => ("Foreground Color".to_owned(), state.color.as_str()),
            ColorTarget::Background => (
                "Background Color".to_owned(),
                state.background_color.as_str(),
            ),
            ColorTarget::Text => (
                "Text color".to_owned(),
                if state.text_editing {
                    state.current_text.content["color"]
                        .as_str()
                        .unwrap_or("#000000")
                } else {
                    state.color.as_str()
                },
            ),
            ColorTarget::Layer => {
                let Some(layer) = state.selected() else {
                    return;
                };
                let Some(fill) = layer.fill() else { return };
                (layer.name.clone(), fill)
            }
        };
        self.modal = Some(Modal::Color(ColorDraft {
            target: target.clone(),
            title,
            hsb: Hsb::from_hex(color).unwrap_or_default(),
            original: color.to_owned(),
            text_id: (state.text_editing || matches!(target, ColorTarget::Layer))
                .then(|| state.selected().unwrap().id.clone()),
            drag: None,
        }));
        let size = window.viewport_size();
        self.color_position[0] =
            self.color_position[0].clamp(0., (f32::from(size.width) - COLOR_PANEL_WIDTH).max(0.));
        self.color_position[1] = self.color_position[1]
            .clamp(28., (f32::from(size.height) - COLOR_PANEL_HEIGHT).max(28.));
        self.notice.clear();
        self.sync_color_fields(window, cx);
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }
    pub(super) fn cancel_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::Close)) {
            menus::cancel_quit(cx);
        }
        self.modal = None;
        self.closing = false;
        self.close_after_save = false;
        if self.state.as_ref().is_some_and(|s| s.text_editing) {
            self.text.read(cx).focus_handle(cx).focus(window, cx);
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }
    pub(super) fn apply_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.commit_active_fields(window, cx);
        let Some(modal) = self.modal.clone() else {
            return;
        };
        let result = (|| -> anyhow::Result<()> {
            match modal {
                Modal::SelectionAmount(kind) => {
                    let max = if kind == "Feather" { 250 } else { 500 };
                    let amount = self
                        .field_value("selection-dialog", cx)
                        .trim()
                        .parse::<u32>()?;
                    anyhow::ensure!(
                        (1..=max).contains(&amount),
                        "Enter a whole number from 1 to {max} px"
                    );
                    let key = match kind {
                        "Expand" => "selection-amount",
                        "Contract" => "selection-contract",
                        _ => "feather",
                    };
                    self.set_field(key, amount.to_string(), true, window, cx);
                    self.send(match kind {
                        "Expand" => Command::ExpandSelection { amount },
                        "Contract" => Command::ContractSelection { amount },
                        _ => Command::FeatherSelection { amount },
                    });
                    self.cancel_modal(window, cx);
                }
                Modal::New => {
                    let width = number(&self.field_value("new-width", cx))?;
                    let height = number(&self.field_value("new-height", cx))?;
                    anyhow::ensure!(
                        width.fract() == 0.
                            && height.fract() == 0.
                            && (1. ..=8192.).contains(&width)
                            && (1. ..=8192.).contains(&height),
                        "Use whole dimensions from 1–8192 pixels"
                    );
                    let document = Document::new(
                        &self.field_value("new-name", cx),
                        width as u32,
                        height as u32,
                    )?;
                    crate::open_editor(document, None, cx)?;
                    self.cancel_modal(window, cx);
                }
                Modal::Image(draft) => {
                    anyhow::ensure!(draft.valid(), "Invalid image dimensions or resolution");
                    self.blocking(Operation::ResizeImage(draft.options()));
                }
                Modal::Canvas(draft) => {
                    anyhow::ensure!(
                        draft.valid(),
                        "Use 1–8192 pixels per side, up to 24 megapixels"
                    );
                    let fill = match draft.fill.as_str() {
                        "transparent" => None,
                        "foreground" => self.state.as_ref().map(|s| s.color.clone()),
                        "background" => self.state.as_ref().map(|s| s.background_color.clone()),
                        "black" => Some("#000000".into()),
                        "white" => Some("#ffffff".into()),
                        _ => Some(draft.custom),
                    };
                    self.blocking(Operation::ResizeCanvas(CanvasSizeOptions {
                        width: draft.size[0].round() as u32,
                        height: draft.size[1].round() as u32,
                        anchor: draft.anchor,
                        fill,
                    }));
                }
                Modal::Color(draft) => {
                    let value = format!("#{}", draft.hsb.hex());
                    match draft.target {
                        ColorTarget::Foreground => self.send(Command::SetPaletteColor {
                            color: value,
                            background: false,
                        }),
                        ColorTarget::Background => self.send(Command::SetPaletteColor {
                            color: value,
                            background: true,
                        }),
                        ColorTarget::Text => self.send(Command::SetTextColor {
                            color: value,
                            draft_id: draft.text_id,
                        }),
                        ColorTarget::Layer => {
                            let key = if self
                                .state
                                .as_ref()
                                .and_then(Snapshot::selected)
                                .is_some_and(|l| l.kind() == "gradient")
                            {
                                "from"
                            } else {
                                "color"
                            };
                            self.engine.request(Operation::ContentField {
                                key,
                                value: value.into(),
                            });
                        }
                    }
                    self.cancel_modal(window, cx);
                }
                Modal::Close => {
                    self.modal = None;
                    self.close_after_save = true;
                    self.file_action(FileAction::Save, window, cx);
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.notice = error.to_string();
        }
        cx.notify();
    }
    pub(super) fn canvas_field(
        &mut self,
        key: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if let Some(Modal::Canvas(draft)) = &mut self.modal {
            match key {
                "canvas-width" => draft.set(0, number(value).unwrap_or(f64::NAN)),
                "canvas-height" => draft.set(1, number(value).unwrap_or(f64::NAN)),
                "canvas-custom" => draft.custom = value.into(),
                _ => {}
            }
            self.sync_canvas_fields(window, cx);
        }
        Ok(())
    }
    pub(super) fn canvas_choice(
        &mut self,
        key: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Modal::Canvas(draft)) = &mut self.modal {
            match key {
                "canvas-unit" => draft.percent = value == "percent",
                "canvas-fill" => draft.fill = value.into(),
                _ => {}
            }
            self.sync_canvas_fields(window, cx);
        }
    }
    fn sync_canvas_fields(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Modal::Canvas(draft)) = &self.modal {
            self.set_field("canvas-width", decimal(draft.display(0)), false, window, cx);
            self.set_field(
                "canvas-height",
                decimal(draft.display(1)),
                false,
                window,
                cx,
            );
            self.set_field("canvas-custom", draft.custom.clone(), false, window, cx);
            self.sync_select(
                "canvas-unit",
                if draft.percent { "percent" } else { "pixels" },
                window,
                cx,
            );
            self.sync_select("canvas-fill", &draft.fill, window, cx);
        }
    }
    pub(super) fn color_field(
        &mut self,
        key: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if let Some(Modal::Color(draft)) = &mut self.modal {
            if key == "color-hex" {
                if let Some(rgb) = parse_hex(value) {
                    draft.hsb.set_rgb(rgb);
                }
            } else {
                let mut rgb = draft.hsb.rgb();
                let i = match key {
                    "color-r" => 0,
                    "color-g" => 1,
                    _ => 2,
                };
                rgb[i] = number(value)?.round().clamp(0., 255.) as u8;
                draft.hsb.set_rgb(rgb);
            }
            self.sync_color_fields(window, cx);
        }
        Ok(())
    }
    pub(super) fn sync_color_fields(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Modal::Color(draft)) = &self.modal {
            self.set_field("color-hex", draft.hsb.hex(), false, window, cx);
            for (key, value) in ["color-r", "color-g", "color-b"]
                .into_iter()
                .zip(draft.hsb.rgb())
            {
                self.set_field(key, value.to_string(), false, window, cx);
            }
        }
    }
    pub(super) fn color_drag(
        &mut self,
        position: Point<Pixels>,
        up: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Modal::Color(draft)) = &mut self.modal else {
            return;
        };
        let Some(area) = draft.drag else { return };
        let bounds = self.probes.lock().unwrap().get(area).copied();
        if let Some([left, top, width, height]) = bounds {
            let x = ((f32::from(position.x) - left) / width).clamp(0., 1.) as f64;
            let y = ((f32::from(position.y) - top) / height).clamp(0., 1.) as f64;
            if area == "color-sv" {
                draft.hsb.s = x;
                draft.hsb.v = 1. - y;
            } else {
                draft.hsb.h = (1. - y) * 360.;
            }
        }
        if up {
            draft.drag = None
        }
        self.sync_color_fields(window, cx);
        cx.notify();
    }
    pub(super) fn modal_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let modal = self.modal.as_ref()?;
        let (title, width, content) = match modal {
            Modal::SelectionAmount(kind) => {
                let key = if *kind == "Feather" {
                    "feather"
                } else {
                    "selection-amount"
                };
                let max = if *kind == "Feather" { 250 } else { 500 };
                (
                    format!("{kind} Selection"),
                    380.,
                    column()
                        .gap(px(16.))
                        .child(
                            row()
                                .gap(px(10.))
                                .child(label("Amount").w(px(60.)))
                                .child(
                                    self.probe(
                                        "selection-dialog-slider",
                                        Slider::new(&self.sliders[key]),
                                    )
                                    .flex_1(),
                                )
                                .child(self.inline_field("selection-dialog", "", 56., false))
                                .child(label("px")),
                        )
                        .child(
                            hint(format!("Enter a whole number from 1 to {max} px.")).opacity(
                                if self
                                    .field_value("selection-dialog", cx)
                                    .trim()
                                    .parse::<u32>()
                                    .is_ok_and(|n| (1..=max).contains(&n))
                                {
                                    0.
                                } else {
                                    1.
                                },
                            ),
                        )
                        .child(div().w_full().h(px(1.)).bg(rgb(LINE))),
                )
            }
            Modal::New => (
                "New canvas".to_owned(),
                360.,
                column()
                    .child(hint("Start with a transparent canvas."))
                    .child(self.field("new-name", "Project name", false))
                    .child(
                        row()
                            .child(self.field("new-width", "Width (px)", false))
                            .child(self.field("new-height", "Height (px)", false)),
                    ),
            ),
            Modal::Close => (
                format!(
                    "Save changes to {}?",
                    self.state
                        .as_ref()
                        .map(|s| s.document.name.as_str())
                        .unwrap_or("project")
                ),
                420.,
                column().child(hint("Your project has unsaved edits.")),
            ),
            Modal::Image(draft) => (
                "Image Size".into(),
                430.,
                self.image_size_content(draft, cx),
            ),
            Modal::Canvas(draft) => {
                let mut anchors = column().gap(px(4.));
                for r in 0..3 {
                    let mut cells = row().gap(px(4.));
                    for c in 0..3 {
                        let i = r * 3 + c;
                        cells = cells.child(
                            self.probe(
                                format!("anchor-{i}"),
                                self.raw_button(
                                    format!("anchor-button-{i}"),
                                    "●",
                                    draft.anchor == i,
                                    cx,
                                )
                                .w(px(30.))
                                .tooltip(ANCHORS[i as usize])
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        if let Some(Modal::Canvas(d)) = &mut this.modal {
                                            d.anchor = i;
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                        );
                    }
                    anchors = anchors.child(cells);
                }
                let content=column().gap(px(12.)).child(hint(format!("Current: {} × {} pixels",draft.original[0],draft.original[1])))
                    .child(row().items_end().child(self.select("canvas-unit","Units",false)).child(self.field("canvas-width","Width",false)).child(self.field("canvas-height","Height",false)))
                    .child(self.probe("canvas-relative",Checkbox::new("relative-check").label("Relative to current dimensions").checked(draft.relative).on_click(cx.listener(|this,value,window,cx|{if let Some(Modal::Canvas(d))=&mut this.modal{d.relative=*value;}this.sync_canvas_fields(window,cx);cx.notify();}))))
                    .child(self.probe("canvas-locked",Checkbox::new("locked-check").label("Lock original aspect ratio").checked(draft.locked).on_click(cx.listener(|this,value,window,cx|{if let Some(Modal::Canvas(d))=&mut this.modal{d.locked=*value;if *value{d.set(0,d.display(0));}}this.sync_canvas_fields(window,cx);cx.notify();}))))
                    .child(hint(if draft.valid(){format!("New: {} × {} pixels",draft.size[0].round(),draft.size[1].round())}else{"Use 1–8192 pixels per side, up to 24 megapixels.".into()}))
                    .child(row().items_start().gap(px(20.)).child(column().child(label("Anchor")).child(anchors)).child(column().flex_1().child(ANCHORS[draft.anchor as usize]).child(hint("Keeps this point fixed. Artwork is not scaled; cropped content remains outside the canvas."))))
                    .child(self.select("canvas-fill","Canvas extension",false)).when(draft.fill=="custom",|d|d.child(self.field("canvas-custom","Extension color",false)));
                ("Canvas Size".into(), 440., content)
            }
            Modal::Color(draft) => {
                let hue = hex_color(
                    &Hsb {
                        h: draft.hsb.h,
                        s: 1.,
                        v: 1.,
                    }
                    .hex(),
                );
                let sv = self
                    .probe(
                        "color-sv",
                        div()
                            .w(px(256.))
                            .h(px(256.))
                            .relative()
                            .bg(gradient(90., rgb(0xffffff), hue))
                            .child(div().absolute().size_full().bg(gradient(
                                180.,
                                hsla(0., 0., 0., 0.),
                                rgb(0),
                            )))
                            .child(
                                div()
                                    .absolute()
                                    .left(px((draft.hsb.s * 256. - 6.) as f32))
                                    .top(px(((1. - draft.hsb.v) * 256. - 6.) as f32))
                                    .size(px(12.))
                                    .rounded_full()
                                    .border_2()
                                    .border_color(rgb(0xffffff)),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            if let Some(Modal::Color(d)) = &mut this.modal {
                                d.drag = Some("color-sv");
                            }
                            this.color_drag(event.position, false, window, cx);
                            cx.stop_propagation();
                        }),
                    );
                let mut strip = div().w(px(20.)).h(px(256.)).flex().flex_col().relative();
                for i in 0..6 {
                    strip = strip.child(
                        div().w_full().flex_1().bg(gradient(
                            180.,
                            hex_color(
                                &Hsb {
                                    h: 360. - i as f64 * 60.,
                                    s: 1.,
                                    v: 1.,
                                }
                                .hex(),
                            ),
                            hex_color(
                                &Hsb {
                                    h: 300. - i as f64 * 60.,
                                    s: 1.,
                                    v: 1.,
                                }
                                .hex(),
                            ),
                        )),
                    );
                }
                strip = strip.child(
                    div()
                        .absolute()
                        .top(px(((1. - draft.hsb.h / 360.) * 256.).clamp(0., 253.) as f32))
                        .left(px(-3.))
                        .w(px(26.))
                        .h(px(3.))
                        .bg(rgb(0xffffff)),
                );
                let strip = self.probe("color-hue", strip).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        if let Some(Modal::Color(d)) = &mut this.modal {
                            d.drag = Some("color-hue");
                        }
                        this.color_drag(event.position, false, window, cx);
                        cx.stop_propagation();
                    }),
                );
                let mut fields = column().gap(px(6.));
                for (key, title, accessibility) in [
                    ("color-r", "R", "Red"),
                    ("color-g", "G", "Green"),
                    ("color-b", "B", "Blue"),
                    ("color-hex", "#", "Hex color"),
                ] {
                    fields = fields.child(
                        row().gap(px(8.)).child(label(title).w(px(14.))).child(
                            self.numeric_field(
                                key,
                                Input::new(&self.fields[key])
                                    .small()
                                    .map(|input| Styled::h(input, px(26.)))
                                    .bg(rgb(0x202020))
                                    .text_size(px(12.))
                                    .aria_label(accessibility)
                                    .w(px(if key == "color-hex" { 84. } else { 52. })),
                            ),
                        ),
                    );
                }
                let preview = div()
                    .id("color-comparison")
                    .w(px(64.))
                    .h(px(64.))
                    .rounded(px(5.))
                    .overflow_hidden()
                    .aria_label("New and current colors")
                    .child(div().w_full().h(px(32.)).bg(hex_color(&draft.hsb.hex())))
                    .child(div().w_full().h(px(32.)).bg(hex_color(&draft.original)));
                let actions =
                    column()
                        .gap(px(8.))
                        .w(px(90.))
                        .child(
                            self.probe(
                                "modal-apply",
                                self.raw_button("color-apply", "OK", true, cx)
                                    .w_full()
                                    .h(px(28.))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.apply_modal(window, cx)
                                    })),
                            ),
                        )
                        .child(
                            self.probe(
                                "modal-cancel",
                                self.raw_button("color-cancel", "Cancel", false, cx)
                                    .w_full()
                                    .h(px(28.))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.cancel_modal(window, cx)
                                    })),
                            ),
                        );
                let content = row()
                    .items_start()
                    .gap(px(14.))
                    .child(sv)
                    .child(div().w(px(34.)).px(px(7.)).child(strip))
                    .child(
                        column()
                            .w(px(180.))
                            .h(px(256.))
                            .child(
                                row()
                                    .items_start()
                                    .gap(px(16.))
                                    .child(preview)
                                    .child(actions),
                            )
                            .child(div().flex_1().min_h(px(12.)))
                            .child(fields)
                            .child(hint("Click the canvas to sample").mt(px(8.))),
                    );
                (draft.title.clone(), COLOR_PANEL_WIDTH, content.into())
            }
        };
        let can_apply = !self.busy
            && !matches!(modal,Modal::Canvas(d) if !d.valid())
            && !matches!(modal,Modal::Image(d) if !d.valid())
            && !matches!(modal,Modal::SelectionAmount(kind) if !self.field_value("selection-dialog", cx).trim().parse::<u32>().is_ok_and(|n| (1..=if *kind == "Feather" {250} else {500}).contains(&n)));
        let mut buttons = row().justify_end().child(
            self.probe(
                "modal-cancel",
                self.raw_button("cancel-modal", "Cancel", false, cx)
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.cancel_modal(window, cx))),
            ),
        );
        if matches!(modal, Modal::Image(_) | Modal::SelectionAmount(_)) {
            buttons = buttons.justify_start().child(div().flex_1());
        }
        if matches!(modal, Modal::Close) {
            buttons = buttons.child(
                self.probe(
                    "modal-discard",
                    self.raw_button("discard-modal", "Discard", false, cx)
                        .on_click(|_, window, _| window.remove_window()),
                ),
            );
        }
        let label = match modal {
            Modal::New => "Create",
            Modal::Canvas(_) => "Resize canvas",
            Modal::Image(_) => "Resize image",
            Modal::Color(_) | Modal::SelectionAmount(_) => "OK",
            Modal::Close => "Save",
        };
        buttons = buttons.child(
            self.probe(
                "modal-apply",
                self.raw_button("apply-modal", label, true, cx)
                    .disabled(!can_apply)
                    .on_click(cx.listener(|this, _, window, cx| this.apply_modal(window, cx))),
            ),
        );
        let popup = column()
            .w(px(width))
            .p(px(if matches!(modal, Modal::Color(_)) {
                20.
            } else {
                24.
            }))
            .gap(px(if matches!(modal, Modal::Color(_)) {
                12.
            } else {
                0.
            }))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(
                self.probe("color-panel-title", div())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            if matches!(this.modal, Some(Modal::Color(_))) {
                                this.color_panel_drag = Some((event.position, this.color_position));
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(content)
            .when(!self.notice.is_empty(), |d| {
                d.child(hint(self.notice.clone()).text_color(rgb(0xffb49b)))
            })
            .when(!matches!(modal, Modal::Color(_)), |d| d.child(buttons));
        if matches!(modal, Modal::Color(_)) {
            return Some(
                self.probe("color-panel", popup)
                    .occlude()
                    .absolute()
                    .left(px(self.color_position[0]))
                    .top(px(self.color_position[1]))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        match event.keystroke.key.as_str() {
                            "escape" => {
                                this.cancel_modal(window, cx);
                                cx.stop_propagation();
                            }
                            "enter" if window.focused_input(cx).is_none() => {
                                this.apply_modal(window, cx);
                                cx.stop_propagation();
                            }
                            _ => {}
                        }
                    }))
                    .into_any_element(),
            );
        }
        let weak = cx.weak_entity();
        Some(
            gpui_kit::base::Dialog::new(cx)
                .focus_handle(self.modal_focus.clone())
                .backdrop(div().size_full().bg(rgba(0x00000099)))
                .popup(popup)
                .close_on_backdrop_press(false)
                .on_open_change(move |open, reason, window, cx| {
                    if !open {
                        let _ = weak.update(cx, |this, cx| {
                            if reason == gpui_kit::base::DialogChangeReason::Confirm {
                                this.apply_modal(window, cx)
                            } else if !this.busy {
                                this.cancel_modal(window, cx)
                            }
                        });
                    }
                })
                .into_any_element(),
        )
    }
}

fn gradient(angle: f32, from: impl Into<Hsla>, to: impl Into<Hsla>) -> Background {
    gpui_kit::linear_gradient(
        angle,
        linear_color_stop(from, 0.),
        linear_color_stop(to, 1.),
    )
}

pub(super) fn color_rgb(text: &str) -> Option<[u8; 3]> {
    parse_hex(text)
}

impl Desktop {
    pub(super) fn open_selection_amount(
        &mut self,
        kind: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.state.as_ref().is_none_or(|s| !s.can_modify_selection) {
            return;
        }
        let key = match kind {
            "Expand" => "selection-amount",
            "Contract" => "selection-contract",
            _ => "feather",
        };
        let value = self.field_value(key, cx);
        self.modal = Some(Modal::SelectionAmount(kind));
        self.notice.clear();
        self.set_field("selection-dialog", value.clone(), true, window, cx);
        let slider = if kind == "Feather" {
            "feather"
        } else {
            "selection-amount"
        };
        self.sliders[slider].update(cx, |slider, cx| {
            slider.set_value(number(&value).unwrap_or(1.) as f32, window, cx)
        });
        self.fields["selection-dialog"].update(cx, |field, cx| field.focus(window, cx));
        cx.notify();
    }
}
