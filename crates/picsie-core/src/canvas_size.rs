//! Compositor CanvasSize.swift / CanvasResizer.swift, MIT © 2026 Wonder Assembly LLC.
use crate::{crop::CropRect, model::*, render};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct CanvasSizeOptions {
    pub width: u32,
    pub height: u32,
    #[serde(default = "center")]
    pub anchor: u8,
    #[serde(default)]
    #[ts(optional)]
    pub fill: Option<String>,
}
fn center() -> u8 {
    4
}
pub fn resize_canvas(doc: &Document, opt: &CanvasSizeOptions) -> Result<Document> {
    dimensions(opt.width, opt.height)?;
    ensure!(opt.anchor <= 8, "Invalid canvas anchor");
    if let Some(fill) = &opt.fill {
        ensure!(
            fill.len() == 7 && color_valid(fill),
            "Use a six-digit extension color"
        );
    }
    if opt.width == doc.width && opt.height == doc.height {
        return Ok(doc.clone());
    }
    let dx = ((opt.width as f64 - doc.width as f64) * (opt.anchor % 3) as f64 / 2.).floor();
    let dy = ((opt.height as f64 - doc.height as f64) * (opt.anchor / 3) as f64 / 2.).floor();
    let mut next = doc.clone();
    next.width = opt.width;
    next.height = opt.height;
    for guide in &mut next.guides {
        guide.offset(dx, dy);
    }
    for l in &mut next.layers {
        // CanvasResizer translates every layer transform by the anchor offset,
        // adjustments included; explicitly placed masks move along with them.
        l.x += dx;
        l.y += dy;
        if l.mask.as_ref().is_some_and(|m| m.placement.is_some()) {
            let placed = Arc::make_mut(l.mask.as_mut().unwrap());
            if let Some(placement) = placed.placement.as_mut() {
                placement.x += dx;
                placement.y += dy;
            }
        }
    }
    next.validate()?;
    if let Some(fill) = &opt.fill
        && (opt.width > doc.width || opt.height > doc.height)
    {
        ensure!(
            next.layers.len() < crate::model::MAX_LAYERS,
            "A colored extension needs one available layer"
        );
        let mut s = render::surface(opt.width, opt.height)?;
        let c = s.canvas();
        c.clear(render::color(fill));
        let mut p = skia_safe::Paint::default();
        p.set_blend_mode(skia_safe::BlendMode::Clear);
        c.draw_rect(
            skia_safe::Rect::from_xywh(dx as f32, dy as f32, doc.width as f32, doc.height as f32),
            &p,
        );
        next.layers.insert(
            0,
            Layer::new(
                "Canvas Extension",
                opt.width,
                opt.height,
                render::png_content(&s.image_snapshot())?,
            ),
        );
    }
    Ok(next)
}

/// Compositor CanvasResizer applies a crop as a document translation. Source pixels stay intact.
pub fn crop_canvas(doc: &Document, rect: CropRect) -> Result<Document> {
    let rect = rect.snapped().validate()?;
    let mut next = doc.clone();
    next.width = rect.width as u32;
    next.height = rect.height as u32;
    for guide in &mut next.guides {
        guide.offset(-rect.x, -rect.y);
    }
    for layer in &mut next.layers {
        // Crop commits through the same resizer upstream: every layer moves
        // with the document, adjustments and placed masks included.
        layer.x -= rect.x;
        layer.y -= rect.y;
        if layer.mask.as_ref().is_some_and(|m| m.placement.is_some()) {
            let placed = Arc::make_mut(layer.mask.as_mut().unwrap());
            if let Some(placement) = placed.placement.as_mut() {
                placement.x -= rect.x;
                placement.y -= rect.y;
            }
        }
    }
    next.validate()?;
    Ok(next)
}
