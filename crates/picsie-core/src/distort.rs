//! Distort.swift::DistortWarp, Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
//! Skia replaces Core Image perspective and Core Graphics triangle drawing for image and mask distortion.
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
        sampling: placement.sampling,
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

/// DistortWarp.warp / warpFolded / warpTrimmed. Convex perspective; otherwise
/// affine triangles on the 0–2 diagonal. Source alpha bounds trim only on Apply.
pub fn image(layer: &Layer, corners: &[Point; 4], limit: Option<f64>) -> Result<Layer> {
    ensure!(usable(corners), "Invalid layer distortion");
    let mut bare = layer.clone();
    bare.mask = None;
    let source = render::Renderer::default().layer_surface(&bare)?;
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
    let factor = limit.map(|l| (l / width.max(height)).min(1.)).unwrap_or(1.);
    let w = (width * factor).ceil().max(1.) as u32;
    let h = (height * factor).ceil().max(1.) as u32;
    let mut output = render::surface(w, h)?;
    output.canvas().clear(sk::Color::TRANSPARENT);
    let local = corners.map(|p| {
        sk::Point::new(
            ((p.x - minx) * w as f64 / width) as f32,
            ((p.y - miny) * h as f64 / height) as f32,
        )
    });
    let index = |i: usize| match (layer.flip_x, layer.flip_y) {
        (false, false) => i,
        (true, false) => [1, 0, 3, 2][i],
        (false, true) => [3, 2, 1, 0][i],
        (true, true) => [2, 3, 0, 1][i],
    };
    let target = [
        local[index(0)],
        local[index(1)],
        local[index(2)],
        local[index(3)],
    ];
    let from = [
        sk::Point::new(0., 0.),
        sk::Point::new(source.width() as f32, 0.),
        sk::Point::new(source.width() as f32, source.height() as f32),
        sk::Point::new(0., source.height() as f32),
    ];
    let halves: &[&[usize]] = if convex(corners) {
        &[&[0, 1, 2, 3]]
    } else {
        &[&[0, 1, 2], &[0, 2, 3]]
    };
    for indices in halves {
        let a = indices.iter().map(|i| from[*i]).collect::<Vec<_>>();
        let b = indices.iter().map(|i| target[*i]).collect::<Vec<_>>();
        let Some(map) = Matrix::poly_to_poly(&a, &b) else {
            continue;
        };
        let mut shape = PathBuilder::new();
        shape.move_to(b[0]);
        for p in &b[1..] {
            shape.line_to(*p);
        }
        shape.close();
        let c = output.canvas();
        c.save();
        c.clip_path(&shape.detach(), None, true);
        c.concat(&map);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        c.draw_image_with_sampling_options(
            &source,
            (0., 0.),
            render::sampling_options(layer.sampling),
            Some(&paint),
        );
        c.restore();
    }
    let mut result = layer.clone();
    result.x = minx;
    result.y = miny;
    result.width = w;
    result.height = h;
    result.scale_x = width / w as f64;
    result.scale_y = height / h as f64;
    result.rotation = 0.;
    result.flip_x = false;
    result.flip_y = false;
    result.strokes.clear();
    let mut warped = output.image_snapshot();
    if limit.is_none() {
        let rgba = render::rgba_pixels(&warped)?;
        let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if rgba[((y * w + x) * 4 + 3) as usize] > 0 {
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x + 1);
                    bottom = bottom.max(y + 1);
                }
            }
        }
        if right > left && bottom > top && (left > 0 || top > 0 || right < w || bottom < h) {
            let mut trimmed = render::surface(right - left, bottom - top)?;
            trimmed
                .canvas()
                .draw_image(&warped, (-(left as f32), -(top as f32)), None);
            warped = trimmed.image_snapshot();
            result.x += left as f64;
            result.y += top as f64;
            result.width = right - left;
            result.height = bottom - top;
        }
    }
    result.content = Arc::new(render::native_content(warped));
    if let Some(owned) = &layer.mask {
        let mut owned = owned.as_ref().clone();
        let original = owned.placement.unwrap_or_else(|| MaskPlacement::of(layer));
        let placed = original.as_layer(layer);
        if owned.linked {
            let mut unflipped = layer.clone();
            unflipped.flip_x = false;
            unflipped.flip_y = false;
            let map = homography(corners);
            let carried = crate::geometry::corners(&placed)
                .into_iter()
                .map(|p| {
                    let local = crate::geometry::to_local(&unflipped, p);
                    let world = map.map_point(sk::Point::new(
                        (local.x / layer.width as f64) as f32,
                        (local.y / layer.height as f64) as f32,
                    ));
                    Point::new(world.x as f64, world.y as f64)
                })
                .collect::<Vec<_>>();
            let carried: [Point; 4] = if owned.placement.is_none() {
                *corners
            } else {
                carried.try_into().unwrap()
            };
            if usable(&carried) {
                let (raster, placement) = mask(layer, original, &carried, limit)?;
                owned.raster = Some(Arc::new(raster));
                owned.strokes.clear();
                let mask_world = placement.as_layer(layer);
                owned.placement = Some(MaskPlacement {
                    scale_x: mask_world.width as f64 * mask_world.scale_x / result.width as f64,
                    scale_y: mask_world.height as f64 * mask_world.scale_y / result.height as f64,
                    ..placement
                });
            }
        } else {
            // An unlinked mask remains in document space, even after alpha trimming.
            owned.placement = Some(MaskPlacement {
                scale_x: layer.width as f64 * original.scale_x / result.width as f64,
                scale_y: layer.height as f64 * original.scale_y / result.height as f64,
                ..original
            });
        }
        result.mask = Some(Arc::new(owned));
    }
    result.validate()?;
    Ok(result)
}
