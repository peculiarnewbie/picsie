//! ShapeTool.dragShape / Selection.DragBox.rect, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Only the rectangle/ellipse/line subset.
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

/// ShapeTool.dragShape's line branch: Shift snaps the angle to eighths of a turn —
/// flat, upright, or 45° — keeping the dragged length.
pub fn snap_line_end(anchor: Point, point: Point) -> Point {
    let (dx, dy) = (point.x - anchor.x, point.y - anchor.y);
    let length = dx.hypot(dy);
    if length <= 0. || !length.is_finite() {
        return point;
    }
    let angle = (dy.atan2(dx) / (std::f64::consts::PI / 4.)).round() * std::f64::consts::PI / 4.;
    Point::new(
        anchor.x + angle.cos() * length,
        anchor.y + angle.sin() * length,
    )
}

/// A line layer's box: the dragged points with room for the stroke's own thickness
/// (and its round ends) around them. ShapeTool.finishShape; Option grows the box
/// around the anchor, as DragBox does for other shapes.
pub fn line_box(anchor: Point, end: Point, thickness: f64, from_center: bool) -> CropRect {
    if from_center {
        let (dx, dy) = ((end.x - anchor.x).abs(), (end.y - anchor.y).abs());
        return CropRect {
            x: anchor.x - dx - thickness / 2.,
            y: anchor.y - dy - thickness / 2.,
            width: dx * 2. + thickness,
            height: dy * 2. + thickness,
        };
    }
    CropRect {
        x: anchor.x.min(end.x) - thickness / 2.,
        y: anchor.y.min(end.y) - thickness / 2.,
        width: (end.x - anchor.x).abs() + thickness,
        height: (end.y - anchor.y).abs() + thickness,
    }
}

/// LayerShapeStyle.path: ellipses ignore the radius; rectangles clamp it to half the
/// shorter side, so a large radius makes a pill.
pub fn corner_radius(width: f64, height: f64, radius: f64) -> f64 {
    radius.max(0.).min(width / 2.).min(height / 2.)
}

/// LayerShapeStyle.redrawShape: a scaled shape layer redraws at its displayed size,
/// so a rounded corner keeps its document-pixel radius (and a line its width)
/// instead of stretching. Part of the edit that changed the size.
///
/// Takes the layer with its new transform, returning the normalized layer (source grid at the
/// displayed size, unit scale, same center/rotation/flips), or None when no
/// redraw applies. A following mask is resampled into the new grid through the
/// unchanged document mapping, as upstream pins it with an explicit placement;
/// independent placements are kept and their coverage resampled the same way.
/// Layers with legacy mask strokes are left alone rather than baking them.
pub fn redraw_at_displayed_size(next: &crate::model::Layer) -> Option<crate::model::Layer> {
    use crate::model::{Content, MaskPlacement};
    if !matches!(next.content.as_ref(), Content::Shape { .. }) {
        return None;
    }
    let displayed_w = (next.width as f64 * next.scale_x).round().max(1.);
    let displayed_h = (next.height as f64 * next.scale_y).round().max(1.);
    if displayed_w == next.width as f64
        && displayed_h == next.height as f64
        && next.scale_x == 1.
        && next.scale_y == 1.
    {
        return None;
    }
    if crate::model::dimensions(displayed_w as u32, displayed_h as u32).is_err() {
        return None;
    }
    let center = crate::geometry::center(next);
    let mut redrawn = next.clone();
    redrawn.width = displayed_w as u32;
    redrawn.height = displayed_h as u32;
    redrawn.x = center.x - displayed_w / 2.;
    redrawn.y = center.y - displayed_h / 2.;
    redrawn.scale_x = 1.;
    redrawn.scale_y = 1.;
    if redrawn.validate().is_err() {
        return None;
    }
    // redrawShape pins the current mask transform as an explicit placement before
    // replacing the asset; it never resamples mask pixels into world coordinates.
    // The local equivalent compensates the pre-redraw effective placement for the
    // grid change (scale by old/new grid ratio, footprint center preserved), so the
    // retained raster keeps covering the same document pixels through the existing
    // mask_surface grid mapping. No new raster buffers are allocated for this.
    if let Some(mask) = &next.mask {
        let mut kept = mask.as_ref().clone();
        // Legacy vector strokes bake into coverage through the established raster
        // path (identical pixels to rendering); the caller bumps the project to
        // version 2 for the new raster, as mask_edit does.
        if !kept.strokes.is_empty() || kept.raster.is_none() {
            let mut grid = next.clone();
            grid.mask = None;
            match crate::render::rasterize_mask(&kept, &grid) {
                Ok(raster) => {
                    kept.raster = Some(std::sync::Arc::new(raster));
                    kept.strokes.clear();
                }
                Err(_) => return None,
            }
        }
        let base = kept.placement.unwrap_or_else(|| MaskPlacement::of(next));
        let (old_w, old_h) = (next.width as f64, next.height as f64);
        // Footprint center in document pixels under the pre-redraw grid.
        let placed_old = {
            let mut probe = next.clone();
            probe.mask = None;
            base.as_layer(&probe)
        };
        let footprint_center = crate::geometry::to_world(
            &placed_old,
            crate::model::Point::new(old_w / 2., old_h / 2.),
        );
        let scale_x = base.scale_x * old_w / displayed_w;
        let scale_y = base.scale_y * old_h / displayed_h;
        kept.placement = Some(MaskPlacement {
            x: footprint_center.x - displayed_w * scale_x / 2.,
            y: footprint_center.y - displayed_h * scale_y / 2.,
            scale_x,
            scale_y,
            ..base
        });
        redrawn.mask = Some(std::sync::Arc::new(kept));
    }
    Some(redrawn)
}

