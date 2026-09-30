//! Compositor Guides.swift / Crop.swift / TransformSnap, MIT © 2026 Wonder Assembly LLC.
use super::*;
use crate::placement::{self, CanvasGuide, GuideAxis, GuideDrag};
impl Editor {
    pub fn displayed_guides(&self) -> Vec<CanvasGuide> {
        let mut guides = self.history.document.guides.clone();
        if let Some(drag) = &self.guide_drag {
            if let Some(guide) = guides.iter_mut().find(|g| g.id == drag.guide.id) {
                *guide = drag.guide.clone();
            } else if drag.new {
                guides.push(drag.guide.clone());
            }
        }
        guides
    }
    pub fn guide_drag_active(&self) -> bool {
        self.guide_drag.is_some()
    }
    pub fn hit_guide(&self, point: Point) -> Option<CanvasGuide> {
        if !self.view_options.guides || self.view_options.lock_guides {
            return None;
        }
        self.history
            .document
            .guides
            .iter()
            .filter_map(|guide| {
                let delta = (guide.position
                    - if guide.axis == GuideAxis::Vertical {
                        point.x
                    } else {
                        point.y
                    })
                .abs();
                (delta * self.viewport.zoom <= 5. * self.display_scale).then_some((delta, guide))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, g)| g.clone())
    }
    pub(super) fn begin_guide(&mut self, axis: GuideAxis, point: Point) -> Result<()> {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "Invalid guide coordinate"
        );
        if self.view_options.lock_guides {
            return Ok(());
        }
        self.finish_gesture()?;
        self.view_options.guides = true;
        let point = geometry::to_document(&self.history.document, &self.viewport, point);
        self.guide_drag = Some(GuideDrag {
            new: true,
            guide: CanvasGuide {
                id: id(),
                axis,
                position: if axis == GuideAxis::Vertical {
                    point.x
                } else {
                    point.y
                },
            },
        });
        self.move_guide(point);
        Ok(())
    }
    fn move_guide(&mut self, point: Point) {
        let Some(drag) = &self.guide_drag else {
            return;
        };
        let axis = drag.guide.axis;
        let mut doc = self.history.document.clone();
        doc.guides.retain(|g| g.id != drag.guide.id);
        let (xs, ys) = placement::targets(&doc, &self.view_options, &[], true);
        let value = if axis == GuideAxis::Vertical {
            point.x
        } else {
            point.y
        };
        let (_, snapped) = placement::nearest(
            &[value],
            if axis == GuideAxis::Vertical {
                &xs
            } else {
                &ys
            },
            10. * self.display_scale / self.viewport.zoom,
        );
        self.guide_drag.as_mut().unwrap().guide.position =
            snapped.unwrap_or(value).clamp(-100000., 100000.);
    }
    pub(super) fn finish_guide(&mut self, delete: bool) -> Result<()> {
        let Some(drag) = self.guide_drag.take() else {
            return Ok(());
        };
        if delete && drag.new {
            return Ok(());
        }
        let mut doc = self.history.document.clone();
        if delete {
            doc.guides.retain(|g| g.id != drag.guide.id);
        } else if let Some(existing) = doc.guides.iter_mut().find(|g| g.id == drag.guide.id) {
            *existing = drag.guide;
        } else {
            doc.guides.push(drag.guide);
        }
        self.edit(
            if delete {
                "Delete Guide"
            } else if drag.new {
                "New Guide"
            } else {
                "Move Guide"
            },
            doc,
            None,
        )
    }
    pub(super) fn guide_pointer(&mut self, phase: Phase, point: Point) -> Result<bool> {
        if self.guide_drag.is_some() {
            if matches!(phase, Phase::Move | Phase::Up) {
                self.move_guide(point);
            }
            // Desktop sends FinishGuide after Up, carrying whether the release is over a ruler.
            return Ok(true);
        }
        if phase == Phase::Down
            && self.tool == Tool::Move
            && !(self.selection.ids.len() == 1
                && (self.view_options.show_controls || self.transform_active())
                && self.selected().is_some_and(|l| {
                    !l.locked
                        && l.visible
                        && geometry::hit_handle(l, point, self.viewport.zoom).is_some()
                }))
            && let Some(guide) = self.hit_guide(point)
        {
            self.finish_gesture()?;
            self.guide_drag = Some(GuideDrag { guide, new: false });
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn snap_move(&mut self, layers: &[Layer], delta: Point, disabled: bool) -> Point {
        self.snap_lines = (vec![], vec![]);
        if disabled || !self.view_options.snap || layers.is_empty() {
            return delta;
        }
        let moved = Self::moved(layers, delta);
        let boxes: Vec<_> = moved
            .iter()
            .filter(|l| !matches!(l.content.as_ref(), Content::Group))
            .map(placement::bounds)
            .collect();
        if boxes.is_empty() {
            return delta;
        }
        let left = boxes.iter().map(|b| b.0).fold(f64::INFINITY, f64::min);
        let top = boxes.iter().map(|b| b.1).fold(f64::INFINITY, f64::min);
        let right = boxes.iter().map(|b| b.2).fold(f64::NEG_INFINITY, f64::max);
        let bottom = boxes.iter().map(|b| b.3).fold(f64::NEG_INFINITY, f64::max);
        let ids: Vec<_> = layers.iter().map(|l| l.id.clone()).collect();
        let (xs, ys) = placement::targets(&self.history.document, &self.view_options, &ids, true);
        let (dx, x) = placement::nearest(
            &[left, (left + right) / 2., right],
            &xs,
            10. * self.display_scale / self.viewport.zoom,
        );
        let (dy, y) = placement::nearest(
            &[top, (top + bottom) / 2., bottom],
            &ys,
            10. * self.display_scale / self.viewport.zoom,
        );
        self.snap_lines = (x.into_iter().collect(), y.into_iter().collect());
        Point::new(delta.x + dx, delta.y + dy)
    }
    pub fn draw_placement(&self, canvas: &skia_safe::Canvas) {
        use skia_safe::{Color, Paint, Rect};
        let doc = &self.history.document;
        let origin = geometry::canvas_origin(doc, &self.viewport);
        let z = self.viewport.zoom;
        let mut paint = Paint::default();
        if self.view_options.grid {
            canvas.save();
            canvas.clip_rect(
                Rect::from_xywh(
                    origin.x as f32,
                    origin.y as f32,
                    (doc.width as f64 * z) as f32,
                    (doc.height as f64 * z) as f32,
                ),
                None,
                false,
            );
            for axis in [GuideAxis::Vertical, GuideAxis::Horizontal] {
                let length = if axis == GuideAxis::Vertical {
                    doc.width
                } else {
                    doc.height
                };
                for value in placement::grid_lines(length as f64) {
                    // Eight subdivisions of the 64px major grid; keep the viewport path bounded.
                    let pos = if axis == GuideAxis::Vertical {
                        origin.x + value * z
                    } else {
                        origin.y + value * z
                    };
                    let max = if axis == GuideAxis::Vertical {
                        self.viewport.width
                    } else {
                        self.viewport.height
                    };
                    if pos < 0. || pos > max {
                        continue;
                    }
                    paint.set_color(Color::from_argb(
                        if value as u32 % 64 == 0 { 100 } else { 40 },
                        255,
                        255,
                        255,
                    ));
                    if axis == GuideAxis::Vertical {
                        canvas.draw_rect(
                            Rect::from_xywh(
                                pos.round() as f32,
                                0.,
                                1.,
                                self.viewport.height as f32,
                            ),
                            &paint,
                        );
                    } else {
                        canvas.draw_rect(
                            Rect::from_xywh(0., pos.round() as f32, self.viewport.width as f32, 1.),
                            &paint,
                        );
                    }
                }
            }
            canvas.restore();
        }
        paint.set_color(Color::from_argb(230, 0, 255, 255));
        let line = |axis: GuideAxis, value: f64| {
            if axis == GuideAxis::Vertical {
                canvas.draw_rect(
                    Rect::from_xywh(
                        (origin.x + value * z).round() as f32,
                        0.,
                        1.,
                        self.viewport.height as f32,
                    ),
                    &paint,
                );
            } else {
                canvas.draw_rect(
                    Rect::from_xywh(
                        0.,
                        (origin.y + value * z).round() as f32,
                        self.viewport.width as f32,
                        1.,
                    ),
                    &paint,
                );
            }
        };
        if self.view_options.guides {
            for guide in self.displayed_guides() {
                line(guide.axis, guide.position);
            }
        }
        for &x in &self.snap_lines.0 {
            line(GuideAxis::Vertical, x);
        }
        for &y in &self.snap_lines.1 {
            line(GuideAxis::Horizontal, y);
        }
    }
}
