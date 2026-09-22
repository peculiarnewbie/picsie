//! CropGeometry and CropSnap adapted from Compositor/Document/Crop.swift.
//! MIT © 2026 Wonder Assembly LLC. QuickGUI pointer routing is the local adaptation.
use crate::model::{Document, Point, dimensions};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl CropRect {
    pub fn from_document(doc: &Document) -> Self {
        Self {
            x: 0.,
            y: 0.,
            width: doc.width as f64,
            height: doc.height as f64,
        }
    }
    pub fn snapped(self) -> Self {
        let (x, y) = (self.x.round(), self.y.round());
        Self {
            x,
            y,
            width: ((self.x + self.width).round() - x).max(1.),
            height: ((self.y + self.height).round() - y).max(1.),
        }
    }
    pub fn validate(self) -> Result<Self> {
        ensure!(
            self.x.is_finite()
                && self.y.is_finite()
                && self.width.is_finite()
                && self.height.is_finite(),
            "Invalid crop rectangle"
        );
        ensure!(
            self.x.abs() <= 100000. && self.y.abs() <= 100000.,
            "Crop is too far from the canvas"
        );
        dimensions(
            self.width.round().max(1.) as u32,
            self.height.round().max(1.) as u32,
        )?;
        ensure!(
            self.width >= 1. && self.height >= 1.,
            "Crop must have a positive size"
        );
        Ok(self)
    }
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.x + self.width && p.y >= self.y && p.y <= self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CropRatio {
    Free,
    Original,
    Square,
    FourThree,
    SixteenNine,
}
impl CropRatio {
    pub fn value(self, doc: &Document) -> Option<f64> {
        match self {
            Self::Free => None,
            Self::Original => Some(doc.width as f64 / doc.height as f64),
            Self::Square => Some(1.),
            Self::FourThree => Some(4. / 3.),
            Self::SixteenNine => Some(16. / 9.),
        }
    }
}

#[derive(Clone, Copy)]
pub enum DragMode {
    Create,
    Move,
    Resize(usize),
}
#[derive(Clone, Copy)]
pub struct CropDrag {
    pub start: Point,
    pub original: CropRect,
    pub mode: DragMode,
}
impl CropDrag {
    pub fn updated(self, point: Point, ratio: Option<f64>, symmetric: bool) -> CropRect {
        match self.mode {
            DragMode::Move => CropRect {
                x: self.original.x + point.x - self.start.x,
                y: self.original.y + point.y - self.start.y,
                ..self.original
            }
            .snapped(),
            DragMode::Create => create(self.start, point, ratio, symmetric),
            DragMode::Resize(index) => {
                // Crop.swift delegates handle resizing to TransformDrag.updated. Reuse the
                // corresponding Rust transform port so crossed handles and anchors agree.
                let original = self.original;
                let mut proxy = crate::model::Layer::new(
                    "Crop frame",
                    original.width.round().max(1.) as u32,
                    original.height.round().max(1.) as u32,
                    crate::model::Content::Paint,
                );
                proxy.x = original.x;
                proxy.y = original.y;
                proxy.scale_x = original.width / proxy.width as f64;
                proxy.scale_y = original.height / proxy.height as f64;
                let handle = crate::geometry::HANDLES[index].0;
                let handle_point = crate::geometry::handles(&proxy, 1.)[index].1;
                let adjusted = crate::geometry::resize(
                    &proxy,
                    handle,
                    Point::new(
                        handle_point.x + point.x - self.start.x,
                        handle_point.y + point.y - self.start.y,
                    ),
                    ratio.is_some(),
                    symmetric,
                );
                CropRect {
                    x: adjusted.x,
                    y: adjusted.y,
                    width: adjusted.width as f64 * adjusted.scale_x,
                    height: adjusted.height as f64 * adjusted.scale_y,
                }
                .snapped()
            }
        }
    }
}

/// CropGeometry.create: reversed and Option-centered drags are normalized before rounding.
pub fn create(start: Point, end: Point, ratio: Option<f64>, symmetric: bool) -> CropRect {
    let (mut dx, mut dy) = (end.x - start.x, end.y - start.y);
    if let Some(r) = ratio {
        if dx.abs() > dy.abs() * r {
            dy = if dy < 0. { -1. } else { 1. } * dx.abs() / r;
        } else {
            dx = if dx < 0. { -1. } else { 1. } * dy.abs() * r;
        }
    }
    if symmetric {
        CropRect {
            x: start.x - dx.abs(),
            y: start.y - dy.abs(),
            width: dx.abs() * 2.,
            height: dy.abs() * 2.,
        }
        .snapped()
    } else {
        CropRect {
            x: start.x.min(start.x + dx),
            y: start.y.min(start.y + dy),
            width: dx.abs(),
            height: dy.abs(),
        }
        .snapped()
    }
}

pub fn hit(rect: CropRect, p: Point, zoom: f64) -> DragMode {
    const H: [(f64, f64); 8] = [
        (0., 0.),
        (0.5, 0.),
        (1., 0.),
        (1., 0.5),
        (1., 1.),
        (0.5, 1.),
        (0., 1.),
        (0., 0.5),
    ];
    if let Some((index, _)) = H.iter().enumerate().find(|(_, (x, y))| {
        Point::new(rect.x + x * rect.width, rect.y + y * rect.height).distance(p) * zoom <= 8.
    }) {
        return DragMode::Resize(index);
    }
    if rect.contains(p) {
        DragMode::Move
    } else {
        DragMode::Create
    }
}

/// CropSnap: canvas and visible layer edges; fixed-ratio resize keeps its exact ratio.
pub fn snap(
    mut rect: CropRect,
    drag: CropDrag,
    point: Point,
    ratio: Option<f64>,
    targets: (&[f64], &[f64]),
    tolerance: f64,
) -> CropRect {
    if tolerance <= 0. {
        return rect;
    }
    let nearest = |v: f64, list: &[f64]| {
        list.iter()
            .filter(|t| (**t - v).abs() <= tolerance)
            .min_by(|a, b| (*a - v).abs().total_cmp(&(*b - v).abs()))
            .copied()
    };
    if matches!(drag.mode, DragMode::Move) {
        let sx = [rect.x, rect.x + rect.width]
            .iter()
            .filter_map(|v| nearest(*v, targets.0).map(|n| n - v))
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or(0.);
        let sy = [rect.y, rect.y + rect.height]
            .iter()
            .filter_map(|v| nearest(*v, targets.1).map(|n| n - v))
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or(0.);
        rect.x += sx;
        rect.y += sy;
        return rect;
    }
    if ratio.is_some() {
        return rect;
    }
    let (horizontal, vertical) = match drag.mode {
        DragMode::Create => (true, true),
        DragMode::Resize(i) => (i != 1 && i != 5, i != 3 && i != 7),
        DragMode::Move => unreachable!(),
    };
    if horizontal {
        let left = (point.x - rect.x).abs() <= (point.x - rect.x - rect.width).abs();
        if left {
            if let Some(x) = nearest(rect.x, targets.0) {
                if x < rect.x + rect.width {
                    rect.width += rect.x - x;
                    rect.x = x;
                }
            }
        } else if let Some(x) = nearest(rect.x + rect.width, targets.0) {
            if x > rect.x {
                rect.width = x - rect.x;
            }
        }
    }
    if vertical {
        let top = (point.y - rect.y).abs() <= (point.y - rect.y - rect.height).abs();
        if top {
            if let Some(y) = nearest(rect.y, targets.1) {
                if y < rect.y + rect.height {
                    rect.height += rect.y - y;
                    rect.y = y;
                }
            }
        } else if let Some(y) = nearest(rect.y + rect.height, targets.1) {
            if y > rect.y {
                rect.height = y - rect.y;
            }
        }
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas_size::crop_canvas,
        model::{Content, Layer},
    };

    // Compositor CropTests.dragGeometrySupportsReverseRatioMoveAndEveryHandle.
    #[test]
    fn source_drag_geometry() {
        let rect = create(
            Point::new(100., 100.),
            Point::new(20., 60.),
            Some(2.),
            false,
        );
        assert_eq!(
            rect,
            CropRect {
                x: 20.,
                y: 60.,
                width: 80.,
                height: 40.
            }
        );
        let drag = CropDrag {
            start: Point::new(40., 70.),
            original: rect,
            mode: DragMode::Move,
        };
        assert_eq!(
            drag.updated(Point::new(30., 40.), None, false),
            CropRect {
                x: 10.,
                y: 30.,
                ..rect
            }
        );
        for (index, (hx, hy)) in [
            (0., 0.),
            (0.5, 0.),
            (1., 0.),
            (1., 0.5),
            (1., 1.),
            (0.5, 1.),
            (0., 1.),
            (0., 0.5),
        ]
        .into_iter()
        .enumerate()
        {
            let start = Point::new(rect.x + hx * rect.width, rect.y + hy * rect.height);
            let drag = CropDrag {
                start,
                original: rect,
                mode: DragMode::Resize(index),
            };
            let next = drag.updated(
                Point::new(
                    start.x + (hx * 2. - 1.) * 20.,
                    start.y + (hy * 2. - 1.) * 10.,
                ),
                Some(2.),
                false,
            );
            next.validate().unwrap();
            assert!(
                (next.width / next.height - 2.).abs() < 0.05,
                "handle {index}: {next:?}"
            );
            assert_ne!(next, rect);
        }
    }

    // Compositor CropTests.cropTranslatesWithoutResamplingAndUndoRestoresBounds,
    // sameSizeOffsetCropAndExpansionUseExactBounds: pure document portion.
    #[test]
    fn source_crop_moves_original_layer_pixels() {
        let mut doc = Document::new("Crop", 100, 50).unwrap();
        doc.layers
            .push(Layer::new("Paint", 100, 50, Content::Paint));
        let layer_id = doc.layers[0].id.clone();
        let next = crop_canvas(
            &doc,
            CropRect {
                x: 8.,
                y: 4.,
                width: 32.,
                height: 16.,
            },
        )
        .unwrap();
        assert_eq!((next.width, next.height), (32, 16));
        assert_eq!((next.layers[0].x, next.layers[0].y), (-8., -4.));
        assert_eq!(next.layers[0].id, layer_id);
        assert_eq!(doc.layers[0].x, 0.);
        let moved = crop_canvas(
            &doc,
            CropRect {
                x: -20.,
                y: 10.,
                width: 100.,
                height: 50.,
            },
        )
        .unwrap();
        assert_eq!((moved.layers[0].x, moved.layers[0].y), (20., -10.));
    }
}
