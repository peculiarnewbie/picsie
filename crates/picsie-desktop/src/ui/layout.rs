//! ContentView.swift shell adaptation, Compositor 609dbeae. MIT © 2026 Wonder Assembly LLC.
//! macOS titlebar tabs are represented by the current document; Linux/Windows use Kit menus.
use super::input::TOOLS;
use super::*;
use gpui_kit::component::{
    menu::{DropdownMenu, PopupMenu, PopupMenuItem},
    popover::Popover,
};
impl Desktop {
    fn menu_item(
        &self,
        id: &'static str,
        title: impl Into<SharedString>,
        action: Action,
        disabled: bool,
        cx: &Context<Self>,
    ) -> PopupMenuItem {
        let entity = cx.entity();
        let title = title.into();
        let click = entity.clone();
        let item = PopupMenuItem::element(move |_, cx| {
            entity.update(cx, |this, _| {
                this.probe(id, div().w_full().text_size(px(12.)).child(title.clone()))
            })
        })
        .disabled(disabled || self.busy);
        match id {
            "pixels-copy" => item.action(Box::new(gpui_kit::component::input::Copy)),
            "pixels-cut" => item.action(Box::new(gpui_kit::component::input::Cut)),
            "pixels-paste" => item.action(Box::new(gpui_kit::component::input::Paste)),
            _ => item.on_click(move |_, window, cx| {
                click.update(cx, |this, cx| this.act(action.clone(), window, cx))
            }),
        }
    }
    fn app_menu(&self, name: &'static str, cx: &Context<Self>) -> Stateful<Div> {
        // Kit handles keyboard navigation, focus restoration, dismissal and popup placement.
        // PopupMenuItem isn't Clone, so prepare a fresh list when the trigger opens.
        let entity = cx.entity();
        let close = entity.clone();
        self.probe(
            format!("menu-{name}"),
            self.raw_button(format!("menu-button-{name}"), name, false, cx)
                .ghost()
                .rounded(px(5.))
                .h(px(28.))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        // Capture before BasePopover takes focus, so its commands target the input/canvas.
                        this.menu_focus = window.focused(cx);
                        this.menu_text_focus = window.focused_input(cx).is_some();
                    }),
                )
                .dropdown_menu(move |menu, window, cx| {
                    entity.update(cx, |this, dcx| this.menu_contents(name, menu, window, dcx))
                })
                .on_open_change(move |open, _, cx| {
                    close.update(cx, |this, _| {
                        if !*open {
                            this.menu_focus = None;
                            this.menu_text_focus = false;
                        }
                    })
                }),
        )
    }
    fn menu_contents(
        &self,
        name: &str,
        mut menu: PopupMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        menu = menu.action_context(self.menu_focus.clone().unwrap_or(self.focus.clone()));
        let text_focus = self.menu_text_focus;
        let state = self.state.as_ref();
        let no_selection = state.is_none_or(|s| s.selection.ids.is_empty());
        let locked = state.is_none_or(|s| !s.unlocked_selection());
        let entries: Vec<(&'static str, &'static str, Action, bool)> = match name {
            "File" => vec![
                ("new", "New Canvas…", Action::New, false),
                (
                    "open",
                    "Open Project…",
                    Action::File(FileAction::Open),
                    false,
                ),
                (
                    "open-comp",
                    "Open Compositor Package…",
                    Action::File(FileAction::OpenComp),
                    false,
                ),
                (
                    "import",
                    "Import Image…",
                    Action::File(FileAction::Import),
                    false,
                ),
                ("save", "Save", Action::File(FileAction::Save), false),
                (
                    "save-as",
                    "Save As…",
                    Action::File(FileAction::SaveAs),
                    false,
                ),
                (
                    "save-comp",
                    "Save Compositor Package…",
                    Action::File(FileAction::SaveComp),
                    false,
                ),
                (
                    "export-png",
                    "Export PNG…",
                    Action::File(FileAction::ExportPng),
                    false,
                ),
                (
                    "export-jpeg",
                    "Export JPEG…",
                    Action::File(FileAction::ExportJpeg),
                    false,
                ),
                ("close", "Close Window", Action::Close, false),
            ],
            "Edit" => vec![
                (
                    "undo",
                    "Undo",
                    Action::Command(Command::Undo),
                    state.is_none_or(|s| !s.history.can_undo),
                ),
                (
                    "redo",
                    "Redo",
                    Action::Command(Command::Redo),
                    state.is_none_or(|s| !s.history.can_redo),
                ),
                (
                    "pixels-cut",
                    "Cut",
                    Action::Copy {
                        merged: false,
                        cut: true,
                    },
                    state.is_none_or(|s| !s.can_edit_pixels || !s.has_pixel_selection),
                ),
                (
                    "pixels-copy",
                    "Copy",
                    Action::Copy {
                        merged: false,
                        cut: false,
                    },
                    state.is_none_or(|s| !s.can_copy_pixels),
                ),
                (
                    "pixels-copy-merged",
                    "Copy Merged",
                    Action::Copy {
                        merged: true,
                        cut: false,
                    },
                    false,
                ),
                ("pixels-paste", "Paste", Action::Paste, false),
                (
                    "pixels-fill-foreground",
                    "Fill with Foreground Color",
                    Action::Command(Command::FillSelection),
                    state.is_none_or(|s| !s.can_edit_pixels),
                ),
                (
                    "pixels-fill-background",
                    "Fill with Background Color",
                    Action::Command(Command::FillBackground),
                    state.is_none_or(|s| !s.can_edit_pixels),
                ),
                (
                    "pixels-transform",
                    "Transform…",
                    Action::Command(Command::BeginTransform),
                    locked,
                ),
                (
                    "pixels-distort",
                    "Distort",
                    Action::Command(Command::BeginDistort),
                    locked
                        || state.is_none_or(|s| {
                            s.selection.ids.len() != 1
                                || s.selected()
                                    .is_none_or(|l| l.kind() == "text" || l.kind() == "group")
                        }),
                ),
            ],
            "Select" => vec![
                (
                    "pixels-expand-menu",
                    "Expand…",
                    Action::SelectionAmount("Expand"),
                    state.is_none_or(|s| !s.can_modify_selection),
                ),
                (
                    "pixels-contract-menu",
                    "Contract…",
                    Action::SelectionAmount("Contract"),
                    state.is_none_or(|s| !s.can_modify_selection),
                ),
                (
                    "pixels-feather-menu",
                    "Feather…",
                    Action::SelectionAmount("Feather"),
                    state.is_none_or(|s| !s.can_modify_selection),
                ),
                (
                    "pixels-layer",
                    "Layer Pixels",
                    Action::Command(Command::SelectLayerPixels),
                    state.is_none_or(|s| s.selected().is_none_or(|l| l.kind() == "group")),
                ),
                (
                    "pixels-all",
                    "All",
                    Action::Command(Command::SelectAllPixels),
                    false,
                ),
                (
                    "pixels-deselect-menu",
                    "Deselect",
                    Action::Command(Command::DeselectPixels),
                    state.is_none_or(|s| !s.has_pixel_selection),
                ),
                (
                    "pixels-invert",
                    "Inverse",
                    Action::Command(Command::InvertSelection),
                    state.is_none_or(|s| !s.has_pixel_selection),
                ),
                (
                    "pixels-fill",
                    "Fill with Foreground",
                    Action::Command(Command::FillSelection),
                    state.is_none_or(|s| !s.can_edit_pixels),
                ),
                (
                    "pixels-clear",
                    "Clear Pixels",
                    Action::Command(Command::ClearSelectedPixels),
                    state.is_none_or(|s| !s.can_edit_pixels || !s.has_pixel_selection),
                ),
            ],
            "Image" => vec![
                (
                    "canvas-size-menu",
                    "Canvas Size…",
                    Action::CanvasSize,
                    false,
                ),
                ("image-size", "Image Size…", Action::ImageSize, false),
            ],
            "Layer" => vec![
                (
                    "add-gradient",
                    "New Gradient Layer",
                    Action::Command(Command::AddGradient),
                    false,
                ),
                (
                    "duplicate",
                    "Duplicate Layers",
                    Action::Command(Command::Duplicate),
                    no_selection,
                ),
                (
                    "pixels-via-copy",
                    "Layer via Copy",
                    Action::Command(Command::LayerViaCopy),
                    state.is_none_or(|s| !s.can_copy_pixels),
                ),
                (
                    "group",
                    "Group Selected",
                    Action::Command(Command::GroupSelected),
                    no_selection,
                ),
                (
                    "out-of-folder",
                    "Move Out of Folder",
                    Action::Command(Command::MoveToGroup { parent_id: None }),
                    no_selection,
                ),
                (
                    "raise",
                    "Raise Layer",
                    Action::Command(Command::Reorder { direction: 1 }),
                    locked,
                ),
                (
                    "lower",
                    "Lower Layer",
                    Action::Command(Command::Reorder { direction: -1 }),
                    locked,
                ),
                (
                    "layers-merge",
                    "Merge Layers / Down / Group",
                    Action::Command(Command::MergeLayers),
                    locked,
                ),
            ],
            _ => vec![
                (
                    "fit-menu",
                    "Fit Canvas",
                    Action::Command(Command::Fit),
                    false,
                ),
                (
                    "actual-menu",
                    "Actual Pixels",
                    Action::Command(Command::Zoom {
                        zoom: 1.,
                        point: None,
                    }),
                    false,
                ),
            ],
        };
        for (id, title, action, disabled) in entries {
            let title = match id {
                "undo" => state
                    .filter(|s| s.history.can_undo)
                    .map(|s| format!("Undo {}", s.history.undo_label)),
                "redo" => state
                    .filter(|s| s.history.can_redo)
                    .map(|s| format!("Redo {}", s.history.redo_label)),
                "layers-merge" => state.map(|s| {
                    if s.selection.ids.len() > 1 {
                        "Merge Layers"
                    } else if s.selected().is_some_and(|l| l.kind() == "group") {
                        "Merge Folder"
                    } else {
                        "Merge Down"
                    }
                    .to_owned()
                }),
                _ => None,
            }
            .unwrap_or_else(|| title.to_owned());
            menu = menu.item(self.menu_item(
                id,
                title,
                action,
                disabled
                    && !(text_focus && matches!(id, "pixels-copy" | "pixels-cut" | "pixels-paste")),
                cx,
            ));
        }
        if name == "View" {
            menu = menu.separator();
            for (key, title) in [
                ("rulers", "Rulers"),
                ("guides", "Guides"),
                ("grid", "Grid"),
                ("lock-guides", "Lock Guides"),
                ("snap", "Snap"),
                ("snap-guides", "Snap to Guides"),
                ("snap-grid", "Snap to Grid"),
                ("snap-layers", "Snap to Layers"),
                ("snap-bounds", "Snap to Document Bounds"),
            ] {
                menu = menu.item(
                    self.menu_item(key, title, Action::ViewOption(key), false, cx)
                        .checked(self.view_option(key)),
                );
            }
            menu = menu.separator().item(self.menu_item(
                "clear-guides",
                "Clear Guides",
                Action::Command(Command::ClearGuides),
                state.is_none_or(|s| s.document.guides.is_empty()),
                cx,
            ));
        }
        menu.min_w(px(220.))
    }
    fn header(&self, cx: &Context<Self>) -> Div {
        let mut header = row()
            .h(px(42.))
            .flex_shrink_0()
            .px(px(12.))
            .gap(px(3.))
            .border_b_1()
            .border_color(rgb(LINE));
        header = header.child(self.icon_button(
            "new-canvas",
            "plus",
            "New canvas (Ctrl/⌘N)".into(),
            Action::New,
            false,
            false,
            cx,
        ));
        for name in ["File", "Edit", "Select", "Image", "Layer", "View"] {
            header = header.child(self.app_menu(name, cx));
        }
        header = header
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_center()
                    .text_ellipsis()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(
                        self.state
                            .as_ref()
                            .map(|s| {
                                format!(
                                    "{}{}",
                                    s.document.name,
                                    if s.history.dirty { " •" } else { "" }
                                )
                            })
                            .unwrap_or_else(|| "Picsie".into()),
                    ),
            )
            .child(self.command_button("fit", "Fit", Command::Fit, false, false, cx))
            .child(self.command_button(
                "actual",
                "100%",
                Command::Zoom {
                    zoom: 1.,
                    point: None,
                },
                false,
                false,
                cx,
            ));
        for (id, title, factor) in [("zoom-in", "+", 1.25), ("zoom-out", "−", 0.8)] {
            header = header.child(
                self.probe(
                    id,
                    self.raw_button(format!("{id}-button"), title, false, cx)
                        .w(px(28.))
                        .tooltip(if factor > 1. { "Zoom in" } else { "Zoom out" })
                        .on_click(cx.listener(move |this, _, _, _| {
                            this.engine.request(Operation::ZoomBy {
                                factor,
                                point: None,
                            });
                        })),
                ),
            );
        }
        header
    }
    fn tool_rail(&self, cx: &Context<Self>) -> Stateful<Div> {
        let tool = self.state.as_ref().map(|s| s.tool).unwrap_or(Tool::Move);
        let mut rail = div()
            .id("tool-rail")
            .w(px(56.))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .pt(px(16.))
            .pb(px(12.))
            .border_r_1()
            .border_color(rgb(LINE))
            .overflow_y_scroll();
        for (value, id, name, key) in TOOLS {
            if matches!(value, Tool::Eraser | Tool::Ellipse) {
                continue;
            }
            let active = tool == *value
                || (*value == Tool::Brush && tool == Tool::Eraser)
                || (*value == Tool::Rectangle && tool == Tool::Ellipse);
            let command = Command::SetTool { tool: *value };
            rail = rail.child(
                self.probe(
                    format!("tool-{id}"),
                    self.raw_button(format!("tool-button-{id}"), "", active, cx)
                        .with_size(gpui_kit::component::Size::Size(px(24.)))
                        .icon(icon(
                            if *value == Tool::Rectangle && tool == Tool::Ellipse {
                                "ellipse"
                            } else if *value == Tool::Marquee
                                && self
                                    .state
                                    .as_ref()
                                    .is_some_and(|s| s.marquee_kind == "ellipse")
                            {
                                "marquee-ellipse"
                            } else if *value == Tool::Lasso
                                && self
                                    .state
                                    .as_ref()
                                    .is_some_and(|s| s.lasso_kind == "polygonal")
                            {
                                "lasso-polygonal"
                            } else {
                                id
                            },
                            18.,
                        ))
                        .w(px(36.))
                        .h(px(36.))
                        .rounded(px(7.))
                        .ghost()
                        .when(active, |b| {
                            b.bg(rgb(0x414141)).border_1().border_color(rgb(0x505050))
                        })
                        .tooltip(format!("{name} ({key})"))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.act(Action::Command(command.clone()), window, cx)
                        })),
                )
                .flex_shrink_0(),
            );
        }
        rail.child(self.foreground_swatch(cx, true).mt(px(8.)).flex_shrink_0())
    }
    fn footer(&self, cx: &Context<Self>) -> Div {
        let state = self.state.as_ref();
        row()
            .h(px(30.))
            .flex_shrink_0()
            .px(px(18.))
            .gap(px(16.))
            .border_t_1()
            .border_color(rgb(LINE))
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .child(div().w(px(62.)).flex_shrink_0().child(
                format!("{}%",state.map(|s|(s.viewport.zoom*100.).round() as u32).unwrap_or(100)),
            ))
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
                    .ghost()
                    .h(px(24.))
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .tooltip("Canvas Size (Ctrl/⌘+Alt+C)")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.act(Action::CanvasSize, window, cx)),
                    ),
                ),
            )
            .child(div().flex_shrink_0().child("sRGB · Transparent"))
            .child(div().flex_1())
            .child(
                div()
                    .min_w_0()
                    .text_ellipsis()
                    .text_color(rgb(if self.busy { ACCENT } else { MUTED }))
                    .child(if self.busy {
                        "Working…".into()
                    } else {
                        self.notice.clone()
                    }),
            )
    }
    fn layers_panel(&self, cx: &Context<Self>) -> Div {
        let mut panel = column()
            .gap_0()
            .w(px(self.layers_width))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(PANEL));
        let Some(state) = &self.state else {
            return panel;
        };
        let layer = state.selected();
        let locked = layer.is_none_or(|l| l.locked);
        panel = panel
            .child(
                row()
                    .h(px(52.))
                    .px(px(18.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Layers"))
                    .child(label(state.document.layers.len().to_string())),
            )
            .child(
                column()
                    .gap(px(8.))
                    .p(px(12.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        row()
                            .child(label("Blend").w(px(42.)))
                            .child(self.blend_picker(
                                locked
                                    || state.selection.ids.len() != 1
                                    || layer.is_none_or(|l| l.kind() == "group"),
                                cx,
                            )),
                    )
                    .child(self.slider(
                        "opacity",
                        "Opacity",
                        layer.map(|l| l.opacity * 100.).unwrap_or(100.),
                        "%",
                        locked,
                    )),
            )
            .child(
                self.probe("layer-list", self.layer_list(state, cx))
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .w_full(),
            );
        let entity = cx.entity();
        let props = entity.clone();
        let effects = entity;
        let effect_open = cx.entity();
        let props_open = cx.entity();
        panel.child(
            row()
                .gap(px(4.))
                .h(px(48.))
                .px(px(8.))
                .flex_shrink_0()
                .border_t_1()
                .border_color(rgb(LINE))
                .child(self.icon_button(
                    "add-paint",
                    "layer-add",
                    "New paint layer".into(),
                    Action::Command(Command::AddPaintLayer),
                    false,
                    false,
                    cx,
                ))
                .child(self.icon_button(
                    "add-folder",
                    "folder-add",
                    "New folder".into(),
                    Action::Command(Command::AddGroup),
                    false,
                    false,
                    cx,
                ))
                .child(self.icon_button(
                    "mask-menu",
                    "mask",
                    if state.has_pixel_selection {
                        "Add layer mask (the selection becomes black)".into()
                    } else {
                        "Add layer mask".into()
                    },
                    Action::Command(Command::AddMask {
                        base: picsie_core::model::MaskMode::Reveal,
                    }),
                    locked
                        || state.selection.ids.len() != 1
                        || layer.is_none_or(|l| l.mask.is_some()),
                    false,
                    cx,
                ))
                .child(
                    self.probe(
                        "adjustments-menu",
                        Popover::new("adjustments-popover")
                            .open(self.open_palette == Some("adjustments-menu"))
                            .on_open_change(move |open, window, cx| {
                                effect_open.update(cx, |this, cx| {
                                    this.open_palette = if *open {
                                        Some("adjustments-menu")
                                    } else {
                                        None
                                    };
                                    if !*open {
                                        this.commit_active_fields(window, cx);
                                        window.focus(&this.focus, cx);
                                    }
                                    cx.notify();
                                })
                            })
                            .anchor(Anchor::BottomLeft)
                            .trigger(
                                self.raw_button("adjustments-trigger", "", false, cx)
                                    .with_size(gpui_kit::component::Size::Size(px(24.)))
                                    .ghost()
                                    .icon(icon("adjustments", 18.))
                                    .w(px(30.))
                                    .disabled(layer.is_none())
                                    .tooltip("Adjustments"),
                            )
                            .content(move |_, _, cx| {
                                effects.update(cx, |this, _| {
                                    section("Adjustments")
                                        .w(px(330.))
                                        .child(this.slider(
                                            "brightness",
                                            "Brightness",
                                            0.,
                                            "%",
                                            false,
                                        ))
                                        .child(this.slider(
                                            "saturation",
                                            "Saturation",
                                            0.,
                                            "%",
                                            false,
                                        ))
                                        .child(this.slider(
                                            "blur",
                                            "Gaussian blur",
                                            0.,
                                            "px",
                                            false,
                                        ))
                                })
                            }),
                    ),
                )
                .child(
                    self.probe(
                        "properties-menu",
                        Popover::new("layer-properties-popover")
                            .open(self.open_palette == Some("properties-menu"))
                            .on_open_change(move |open, window, cx| {
                                props_open.update(cx, |this, cx| {
                                    this.open_palette =
                                        if *open { Some("properties-menu") } else { None };
                                    if !*open {
                                        this.commit_active_fields(window, cx);
                                        window.focus(&this.focus, cx);
                                    }
                                    cx.notify();
                                })
                            })
                            .anchor(Anchor::BottomLeft)
                            .trigger(
                                self.raw_button("layer-properties-trigger", "", false, cx)
                                    .with_size(gpui_kit::component::Size::Size(px(24.)))
                                    .ghost()
                                    .icon(icon("more", 18.))
                                    .w(px(30.))
                                    .disabled(layer.is_none())
                                    .tooltip("Layer properties"),
                            )
                            .content(move |_, _, cx| {
                                props.update(cx, |this, cx| {
                                    let mut d = section("Layer Properties").w(px(300.));
                                    if let Some(state) = &this.state
                                        && let Some(layer) = state.selected()
                                    {
                                        d = d
                                            .child(this.field("name", "Name", layer.locked))
                                            .child(this.mask_sections(state, layer, cx))
                                            .child(this.command_button(
                                                "lock",
                                                if layer.locked {
                                                    "Unlock layer"
                                                } else {
                                                    "Lock layer"
                                                },
                                                Command::UpdateLayer {
                                                    patch: json!({"locked":!layer.locked}),
                                                },
                                                false,
                                                layer.locked,
                                                cx,
                                            ));
                                        if layer.fill().is_some() {
                                            d = d.child(this.layer_swatch(cx)).child(
                                                this.probe(
                                                    "use-foreground",
                                                    this.raw_button(
                                                        "use-foreground-button",
                                                        "Use foreground color",
                                                        false,
                                                        cx,
                                                    )
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.commit_active_fields(window, cx);
                                                        let key = if this
                                                            .state
                                                            .as_ref()
                                                            .and_then(|s| s.selected())
                                                            .is_some_and(|l| l.kind() == "gradient")
                                                        {
                                                            "from"
                                                        } else {
                                                            "color"
                                                        };
                                                        this.engine.request(
                                                            Operation::ForegroundFill { key },
                                                        );
                                                    })),
                                                ),
                                            );
                                        }
                                        if layer.kind() == "gradient" {
                                            d = d.child(
                                                this.probe(
                                                    "gradient-end",
                                                    this.raw_button(
                                                        "gradient-end-button",
                                                        "Use foreground for end color",
                                                        false,
                                                        cx,
                                                    )
                                                    .on_click(cx.listener(|this, _, _, _| {
                                                        this.engine.request(
                                                            Operation::ForegroundFill { key: "to" },
                                                        );
                                                    })),
                                                ),
                                            );
                                        }
                                    }
                                    d
                                })
                            }),
                    ),
                )
                .child(div().flex_1())
                .child(
                    self.icon_button(
                        "delete",
                        "trash",
                        if state.paint_target == "mask" {
                            "Delete layer mask"
                        } else {
                            "Delete selected layers"
                        }
                        .into(),
                        Action::Command(if state.paint_target == "mask" {
                            Command::RemoveMask
                        } else {
                            Command::Remove
                        }),
                        !state.unlocked_selection(),
                        false,
                        cx,
                    ),
                ),
        )
    }
    fn panel_edge(&self, cx: &Context<Self>) -> Stateful<Div> {
        self.probe("layers-resize", div().w(px(1.)).h_full().bg(rgb(LINE)))
            .flex_shrink_0()
            .relative()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(-4.))
                    .w(px(8.))
                    .h_full()
                    .cursor(CursorStyle::ResizeLeftRight)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.panel_drag =
                                Some((f32::from(event.position.x), this.layers_width));
                            cx.stop_propagation();
                        }),
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
            .on_modifiers_changed(cx.listener(|this, event: &ModifiersChangedEvent, _, cx| {
                this.cursor_modifiers = picsie_core::editor::Modifiers {
                    shift: event.modifiers.shift,
                    alt: event.modifiers.alt,
                    control: event.modifiers.control,
                    meta: event.modifiers.platform,
                };
                this.refresh_cursor(cx);
                cx.notify();
            }))
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
                            .child(self.canvas_workspace(cx)),
                    )
                    .child(self.panel_edge(cx))
                    .child(self.layers_panel(cx)),
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
