//! BrushStroke.swift (software path) and EditorSession+Brush.swift, pinned 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Skia/native buffers replace CoreGraphics tile images.
use crate::{geometry, model::*, pixel_selection::PixelSelection, render};
use anyhow::{Result, ensure};
use std::{collections::HashMap, sync::Arc};
const TILE: i32 = 256;
type Tiles = HashMap<(i32, i32), Arc<Vec<f32>>>;
#[derive(Clone, Copy)]
pub struct Settings {
    pub diameter: f64,
    pub hardness: f64,
    pub opacity: f64,
    pub smoothing: f64,
    pub erase: bool,
    pub color: [u8; 3],
}
pub struct BrushStroke {
    original: Layer,
    grid: Layer,
    doc: Document,
    mask: bool,
    settings: Settings,
    source: Vec<u8>,
    selection: Option<Arc<Vec<u8>>>,
    tiles: Tiles,
    tail: Option<Tiles>,
    samples: Vec<Point>,
    previous: Option<Point>,
    next_distance: f64,
    anchor: Option<Point>,
    pointer: Option<Point>,
    zoom: f64,
    finished: bool,
}
impl BrushStroke {
    pub fn new(
        layer: Layer,
        doc: &Document,
        selection: Option<&PixelSelection>,
        mask: bool,
        settings: Settings,
        zoom: f64,
    ) -> Result<Self> {
        ensure!(
            settings.diameter.is_finite()
                && (1. ..=2000.).contains(&settings.diameter)
                && (0. ..=1.).contains(&settings.hardness)
                && (0. ..=1.).contains(&settings.opacity)
                && (0. ..=100.).contains(&settings.smoothing),
            "Invalid brush settings"
        );
        let grid = if mask {
            layer
                .mask
                .as_ref()
                .and_then(|m| m.placement)
                .map(|p| p.as_layer(&layer))
                .unwrap_or_else(|| layer.clone())
        } else {
            layer.clone()
        };
        let source = if mask {
            render::rasterize_mask(layer.mask.as_ref().expect("validated mask target"), &layer)?
                .pixels
                .as_ref()
                .clone()
        } else if matches!(layer.content.as_ref(), Content::Paint) && layer.strokes.is_empty() {
            vec![]
        } else {
            let mut bare = layer.clone();
            bare.mask = None;
            render::rgba_pixels(&render::Renderer::default().layer_surface(&bare)?)?
        };
        Ok(Self {
            original: layer,
            grid,
            doc: doc.clone(),
            mask,
            settings,
            source,
            selection: selection.map(render::selection_coverage).transpose()?,
            tiles: HashMap::new(),
            tail: None,
            samples: vec![],
            previous: None,
            next_distance: 0.,
            anchor: None,
            pointer: None,
            zoom,
            finished: false,
        })
    }
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    pub fn last_point(&self) -> Option<Point> {
        self.anchor
    }
    pub fn input(&mut self, point: Point) -> Result<()> {
        self.finished = false;
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > 10_000_000.
            || point.y.abs() > 10_000_000.
        {
            return Ok(());
        }
        self.pointer = Some(point);
        let Some(anchor) = self.anchor else {
            self.anchor = Some(point);
            return self.append(point);
        };
        let radius = self.settings.smoothing / self.zoom.max(0.01);
        let distance = anchor.distance(point);
        if radius > 0. {
            if distance <= radius {
                return Ok(());
            }
            let t = (distance - radius) / distance;
            let moved = Point::new(
                anchor.x + (point.x - anchor.x) * t,
                anchor.y + (point.y - anchor.y) * t,
            );
            self.anchor = Some(moved);
            self.append(moved)
        } else {
            self.anchor = Some(point);
            self.append(point)
        }
    }
    pub fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        if self.settings.smoothing > 0.
            && let Some(pointer) = self.pointer
        {
            self.append(pointer)?;
        }
        if let Some(tail) = self.tail.take() {
            self.tiles = tail;
        }
        let n = self.samples.len();
        if n >= 2 {
            self.curve(
                self.samples[n - 2],
                self.samples[n - 1],
                self.samples[n.saturating_sub(3)],
                self.samples[n - 1],
            )?;
            self.samples = vec![self.samples[n - 1]];
        }
        self.finished = true;
        Ok(())
    }
    fn append(&mut self, p: Point) -> Result<()> {
        if self.samples.last() == Some(&p) {
            return Ok(());
        }
        if let Some(tail) = self.tail.take() {
            self.tiles = tail;
        }
        self.samples.push(p);
        if self.samples.len() > 4 {
            self.samples.remove(0);
        }
        let n = self.samples.len();
        if n == 1 {
            self.walk(p)?;
        } else if n >= 3 {
            self.curve(
                self.samples[n - 3],
                self.samples[n - 2],
                self.samples[n.saturating_sub(4)],
                self.samples[n - 1],
            )?;
        }
        if n >= 2 {
            self.tail = Some(self.tiles.clone());
            let saved = (self.previous, self.next_distance);
            self.walk(p)?;
            (self.previous, self.next_distance) = saved;
        }
        Ok(())
    }
    fn curve(&mut self, start: Point, end: Point, before: Point, after: Point) -> Result<()> {
        fn mix(a: Point, b: Point, ta: f64, tb: f64, t: f64) -> Point {
            let wa = (tb - t) / (tb - ta);
            let wb = (t - ta) / (tb - ta);
            Point::new(a.x * wa + b.x * wb, a.y * wa + b.y * wb)
        }
        let t0 = 0.;
        let t1 = t0 + before.distance(start).sqrt().max(0.0001);
        let t2 = t1 + start.distance(end).sqrt().max(0.0001);
        let t3 = t2 + end.distance(after).sqrt().max(0.0001);
        let pieces = (start.distance(end) / 2.).ceil().max(1.) as usize;
        for i in 1..=pieces {
            let t = t1 + (t2 - t1) * i as f64 / pieces as f64;
            let a1 = mix(before, start, t0, t1, t);
            let a2 = mix(start, end, t1, t2, t);
            let a3 = mix(end, after, t2, t3, t);
            let b1 = mix(a1, a2, t0, t2, t);
            let b2 = mix(a2, a3, t1, t3, t);
            self.walk(if i == pieces {
                end
            } else {
                mix(b1, b2, t1, t2, t)
            })?;
        }
        Ok(())
    }
    fn walk(&mut self, p: Point) -> Result<()> {
        let spacing = (self.settings.diameter
            * if self.settings.hardness >= 1. {
                0.015
            } else {
                0.025
            })
        .max(0.25);
        if let Some(previous) = self.previous {
            let length = previous.distance(p);
            if length > 0. {
                let mut distance = self.next_distance;
                while distance <= length {
                    self.dab(Point::new(
                        previous.x + (p.x - previous.x) * distance / length,
                        previous.y + (p.y - previous.y) * distance / length,
                    ))?;
                    distance += spacing;
                }
                self.next_distance = distance - length;
            }
        } else {
            self.dab(p)?;
            self.next_distance = spacing;
        }
        self.previous = Some(p);
        Ok(())
    }
    fn dab(&mut self, p: Point) -> Result<()> {
        let r = self.settings.diameter / 2.;
        let (left, top, right, bottom) = (
            (p.x - r).max(0.),
            (p.y - r).max(0.),
            (p.x + r).min(self.doc.width as f64),
            (p.y + r).min(self.doc.height as f64),
        );
        if right <= left || bottom <= top {
            return Ok(());
        }
        let corners = [(left, top), (right, top), (right, bottom), (left, bottom)]
            .map(|(x, y)| geometry::to_local(&self.grid, Point::new(x, y)));
        let mut x0 = corners
            .iter()
            .map(|p| p.x.floor())
            .fold(f64::INFINITY, f64::min) as i32;
        let mut y0 = corners
            .iter()
            .map(|p| p.y.floor())
            .fold(f64::INFINITY, f64::min) as i32;
        let mut x1 = corners
            .iter()
            .map(|p| p.x.ceil())
            .fold(f64::NEG_INFINITY, f64::max) as i32;
        let mut y1 = corners
            .iter()
            .map(|p| p.y.ceil())
            .fold(f64::NEG_INFINITY, f64::max) as i32;
        if self.mask || self.settings.erase {
            x0 = x0.max(0);
            y0 = y0.max(0);
            x1 = x1.min(self.grid.width as i32);
            y1 = y1.min(self.grid.height as i32);
        }
        if x1 <= x0 || y1 <= y0 {
            return Ok(());
        }
        dimensions((x1 - x0) as u32, (y1 - y0) as u32)?;
        if !self.source.is_empty() && !self.mask {
            dimensions(
                (x1.max(self.grid.width as i32) - x0.min(0)) as u32,
                (y1.max(self.grid.height as i32) - y0.min(0)) as u32,
            )?;
        }
        let hard = self.settings.hardness >= 1.;
        for ty in y0.div_euclid(TILE)..=(y1 - 1).div_euclid(TILE) {
            for tx in x0.div_euclid(TILE)..=(x1 - 1).div_euclid(TILE) {
                // Sparse COW tiles also back up the provisional tail without copying untouched coverage.
                ensure!(
                    self.tiles.len() <= MAX_PIXELS as usize / (TILE * TILE) as usize,
                    "Brush exceeds the pixel budget"
                );
                let tile = Arc::make_mut(
                    self.tiles
                        .entry((tx, ty))
                        .or_insert_with(|| Arc::new(vec![0.; (TILE * TILE) as usize])),
                );
                for y in y0.max(ty * TILE)..y1.min((ty + 1) * TILE) {
                    for x in x0.max(tx * TILE)..x1.min((tx + 1) * TILE) {
                        let world = geometry::to_world(
                            &self.grid,
                            Point::new(x as f64 + 0.5, y as f64 + 0.5),
                        );
                        if render::edit_coverage(&self.doc, None, world) == 0 {
                            continue;
                        }
                        let distance = world.distance(p) / r;
                        let amount = if hard {
                            // CoreGraphics ellipse antialiasing is approximated by four native samples.
                            [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                                .iter()
                                .filter(|(dx, dy)| {
                                    geometry::to_world(
                                        &self.grid,
                                        Point::new(x as f64 + dx, y as f64 + dy),
                                    )
                                    .distance(p)
                                        <= r
                                })
                                .count() as f32
                                / 4.
                        } else if distance >= 1. {
                            0.
                        } else if distance <= self.settings.hardness {
                            1.
                        } else {
                            let u =
                                (distance - self.settings.hardness) / (1. - self.settings.hardness);
                            (((-2.5 * u * u).exp() - (-2.5f64).exp()) / (1. - (-2.5f64).exp()))
                                as f32
                        };
                        let at = ((y - ty * TILE) * TILE + x - tx * TILE) as usize;
                        tile[at] = if hard {
                            tile[at].max(amount)
                        } else {
                            1. - (1. - tile[at]) * (1. - amount)
                        };
                    }
                }
            }
        }
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Layer> {
        let mut extent: Option<(i32, i32, i32, i32)> = None;
        if !self.source.is_empty() {
            extent = Some((0, 0, self.grid.width as i32, self.grid.height as i32));
        }
        let mut touched = false;
        for (&(tx, ty), tile) in &self.tiles {
            for (i, &amount) in tile.iter().enumerate() {
                if amount <= 0. {
                    continue;
                }
                let x = tx * TILE + i as i32 % TILE;
                let y = ty * TILE + i as i32 / TILE;
                let world =
                    geometry::to_world(&self.grid, Point::new(x as f64 + 0.5, y as f64 + 0.5));
                if render::edit_coverage(
                    &self.doc,
                    self.selection.as_deref().map(Vec::as_slice),
                    world,
                ) == 0
                {
                    continue;
                }
                touched = true;
                extent = Some(match extent {
                    Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x + 1), d.max(y + 1)),
                    None => (x, y, x + 1, y + 1),
                });
            }
        }
        if !touched
            || self.settings.opacity == 0.
            || (self.source.is_empty() && self.settings.erase)
        {
            return Ok(self.original.clone());
        }
        let (left, top, right, bottom) = extent.expect("touched pixels");
        let (width, height) = ((right - left) as u32, (bottom - top) as u32);
        dimensions(width, height)?;
        let mut next = self.original.clone();
        if self.mask {
            let mut pixels = self.source.clone();
            for (&(tx, ty), tile) in &self.tiles {
                for (i, &amount) in tile.iter().enumerate() {
                    if amount <= 0. {
                        continue;
                    }
                    let x = tx * TILE + i as i32 % TILE;
                    let y = ty * TILE + i as i32 / TILE;
                    if x < 0 || y < 0 || x >= self.grid.width as i32 || y >= self.grid.height as i32
                    {
                        continue;
                    }
                    let world =
                        geometry::to_world(&self.grid, Point::new(x as f64 + 0.5, y as f64 + 0.5));
                    let a = amount as f64
                        * self.settings.opacity
                        * render::edit_coverage(
                            &self.doc,
                            self.selection.as_deref().map(Vec::as_slice),
                            world,
                        ) as f64
                        / 255.;
                    let at = y as usize * self.grid.width as usize + x as usize;
                    pixels[at] = (self.source[at] as f64 * (1. - a)
                        + self.settings.color[0] as f64 * a)
                        .round() as u8;
                }
            }
            let mut mask = next.mask.as_ref().unwrap().as_ref().clone();
            mask.raster = Some(Arc::new(MaskRaster {
                width: self.grid.width,
                height: self.grid.height,
                pixels: Arc::new(pixels),
            }));
            mask.strokes.clear();
            next.mask = Some(Arc::new(mask));
            return Ok(next);
        }
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        if !self.source.is_empty() {
            for y in 0..self.grid.height as usize {
                let at = ((y as i32 - top) as usize * width as usize + (-left) as usize) * 4;
                pixels[at..at + self.grid.width as usize * 4].copy_from_slice(
                    &self.source
                        [y * self.grid.width as usize * 4..(y + 1) * self.grid.width as usize * 4],
                );
            }
        }
        for (&(tx, ty), tile) in &self.tiles {
            for (i, &amount) in tile.iter().enumerate() {
                if amount <= 0. {
                    continue;
                }
                let x = tx * TILE + i as i32 % TILE;
                let y = ty * TILE + i as i32 / TILE;
                if x < left || y < top || x >= right || y >= bottom {
                    continue;
                }
                let world =
                    geometry::to_world(&self.grid, Point::new(x as f64 + 0.5, y as f64 + 0.5));
                let a = amount as f64
                    * self.settings.opacity
                    * render::edit_coverage(
                        &self.doc,
                        self.selection.as_deref().map(Vec::as_slice),
                        world,
                    ) as f64
                    / 255.;
                let at = ((y - top) as usize * width as usize + (x - left) as usize) * 4;
                render::composite_pixel(
                    &mut pixels[at..at + 4],
                    self.settings.color,
                    a,
                    self.settings.erase,
                );
            }
        }
        let center = geometry::to_world(
            &self.grid,
            Point::new((left + right) as f64 / 2., (top + bottom) as f64 / 2.),
        );
        next.width = width;
        next.height = height;
        next.x = center.x - width as f64 * next.scale_x / 2.;
        next.y = center.y - height as f64 * next.scale_y / 2.;
        if let Some(mask) = &mut next.mask {
            let mask = Arc::make_mut(mask);
            let mut placement = mask.placement.unwrap_or(MaskPlacement::of(&self.original));
            placement.scale_x *= self.original.width as f64 / width as f64;
            placement.scale_y *= self.original.height as f64 / height as f64;
            mask.placement = Some(placement);
        }
        next.content = Arc::new(render::native_content(render::rgba_image(
            width, height, &pixels,
        )?));
        next.strokes.clear();
        Ok(next)
    }
}
