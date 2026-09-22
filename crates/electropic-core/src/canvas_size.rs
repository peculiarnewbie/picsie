//! Compositor CanvasSize.swift / CanvasResizer.swift, MIT © 2026 Wonder Assembly LLC.
use crate::{model::*, render};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
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
    for l in &mut next.layers {
        l.x += dx;
        l.y += dy;
    }
    next.validate()?;
    if let Some(fill) = &opt.fill
        && (opt.width > doc.width || opt.height > doc.height)
    {
        ensure!(
            next.layers.len() < 100,
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
