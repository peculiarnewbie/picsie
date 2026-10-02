//! ToolHeaderStyle, TransformInspector, BrushControls, LassoControls, TypeControls,
//! ShapeControls and CropControls, Compositor 609dbeae. MIT © 2026 Wonder Assembly LLC.
//! Only controls backed by the current engine are exposed; see docs/compositor-port.md.
use super::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::popover::Popover;
use picsie_core::{
    editor::LassoKind,
    model::MaskMode,
    pixel_selection::{MarqueeKind, PixelSelectionMode},
};
impl Desktop {
    pub(super) fn tool_hint(&self) -> String {
        let Some(state) = &self.state else {
            return "Ready when you are".into();
        };
        match state.tool {
            Tool::Move=>"Drag to move · Handles resize · Shift keeps proportions · Space to pan",
            Tool::Brush=>"Drag to paint · [ ] size · Shift [ ] hardness · Digits opacity · X mask mode · Space to pan",
            Tool::Eraser=>"Drag to erase · [ ] size · Shift [ ] hardness · Digits opacity · Space to pan",
            Tool::Rectangle|Tool::Ellipse=>"Drag to draw · Shift square/circle · Alt from center · Space to pan",
            Tool::Text=>"Click for point text · Drag a paragraph box · Space to pan",
            Tool::Hand=>"Drag to pan · Scroll to zoom",
            Tool::Eyedropper=>"Click the canvas to sample a color",
            Tool::Crop=>"Drag to crop · Enter apply · Escape cancel · Space to pan",
            Tool::Marquee=>"Drag to select · Shift add · Alt subtract · Ctrl/⌘-drag pixels · Ctrl/⌘D deselect",
            Tool::Lasso if state.lasso_kind=="polygonal"=>"Click corners · Enter or double-click to close · Backspace removes a corner · Escape cancel",
            Tool::Lasso=>"Drag to select · Shift add · Alt subtract · Ctrl/⌘D deselect",
            Tool::Wand=>"Click similar colors · Shift add · Alt subtract · Ctrl/⌘-drag pixels · Ctrl/⌘D deselect",
        }.into()
    }

