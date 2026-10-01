//! Retained preview composition. This is a Skia backend adaptation, not a GIMP code port.
//! Compositor's EditorCanvas/TiledLayerRenderer also retain unchanged pixels and clear
//! replacement regions before drawing them. See docs/compositor-port.md for scope.
use super::*;

pub(super) struct Composite {
    document: Document,
    pixels: Vec<u8>,
    image: Image,
    prefixes: std::collections::HashMap<(i32, i32), Vec<Prefix>>,
}
struct Prefix {
    nodes: usize,
    image: Image,
}

fn trim_prefixes(
    prefixes: &mut std::collections::HashMap<(i32, i32), Vec<Prefix>>,
    budget: u64,
) -> u64 {
    let mut ids = std::collections::HashSet::new();
    let mut used: u64 = prefixes
        .values()
        .flatten()
        .filter(|p| ids.insert(p.image.unique_id()))
        .map(|p| p.image.width() as u64 * p.image.height() as u64)
        .sum();
    while used > budget {
        let victim = prefixes.iter().find_map(|(&cell, entries)| {
            let newest = entries.last()?.image.unique_id();
            entries
                .iter()
                .find(|p| p.image.unique_id() != newest)
                .map(|p| {
                    (
                        cell,
                        p.image.unique_id(),
                        p.image.width() as u64 * p.image.height() as u64,
                    )
                })
        });
        let Some((cell, id, size)) = victim else {
            break;
        };
        prefixes
            .get_mut(&cell)
            .unwrap()
            .retain(|p| p.image.unique_id() != id);
        used -= size;
    }
    used
}

pub(super) struct Background {
    document_size: (u32, u32),
    bounds: Rect,
    viewport: Viewport,
    image: Image,
}

/// TiledLayerRenderer.swift's padded pieces, adapted to Skia's existing preview
/// sampling. These are immutable document-space pixels, not a second raster engine.
pub(super) struct PreviewTiles {
    document: Document,
    images: std::collections::HashMap<(i32, i32), Image>,
}

pub(super) fn tiled_preview_eligible(doc: &Document) -> bool {
    doc.layers.iter().all(|l| {
        simple(l)
            && l.x.fract() == 0.
            && l.y.fract() == 0.
            && l.blend == Blend::SourceOver
            && l.saturation == 1.
            && l.brightness == 1.
            && (!matches!(
                l.content.as_ref(),
                Content::Image {
                    data: crate::asset::ImageAsset::Tiled(_)
                }
            ) || l.opacity == 1.)
    })
}

pub(super) fn has_tiled_rasters(doc: &Document) -> bool {
    doc.layers.iter().any(|layer| {
        matches!(
            layer.content.as_ref(),
            Content::Image {
                data: crate::asset::ImageAsset::Tiled(_)
            }
        )
    })
}

fn piece_bounds(bounds: sk::IRect, x: i32, y: i32) -> (sk::IRect, sk::IRect) {
    let core =
        sk::IRect::intersect(&sk::IRect::from_xywh(x * 256, y * 256, 256, 256), &bounds).unwrap();
    // Two source pixels cover linear and Catmull-Rom sampling.
    let padded = sk::IRect::intersect(
        &sk::IRect::new(core.left - 2, core.top - 2, core.right + 2, core.bottom + 2),
        &bounds,
    )
    .unwrap();
    (core, padded)
}

fn content_damage(a: &Layer, b: &Layer) -> Option<Vec<sk::IRect>> {
    use crate::asset::ImageAsset;
    let (Content::Image { data: a }, Content::Image { data: b }) =
        (a.content.as_ref(), b.content.as_ref())
    else {
        return None;
    };
    let identity = |asset: &ImageAsset| -> Option<(u32, sk::IRect)> {
        match asset {
            ImageAsset::Tiled(raster) => {
                Some((raster.base.as_ref()?.unique_id(), raster.base_rect))
            }
            _ => {
                let image = asset.image().ok()?;
                Some((
                    image.unique_id(),
                    sk::IRect::from_wh(image.width(), image.height()),
                ))
            }
        }
    };
    if identity(a) != identity(b) || identity(a).is_none() {
        return None;
    }
    fn patches(asset: &ImageAsset) -> &[crate::raster_snapshot::RasterPatch] {
        match asset {
            ImageAsset::Tiled(raster) => raster.patches.as_slice(),
            _ => &[],
        }
    }
    let (old, new) = (patches(a), patches(b));
    Some(
        old.iter()
            .filter(|p| {
                !new.iter()
                    .any(|q| p.rect == q.rect && p.image.unique_id() == q.image.unique_id())
            })
            .chain(new.iter().filter(|p| {
                !old.iter()
                    .any(|q| p.rect == q.rect && p.image.unique_id() == q.image.unique_id())
            }))
            .map(|p| p.rect)
            .collect(),
    )
}

fn tile_damage(before: &Document, after: &Document) -> Damage {
    if before.width != after.width
        || before.height != after.height
        || before.layers.len() != after.layers.len()
    {
        return Damage::Full;
    }
    let mut region: Option<Rect> = None;
    for (a, b) in before.layers.iter().zip(&after.layers) {
        if same_layer(a, b) {
            continue;
        }
        let mut without_content = a.clone();
        without_content.content = b.content.clone();
        if !same_layer(&without_content, b) {
            // Ordinary placement changes use the existing conservative bounds
            // calculation. Invalidating every piece regresses integer nudges;
            // mixed content/appearance changes still return full damage there.
            return damage(before, after, &mut std::collections::HashSet::new());
        }
        let Some(rects) = content_damage(a, b) else {
            return Damage::Full;
        };
        for r in rects {
            let bounds = Rect::new(
                r.left as f32 + b.x as f32 - 2.,
                r.top as f32 + b.y as f32 - 2.,
                r.right as f32 + b.x as f32 + 2.,
                r.bottom as f32 + b.y as f32 + 2.,
            );
            region = Some(region.map_or(bounds, |old| {
                Rect::new(
                    old.left.min(bounds.left),
                    old.top.min(bounds.top),
                    old.right.max(bounds.right),
                    old.bottom.max(bounds.bottom),
                )
            }));
        }
    }
    region.map_or(Damage::Clean, Damage::Region)
}

