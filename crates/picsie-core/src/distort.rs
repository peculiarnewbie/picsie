//! Distort.swift::DistortWarp, Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
//! Skia replaces Core Image perspective and Core Graphics triangle drawing. Mask-only port.
use crate::{model::*, render};
use anyhow::{Result, anyhow, ensure};
use skia_safe::{self as sk, Matrix, Paint, PathBuilder, Rect};
use std::sync::Arc;
pub fn usable(c: &[Point; 4]) -> bool {
    c.iter().all(|p| {
        p.x.is_finite() && p.y.is_finite() && p.x.abs() <= 1_000_000. && p.y.abs() <= 1_000_000.
    }) && area(c[0], c[1], c[2]).abs() > 0.01
        && area(c[0], c[2], c[3]).abs() > 0.01
}
fn area(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
pub fn convex(c: &[Point; 4]) -> bool {
    usable(c)
        && (0..4).all(|i| {
            let cross = area(c[i], c[(i + 1) % 4], c[(i + 2) % 4]);
            cross.abs() > 0.01 && cross.signum() == area(c[0], c[1], c[2]).signum()
        })
}
/// Unit-square perspective mapping, including mirrored shapes.
pub fn homography(c: &[Point; 4]) -> Matrix {
    let sx = c[0].x - c[1].x + c[2].x - c[3].x;
    let sy = c[0].y - c[1].y + c[2].y - c[3].y;
    let (mut g, mut h) = (0., 0.);
    if sx.abs() > 1e-9 || sy.abs() > 1e-9 {
        let dx1 = c[1].x - c[2].x;
        let dx2 = c[3].x - c[2].x;
        let dy1 = c[1].y - c[2].y;
        let dy2 = c[3].y - c[2].y;
        let den = dx1 * dy2 - dx2 * dy1;
        if den.abs() > 1e-12 {
            g = (sx * dy2 - dx2 * sy) / den;
            h = (dx1 * sy - sx * dy1) / den;
        }
    }
    Matrix::new_all(
        (c[1].x - c[0].x + g * c[1].x) as f32,
        (c[3].x - c[0].x + h * c[3].x) as f32,
        c[0].x as f32,
        (c[1].y - c[0].y + g * c[1].y) as f32,
        (c[3].y - c[0].y + h * c[3].y) as f32,
        c[0].y as f32,
        g as f32,
        h as f32,
        1.,
    )
}
pub fn mask(
    layer: &Layer,
    placement: MaskPlacement,
    corners: &[Point; 4],
    limit: Option<f64>,
) -> Result<(MaskRaster, MaskPlacement)> {
    ensure!(usable(corners), "Invalid mask distortion");
    let owned = layer
        .mask
        .as_ref()
        .ok_or_else(|| anyhow!("No layer mask"))?;
    let raster = if owned.strokes.is_empty()
        && let Some(raster) = &owned.raster
    {
        raster.as_ref().clone()
    } else {
        render::rasterize_mask(owned, layer)?
    };
    let minx = corners
        .iter()
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min)
        .floor();
    let miny = corners
        .iter()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min)
        .floor();
    let width = corners
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        - minx;
    let height = corners
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        - miny;
    dimensions(width as u32, height as u32)?;
    let placed = MaskPlacement {
        x: minx,
        y: miny,
        scale_x: width / layer.width as f64,
        scale_y: height / layer.height as f64,
        rotation: 0.,
        flip_x: false,
        flip_y: false,
    };
    if raster.width == 1 && raster.height == 1 {
        return Ok((raster, placed));
    }
    let factor = limit.map(|l| (l / width.max(height)).min(1.)).unwrap_or(1.);
    let w = (width * factor).ceil().max(1.) as u32;
    let h = (height * factor).ceil().max(1.) as u32;
    let background = render::mask_background(&raster);
    let mut output = render::surface(w, h)?;
    output
        .canvas()
        .clear(sk::Color::from_rgb(background, background, background));
    let rgba = raster
        .pixels
        .iter()
        .flat_map(|v| [*v, *v, *v, 255])
        .collect::<Vec<_>>();
    let image = render::rgba_image(raster.width, raster.height, &rgba)?;
    let local = corners.map(|p| {
        sk::Point::new(
            ((p.x - minx) * w as f64 / width) as f32,
            ((p.y - miny) * h as f64 / height) as f32,
        )
    });
    let target = [
        local[if placement.flip_y {
            if placement.flip_x { 2 } else { 3 }
        } else {
            if placement.flip_x { 1 } else { 0 }
        }],
        local[if placement.flip_y {
            if placement.flip_x { 3 } else { 2 }
        } else {
            if placement.flip_x { 0 } else { 1 }
        }],
        local[if placement.flip_y {
            if placement.flip_x { 0 } else { 1 }
        } else {
            if placement.flip_x { 3 } else { 2 }
        }],
        local[if placement.flip_y {
            if placement.flip_x { 1 } else { 0 }
        } else {
            if placement.flip_x { 2 } else { 3 }
        }],
    ];
    let source = [
        sk::Point::new(0., 0.),
        sk::Point::new(raster.width as f32, 0.),
        sk::Point::new(raster.width as f32, raster.height as f32),
        sk::Point::new(0., raster.height as f32),
    ];
    let halves: Vec<Vec<usize>> = if convex(corners) {
        vec![vec![0, 1, 2, 3]]
    } else {
        vec![vec![0, 1, 2], vec![0, 2, 3]]
    };
    for indices in halves {
        let from = indices.iter().map(|i| source[*i]).collect::<Vec<_>>();
        let to = indices.iter().map(|i| target[*i]).collect::<Vec<_>>();
        let Some(matrix) = Matrix::poly_to_poly(&from, &to) else {
            continue;
        };
        let c = output.canvas();
        c.save();
        let mut path = PathBuilder::new();
        path.move_to(to[0]);
        for p in &to[1..] {
            path.line_to(*p);
        }
        path.close();
        c.clip_path(&path.detach(), None, false);
        c.concat(&matrix);
        let mut paint = Paint::default();
        paint.set_blend_mode(sk::BlendMode::Src);
        c.draw_image_rect_with_sampling_options(
            &image,
            None,
            Rect::from_wh(raster.width as f32, raster.height as f32),
            sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
            &paint,
        );
        c.restore();
    }
    let pixels = render::rgba_pixels(&output.image_snapshot())?
        .chunks_exact(4)
        .map(|p| p[0])
        .collect();
    Ok((
        MaskRaster {
            width: w,
            height: h,
            pixels: Arc::new(pixels),
        },
        placed,
    ))
}
