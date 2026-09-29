use super::*;
use picsie_core::{
    editor::{Modifiers as EngineModifiers, Phase, PointerSample, SelectionMode, Side},
    model::{MaskMode, Point as EnginePoint},
};
pub(super) const TOOLS: &[(Tool, &str, &str, &str)] = &[
    (Tool::Move, "move", "Move", "V"),
    (Tool::Brush, "brush", "Brush", "B"),
    (Tool::Eraser, "eraser", "Eraser", "E"),
    (Tool::Rectangle, "rectangle", "Rectangle", "U"),
    (Tool::Ellipse, "ellipse", "Ellipse", "O"),
    (Tool::Text, "text", "Text", "T"),
    (Tool::Hand, "hand", "Hand", "H"),
    (Tool::Eyedropper, "eyedropper", "Sample color", "I"),
    (Tool::Crop, "crop", "Crop", "C"),
    (Tool::Marquee, "marquee", "Marquee", "M"),
    (Tool::Lasso, "lasso", "Lasso", "L"),
];
impl Desktop {
    fn local(&self, position: Point<Pixels>, scale: f32) -> EnginePoint {
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
        let cursor = match self.state.as_ref().map(|s| s.tool) {
            Some(Tool::Hand) => CursorStyle::OpenHand,
            Some(Tool::Move) => CursorStyle::Arrow,
            _ => CursorStyle::Crosshair,
        };
        div()
            .id("image-canvas")
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x15171c))
            .cursor(cursor)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if this.busy || this.modal.is_some() {
                        return;
                    }
                    this.commit_active_fields(window, cx);
                    window.focus(&this.focus, cx);
                    let point = this.local(event.position, window.scale_factor());
                    if event.modifiers.control || event.modifiers.platform {
                        this.send(Command::PickUnder { point });
                    } else if this
                        .state
                        .as_ref()
                        .is_some_and(|s| s.tool == Tool::Eyedropper)
                    {
                        this.blocking(Operation::Sample(point));
                    } else {
                        this.dragging = true;
                        this.pointer(
                            Phase::Down,
                            event.position,
                            event.modifiers,
                            window.scale_factor(),
                        );
                    }
                    if event.click_count >= 2 {
                        this.send(Command::EditText {
                            id: None,
                            point: Some(point),
                        });
                    }
                    cx.stop_propagation();
                }),
            )
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
                    move |bounds, _, window, _cx| {
                        let moving = paint.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = moving.update(cx, |this, cx| {
                                    if this.dragging {
                                        this.pointer(
                                            Phase::Move,
                                            event.position,
                                            event.modifiers,
                                            window.scale_factor(),
                                        );
                                    }
                                    if this.layer_drag.is_some() {
                                        this.update_layer_drag(event.position, cx);
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
                                    this.finish_layer_drag(cx);
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
                    },
                )
                .size_full(),
            )
    }
    pub(super) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.modal.is_some() {
            return;
        }
        let key = event.keystroke.key.to_lowercase();
        let m = event.keystroke.modifiers;
        if window.focused_input(cx).is_some()
            && !((m.control || m.platform) && matches!(key.as_str(), "n" | "o" | "s" | "w" | "q"))
        {
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
                "a" => Some(Action::Command(
                    if matches!(state.tool, Tool::Marquee | Tool::Lasso) {
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
                "j" => Some(Action::Command(Command::Duplicate)),
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
                        Some(Action::Command(match state.tool {
                            Tool::Crop => Command::CancelCrop,
                            Tool::Marquee | Tool::Lasso => Command::CancelPixelSelection,
                            _ => Command::CancelGesture,
                        }))
                    }
                    "enter" if state.tool == Tool::Crop => {
                        Some(Action::Command(Command::CommitCrop))
                    }
                    "x" if state.paint_target == "mask" => {
                        Some(Action::Command(Command::SetMaskMode {
                            mode: if state.mask_mode == "hide" {
                                MaskMode::Reveal
                            } else {
                                MaskMode::Hide
                            },
                        }))
                    }
                    "delete" | "backspace" => Some(Action::Command(if state.has_pixel_selection {
                        Command::ClearSelectedPixels
                    } else if state.paint_target == "mask" {
                        Command::RemoveMask
                    } else {
                        Command::Remove
                    })),
                    "[" | "]" => {
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
                        Some(Action::Command(Command::Nudge {
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
                        }))
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
        window.focus(&self.focus, cx);
        let mode = if event.modifiers.shift {
            SelectionMode::Range
        } else if event.modifiers.control || event.modifiers.platform {
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
        if event.click_count >= 2 {
            self.send(Command::EditText {
                id: Some(id.clone()),
                point: None,
            });
        }
        self.layer_drag = Some(LayerDrag {
            origin: event.position,
            id,
            moved: false,
            select_on_click: mode == SelectionMode::Replace && already,
            destination: None,
        });
        cx.stop_propagation();
        cx.notify();
    }
    fn update_layer_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.layer_drag.as_mut() else {
            return;
        };
        drag.moved |= f32::from(position.y - drag.origin.y).abs() > 4.;
        if !drag.moved {
            return;
        }
        drag.destination = None;
        let bounds = self.probes.lock().unwrap();
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let x = f32::from(position.x);
        let y = f32::from(position.y);
        for row in &state.layer_rows {
            if let Some(&[left, top, width, height]) = bounds.get(&format!("layer-{}", row.id)) {
                if x >= left
                    && x < left + width
                    && y >= top
                    && y < top + height
                    && !state.is_selected(&row.id)
                {
                    let into = state.layer(&row.id).is_some_and(|l| l.kind() == "group")
                        && y - top > 10.
                        && y - top < 33.;
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
    fn finish_layer_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.layer_drag.take() {
            if drag.moved {
                if let Some((id, side)) = drag.destination {
                    self.send(if side == "into" {
                        Command::MoveToGroup {
                            parent_id: Some(id),
                        }
                    } else {
                        Command::ReorderTo {
                            target_id: id,
                            side: if side == "above" {
                                Side::Above
                            } else {
                                Side::Below
                            },
                        }
                    });
                }
            } else if drag.select_on_click {
                self.send(Command::Select {
                    id: Some(drag.id),
                    mode: SelectionMode::Replace,
                });
            }
            cx.notify();
        }
    }
}