enum Damage {
    Clean,
    Region(Rect),
    Full,
}

// Source pixels and coverage are immutable native resources. Their identity avoids
// scanning large mask buffers or stroke arrays just to recognize an unchanged frame.
pub(super) fn same_layer(a: &Layer, b: &Layer) -> bool {
    a.id == b.id
        && a.parent_id == b.parent_id
        && a.mask_source_id == b.mask_source_id
        && a.visible == b.visible
        && a.width == b.width
        && a.height == b.height
        && a.x == b.x
        && a.y == b.y
        && a.scale_x == b.scale_x
        && a.scale_y == b.scale_y
        && a.rotation == b.rotation
        && a.flip_x == b.flip_x
        && a.flip_y == b.flip_y
        && a.opacity == b.opacity
        && a.blend == b.blend
        && a.sampling == b.sampling
        && a.brightness == b.brightness
        && a.saturation == b.saturation
        && a.blur == b.blur
        && Arc::ptr_eq(&a.content, &b.content)
        && match (&a.mask, &b.mask) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
        && a.strokes.len() == b.strokes.len()
        && a.strokes
            .iter()
            .zip(&b.strokes)
            .all(|(a, b)| Arc::ptr_eq(a, b))
}

fn simple(layer: &Layer) -> bool {
    layer.parent_id.is_none()
        && layer.mask_source_id.is_none()
        && layer.mask.is_none()
        && layer.blur == 0.
        && layer.rotation == 0.
        && layer.scale_x == 1.
        && layer.scale_y == 1.
        && !layer.flip_x
        && !layer.flip_y
        && !matches!(layer.content.as_ref(), Content::Group)
}

fn damage(
    old: &Document,
    new: &Document,
    changed: &mut std::collections::HashSet<String>,
) -> Damage {
    use crate::layer_index::LayerIndex;
    use std::collections::{HashMap, HashSet, VecDeque};
    if old.width != new.width || old.height != new.height {
        return Damage::Full;
    }
    let a = LayerIndex::new(old);
    let b = LayerIndex::new(new);
    let mut affected: HashSet<&str> = HashSet::new();
    for layer in &old.layers {
        if b.layer(&layer.id)
            .is_none_or(|next| !same_layer(layer, next))
        {
            affected.insert(&layer.id);
        }
    }
    for layer in &new.layers {
        if a.layer(&layer.id).is_none() {
            affected.insert(&layer.id);
        }
    }
    let old_order = a.ordered_layers();
    let new_order = b.ordered_layers();
    let positions: HashMap<_, _> = old_order
        .iter()
        .enumerate()
        .map(|(i, l)| (l.id.as_str(), i))
        .collect();
    let common: Vec<_> = new_order
        .iter()
        .filter_map(|l| positions.get(l.id.as_str()).map(|&i| (l.id.as_str(), i)))
        .collect();
    // Keep the longest subsequence whose relative order did not change. Only the
    // moved nodes can alter pixels; shifted vector positions are not damage.
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![None; common.len()];
    for (i, &(_, position)) in common.iter().enumerate() {
        let slot = tails.partition_point(|&j| common[j].1 < position);
        if slot > 0 {
            previous[i] = Some(tails[slot - 1]);
        }
        if slot == tails.len() {
            tails.push(i);
        } else {
            tails[slot] = i;
        }
    }
    let mut stable = HashSet::new();
    let mut current = tails.last().copied();
    while let Some(i) = current {
        stable.insert(common[i].0);
        current = previous[i];
    }
    for &(id, _) in &common {
        if !stable.contains(id) {
            affected.insert(id);
        }
    }

    fn stacks<'a>(index: &LayerIndex<'a>) -> HashMap<&'a str, Vec<&'a str>> {
        let layers: Vec<_> = index
            .ordered_layers()
            .into_iter()
            .filter(|l| index.effective(l).0 && !matches!(l.content.as_ref(), Content::Group))
            .collect();
        let mut result = HashMap::new();
        for (i, layer) in layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.mask_source_id.is_none())
        {
            let children: Vec<_> = layers[i + 1..]
                .iter()
                .take_while(|l| {
                    l.mask_source_id.as_deref() == Some(&layer.id) && l.parent_id == layer.parent_id
                })
                .map(|l| l.id.as_str())
                .collect();
            if !children.is_empty() {
                result.insert(layer.id.as_str(), children);
            }
        }
        result
    }
    let old_stacks = stacks(&a);
    let new_stacks = stacks(&b);
    for (&base, children) in &old_stacks {
        if new_stacks.get(base) != Some(children) {
            affected.insert(base);
        }
    }
    for (&base, children) in &new_stacks {
        if old_stacks.get(base) != Some(children) {
            affected.insert(base);
        }
    }
    // Damage follows both versions of the dependency graph: folder descendants,
    // mask consumers, and the shared-alpha base of a changed clipping child.
    let mut dependencies: HashMap<&str, Vec<&str>> = HashMap::new();
    for doc in [old, new] {
        for layer in &doc.layers {
            if let Some(parent) = layer.parent_id.as_deref() {
                dependencies.entry(parent).or_default().push(&layer.id);
            }
            if let Some(source) = layer.mask_source_id.as_deref() {
                dependencies.entry(source).or_default().push(&layer.id);
                dependencies.entry(&layer.id).or_default().push(source);
            }
        }
    }
    let mut pending: VecDeque<_> = affected.iter().copied().collect();
    while let Some(id) = pending.pop_front() {
        for &dependent in dependencies.get(id).into_iter().flatten() {
            if affected.insert(dependent) {
                pending.push_back(dependent);
            }
        }
    }
    changed.extend(affected.iter().map(|id| (*id).to_owned()));
    let mut region = Rect::new(
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for id in affected {
        for layer in [a.layer(id), b.layer(id)].into_iter().flatten() {
            if !matches!(layer.content.as_ref(), Content::Group) {
                region.join(super::projection::support(layer));
            }
        }
    }
    if region.intersect(rect(new.width, new.height)) && region.width() > 0. && region.height() > 0.
    {
        Damage::Region(region)
    } else {
        Damage::Clean
    }
}

