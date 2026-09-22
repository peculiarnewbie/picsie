//! DocumentSelection / DragBox / LassoDraft coverage translated from the pinned Compositor.
//! MIT © 2026 Wonder Assembly LLC. Raster union is an adaptation for the Rust surface model.
use crate::model::{Document, Point};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PixelSelectionMode {
    Replace,
    Add,
    Subtract,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MarqueeKind {
    Rectangle,
    Ellipse,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct SelectionBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug)]
pub struct PixelSelection {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
    pub bounds: Option<SelectionBounds>,
}
impl PixelSelection {
    pub fn at(&self, x: u32, y: u32) -> u8 {
        self.pixels[y as usize * self.width as usize + x as usize]
    }
    pub fn contains(&self, p: Point) -> bool {
        p.x >= 0.
            && p.y >= 0.
            && (p.x as u32) < self.width
            && (p.y as u32) < self.height
            && self.at(p.x as u32, p.y as u32) > 0
    }
}
#[derive(Clone, Debug)]
pub struct SelectionDraft {
    pub points: Vec<Point>,
    pub marquee: Option<MarqueeKind>,
    pub mode: PixelSelectionMode,
}
impl SelectionDraft {
    pub fn new(point: Point, marquee: Option<MarqueeKind>, mode: PixelSelectionMode) -> Self {
        Self {
            points: vec![point],
            marquee,
            mode,
        }
    }
    pub fn drag(&mut self, point: Point, square: bool) {
        if self.marquee.is_some() {
            self.points.truncate(1);
            let anchor = self.points[0];
            let mut dx = point.x.round() - anchor.x.round();
            let mut dy = point.y.round() - anchor.y.round();
            if square {
                let side = dx.abs().max(dy.abs());
                dx = dx.signum() * side;
                dy = dy.signum() * side;
            }
            self.points
                .push(Point::new(anchor.x.round() + dx, anchor.y.round() + dy));
        } else if self.points.last().is_none_or(|p| p.distance(point) >= 0.25) {
            self.points.push(point);
        }
    }
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        if self.points.len() < 2 {
            return None;
        }
        let x0 = self
            .points
            .iter()
            .map(|p| p.x)
            .fold(f64::INFINITY, f64::min);
        let y0 = self
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::INFINITY, f64::min);
        let x1 = self
            .points
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let y1 = self
            .points
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max);
        (x1 > x0 && y1 > y0 && (self.marquee.is_some() || self.points.len() >= 3))
            .then_some((x0, y0, x1, y1))
    }
}
fn inside(draft: &SelectionDraft, x: f64, y: f64, bounds: (f64, f64, f64, f64)) -> bool {
    if let Some(kind) = draft.marquee {
        let (x0, y0, x1, y1) = bounds;
        return match kind {
            MarqueeKind::Rectangle => x >= x0 && x < x1 && y >= y0 && y < y1,
            MarqueeKind::Ellipse => {
                let cx = (x0 + x1) / 2.;
                let cy = (y0 + y1) / 2.;
                let rx = (x1 - x0) / 2.;
                let ry = (y1 - y0) / 2.;
                ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.
            }
        };
    }
    let mut hit = false;
    let mut prev = *draft.points.last().unwrap();
    for point in &draft.points {
        if (point.y > y) != (prev.y > y)
            && x < (prev.x - point.x) * (y - point.y) / (prev.y - point.y) + point.x
        {
            hit = !hit;
        }
        prev = *point;
    }
    hit
}
/// Four samples per pixel retain antialiased edges; an explicit empty raster touches nothing.
pub fn finish(
    doc: &Document,
    old: Option<&PixelSelection>,
    draft: &SelectionDraft,
) -> Result<Option<PixelSelection>> {
    let bounds = draft.bounds();
    if bounds.is_none() && draft.mode != PixelSelectionMode::Replace {
        return Ok(old.cloned());
    }
    let mut pixels = if draft.mode == PixelSelectionMode::Replace {
        vec![0; doc.width as usize * doc.height as usize]
    } else {
        old.map(|v| v.pixels.as_ref().clone())
            .unwrap_or_else(|| vec![0; doc.width as usize * doc.height as usize])
    };
    if let Some(bounds) = bounds {
        let (x0, y0, x1, y1) = bounds;
        ensure!(
            x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite(),
            "Invalid selection outline"
        );
        let left = (x0.floor() as i64 - 1).clamp(0, doc.width as i64) as usize;
        let top = (y0.floor() as i64 - 1).clamp(0, doc.height as i64) as usize;
        let right = (x1.ceil() as i64 + 1).clamp(0, doc.width as i64) as usize;
        let bottom = (y1.ceil() as i64 + 1).clamp(0, doc.height as i64) as usize;
        for y in top..bottom {
            for x in left..right {
                let coverage = [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                    .iter()
                    .filter(|(sx, sy)| inside(draft, x as f64 + sx, y as f64 + sy, bounds))
                    .count() as u16
                    * 255
                    / 4;
                let at = y * doc.width as usize + x;
                let prior = pixels[at] as u16;
                pixels[at] = match draft.mode {
                    PixelSelectionMode::Replace => coverage as u8,
                    PixelSelectionMode::Add => {
                        (prior + coverage - prior * coverage / 255).min(255) as u8
                    }
                    PixelSelectionMode::Subtract => (prior * (255 - coverage) / 255) as u8,
                };
            }
        }
    }
    let mut minx = doc.width;
    let mut miny = doc.height;
    let mut maxx = 0;
    let mut maxy = 0;
    for y in 0..doc.height {
        for x in 0..doc.width {
            if pixels[y as usize * doc.width as usize + x as usize] != 0 {
                minx = minx.min(x);
                miny = miny.min(y);
                maxx = maxx.max(x + 1);
                maxy = maxy.max(y + 1);
            }
        }
    }
    let bounds = (maxx > minx && maxy > miny).then_some(SelectionBounds {
        x: minx,
        y: miny,
        width: maxx - minx,
        height: maxy - miny,
    });
    Ok(Some(PixelSelection {
        width: doc.width,
        height: doc.height,
        pixels: Arc::new(pixels),
        bounds,
    }))
}
