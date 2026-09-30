//! Shared pointer/field feedback, following Compositor ContentView ArrowStepping,
//! LayerAppearanceControls and InlineTextEditor (609dbeae, MIT Wonder Assembly LLC).
use super::*;
use picsie_core::feedback::CursorHint;
use std::time::Duration;

impl Desktop {
    pub(super) fn paint_brush_outline(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        if self.busy || self.modal.is_some() || self.sampling_color || self.cursor_modifiers.alt {
            return;
        }
        let Some(diameter) = self
            .state
            .as_ref()
            .and_then(|s| s.cursor_map.brush_diameter)
        else {
            return;
        };
        let Some(point) = self.cursor_position else {
            return;
        };
        if diameter < 3. {
            return;
        }
        let radius = px(diameter as f32 / 2.);
        let origin = gpui_kit::point(
            bounds.origin.x + px(point.x as f32 / self.display_scale) - radius,
            bounds.origin.y + px(point.y as f32 / self.display_scale) - radius,
        );
        let circle = Bounds::new(origin, size(radius * 2., radius * 2.));
        // BrushCursorOverlay: white 2.5-point outline underneath a black one-point outline.
        window.paint_quad(quad(
            circle,
            radius,
            rgba(0x00000000),
            px(2.5),
            rgb(0xffffff),
            BorderStyle::Solid,
        ));
        window.paint_quad(quad(
            circle,
            radius,
            rgba(0x00000000),
            px(1.),
            rgb(0x000000),
            BorderStyle::Solid,
        ));
    }
    pub(super) fn refresh_cursor(&mut self, cx: &mut Context<Self>) {
        let hint = self
            .state
            .as_ref()
            .map(|s| {
                self.cursor_position
                    .map(|p| s.cursor_map.at_view(p, self.cursor_modifiers))
                    .unwrap_or(s.cursor_map.default)
            })
            .unwrap_or(CursorHint::Arrow);
        if hint != self.cursor_hint {
            self.cursor_hint = hint;
            cx.notify();
        }
    }
    pub(super) fn canvas_cursor(&self) -> CursorStyle {
        if self.busy {
            return CursorStyle::Arrow;
        }
        if matches!(self.modal, Some(Modal::Color(_)))
            || self.sampling_color
            || (self.cursor_modifiers.alt
                && self
                    .state
                    .as_ref()
                    .is_some_and(|s| matches!(s.tool, Tool::Brush | Tool::Eraser)))
        {
            return CursorStyle::Crosshair;
        }
        if self.modal.is_some() {
            return CursorStyle::Arrow;
        }
        match self.cursor_hint {
            CursorHint::Arrow => CursorStyle::Arrow,
            CursorHint::Text => CursorStyle::IBeam,
            CursorHint::Crosshair | CursorHint::Rotate => CursorStyle::Crosshair,
            CursorHint::Move => CursorStyle::ClosedHand,
            CursorHint::Copy => CursorStyle::DragCopy,
            CursorHint::Hand if self.dragging => CursorStyle::ClosedHand,
            CursorHint::Hand => CursorStyle::OpenHand,
            CursorHint::ResizeHorizontal => CursorStyle::ResizeLeftRight,
            CursorHint::ResizeVertical => CursorStyle::ResizeUpDown,
            CursorHint::ResizeDiagonalDown => CursorStyle::ResizeUpLeftDownRight,
            CursorHint::ResizeDiagonalUp => CursorStyle::ResizeUpRightDownLeft,
        }
    }
    pub(super) fn reset_caret(&mut self) {
        self.caret_last_input = Instant::now();
        if self
            .state
            .as_ref()
            .is_some_and(|s| s.text_editing && !s.text_caret_visible)
        {
            self.send(Command::SetTextCaretVisible { visible: true });
        }
    }
    pub(super) fn sync_caret_blink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.state.as_ref().is_some_and(|s| s.text_editing);
        if !active {
            self.caret_blink = None;
            return;
        }
        if self.caret_blink.is_some() {
            return;
        }
        self.reset_caret();
        self.caret_blink = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let result = this.update_in(cx, |this, window, cx| {
                    let Some(s) = &this.state else {
                        return false;
                    };
                    if !s.text_editing {
                        return false;
                    }
                    let focused = this.modal.is_none()
                        && window.is_window_active()
                        && this.text.read(cx).focus_handle(cx).is_focused(window);
                    let selected = s.text_selection.anchor != s.text_selection.head;
                    let visible = focused
                        && !selected
                        && (this.caret_last_input.elapsed() < Duration::from_millis(500)
                            || !s.text_caret_visible);
                    if visible != s.text_caret_visible {
                        this.send(Command::SetTextCaretVisible { visible });
                    }
                    true
                });
                if !matches!(result, Ok(true)) {
                    break;
                }
            }
        }));
    }
    pub(super) fn numeric_field(
        &self,
        key: &'static str,
        element: impl IntoElement,
    ) -> Stateful<Div> {
        self.probe(format!("field-{key}"), element)
            .when(numeric_limits(key).is_some(), |d| {
                d.key_context("PicsieNumeric").capture_key_down({
                    // The handler updates the form first, so repeated keys never read a stale worker snapshot.
                    // UI-side range checks apply only to transient form values; Rust still validates commands.
                    let entity = self.ui_entity.clone();
                    move |event, window, cx| {
                        let _ =
                            entity.update(cx, |this, cx| this.step_numeric(key, event, window, cx));
                    }
                })
            })
    }
    pub(super) fn step_numeric(
        &mut self,
        key: &'static str,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.alt
        {
            return;
        }
        let direction = match event.keystroke.key.as_str() {
            "up" => 1.,
            "down" => -1.,
            _ => return,
        };
        let Some((min, max)) = numeric_limits(key) else {
            return;
        };
        let current = number(&self.field_value(key, cx)).unwrap_or_else(|_| {
            if key == "leading" {
                self.state
                    .as_ref()
                    .map(|s| s.current_text.content["fontSize"].as_f64().unwrap_or(72.) * 1.2)
                    .unwrap_or(86.4)
            } else {
                min.max(0.)
            }
        });
        let value = (current
            + direction
                * if event.keystroke.modifiers.shift {
                    10.
                } else {
                    1.
                })
        .clamp(min, max);
        if matches!(key, "opacity" | "brightness" | "saturation" | "blur")
            && self.numeric_edit != Some(key)
        {
            self.send(Command::BeginPropertyEdit {
                label: format!("Edit {key}"),
            });
            self.numeric_edit = Some(key);
        }
        self.fields[key].update(cx, |input, cx| input.set_value(decimal(value), window, cx));
        self.edited_fields.insert(key);
        self.commit_field(key, window, cx);
        cx.stop_propagation();
    }
}
fn numeric_limits(key: &str) -> Option<(f64, f64)> {
    Some(match key {
        "x" | "y" => (-100_000., 100_000.),
        "rotation" => (-360., 360.),
        "width" | "height" => (1., 819_200.),
        "font-size" => (1., 1000.),
        "tracking" => (-100., 1000.),
        "leading" => (0., 5000.),
        "opacity" | "brush-opacity" | "hardness" | "smoothing" => (0., 100.),
        "brightness" | "saturation" => (0., 300.),
        "blur" => (0., 100.),
        "brush-size" => (1., 2000.),
        "selection-amount" => (1., 500.),
        "feather" => (1., 250.),
        "wand-tolerance" | "color-r" | "color-g" | "color-b" => (0., 255.),
        _ => return None,
    })
}

