use super::*;
pub const BG: u32 = 0x242424;
pub const PANEL: u32 = 0x282828;
pub const LINE: u32 = 0x3a3a3a;
pub const TEXT: u32 = 0xeeeeee;
pub const MUTED: u32 = 0xa0a0a0;
pub const ACCENT: u32 = 0x75a7d9;
pub const FIELD_KEYS: &[&str] = &[
    "foreground",
    "opacity",
    "brightness",
    "saturation",
    "blur",
    "name",
    "inline-name",
    "x",
    "y",
    "width",
    "height",
    "rotation",
    "font-size",
    "tracking",
    "leading",
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
    "image-width",
    "image-height",
    "image-resolution",
    "wand-tolerance",
];
pub const BLENDS: &[(&str, &str)] = &[
    ("source-over", "Normal"),
    ("darken", "Darken"),
    ("multiply", "Multiply"),
    ("color-burn", "Color Burn"),
    ("linear-burn", "Linear Burn"),
    ("lighten", "Lighten"),
    ("screen", "Screen"),
    ("color-dodge", "Color Dodge"),
    ("linear-dodge", "Linear Dodge (Add)"),
    ("overlay", "Overlay"),
    ("soft-light", "Soft Light"),
    ("hard-light", "Hard Light"),
    ("vivid-light", "Vivid Light"),
    ("linear-light", "Linear Light"),
    ("pin-light", "Pin Light"),
    ("hard-mix", "Hard Mix"),
    ("difference", "Difference"),
    ("exclusion", "Exclusion"),
    ("subtract", "Subtract"),
    ("divide", "Divide"),
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
pub const IMAGE_UNITS: &[(&str, &str)] = &[
    ("pixels", "Pixels"),
    ("percent", "Percent"),
    ("inches", "Inches"),
    ("centimeters", "Centimeters"),
];
pub const SAMPLING: &[(&str, &str)] = &[
    ("nearest", "Nearest"),
    ("smooth", "Smooth"),
    ("high", "High quality"),
];
pub const WAND_SAMPLES: &[(&str, &str)] = &[
    ("0", "Point sample"),
    ("1", "3 × 3 average"),
    ("2", "5 × 5 average"),
];
pub const FILLS: &[(&str, &str)] = &[
    ("transparent", "Transparent"),
    ("foreground", "Foreground"),
    ("background", "Background"),
    ("black", "Black"),
    ("white", "White"),
    ("custom", "Custom"),
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
                .child(title),
        )
}
pub fn icon(name: &str, size: f32) -> Icon {
    let icon = match name {
        "layer-add" => Icon::from(gpui_kit::assets::IconName::SquarePlus),
        "folder-add" => Icon::from(gpui_kit::assets::IconName::FolderPlus),
        "link" => Icon::from(gpui_kit::assets::IconName::Link),
        "folder" => Icon::from(gpui_kit::assets::IconName::Folder),
        "adjustments" => Icon::from(gpui_kit::assets::IconName::Contrast),
        "more" => Icon::from(gpui_kit::assets::IconName::Ellipsis),
        _ => Icon::default().path(SharedString::from(format!("picsie/{name}.svg"))),
    };
    icon.size(px(size))
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
// TransformValueField.formatted, Compositor 609dbeae: two decimals unless almost whole.
pub fn transform_decimal(value: f64) -> String {
    if (value - value.round()).abs() < 0.005 {
        format!("{:.0}", value.round())
    } else {
        format!("{value:.2}")
    }
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
            .small()
            .h(px(26.))
            .text_size(px(12.))
            .rounded(px(13.))
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(rgb(if active { 0x454545 } else { 0x373737 }).into())
                    .foreground(rgb(if active { 0xeeeeee } else { TEXT }).into())
                    .hover(rgb(0x484848).into())
                    .active(rgb(0x454545).into()),
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
        .flex_shrink_0()
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
                .ghost()
                .with_size(gpui_kit::component::Size::Size(px(24.)))
                .h(px(30.))
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
                self.numeric_field(
                    key,
                    Input::new(&self.fields[key])
                        .small()
                        .map(|input| Styled::h(input, px(30.)))
                        .bg(rgb(0x202020))
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
                        .bg(rgb(0x202020))
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
            .child(label(title.to_owned()).w(px(46.)).flex_shrink_0())
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
                self.numeric_field(
                    key,
                    Input::new(&self.fields[key])
                        .small()
                        .map(|input| Styled::h(input, px(30.)))
                        .bg(rgb(0x202020))
                        .text_size(px(12.))
                        .disabled(disabled || self.busy)
                        .w(px(44.))
                        .aria_label(title.to_owned()),
                )
                .w(px(44.))
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
                        if key.starts_with("canvas-")
                            || key.starts_with("color-")
                            || key.starts_with("image-")
                        {
                            this.commit_field(key, window, cx);
                        }
                    }
                    InputEvent::PressEnter { .. } if key == "inline-name" => {
                        this.finish_rename(true, window, cx);
                    }
                    InputEvent::Blur if key == "inline-name" => {
                        this.finish_rename(true, window, cx);
                    }
                    InputEvent::PressEnter { .. } => {
                        this.commit_field(key, window, cx);
                        if this.modal.is_none() {
                            if this.state.as_ref().is_some_and(|s| s.text_editing) {
                                this.text.update(cx, |input, cx| input.focus(window, cx));
                            } else {
                                window.focus(&this.focus, cx);
                            }
                        }
                    }
                    InputEvent::Blur => {
                        this.commit_field(key, window, cx);
                        if this.numeric_edit == Some(key) {
                            this.numeric_edit = None;
                            this.send(Command::FinishGesture);
                        }
                    }
                    _ => {}
                },
            ));
        }
        self._subscriptions.push(cx.subscribe_in(
            &self.text,
            window,
            |this, input, event, window, cx| match event {
                InputEvent::Change => {
                    if this.busy || !this.state.as_ref().is_some_and(|s| s.text_editing) {
                        return;
                    }
                    let input = input.read(cx);
                    let range = input.selected_range();
                    let head = input.cursor();
                    let anchor = if head == range.start {
                        range.end
                    } else {
                        range.start
                    };
                    this.last_text_selection = (anchor, head);
                    this.reset_caret();
                    this.text_input_sequence = this.engine.send(vec![
                        Command::UpdateText {
                            patch: picsie_core::text::TextPatch {
                                text: Some(input.value().to_string()),
                                ..Default::default()
                            },
                        },
                        Command::SetTextSelection { anchor, head },
                    ]);
                }
                InputEvent::PressEnter { shift: false, .. } => {
                    this.send(Command::CommitText);
                    window.focus(&this.focus, cx);
                }
                _ => {}
            },
        ));
        self._subscriptions
            .push(cx.observe(&self.text, |this, input, cx| {
                if !this.state.as_ref().is_some_and(|s| s.text_editing) {
                    return;
                }
                let input = input.read(cx);
                let range = input.selected_range();
                let head = input.cursor();
                let anchor = if head == range.start {
                    range.end
                } else {
                    range.start
                };
                if (anchor, head) != this.last_text_selection {
                    this.last_text_selection = (anchor, head);
                    this.reset_caret();
                    this.text_input_sequence = this
                        .engine
                        .send(vec![Command::SetTextSelection { anchor, head }]);
                }
            }));
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
                            if matches!(key, "hardness" | "brush-opacity" | "smoothing") {
                                this.engine.request(Operation::BrushField {
                                    key: match key {
                                        "brush-opacity" => "opacity",
                                        "hardness" => "hardness",
                                        _ => "smoothing",
                                    },
                                    value: *value as f64
                                        / if key == "smoothing" { 1. } else { 100. },
                                });
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
            ("wand-tolerance", decimal(state.wand.tolerance as f64)),
        ] {
            self.set_field(key, value, false, window, cx);
        }
        self.sync_select("wand-sample", &state.wand.radius.to_string(), window, cx);
        self.sync_select("crop-ratio", &state.crop_ratio, window, cx);
        for (key, value) in [
            ("hardness", state.brush_hardness * 100.),
            ("brush-opacity", state.brush_opacity * 100.),
            ("smoothing", state.brush_smoothing),
        ] {
            self.sliders[key].update(cx, |slider, cx| slider.set_value(value as f32, window, cx));
        }
        if let Some(layer) = state.selected() {
            let target = state.transform_target.as_ref().unwrap_or(layer);
            for (key, value) in [
                ("name", layer.name.clone()),
                ("x", transform_decimal(target.x)),
                ("y", transform_decimal(target.y)),
                (
                    "width",
                    transform_decimal(target.width as f64 * target.scale_x),
                ),
                (
                    "height",
                    transform_decimal(target.height as f64 * target.scale_y),
                ),
                ("rotation", transform_decimal(target.rotation)),
            ] {
                self.set_field(key, value, changed, window, cx);
            }
            self.sync_select("blend", &layer.blend, window, cx);
            let items = state
                .mask_source_ids
                .iter()
                .filter_map(|id| state.layer(id))
                .map(|layer| Choice {
                    value: layer.id.clone(),
                    label: layer.name.clone(),
                })
                .collect::<Vec<_>>();
            self.selects["mask-source"]
                .update(cx, |select, cx| select.set_items(items.into(), window, cx));
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
        }
        let layer = &state.current_text;
        let layout = layer
            .text_layout
            .clone()
            .unwrap_or_else(|| picsie_core::text::TextLayout {
                font_name: layer.content["fontFamily"]
                    .as_str()
                    .unwrap_or("sans-serif")
                    .into(),
                ..Default::default()
            });
        for (key, value) in [
            (
                "font-size",
                decimal(layer.content["fontSize"].as_f64().unwrap_or(72.)),
            ),
            ("tracking", decimal(layout.tracking)),
            (
                "leading",
                if layout.leading == 0. {
                    String::new()
                } else {
                    decimal(layout.leading)
                },
            ),
        ] {
            self.set_field(key, value, changed, window, cx);
        }
        self.sync_select("font", &layout.font_name, window, cx);
        let text = layer.content["text"].as_str().unwrap_or("");
        if (changed || !self.text.read(cx).focus_handle(cx).is_focused(window))
            && self.text.read(cx).value().as_str() != text
        {
            self.text
                .update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
        }
    }
    pub(super) fn commit_active_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.edited_fields.iter().copied().collect::<Vec<_>>();
        for key in keys {
            self.commit_field(key, window, cx);
        }
        if self.editing_slider.take().is_some() {
            self.send(Command::FinishGesture);
        }
        if self.numeric_edit.take().is_some() {
            self.send(Command::FinishGesture);
        }
    }
    pub(super) fn commit_field(
        &mut self,
        key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                "x" | "y" | "rotation" => self.transform_patch(json!({key:number(&value)?})),
                "width" | "height" => {
                    if let Some(layer) = self.state.as_ref().and_then(Snapshot::selected) {
                        let (property, size) = if key == "width" {
                            ("scaleX", layer.width)
                        } else {
                            ("scaleY", layer.height)
                        };
                        self.transform_patch(json!({property:number(&value)?/size as f64}));
                    }
                }
                "font-size" => self.text_update(picsie_core::text::TextPatch {
                    font_size: Some(number(&value)?),
                    ..Default::default()
                }),
                "tracking" => self.text_update(picsie_core::text::TextPatch {
                    tracking: Some(number(&value)?),
                    ..Default::default()
                }),
                "leading" => self.text_update(picsie_core::text::TextPatch {
                    leading: Some(if value.trim().is_empty() {
                        0.
                    } else {
                        number(&value)?
                    }),
                    ..Default::default()
                }),
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
                "wand-tolerance" => {
                    if let Some(state) = &self.state {
                        let mut settings = state.wand;
                        settings.tolerance = number(&value)?.round().clamp(0., 255.) as u8;
                        self.send(Command::SetWand { settings });
                    }
                }
                key if key.starts_with("image-") => self.image_field(key, &value, window, cx)?,
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
            "crop-ratio" => self.send(Command::SetCropRatio {
                ratio: match value {
                    "original" => picsie_core::crop::CropRatio::Original,
                    "square" => picsie_core::crop::CropRatio::Square,
                    "fourThree" => picsie_core::crop::CropRatio::FourThree,
                    "sixteenNine" => picsie_core::crop::CropRatio::SixteenNine,
                    _ => picsie_core::crop::CropRatio::Free,
                },
            }),
            "font" => self.text_update(picsie_core::text::TextPatch {
                font_name: Some(value.into()),
                ..Default::default()
            }),
            "mask-source" => self.send(Command::LinkMask {
                source_id: value.into(),
            }),
            "wand-sample" => {
                if let Some(state) = &self.state {
                    let mut settings = state.wand;
                    settings.radius = value.parse().unwrap_or(0);
                    self.send(Command::SetWand { settings });
                }
            }
            key if key.starts_with("image-") => self.image_choice(key, value, window, cx),
            _ => self.canvas_choice(key, value, window, cx),
        }
        cx.notify();
    }
}
