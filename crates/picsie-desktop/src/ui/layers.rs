//! LayersPanel.swift / NativeLayerList.swift adaptations, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Native Kit controls retain the existing Rust commands.
use super::*;
use gpui_kit::component::menu::ContextMenuExt;
use picsie_core::{
    adjustment::AdjustmentKind,
    editor::{PaintTarget, SelectionMode},
    model::MaskMode,
};
impl Desktop {
    pub(super) fn layer_list(&self, state: &Snapshot, cx: &Context<Self>) -> Stateful<Div> {
        let entity = cx.entity().downgrade();
        let list = gpui::uniform_list(
            "layers-items",
            state.layer_rows.len(),
            move |range, _, cx| {
                entity
                    .update(cx, |this, cx| {
                        let Some(state) = this.state.as_ref() else {
                            return Vec::new();
                        };
                        range
                            .filter_map(|i| state.layer_rows.get(i))
                            .map(|item| this.layer_row(state, item, cx))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            },
        )
        .track_scroll(&self.list_scroll)
        .h_full()
        .w_full()
        .min_h_0()
        .flex_1();
        div()
            .id("layers-scroll")
            .role(gpui::Role::ListBox)
            .aria_label("Layers")
            .h_full()
            .w_full()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .min_h_0()
            .when(state.document.layers.is_empty(), |d| {
                d.child(hint("Import an image or add a layer to begin.").p(px(12.)))
            })
            .child(list)
    }
    fn layer_row(
        &self,
        state: &Snapshot,
        item: &crate::state::Row,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(layer) = state.layer(&item.id) else {
            return div().into_any_element();
        };
        let id = layer.id.clone();
        let selected = state.is_selected(&id);
        let clipping_boundary = self.cursor_modifiers.alt
            && item.can_toggle_clipping
            && self.list_pointer.is_some_and(|p| {
                self.probes
                    .lock()
                    .unwrap()
                    .get(&format!("layer-{id}"))
                    .is_some_and(|b| {
                        f32::from(p.y) >= b[1] + b[3] - 10. && f32::from(p.y) < b[1] + b[3]
                    })
            });
        let drag_target = self
            .layer_drag
            .as_ref()
            .and_then(|d| d.destination.as_ref())
            .filter(|(target, _)| target == &id)
            .map(|(_, side)| *side);
        let mut entry = row()
            .gap(px(4.))
            .h(px(52.))
            .w_full()
            .flex_shrink_0()
            .pl(px(5. + item.depth as f32 * 14.))
            .pr(px(6.))
            .border_b_1()
            .border_color(rgb(0x2d2d2d))
            .bg(rgb(if selected { 0x364c65 } else { PANEL }))
            .relative();
        // GPUI's portable cursor set replaces the AppKit bitmap cursors.
        entry = entry.cursor(if clipping_boundary {
            CursorStyle::DragLink
        } else if self.cursor_modifiers.alt && !layer.locked {
            CursorStyle::DragCopy
        } else {
            CursorStyle::Arrow
        });
        if clipping_boundary {
            entry = entry.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(1.))
                    .bg(rgb(ACCENT)),
            );
        }
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
        entry = entry.child(
            self.probe(
                format!("visibility-{id}"),
                self.raw_button(format!("visibility-button-{id}"), "", false, cx)
                    .ghost()
                    .with_size(gpui_kit::component::Size::Size(px(22.)))
                    .h(px(28.))
                    .w(px(20.))
                    .icon(
                        icon(if layer.visible { "eye" } else { "eyeOff" }, 16.)
                            .text_color(rgb(if item.visible { TEXT } else { MUTED })),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = id.clone();
                    move |this, event: &MouseDownEvent, window, cx| {
                        if !this.busy {
                            this.commit_active_fields(window, cx);
                            this.visibility_swiping = true;
                            this.list_pointer = Some(event.position);
                            this.send(Command::BeginVisibilitySwipe { id: id.clone() });
                            this.start_list_autoscroll(window, cx);
                        }
                        cx.stop_propagation();
                    }
                }),
            ),
        );
        if layer.mask_source_id.is_some() {
            entry = entry.child(div().text_color(rgb(ACCENT)).child("↳"));
        }
        let thumbnail = div()
            .w(px(36.))
            .h(px(36.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center();
        let thumbnail = if let Some((picture, image)) = self.thumbnails.get(&id) {
            thumbnail.child(
                img(image.clone())
                    .w(px(picture.width as f32 / 2.))
                    .h(px(picture.height as f32 / 2.))
                    .rounded(px(3.))
                    .border_color(rgb(if selected && state.paint_target != "mask" {
                        ACCENT
                    } else {
                        LINE
                    }))
                    .border_1(),
            )
        } else {
            thumbnail.child(
                icon(
                    if layer.kind() == "group" {
                        "folder"
                    } else {
                        "image"
                    },
                    22.,
                )
                .text_color(rgb(MUTED)),
            )
        };
        let thumb_id = id.clone();
        entry = entry.child(
            self.probe(format!("thumbnail-{id}"), thumbnail)
                .cursor(
                    if self.cursor_modifiers.control || self.cursor_modifiers.meta {
                        CursorStyle::Crosshair
                    } else {
                        CursorStyle::Arrow
                    },
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.thumbnail_down(thumb_id.clone(), false, event, window, cx)
                    }),
                ),
        );
        if let Some(mask) = &layer.mask {
            if layer.kind() != "group" {
                entry = entry.child(
                    self.probe(
                        format!("mask-chain-{id}"),
                        self.raw_button(format!("mask-chain-button-{id}"), "", false, cx)
                            .ghost()
                            .p_0()
                            .w(px(9.))
                            .h(px(20.))
                            .when(mask["linked"] == true, |b| {
                                b.icon(icon("link", 10.).text_color(rgb(MUTED)))
                            })
                            .tooltip(if mask["linked"] == true {
                                "Unlink layer and mask"
                            } else {
                                "Link layer and mask"
                            })
                            .disabled(layer.locked || self.busy)
                            .on_click(cx.listener({
                                let id = id.clone();
                                move |this, _, window, cx| {
                                    this.act(
                                        Action::Command(Command::ToggleMaskLinkFor {
                                            id: id.clone(),
                                        }),
                                        window,
                                        cx,
                                    )
                                }
                            })),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                );
            }
            entry = entry.child(
                    self.probe(
                        format!("mask-row-{id}"),
                        self.raw_button(
                            format!("mask-row-button-{id}"),
                            "",
                            selected && state.paint_target == "mask",
                            cx,
                        )
                        .w(px(30.))
                        .h(px(30.))
                        .rounded(px(3.))
                        .p_0()
                        .tooltip("Select mask · Shift enables/disables · Ctrl/⌘ selects black areas · Alt drags a copy")
                        .when_some(
                            self.thumbnails.get(&format!("@mask-{id}")),
                            |b, (picture, image)| {
                                b.child(
                                    img(image.clone())
                                        .w(px(picture.width as f32 * 5. / 12.))
                                        .h(px(picture.height as f32 * 5. / 12.))
                                        .rounded(px(3.))
                                        .border_1()
                                        .border_color(rgb(
                                            if selected && state.paint_target == "mask" {
                                                ACCENT
                                            } else {
                                                LINE
                                            },
                                        )),
                                )
                            },
                        )
                        .when(mask["enabled"] == false, |b| b.child(div().absolute().inset_0().flex().items_center().justify_center().text_size(px(30.)).text_color(rgb(0xff4c55)).child("×"))),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let id = id.clone();
                            move |this, event: &MouseDownEvent, window, cx| {
                                this.thumbnail_down(id.clone(), true, event, window, cx)
                            }
                        }),
                    ),
                );
        }
        entry = entry.child(
            column()
                .gap(px(2.))
                .flex_1()
                .min_w_0()
                .child(if self.rename_layer.as_ref() == Some(&id) {
                    self.probe(
                        "inline-rename",
                        Input::new(&self.fields["inline-name"])
                            .small()
                            .h(px(26.))
                            .aria_label("Layer name"),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .into_any_element()
                } else {
                    self.probe(
                        format!("layer-name-{id}"),
                        div()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .child(layer.name.clone()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let id = id.clone();
                            move |this, event: &MouseDownEvent, window, cx| {
                                if event.click_count == 2 {
                                    let adjustment = this
                                        .state
                                        .as_ref()
                                        .and_then(|s| s.layer(&id))
                                        .and_then(|l| l.adjustment);
                                    if adjustment.is_some() {
                                        this.commit_active_fields(window, cx);
                                        this.send(Command::BeginAdjustmentEdit { id: id.clone() });
                                    } else {
                                        this.begin_rename(id.clone(), window, cx);
                                    }
                                    cx.stop_propagation();
                                }
                            }
                        }),
                    )
                    .into_any_element()
                })
                .child(label(format!("{} × {} px", layer.width, layer.height))),
        );
        if let Some(kind) = &layer.adjustment {
            entry = entry.child(
                self.probe(
                    format!("adjustment-tag-{id}"),
                    div()
                        .flex_shrink_0()
                        .px(px(6.))
                        .py(px(1.))
                        .rounded(px(6.))
                        .bg(rgb(ACCENT))
                        .text_size(px(10.))
                        .text_color(rgb(0x101010))
                        .child(match kind {
                            AdjustmentKind::Levels => "Levels",
                            AdjustmentKind::Curves => "Curves",
                        }),
                ),
            );
        }
        if layer.locked {
            entry = entry.child(icon("lock", 14.).text_color(rgb(MUTED)));
        }
        let menu_id = id.clone();
        let entity = cx.entity();
        return (self
            .probe(format!("layer-{id}"), entry)
            .role(gpui::Role::ListBoxOption)
            .aria_selected(selected)
            .aria_label(format!(
                "{}, {}, {} by {} pixels{}{}",
                layer.name,
                layer.kind(),
                layer.width,
                layer.height,
                if layer.locked { ", locked" } else { "" },
                if layer.mask.is_some() {
                    ", layer mask"
                } else {
                    ""
                }
            ))
            .w_full()
            .flex_shrink_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.layer_down(id.clone(), event, window, cx)
                }),
            )
            .context_menu(move |menu, window, cx| {
                entity.update(cx, |this, cx| {
                    this.layer_context_menu(menu, &menu_id, window, cx)
                })
            }))
        .into_any_element();
    }
    pub(super) fn mask_sections(
        &self,
        state: &Snapshot,
        layer: &LayerInfo,
        cx: &Context<Self>,
    ) -> Div {
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
                .child(self.command_button("mask-link",if linked{"Linked to layer"}else{"Independent mask"},Command::ToggleMaskLink,layer.locked,linked,cx))
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
}

