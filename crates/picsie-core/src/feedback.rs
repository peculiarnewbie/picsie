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
                crate::crop::DragMode::Move if self.crop_active => CursorHint::Move,
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
            if let Some(c) = self.mask_distortion_corners() {
                let mut points = vec![];
                for i in 0..4 {
                    points.push(c[i]);
                    points.push(Point::new(
                        (c[i].x + c[(i + 1) % 4].x) / 2.,
                        (c[i].y + c[(i + 1) % 4].y) / 2.,
                    ));
                }
                points.push(Point::new(
                    (c[0].x + c[1].x) / 2.,
                    (c[0].y + c[1].y) / 2. - 28. / zoom,
                ));
                map.handles = Some(HandleGeometry {
                    points,
                    zoom,
                    rotation: true,
                });
                map.movable = true;
                return map;
            }
            map.movable = selected.is_some_and(|l| self.history.document.effective(l).0);
            if self.selection.ids.len() == 1
                && (self.view_options.show_controls || self.transform_active())
            {
                map.handles = self
                    .independent_mask_layer()
                    .as_ref()
                    .or(selected)
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
