//! BrushStroke.swift (software path) and EditorSession+Brush.swift, pinned 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Skia/native buffers replace CoreGraphics tile images.
use crate::{
    asset::ImageAsset,
    geometry,
    model::*,
    pixel_selection::PixelSelection,
    raster_snapshot::{RasterPatch, RasterSnapshot},
    render,
};
use anyhow::{Result, ensure};
use std::{cell::RefCell, collections::HashMap, sync::Arc};
const TILE: i32 = 256;
type Tiles = HashMap<(i32, i32), Arc<Vec<f32>>>;
struct PublishedTile {
    coverage: Arc<Vec<f32>>,
    base: Arc<Vec<u8>>,
    pixels: Arc<Vec<u8>>,
    clip: Option<Arc<Vec<u8>>>,
    patch: Option<RasterPatch>,
    extent: Option<skia_safe::IRect>,
}
#[derive(Default)]
struct Publication {
    tiles: HashMap<(i32, i32), PublishedTile>,
    layer: Option<Layer>,
}
struct Tip {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    pixels: Vec<f32>,
}
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
    source_asset: Option<ImageAsset>,
    publication: RefCell<Publication>,
    mapping: geometry::PixelMapping,
    tips: HashMap<(u64, u64), Arc<Tip>>,
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
        } else {
            vec![]
        };
        let source_asset = if mask
            || (matches!(layer.content.as_ref(), Content::Paint) && layer.strokes.is_empty())
        {
            None
        } else if let Content::Image { data } = layer.content.as_ref()
            && layer.strokes.is_empty()
        {
            Some(match data {
                ImageAsset::Encoded(_) => ImageAsset::Raster(Arc::new(data.image()?)),
                _ => data.clone(),
            })
        } else {
            let mut bare = layer.clone();
            bare.mask = None;
            Some(ImageAsset::Raster(Arc::new(
                render::Renderer::default().layer_surface(&bare)?,
            )))
        };
        let mapping = geometry::PixelMapping::new(&grid);
        Ok(Self {
            original: layer,
            grid,
            doc: doc.clone(),
            mask,
            settings,
            source,
            source_asset,
            publication: RefCell::default(),
            mapping,
            tips: HashMap::new(),
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
        // BrushStroke.stampLimit/gridTip: reuse the existing software tip on
        // exact quarter-pixel phases for an untransformed integer-origin grid.
        // Arbitrary phases/transforms retain procedural coverage, without phase
        // quantization or a rasterization change. At most sixteen tips are held.
        let phase = Point::new(p.x - p.x.floor(), p.y - p.y.floor());
        let cached = if self.settings.diameter <= 160.
            && self.grid.rotation == 0.
            && self.grid.scale_x == 1.
            && self.grid.scale_y == 1.
            && !self.grid.flip_x
            && !self.grid.flip_y
            && self.grid.x.fract() == 0.
            && self.grid.y.fract() == 0.
            && (phase.x * 4.).fract() == 0.
            && (phase.y * 4.).fract() == 0.
        {
            let key = (phase.x.to_bits(), phase.y.to_bits());
            Some(
                self.tips
                    .entry(key)
                    .or_insert_with(|| {
                        let (left, top) =
                            ((phase.x - r).floor() as i32, (phase.y - r).floor() as i32);
                        let (width, height) = (
                            (phase.x + r).ceil() as i32 - left,
                            (phase.y + r).ceil() as i32 - top,
                        );
                        let mut pixels = Vec::with_capacity((width * height) as usize);
                        for y in top..top + height {
                            for x in left..left + width {
                                let amount = if self.settings.hardness >= 1. {
                                    [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                                        .iter()
                                        .filter(|&&(dx, dy)| {
                                            Point::new(x as f64 + dx, y as f64 + dy).distance(phase)
                                                <= r
                                        })
                                        .count() as f32
                                        / 4.
                                } else {
                                    let distance = Point::new(x as f64 + 0.5, y as f64 + 0.5)
                                        .distance(phase)
                                        / r;
                                    if distance >= 1. {
                                        0.
                                    } else if distance <= self.settings.hardness {
                                        1.
                                    } else {
                                        let u = (distance - self.settings.hardness)
                                            / (1. - self.settings.hardness);
                                        (((-2.5 * u * u).exp() - (-2.5f64).exp())
                                            / (1. - (-2.5f64).exp()))
                                            as f32
                                    }
                                };
                                pixels.push(amount);
                            }
                        }
                        Arc::new(Tip {
                            left,
                            top,
                            width,
                            height,
                            pixels,
                        })
                    })
                    .clone(),
            )
        } else {
            None
        };
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
        if self.source_asset.is_some() && !self.mask {
            dimensions(
                (x1.max(self.grid.width as i32) - x0.min(0)) as u32,
                (y1.max(self.grid.height as i32) - y0.min(0)) as u32,
            )?;
        }
        let hard = self.settings.hardness >= 1.;
        let unit_pixels =
            self.grid.rotation == 0. && self.grid.scale_x == 1. && self.grid.scale_y == 1.;
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
                        let index = ((y - ty * TILE) * TILE + x - tx * TILE) as usize;
                        // Coverage is monotonic inside a dab sequence. Once it
                        // reaches one, no later stamp can alter that source pixel.
                        // Provisional tails still restore the entire saved tile.
                        if tile[index] == 1. {
                            continue;
                        }
                        let world = self
                            .mapping
                            .world(Point::new(x as f64 + 0.5, y as f64 + 0.5));
                        if render::edit_coverage(&self.doc, None, world) == 0 {
                            continue;
                        }
                        let distance = if cached.is_none() && !hard {
                            world.distance(p) / r
                        } else {
                            0.
                        };
                        let amount = if let Some(tip) = &cached {
                            let fx = x + self.grid.x as i32 - p.x.floor() as i32 - tip.left;
                            let fy = y + self.grid.y as i32 - p.y.floor() as i32 - tip.top;
                            debug_assert!(fx >= 0 && fx < tip.width && fy >= 0 && fy < tip.height);
                            tip.pixels[(fy * tip.width + fx) as usize]
                        } else if hard {
                            // CoreGraphics ellipse antialiasing is approximated by four native samples.
                            // A unit source pixel's four samples are less than one
                            // document pixel from its center. Only the edge band
                            // needs the original hypot decisions; no phase is rounded.
                            let dx = world.x - p.x;
                            let dy = world.y - p.y;
                            let squared = dx * dx + dy * dy;
                            if unit_pixels && r > 1. && squared < (r - 1.) * (r - 1.) {
                                1.
                            } else if unit_pixels && squared > (r + 1.) * (r + 1.) {
                                0.
                            } else {
                                [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                                    .iter()
                                    .filter(|(dx, dy)| {
                                        self.mapping
                                            .world(Point::new(x as f64 + dx, y as f64 + dy))
                                            .distance(p)
                                            <= r
                                    })
                                    .count() as f32
                                    / 4.
                            }
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
                        let at = index;
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
    /// BrushStroke dirty tile publication + RasterSnapshot.paintSnapshot. The
    /// existing mask representation stays contiguous; image paint/erase shares
    /// immutable source pixels and recomposes only changed coverage tiles.
    pub fn snapshot(&self) -> Result<Layer> {
        if self.mask {
            return self.snapshot_contiguous();
        }
        if self.settings.opacity == 0. || (self.settings.erase && self.source_asset.is_none()) {
            return Ok(self.original.clone());
        }
        let mut publication = self.publication.borrow_mut();
        let mut changed = false;
        publication.tiles.retain(|key, _| {
            let keep = self.tiles.contains_key(key);
            changed |= !keep;
            keep
        });
        for (&(tx, ty), coverage) in &self.tiles {
            if publication
                .tiles
                .get(&(tx, ty))
                .is_some_and(|tile| Arc::ptr_eq(&tile.coverage, coverage))
            {
                continue;
            }
            let old = publication.tiles.get(&(tx, ty));
            let base = if let Some(tile) = old {
                tile.base.clone()
            } else {
                let mut surface = render::surface(TILE as u32, TILE as u32)?;
                if let Some(source) = &self.source_asset {
                    let c = surface.canvas();
                    c.translate(((-tx * TILE) as f32, (-ty * TILE) as f32));
                    source.draw(
                        c,
                        skia_safe::Rect::from_wh(self.grid.width as f32, self.grid.height as f32),
                    )?;
                }
                Arc::new(render::rgba_pixels(&surface.image_snapshot())?)
            };
            let clip = if let Some(tile) = old {
                tile.clip.clone()
            } else {
                let inside = self.selection.is_none()
                    && [
                        (0.5, 0.5),
                        (TILE as f64 - 0.5, 0.5),
                        (0.5, TILE as f64 - 0.5),
                        (TILE as f64 - 0.5, TILE as f64 - 0.5),
                    ]
                    .iter()
                    .all(|&(x, y)| {
                        render::edit_coverage(
                            &self.doc,
                            None,
                            self.mapping.world(Point::new(
                                tx as f64 * TILE as f64 + x,
                                ty as f64 * TILE as f64 + y,
                            )),
                        ) == 255
                    });
                if inside {
                    None
                } else {
                    Some(Arc::new(
                        (0..TILE * TILE)
                            .map(|i| {
                                let world = self.mapping.world(Point::new(
                                    (tx * TILE + i % TILE) as f64 + 0.5,
                                    (ty * TILE + i / TILE) as f64 + 0.5,
                                ));
                                render::edit_coverage(
                                    &self.doc,
                                    self.selection.as_deref().map(Vec::as_slice),
                                    world,
                                )
                            })
                            .collect(),
                    ))
                }
            };
            let mut pixels = old.map_or_else(
                || base.as_ref().clone(),
                |tile| tile.pixels.as_ref().clone(),
            );
            let (mut left, mut top, mut right, mut bottom) = (TILE, TILE, 0, 0);
            let mut pixel_change = false;
            for (i, &amount) in coverage.iter().enumerate() {
                let selected = clip.as_ref().map_or(255, |clip| clip[i]);
                if selected == 0 {
                    continue;
                }
                if amount > 0. {
                    let (x, y) = (i as i32 % TILE, i as i32 / TILE);
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x + 1);
                    bottom = bottom.max(y + 1);
                }
                let previous = old.map_or(0., |tile| tile.coverage[i]);
                if amount == previous {
                    continue;
                }
                // Recompose changed coverage from the stroke's ORIGINAL source,
                // including zero coverage restored by the provisional-tail backup.
                pixels[i * 4..i * 4 + 4].copy_from_slice(&base[i * 4..i * 4 + 4]);
                render::composite_pixel(
                    &mut pixels[i * 4..i * 4 + 4],
                    self.settings.color,
                    amount as f64 * self.settings.opacity * selected as f64 / 255.,
                    self.settings.erase,
                );
                pixel_change = true;
            }
            let extent = (right > left).then(|| {
                skia_safe::IRect::new(
                    tx * TILE + left,
                    ty * TILE + top,
                    tx * TILE + right,
                    ty * TILE + bottom,
                )
            });
            changed |= old.is_none_or(|tile| pixel_change || tile.extent != extent);
            let patch = if !pixel_change
                && let Some(tile) = old
                && tile.extent == extent
            {
                tile.patch.clone()
            } else {
                extent
                    .map(|extent| {
                        let mut cropped =
                            Vec::with_capacity((extent.width() * extent.height() * 4) as usize);
                        for y in top..bottom {
                            let at = ((y * TILE + left) * 4) as usize;
                            cropped
                                .extend_from_slice(&pixels[at..at + (extent.width() * 4) as usize]);
                        }
                        Ok::<_, anyhow::Error>(RasterPatch::new(
                            extent,
                            Arc::new(render::rgba_image(
                                extent.width() as u32,
                                extent.height() as u32,
                                &cropped,
                            )?),
                        ))
                    })
                    .transpose()?
            };
            publication.tiles.insert(
                (tx, ty),
                PublishedTile {
                    coverage: coverage.clone(),
                    base,
                    pixels: Arc::new(pixels),
                    clip,
                    patch,
                    extent,
                },
            );
        }
        if !changed && let Some(layer) = &publication.layer {
            return Ok(layer.clone());
        }
        let mut extent = self
            .source_asset
            .as_ref()
            .map(|_| skia_safe::IRect::from_wh(self.grid.width as i32, self.grid.height as i32));
        let mut touched = false;
        let mut patches = vec![];
        for tile in publication.tiles.values() {
            if let Some(touched_rect) = tile.extent {
                touched = true;
                extent = Some(extent.map_or(touched_rect, |old| {
                    skia_safe::IRect::join(&old, &touched_rect)
                }));
                patches.push(
                    tile.patch
                        .as_ref()
                        .expect("coverage extent has a patch")
                        .clone(),
                );
            }
        }
        if !touched {
            return Ok(self.original.clone());
        }
        let bounds = extent.expect("touched pixels");
        let (width, height) = (bounds.width() as u32, bounds.height() as u32);
        dimensions(width, height)?;
        let mut next = self.original.clone();
        let center = geometry::to_world(
            &self.grid,
            Point::new(
                (bounds.left + bounds.right) as f64 / 2.,
                (bounds.top + bounds.bottom) as f64 / 2.,
            ),
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
        next.content = Arc::new(Content::Image {
            data: ImageAsset::Tiled(Arc::new(RasterSnapshot::replacing(
                self.source_asset.as_ref(),
                (self.grid.width, self.grid.height),
                patches,
                bounds,
            )?)),
        });
        next.strokes.clear();
        publication.layer = Some(next.clone());
        Ok(next)
    }
    fn snapshot_contiguous(&self) -> Result<Layer> {
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
