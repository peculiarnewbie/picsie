use super::*;
pub const BG: u32 = 0x1b1d23;
pub const PANEL: u32 = 0x22252d;
pub const LINE: u32 = 0x343843;
pub const TEXT: u32 = 0xe9eaf0;
pub const MUTED: u32 = 0x979faf;
pub const ACCENT: u32 = 0xa5b4fc;
pub const FIELD_KEYS: &[&str] = &[
    "foreground",
    "opacity",
    "brightness",
    "saturation",
    "blur",
    "name",
    "x",
    "y",
    "width",
    "height",
    "rotation",
    "font-size",
    "brush-size",
    "brush-opacity",
    "hardness",
    "smoothing",
    "selection-amount",
    "feather",
    "new-name",
    "new-width",
    "new-height",
    "canvas-width",
    "canvas-height",
    "canvas-custom",
    "color-hex",
    "color-r",
    "color-g",
    "color-b",
];
pub const BLENDS: &[(&str, &str)] = &[
    ("source-over", "Normal"),
    ("multiply", "Multiply"),
    ("screen", "Screen"),
    ("overlay", "Overlay"),
    ("darken", "Darken"),
    ("lighten", "Lighten"),
    ("soft-light", "Soft light"),
    ("hard-light", "Hard light"),
    ("difference", "Difference"),
    ("exclusion", "Exclusion"),
    ("color-dodge", "Color dodge"),
    ("color-burn", "Color burn"),
    ("hue", "Hue"),
    ("saturation", "Saturation"),
    ("color", "Color"),
    ("luminosity", "Luminosity"),
];
pub const FONTS: &[(&str, &str)] = &[
    ("sans-serif", "Sans serif"),
    ("serif", "Serif"),
    ("monospace", "Monospace"),
];
pub const UNITS: &[(&str, &str)] = &[("pixels", "Pixels"), ("percent", "Percent")];
pub const FILLS: &[(&str, &str)] = &[
    ("transparent", "Transparent"),
    ("foreground", "Foreground"),
    ("black", "Black"),
    ("white", "White"),
    ("custom", "Custom"),
];
pub const SWATCHES: &[&str] = &[
    "#fff5e8", "#a5b4fc", "#6587ff", "#f6a484", "#e86c86", "#18213b",
];
pub fn column() -> Div {
    div().flex().flex_col().gap(px(10.)).min_w_0()
}
pub fn row() -> Div {
    div().flex().items_center().gap(px(8.)).min_w_0()
}
pub fn label(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(10.))
        .line_height(px(14.))
        .text_color(rgb(MUTED))
        .child(text.into())
}
pub fn hint(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(rgb(MUTED))
        .child(text.into())
}
pub fn section(title: &'static str) -> Div {
    column()
        .p(px(14.))
        .border_b_1()
        .border_color(rgb(LINE))
        .flex_shrink_0()
        .child(
            div()
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(MUTED))
                .child(title.to_uppercase()),
        )
}
pub fn icon(name: &str, size: f32) -> Icon {
    Icon::default()
        .path(SharedString::from(format!("picsie/{name}.svg")))
        .size(px(size))
}
pub fn hex_color(value: &str) -> Hsla {
    rgb(u32::from_str_radix(value.trim_start_matches('#'), 16).unwrap_or(0)).into()
}
pub fn number(value: &str) -> anyhow::Result<f64> {
    let number = value
        .trim()
        .parse::<f64>()
        .map_err(|_| anyhow::anyhow!("Enter a finite number"))?;
    anyhow::ensure!(number.is_finite(), "Enter a finite number");
    Ok(number)
}
pub fn decimal(value: f64) -> String {
    let n = (value * 1000.).round() / 1000.;
    format!("{n}")
}
impl Desktop {
    pub(super) fn probe(&self, id: impl Into<String>, element: impl IntoElement) -> Stateful<Div> {
        let id = id.into();
        let layout = self.probes.clone();
        let name = id.clone();
        div()
            .id(SharedString::from(id))
            .relative()
            .child(element)
            .child(
                canvas(
                    move |bounds, _, _| {
                        layout.lock().unwrap().insert(
                            name.clone(),
                            [
                                bounds.origin.x.into(),
                                bounds.origin.y.into(),
                                bounds.size.width.into(),
                                bounds.size.height.into(),
                            ],
                        );
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }
    pub(super) fn button(
        &self,
        id: impl Into<String>,
        text: impl Into<SharedString>,
        action: Action,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let id = id.into();
        self.probe(
            id.clone(),
            self.raw_button(id, text, false, cx)
                .disabled(self.busy)
                .on_click(
                    cx.listener(move |this, _, window, cx| this.act(action.clone(), window, cx)),
                ),
        )
    }
    pub(super) fn raw_button(
        &self,
        id: impl Into<String>,
        text: impl Into<SharedString>,
        active: bool,
        cx: &App,
    ) -> Button {
        let text = text.into();
        Button::new(SharedString::from(id.into()))
            .when(!text.is_empty(), |button| button.label(text))
            .bg(rgb(if active { 0x3a4160 } else { 0x2c303a }))
            .small()
            .h(px(30.))
            .text_size(px(12.))
            .rounded(px(5.))
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(rgb(if active { 0x3a4160 } else { 0x2c303a }).into())
                    .foreground(rgb(if active { 0xcbd5ff } else { TEXT }).into())
                    .hover(rgb(0x3c4250).into())
                    .active(rgb(0x3a4160).into()),
            )
    }
    pub(super) fn command_button(
        &self,
        id: &str,
        text: &str,
        command: Command,
        disabled: bool,
        active: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        self.probe(
            id,
            self.raw_button(id, text.to_owned(), active, cx)
                .disabled(self.busy || disabled)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.act(Action::Command(command.clone()), window, cx)
                })),
        )
    }
    pub(super) fn icon_button(
        &self,
        id: &str,
        name: &str,
        tooltip: String,
        action: Action,
        disabled: bool,
        active: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        self.probe(
            id,
            self.raw_button(id, "", active, cx)
                .icon(icon(name, 18.))
                .w(px(30.))
                .tooltip(tooltip)
                .disabled(self.busy || disabled)
                .on_click(
                    cx.listener(move |this, _, window, cx| this.act(action.clone(), window, cx)),
                ),
        )
    }
    pub(super) fn field(&self, key: &'static str, title: &str, disabled: bool) -> Div {
        column()
            .gap(px(5.))
            .flex_1()
            .child(label(title.to_owned()))
            .child(
                self.probe(
                    format!("field-{key}"),
                    Input::new(&self.fields[key])
                        .small()
                        .map(|input| Styled::h(input, px(30.)))
                        .bg(rgb(0x191c22))
                        .text_size(px(12.))
                        .disabled(disabled || self.busy)
                        .aria_label(title.to_owned())
                        .w_full(),
                ),
            )
    }
    pub(super) fn select(&self, key: &'static str, title: &str, disabled: bool) -> Div {
        column()
            .gap(px(5.))
            .flex_1()
            .child(label(title.to_owned()))
            .child(
                self.probe(
                    format!("select-{key}"),
                    Select::new(&self.selects[key])
                        .small()
                        .h(px(30.))
                        .bg(rgb(0x191c22))
                        .text_size(px(12.))
                        .w_full()
                        .disabled(disabled || self.busy),
                ),
            )
    }
    pub(super) fn slider(
        &self,
        key: &'static str,
        title: &str,
        value: f64,
        unit: &str,
        disabled: bool,
    ) -> Div {
        let _ = value; // Kept with the shared inspector call signature; sync_controls owns values.
        row()
            .min_h(px(30.))
            .child(label(title.to_owned()).w(px(74.)).flex_shrink_0())
            .child(
                self.probe(
                    format!("slider-{key}"),
                    div()
                        .h(px(20.))
                        .child(Slider::new(&self.sliders[key]).disabled(disabled || self.busy)),
                )
                .flex_1()
                .min_w_0(),
            )
            .child(
                self.probe(
                    format!("field-{key}"),
                    Input::new(&self.fields[key])
                        .small()
                        .map(|input| Styled::h(input, px(30.)))
                        .bg(rgb(0x191c22))
                        .text_size(px(12.))
                        .disabled(disabled || self.busy)
                        .w(px(52.))
                        .aria_label(title.to_owned()),
                )
                .w(px(52.))
                .flex_shrink_0(),
            )
            .child(label(unit.to_owned()).w(px(14.)).flex_shrink_0())
    }
    pub(super) fn bind_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (&key, field) in &self.fields {
            self._subscriptions.push(cx.subscribe_in(
                field,
                window,
                move |this, _, event, window, cx| match event {
                    InputEvent::Change => {
                        this.edited_fields.insert(key);
                        if key.starts_with("canvas-") || key.starts_with("color-") {
                            this.commit_field(key, window, cx);
                        }
                    }
                    InputEvent::PressEnter { .. } => {
                        this.commit_field(key, window, cx);
                        if this.modal.is_none() {
                            window.focus(&this.focus, cx);
                        }
                    }
                    InputEvent::Blur => this.commit_field(key, window, cx),
                    _ => {}
                },
            ));
        }
        self._subscriptions.push(cx.subscribe_in(
            &self.text,
            window,
            |this, input, event, window, cx| match event {
                InputEvent::Change => {
                    if this.busy {
                        return;
                    }
                    if !this.text_editing {
                        this.text_editing = true;
                        this.send(Command::BeginPropertyEdit {
                            label: "Edit text".into(),
                        });
                    }
                    this.engine.request(Operation::ContentField {
                        key: "text",
                        value: input.read(cx).value().to_string().into(),
                    });
                }
                InputEvent::Blur | InputEvent::PressEnter { shift: false, .. } => {
                    if this.text_editing {
                        this.text_editing = false;
                        this.send(Command::FinishGesture);
                    }
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        window.focus(&this.focus, cx);
                    }
                }
                _ => {}
            },
        ));
        for (&key, select) in &self.selects {
            self._subscriptions.push(cx.subscribe_in(
                select,
                window,
                move |this, _, event, window, cx| {
                    if let SelectEvent::Confirm(Some(value)) = event {
                        this.commit_choice(key, value, window, cx);
                    }
                },
            ));
        }
        for (&key, slider) in &self.sliders {
            self._subscriptions.push(cx.subscribe_in(
                slider,
                window,
                move |this, _, event, _, cx| {
                    match event {
                        SliderEvent::Change(SliderValue::Single(value)) => {
                            if this.busy {
                                return;
                            }
                            if this.editing_slider != Some(key) {
                                this.send(Command::BeginPropertyEdit {
                                    label: format!("Edit layer {key}"),
                                });
                                this.editing_slider = Some(key);
                            }
                            let value = *value as f64 / if key == "blur" { 1. } else { 100. };
                            this.patch(json!({key:value}));
                        }
                        SliderEvent::Release(_) => {
                            if this.editing_slider.take().is_some() {
                                this.send(Command::FinishGesture);
                            }
                        }
                        _ => {}
                    }
                    cx.notify();
                },
            ));
        }
        for (key, value) in [
            ("selection-amount", "5"),
            ("feather", "2"),
            ("new-name", "Untitled"),
            ("new-width", "1200"),
            ("new-height", "800"),
            ("canvas-custom", "#ffffff"),
        ] {
            self.set_field(key, value, false, window, cx);
        }
    }
    pub(super) fn set_field(
        &self,
        key: &'static str,
        value: impl Into<String>,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = &self.fields[key];
        let value = value.into();
        if (force || !field.read(cx).focus_handle(cx).is_focused(window))
            && field.read(cx).value().as_str() != value
        {
            field.update(cx, |input, cx| input.set_value(value, window, cx));
        }
    }
    pub(super) fn field_value(&self, key: &'static str, cx: &App) -> String {
        self.fields[key].read(cx).value().to_string()
    }
    pub(super) fn sync_select(
        &self,
        key: &'static str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &self.selects[key];
        if state.read(cx).selected_value().map(String::as_str) != Some(value) {
            state.update(cx, |select, cx| {
                select.set_selected_value(&value.to_owned(), window, cx)
            });
        }
    }
    pub(super) fn sync_controls(
        &mut self,
        changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        for (key, value) in [
            ("foreground", state.color.clone()),
            ("brush-size", decimal(state.brush_size)),
            ("brush-opacity", decimal(state.brush_opacity * 100.)),
            ("hardness", decimal(state.brush_hardness * 100.)),
            ("smoothing", decimal(state.brush_smoothing)),
        ] {
            self.set_field(key, value, false, window, cx);
        }
        if let Some(layer) = state.selected() {
            for (key, value) in [
                ("name", layer.name.clone()),
                ("x", decimal(layer.x.round())),
                ("y", decimal(layer.y.round())),
                (
                    "width",
                    decimal((layer.width as f64 * layer.scale_x).round()),
                ),
                (
                    "height",
                    decimal((layer.height as f64 * layer.scale_y).round()),
                ),
                ("rotation", decimal((layer.rotation * 10.).round() / 10.)),
                (
                    "font-size",
                    decimal(layer.content["fontSize"].as_f64().unwrap_or(64.)),
                ),
            ] {
                self.set_field(key, value, changed, window, cx);
            }
            self.sync_select("blend", &layer.blend, window, cx);
            self.sync_select(
                "font",
                layer.content["fontFamily"].as_str().unwrap_or("sans-serif"),
                window,
                cx,
            );
            let items = state
                .mask_source_ids
                .iter()
                .filter_map(|id| state.layer(id))
                .map(|layer| Choice {
                    value: layer.id.clone(),
                    label: layer.name.clone(),
                })
                .collect();
            self.selects["mask-source"]
                .update(cx, |select, cx| select.set_items(items, window, cx));
            self.sync_select(
                "mask-source",
                layer.mask_source_id.as_deref().unwrap_or(""),
                window,
                cx,
            );
            for (key, value) in [
                ("opacity", layer.opacity * 100.),
                ("brightness", layer.brightness * 100.),
                ("saturation", layer.saturation * 100.),
                ("blur", layer.blur),
            ] {
                self.set_field(key, decimal(value), false, window, cx);
                if self.editing_slider != Some(key) {
                    self.sliders[key]
                        .update(cx, |slider, cx| slider.set_value(value as f32, window, cx));
                }
            }
            let text = layer.content["text"].as_str().unwrap_or("");
            if (changed || !self.text.read(cx).focus_handle(cx).is_focused(window))
                && self.text.read(cx).value().as_str() != text
            {
                self.text
                    .update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
            }
        }
    }
    pub(super) fn commit_active_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.edited_fields.iter().copied().collect::<Vec<_>>();
        for key in keys {
            self.commit_field(key, window, cx);
        }
        if self.text_editing {
            self.text_editing = false;
            self.send(Command::FinishGesture);
        }
        if self.editing_slider.take().is_some() {
            self.send(Command::FinishGesture);
        }
    }
    fn commit_field(&mut self, key: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.edited_fields.remove(key) {
            return;
        }
        let value = self.field_value(key, cx);
        let result = (|| -> anyhow::Result<()> {
            match key {
                "foreground" => self.send(Command::SetColor { color: value }),
                "opacity" | "brightness" | "saturation" | "blur" => {
                    let max = if matches!(key, "brightness" | "saturation") {
                        300.
                    } else {
                        100.
                    };
                    let value =
                        number(&value)?.clamp(0., max) / if key == "blur" { 1. } else { 100. };
                    self.patch(json!({key:value}));
                }
                "name" => self.patch(json!({"name":value})),
                "x" | "y" | "rotation" => self.patch(json!({key:number(&value)?})),
                "width" | "height" => {
                    if let Some(layer) = self.state.as_ref().and_then(Snapshot::selected) {
                        let (property, size) = if key == "width" {
                            ("scaleX", layer.width)
                        } else {
                            ("scaleY", layer.height)
                        };
                        self.patch(json!({property:number(&value)?/size as f64}));
                    }
                }
                "font-size" => {
                    self.engine.request(Operation::ContentField {
                        key: "fontSize",
                        value: json!(number(&value)?),
                    });
                }
                "brush-size" | "brush-opacity" | "hardness" | "smoothing" => {
                    let (property, max, divisor) = match key {
                        "brush-size" => ("size", 2000., 1.),
                        "brush-opacity" => ("opacity", 100., 100.),
                        "hardness" => ("hardness", 100., 100.),
                        _ => ("smoothing", 100., 1.),
                    };
                    self.engine.request(Operation::BrushField {
                        key: property,
                        value: number(&value)?
                            .clamp(if key == "brush-size" { 1. } else { 0. }, max)
                            / divisor,
                    });
                }
                "selection-amount" | "feather" => {
                    let n = number(&value)?
                        .round()
                        .clamp(1., if key == "feather" { 250. } else { 500. });
                    self.set_field(key, decimal(n), true, window, cx);
                }
                key if key.starts_with("canvas-") => self.canvas_field(key, &value, window, cx)?,
                key if key.starts_with("color-") => self.color_field(key, &value, window, cx)?,
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.notice = error.to_string();
        }
        cx.notify();
    }
    fn commit_choice(
        &mut self,
        key: &'static str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match key {
            "blend" => self.patch(json!({"blend":value})),
            "font" => {
                self.engine.request(Operation::ContentField {
                    key: "fontFamily",
                    value: value.into(),
                });
            }
            "mask-source" => self.send(Command::LinkMask {
                source_id: value.into(),
            }),
            _ => self.canvas_choice(key, value, window, cx),
        }
        cx.notify();
    }
}