impl Desktop {
    pub(super) fn begin_rename(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.finish_rename(true, window, cx);
        let Some(layer) = self
            .state
            .as_ref()
            .and_then(|s| s.layer(&id))
            .filter(|l| !l.locked)
        else {
            return;
        };
        let name = layer.name.clone();
        self.send(Command::Select {
            id: Some(id.clone()),
            mode: SelectionMode::Replace,
        });
        self.rename_layer = Some(id);
        self.layer_drag = None;
        self.set_field("inline-name", name, true, window, cx);
        self.edited_fields.remove("inline-name");
        self.fields["inline-name"].update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }
    pub(super) fn finish_rename(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.rename_layer.take() {
            self.edited_fields.remove("inline-name");
            let name = self.field_value("inline-name", cx).trim().to_owned();
            if commit && !name.is_empty() {
                self.send(Command::Select {
                    id: Some(id),
                    mode: SelectionMode::Replace,
                });
                self.patch(json!({"name":name}));
            }
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }
    pub(super) fn thumbnail_down(
        &mut self,
        id: String,
        mask: bool,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        if event.modifiers.control || event.modifiers.platform {
            let mode = if event.modifiers.alt {
                picsie_core::pixel_selection::PixelSelectionMode::Subtract
            } else if event.modifiers.shift {
                picsie_core::pixel_selection::PixelSelectionMode::Add
            } else {
                picsie_core::pixel_selection::PixelSelectionMode::Replace
            };
            self.commit_active_fields(window, cx);
            self.send(Command::LoadThumbnailSelection { id, mask, mode });
            window.focus(&self.focus, cx);
            cx.stop_propagation();
            return;
        }
        if mask {
            self.commit_active_fields(window, cx);
            self.send(Command::Select {
                id: Some(id.clone()),
                mode: SelectionMode::Replace,
            });
            self.send(Command::SetPaintTarget {
                target: PaintTarget::Mask,
            });
            if event.modifiers.shift && !event.modifiers.alt {
                if let Some(mask) = self
                    .state
                    .as_ref()
                    .and_then(|s| s.layer(&id))
                    .and_then(|l| l.mask.as_ref())
                {
                    self.send(Command::UpdateLayer {
                        patch: json!({"mask":{"enabled":mask["enabled"] != true}}),
                    });
                }
            }
            window.focus(&self.focus, cx);
            if event.modifiers.alt {
                self.layer_drag = Some(LayerDrag {
                    origin: event.position,
                    id,
                    moved: false,
                    copy: true,
                    mask: true,
                    select_on_click: false,
                    destination: None,
                });
                self.list_pointer = Some(event.position);
                self.start_list_autoscroll(window, cx);
            }
            cx.stop_propagation();
            cx.notify();
        } else if event.click_count == 2 {
            let adjustment = self
                .state
                .as_ref()
                .and_then(|s| s.layer(&id))
                .and_then(|l| l.adjustment);
            if adjustment.is_some() {
                self.commit_active_fields(window, cx);
                self.send(Command::BeginAdjustmentEdit { id: id.clone() });
            } else if self
                .state
                .as_ref()
                .and_then(|s| s.layer(&id))
                .is_some_and(|l| l.kind() == "text")
            {
                self.send(Command::EditText {
                    id: Some(id),
                    point: None,
                });
            } else {
                self.begin_rename(id, window, cx);
            }
            cx.stop_propagation();
        } else if !event.modifiers.shift && !event.modifiers.alt {
            self.send(Command::Select {
                id: Some(id),
                mode: SelectionMode::Replace,
            });
            self.send(Command::SetPaintTarget {
                target: PaintTarget::Content,
            });
        }
    }
    pub(super) fn update_visibility_swipe(&mut self, position: Point<Pixels>) {
        if !self.visibility_swiping {
            return;
        }
        let bounds = self.probes.lock().unwrap();
        let Some(state) = &self.state else {
            return;
        };
        let y = f32::from(position.y);
        let id = state.layer_rows.iter().find_map(|row| {
            bounds
                .get(&format!("layer-{}", row.id))
                .filter(|b| y >= b[1] && y < b[1] + b[3])
                .map(|_| row.id.clone())
        });
        drop(bounds);
        if let Some(id) = id {
            self.send(Command::SwipeVisibility { id });
        }
    }
    pub(super) fn start_list_autoscroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.list_autoscroll.is_some() {
            return;
        }
        self.list_autoscroll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                let active = this
                    .update_in(cx, |this, _, cx| {
                        if !this.visibility_swiping
                            && !this.layer_drag.as_ref().is_some_and(|d| d.moved)
                        {
                            return this.layer_drag.is_some();
                        }
                        let Some(position) = this.list_pointer else {
                            return true;
                        };
                        let bounds = this.probes.lock().unwrap().get("layer-list").copied();
                        if let Some([left, top, width, height]) = bounds {
                            let x = f32::from(position.x);
                            let y = f32::from(position.y);
                            if x >= left
                                && x <= left + width
                                && y >= top - 40.
                                && y <= top + height + 40.
                            {
                                let delta = if y < top + 24. {
                                    (top + 24. - y).min(40.) * 0.4
                                } else if y > top + height - 24. {
                                    -(y - (top + height - 24.)).min(40.) * 0.4
                                } else {
                                    0.
                                };
                                if delta != 0. {
                                    let mut offset =
                                        this.list_scroll.0.borrow().base_handle.offset();
                                    offset.y += px(delta);
                                    this.list_scroll.0.borrow().base_handle.set_offset(offset);
                                    cx.notify();
                                }
                            }
                        }
                        this.update_layer_drag(position, cx);
                        this.update_visibility_swipe(position);
                        true
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        }));
    }
    pub(super) fn layer_context_menu(
        &mut self,
        mut menu: gpui_kit::component::menu::PopupMenu,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::component::menu::PopupMenu {
        use gpui_kit::component::menu::PopupMenuItem;
        let Some(layer) = self.state.as_ref().and_then(|s| s.layer(id)).cloned() else {
            return menu;
        };
        self.commit_active_fields(window, cx);
        if !self.state.as_ref().is_some_and(|s| s.is_selected(id)) {
            self.send(Command::Select {
                id: Some(id.to_owned()),
                mode: SelectionMode::Replace,
            });
        }
        let rename = cx.entity();
        let rename_id = id.to_owned();
        menu = menu.item(
            PopupMenuItem::new("Rename…")
                .disabled(layer.locked)
                .on_click(move |_, window, cx| {
                    rename.update(cx, |this, cx| {
                        this.begin_rename(rename_id.clone(), window, cx)
                    })
                }),
        );
        let mut actions = vec![
            (
                "Hide/Show Layer",
                Command::UpdateLayer {
                    patch: json!({"visible":!layer.visible}),
                },
                false,
            ),
            (
                if layer.locked {
                    "Unlock Layer"
                } else {
                    "Lock Layer"
                },
                Command::UpdateLayer {
                    patch: json!({"locked":!layer.locked}),
                },
                false,
            ),
            ("Duplicate Layer", Command::Duplicate, false),
        ];
        if let Some(mask) = &layer.mask {
            actions.extend([
                (
                    if mask["enabled"] == true {
                        "Disable Layer Mask"
                    } else {
                        "Enable Layer Mask"
                    },
                    Command::UpdateLayer {
                        patch: json!({"mask":{"enabled":mask["enabled"]!=true}}),
                    },
                    layer.locked,
                ),
                (
                    if mask["linked"] == true {
                        "Unlink Layer Mask"
                    } else {
                        "Link Layer Mask"
                    },
                    Command::ToggleMaskLink,
                    layer.locked,
                ),
                ("Delete Layer Mask", Command::RemoveMask, layer.locked),
            ]);
        } else {
            actions.extend([
                (
                    "Add White Mask",
                    Command::AddMask {
                        base: MaskMode::Reveal,
                    },
                    layer.locked,
                ),
                (
                    "Add Black Mask",
                    Command::AddMask {
                        base: MaskMode::Hide,
                    },
                    layer.locked,
                ),
            ]);
        }
        if layer.kind() != "group" {
            actions.push((
                if layer.mask_source_id.is_some() {
                    "Release Clipping Mask"
                } else {
                    "Create Clipping Mask"
                },
                Command::ToggleClippingMask,
                layer.locked
                    || !self.state.as_ref().is_some_and(|s| {
                        s.layer_rows
                            .iter()
                            .any(|r| r.id == layer.id && r.can_toggle_clipping)
                    }),
            ));
        }
        if layer.parent_id.is_some() {
            actions.push((
                "Move Out of Folder",
                Command::MoveToGroup { parent_id: None },
                layer.locked,
            ));
        }
        actions.push((
            if layer.kind() == "group" {
                "Delete Folder"
            } else {
                "Delete Layer"
            },
            Command::Remove,
            layer.locked,
        ));
        for (label, command, disabled) in actions {
            let entity = cx.entity();
            menu = menu.item(
                PopupMenuItem::new(label)
                    .disabled(disabled || self.busy)
                    .on_click(move |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            this.act(Action::Command(command.clone()), window, cx)
                        })
                    }),
            );
        }
        menu
    }
}
impl Desktop {
    pub(super) fn preview_blend(&mut self, index: usize) {
        self.send(Command::PreviewBlendMode {
            id: self.blend_layer.clone(),
            mode: Some(picsie_core::model::Blend::ALL[index]),
        });
    }
    pub(super) fn close_blend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_palette = None;
        self.blend_layer = None;
        self.send(Command::PreviewBlendMode {
            id: None,
            mode: None,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }
    pub(super) fn commit_blend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .state
            .as_ref()
            .and_then(Snapshot::selected)
            .map(|l| &l.id)
            == self.blend_layer.as_ref()
        {
            self.patch(json!({"blend":BLENDS[self.blend_index].0}));
        }
        self.close_blend(window, cx);
    }
    pub(super) fn blend_picker(&self, disabled: bool, cx: &Context<Self>) -> AnyElement {
        let selected = self.state.as_ref().and_then(Snapshot::selected);
        let label = selected
            .and_then(|l| BLENDS.iter().find(|b| b.0 == l.blend).map(|b| b.1))
            .unwrap_or("Normal");
        let entity = cx.entity();
        let confirm = entity.clone();
        let open = entity.clone();
        let dismiss = entity.clone();
        self.probe(
            "select-blend",
            gpui_kit::base::Select::new("blend-picker")
                .open(self.open_palette == Some("blend-menu"))
                .disabled(disabled || self.busy)
                .accessibility_label("Blend mode")
                .accessibility_value(label)
                .content_focus_handle(&self.blend_focus)
                .on_confirm(move |window, cx| {
                    confirm.update(cx, |this, cx| this.commit_blend(window, cx))
                })
                .on_dismiss(move |window, cx| {
                    dismiss.update(cx, |this, cx| this.close_blend(window, cx))
                })
                .on_open_change(move |is_open, window, cx| {
                    open.update(cx, |this, cx| {
                        if is_open {
                            this.commit_active_fields(window, cx);
                            this.blend_layer = this
                                .state
                                .as_ref()
                                .and_then(Snapshot::selected)
                                .map(|l| l.id.clone());
                            this.blend_index = this
                                .state
                                .as_ref()
                                .and_then(Snapshot::selected)
                                .and_then(|l| BLENDS.iter().position(|b| b.0 == l.blend))
                                .unwrap_or(0);
                            this.open_palette = Some("blend-menu");
                            this.blend_scroll.scroll_to_item(this.blend_index);
                            cx.notify();
                        } else if this.open_palette == Some("blend-menu") {
                            this.close_blend(window, cx);
                        }
                    })
                })
                .child(
                    gpui_kit::component::popover::Popover::new("blend-popover")
                        .track_focus(&self.blend_focus)
                        .open(self.open_palette == Some("blend-menu"))
                        .on_open_change({
                            let entity = entity.clone();
                            move |is_open, window, cx| {
                                entity.update(cx, |this, cx| {
                                    if *is_open {
                                        this.commit_active_fields(window, cx);
                                        this.blend_layer = this
                                            .state
                                            .as_ref()
                                            .and_then(Snapshot::selected)
                                            .map(|l| l.id.clone());
                                        this.blend_index = this
                                            .state
                                            .as_ref()
                                            .and_then(Snapshot::selected)
                                            .and_then(|l| {
                                                BLENDS.iter().position(|b| b.0 == l.blend)
                                            })
                                            .unwrap_or(0);
                                        this.open_palette = Some("blend-menu");
                                        this.blend_scroll.scroll_to_item(this.blend_index);
                                        window.focus(&this.blend_focus, cx);
                                        cx.notify();
                                    } else if this.open_palette == Some("blend-menu") {
                                        this.close_blend(window, cx);
                                    }
                                })
                            }
                        })
                        .trigger(
                            self.raw_button("blend-trigger", label, false, cx)
                                .w(px(self.layers_width - 82.))
                                .disabled(disabled || self.busy),
                        )
                        .content(move |_, _, cx| {
                            entity.update(cx, |this, cx| this.blend_options(cx))
                        }),
                ),
        )
        .into_any_element()
    }
    fn blend_options(&self, cx: &Context<Self>) -> Stateful<Div> {
        let selected = self.state.as_ref().and_then(Snapshot::selected);
        let entity = cx.entity();
        let confirm = entity.clone();
        let mut options = div()
            .id("blend-options")
            .track_focus(&self.blend_focus)
            .on_action(move |_: &gpui_kit::base::actions::Confirm, window, cx| {
                confirm.update(cx, |this, cx| this.commit_blend(window, cx));
                cx.stop_propagation();
            })
            .flex()
            .flex_col()
            .w(px(210.))
            .max_h(px(480.))
            .overflow_y_scroll()
            .track_scroll(&self.blend_scroll);
        for (i, (_, name)) in BLENDS.iter().enumerate() {
            let row_entity = entity.clone();
            let choose = entity.clone();
            options = options.child(
                self.probe(
                    format!("blend-choice-{i}"),
                    div()
                        .id(("blend-choice", i))
                        .h(px(25.))
                        .w_full()
                        .flex_shrink_0()
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .when([1, 5, 9, 16, 20].contains(&i), |d| {
                            d.border_t_1().border_color(rgb(LINE))
                        })
                        .bg(rgb(
                            if self.open_palette == Some("blend-menu") && self.blend_index == i {
                                0x364c65
                            } else {
                                PANEL
                            },
                        ))
                        .child(format!(
                            "{} {}",
                            if selected.is_some_and(|l| l.blend == BLENDS[i].0) {
                                "✓"
                            } else {
                                " "
                            },
                            name
                        )),
                )
                .on_mouse_move(move |_, _, cx| {
                    row_entity.update(cx, |this, cx| {
                        if this.blend_index != i {
                            this.blend_index = i;
                            this.preview_blend(i);
                            cx.notify();
                        }
                    })
                })
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    choose.update(cx, |this, cx| this.commit_blend(window, cx))
                }),
            );
        }
        options
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .rounded(px(8.))
            .track_focus(&self.blend_focus)
            .on_key_down(cx.listener(Self::key))
    }
}
