//! InlineTextEditor adapter, Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
//! Kit owns typing/IME/history; the engine paints glyphs and resolves pointer/caret geometry.
use super::*;
impl Desktop {
    pub(super) fn text_update(&mut self, patch: picsie_core::text::TextPatch) {
        self.reset_caret();
        self.send(Command::UpdateText { patch });
    }
    fn text_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.reset_caret();
        use picsie_core::text::{TextNavigation, TextPatch};
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.to_lowercase();
        if key == "escape" {
            self.send(Command::CancelText);
            window.focus(&self.focus, cx);
            cx.stop_propagation();
        } else if !modifiers.control && !modifiers.platform {
            if modifiers.alt && matches!(key.as_str(), "left" | "right" | "up" | "down") {
                let Some(state) = &self.state else {
                    return;
                };
                let layout = state.current_text.text_layout.clone().unwrap_or_default();
                let step = if modifiers.shift { 10. } else { 1. };
                self.text_update(if key == "left" || key == "right" {
                    TextPatch {
                        tracking: Some(
                            (layout.tracking + if key == "left" { -step } else { step })
                                .clamp(-100., 1000.),
                        ),
                        ..Default::default()
                    }
                } else {
                    let leading = if layout.leading > 0. {
                        layout.leading
                    } else {
                        state.current_text.content["fontSize"]
                            .as_f64()
                            .unwrap_or(72.)
                            * 1.2
                    };
                    TextPatch {
                        leading: Some(
                            (leading + if key == "up" { -step } else { step }).clamp(1., 5000.),
                        ),
                        ..Default::default()
                    }
                });
                cx.stop_propagation();
            } else if !modifiers.alt {
                let direction = match key.as_str() {
                    "up" => Some(TextNavigation::Up),
                    "down" => Some(TextNavigation::Down),
                    "home" => Some(TextNavigation::LineStart),
                    "end" => Some(TextNavigation::LineEnd),
                    _ => None,
                };
                if let Some(direction) = direction {
                    self.send(Command::MoveTextCaret {
                        direction,
                        extend: modifiers.shift,
                    });
                    cx.stop_propagation();
                }
            }
        }
    }
    pub(super) fn inline_text(&self, cx: &Context<Self>) -> Option<Stateful<Div>> {
        let state = self.state.as_ref()?;
        if !state.text_editing {
            return None;
        }
        let caret = state.text_caret?;
        let v = &state.viewport;
        let scale = self.display_scale;
        let x =
            ((v.width - state.document.width as f64 * v.zoom) / 2. + v.pan.x + caret.x * v.zoom)
                / scale as f64;
        let y =
            ((v.height - state.document.height as f64 * v.zoom) / 2. + v.pan.y + caret.y * v.zoom)
                / scale as f64;
        // A native text input at the actual caret supplies keyboard/IME/clipboard editing.
        // Its pixels are hidden: Rust paints the source font, tracking, selection and transformed caret.
        Some(
            self.probe(
                "text-editor",
                div()
                    .size_full()
                    .opacity(0.)
                    .capture_key_down(cx.listener(Self::text_key))
                    .child(
                        Textarea::new(&self.text)
                            .appearance(false)
                            .bordered(false)
                            .h(px(18.))
                            .w(px(1.))
                            .aria_label("Canvas text editor"),
                    ),
            )
            .absolute()
            .key_context("PicsieCanvasText")
            .left(px(x as f32))
            .top(px(y as f32))
            .w(px(1.))
            .h(px(18.)),
        )
    }
}
