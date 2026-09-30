//! DocumentSelection / DragBox / LassoDraft coverage translated from the pinned Compositor.
//! MIT © 2026 Wonder Assembly LLC. Raster union is an adaptation for the Rust surface model.
use crate::model::{Document, Point};
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use skia_safe::{self as sk, Path, PathOp};
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
/// Grayscale coverage at document resolution standing in for Compositor's `DocumentSelection`
/// path. `feather` carries the same metadata as upstream: the stored coverage stays crisp and
/// `render::selection_coverage` softens it by `feather / 2` whenever edits or the overlay read it.
#[derive(Clone, Debug, PartialEq)]
pub struct PixelSelection {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
    /// Upstream document-space outline; raster coverage is a derived cache.
    pub outline: Path,
    pub bounds: Option<SelectionBounds>,
    /// How far the edge fades, in document pixels. 0 is a hard edge.
    pub feather: f64,
}
impl PixelSelection {
    /// Selection.swift::moveSelection: retain the full outline when it leaves the canvas.
    pub fn transformed(&self, matrix: &sk::Matrix) -> Result<Self> {
        Self::from_path(
            self.width,
            self.height,
            self.outline.with_transform(matrix),
            self.feather,
        )
    }
    pub fn translated(&self, delta: Point) -> Result<Self> {
        self.transformed(&sk::Matrix::translate((
            delta.x.round() as f32,
            delta.y.round() as f32,
        )))
    }
    pub fn combined(&self, outline: &Path, mode: PixelSelectionMode) -> Result<Self> {
        let clipped = operation(
            outline,
            &canvas_path(self.width, self.height),
            PathOp::Intersect,
        )?;
        let path = match mode {
            PixelSelectionMode::Replace => clipped,
            PixelSelectionMode::Add => operation(&self.outline, &clipped, PathOp::Union)?,
            PixelSelectionMode::Subtract => operation(&self.outline, &clipped, PathOp::Difference)?,
        };
        Self::from_path(self.width, self.height, path, 0.)
    }
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
    /// The region `SelectionClip` covers: upstream `coverageBounds` (four Gaussian standard
    /// deviations past the outline, so `ceil(feather * 2)`) plus `clip`'s one-pixel
    /// antialias allowance, clipped to the canvas as `clip` does.
    pub fn coverage_bounds(&self) -> Option<SelectionBounds> {
        let bounds = self.bounds.as_ref()?;
        let grow = (self.feather * 2.).ceil() as i64 + 1;
        let x0 = (bounds.x as i64 - grow).clamp(0, self.width as i64);
        let y0 = (bounds.y as i64 - grow).clamp(0, self.height as i64);
        let x1 = (bounds.x as i64 + bounds.width as i64 + grow).clamp(0, self.width as i64);
        let y1 = (bounds.y as i64 + bounds.height as i64 + grow).clamp(0, self.height as i64);
        (x1 > x0 && y1 > y0).then(|| SelectionBounds {
            x: x0 as u32,
            y: y0 as u32,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
        })
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
/// Selection.swift::applySelection / finishLasso; Skia path operations replace CGPath.
pub fn finish(
    doc: &Document,
    old: Option<&PixelSelection>,
    draft: &SelectionDraft,
) -> Result<Option<PixelSelection>> {
    let Some((x0, y0, x1, y1)) = draft.bounds() else {
        return Ok(if draft.mode == PixelSelectionMode::Replace {
            None
        } else {
            old.cloned()
        });
    };
    ensure!(
        [x0, y0, x1, y1].iter().all(|v| v.is_finite()),
        "Invalid selection outline"
    );
    if old.is_none() && draft.mode == PixelSelectionMode::Subtract {
        return Ok(None);
    }
    let mut path = sk::PathBuilder::new();
    let rect = sk::Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32);
    match draft.marquee {
        Some(MarqueeKind::Rectangle) => {
            path.add_rect(rect, None, None);
        }
        Some(MarqueeKind::Ellipse) => {
            path.add_oval(rect, None, None);
        }
        None => {
            path.move_to((draft.points[0].x as f32, draft.points[0].y as f32));
            for p in &draft.points[1..] {
                path.line_to((p.x as f32, p.y as f32));
            }
            path.close();
        }
    }
    let clipped = operation(
        &path.detach(),
        &canvas_path(doc.width, doc.height),
        PathOp::Intersect,
    )?;
    let result = match (draft.mode, old) {
        (PixelSelectionMode::Add, Some(old)) => operation(&old.outline, &clipped, PathOp::Union)?,
        (PixelSelectionMode::Subtract, Some(old)) => {
            operation(&old.outline, &clipped, PathOp::Difference)?
        }
        _ => clipped,
    };
    Ok(Some(PixelSelection::from_path(
        doc.width, doc.height, result, 0.,
    )?))
}
fn operation(a: &Path, b: &Path, op: PathOp) -> Result<Path> {
    a.op(b, op)
        .ok_or_else(|| anyhow!("Cannot combine selection outlines"))
}
fn canvas_path(width: u32, height: u32) -> Path {
    Path::rect(sk::Rect::from_wh(width as f32, height as f32), None)
}
impl PixelSelection {
    pub fn all(width: u32, height: u32) -> Result<Self> {
        Self::from_path(width, height, canvas_path(width, height), 0.)
    }
    pub fn inverted(&self) -> Result<Self> {
        Self::from_path(
            self.width,
            self.height,
            operation(
                &canvas_path(self.width, self.height),
                &self.outline,
                PathOp::Difference,
            )?,
            self.feather,
        )
    }
    /// Selection.swift::resizeSelection: union/subtract a round stroked band, then clip.
    pub fn resized(&self, delta: f64) -> Result<Self> {
        ensure!(
            delta.is_finite() && delta.abs() >= 1. && delta.abs() <= 500.,
            "Selection amount must be between 1 and 500 pixels"
        );
        let mut paint = sk::Paint::default();
        paint
            .set_style(sk::paint::Style::Stroke)
            .set_stroke_width((delta.abs() * 2.) as f32)
            .set_stroke_cap(sk::paint::Cap::Round)
            .set_stroke_join(sk::paint::Join::Round);
        let mut band = sk::PathBuilder::new();
        ensure!(
            sk::path_utils::fill_path_with_paint(&self.outline, &paint, &mut band, None, None),
            "Cannot resize selection outline"
        );
        let result = operation(
            &self.outline,
            &band.detach(),
            if delta > 0. {
                PathOp::Union
            } else {
                PathOp::Difference
            },
        )?;
        let result = operation(
            &result,
            &canvas_path(self.width, self.height),
            PathOp::Intersect,
        )?;
        Self::from_path(self.width, self.height, result, self.feather)
    }
    pub fn from_path(width: u32, height: u32, outline: Path, feather: f64) -> Result<Self> {
        crate::model::dimensions(width, height)?;
        let info = sk::ImageInfo::new(
            (width as i32, height as i32),
            sk::ColorType::Alpha8,
            sk::AlphaType::Premul,
            None,
        );
        let mut surface = sk::surfaces::raster(&info, None, None)
            .ok_or_else(|| anyhow!("Cannot allocate selection coverage"))?;
        let mut paint = sk::Paint::default();
        paint.set_anti_alias(true).set_color(sk::Color::WHITE);
        surface.canvas().draw_path(&outline, &paint);
        let mut pixels = vec![0u8; width as usize * height as usize];
        ensure!(
            surface.read_pixels(&info, &mut pixels, width as usize, (0, 0)),
            "Cannot read selection coverage"
        );
        let (mut minx, mut miny, mut maxx, mut maxy) = (width, height, 0, 0);
        for (i, value) in pixels.iter().enumerate() {
            if *value > 0 {
                let x = i as u32 % width;
                let y = i as u32 / width;
                minx = minx.min(x);
                miny = miny.min(y);
                maxx = maxx.max(x + 1);
                maxy = maxy.max(y + 1);
            }
        }
        let bounds = (maxx > minx && maxy > miny).then_some(SelectionBounds {
            x: minx,
            y: miny,
            width: maxx.saturating_sub(minx),
            height: maxy.saturating_sub(miny),
        });
        Ok(Self {
            width,
            height,
            pixels: Arc::new(pixels),
            outline,
            bounds,
            feather,
        })
    }
}
