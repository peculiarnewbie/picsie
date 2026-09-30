//! CanvasRulers.swift / Guides View menu adaptation, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. The engine supplies all ruler pixels.
use super::*;
use picsie_core::{model::Point as EnginePoint, placement::GuideAxis};
impl Desktop {
    pub(super) fn view_option(&self, key: &str) -> bool {
        let Some(s) = &self.state else {
            return false;
        };
        let v = &s.view_options;
        match key {
            "rulers" => v.rulers,
            "guides" => v.guides,
            "grid" => v.grid,
            "lock-guides" => v.lock_guides,
            "snap" => v.snap,
            "snap-guides" => v.snap_guides,
            "snap-grid" => v.snap_grid,
            "snap-layers" => v.snap_layers,
            "snap-bounds" => v.snap_bounds,
            "auto-select" => v.auto_select,
            "show-controls" => v.show_controls,
            _ => false,
        }
    }
    pub(super) fn toggle_view_option(&mut self, key: &str) {
        let Some(s) = &self.state else {
            return;
        };
        let mut options = s.view_options.clone();
        let value = match key {
            "rulers" => &mut options.rulers,
            "guides" => &mut options.guides,
            "grid" => &mut options.grid,
            "lock-guides" => &mut options.lock_guides,
            "snap" => &mut options.snap,
            "snap-guides" => &mut options.snap_guides,
            "snap-grid" => &mut options.snap_grid,
            "snap-layers" => &mut options.snap_layers,
            "snap-bounds" => &mut options.snap_bounds,
            "auto-select" => &mut options.auto_select,
            "show-controls" => &mut options.show_controls,
            _ => return,
        };
        *value = !*value;
        crate::preferences::update(|p| p.view_options = Some(options.clone()));
        self.send(Command::SetViewOptions { options });
    }
    fn ruler(&self, axis: GuideAxis, cx: &Context<Self>) -> Stateful<Div> {
        let horizontal = axis == GuideAxis::Horizontal;
        let id = if horizontal {
            "ruler-horizontal"
        } else {
            "ruler-vertical"
        };
        let mut d = self
            .probe(id, div().relative().overflow_hidden().bg(rgb(0x333333)))
            .flex_shrink_0()
            .cursor(if horizontal {
                CursorStyle::ResizeUpDown
            } else {
                CursorStyle::ResizeLeftRight
            });
        d = if horizontal {
            d.w_full().h(px(18.))
        } else {
            d.w(px(18.)).h_full()
        };
        if let Some((_, image)) = self.thumbnails.get(if horizontal {
            "@ruler-horizontal"
        } else {
            "@ruler-vertical"
        }) {
            d = d.child(img(image.clone()).size_full());
        }
        d.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if this.busy
                    || this
                        .state
                        .as_ref()
                        .is_none_or(|s| s.view_options.lock_guides)
                {
                    return;
                }
                this.commit_active_fields(window, cx);
                window.focus(&this.focus, cx);
                this.dragging = true;
                this.guide_dragging = true;
                this.send(Command::BeginGuide {
                    axis: if horizontal {
                        GuideAxis::Horizontal
                    } else {
                        GuideAxis::Vertical
                    },
                    point: this.local(event.position, window.scale_factor()),
                });
                cx.stop_propagation();
            }),
        )
    }
    pub(super) fn canvas_workspace(&self, cx: &Context<Self>) -> Div {
        let rulers = self.view_option("rulers");
        let mut d = div().flex().flex_col().size_full().min_h_0().min_w_0();
        if rulers {
            d = d.child(
                row()
                    .gap_0()
                    .h(px(18.))
                    .flex_shrink_0()
                    .child(
                        div()
                            .w(px(18.))
                            .h(px(18.))
                            .flex_shrink_0()
                            .bg(rgb(0x333333)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.ruler(GuideAxis::Horizontal, cx)),
                    ),
            );
        }
        let mut body = row().gap_0().items_stretch().flex_1().min_h_0();
        if rulers {
            body = body.child(self.ruler(GuideAxis::Vertical, cx));
        }
        d.child(
            body.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.probe("canvas", self.canvas_view(cx)).size_full()),
            ),
        )
    }
    pub(super) fn over_ruler(&self, position: Point<Pixels>) -> bool {
        self.view_option("rulers")
            && (position.x < self.bounds.origin.x || position.y < self.bounds.origin.y)
    }
    pub(super) fn guide_at(&self, position: Point<Pixels>, scale: f32) -> bool {
        let Some(state) = &self.state else {
            return false;
        };
        if !state.view_options.guides || state.view_options.lock_guides || state.tool != Tool::Move
        {
            return false;
        }
        let point = self.local(position, scale);
        let v = &state.viewport;
        let origin = EnginePoint::new(
            (v.width - state.document.width as f64 * v.zoom) / 2. + v.pan.x,
            (v.height - state.document.height as f64 * v.zoom) / 2. + v.pan.y,
        );
        state.displayed_guides.iter().any(|g| {
            let distance = if g.axis == GuideAxis::Vertical {
                (origin.x + g.position * v.zoom - point.x).abs()
            } else {
                (origin.y + g.position * v.zoom - point.y).abs()
            };
            distance <= 5. * scale as f64
        })
    }
}