impl Renderer {
    pub(super) fn retain_preview_tiles_when_unpatched(&mut self, doc: &Document) {
        // Undo can restore the ordinary source. Present it through the existing
        // compositor, but keep the previous pieces for the next patch edit.
        // Their recorded document still describes their pixels, so tile_damage
        // invalidates every changed/removed patch before reuse. Deleted/replaced
        // layer sets and unsupported scenes release these resources immediately.
        if !tiled_preview_eligible(doc)
            || self.preview_tiles.as_ref().is_some_and(|cache| {
                cache.document.layers.len() != doc.layers.len()
                    || cache
                        .document
                        .layers
                        .iter()
                        .zip(&doc.layers)
                        .any(|(a, b)| a.id != b.id)
            })
        {
            self.preview_tiles = None;
        }
    }

    pub(crate) fn retain_thumbnail_sources(&mut self, doc: &Document) {
        self.thumbnail_tiles
            .retain(|id, _| doc.layers.iter().any(|layer| &layer.id == id));
    }
    fn draw_cached_tiles(
        &mut self,
        doc: &Document,
        canvas: &Canvas,
        visible: sk::IRect,
        sampling: sk::SamplingOptions,
        previous: Option<PreviewTiles>,
        source_edges: bool,
    ) -> Result<PreviewTiles> {
        let changed = previous
            .as_ref()
            .map_or(Damage::Full, |p| tile_damage(&p.document, doc));
        let mut images = previous.map_or_else(std::collections::HashMap::new, |p| p.images);
        images.retain(|&(x, y), _| {
            let mut cell = Rect::from_xywh((x * 256) as f32, (y * 256) as f32, 256., 256.);
            match changed {
                Damage::Clean => true,
                Damage::Full => false,
                Damage::Region(r) => !cell.intersect(r),
            }
        });
        let bounds = sk::IRect::from_wh(doc.width as i32, doc.height as i32);
        if let Some(visible) = sk::IRect::intersect(&visible, &bounds) {
            for y in visible.top / 256..=(visible.bottom - 1) / 256 {
                for x in visible.left / 256..=(visible.right - 1) / 256 {
                    let (core, padded) = piece_bounds(bounds, x, y);
                    if let std::collections::hash_map::Entry::Vacant(entry) = images.entry((x, y)) {
                        let mut tile = surface(padded.width() as u32, padded.height() as u32)?;
                        let c = tile.canvas();
                        c.clear(Color::TRANSPARENT);
                        c.translate((-padded.left as f32, -padded.top as f32));
                        self.draw_document(doc, c)?;
                        entry.insert(tile.image_snapshot());
                    }
                    let source_shader = if source_edges {
                        let matrix = sk::Matrix::translate((padded.left as f32, padded.top as f32));
                        Some(
                            images[&(x, y)]
                                .to_shader(None, sampling, &matrix)
                                .ok_or_else(|| anyhow!("Cannot sample thumbnail piece"))?,
                        )
                    } else {
                        None
                    };
                    canvas.save();
                    if let Some(shader) = source_shader {
                        // Thumbnail placement antialiases the source's outer
                        // rectangle, not the cache-piece edges. Clip only at
                        // internal boundaries and sample the padded texture.
                        let clip = canvas.local_clip_bounds().unwrap_or(Rect::from_irect(core));
                        canvas.clip_rect(
                            Rect::new(
                                if core.left == bounds.left {
                                    clip.left
                                } else {
                                    core.left as f32
                                },
                                if core.top == bounds.top {
                                    clip.top
                                } else {
                                    core.top as f32
                                },
                                if core.right == bounds.right {
                                    clip.right
                                } else {
                                    core.right as f32
                                },
                                if core.bottom == bounds.bottom {
                                    clip.bottom
                                } else {
                                    core.bottom as f32
                                },
                            ),
                            None,
                            false,
                        );
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true).set_shader(shader);
                        canvas.draw_rect(Rect::from_irect(bounds), &paint);
                    } else {
                        canvas.clip_rect(Rect::from_irect(core), None, false);
                        canvas.draw_image_with_sampling_options(
                            &images[&(x, y)],
                            (padded.left as f32, padded.top as f32),
                            sampling,
                            None,
                        );
                    }
                    canvas.restore();
                }
            }
            // Keep only the visible piece grid; scrolling repopulates newly visible cells.
            images.retain(|&(x, y), _| {
                x >= visible.left / 256
                    && x <= (visible.right - 1) / 256
                    && y >= visible.top / 256
                    && y <= (visible.bottom - 1) / 256
            });
        } else {
            images.clear();
        }
        while images
            .values()
            .map(|i| i.width() as u64 * i.height() as u64)
            .sum::<u64>()
            > MAX_PIXELS
        {
            if let Some(key) = images.keys().next().copied() {
                images.remove(&key);
            } else {
                break;
            }
        }
        Ok(PreviewTiles {
            document: doc.clone(),
            images,
        })
    }

    pub(super) fn draw_preview_tiles(
        &mut self,
        doc: &Document,
        canvas: &Canvas,
        v: &Viewport,
    ) -> Result<()> {
        let origin = geometry::canvas_origin(doc, v);
        let visible = sk::IRect::new(
            (-origin.x / v.zoom).floor() as i32,
            (-origin.y / v.zoom).floor() as i32,
            ((v.width - origin.x) / v.zoom).ceil() as i32,
            ((v.height - origin.y) / v.zoom).ceil() as i32,
        );
        // Ordinary source images keep the existing compositor. On the first
        // patch edit, reuse its already composed pixels for unchanged pieces
        // instead of rebuilding the whole visible stack before showing paint.
        let previous = self.preview_tiles.take().or_else(|| {
            let composite = self
                .composite
                .as_ref()
                .filter(|full| tiled_preview_eligible(&full.document))?;
            let bounds = sk::IRect::from_wh(
                composite.document.width as i32,
                composite.document.height as i32,
            );
            let mut images = std::collections::HashMap::new();
            if let Some(visible) = sk::IRect::intersect(&visible, &bounds) {
                for y in visible.top / 256..=(visible.bottom - 1) / 256 {
                    for x in visible.left / 256..=(visible.right - 1) / 256 {
                        let (_, padded) = piece_bounds(bounds, x, y);
                        if let Some(image) =
                            composite
                                .image
                                .make_subset(None, padded, Default::default())
                        {
                            images.insert((x, y), image);
                        }
                    }
                }
            }
            Some(PreviewTiles {
                document: composite.document.clone(),
                images,
            })
        });
        self.preview_tiles = Some(self.draw_cached_tiles(
            doc,
            canvas,
            visible,
            sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
            previous,
            false,
        )?);
        Ok(())
    }

    /// CanvasThumbnail source-only composition with the same padded piece cache.
    /// Complex transforms retain the existing contiguous path.
    pub(crate) fn draw_thumbnail_source(
        &mut self,
        doc: &Document,
        layer: &Layer,
        canvas: &Canvas,
    ) -> Result<()> {
        self.thumbnail_tiles
            .retain(|id, _| doc.layers.iter().any(|l| &l.id == id));
        let mut source = layer.clone();
        source.mask = None;
        source.parent_id = None;
        source.mask_source_id = None;
        source.opacity = 1.;
        source.visible = true;
        source.blend = Blend::SourceOver;
        source.saturation = 1.;
        source.brightness = 1.;
        source.blur = 0.;
        let mut picture = doc.clone();
        picture.layers = vec![source.clone()];
        if tiled_preview_eligible(&picture)
            && matches!(source.content.as_ref(), Content::Image { .. })
        {
            // CanvasThumbnail.place applies placement after drawing the source.
            // Cache source-local pixels so moving the layer only moves their
            // placement on the canvas-shaped thumbnail, without repainting them.
            source.x = 0.;
            source.y = 0.;
            picture.width = source.width;
            picture.height = source.height;
            picture.layers = vec![source];
            let previous = self.thumbnail_tiles.remove(&layer.id);
            canvas.save();
            canvas.translate((layer.x as f32, layer.y as f32));
            let result = self.draw_cached_tiles(
                &picture,
                canvas,
                sk::IRect::from_wh(picture.width as i32, picture.height as i32),
                sampling_options(layer.sampling),
                previous,
                true,
            );
            canvas.restore();
            let cache = result?;
            self.thumbnail_tiles.insert(layer.id.clone(), cache);
            // Source-sized pieces must not multiply the existing renderer budget
            // by the number of layers. Evict other source caches first.
            let pixels = |cache: &PreviewTiles| {
                cache
                    .images
                    .values()
                    .map(|i| i.width() as u64 * i.height() as u64)
                    .sum::<u64>()
            };
            while self.thumbnail_tiles.values().map(pixels).sum::<u64>() > MAX_PIXELS {
                if let Some(id) = self
                    .thumbnail_tiles
                    .keys()
                    .find(|id| *id != &layer.id)
                    .cloned()
                {
                    self.thumbnail_tiles.remove(&id);
                } else {
                    let cache = self.thumbnail_tiles.get_mut(&layer.id).unwrap();
                    if let Some(key) = cache.images.keys().next().copied() {
                        cache.images.remove(&key);
                    } else {
                        break;
                    }
                }
            }
        } else {
            self.thumbnail_tiles.remove(&layer.id);
            let image = self.layer_surface(&source)?;
            draw_pixels(canvas, layer, &image);
        }
        Ok(())
    }

    pub(super) fn preview_background(
        &mut self,
        doc: &Document,
        v: &Viewport,
        bounds: Rect,
    ) -> Result<Image> {
        if let Some(background) = &self.background
            && background.document_size == (doc.width, doc.height)
            && background.viewport == *v
            && background.bounds == bounds
        {
            return Ok(background.image.clone());
        }
        let mut s = surface(
            v.width.round().max(1.) as u32,
            v.height.round().max(1.) as u32,
        )?;
        let c = s.canvas();
        // EditorCanvas.swift: neutral pasteboard and a 10-point dark transparency grid.
        c.clear(color("#1b1b1b"));
        let o = geometry::canvas_origin(doc, v);
        // EditorCanvas.draw: y-down 3-point offset, 14-point soft shadow, black 35%.
        // Skia's Gaussian sigma adapts CoreGraphics' blur radius.
        let mut shadow = paint("#000000");
        shadow
            .set_alpha_f(0.35)
            .set_mask_filter(skia_safe::MaskFilter::blur(
                skia_safe::BlurStyle::Normal,
                7.,
                true,
            ));
        c.draw_rect(
            Rect::from_xywh(
                o.x as f32 + bounds.left * v.zoom as f32,
                (o.y + 3.) as f32 + bounds.top * v.zoom as f32,
                bounds.width() * v.zoom as f32,
                bounds.height() * v.zoom as f32,
            ),
            &shadow,
        );
        c.save();
        c.translate(point(o));
        c.scale((v.zoom as f32, v.zoom as f32));
        c.clip_rect(bounds, None, false);
        // EditorCanvas.swift 609dbeae, MIT © 2026 Wonder Assembly LLC.
        // Retain the viewport-clipped checkerboard in the Skia background cache.
        c.draw_rect(bounds, &paint("#4d4d4d"));
        let tile = 10. / v.zoom;
        let sx = (bounds.left as f64 / tile)
            .floor()
            .max((-o.x / v.zoom / tile).floor()) as i32;
        let sy = (bounds.top as f64 / tile)
            .floor()
            .max((-o.y / v.zoom / tile).floor()) as i32;
        let ex = (bounds.right as f64 / tile)
            .ceil()
            .min(((v.width - o.x) / v.zoom / tile).ceil()) as i32;
        let ey = (bounds.bottom as f64 / tile)
            .ceil()
            .min(((v.height - o.y) / v.zoom / tile).ceil()) as i32;
        let p = paint("#595959");
        for y in sy..ey {
            for x in sx..ex {
                if (x + y) % 2 == 0 {
                    c.draw_rect(
                        Rect::from_xywh(
                            (x as f64 * tile) as f32,
                            (y as f64 * tile) as f32,
                            tile as f32,
                            tile as f32,
                        ),
                        &p,
                    );
                }
            }
        }
        c.restore();
        let image = s.image_snapshot();
        self.background = Some(Background {
            document_size: (doc.width, doc.height),
            viewport: v.clone(),
            bounds,
            image: image.clone(),
        });
        Ok(image)
    }

    pub(super) fn preview_composite(&mut self, document: &Document) -> Result<Image> {
        // Retain Rust-owned pixels rather than a Skia Surface: the Node addon can
        // move its locked Renderer between worker threads, while Surface is !Send.
        // Skia borrows this buffer only during the current call, with no unsafe Send.
        let previous = self.composite.take();
        let mut changed = std::collections::HashSet::new();
        let damage = previous.as_ref().map_or(Damage::Full, |c| {
            damage(&c.document, document, &mut changed)
        });
        if matches!(damage, Damage::Clean) {
            let mut composite = previous.expect("clean composite exists");
            composite.document = document.clone();
            let image = composite.image.clone();
            self.composite = Some(composite);
            return Ok(image);
        }
        dimensions(document.width, document.height)?;
        let plan = super::projection::Projection::new(document);
        let stable = previous
            .as_ref()
            .filter(|_| !matches!(damage, Damage::Full))
            .map_or(0, |old| {
                let old_plan = super::projection::Projection::new(&old.document);
                plan.nodes
                    .iter()
                    .take_while(|node| {
                        node.layers.clone().all(|i| {
                            let layer = plan.layers[i];
                            !changed.contains(&layer.id)
                                && old_plan
                                    .layers
                                    .get(i)
                                    .is_some_and(|old| same_layer(old, layer))
                        })
                    })
                    .count()
            });
        let (mut pixels, mut prefixes) = previous.map_or_else(
            || (Vec::new(), std::collections::HashMap::new()),
            |c| (c.pixels, c.prefixes),
        );
        prefixes.retain(|_, entries| {
            entries.retain(|prefix| prefix.nodes <= stable);
            !entries.is_empty()
        });
        // Sparse checkpoints every 64 atomic nodes plus a short final tail.
        // Empty spans share the preceding immutable image; the total unique
        // checkpoint pixel budget stays independent of layer count.
        let checkpoint = plan.nodes.len().saturating_sub(8);
        pixels.resize(document.width as usize * document.height as usize * 4, 0);
        let info =
            sk::ImageInfo::new_n32_premul((document.width as i32, document.height as i32), None);
        let image = {
            let mut working =
                sk::surfaces::wrap_pixels(&info, &mut pixels, document.width as usize * 4, None)
                    .ok_or_else(|| anyhow!("Cannot wrap retained composite pixels"))?;
            let canvas = working.canvas();
            self.retain_projection_sources(&plan.index);
            // Fixed document-grid chunks keep Skia's interpolation/rounding
            // identical on initial and partial redraws. Arbitrary damage clips
            // can change the sampling phase of unchanged overlapping layers.
            let bounds = sk::IRect::from_wh(document.width as i32, document.height as i32);
            for y in 0..(document.height as i32 + 255) / 256 {
                for x in 0..(document.width as i32 + 255) / 256 {
                    let core = sk::IRect::intersect(
                        &sk::IRect::from_xywh(x * 256, y * 256, 256, 256),
                        &bounds,
                    )
                    .unwrap();
                    let cell = Rect::from_irect(core);
                    if matches!(damage, Damage::Region(region) if !cell.intersects(region)) {
                        continue;
                    }
                    canvas.save();
                    canvas.clip_rect(cell, None, false);
                    canvas.clear(Color::TRANSPARENT);
                    let entries = prefixes.entry((x, y)).or_default();
                    let mut start = 0;
                    if let Some(prefix) = entries.last() {
                        canvas.draw_image(&prefix.image, (core.left as f32, core.top as f32), None);
                        start = prefix.nodes;
                    }
                    let mut checkpoints: Vec<_> =
                        ((start / 64 + 1) * 64..=checkpoint).step_by(64).collect();
                    if checkpoint > start && checkpoints.last() != Some(&checkpoint) {
                        checkpoints.push(checkpoint);
                    }
                    for target in checkpoints {
                        let changes_pixels = plan.intersects(start..target, cell);
                        self.draw_projection_range(&plan, start..target, canvas)?;
                        let image = if !changes_pixels && !entries.is_empty() {
                            entries.last().unwrap().image.clone()
                        } else {
                            let info =
                                sk::ImageInfo::new_n32_premul((core.width(), core.height()), None);
                            let mut checkpoint_pixels =
                                vec![0u8; core.width() as usize * core.height() as usize * 4];
                            ensure!(
                                canvas.read_pixels(
                                    &info,
                                    &mut checkpoint_pixels,
                                    core.width() as usize * 4,
                                    (core.left, core.top)
                                ),
                                "Cannot read projection checkpoint"
                            );
                            sk::images::raster_from_data(
                                &info,
                                sk::Data::new_copy(&checkpoint_pixels),
                                core.width() as usize * 4,
                            )
                            .ok_or_else(|| anyhow!("Cannot retain projection checkpoint"))?
                        };
                        entries.push(Prefix {
                            nodes: target,
                            image,
                        });
                        start = target;
                    }
                    self.draw_projection_range(&plan, start..plan.nodes.len(), canvas)?;
                    canvas.restore();
                }
            }
            // Raster-direct snapshots own an immutable copy, so subsequent edits
            // cannot alter an image still held by a consumer on another thread.
            working.image_snapshot()
        };
        // Keep every chunk's newest image, then evict older distinct images
        // until unique storage fits the document-independent pixel budget.
        // Shared empty-span records do not multiply bitmap accounting.
        trim_prefixes(&mut prefixes, MAX_PIXELS);
        self.composite = Some(Composite {
            document: document.clone(),
            pixels,
            image: image.clone(),
            prefixes,
        });
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_budget_counts_shared_images_once_and_keeps_newest_per_chunk() {
        let mut prefixes = std::collections::HashMap::new();
        for x in 0..3 {
            let image = |color| rgba_image(32, 32, &[color, 80, 90, 255].repeat(32 * 32)).unwrap();
            let first = image(30);
            let newest = image(60);
            prefixes.insert(
                (x, 0),
                vec![
                    Prefix {
                        nodes: 64,
                        image: first.clone(),
                    },
                    Prefix {
                        nodes: 128,
                        image: first,
                    },
                    Prefix {
                        nodes: 256,
                        image: newest.clone(),
                    },
                    Prefix {
                        nodes: 300,
                        image: newest,
                    },
                ],
            );
        }
        assert_eq!(trim_prefixes(&mut prefixes, 3 * 32 * 32), 3 * 32 * 32);
        for entries in prefixes.values() {
            assert_eq!(entries.last().unwrap().nodes, 300);
            assert!(
                entries
                    .iter()
                    .all(|p| p.image.unique_id() == entries.last().unwrap().image.unique_id())
            );
        }
    }

    #[test]
    fn projection_prefix_reuses_unchanged_backdrop_and_invalidates_external_dependencies() {
        let mut doc = Document::new("Projection prefix", 310, 270).unwrap();
        doc.version = 2;
        for i in 0..90 {
            let mut layer = Layer::new(
                "Overlapping layer",
                39,
                31,
                Content::Shape {
                    shape: Shape::Ellipse,
                    color: "#6587ff80".into(),
                },
            );
            layer.x = 218.5 + (i % 8) as f64 * 7.;
            layer.y = 219. + (i % 5) as f64 * 7.;
            layer.scale_x = 1.4;
            layer.rotation = (i % 4) as f64 * 13.;
            layer.opacity = 0.63;
            if i % 3 == 0 {
                layer.blend = Blend::Screen;
            }
            doc.layers.push(layer);
        }
        // A source above the checkpoint affects a target below it. Neither its
        // visibility nor its position makes the dependency safe to ignore.
        doc.layers[1].mask_source_id = Some(doc.layers[75].id.clone());
        doc.layers[75].visible = false;
        doc.layers[33].mask_source_id = Some(doc.layers[32].id.clone());
        let mut renderer = Renderer::default();
        let check = |renderer: &mut Renderer, doc: &Document| {
            let actual = renderer.preview_composite(doc).unwrap();
            let fresh = Renderer::default().preview_composite(doc).unwrap();
            assert!(rgba_pixels(&actual).unwrap() == rgba_pixels(&fresh).unwrap());
        };
        check(&mut renderer, &doc);
        let prefix = renderer.composite.as_ref().unwrap().prefixes[&(0, 0)]
            .last()
            .unwrap()
            .image
            .unique_id();
        doc.layers.swap(88, 89);
        check(&mut renderer, &doc);
        assert_eq!(
            renderer.composite.as_ref().unwrap().prefixes[&(0, 0)]
                .last()
                .unwrap()
                .image
                .unique_id(),
            prefix
        );
        doc.layers[75].opacity = 0.2;
        check(&mut renderer, &doc);
        assert_ne!(
            renderer.composite.as_ref().unwrap().prefixes[&(0, 0)]
                .last()
                .unwrap()
                .image
                .unique_id(),
            prefix
        );
        doc.layers.swap(20, 86);
        check(&mut renderer, &doc);
        doc.layers[40].x -= 180.;
        check(&mut renderer, &doc);
        doc.layers.remove(80);
        check(&mut renderer, &doc);
    }

    #[test]
    fn source_thumbnail_moves_reuse_pixels_and_match_contiguous_placement() {
        use crate::{
            asset::ImageAsset,
            raster_snapshot::{RasterPatch, RasterSnapshot},
        };
        let (w, h) = (333, 267);
        let mut pixels = vec![];
        for y in 0..h {
            for x in 0..w {
                pixels.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 173, 137]);
            }
        }
        let base = ImageAsset::Raster(Arc::new(rgba_image(w, h, &pixels).unwrap()));
        let raster = RasterSnapshot::replacing(
            Some(&base),
            (w, h),
            vec![RasterPatch::new(
                sk::IRect::from_xywh(240, 140, 70, 30),
                Arc::new(rgba_image(70, 30, &[0, 0, 0, 0].repeat(70 * 30)).unwrap()),
            )],
            sk::IRect::from_wh(w as i32, h as i32),
        )
        .unwrap();
        let mut doc = Document::new("Thumbnail placement", 777, 555).unwrap();
        doc.layers.push(Layer::new(
            "Translucent source",
            w,
            h,
            Content::Image {
                data: ImageAsset::Tiled(Arc::new(raster)),
            },
        ));
        let mut renderer = Renderer::default();
        let mut identity = None;
        for (x, y) in [(50., 80.), (290., 180.), (-80., -30.), (730., 500.)] {
            doc.layers[0].x = x;
            doc.layers[0].y = y;
            let mut actual = surface(72, 52).unwrap();
            actual.canvas().clear(Color::from_rgb(56, 56, 56));
            actual.canvas().scale((72. / 777., 72. / 777.));
            renderer
                .draw_thumbnail_source(&doc, &doc.layers[0], actual.canvas())
                .unwrap();
            let id = renderer.thumbnail_tiles[&doc.layers[0].id].images[&(0, 0)].unique_id();
            if let Some(previous) = identity {
                assert_eq!(
                    previous, id,
                    "source pixels stay shared across placement changes"
                );
            }
            identity = Some(id);
            let mut expected = surface(72, 52).unwrap();
            expected.canvas().clear(Color::from_rgb(56, 56, 56));
            expected.canvas().scale((72. / 777., 72. / 777.));
            Renderer::default()
                .draw_own(&doc, &doc.layers[0], expected.canvas(), Blend::SourceOver)
                .unwrap();
            let a = rgba_pixels(&actual.image_snapshot()).unwrap();
            let b = rgba_pixels(&expected.image_snapshot()).unwrap();
            let (index, maximum) = a
                .iter()
                .zip(&b)
                .enumerate()
                .map(|(i, (a, b))| (i, a.abs_diff(*b)))
                .max_by_key(|(_, difference)| *difference)
                .unwrap();
            assert!(
                maximum <= 1,
                "position {x}/{y}, max {maximum}, index {index}, actual {:?}, expected {:?}",
                &a[index / 4 * 4..index / 4 * 4 + 4],
                &b[index / 4 * 4..index / 4 * 4 + 4]
            );
        }
    }

    #[test]
    fn cached_tiles_follow_moves_and_reuse_unaffected_pieces() {
        use crate::{
            asset::ImageAsset,
            raster_snapshot::{RasterPatch, RasterSnapshot},
        };
        let base = ImageAsset::Raster(Arc::new(
            rgba_image(20, 20, &[65, 135, 240, 137].repeat(20 * 20)).unwrap(),
        ));
        let raster = RasterSnapshot::replacing(
            Some(&base),
            (20, 20),
            vec![RasterPatch::new(
                sk::IRect::from_xywh(5, 5, 5, 5),
                Arc::new(rgba_image(5, 5, &[0, 0, 0, 0].repeat(25)).unwrap()),
            )],
            sk::IRect::from_wh(20, 20),
        )
        .unwrap();
        let mut doc = Document::new("Moving retained pixels", 777, 555).unwrap();
        doc.layers.push(Layer::new(
            "Translucent source with a transparent replacement",
            20,
            20,
            Content::Image {
                data: ImageAsset::Tiled(Arc::new(raster)),
            },
        ));
        let viewport = Viewport {
            width: 777.,
            height: 555.,
            zoom: 1.,
            pan: Point::default(),
        };
        let mut renderer = Renderer::default();
        let mut unaffected = None;
        for (x, y) in [(60., 80.), (270., 80.), (270., 287.), (60., 80.)] {
            doc.layers[0].x = x;
            doc.layers[0].y = y;
            let actual = renderer
                .preview(&doc, &viewport, &[], false, None, None, None, None)
                .unwrap()
                .image_snapshot();
            let id = renderer.preview_tiles.as_ref().unwrap().images[&(2, 1)].unique_id();
            if let Some(previous) = unaffected {
                assert_eq!(previous, id, "a remote unchanged piece must stay shared");
            }
            unaffected = Some(id);
            let mut reference = Renderer::default();
            let mut expected = surface(777, 555).unwrap();
            expected.canvas().draw_image(
                reference
                    .preview_background(&doc, &viewport, rect(777, 555))
                    .unwrap(),
                (0., 0.),
                None,
            );
            expected.canvas().draw_image(
                reference.render(&doc).unwrap().image_snapshot(),
                (0., 0.),
                None,
            );
            assert_eq!(
                rgba_pixels(&actual).unwrap(),
                rgba_pixels(&expected.image_snapshot()).unwrap(),
                "incremental placement and restoration must match complete output"
            );
        }
    }

    #[test]
    fn padded_tiles_match_contiguous_preview_at_fractional_zoom_and_pan() {
        use crate::{
            asset::ImageAsset,
            raster_snapshot::{RasterPatch, RasterSnapshot},
        };
        let (w, h) = (777, 555);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[
                    (x % 256) as u8,
                    (y % 256) as u8,
                    173,
                    if (x / 13 + y / 13) % 2 == 0 { 137 } else { 255 },
                ]);
            }
        }
        let base = ImageAsset::Raster(Arc::new(rgba_image(w, h, &data).unwrap()));
        let mut doc = Document::new("Tile seams", w, h).unwrap();
        doc.layers.push(Layer::new(
            "Raster",
            w,
            h,
            Content::Image { data: base.clone() },
        ));
        let mut retained = Renderer::default();
        for changed in [false, true, false, true] {
            doc.layers[0].content = Arc::new(Content::Image {
                data: if changed {
                    let pixels = [240, 19, 87, 91].repeat(256 * 256);
                    ImageAsset::Tiled(Arc::new(
                        RasterSnapshot::replacing(
                            Some(&base),
                            (w, h),
                            vec![RasterPatch::new(
                                sk::IRect::from_xywh(256, 256, 256, 256),
                                Arc::new(rgba_image(256, 256, &pixels).unwrap()),
                            )],
                            sk::IRect::from_wh(w as i32, h as i32),
                        )
                        .unwrap(),
                    ))
                } else {
                    base.clone()
                },
            });
            for zoom in [0.237777, 0.713333, 1., 1.37, 2.5] {
                let viewport = Viewport {
                    width: 619.,
                    height: 431.,
                    zoom,
                    pan: Point::new(17.25, -28.125),
                };
                let actual = retained
                    .preview(&doc, &viewport, &[], false, None, None, None, None)
                    .unwrap()
                    .image_snapshot();
                let mut flat = doc.clone();
                if let Content::Image {
                    data: ImageAsset::Tiled(raster),
                } = flat.layers[0].content.as_ref()
                {
                    flat.layers[0].content = Arc::new(Content::Image {
                        data: ImageAsset::Raster(Arc::new(raster.image().unwrap())),
                    });
                }
                let mut reference = Renderer::default();
                let mut expected = surface(619, 431).unwrap();
                let c = expected.canvas();
                c.draw_image(
                    reference
                        .preview_background(&flat, &viewport, rect(w, h))
                        .unwrap(),
                    (0., 0.),
                    None,
                );
                c.save();
                let o = geometry::canvas_origin(&flat, &viewport);
                c.translate(point(o));
                c.scale((zoom as f32, zoom as f32));
                c.clip_rect(rect(w, h), None, false);
                c.draw_image_with_sampling_options(
                    reference.render(&flat).unwrap().image_snapshot(),
                    (0., 0.),
                    sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
                    None,
                );
                c.restore();
                let a = rgba_pixels(&actual).unwrap();
                let b = rgba_pixels(&expected.image_snapshot()).unwrap();
                let differences = a
                    .chunks_exact(4)
                    .zip(b.chunks_exact(4))
                    .filter(|(a, b)| a != b)
                    .count();
                let maximum = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                // Local tile origins can change Skia's float interpolation rounding
                // by one 8-bit step at fractional zoom; gutters prevent missing
                // neighbors or visible seams. Unit zoom must remain byte-exact.
                assert!(
                    maximum <= if zoom == 1. { 0 } else { 1 },
                    "zoom {zoom}, changed {changed}, max {maximum}"
                );
                assert!(
                    differences < 619 * 431 / 100,
                    "unexpected extent of sampling differences"
                );
            }
        }
    }

    #[test]
    fn immutable_snapshots_survive_later_redraws_and_clean_frames_reuse_pixels() {
        let mut doc = Document::new("Retained preview", 64, 48).unwrap();
        doc.layers.push(Layer::new(
            "Blue",
            20,
            20,
            Content::Shape {
                shape: Shape::Ellipse,
                color: "#6587ff80".into(),
            },
        ));
        let mut renderer = Renderer::default();
        let original = renderer.preview_composite(&doc).unwrap();
        let original_pixels = rgba_pixels(&original).unwrap();
        doc.name = "Renamed".into();
        doc.layers[0].name = "Renamed layer".into();
        doc.layers[0].locked = true;
        assert_eq!(
            original.unique_id(),
            renderer.preview_composite(&doc).unwrap().unique_id()
        );
        doc.layers[0].x = 9.25;
        let updated = renderer.preview_composite(&doc).unwrap();
        assert_ne!(original_pixels, rgba_pixels(&updated).unwrap());
        assert_eq!(original_pixels, rgba_pixels(&original).unwrap());
        assert_eq!(
            rgba_pixels(&updated).unwrap(),
            rgba_pixels(&Renderer::default().render(&doc).unwrap().image_snapshot()).unwrap()
        );
    }
}
