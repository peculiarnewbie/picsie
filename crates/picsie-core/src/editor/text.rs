//! TypeTool / InlineTextEditor draft and box rules, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. History previews retain drafts in the native model.
use super::*;
use crate::text::{self, TextLayout, TextPatch};
pub(super) struct TextSession {
    pub id: String,
    pub new: bool,
}
pub(super) fn defaults() -> Layer {
    let mut layer = Layer::new(
        "Text",
        16,
        16,
        Content::Text {
            text: String::new(),
            font_size: 72.,
            font_family: FontFamily::SansSerif,
            color: "#000000".into(),
        },
    );
    layer.text_layout = Some(TextLayout {
        point: true,
        ..Default::default()
    });
    layer
}
impl Editor {
    pub(super) fn move_text_caret(&mut self, direction: text::TextNavigation, extend: bool) {
        if !self.text_editing() {
            return;
        }
        let Some(layer) = self.selected() else {
            return;
        };
        let (head, upstream, x) = text::navigate(
            layer,
            self.text_selection.1,
            self.text_upstream,
            direction,
            self.text_vertical_x,
        );
        self.text_selection = (if extend { self.text_selection.0 } else { head }, head);
        self.text_upstream = upstream;
        self.text_vertical_x = x;
    }
    pub fn text_editing(&self) -> bool {
        self.text_session.is_some()
    }
    pub fn current_text(&self) -> &Layer {
        self.selected()
            .filter(|l| matches!(l.content.as_ref(), Content::Text { .. }))
            .unwrap_or(&self.text_defaults)
    }
    pub(super) fn begin_text_edit(&mut self, id: String) -> Result<()> {
        if self.text_session.as_ref().is_some_and(|s| s.id == id) {
            self.text_edit_requests += 1;
            return Ok(());
        }
        self.finish_text()?;
        self.finish_gesture()?;
        let Some(layer) = self
            .history
            .document
            .layers
            .iter()
            .find(|l| l.id == id && !l.locked && matches!(l.content.as_ref(), Content::Text { .. }))
            .cloned()
        else {
            return Ok(());
        };
        self.single_selection(Some(id.clone()));
        self.begin_edit("Edit Text");
        self.text_session = Some(TextSession { id, new: false });
        let Content::Text { text, .. } = layer.content.as_ref() else {
            unreachable!()
        };
        self.text_selection = (text.len(), text.len());
        self.text_upstream = false;
        self.text_vertical_x = None;
        self.tool = Tool::Text;
        self.text_edit_requests += 1;
        Ok(())
    }
    pub(super) fn begin_new_text(&mut self, origin: Point, size: Option<(u32, u32)>) -> Result<()> {
        self.finish_text()?;
        ensure!(
            self.history.document.layers.len() < crate::model::MAX_LAYERS,
            "The editor supports up to 10,000 layers"
        );
        let mut layer = self.text_defaults.clone();
        layer.id = id();
        layer.x = origin.x;
        layer.y = origin.y;
        layer.name = "Text".into();
        if let Content::Text { color, text, .. } = Arc::make_mut(&mut layer.content) {
            *color = self.color.clone();
            text.clear();
        }
        layer.text_layout.as_mut().unwrap().point = size.is_none();
        if let Some((w, h)) = size {
            layer.width = w.max(16);
            layer.height = h.max(16);
        } else {
            text::grow_point(&mut layer)?;
        }
        layer.validate()?;
        self.begin_edit("New Text Layer");
        self.history.preview(self.inserted(vec![layer.clone()]));
        self.single_selection(Some(layer.id.clone()));
        self.text_session = Some(TextSession {
            id: layer.id,
            new: true,
        });
        self.text_selection = (0, 0);
        self.text_upstream = false;
        self.text_vertical_x = None;
        self.text_edit_requests += 1;
        self.tool = Tool::Text;
        Ok(())
    }
    pub fn finish_text(&mut self) -> Result<()> {
        let Some(session) = self.text_session.take() else {
            return Ok(());
        };
        self.gesture = None;
        let Some(layer) = self
            .history
            .document
            .layers
            .iter()
            .find(|l| l.id == session.id)
            .cloned()
        else {
            let selection = self.history.cancel();
            self.restore_selection(selection);
            return Ok(());
        };
        let Content::Text { text, .. } = layer.content.as_ref() else {
            unreachable!()
        };
        if session.new && text.trim().is_empty() {
            let selection = self.history.cancel();
            self.restore_selection(selection);
            return Ok(());
        }
        self.text_defaults = layer.clone();
        self.text_defaults
            .text_layout
            .get_or_insert_with(Default::default);
        if session.new {
            let mut doc = self.history.document.clone();
            let name = text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(40)
                .collect();
            let mut named = layer;
            named.name = name;
            doc.replace(named);
            self.history.preview(doc);
        }
        self.end_edit();
        Ok(())
    }
    pub fn cancel_text(&mut self) {
        if self.text_session.take().is_some() {
            self.gesture = None;
            let selection = self.history.cancel();
            self.restore_selection(selection);
        }
    }
    pub(super) fn update_text(&mut self, patch: TextPatch) -> Result<()> {
        if self.text_session.is_none()
            && self
                .selected()
                .is_some_and(|l| matches!(l.content.as_ref(), Content::Text { .. }))
        {
            self.begin_text_edit(self.selected_id().unwrap().to_owned())?;
        }
        let editing = self.text_session.is_some();
        let mut layer = if editing {
            self.selected().unwrap().clone()
        } else {
            self.text_defaults.clone()
        };
        let layout = layer.text_layout.get_or_insert_with(|| TextLayout {
            font_name: match layer.content.as_ref() {
                Content::Text {
                    font_family: FontFamily::Serif,
                    ..
                } => "serif".into(),
                Content::Text {
                    font_family: FontFamily::Monospace,
                    ..
                } => "monospace".into(),
                _ => "sans-serif".into(),
            },
            ..Default::default()
        });
        if let Some(v) = patch.font_name {
            layout.font_name = v;
        }
        if let Some(v) = patch.alignment {
            layout.alignment = v;
        }
        if let Some(v) = patch.tracking {
            layout.tracking = v;
        }
        if let Some(v) = patch.leading {
            layout.leading = v;
        }
        if let Content::Text {
            text,
            font_size,
            font_family,
            color,
        } = Arc::make_mut(&mut layer.content)
        {
            if let Some(v) = patch.text {
                *text = v;
            }
            if let Some(v) = patch.font_size {
                *font_size = v;
            }
            if let Some(v) = patch.color {
                *color = v;
            }
            *font_family = match layout.font_name.as_str() {
                "serif" => FontFamily::Serif,
                "monospace" => FontFamily::Monospace,
                _ => FontFamily::SansSerif,
            };
        }
        layer.validate()?;
        text::grow_point(&mut layer)?;
        self.text_upstream = false;
        self.text_vertical_x = None;
        if editing {
            let mut doc = self.history.document.clone();
            doc.replace(layer);
            self.history.preview(doc);
        } else {
            self.text_defaults = layer;
        }
        Ok(())
    }
    pub(super) fn text_pointer(
        &mut self,
        phase: Phase,
        p: Point,
        modifiers: Modifiers,
    ) -> Result<bool> {
        if matches!(phase, Phase::Down) {
            self.text_mouse_unit = None;
            self.text_caret_visible = true;
            self.text_upstream = false;
            self.text_vertical_x = None;
        }
        if self.tool != Tool::Text {
            return Ok(false);
        }
        if phase == Phase::Down {
            if self.text_session.is_some()
                && let Some(layer) = self.selected().cloned()
            {
                if let Some(handle) =
                    geometry::HandleGeometry::new(&layer, self.viewport.zoom, false).hit(p)
                {
                    self.gesture = Some(Gesture::TextResize {
                        origin: p,
                        handle,
                        layer,
                    });
                    return Ok(true);
                }
                let at = geometry::to_local(&layer, p);
                if at.x >= 0.
                    && at.y >= 0.
                    && at.x <= layer.width as f64
                    && at.y <= layer.height as f64
                {
                    let head = text::hit(&layer, p);
                    self.text_selection = (
                        if modifiers.shift {
                            self.text_selection.0
                        } else {
                            head
                        },
                        head,
                    );
                    return Ok(true);
                }
            }
            self.finish_text()?;
            if let Some(id) = geometry::hit_layers(&self.history.document, p)
                .into_iter()
                .find(|l| matches!(l.content.as_ref(), Content::Text { .. }))
                .map(|l| l.id.clone())
            {
                self.begin_text_edit(id)?;
                let head = text::hit(self.selected().unwrap(), p);
                self.text_selection = (head, head);
            } else {
                self.gesture = Some(Gesture::TextBox {
                    origin: p,
                    current: p,
                });
            }
            return Ok(true);
        }
        if matches!(
            self.gesture,
            Some(Gesture::TextBox { .. }) | Some(Gesture::TextResize { .. })
        ) {
            return Ok(false);
        }
        if self.text_session.is_some() && matches!(phase, Phase::Move | Phase::Up) {
            if let Some(layer) = self.selected() {
                if let Some((start, end, paragraph)) = self.text_mouse_unit {
                    let (next_start, next_end) = text::unit_at(layer, p, paragraph);
                    self.text_selection = if next_start < start {
                        (end, next_start)
                    } else {
                        (start, next_end)
                    };
                } else {
                    self.text_selection.1 = text::hit(layer, p);
                }
            }
            if phase == Phase::Up {
                self.text_mouse_unit = None;
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn resize_text_box(
        &mut self,
        layer: &Layer,
        handle: &str,
        origin: Point,
        p: Point,
    ) -> Result<()> {
        let &(_, hx, hy) = geometry::HANDLES.iter().find(|h| h.0 == handle).unwrap();
        let a = layer.rotation.to_radians();
        let dx = p.x - origin.x;
        let dy = p.y - origin.y;
        let localx = (dx * a.cos() + dy * a.sin()) / layer.scale_x;
        let localy = (-dx * a.sin() + dy * a.cos()) / layer.scale_y;
        let (mut left, mut top, mut right, mut bottom) =
            (0., 0., layer.width as f64, layer.height as f64);
        if hx == 0. {
            left = localx.min(right - 16.);
        }
        if hx == 1. {
            right = (right + localx).max(left + 16.);
        }
        if hy == 0. {
            top = localy.min(bottom - 16.);
        }
        if hy == 1. {
            bottom = (bottom + localy).max(top + 16.);
        }
        let mut next = layer.clone();
        next.text_layout.get_or_insert_with(Default::default).point = false;
        next.width = (right - left).round() as u32;
        next.height = (bottom - top).round() as u32;
        let anchor = geometry::bounds_point(
            layer,
            Point::new(left / layer.width as f64, top / layer.height as f64),
        );
        let current = geometry::bounds_point(&next, Point::default());
        next.x += anchor.x - current.x;
        next.y += anchor.y - current.y;
        if next.validate().is_ok() {
            let mut doc = self.history.document.clone();
            doc.replace(next);
            self.history.preview(doc);
        }
        Ok(())
    }
    pub fn draw_text_editor(&self, canvas: &skia_safe::Canvas) {
        use skia_safe::{Color, Paint, Rect};
        if let Some(Gesture::TextBox { origin, current }) = &self.gesture {
            let o = geometry::canvas_origin(&self.history.document, &self.viewport);
            let z = self.viewport.zoom;
            let mut paint = Paint::default();
            paint
                .set_color(Color::from_rgb(117, 167, 217))
                .set_style(skia_safe::paint::Style::Stroke);
            canvas.draw_rect(
                Rect::from_xywh(
                    (o.x + origin.x.min(current.x) * z) as f32,
                    (o.y + origin.y.min(current.y) * z) as f32,
                    ((current.x - origin.x).abs() * z) as f32,
                    ((current.y - origin.y).abs() * z) as f32,
                ),
                &paint,
            );
        }
        if self.text_session.is_none() {
            return;
        }
        let Some(layer) = self.selected() else {
            return;
        };
        let Content::Text { text, .. } = layer.content.as_ref() else {
            return;
        };
        let o = geometry::canvas_origin(&self.history.document, &self.viewport);
        let z = self.viewport.zoom;
        canvas.save();
        canvas
            .translate((o.x as f32, o.y as f32))
            .scale((z as f32, z as f32));
        // InlineTextEditor mirrors the text surface, while box handles and the
        // overset marker retain their logical corner order.
        let mut bounds = layer.clone();
        bounds.flip_x = false;
        bounds.flip_y = false;
        render::transform(canvas, &bounds);
        let mut paint = Paint::default();
        paint
            .set_color(Color::from_rgb(117, 167, 217))
            .set_style(skia_safe::paint::Style::Stroke)
            .set_stroke_width((1. / z / layer.scale_x) as f32);
        canvas.draw_rect(
            Rect::from_wh(layer.width as f32, layer.height as f32),
            &paint,
        );
        paint.set_style(skia_safe::paint::Style::Fill);
        for &(_, x, y) in &geometry::HANDLES {
            let w = (6. / z / layer.scale_x) as f32;
            let h = (6. / z / layer.scale_y) as f32;
            let rect = Rect::from_xywh(
                x as f32 * layer.width as f32 - w / 2.,
                y as f32 * layer.height as f32 - h / 2.,
                w,
                h,
            );
            paint
                .set_style(skia_safe::paint::Style::Fill)
                .set_color(Color::WHITE);
            canvas.draw_rect(rect, &paint);
            paint
                .set_style(skia_safe::paint::Style::Stroke)
                .set_color(Color::from_rgb(117, 167, 217));
            canvas.draw_rect(rect, &paint);
        }
        paint.set_style(skia_safe::paint::Style::Fill);
        canvas.save();
        canvas.translate((layer.width as f32 / 2., layer.height as f32 / 2.));
        canvas.scale((
            if layer.flip_x { -1. } else { 1. },
            if layer.flip_y { -1. } else { 1. },
        ));
        canvas.translate((-(layer.width as f32) / 2., -(layer.height as f32) / 2.));
        canvas.clip_rect(
            Rect::from_xywh(
                12.,
                12.,
                (layer.width as f32 - 24.).max(0.),
                (layer.height as f32 - 24.).max(0.),
            ),
            None,
            false,
        );
        let (start, end) = (
            self.text_selection
                .0
                .min(self.text_selection.1)
                .min(text.len()),
            self.text_selection
                .0
                .max(self.text_selection.1)
                .min(text.len()),
        );
        let para = text::paragraph(layer, Some((layer.width as f32 - 24.).max(1.)));
        if start != end && text.is_char_boundary(start) && text.is_char_boundary(end) {
            paint.set_color(Color::from_argb(100, 65, 140, 210));
            for b in para.get_rects_for_range(
                text::utf16_at(text, start)..text::utf16_at(text, end),
                skia_safe::textlayout::RectHeightStyle::Max,
                skia_safe::textlayout::RectWidthStyle::Tight,
            ) {
                let mut r = b.rect;
                r.offset((12., 12. + text::top_offset(&para)));
                canvas.draw_rect(r, &paint);
            }
        } else if self.text_caret_visible && text.is_char_boundary(end) {
            let mut r = text::caret_with_affinity(layer, end, self.text_upstream);
            r.right = r.left + (1.5 / z / layer.scale_x) as f32;
            paint.set_color(match layer.content.as_ref() {
                Content::Text { color, .. } => crate::render::color(color),
                _ => Color::WHITE,
            });
            canvas.draw_rect(r, &paint);
        }
        canvas.restore();
        if text::overset(layer) {
            paint.set_color(Color::BLACK);
            let x = layer.width as f32;
            let y = layer.height as f32;
            let arm = (2.5 / z / layer.scale_x) as f32;
            canvas.draw_rect(
                Rect::from_xywh(x - arm, y - 0.5 / z as f32, arm * 2., 1. / z as f32),
                &paint,
            );
            canvas.draw_rect(
                Rect::from_xywh(x - 0.5 / z as f32, y - arm, 1. / z as f32, arm * 2.),
                &paint,
            );
        }
        canvas.restore();
    }
}
