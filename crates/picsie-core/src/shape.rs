//! ShapeTool.dragShape / Selection.DragBox.rect, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Only the existing rectangle/ellipse subset.
use crate::{crop::CropRect, model::Point};
pub fn drag_box(anchor: Point, point: Point, square: bool, from_center: bool) -> Option<CropRect> {
    let (ax, ay) = (anchor.x.round(), anchor.y.round());
    let (mut dx, mut dy) = (point.x.round() - ax, point.y.round() - ay);
    if square {
        let side = dx.abs().max(dy.abs());
        dx = if dx < 0. { -side } else { side };
        dy = if dy < 0. { -side } else { side };
    }
    let rect = if from_center {
        CropRect {
            x: ax - dx.abs(),
            y: ay - dy.abs(),
            width: dx.abs() * 2.,
            height: dy.abs() * 2.,
        }
    } else {
        CropRect {
            x: ax.min(ax + dx),
            y: ay.min(ay + dy),
            width: dx.abs(),
            height: dy.abs(),
        }
    };
    (rect.width >= 1. && rect.height >= 1.).then_some(rect)
}
