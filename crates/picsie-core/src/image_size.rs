//! ImageResizer.swift translated from Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
//! Skia replaces CoreGraphics; Picsie's existing allocation limits apply.
use crate::{
    geometry,
    model::*,
    render::{self, Renderer},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

#[derive(Clone, Serialize, Deserialize, TS)]
pub struct ImageSizeOptions {
    pub width: u32,
    pub height: u32,
    pub resolution: f64,
    pub sampling: Sampling,
}
pub fn resize(doc: &Document, options: &ImageSizeOptions) -> Result<Document> {
    dimensions(options.width, options.height)?;
    ensure!(
        options.resolution.is_finite() && (1. ..=9600.).contains(&options.resolution),
        "Resolution must be between 1 and 9600 pixels/inch"
    );
    let mut next = doc.clone();
    next.resolution = options.resolution;
    if doc.width == options.width && doc.height == options.height {
        return Ok(next);
    }
    let (sx, sy) = (
        options.width as f64 / doc.width as f64,
        options.height as f64 / doc.height as f64,
    );
    let mut renderer = Renderer::default();
    let mut used = 0u64;
    let mut used_masks = 0u64;
    for layer in &mut next.layers {
        let old = layer.clone();
        let corners = geometry::corners(&old);
        let left = corners
            .iter()
            .map(|p| (p.x * sx).floor())
            .fold(f64::INFINITY, f64::min);
        let top = corners
            .iter()
            .map(|p| (p.y * sy).floor())
            .fold(f64::INFINITY, f64::min);
        let right = corners
            .iter()
            .map(|p| (p.x * sx).ceil())
            .fold(f64::NEG_INFINITY, f64::max);
        let bottom = corners
            .iter()
            .map(|p| (p.y * sy).ceil())
            .fold(f64::NEG_INFINITY, f64::max);
        let (w, h) = ((right - left).max(1.) as u32, (bottom - top).max(1.) as u32);
        dimensions(w, h)?;
        // Adjustment layers carry no pixels to resample; only their masks and
        // full-document geometry follow the new size.
        let is_adjustment = old.adjustment.is_some();
        if !matches!(old.content.as_ref(), Content::Group) && !is_adjustment {
            used += w as u64 * h as u64;
            ensure!(
                used <= MAX_PIXELS,
                "Resized layer assets exceed the 24 megapixel budget"
            );
            let mut bare = old.clone();
            bare.mask = None;
            bare.sampling = options.sampling;
            let image = renderer.layer_surface(&bare)?;
            let mut surface = render::surface(w, h)?;
            surface
                .canvas()
                .translate((-left as f32, -top as f32))
                .scale((sx as f32, sy as f32));
            render::draw_pixels(surface.canvas(), &bare, &image);
            layer.content = Arc::new(render::native_content(surface.image_snapshot()));
            layer.strokes.clear();
        }
        if let Some(mask) = &old.mask {
            let raster = if mask.strokes.is_empty() {
                mask.raster
                    .as_deref()
                    .cloned()
                    .unwrap_or_else(|| MaskRaster::solid(mask.base == MaskMode::Reveal))
            } else {
                render::rasterize_mask(mask, &old)?
            };
            if let Some(placement) = mask.placement {
                // Retain independent mask pixels; scale its document placement, as upstream.
                let axis = Layer::new("Mask axes", old.width, old.height, Content::Paint);
                let mut scaled = axis.clone();
                scaled.scale_x = sx;
                scaled.scale_y = sy;
                let mut placement = placement.following(&axis, &scaled);
                placement.scale_x *= old.width as f64 / w as f64;
                placement.scale_y *= old.height as f64 / h as f64;
                let mut m = mask.as_ref().clone();
                m.raster = Some(Arc::new(raster));
                m.strokes.clear();
                m.placement = Some(placement);
                layer.mask = Some(Arc::new(m));
            } else if raster.width != 1 || raster.height != 1 {
                used_masks += w as u64 * h as u64;
                ensure!(
                    used_masks <= MAX_PIXELS,
                    "Resized mask assets exceed the 24 megapixel budget"
                );
                let mut output = render::surface(w, h)?;
                let placed = mask
                    .placement
                    .map(|p| p.as_layer(&old))
                    .unwrap_or_else(|| old.clone());
                let mut rgba = Vec::with_capacity(raster.pixels.len() * 4);
                for &v in raster.pixels.iter() {
                    rgba.extend([v, v, v, 255]);
                }
                let image = render::rgba_image(raster.width, raster.height, &rgba)?;
                output.canvas().clear(skia_safe::Color::BLACK);
                output
                    .canvas()
                    .translate((-left as f32, -top as f32))
                    .scale((sx as f32, sy as f32));
                let mut placed = placed;
                placed.sampling = options.sampling;
                render::draw_pixels(output.canvas(), &placed, &image);
                let pixels = render::rgba_pixels(&output.image_snapshot())?
                    .chunks_exact(4)
                    .map(|p| p[0])
                    .collect();
                let mut m = mask.as_ref().clone();
                m.raster = Some(Arc::new(MaskRaster {
                    width: w,
                    height: h,
                    pixels: Arc::new(pixels),
                }));
                m.placement = None;
                m.strokes.clear();
                layer.mask = Some(Arc::new(m));
            }
        }
        layer.x = left;
        layer.y = top;
        layer.width = w;
        layer.height = h;
        layer.scale_x = 1.;
        layer.scale_y = 1.;
        layer.rotation = 0.;
        layer.flip_x = false;
        layer.flip_y = false;
        layer.sampling = options.sampling;
        if layer.adjustment.is_some() {
            layer.x = 0.;
            layer.y = 0.;
            layer.width = options.width;
            layer.height = options.height;
        }
    }
    for guide in &mut next.guides {
        guide.scale(sx, sy);
    }
    next.width = options.width;
    next.height = options.height;
    next.validate()?;
    Ok(next)
}
