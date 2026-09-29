use super::input::TOOLS;
use super::*;
use picsie_core::{
    crop::CropRatio,
    editor::{PaintTarget, SelectionMode},
    model::MaskMode,
    pixel_selection::{MarqueeKind, PixelSelectionMode},
};
impl Desktop {
    fn header(&self, cx: &Context<Self>) -> Div {
        row()
            .h(px(52.))
            .flex_shrink_0()
            .px(px(18.))
            .border_b_1()
            .border_color(rgb(LINE))
            .child(icon("image", 20.).text_color(rgb(ACCENT)))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("picsie"),
            )
            .child(label("IMAGE EDITOR").ml(px(12.)))
            .child(div().flex_1())
            .child(self.button("new", "New", Action::New, cx))
            .child(self.button("open", "Open", Action::File(FileAction::Open), cx))
            .child(self.button(
                "open-comp",
                "Open .comp",
                Action::File(FileAction::OpenComp),
                cx,
            ))
            .child(self.button(
                "import",
                "Import image",
                Action::File(FileAction::Import),
                cx,
            ))
            .child(self.button("save", "Save", Action::File(FileAction::Save), cx))
            .child(self.button(
                "save-comp",
                "Save .comp",
                Action::File(FileAction::SaveComp),
                cx,
            ))
            .child(
                self.probe(
                    "export-png",
                    self.raw_button("export-png-button", "Export PNG", true, cx)
                        .bg(rgb(ACCENT))
                        .text_color(rgb(0x171b2c))
                        .icon(icon("export", 18.))
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.act(Action::File(FileAction::ExportPng), window, cx)
                        })),
                ),
            )
    }
    fn toolbar(&self, cx: &Context<Self>) -> Div {
        let mut bar = row()
            .h(px(46.))
            .flex_shrink_0()
            .px(px(18.))
            .border_b_1()
            .border_color(rgb(LINE));
        let Some(state) = &self.state else { return bar };
        let title = TOOLS
            .iter()
            .find(|(tool, _, _, _)| *tool == state.tool)
            .map(|t| t.2)
            .unwrap_or("Move");
        bar = bar.child(
            div()
                .w(px(98.))
                .flex_shrink_0()
                .text_color(rgb(ACCENT))
                .child(title),
        );
        match state.tool {
            Tool::Brush | Tool::Eraser => {
                bar = bar.child(
                    row()
                        .w(px(460.))
                        .items_center()
                        .child(self.field("brush-size", "Size (px)", false))
                        .child(self.field("brush-opacity", "Opacity (%)", false))
                        .child(self.field("hardness", "Hardness (%)", false))
                        .child(self.field("smoothing", "Smoothing", false)),
                );
            }
            Tool::Crop => {
                for (id, label, ratio) in [
                    ("free", "Free", CropRatio::Free),
                    ("original", "Original", CropRatio::Original),
                    ("square", "1:1", CropRatio::Square),
                    ("fourThree", "4:3", CropRatio::FourThree),
                    ("sixteenNine", "16:9", CropRatio::SixteenNine),
                ] {
                    bar = bar.child(self.command_button(
                        &format!("crop-{id}"),
                        label,
                        Command::SetCropRatio { ratio },
                        false,
                        state.crop_ratio == id,
                        cx,
                    ));
                }
                bar = bar
                    .child(self.command_button(
                        "crop-apply",
                        "Apply",
                        Command::CommitCrop,
                        false,
                        true,
                        cx,
                    ))
                    .child(self.command_button(
                        "crop-cancel",
                        "Cancel",
                        Command::CancelCrop,
                        false,
                        false,
                        cx,
                    ));
            }
            Tool::Marquee | Tool::Lasso => {
                if state.tool == Tool::Marquee {
                    bar = bar
                        .child(self.command_button(
                            "marquee-rectangle",
                            "Rectangle",
                            Command::SetMarqueeKind {
                                kind: MarqueeKind::Rectangle,
                            },
                            false,
                            state.marquee_kind == "rectangle",
                            cx,
                        ))
                        .child(self.command_button(
                            "marquee-ellipse",
                            "Ellipse",
                            Command::SetMarqueeKind {
                                kind: MarqueeKind::Ellipse,
                            },
                            false,
                            state.marquee_kind == "ellipse",
                            cx,
                        ));
                }
                for (id, label, mode) in [
                    ("replace", "New", PixelSelectionMode::Replace),
                    ("add", "Add", PixelSelectionMode::Add),
                    ("subtract", "Subtract", PixelSelectionMode::Subtract),
                ] {
                    bar = bar.child(self.command_button(
                        &format!("selection-{id}"),
                        label,
                        Command::SetSelectionMode { mode },
                        false,
                        state.selection_mode == id,
                        cx,
                    ));
                }
            }
            _ => bar = bar.child(hint("V move · B brush · M marquee · L lasso · C crop")),
        }
        if state.paint_target == "mask" {
            bar = bar
                .child(label("Mask").text_color(rgb(0x62deca)))
                .child(self.command_button(
                    "mask-hide",
                    "Hide",
                    Command::SetMaskMode {
                        mode: MaskMode::Hide,
                    },
                    false,
                    state.mask_mode == "hide",
                    cx,
                ))
                .child(self.command_button(
                    "mask-reveal",
                    "Reveal",
                    Command::SetMaskMode {
                        mode: MaskMode::Reveal,
                    },
                    false,
                    state.mask_mode == "reveal",
                    cx,
                ));
        }
        bar.child(div().flex_1())
            .child(self.icon_button(
                "undo",
                "undo",
                format!("Undo {}", state.history.undo_label),
                Action::Command(Command::Undo),
                !state.history.can_undo,
                false,
                cx,
            ))
            .child(self.icon_button(
                "redo",
                "redo",
                format!("Redo {}", state.history.redo_label),
                Action::Command(Command::Redo),
                !state.history.can_redo,
                false,
                cx,
            ))
            .when(state.viewport.width >= 700., |d| {
                d.child(
                    label(format!(
                        "{}{}",
                        state.document.name,
                        if state.history.dirty { " •" } else { "" }
                    ))
                    .ml(px(10.)),
                )
            })
    }
    fn tool_rail(&self, cx: &Context<Self>) -> Stateful<Div> {
        let tool = self.state.as_ref().map(|s| s.tool).unwrap_or(Tool::Move);
        let mut rail = div()
            .id("tool-rail")
            .w(px(64.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(7.))
            .p(px(10.))
            .border_r_1()
            .border_color(rgb(LINE))
            .overflow_y_scroll();
        for (value, id, name, key) in TOOLS {
            let command = Command::SetTool { tool: *value };
            rail = rail.child(
                self.probe(
                    format!("tool-{id}"),
                    self.raw_button(format!("tool-button-{id}"), "", tool == *value, cx)
                        .icon(icon(id, 18.))
                        .w(px(42.))
                        .h(px(37.))
                        .tooltip(format!("{name} ({key})"))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.act(Action::Command(command.clone()), window, cx)
                        })),
                )
                .flex_shrink_0(),
            );
        }
        rail.child(div().h(px(10.)).flex_shrink_0())
            .child(
                div()
                    .size(px(32.))
                    .flex_shrink_0()
                    .rounded(px(6.))
                    .border_2()
                    .border_color(rgb(0xedf0fc))
                    .bg(hex_color(
                        self.state
                            .as_ref()
                            .map(|s| s.color.as_str())
                            .unwrap_or("#a5b4fc"),
                    )),
            )
            .child(div().flex_1())
            .child(label("RGB"))
    }
    fn color_section(&self, state: &Snapshot, cx: &Context<Self>) -> Div {
        let mut swatches = row().gap(px(7.));
        for color in SWATCHES {
            let color = color.to_string();
            let command = Command::SetColor {
                color: color.clone(),
            };
            swatches = swatches.child(
                self.probe(
                    format!("swatch-{color}"),
                    self.raw_button(format!("color-{color}"), "", false, cx)
                        .w(px(29.))
                        .h(px(20.))
                        .custom(
                            ButtonCustomVariant::new(cx)
                                .color(hex_color(&color))
                                .hover(hex_color(&color))
                                .active(hex_color(&color)),
                        )
                        .when(state.color.eq_ignore_ascii_case(&color), |b| {
                            b.border_2().border_color(rgb(0xffffff))
                        })
                        .bg(hex_color(&color))
                        .tooltip(color)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.act(Action::Command(command.clone()), window, cx)
                        })),
                ),
            );
        }
        section("Color")
            .child(
                row()
                    .items_end()
                    .child(
                        self.probe(
                            "foreground-picker",
                            self.raw_button("foreground-picker-button", "", false, cx)
                                .w(px(33.))
                                .h(px(33.))
                                .custom(
                                    ButtonCustomVariant::new(cx)
                                        .color(hex_color(&state.color))
                                        .hover(hex_color(&state.color)),
                                )
                                .border_2()
                                .border_color(rgb(0xedf0fc))
                                .bg(hex_color(&state.color))
                                .tooltip("Choose foreground color")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.act(Action::Color(ColorTarget::Foreground), window, cx)
                                })),
                        ),
                    )
                    .child(self.field("foreground", "Foreground", false)),
            )
            .child(swatches)
            .child(self.command_button(
                "fill-foreground",
                "Fill with foreground",
                Command::FillSelection,
                !state.can_edit_pixels,
                false,
                cx,
            ))
    }
    fn selection_section(&self, state: &Snapshot, cx: &Context<Self>) -> Div {
        let no_bounds = state.pixel_selection_bounds.is_none();
        let amount = number(&self.field_value("selection-amount", cx))
            .unwrap_or(5.)
            .clamp(1., 500.) as u32;
        let feather = number(&self.field_value("feather", cx))
            .unwrap_or(2.)
            .clamp(1., 250.) as u32;
        section("Pixel selection")
            .child(hint(if !state.has_pixel_selection {
                "No selection · edits affect the canvas."
            } else if no_bounds {
                "The selection is empty."
            } else {
                "Edits affect the selected area."
            }))
            .child(
                row()
                    .child(self.command_button(
                        "pixels-all",
                        "All",
                        Command::SelectAllPixels,
                        false,
                        false,
                        cx,
                    ))
                    .child(self.command_button(
                        "pixels-invert",
                        "Invert",
                        Command::InvertSelection,
                        !state.has_pixel_selection,
                        false,
                        cx,
                    ))
                    .child(self.command_button(
                        "pixels-deselect",
                        "Deselect",
                        Command::DeselectPixels,
                        !state.has_pixel_selection,
                        false,
                        cx,
                    )),
            )
            .child(self.field("selection-amount", "Expand / contract (px)", false))
            .child(
                row()
                    .child(self.command_button(
                        "pixels-expand",
                        "Expand",
                        Command::ExpandSelection { amount },
                        no_bounds,
                        false,
                        cx,
                    ))
                    .child(self.command_button(
                        "pixels-contract",
                        "Contract",
                        Command::ContractSelection { amount },
                        no_bounds,
                        false,
                        cx,
                    )),
            )
            .child(
                row()
                    .items_end()
                    .child(self.field("feather", "Feather px", false))
                    .child(self.command_button(
                        "pixels-feather",
                        "Feather",
                        Command::FeatherSelection { amount: feather },
                        no_bounds,
                        false,
                        cx,
                    )),
            )
            .child(
                row()
                    .child(self.command_button(
                        "pixels-fill",
                        "Fill",
                        Command::FillSelection,
                        !state.can_edit_pixels,
                        false,
                        cx,
                    ))
                    .child(self.command_button(
                        "pixels-clear",
                        "Clear pixels",
                        Command::ClearSelectedPixels,
                        !state.has_pixel_selection || !state.can_edit_pixels,
                        false,
                        cx,
                    )),
            )
    }
    fn layers_section(&self, state: &Snapshot, cx: &Context<Self>) -> Div {
        let mut list = div()
            .id("layers-scroll")
            .flex()
            .flex_col()
            .gap(px(3.))
            .max_h(px(255.))
            .overflow_y_scroll()
            .min_h_0();
        for item in &state.layer_rows {
            let Some(layer) = state.layer(&item.id) else {
                continue;
            };
            let id = layer.id.clone();
            let selected = state.is_selected(&id);
            let drag_target = self
                .layer_drag
                .as_ref()
                .and_then(|d| d.destination.as_ref())
                .filter(|(target, _)| target == &id)
                .map(|(_, side)| *side);
            let mut entry = row()
                .gap(px(4.))
                .h(px(40.))
                .w_full()
                .flex_shrink_0()
                .pl(px(5. + item.depth as f32 * 14.))
                .pr(px(6.))
                .rounded(px(5.))
                .bg(rgb(if selected { 0x363e58 } else { 0x282c35 }))
                .relative();
            if let Some(side) = drag_target {
                entry = if side == "into" {
                    entry.border_1().border_color(rgb(ACCENT))
                } else {
                    entry.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .h(px(2.))
                            .bg(rgb(ACCENT))
                            .when(side == "above", |d| d.top_0())
                            .when(side == "below", |d| d.bottom_0()),
                    )
                };
            }
            if layer.kind() == "group" {
                entry = entry.child(
                    self.probe(
                        format!("collapse-{id}"),
                        self.raw_button(
                            format!("collapse-button-{id}"),
                            if item.collapsed { "▸" } else { "▾" },
                            false,
                            cx,
                        )
                        .ghost()
                        .w(px(16.))
                        .on_click(cx.listener({
                            let id = id.clone();
                            move |this, _, window, cx| {
                                if this.busy {
                                    return;
                                }
                                this.commit_active_fields(window, cx);
                                this.send(Command::ToggleGroupExpansion { id: id.clone() })
                            }
                        })),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                );
            }
            entry = entry.child(icon("grip", 14.).text_color(rgb(MUTED))).child(
                self.probe(
                    format!("visibility-{id}"),
                    self.raw_button(format!("visibility-button-{id}"), "", false, cx)
                        .ghost()
                        .w(px(20.))
                        .icon(
                            icon(if layer.visible { "eye" } else { "eyeOff" }, 16.)
                                .text_color(rgb(if item.visible { ACCENT } else { MUTED })),
                        )
                        .on_click(cx.listener({
                            let id = id.clone();
                            let visible = !layer.visible;
                            move |this, _, window, cx| {
                                if this.busy {
                                    return;
                                }
                                this.commit_active_fields(window, cx);
                                this.send(Command::Select {
                                    id: Some(id.clone()),
                                    mode: SelectionMode::Replace,
                                });
                                this.patch(json!({"visible":visible}));
                            }
                        })),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            );
            if layer.mask_source_id.is_some() {
                entry = entry.child(div().text_color(rgb(ACCENT)).child("↳"));
            }
            entry = entry
                .child(
                    icon(
                        match layer.kind() {
                            "text" => "text",
                            "image" => "image",
                            "paint" => "brush",
                            _ => "layers",
                        },
                        16.,
                    )
                    .text_color(rgb(ACCENT)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.))
                        .text_ellipsis()
                        .child(layer.name.clone()),
                );
            if layer.locked {
                entry = entry.child(icon("lock", 14.).text_color(rgb(MUTED)));
            }
            if let Some(mask) = &layer.mask {
                entry = entry.child(
                    self.probe(
                        format!("mask-row-{id}"),
                        self.raw_button(
                            format!("mask-row-button-{id}"),
                            "",
                            selected && state.paint_target == "mask",
                            cx,
                        )
                        .w(px(24.))
                        .h(px(26.))
                        .icon(icon("mask", 16.))
                        .opacity(if mask["enabled"].as_bool() == Some(true) {
                            1.
                        } else {
                            0.4
                        })
                        .on_click(cx.listener({
                            let id = id.clone();
                            move |this, _, window, cx| {
                                if this.busy {
                                    return;
                                }
                                this.commit_active_fields(window, cx);
                                this.send(Command::Select {
                                    id: Some(id.clone()),
                                    mode: SelectionMode::Replace,
                                });
                                this.send(Command::SetPaintTarget {
                                    target: PaintTarget::Mask,
                                });
                            }
                        })),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                );
            }
            list = list.child(
                self.probe(format!("layer-{id}"), entry)
                    .w_full()
                    .flex_shrink_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            this.layer_down(id.clone(), event, window, cx)
                        }),
                    ),
            );
        }
        if state.document.layers.is_empty() {
            list = list.child(hint("Import an image or add a layer to begin.").p(px(12.)));
        }
        let mut panel=section("Layers").child(row().gap(px(5.)).child(self.command_button("add-paint","Paint",Command::AddPaintLayer,false,false,cx)).child(self.command_button("add-gradient","Gradient",Command::AddGradient,false,false,cx)).child(self.command_button("add-folder","Folder",Command::AddGroup,false,false,cx)).child(label(state.document.layers.len().to_string())))
            .child(hint("Shift: range · Ctrl/⌘: toggle · Drag rows: reorder · Drop on a folder: file inside").text_size(px(10.)))
            .child(self.probe("layer-list",list))
            .child(row().gap(px(5.)).child(self.command_button("group","Group",Command::GroupSelected,state.selection.ids.is_empty(),false,cx))
                .child(self.icon_button("raise","up","Raise selected layers".into(),Action::Command(Command::Reorder{direction:1}),!state.unlocked_selection(),false,cx))
                .child(self.icon_button("lower","down","Lower selected layers".into(),Action::Command(Command::Reorder{direction:-1}),!state.unlocked_selection(),false,cx))
                .child(self.command_button("duplicate","Duplicate",Command::Duplicate,state.selection.ids.is_empty(),false,cx))
                .child(self.icon_button("delete","trash","Delete selected unlocked layers".into(),Action::Command(Command::Remove),!state.unlocked_selection(),false,cx)));
        if !state.selection.ids.is_empty() {
            let mut filing = row().flex_wrap().gap(px(4.)).child(
                self.command_button(
                    "out-of-folder",
                    "Out of folder",
                    Command::MoveToGroup { parent_id: None },
                    !state
                        .document
                        .layers
                        .iter()
                        .any(|l| state.is_selected(&l.id) && l.parent_id.is_some()),
                    false,
                    cx,
                ),
            );
            for folder in state
                .document
                .layers
                .iter()
                .filter(|l| l.kind() == "group" && !state.is_selected(&l.id))
            {
                filing = filing.child(self.command_button(
                    &format!("into-{}", folder.id),
                    &format!("Into {}", folder.name),
                    Command::MoveToGroup {
                        parent_id: Some(folder.id.clone()),
                    },
                    false,
                    false,
                    cx,
                ));
            }
            panel = panel.child(filing);
        }
        if state.selection.ids.len() > 1 {
            panel=panel.child(hint(format!("{} layers selected",state.selection.ids.len())).text_color(rgb(ACCENT))).child(hint("Drag with Move or use arrow keys to move together. Select one layer to resize, rotate, or paint. Locked layers stay in place."));
        }
        panel
    }
    fn mask_sections(&self, state: &Snapshot, layer: &LayerInfo, cx: &Context<Self>) -> Div {
        let mut mask = section(if layer.kind() == "group" {
            "Folder mask"
        } else {
            "Layer mask"
        });
        if let Some(value) = &layer.mask {
            let enabled = value["enabled"].as_bool().unwrap_or(true);
            let linked = value["linked"].as_bool().unwrap_or(true);
            mask=mask.child(row().child(self.command_button("paint-content",if layer.kind()=="group"{"Folder"}else{"Layer pixels"},Command::SetPaintTarget{target:PaintTarget::Content},false,state.paint_target=="content",cx)).child(self.command_button("paint-mask","Mask",Command::SetPaintTarget{target:PaintTarget::Mask},layer.locked,state.paint_target=="mask",cx)))
                .child(hint(if enabled{"Paint Hide to conceal; Reveal to restore. X swaps modes. Eraser reverses the mode."}else{"Mask disabled. Enable it to paint."}))
                .child(row().child(self.command_button("mask-reset-reveal","Reveal all",Command::ResetMask{base:MaskMode::Reveal},layer.locked,false,cx)).child(self.command_button("mask-reset-hide","Hide all",Command::ResetMask{base:MaskMode::Hide},layer.locked,false,cx)))
                .when(layer.kind()!="group",|d|d.child(self.command_button("mask-link",if linked{"Linked to layer"}else{"Independent mask"},Command::ToggleMaskLink,layer.locked,linked,cx)))
                .child(row().child(self.command_button("mask-enabled",if enabled{"Disable"}else{"Enable"},Command::UpdateLayer{patch:json!({"mask":{"enabled":!enabled}})},layer.locked,false,cx)).child(self.command_button("mask-remove","Remove mask",Command::RemoveMask,layer.locked,false,cx)));
        } else {
            mask = mask
                .child(self.command_button(
                    "mask-add",
                    "Add mask",
                    Command::AddMask {
                        base: MaskMode::Reveal,
                    },
                    layer.locked,
                    false,
                    cx,
                ))
                .child(hint(
                    "Hide or restore areas while keeping the original pixels.",
                ));
        }
        column()
            .gap_0()
            .child(mask)
            .when(layer.kind() != "group", |d| {
                d.child(
                    section("Clipping")
                        .child(self.command_button(
                            "clipping",
                            if layer.mask_source_id.is_some() {
                                "Release clipping mask"
                            } else {
                                "Clip to layer below"
                            },
                            Command::ToggleClippingMask,
                            !state.can_toggle_clipping,
                            false,
                            cx,
                        ))
                        .child(self.select(
                            "mask-source",
                            "Live mask source",
                            layer.locked || state.mask_source_ids.is_empty(),
                        )),
                )
            })
    }
    fn inspector(&self, cx: &Context<Self>) -> Stateful<Div> {
        let mut panel = div()
            .id("inspector-scroll")
            .track_scroll(&self.inspector_scroll)
            .flex()
            .flex_col()
            .w(px(280.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(PANEL))
            .border_l_1()
            .border_color(rgb(LINE))
            .overflow_y_scroll();
        let Some(state) = &self.state else {
            return panel;
        };
        panel = panel.child(self.color_section(state, cx));
        if state.has_pixel_selection || matches!(state.tool, Tool::Marquee | Tool::Lasso) {
            panel = panel.child(self.selection_section(state, cx));
        }
        panel = panel.child(self.layers_section(state, cx));
        if state.selection.ids.len() == 1
            && let Some(layer) = state.selected()
        {
            let locked = layer.locked;
            panel = panel
                .child(
                    section("Properties")
                        .child(self.field("name", "Name", locked))
                        .child(self.slider("opacity", "Opacity", layer.opacity * 100., "%", locked))
                        .child(
                            row()
                                .items_end()
                                .child(self.select(
                                    "blend",
                                    "Blend mode",
                                    locked || layer.kind() == "group",
                                ))
                                .child(self.command_button(
                                    "lock",
                                    if locked { "Unlock layer" } else { "Lock layer" },
                                    Command::UpdateLayer {
                                        patch: json!({"locked":!locked}),
                                    },
                                    false,
                                    locked,
                                    cx,
                                )),
                        ),
                )
                .child(self.mask_sections(state, layer, cx));
            if layer.kind() != "group" {
                panel = panel.child(
                    section("Transform")
                        .child(
                            row()
                                .child(self.field("x", "X", locked))
                                .child(self.field("y", "Y", locked)),
                        )
                        .child(
                            row()
                                .child(self.field("width", "Width", locked))
                                .child(self.field("height", "Height", locked)),
                        )
                        .child(
                            row()
                                .items_end()
                                .child(self.field("rotation", "Rotation °", locked))
                                .child(self.command_button(
                                    "flip-x",
                                    "Flip X",
                                    Command::UpdateLayer {
                                        patch: json!({"flipX":!layer.flip_x}),
                                    },
                                    locked,
                                    false,
                                    cx,
                                ))
                                .child(self.command_button(
                                    "flip-y",
                                    "Flip Y",
                                    Command::UpdateLayer {
                                        patch: json!({"flipY":!layer.flip_y}),
                                    },
                                    locked,
                                    false,
                                    cx,
                                )),
                        ),
                );
                if layer.kind() == "text" {
                    panel = panel.child(
                        section("Text")
                            .child(
                                self.probe(
                                    "text-editor",
                                    Textarea::new(&self.text)
                                        .small()
                                        .bg(rgb(0x191c22))
                                        .text_size(px(12.))
                                        .aria_label("Layer text")
                                        .h(px(80.))
                                        .disabled(locked || self.busy),
                                ),
                            )
                            .child(self.field("font-size", "Font size", locked))
                            .child(self.select("font", "Font family", locked)),
                    );
                }
                if let Some(fill) = layer.fill() {
                    let property = if layer.kind() == "gradient" {
                        "from"
                    } else {
                        "color"
                    };
                    let mut section = section("Fill").child(
                        row()
                            .child(
                                self.probe(
                                    "layer-color",
                                    self.raw_button("layer-color-button", "", false, cx)
                                        .bg(hex_color(fill))
                                        .w(px(33.))
                                        .h(px(33.))
                                        .custom(
                                            ButtonCustomVariant::new(cx)
                                                .color(hex_color(fill))
                                                .hover(hex_color(fill)),
                                        )
                                        .disabled(locked)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.act(Action::Color(ColorTarget::Layer), window, cx)
                                        })),
                                ),
                            )
                            .child(
                                self.probe(
                                    "use-foreground",
                                    self.raw_button(
                                        "use-foreground-button",
                                        "Use foreground color",
                                        false,
                                        cx,
                                    )
                                    .disabled(locked)
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            if this.busy {
                                                return;
                                            }
                                            this.commit_active_fields(window, cx);
                                            this.engine.request(Operation::ForegroundFill {
                                                key: property,
                                            });
                                        },
                                    )),
                                ),
                            ),
                    );
                    if layer.kind() == "gradient" {
                        section = section.child(
                            self.probe(
                                "gradient-end",
                                self.raw_button(
                                    "gradient-end-button",
                                    "Use foreground for end color",
                                    false,
                                    cx,
                                )
                                .disabled(locked)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        if this.busy {
                                            return;
                                        }
                                        this.commit_active_fields(window, cx);
                                        this.engine
                                            .request(Operation::ForegroundFill { key: "to" });
                                    },
                                )),
                            ),
                        );
                    }
                    panel = panel.child(section);
                }
                panel = panel.child(
                    section("Adjustments")
                        .child(self.slider(
                            "brightness",
                            "Brightness",
                            layer.brightness * 100.,
                            "%",
                            locked,
                        ))
                        .child(self.slider(
                            "saturation",
                            "Saturation",
                            layer.saturation * 100.,
                            "%",
                            locked,
                        ))
                        .child(self.slider("blur", "Gaussian blur", layer.blur, "px", locked)),
                );
            }
        }
        panel
    }
    fn footer(&self, cx: &Context<Self>) -> Div {
        let state = self.state.as_ref();
        row()
            .h(px(28.))
            .flex_shrink_0()
            .px(px(14.))
            .border_t_1()
            .border_color(rgb(LINE))
            .child(
                self.probe(
                    "canvas-size",
                    self.raw_button(
                        "canvas-size-button",
                        state
                            .map(|s| format!("{} × {} px", s.document.width, s.document.height))
                            .unwrap_or_default(),
                        false,
                        cx,
                    )
                    .h(px(22.))
                    .tooltip("Canvas Size (Ctrl/⌘+Alt+C)")
                    .disabled(self.busy)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.act(Action::CanvasSize, window, cx)),
                    ),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .ml(px(12.))
                    .text_size(px(10.))
                    .text_color(rgb(if self.busy { ACCENT } else { MUTED }))
                    .text_ellipsis()
                    .child(if self.busy {
                        "Working…".into()
                    } else {
                        self.notice.clone()
                    }),
            )
            .child(
                self.probe(
                    "zoom-out",
                    self.raw_button("zoom-out-button", "−", false, cx)
                        .h(px(22.))
                        .w(px(26.))
                        .on_click(cx.listener(|this, _, _, _| {
                            this.engine.request(Operation::ZoomBy {
                                factor: 0.8,
                                point: None,
                            });
                        })),
                ),
            )
            .child(div().w(px(35.)).text_size(px(10.)).text_center().child(
                format!("{}%",state.map(|s|(s.viewport.zoom*100.).round() as u32).unwrap_or(100)),
            ))
            .child(
                self.probe(
                    "zoom-in",
                    self.raw_button("zoom-in-button", "+", false, cx)
                        .h(px(22.))
                        .w(px(26.))
                        .on_click(cx.listener(|this, _, _, _| {
                            this.engine.request(Operation::ZoomBy {
                                factor: 1.25,
                                point: None,
                            });
                        })),
                ),
            )
            .child(
                self.probe(
                    "fit",
                    self.raw_button("fit-button", "Fit", false, cx)
                        .h(px(22.))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.act(Action::Command(Command::Fit), window, cx)
                        })),
                ),
            )
    }
}
impl Render for Desktop {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.probes.lock().unwrap().clear();
        let weak = cx.weak_entity();
        let modal = self.modal_view(cx);
        menus::bind(div(), cx)
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key))
            .child(self.header(cx))
            .child(self.toolbar(cx))
            .child(
                row()
                    .gap_0()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .child(self.tool_rail(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.probe("canvas", self.canvas_view(cx)).size_full()),
                    )
                    .child(self.inspector(cx)),
            )
            .child(self.footer(cx))
            .children(modal)
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, _, cx| {
                        let _ = weak.update(cx, |this, _| this.trace_paint());
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}