    pub(super) fn inline_field(
        &self,
        key: &'static str,
        title: &str,
        width: f32,
        disabled: bool,
    ) -> Div {
        row()
            .gap(px(5.))
            .flex_shrink_0()
            .when(!title.is_empty(), |d| {
                d.child(div().text_color(rgb(MUTED)).child(title.to_owned()))
            })
            .child(
                self.numeric_field(
                    key,
                    Input::new(&self.fields[key])
                        .small()
                        .map(|input| Styled::h(input, px(26.)))
                        .bg(rgb(0x202020))
                        .text_size(px(12.))
                        .disabled(disabled || self.busy)
                        .aria_label(key.to_owned())
                        .w(px(width)),
                ),
            )
    }
    pub(super) fn inline_select(
        &self,
        key: &'static str,
        width: f32,
        disabled: bool,
    ) -> Stateful<Div> {
        self.probe(
            format!("select-{key}"),
            Select::new(&self.selects[key])
                .small()
                .h(px(26.))
                .bg(rgb(0x202020))
                .text_size(px(12.))
                .w(px(width))
                .disabled(disabled || self.busy),
        )
        .flex_shrink_0()
    }
    pub(super) fn palette_swatch(
        &self,
        background: bool,
        rail: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let masked = self
            .state
            .as_ref()
            .is_some_and(|s| s.paint_target == "mask");
        let color = self
            .state
            .as_ref()
            .map(|s| {
                if masked {
                    if (s.mask_mode == "reveal") != background {
                        "#ffffff"
                    } else {
                        "#000000"
                    }
                } else if background {
                    s.background_color.as_str()
                } else {
                    s.color.as_str()
                }
            })
            .unwrap_or(if background { "#ffffff" } else { "#000000" });
        let id = if background {
            "background-picker-rail"
        } else if rail {
            "foreground-picker-rail"
        } else {
            "foreground-picker"
        };
        let button = self
            .raw_button(format!("{id}-button"), "", false, cx)
            .w(px(if rail { 24. } else { 34. }))
            .h(px(if rail { 24. } else { 18. }))
            .rounded(px(if rail { 6. } else { 3. }))
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(hex_color(color))
                    .hover(hex_color(color)),
            )
            .border_1()
            .border_color(rgb(0xffffff))
            .bg(hex_color(color))
            .tooltip(if background {
                "Background color"
            } else {
                "Foreground color"
            });
        if masked {
            let entity = cx.entity();
            let opening = entity.clone();
            let palette = if background {
                "mask-background"
            } else {
                "mask-foreground"
            };
            self.probe(
                id,
                Popover::new(format!("{id}-mask-palette"))
                    .anchor(Anchor::TopLeft)
                    .open(self.open_palette == Some(palette))
                    .on_open_change(move |open, _, cx| {
                        opening.update(cx, |this, cx| {
                            this.open_palette = if *open { Some(palette) } else { None };
                            cx.notify();
                        })
                    })
                    .trigger(button)
                    .content(move |_, _, cx| {
                        entity.update(cx, |this, cx| {
                            row()
                                .child(this.command_button(
                                    "mask-color-black",
                                    "Black · Hide",
                                    Command::SetPaletteColor {
                                        color: "#000000".into(),
                                        background,
                                    },
                                    false,
                                    false,
                                    cx,
                                ))
                                .child(this.command_button(
                                    "mask-color-white",
                                    "White · Reveal",
                                    Command::SetPaletteColor {
                                        color: "#ffffff".into(),
                                        background,
                                    },
                                    false,
                                    false,
                                    cx,
                                ))
                        })
                    }),
            )
            .into_any_element()
        } else {
            self.probe(
                id,
                button.on_click(cx.listener(move |this, _, window, cx| {
                    this.act(
                        Action::Color(if background {
                            ColorTarget::Background
                        } else {
                            ColorTarget::Foreground
                        }),
                        window,
                        cx,
                    )
                })),
            )
            .into_any_element()
        }
    }
    pub(super) fn foreground_swatch(&self, cx: &Context<Self>, rail: bool) -> Stateful<Div> {
        if !rail {
            return div()
                .id("foreground-swatch-container")
                .child(self.palette_swatch(false, false, cx));
        }
        div()
            .id("palette-controls")
            .relative()
            .w(px(36.))
            .h(px(36.))
            .mt(px(5.))
            .child(
                div()
                    .absolute()
                    .left(px(12.))
                    .top(px(12.))
                    .child(self.palette_swatch(true, true, cx)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .occlude()
                    .child(self.palette_swatch(false, true, cx)),
            )
            .child(
                self.probe(
                    "palette-swap",
                    self.raw_button("palette-swap-button", "↔", false, cx)
                        .ghost()
                        .p_0()
                        .w(px(12.))
                        .h(px(12.))
                        .text_size(px(10.))
                        .tooltip("Swap foreground and background (X)")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.act(Action::Command(Command::SwapPaletteColors), window, cx)
                        })),
                )
                .absolute()
                .left(px(27.))
                .top(px(-3.)),
            )
            .child(
                self.probe(
                    "palette-defaults",
                    self.raw_button("palette-defaults-button", "↶", false, cx)
                        .ghost()
                        .p_0()
                        .w(px(12.))
                        .h(px(12.))
                        .text_size(px(9.))
                        .tooltip("Default colors (D)")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.act(Action::Command(Command::ResetPaletteColors), window, cx)
                        })),
                )
                .absolute()
                .left(px(-1.))
                .top(px(27.)),
            )
    }
    pub(super) fn layer_swatch(&self, cx: &Context<Self>) -> Div {
        let Some(layer) = self.state.as_ref().and_then(|s| s.selected()) else {
            return row().child(self.foreground_swatch(cx, false));
        };
        let Some(fill) = layer.fill() else {
            return row().child(self.foreground_swatch(cx, false));
        };
        row()
            .child(div().text_color(rgb(MUTED)).child("Color"))
            .child(
                self.probe(
                    "layer-color",
                    self.raw_button("layer-color-button", "", false, cx)
                        .bg(hex_color(fill))
                        .w(px(34.))
                        .h(px(18.))
                        .rounded(px(3.))
                        .custom(
                            ButtonCustomVariant::new(cx)
                                .color(hex_color(fill))
                                .hover(hex_color(fill)),
                        )
                        .disabled(layer.locked)
                        .tooltip("Layer color")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.act(Action::Color(ColorTarget::Layer), window, cx)
                        })),
                ),
            )
    }
    pub(super) fn text_palette(&self, cx: &Context<Self>) -> Stateful<Div> {
        self.probe(
            "edit-text",
            self.raw_button("edit-text-button", "Edit Text", false, cx)
                .disabled(
                    !self
                        .state
                        .as_ref()
                        .and_then(|s| s.selected())
                        .is_some_and(|l| l.kind() == "text"),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.act(
                        Action::Command(Command::EditText {
                            id: this.selected_id.clone(),
                            point: None,
                        }),
                        window,
                        cx,
                    );
                })),
        )
    }
    pub(super) fn toolbar(&self, cx: &Context<Self>) -> Div {
        let mut bar = row()
            .gap(px(12.))
            .h(px(42.))
            .flex_shrink_0()
            .px(px(18.))
            .border_b_1()
            .border_color(rgb(LINE));
        let Some(state) = &self.state else { return bar };
        let transform = state.tool == Tool::Move || state.transform_active;
        let title = if transform {
            if state.paint_target == "mask" {
                "Transform Mask"
            } else {
                "Transform"
            }
        } else {
            match state.tool {
                Tool::Brush => "Brush",
                Tool::Eraser => "Eraser",
                Tool::Rectangle | Tool::Ellipse => "Shape",
                Tool::Text => "Type",
                Tool::Hand => "Hand",
                Tool::Eyedropper => "Eyedropper",
                Tool::Crop => "Crop",
                Tool::Marquee => "Marquee",
                Tool::Lasso => "Lasso",
                Tool::Wand => "Magic Wand",
                _ => "Transform",
            }
        };
        bar = bar.child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::SEMIBOLD)
                .text_size(px(13.))
                .child(title),
        );
        let mut controls = div()
            .id("tool-header-scroll")
            .flex()
            .items_center()
            .gap(px(12.))
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_x_scroll();
        if transform {
            for (key, title) in [
                ("auto-select", "Auto Select"),
                ("show-controls", "Show Controls"),
            ] {
                bar = bar.child(
                    self.probe(
                        key,
                        Checkbox::new(key)
                            .label(title)
                            .checked(self.view_option(key))
                            .on_click(
                                cx.listener(move |this, _, _, _| this.toggle_view_option(key)),
                            ),
                    ),
                );
            }
            let layer = state.selected();
            // A folder or multi-selection edits the shared group box; its numeric
            // fields, ratio lock and flips stay enabled while sampling (meaningless
            // for a box) stays off. The engine publishes the authoritative
            // capability: an empty folder or a selection without transformable
            // members yields no box and keeps everything disabled.
            let group = state.group_box.is_some();
            let locked = (state.mask_distortion.is_some() || state.image_distortion.is_some())
                || layer.is_none_or(|l| {
                    l.locked
                        || (l.kind() == "group"
                            && !(state.paint_target == "mask"
                                && l.mask.as_ref().is_some_and(|m| m["linked"] == false)))
                })
                || state.selection.ids.len() != 1;
            let fields_locked = locked && !group;
            for (key, title, width) in [
                ("x", "X", 58.),
                ("y", "Y", 58.),
                ("width", "W", 62.),
                ("height", "H", 62.),
            ] {
                controls = controls.child(self.inline_field(key, title, width, fields_locked));
            }
            controls = controls
                .child(
                    self.probe(
                        "transform-ratio",
                        self.raw_button(
                            "transform-ratio-button",
                            "",
                            state.locks_transform_ratio,
                            cx,
                        )
                        .icon(icon("link", 15.))
                        .disabled(fields_locked || self.busy)
                        .tooltip("Lock aspect ratio")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(s) = &this.state {
                                this.send(Command::SetTransformRatio {
                                    locked: !s.locks_transform_ratio,
                                });
                            }
                            cx.notify();
                        })),
                    ),
                )
                .child(self.inline_field("transform-scale", "Scale", 58., fields_locked))
                .child(div().flex_shrink_0().text_color(rgb(MUTED)).child("%"))
                .child(self.inline_field("rotation", "°", 56., fields_locked))
                .child(self.inline_select("transform-sampling", 170., locked));
            if let Some(layer) = layer {
                let target = state.transform_target.as_ref().unwrap_or(layer);
                for (id, title, property, value) in [
                    ("flip-x", "Flip H", "flipX", target.flip_x),
                    ("flip-y", "Flip V", "flipY", target.flip_y),
                ] {
                    controls = controls.child(self.command_button(
                        id,
                        title,
                        Command::UpdateLayer {
                            patch: json!({property:!value}),
                        },
                        fields_locked,
                        false,
                        cx,
                    ));
                }
                if layer.kind() == "text" {
                    controls = controls.child(self.text_palette(cx));
                }
            }
            bar = bar
                .child(
                    self.probe("transform-fields", controls)
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .h_full(),
                )
                .child(self.command_button(
                    "transform-cancel",
                    "Cancel",
                    Command::CancelTransform,
                    !state.transform_active,
                    false,
                    cx,
                ))
                .child(self.command_button(
                    "transform-apply",
                    "Apply",
                    Command::CommitTransform,
                    !state.transform_active,
                    false,
                    cx,
                ));
            return bar;
        }
        match state.tool {
            Tool::Brush | Tool::Eraser => {
                controls = controls.child(
                    row()
                        .gap(px(1.))
                        .child(self.command_button(
                            "brush-paint",
                            "Paint",
                            Command::SetTool { tool: Tool::Brush },
                            false,
                            state.tool == Tool::Brush,
                            cx,
                        ))
                        .child(self.command_button(
                            "brush-erase",
                            "Erase",
                            Command::SetTool { tool: Tool::Eraser },
                            false,
                            state.tool == Tool::Eraser,
                            cx,
                        )),
                );
                controls = controls
                    .child(self.inline_field("brush-size", "Size", 48., false))
                    .child(label("px"));
                for (key, title) in [
                    ("hardness", "Hardness"),
                    ("brush-opacity", "Opacity"),
                    ("smoothing", "Smoothing"),
                ] {
                    if state.tool == Tool::Eraser && key == "smoothing" {
                        continue;
                    }
                    controls = controls.child(
                        row()
                            .gap(px(6.))
                            .flex_shrink_0()
                            .child(div().text_color(rgb(MUTED)).child(title))
                            .child(
                                self.probe(
                                    format!("slider-{key}"),
                                    div()
                                        .w(px(85.))
                                        .child(Slider::new(&self.sliders[key]).disabled(self.busy)),
                                ),
                            )
                            .child(self.inline_field(key, "", 42., false))
                            .when(key != "smoothing", |d| d.child(label("%"))),
                    );
                }
                if state.paint_target == "mask" {
                    for (id, title, mode) in [
                        ("mask-hide", "Black (Hide)", MaskMode::Hide),
                        ("mask-reveal", "White (Reveal)", MaskMode::Reveal),
                    ] {
                        controls = controls.child(self.command_button(
                            id,
                            title,
                            Command::SetMaskMode { mode },
                            false,
                            state.mask_mode
                                == if mode == MaskMode::Hide {
                                    "hide"
                                } else {
                                    "reveal"
                                },
                            cx,
                        ));
                    }
                } else {
                    controls = controls
                        .child(div().text_color(rgb(MUTED)).child("Color"))
                        .child(self.foreground_swatch(cx, false));
                }
            }
            Tool::Marquee | Tool::Lasso | Tool::Wand => {
                if state.tool == Tool::Marquee {
                    let mut group = row().gap(px(1.)).flex_shrink_0();
                    for (id, title, kind) in [
                        ("rectangle", "Rectangle", MarqueeKind::Rectangle),
                        ("ellipse", "Ellipse", MarqueeKind::Ellipse),
                    ] {
                        group = group.child(self.command_button(
                            &format!("marquee-{id}"),
                            title,
                            Command::SetMarqueeKind { kind },
                            false,
                            state.marquee_kind == id,
                            cx,
                        ));
                    }
                    controls = controls.child(group);
                }
                if state.tool == Tool::Lasso {
                    let mut group = row().gap(px(1.)).flex_shrink_0();
                    for (id, title, kind) in [
                        ("freehand", "Freehand", LassoKind::Freehand),
                        ("polygonal", "Polygonal", LassoKind::Polygonal),
                    ] {
                        group = group.child(self.command_button(
                            &format!("lasso-{id}"),
                            title,
                            Command::SetLassoKind { kind },
                            false,
                            state.lasso_kind == id,
                            cx,
                        ));
                    }
                    controls = controls.child(group);
                }
                let mut group = row().gap(px(1.)).flex_shrink_0();
                for (id, title, mode) in [
                    ("replace", "New", PixelSelectionMode::Replace),
                    ("add", "Add", PixelSelectionMode::Add),
                    ("subtract", "Subtract", PixelSelectionMode::Subtract),
                ] {
                    group = group.child(
                        self.command_button(
                            &format!("selection-{id}"),
                            title,
                            Command::SetSelectionMode { mode },
                            false,
                            state
                                .selection_feedback
                                .as_ref()
                                .map(|f| {
                                    format!("{:?}", f.effective_mode(self.cursor_modifiers))
                                        .to_lowercase()
                                })
                                .unwrap_or_else(|| state.selection_mode.clone())
                                == id,
                            cx,
                        ),
                    );
                }
                controls = controls.child(group);
                if state.tool == Tool::Wand {
                    controls = controls
                        .child(self.inline_field("wand-tolerance", "Tolerance", 44., false))
                        .child(self.inline_select("wand-sample", 125., false))
                        .child(
                            self.probe(
                                "wand-all-layers",
                                Checkbox::new("wand-all-check")
                                    .label("All Layers")
                                    .checked(state.wand.sample_all_layers)
                                    .on_click(cx.listener(|this, value, _, cx| {
                                        if let Some(state) = &this.state {
                                            let mut settings = state.wand;
                                            settings.sample_all_layers = *value;
                                            this.send(Command::SetWand { settings });
                                        }
                                        cx.notify();
                                    })),
                            ),
                        )
                        .child(
                            self.probe(
                                "wand-contiguous",
                                Checkbox::new("wand-contiguous-check")
                                    .label("Contiguous")
                                    .checked(state.wand.contiguous)
                                    .on_click(cx.listener(|this, value, _, cx| {
                                        if let Some(state) = &this.state {
                                            let mut settings = state.wand;
                                            settings.contiguous = *value;
                                            this.send(Command::SetWand { settings });
                                        }
                                        cx.notify();
                                    })),
                            ),
                        );
                }
                if state.tool != Tool::Marquee || state.marquee_kind == "ellipse" {
                    controls = controls.child(
                        self.probe(
                            "selection-antialiased",
                            Checkbox::new("selection-antialiased-check")
                                .label("Anti-alias")
                                .checked(state.selection_antialiased)
                                .on_click(cx.listener(|this, value, _, cx| {
                                    this.send(Command::SetSelectionAntialiased {
                                        antialiased: *value,
                                    });
                                    cx.notify();
                                })),
                        ),
                    );
                }
                let no_bounds = !state.can_modify_selection;
                controls = controls
                    .child(div().h(px(18.)).border_l_1().border_color(rgb(LINE)))
                    .child(self.command_button(
                        "pixels-expand",
                        "Expand",
                        Command::ExpandSelection { amount: 5 },
                        no_bounds,
                        false,
                        cx,
                    ))
                    .child(self.inline_field("selection-amount", "", 40., false))
                    .child(div().flex_shrink_0().text_color(rgb(MUTED)).child("px"))
                    .child(self.command_button(
                        "pixels-contract",
                        "Contract",
                        Command::ContractSelection { amount: 5 },
                        no_bounds,
                        false,
                        cx,
                    ))
                    .child(self.inline_field("selection-contract", "", 40., false))
                    .child(div().flex_shrink_0().text_color(rgb(MUTED)).child("px"))
                    .child(self.command_button(
                        "pixels-feather",
                        "Feather",
                        Command::FeatherSelection { amount: 2 },
                        no_bounds,
                        false,
                        cx,
                    ))
                    .child(self.inline_field("feather", "", 40., false))
                    .child(div().flex_shrink_0().text_color(rgb(MUTED)).child("px"));
                if state.selection_draft {
                    bar = bar.child(controls).child(self.command_button(
                        "selection-finish",
                        "Finish",
                        Command::FinishSelection,
                        false,
                        false,
                        cx,
                    ));
                } else {
                    bar = bar
                        .child(controls)
                        .when(state.selection_empty, |bar| {
                            bar.child(div().text_color(rgb(MUTED)).child("Empty selection"))
                        })
                        .child(self.command_button(
                            "pixels-deselect",
                            "Deselect",
                            Command::DeselectPixels,
                            !state.has_pixel_selection,
                            false,
                            cx,
                        ));
                }
                return bar;
            }
            Tool::Crop => {
                controls = controls.child(self.inline_select("crop-ratio", 170., false));
                if let Some(rect) = state
                    .cursor_map
                    .crop
                    .filter(|_| state.cursor_map.crop_active)
                {
                    controls = controls.child(
                        div()
                            .flex_shrink_0()
                            .child(format!("{} × {} px", rect.width as u32, rect.height as u32)),
                    );
                }
                return bar
                    .child(controls)
                    .child(self.command_button(
                        "crop-cancel",
                        "Cancel",
                        Command::CancelCrop,
                        !state.cursor_map.crop_active,
                        false,
                        cx,
                    ))
                    .child(self.command_button(
                        "crop-apply",
                        "Apply Crop",
                        Command::CommitCrop,
                        !state.cursor_map.crop_active,
                        false,
                        cx,
                    ));
            }
            Tool::Text => {
                let locked = state
                    .selected()
                    .is_some_and(|l| l.kind() == "text" && l.locked);
                let layout = state.current_text.text_layout.clone().unwrap_or_default();
                controls = controls
                    .child(self.inline_select("font", 210., locked))
                    .child(
                        row()
                            .gap(px(2.))
                            .child(self.inline_field("font-size", "", 52., locked))
                            .child(div().text_color(rgb(MUTED)).child("px")),
                    )
                    .child(
                        self.probe(
                            "text-color",
                            self.raw_button("text-color-button", "", false, cx)
                                .w(px(36.))
                                .h(px(18.))
                                .rounded(px(3.))
                                .border_1()
                                .border_color(rgb(0x101010))
                                .tooltip("Text color")
                                .bg(hex_color(
                                    state.current_text.content["color"]
                                        .as_str()
                                        .unwrap_or("#000000"),
                                ))
                                .disabled(locked)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.act(Action::Color(ColorTarget::Text), window, cx)
                                })),
                        ),
                    );
                let mut alignment_controls = row().gap(px(2.)).flex_shrink_0();
                for (alignment, title, id) in [
                    (picsie_core::text::TextAlignment::Left, "Left", "text-left"),
                    (
                        picsie_core::text::TextAlignment::Center,
                        "Center",
                        "text-center",
                    ),
                    (
                        picsie_core::text::TextAlignment::Right,
                        "Right",
                        "text-right",
                    ),
                ] {
                    alignment_controls = alignment_controls.child(
                        self.probe(
                            id,
                            self.raw_button(id, "", layout.alignment == alignment, cx)
                                .icon(icon(&format!("align-{}", title.to_lowercase()), 18.))
                                .w(px(30.))
                                .h(px(26.))
                                .rounded(px(4.))
                                .tooltip(format!("Align {}", title.to_lowercase()))
                                .disabled(locked || self.busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.act(
                                        Action::Command(Command::UpdateText {
                                            patch: picsie_core::text::TextPatch {
                                                alignment: Some(alignment),
                                                ..Default::default()
                                            },
                                        }),
                                        window,
                                        cx,
                                    )
                                })),
                        )
                        .flex_shrink_0(),
                    );
                }
                controls = controls
                    .child(alignment_controls)
                    .child(self.inline_field("tracking", "Tracking", 45., locked))
                    .child(self.inline_field("leading", "Leading", 52., locked));
                bar = bar.child(controls);
                if state.text_editing {
                    bar = bar
                        .child(self.command_button(
                            "text-cancel",
                            "Cancel",
                            Command::CancelText,
                            false,
                            false,
                            cx,
                        ))
                        .child(self.command_button(
                            "text-done",
                            "Done",
                            Command::CommitText,
                            false,
                            false,
                            cx,
                        ));
                } else {
                    bar = bar.child(self.text_palette(cx));
                }
                return bar;
            }
            Tool::Rectangle | Tool::Ellipse => {
                controls = controls
                    .child(self.command_button(
                        "shape-rectangle",
                        "Rectangle",
                        Command::SetTool {
                            tool: Tool::Rectangle,
                        },
                        false,
                        state.tool == Tool::Rectangle,
                        cx,
                    ))
                    .child(self.command_button(
                        "shape-ellipse",
                        "Ellipse",
                        Command::SetTool {
                            tool: Tool::Ellipse,
                        },
                        false,
                        state.tool == Tool::Ellipse,
                        cx,
                    ))
                    .child(self.foreground_swatch(cx, false));
            }
            Tool::Hand => {
                controls = controls.child(hint(
                    "Drag to pan · Scroll to zoom · Space temporarily selects Hand",
                ));
            }
            Tool::Eyedropper => {
                controls = controls
                    .child(self.foreground_swatch(cx, false))
                    .child(
                        self.probe(
                            "sample-ring-toggle",
                            gpui_kit::component::checkbox::Checkbox::new("sample-ring-check")
                                .label("Sample Ring")
                                .checked(self.shows_sample_ring)
                                .on_click(cx.listener(|this, value, _, cx| {
                                    this.shows_sample_ring = *value;
                                    crate::preferences::update(|p| {
                                        p.shows_sample_ring = Some(*value)
                                    });
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(hint("Click or drag to sample"));
            }
            _ => {}
        }
        bar.child(controls)
    }
}