/// ShapeTool.shapeImage: the shape filling its box, anti-aliased where it curves.
/// Lines run between their fractional box ends (older lines without ends run corner
/// to corner, inset by half their thickness) with round caps.
pub fn draw_shape(
    canvas: &skia_safe::Canvas,
    width: u32,
    height: u32,
    shape: crate::model::Shape,
    color: &str,
    radius: f64,
    line_width: Option<f64>,
    line_start: Option<Point>,
    line_end: Option<Point>,
) {
    use crate::model::Shape as Kind;
    let paint_color = crate::render::color(color);
    if shape == Kind::Line {
        let thickness = line_width.unwrap_or(0.).max(1.);
        let (w, h) = (width as f64, height as f64);
        // Corner to corner, inset by half the thickness (capped by the box) so the
        // stroke stays inside the layer.
        let (ix, iy) = (thickness.min(w) / 2., thickness.min(h) / 2.);
        let at = |end: Option<Point>, fallback: (f64, f64)| {
            end.map(|p| skia_safe::Point::new((p.x * w) as f32, (p.y * h) as f32))
                .unwrap_or_else(|| skia_safe::Point::new(fallback.0 as f32, fallback.1 as f32))
        };
        let from = at(line_start, (ix, iy));
        let to = at(line_end, (w - ix, h - iy));
        let mut paint = skia_safe::Paint::default();
        paint
            .set_anti_alias(true)
            .set_color(paint_color)
            .set_style(skia_safe::paint::Style::Stroke)
            .set_stroke_width(thickness as f32)
            .set_stroke_cap(skia_safe::paint::Cap::Round);
        let mut path = skia_safe::PathBuilder::new();
        path.move_to(from);
        path.line_to(to);
        canvas.draw_path(&path.detach(), &paint);
        return;
    }
    let bounds = skia_safe::Rect::from_wh(width as f32, height as f32);
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true).set_color(paint_color);
    match shape {
        Kind::Ellipse => {
            canvas.draw_oval(bounds, &paint);
        }
        Kind::Rectangle => {
            let radius = corner_radius(width as f64, height as f64, radius) as f32;
            if radius > 0. {
                canvas.draw_round_rect(bounds, radius, radius, &paint);
            } else {
                canvas.draw_rect(bounds, &paint);
            }
        }
        Kind::Line => unreachable!(),
    }
}
