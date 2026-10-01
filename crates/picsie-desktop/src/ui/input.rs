use super::*;
use picsie_core::{
    editor::{Modifiers as EngineModifiers, Phase, PointerSample, SelectionMode, Side},
    model::Point as EnginePoint,
};
pub(super) const TOOLS: &[(Tool, &str, &str, &str)] = &[
    (Tool::Move, "move", "Move", "V"),
    (Tool::Marquee, "marquee", "Marquee", "M"),
    (Tool::Lasso, "lasso", "Lasso", "L"),
    (Tool::Wand, "wand", "Magic Wand", "W"),
    (Tool::Crop, "crop", "Crop", "C"),
    (Tool::Brush, "brush", "Brush", "B"),
    (Tool::Eraser, "eraser", "Eraser", "E"),
    (Tool::Rectangle, "rectangle", "Shape", "U"),
    (Tool::Ellipse, "ellipse", "Ellipse", "O"),
    (Tool::Text, "text", "Type", "T"),
    (Tool::Eyedropper, "eyedropper", "Eyedropper", "I"),
    (Tool::Hand, "hand", "Hand", "H"),
];
impl Desktop {
    pub(super) fn local(&self, position: Point<Pixels>, scale: f32) -> EnginePoint {
        EnginePoint::new(
            f32::from(position.x - self.bounds.origin.x) as f64 * scale as f64,
            f32::from(position.y - self.bounds.origin.y) as f64 * scale as f64,
        )
    }
    fn pointer(&mut self, phase: Phase, position: Point<Pixels>, modifiers: Modifiers, scale: f32) {
        self.send(Command::Pointer {
            samples: vec![PointerSample {
                phase,
                point: self.local(position, scale),
                modifiers: EngineModifiers {
                    shift: modifiers.shift,
                    alt: modifiers.alt,
                    control: modifiers.control,
                    meta: modifiers.platform,
                },
            }],
        });
    }
    fn resize_canvas(&mut self, bounds: Bounds<Pixels>, scale: f32) {
        self.bounds = bounds;
        if self.display_scale != scale {
            self.display_scale = scale;
            self.send(Command::SetDisplayScale {
                scale: scale as f64,
            });
        }
        let next = (
            (f32::from(bounds.size.width) * scale).round().max(100.) as u32,
            (f32::from(bounds.size.height) * scale).round().max(100.) as u32,
        );
        if self.physical_size != next {
            let first = self.physical_size == (0, 0);
            self.physical_size = next;
            let mut commands = vec![Command::ResizeViewport {
                width: next.0 as f64,
                height: next.1 as f64,
            }];
            if first {
                commands.push(Command::Fit)
            }
            self.engine.send(commands);
        }
    }
    pub(super) fn canvas_view(&self, cx: &Context<Self>) -> Stateful<Div> {
        let layout = cx.weak_entity();
        let paint = layout.clone();
        let image = self.image.clone();
        let cursor = self.canvas_cursor();
        div()
            .id("image-canvas")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x1b1b1b))
            .cursor(cursor)
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                if event.button != MouseButton::Left
                    || this.busy
                    || this
                        .modal
                        .as_ref()
                        .is_some_and(|m| !matches!(m, Modal::Color(_)))
                {
                    return;
                }
                if this.is_sampling_tool(event.modifiers) {
                    this.commit_active_fields(window, cx);
                    this.sampling_color = true;
                    this.sample_point = Some(event.position);
                    this.sample_original = if let Some(Modal::Color(d)) = &this.modal {
                        format!("#{}", d.hsb.hex())
                    } else {
                        this.state
                            .as_ref()
                            .map(|s| s.color.clone())
                            .unwrap_or_else(|| "#000000".into())
                    };
                    this.sample_current = this.sample_original.clone();
                    this.sample_canvas(event.position, window.scale_factor());
                    cx.notify();
                    return;
                }
                this.commit_active_fields(window, cx);
                if this.state.as_ref().is_some_and(|s| s.text_editing) {
                    this.text.update(cx, |input, cx| input.focus(window, cx));
                } else {
                    window.focus(&this.focus, cx);
                }
                let point = this.local(event.position, window.scale_factor());
                this.guide_dragging = this.guide_at(event.position, window.scale_factor());
                if (event.modifiers.control || event.modifiers.platform)
                    && this.state.as_ref().is_some_and(|s| {
                        !matches!(
                            s.tool,
                            Tool::Move
                                | Tool::Text
                                | Tool::Marquee
                                | Tool::Lasso
                                | Tool::Wand
                                | Tool::Crop
                        )
                    })
                {
                    this.send(Command::PickUnder { point });
                } else {
                    this.dragging = true;
                    this.pointer(
                        Phase::Down,
                        event.position,
                        event.modifiers,
                        window.scale_factor(),
                    );
                }
                if event.click_count >= 2
                    && this
                        .state
                        .as_ref()
                        .is_some_and(|s| s.tool == Tool::Text && s.text_editing)
                {
                    this.send(Command::SelectTextUnit {
                        point,
                        paragraph: event.click_count >= 3,
                    });
                } else if event.click_count >= 2
                    && this
                        .state
                        .as_ref()
                        .is_some_and(|s| s.tool == Tool::Lasso && s.lasso_kind == "polygonal")
                {
                    this.send(Command::FinishSelection);
                } else if event.click_count >= 2
                    && this
                        .state
                        .as_ref()
                        .is_some_and(|s| s.tool == Tool::Move && !s.transform_active)
                {
                    this.send(Command::EditText {
                        id: None,
                        point: Some(point),
                    });
                }
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if !this.busy && this.modal.is_none() {
                        this.commit_active_fields(window, cx);
                        window.focus(&this.focus, cx);
                        this.send(Command::PickUnder {
                            point: this.local(event.position, window.scale_factor()),
                        });
                    }
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                if this.busy || this.modal.is_some() {
                    return;
                }
                let delta = event.delta.pixel_delta(px(20.));
                let scale = window.scale_factor();
                if event.modifiers.control || event.modifiers.platform {
                    this.engine.request(Operation::ZoomBy {
                        factor: (f32::from(delta.y) as f64 * 0.002).exp(),
                        point: Some(this.local(event.position, scale)),
                    });
                } else {
                    this.send(Command::Pan {
                        delta: EnginePoint::new(
                            f32::from(delta.x) as f64 * scale as f64,
                            f32::from(delta.y) as f64 * scale as f64,
                        ),
                    });
                }
                cx.stop_propagation();
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                if !this.busy && this.modal.is_none() {
                    this.commit_active_fields(window, cx);
                    this.import_paths(paths.paths().to_vec(), cx);
                }
            }))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let _ = layout.update(cx, |this, _| {
                            this.resize_canvas(bounds, window.scale_factor())
                        });
                    },
                    move |bounds, _, window, cx| {
                        let moving = paint.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = moving.update(cx, |this, cx| {
                                    let position =
                                        if this.dragging || this.bounds.contains(&event.position) {
                                            Some(this.local(event.position, window.scale_factor()))
                                        } else {
                                            None
                                        };
                                    if position != this.cursor_position
                                        && this
                                            .state
                                            .as_ref()
                                            .is_some_and(|s| s.cursor_map.brush_diameter.is_some())
                                    {
                                        cx.notify();
                                    }
                                    this.cursor_position = position;
                                    if this.cursor_modifiers.shift != event.modifiers.shift
                                        || this.cursor_modifiers.alt != event.modifiers.alt
                                        || this.cursor_modifiers.control != event.modifiers.control
                                        || (event.modifiers.alt
                                            && this.list_pointer != Some(event.position))
                                    {
                                        cx.notify();
                                    }
                                    this.cursor_modifiers = picsie_core::editor::Modifiers {
                                        shift: event.modifiers.shift,
                                        alt: event.modifiers.alt,
                                        control: event.modifiers.control,
                                        meta: event.modifiers.platform,
                                    };
                                    this.refresh_cursor(cx);
                                    if this.dragging
                                        || this.state.as_ref().is_some_and(|s| s.selection_draft)
                                    {
                                        this.pointer(
                                            Phase::Move,
                                            event.position,
                                            event.modifiers,
                                            window.scale_factor(),
                                        );
                                    }
                                    if this.sampling_color {
                                        this.sample_point = Some(event.position);
                                        this.sample_canvas(event.position, window.scale_factor());
                                        cx.notify();
                                    }
                                    if let Some((origin, position)) = this.color_panel_drag {
                                        let viewport = window.viewport_size();
                                        this.color_position = [
                                            (position[0] + f32::from(event.position.x - origin.x))
                                                .clamp(
                                                    0.,
                                                    (f32::from(viewport.width)
                                                        - dialogs::COLOR_PANEL_WIDTH)
                                                        .max(0.),
                                                ),
                                            (position[1] + f32::from(event.position.y - origin.y))
                                                .clamp(
                                                    0.,
                                                    (f32::from(viewport.height)
                                                        - dialogs::COLOR_PANEL_HEIGHT)
                                                        .max(0.),
                                                ),
                                        ];
                                        cx.notify();
                                    }
                                    this.list_pointer = Some(event.position);
                                    if this.layer_drag.is_some() || this.visibility_swiping {
                                        this.update_layer_drag(event.position, cx);
                                        this.update_visibility_swipe(event.position);
                                    }
                                    if let Some((start, width)) = this.panel_drag {
                                        this.layers_width = (width + start
                                            - f32::from(event.position.x))
                                        .round()
                                        .clamp(202., 352.);
                                        cx.notify();
                                    }
                                    this.color_drag(event.position, false, window, cx);
                                });
                            }
                        });
                        let ending = paint.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture && event.button == MouseButton::Left
                            {
                                let _ = ending.update(cx, |this, cx| {
                                    if this.dragging {
                                        this.dragging = false;
                                        cx.notify();
                                        this.pointer(
                                            Phase::Up,
                                            event.position,
                                            event.modifiers,
                                            window.scale_factor(),
                                        );
                                    }
                                    if this.guide_dragging
                                        || this.state.as_ref().is_some_and(|s| s.guide_drag_active)
                                    {
                                        this.guide_dragging = false;
                                        this.send(Command::FinishGuide {
                                            delete: this.over_ruler(event.position),
                                        });
                                    }
                                    if this.sampling_color {
                                        this.sample_canvas(event.position, window.scale_factor());
                                        this.sampling_color = false;
                                        this.sample_point = None;
                                        if matches!(this.modal, Some(Modal::Color(_))) {
                                            window.focus(&this.modal_focus, cx);
                                        }
                                        cx.notify();
                                    }
                                    if this.color_panel_drag.take().is_some() {
                                        crate::preferences::update(|p| {
                                            p.color_picker_position = Some(this.color_position)
                                        });
                                    }
                                    this.finish_layer_drag(cx);
                                    if this.visibility_swiping {
                                        this.visibility_swiping = false;
                                        this.send(Command::EndVisibilitySwipe);
                                    }
                                    this.list_autoscroll = None;
                                    if this.panel_drag.take().is_some() {
                                        crate::preferences::update(|p| {
                                            p.layers_width = Some(this.layers_width)
                                        });
                                        cx.notify();
                                    }
                                    this.color_drag(event.position, true, window, cx);
                                });
                            }
                        });
                        if let Some(image) = image {
                            if let Err(error) = window.paint_image(
                                bounds,
                                bounds,
                                Corners::default(),
                                image,
                                0,
                                false,
                            ) {
                                eprintln!("Canvas upload: {error}");
                            }
                        }
                        if let Some(this) = paint.upgrade() {
                            this.read(cx).paint_selection_outline(bounds, window);
                            this.read(cx).paint_selection_badge(bounds, window);
                            this.read(cx).paint_brush_outline(bounds, window);
                            this.read(cx).paint_sample_ring(window);
                        }
                    },
                )
                .size_full(),
            )
            .children(self.inline_text(cx))
    }
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open_palette == Some("blend-menu") {
            match event.keystroke.key.as_str() {
                "up" | "down" => {
                    self.blend_index = (self.blend_index
                        + if event.keystroke.key == "down" { 1 } else { 23 })
                        % 24;
                    self.preview_blend(self.blend_index);
                    self.blend_scroll.scroll_to_item(self.blend_index);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                "escape" => {
                    self.close_blend(window, cx);
                    cx.stop_propagation();
                    return;
                }
                "enter" => {
                    self.commit_blend(window, cx);
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }
        if self.rename_layer.is_some() && event.keystroke.key == "escape" {
            self.finish_rename(false, window, cx);
            cx.stop_propagation();
            return;
        }
        if self.busy {
            return;
        }
        if matches!(self.modal, Some(Modal::Color(_))) {
            if event.keystroke.key == "escape" {
                self.cancel_modal(window, cx);
                cx.stop_propagation();
            } else if event.keystroke.key == "enter" && window.focused_input(cx).is_none() {
                self.apply_modal(window, cx);
                cx.stop_propagation();
            }
            return;
        }
        if self.modal.is_some() {
            return;
        }
        let key = event.keystroke.key.to_lowercase();
        let m = event.keystroke.modifiers;
        if key == "escape" && self.state.as_ref().is_some_and(|s| s.text_editing) {
            self.send(Command::CancelText);
            window.focus(&self.focus, cx);
            cx.stop_propagation();
            return;
        }
        if window.focused_input(cx).is_some()
            && !((m.control || m.platform) && matches!(key.as_str(), "n" | "o" | "s" | "w" | "q"))
        {
            return;
        }
        if !m.alt
            && !(m.control || m.platform)
            && (matches!(key.as_str(), "+" | "_") || (m.shift && matches!(key.as_str(), "=" | "-")))
        {
            self.send(Command::CycleBlendMode {
                forward: matches!(key.as_str(), "+" | "="),
            });
            cx.stop_propagation();
            return;
        }
        // Dropdowns and sliders own their navigation keys. Only the canvas/layer
        // focus handles tool shortcuts and nudges; file shortcuts remain global.
        if !(m.control || m.platform) && !self.focus.is_focused(window) {
            return;
        }
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let action = if m.control || m.platform {
            match key.as_str() {
                "backspace" | "delete" => Some(Action::Command(Command::FillBackground)),
                "a" => Some(Action::Command(
                    if matches!(state.tool, Tool::Marquee | Tool::Lasso | Tool::Wand) {
                        Command::SelectAllPixels
                    } else {
                        Command::SelectAll
                    },
                )),
                "z" => Some(Action::Command(if m.shift {
                    Command::Redo
                } else {
                    Command::Undo
                })),
                "y" => Some(Action::Command(Command::Redo)),
                "n" => Some(if m.shift {
                    Action::Command(Command::AddPaintLayer)
                } else {
                    Action::New
                }),
                "o" => Some(Action::File(if m.shift {
                    FileAction::Import
                } else {
                    FileAction::Open
                })),
                "s" => Some(Action::File(if m.alt {
                    if m.shift {
                        FileAction::ExportJpeg
                    } else {
                        FileAction::ExportPng
                    }
                } else if m.shift {
                    FileAction::SaveAs
                } else {
                    FileAction::Save
                })),
                "w" => Some(Action::Close),
                "q" => {
                    menus::request_quit(window, cx);
                    cx.stop_propagation();
                    return;
                }
                "c" if !m.alt => Some(Action::Copy {
                    merged: m.shift,
                    cut: false,
                }),
                "x" => Some(Action::Copy {
                    merged: false,
                    cut: true,
                }),
                "v" => Some(Action::Paste),
                "j" => Some(Action::Command(Command::LayerViaCopy)),
                "e" => Some(Action::Command(Command::MergeLayers)),
                "t" => Some(Action::Command(Command::BeginTransform)),
                "i" if m.alt => Some(Action::ImageSize),
                "r" => Some(Action::ViewOption("rulers")),
                "'" => Some(Action::ViewOption(if m.shift { "grid" } else { "guides" })),
                ";" => Some(Action::ViewOption("guides")),
                "left" | "right" | "up" | "down" => {
                    let step = if m.shift { 10. } else { 1. };
                    Some(Action::Command(Command::MovePixels {
                        delta: EnginePoint::new(
                            if key == "left" {
                                -step
                            } else if key == "right" {
                                step
                            } else {
                                0.
                            },
                            if key == "up" {
                                -step
                            } else if key == "down" {
                                step
                            } else {
                                0.
                            },
                        ),
                        duplicate: m.alt,
                    }))
                }
                "c" if m.alt => Some(Action::CanvasSize),
                "g" if m.alt => Some(Action::Command(Command::ToggleClippingMask)),
                "i" if m.shift => Some(Action::Command(Command::InvertSelection)),
                "d" => Some(Action::Command(Command::DeselectPixels)),
                "0" => Some(Action::Command(Command::Fit)),
                "1" => Some(Action::Command(Command::Zoom {
                    zoom: 1.,
                    point: None,
                })),
                "=" | "+" | "-" => {
                    self.engine.request(Operation::ZoomBy {
                        factor: if key == "-" { 0.8 } else { 1.25 },
                        point: None,
                    });
                    cx.stop_propagation();
                    return;
                }
                "[" | "]" => Some(Action::Command(Command::Reorder {
                    direction: if key == "[" { -1 } else { 1 },
                })),
                _ => None,
            }
        } else if m.alt && (key == "backspace" || key == "delete") {
            Some(Action::Command(Command::FillSelection))
        } else if !m.alt {
            if let Some((tool, _, _, _)) = TOOLS
                .iter()
                .find(|(_, _, _, shortcut)| shortcut.eq_ignore_ascii_case(&key))
            {
                Some(Action::Command(Command::SetTool { tool: *tool }))
            } else {
                match key.as_str() {
                    "escape" => {
                        self.dragging = false;
                        self.layer_drag = None;
                        Some(Action::Command(if state.transform_active {
                            Command::CancelTransform
                        } else {
                            match state.tool {
                                Tool::Crop => Command::CancelCrop,
                                Tool::Marquee | Tool::Lasso => Command::CancelPixelSelection,
                                _ => Command::CancelGesture,
                            }
                        }))
                    }
                    "enter" if state.transform_active => {
                        Some(Action::Command(Command::CommitTransform))
                    }
                    "enter" if state.selection_draft => {
                        Some(Action::Command(Command::FinishSelection))
                    }
                    "backspace" if state.selection_draft => {
                        Some(Action::Command(Command::RemoveSelectionPoint))
                    }
                    "enter" if state.tool == Tool::Crop => {
                        Some(Action::Command(Command::CommitCrop))
                    }
                    "x" => Some(Action::Command(Command::SwapPaletteColors)),
                    "d" => Some(Action::Command(Command::ResetPaletteColors)),
                    "delete" | "backspace" => Some(Action::Command(if state.has_pixel_selection {
                        Command::ClearSelectedPixels
                    } else if state.paint_target == "mask" {
                        Command::RemoveMask
                    } else {
                        Command::Remove
                    })),
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" if !m.shift => {
                        Some(Action::Command(Command::TypeOpacityDigit {
                            digit: key.parse().unwrap(),
                        }))
                    }
                    // GPUI consumes Shift for printable symbols on Linux: `}`
                    // is already the shifted form even when modifiers.shift is false.
                    "{" | "}" => Some(Action::Command(Command::StepBrushHardness {
                        increase: key == "}",
                    })),
                    "[" | "]" if m.shift => Some(Action::Command(Command::StepBrushHardness {
                        increase: matches!(key.as_str(), "]" | "}"),
                    })),
                    "[" | "]" if matches!(state.tool, Tool::Brush | Tool::Eraser) => {
                        self.engine.request(Operation::BrushStep(if key == "[" {
                            -5.
                        } else {
                            5.
                        }));
                        cx.stop_propagation();
                        return;
                    }
                    "left" | "right" | "up" | "down" => {
                        let step = if m.shift { 10. } else { 1. };
                        let delta = EnginePoint::new(
                            if key == "left" {
                                -step
                            } else if key == "right" {
                                step
                            } else {
                                0.
                            },
                            if key == "up" {
                                -step
                            } else if key == "down" {
                                step
                            } else {
                                0.
                            },
                        );
                        Some(Action::Command(
                            if !state.transform_active
                                && matches!(state.tool, Tool::Marquee | Tool::Lasso)
                                && state.has_pixel_selection
                            {
                                Command::MoveSelection { delta }
                            } else {
                                Command::Nudge { delta }
                            },
                        ))
                    }
                    _ => None,
                }
            }
        } else {
            None
        };
        if let Some(action) = action {
            self.act(action, window, cx);
            cx.stop_propagation();
        }
    }
    pub(super) fn layer_down(
        &mut self,
        id: String,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.commit_active_fields(window, cx);
        self.finish_rename(true, window, cx);
        window.focus(&self.focus, cx);
        let command = event.modifiers.control || event.modifiers.platform;
        if event.modifiers.alt && !command {
            let clipping = self
                .probes
                .lock()
                .unwrap()
                .get(&format!("layer-{id}"))
                .is_some_and(|r| f32::from(event.position.y) >= r[1] + r[3] - 10.);
            if clipping
                && self
                    .state
                    .as_ref()
                    .and_then(|s| s.layer(&id))
                    .is_some_and(|l| l.kind() != "group")
            {
                self.send(Command::ToggleClippingFor { id });
                cx.stop_propagation();
                return;
            }
        }
        let mode = if event.modifiers.shift {
            SelectionMode::Range
        } else if command {
            SelectionMode::Toggle
        } else {
            SelectionMode::Replace
        };
        let already = self.state.as_ref().is_some_and(|s| s.is_selected(&id));
        if mode != SelectionMode::Replace || !already {
            self.send(Command::Select {
                id: Some(id.clone()),
                mode,
            });
        }
        self.layer_drag = Some(LayerDrag {
            origin: event.position,
            id,
            moved: false,
            copy: event.modifiers.alt,
            mask: false,
            select_on_click: mode == SelectionMode::Replace && already,
            destination: None,
        });
        self.list_pointer = Some(event.position);
        self.start_list_autoscroll(window, cx);
        cx.stop_propagation();
        cx.notify();
    }
    pub(super) fn update_layer_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.layer_drag.as_mut() else {
            return;
        };
        let dx = f32::from(position.x - drag.origin.x);
        let dy = f32::from(position.y - drag.origin.y);
        drag.moved |= dx * dx + dy * dy >= 9.;
        if !drag.moved {
            return;
        }
        drag.destination = None;
        let bounds = self.probes.lock().unwrap();
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let x = f32::from(position.x);
        let y = if let Some([_, top, _, height]) = bounds.get("layer-list") {
            f32::from(position.y).clamp(*top + 1., *top + *height - 1.)
        } else {
            f32::from(position.y)
        };
        for row in state.layer_rows.iter() {
            if let Some(&[left, top, width, height]) = bounds.get(&format!("layer-{}", row.id)) {
                let eligible = if drag.mask {
                    row.id != drag.id
                        && state
                            .layer(&row.id)
                            .is_some_and(|l| l.kind() != "group" && !l.locked)
                } else {
                    drag.copy || !state.is_selected(&row.id)
                };
                if x >= left && x < left + width && y >= top && y < top + height && eligible {
                    let into = drag.mask
                        || (state.layer(&row.id).is_some_and(|l| l.kind() == "group")
                            && y - top > 10.
                            && y - top < height - 10.);
                    drag.destination = Some((
                        row.id.clone(),
                        if into {
                            "into"
                        } else if y - top < height / 2. {
                            "above"
                        } else {
                            "below"
                        },
                    ));
                    break;
                }
            }
        }
        cx.notify();
    }
    pub(super) fn finish_layer_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.layer_drag.take() {
            if drag.moved
                && let Some((id, side)) = drag.destination
            {
                let side_enum = if side == "above" {
                    Side::Above
                } else {
                    Side::Below
                };
                self.send(if drag.mask {
                    Command::CopyMask {
                        source_id: drag.id,
                        target_id: id,
                    }
                } else if drag.copy {
                    Command::DuplicateTo {
                        target_id: id,
                        side: side_enum,
                        into: side == "into",
                    }
                } else if side == "into" {
                    Command::MoveToGroup {
                        parent_id: Some(id),
                    }
                } else {
                    Command::ReorderTo {
                        target_id: id,
                        side: side_enum,
                    }
                });
            } else if !drag.moved && drag.select_on_click {
                self.send(Command::Select {
                    id: Some(drag.id),
                    mode: SelectionMode::Replace,
                });
            }
            cx.notify();
        }
    }
}

impl Desktop {
    pub(super) fn is_sampling_tool(&self, modifiers: gpui_kit::Modifiers) -> bool {
        matches!(self.modal, Some(Modal::Color(_)))
            || self.state.as_ref().is_some_and(|s| {
                s.tool == Tool::Eyedropper
                    || (modifiers.alt && matches!(s.tool, Tool::Brush | Tool::Eraser))
            })
    }
    fn sample_canvas(&mut self, point: Point<Pixels>, scale: f32) {
        self.engine.request(Operation::Sample {
            point: self.local(point, scale),
            picker: matches!(self.modal, Some(Modal::Color(_))),
        });
    }
}
