//! Canvas cursor semantics adapted from EditorCanvas, TransformOverlay and
//! InlineTextEditor, Compositor 609dbeae. MIT © 2026 Wonder Assembly LLC.
//! Small geometry metadata lets the UI update hover feedback without rendering
//! or submitting an engine command for every mouse move. No pixels cross here.
use crate::{
    editor::{Editor, Modifiers, Tool},
    geometry::{self, HandleGeometry},
    model::Point,
    placement::{CanvasGuide, GuideAxis},
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CursorHint {
    Arrow,
    Text,
    Crosshair,
    Move,
    Copy,
    Hand,
    Rotate,
    ResizeHorizontal,
    ResizeDiagonalDown,
    ResizeVertical,
    ResizeDiagonalUp,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct CursorMap {
    pub crop: Option<crate::crop::CropRect>,
    pub crop_active: bool,
    pub crop_movable: bool,
    pub brush_diameter: Option<f64>,
    pub origin: Point,
    pub viewport_zoom: f64,
    pub default: CursorHint,
    pub handles: Option<HandleGeometry>,
    pub guides: Vec<CanvasGuide>,
    pub zoom: f64,
    pub movable: bool,
    pub pickable: Vec<Vec<Point>>,
}
impl CursorMap {
    pub fn at_view(&self, point: Point, modifiers: Modifiers) -> CursorHint {
        self.at(
            Point::new(
                (point.x - self.origin.x) / self.viewport_zoom,
                (point.y - self.origin.y) / self.viewport_zoom,
            ),
            modifiers,
        )
    }
    pub fn at(&self, point: Point, modifiers: Modifiers) -> CursorHint {
        if let Some(crop) = self.crop {
            return match crate::crop::hit(crop, point, self.zoom) {
                crate::crop::DragMode::Move if self.crop_movable => CursorHint::Move,
                crate::crop::DragMode::Move => CursorHint::Crosshair,
                crate::crop::DragMode::Create => CursorHint::Crosshair,
                crate::crop::DragMode::Resize(i) => [
                    CursorHint::ResizeDiagonalDown,
                    CursorHint::ResizeVertical,
                    CursorHint::ResizeDiagonalUp,
                    CursorHint::ResizeHorizontal,
                ][i % 4],
            };
        }
        if let Some(handles) = &self.handles
            && let Some(handle) = handles.hit(point)
        {
            if handle == "rotate" {
                return CursorHint::Rotate;
            }
            return [
                CursorHint::ResizeHorizontal,
                CursorHint::ResizeDiagonalDown,
                CursorHint::ResizeVertical,
                CursorHint::ResizeDiagonalUp,
            ][handles.resize_direction(handle)];
        }
        let guide_distance = |g: &CanvasGuide| {
            (g.position
                - if g.axis == GuideAxis::Vertical {
                    point.x
                } else {
                    point.y
                })
            .abs()
                * self.zoom
        };
        if let Some(guide) = self
            .guides
            .iter()
            .filter(|g| guide_distance(g) <= 5.)
            .min_by(|a, b| guide_distance(a).total_cmp(&guide_distance(b)))
        {
            return if guide.axis == GuideAxis::Vertical {
                CursorHint::ResizeHorizontal
            } else {
                CursorHint::ResizeVertical
            };
        }
        if self.movable || self.pickable.iter().any(|corners| contains(corners, point)) {
            return if modifiers.alt {
                CursorHint::Copy
            } else {
                CursorHint::Move
            };
        }
        self.default
    }
}
fn contains(corners: &[Point], p: Point) -> bool {
    (0..4).all(|i| {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x) >= 0.
    })
}
impl Editor {
    pub fn cursor_map(&self) -> CursorMap {
        let zoom = self.viewport.zoom / self.display_scale;
        let selected = self
            .selected()
            .filter(|l| !l.locked && self.history.document.effective(l).0);
        let mut map = CursorMap {
            crop: if self.tool == Tool::Crop {
                Some(self.crop_rect.unwrap_or_else(|| {
                    crate::crop::CropRect::from_document(&self.history.document)
                }))
            } else {
                None
            },
            crop_active: self.tool == Tool::Crop && self.crop_rect.is_some(),
            crop_movable: self.tool == Tool::Crop
                && self.crop_rect.is_some_and(|r| {
                    r != crate::crop::CropRect::from_document(&self.history.document)
                }),
            brush_diameter: matches!(self.tool, Tool::Brush | Tool::Eraser)
                .then_some(self.brush_size * zoom),
            origin: geometry::canvas_origin(&self.history.document, &self.viewport),
            viewport_zoom: self.viewport.zoom,
            default: match self.tool {
                Tool::Hand => CursorHint::Hand,
                Tool::Move => CursorHint::Arrow,
                Tool::Text if self.text_editing() => CursorHint::Text,
                _ => CursorHint::Crosshair,
            },
            handles: None,
            guides: vec![],
            zoom,
            movable: false,
            pickable: vec![],
        };
        if self.tool == Tool::Move {
            if let Some(c) = self.distortion_corners() {
                let mut points = vec![];
                for i in 0..4 {
                    points.push(c[i]);
                    points.push(Point::new(
                        (c[i].x + c[(i + 1) % 4].x) / 2.,
                        (c[i].y + c[(i + 1) % 4].y) / 2.,
                    ));
                }
                map.handles = Some(HandleGeometry {
                    points,
                    zoom,
                    rotation: false,
                });
                map.movable = true;
                return map;
            }
            // Sourceless adjustments have no pixels to move or reshape; their
            // independent masks (when targeted) still publish handles below.
            map.movable = selected
                .is_some_and(|l| l.adjustment.is_none() && self.history.document.effective(l).0);
            // A folder or multi-selection shows one box around the members
            // (`TransformOverlay.geometry`'s group branch); handles stay hidden for
            // a lone folder or anything without transformable members.
            if (self.view_options.show_controls || self.transform_active())
                && self.edited_group_box().is_some()
            {
                map.handles = self.edited_group_box().map(|b| HandleGeometry {
                    points: geometry::handles(&b, zoom)
                        .into_iter()
                        .map(|(_, p)| p)
                        .collect(),
                    zoom,
                    rotation: true,
                });
                map.movable = true;
            } else if self.selection.ids.len() == 1
                && (self.view_options.show_controls || self.transform_active())
            {
                map.handles = self
                    .independent_mask_layer()
                    .as_ref()
                    .or(selected.filter(|l| l.adjustment.is_none()))
                    .map(|l| HandleGeometry::new(l, zoom, true));
            }
            if self.view_options.guides && !self.view_options.lock_guides {
                map.guides = self.displayed_guides();
            }
            if self.view_options.auto_select {
                map.pickable = self
                    .history
                    .document
                    .ordered_layers()
                    .into_iter()
                    .filter(|l| {
                        let (visible, opacity) = self.history.document.effective(l);
                        visible
                            && opacity > 0.
                            && !l.locked
                            && !matches!(l.content.as_ref(), crate::model::Content::Group)
                    })
                    .map(geometry::corners)
                    .collect();
            }
        } else if self.tool == Tool::Text && self.text_editing() {
            map.handles = selected.map(|l| HandleGeometry::new(l, zoom, false));
        }
        map
    }
}

/// Native-only immutable selection geometry, skipped by JSON snapshots.
#[derive(Clone)]
pub struct SelectionFeedback {
    pub origin: Point,
    pub zoom: f64,
    pub mode: crate::pixel_selection::PixelSelectionMode,
    pub draft_mode: Option<crate::pixel_selection::PixelSelectionMode>,
    outline: Option<skia_safe::Path>,
    wand: bool,
}
impl SelectionFeedback {
    pub fn effective_mode(
        &self,
        modifiers: Modifiers,
    ) -> crate::pixel_selection::PixelSelectionMode {
        use crate::pixel_selection::PixelSelectionMode::*;
        self.draft_mode.unwrap_or(if modifiers.alt {
            Subtract
        } else if modifiers.shift {
            Add
        } else {
            self.mode
        })
    }
    pub fn at_view(&self, p: Point, modifiers: Modifiers) -> CursorHint {
        let mode = self.effective_mode(modifiers);
        let forced = modifiers.control || modifiers.meta;
        let inside = self.outline.as_ref().is_some_and(|path| {
            path.contains((
                ((p.x - self.origin.x) / self.zoom) as f32,
                ((p.y - self.origin.y) / self.zoom) as f32,
            ))
        });
        if self.draft_mode.is_none()
            && inside
            && (forced
                || (!self.wand && mode == crate::pixel_selection::PixelSelectionMode::Replace))
        {
            if forced && modifiers.alt {
                CursorHint::Copy
            } else {
                CursorHint::Move
            }
        } else {
            CursorHint::Crosshair
        }
    }
}
impl Editor {
    pub fn selection_feedback(&self) -> Option<SelectionFeedback> {
        matches!(self.tool, Tool::Marquee | Tool::Lasso | Tool::Wand).then(|| SelectionFeedback {
            origin: geometry::canvas_origin(&self.history.document, &self.viewport),
            zoom: self.viewport.zoom,
            mode: self.selection_mode,
            draft_mode: self.selection_draft().map(|d| d.mode),
            outline: self
                .history
                .pixel_selection
                .as_ref()
                .map(|s| s.outline.clone()),
            wand: self.tool == Tool::Wand,
        })
    }
}

/// Native vector resource for TransformOverlay's marching ants. Skia measures
/// curves once per editing frame; GPUI paints the cached contours on timer ticks.
#[derive(Clone)]
pub struct SelectionOutline {
    pub contours: std::sync::Arc<Vec<Vec<Point>>>,
    pub display_scale: f64,
}
impl SelectionOutline {
    pub fn dashes(&self, phase: f64) -> Vec<Vec<Point>> {
        let unit = self.display_scale;
        let mut result = vec![];
        for contour in self.contours.iter() {
            let mut distance = phase.rem_euclid(8.) * unit;
            let mut current = vec![];
            for pair in contour.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let length = a.distance(b);
                if length <= 1e-9 {
                    continue;
                }
                let mut at = 0.;
                while at < length {
                    let offset = distance.rem_euclid(8. * unit);
                    let black = offset < 4. * unit;
                    let next = (at
                        + (if black {
                            4. * unit - offset
                        } else {
                            8. * unit - offset
                        })
                        .max(1e-6))
                    .min(length);
                    let point = |n: f64| {
                        Point::new(
                            a.x + (b.x - a.x) * n / length,
                            a.y + (b.y - a.y) * n / length,
                        )
                    };
                    if black {
                        if current.is_empty() {
                            current.push(point(at));
                        }
                        current.push(point(next));
                    } else if !current.is_empty() {
                        result.push(std::mem::take(&mut current));
                    }
                    distance += next - at;
                    at = next;
                }
            }
            if !current.is_empty() {
                result.push(current);
            }
        }
        result
    }
}
impl Editor {
    pub fn selection_outline(&self) -> Option<SelectionOutline> {
        let selection = self.history.pixel_selection.as_ref()?;
        if selection.outline.is_empty() {
            return None;
        }
        let o = geometry::canvas_origin(&self.history.document, &self.viewport);
        let map = skia_safe::Matrix::new_all(
            self.viewport.zoom as f32,
            0.,
            o.x as f32,
            0.,
            self.viewport.zoom as f32,
            o.y as f32,
            0.,
            0.,
            1.,
        );
        let path = selection.outline.with_transform(&map);
        let mut measure = skia_safe::PathMeasure::new(&path, false, Some(1.));
        let mut contours = vec![];
        loop {
            let length = measure.length();
            if length > 0. {
                let steps = (length as f64 / (0.75 * self.display_scale))
                    .ceil()
                    .clamp(1., 16000.) as usize;
                let points = (0..=steps)
                    .filter_map(|i| {
                        measure
                            .pos_tan(length * i as f32 / steps as f32)
                            .map(|(p, _)| Point::new(p.x as f64, p.y as f64))
                    })
                    .collect();
                contours.push(points);
            }
            if !measure.next_contour() {
                break;
            }
        }
        Some(SelectionOutline {
            contours: std::sync::Arc::new(contours),
            display_scale: self.display_scale,
        })
    }
}
