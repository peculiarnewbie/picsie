//! MagicWand.swift / WandPixels.c translated from Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Rust vectors replace C allocation; Skia owns paths.
use crate::model::{Point, dimensions};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use skia_safe::{Path, PathBuilder};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WandSettings {
    pub tolerance: u8,
    /// Point / 3×3 / 5×5 average: radius 0 / 1 / 2.
    pub radius: u8,
    pub contiguous: bool,
    pub sample_all_layers: bool,
}
impl Default for WandSettings {
    fn default() -> Self {
        Self {
            tolerance: 32,
            radius: 0,
            contiguous: true,
            sample_all_layers: false,
        }
    }
}
/// Input is premultiplied RGBA, as in BrushRaster.context / wand_mask.
pub fn select(
    rgba: &[u8],
    width: u32,
    height: u32,
    point: Point,
    settings: WandSettings,
) -> Result<Option<Path>> {
    dimensions(width, height)?;
    ensure!(
        rgba.len() == width as usize * height as usize * 4 && settings.radius <= 2,
        "Invalid wand sample"
    );
    if !point.x.is_finite()
        || !point.y.is_finite()
        || point.x < 0.
        || point.y < 0.
        || point.x >= width as f64
        || point.y >= height as f64
    {
        return Ok(None);
    }
    let (w, h) = (width as usize, height as usize);
    let (sx, sy, r) = (
        point.x.floor() as usize,
        point.y.floor() as usize,
        settings.radius as usize,
    );
    let mut sums = [0u64; 4];
    let mut count = 0;
    for y in sy.saturating_sub(r)..=(sy + r).min(h - 1) {
        for x in sx.saturating_sub(r)..=(sx + r).min(w - 1) {
            for c in 0..4 {
                sums[c] += rgba[(y * w + x) * 4 + c] as u64;
            }
            count += 1;
        }
    }
    let reference = sums.map(|s| ((s + count / 2) / count) as i32);
    let matches = |x: usize, y: usize| {
        (0..4).all(|c| {
            (rgba[(y * w + x) * 4 + c] as i32 - reference[c]).abs() <= settings.tolerance as i32
        })
    };
    let mut mask = vec![0; w * h];
    if !settings.contiguous {
        for y in 0..h {
            for x in 0..w {
                if matches(x, y) {
                    mask[y * w + x] = 255;
                }
            }
        }
    } else {
        let mut stack = vec![(sx, sy)];
        while let Some((x, y)) = stack.pop() {
            if mask[y * w + x] != 0 || !matches(x, y) {
                continue;
            }
            let (mut left, mut right) = (x, x);
            while left > 0 && mask[y * w + left - 1] == 0 && matches(left - 1, y) {
                left -= 1;
            }
            while right + 1 < w && mask[y * w + right + 1] == 0 && matches(right + 1, y) {
                right += 1;
            }
            mask[y * w + left..=y * w + right].fill(255);
            for ny in [y.checked_sub(1), (y + 1 < h).then_some(y + 1)]
                .into_iter()
                .flatten()
            {
                let mut in_run = false;
                for nx in left..=right {
                    let candidate = mask[ny * w + nx] == 0 && matches(nx, ny);
                    if candidate && !in_run {
                        stack.push((nx, ny));
                    }
                    in_run = candidate;
                }
            }
        }
    }
    outline(&mask, width, height)
}
/// Exact directed pixel boundaries, including holes and diagonal corner contacts.
pub fn outline(mask: &[u8], width: u32, height: u32) -> Result<Option<Path>> {
    dimensions(width, height)?;
    let (w, h) = (width as usize, height as usize);
    ensure!(mask.len() == w * h, "Invalid wand coverage");
    let stride = w + 1;
    let mut out = vec![0u8; stride * (h + 1)];
    let mut edges = 0usize;
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] == 0 {
                continue;
            }
            if y == 0 || mask[(y - 1) * w + x] == 0 {
                out[y * stride + x] |= 1;
                edges += 1;
            }
            if x + 1 == w || mask[y * w + x + 1] == 0 {
                out[y * stride + x + 1] |= 2;
                edges += 1;
            }
            if y + 1 == h || mask[(y + 1) * w + x] == 0 {
                out[(y + 1) * stride + x + 1] |= 4;
                edges += 1;
            }
            if x == 0 || mask[y * w + x - 1] == 0 {
                out[(y + 1) * stride + x] |= 8;
                edges += 1;
            }
        }
        ensure!(
            edges <= 8_000_000,
            "That selection is too detailed to outline. Try a different Tolerance, or turn on Contiguous."
        );
    }
    if edges == 0 {
        return Ok(None);
    }
    let mut path = PathBuilder::new();
    for start in 0..out.len() {
        while out[start] != 0 {
            let (mut v, mut heading, mut initial) = (start, 0u8, 0u8);
            let mut corners = Vec::new();
            loop {
                let bits = out[v];
                let right = if heading == 8 { 1 } else { heading << 1 };
                let left = if heading == 1 { 8 } else { heading >> 1 };
                let d = if heading == 0 {
                    bits & bits.wrapping_neg()
                } else if bits & right != 0 {
                    right
                } else if bits & heading != 0 {
                    heading
                } else if bits & left != 0 {
                    left
                } else {
                    bits & bits.wrapping_neg()
                };
                ensure!(d != 0, "Cannot trace wand outline");
                out[v] &= !d;
                if d != heading {
                    corners.push(((v % stride) as f32, (v / stride) as f32));
                }
                if heading == 0 {
                    initial = d;
                }
                heading = d;
                v = match d {
                    1 => v + 1,
                    2 => v + stride,
                    4 => v - 1,
                    8 => v - stride,
                    _ => unreachable!(),
                };
                if v == start {
                    break;
                }
            }
            if heading == initial && !corners.is_empty() {
                corners.remove(0);
            }
            if let Some(first) = corners.first() {
                path.move_to(*first);
                for p in &corners[1..] {
                    path.line_to(*p);
                }
                path.close();
            }
        }
    }
    Ok(Some(path.detach()))
}