impl Desktop {
    /// SampleRingOverlay.swift: 116pt ring, gray 24pt rim, color 16pt halves.
    pub(super) fn paint_sample_ring(&self, window: &mut Window) {
        if !self.sampling_color || !self.shows_sample_ring {
            return;
        }
        let Some(at) = self.sample_point else {
            return;
        };
        let origin = point(at.x - px(58.), at.y - px(58.));
        let rim = Bounds::new(
            point(origin.x + px(3.), origin.y + px(3.)),
            size(px(110.), px(110.)),
        );
        window.paint_quad(quad(
            rim,
            px(55.),
            rgba(0),
            px(24.),
            rgb(0x737373),
            BorderStyle::Solid,
        ));
        let ring = Bounds::new(
            point(origin.x + px(7.), origin.y + px(7.)),
            size(px(102.), px(102.)),
        );
        for (color, y) in [(&self.sample_current, 0.), (&self.sample_original, 58.)] {
            window.with_content_mask(
                Some(ContentMask {
                    bounds: Bounds::new(point(origin.x, origin.y + px(y)), size(px(116.), px(58.))),
                }),
                |window| {
                    window.paint_quad(quad(
                        ring,
                        px(51.),
                        rgba(0),
                        px(16.),
                        hex_color(color),
                        BorderStyle::Solid,
                    ));
                },
            );
        }
    }
}
