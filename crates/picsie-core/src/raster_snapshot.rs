//! RasterSnapshot.swift, Compositor 609dbeae. MIT © 2026 Wonder Assembly LLC.
//! Immutable, flat replacement patches; Skia materializes contiguous pixels only
//! for consumers that request an Image. Unchanged source/patches remain shared.
use crate::{asset::ImageAsset, model::dimensions, render};
use anyhow::{Result, anyhow};
use skia_safe::{Canvas, ClipOp, IRect, Image, Paint, Rect, SamplingOptions};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
};

#[derive(Clone, Debug)]
pub struct RasterPatch {
    pub rect: IRect,
    pub image: Arc<Image>,
    // Skia subsets share their original pixel allocation. Count that allocation
    // once in history, rather than undercounting it as cropped visible pixels.
    backing: Arc<Image>,
}
#[derive(Debug)]
pub struct RasterSnapshot {
    pub width: u32,
    pub height: u32,
    pub base: Option<Arc<Image>>,
    pub base_rect: IRect,
    pub patches: Vec<RasterPatch>,
    materialized: OnceLock<Image>,
}
impl RasterPatch {
    pub fn new(rect: IRect, image: Arc<Image>) -> Self {
        Self {
            rect,
            backing: image.clone(),
            image,
        }
    }
    fn cropped(&self, rect: IRect) -> Result<Self> {
        if rect == self.rect {
            return Ok(self.clone());
        }
        let subset = rect.with_offset((-self.rect.left, -self.rect.top));
        let image = self
            .image
            .make_subset(None, subset, Default::default())
            .ok_or_else(|| anyhow!("Cannot crop raster patch"))?;
        Ok(Self {
            rect,
            image: Arc::new(image),
            backing: self.backing.clone(),
        })
    }
}
fn cells(rect: IRect) -> impl Iterator<Item = (i32, i32)> {
    (rect.top.div_euclid(256)..=(rect.bottom - 1).div_euclid(256)).flat_map(move |y| {
        (rect.left.div_euclid(256)..=(rect.right - 1).div_euclid(256)).map(move |x| (x, y))
    })
}
impl RasterSnapshot {
    /// RasterSnapshot.replacing: spatially indexed, disjoint replacement tiles.
    /// New transparent pixels replace old paint; snapshots never chain previous rasters.
    pub fn replacing(
        source: Option<&ImageAsset>,
        source_size: (u32, u32),
        additions: Vec<RasterPatch>,
        crop: IRect,
    ) -> Result<Self> {
        dimensions(crop.width() as u32, crop.height() as u32)?;
        let offset = (-crop.left, -crop.top);
        let old = match source {
            Some(ImageAsset::Tiled(old)) if (old.width, old.height) == source_size => {
                Some(old.as_ref())
            }
            _ => None,
        };
        let (base, base_rect) = if let Some(old) = old {
            (old.base.clone(), old.base_rect.with_offset(offset))
        } else {
            (
                source.map(ImageAsset::image).transpose()?.map(Arc::new),
                IRect::from_xywh(0, 0, source_size.0 as i32, source_size.1 as i32)
                    .with_offset(offset),
            )
        };
        let additions: Vec<_> = additions
            .into_iter()
            .map(|mut patch| {
                patch.rect.offset(offset);
                patch
            })
            .collect();
        let mut buckets: HashMap<_, Vec<usize>> = HashMap::new();
        for (index, patch) in additions.iter().enumerate() {
            for cell in cells(patch.rect) {
                buckets.entry(cell).or_default().push(index);
            }
        }
        let mut patches = vec![];
        if let Some(old) = old {
            for patch in &old.patches {
                let mut shifted = patch.clone();
                shifted.rect.offset(offset);
                let candidates: HashSet<_> = cells(shifted.rect)
                    .flat_map(|cell| buckets.get(&cell).into_iter().flatten().copied())
                    .collect();
                let mut pieces = vec![shifted];
                for index in candidates {
                    let mut next = vec![];
                    for piece in pieces {
                        if let Some(overlap) = IRect::intersect(&piece.rect, &additions[index].rect)
                        {
                            let r = piece.rect;
                            for rect in [
                                IRect::new(r.left, r.top, r.right, overlap.top),
                                IRect::new(r.left, overlap.bottom, r.right, r.bottom),
                                IRect::new(r.left, overlap.top, overlap.left, overlap.bottom),
                                IRect::new(overlap.right, overlap.top, r.right, overlap.bottom),
                            ] {
                                if !rect.is_empty() {
                                    next.push(piece.cropped(rect)?);
                                }
                            }
                        } else {
                            next.push(piece);
                        }
                    }
                    pieces = next;
                }
                patches.extend(pieces);
            }
        }
        patches.extend(additions);
        let bounds = IRect::from_wh(crop.width(), crop.height());
        let patches = patches
            .into_iter()
            .filter_map(|patch| IRect::intersect(&patch.rect, &bounds).map(|r| patch.cropped(r)))
            .collect::<Result<_>>()?;
        Ok(Self {
            width: crop.width() as u32,
            height: crop.height() as u32,
            base,
            base_rect,
            patches,
            materialized: OnceLock::new(),
        })
    }
    /// LayerRenderer.drawBrushPreview: disjoint clips apply opacity/blending once.
    pub fn draw(&self, canvas: &Canvas, sampling: SamplingOptions, paint: &Paint) {
        canvas.save();
        canvas.clip_rect(
            Rect::from_wh(self.width as f32, self.height as f32),
            None,
            false,
        );
        if let Some(base) = &self.base {
            canvas.save();
            for patch in &self.patches {
                canvas.clip_rect(Rect::from_irect(patch.rect), ClipOp::Difference, false);
            }
            canvas.draw_image_rect_with_sampling_options(
                base.as_ref(),
                None,
                Rect::from_irect(self.base_rect),
                sampling,
                paint,
            );
            canvas.restore();
        }
        for patch in &self.patches {
            canvas.save();
            canvas.clip_rect(Rect::from_irect(patch.rect), None, false);
            canvas.draw_image_with_sampling_options(
                patch.image.as_ref(),
                (patch.rect.left as f32, patch.rect.top as f32),
                sampling,
                Some(paint),
            );
            canvas.restore();
        }
        canvas.restore();
    }
    pub fn image(&self) -> Result<Image> {
        if let Some(image) = self.materialized.get() {
            return Ok(image.clone());
        }
        let mut surface = render::surface(self.width, self.height)?;
        self.draw(
            surface.canvas(),
            SamplingOptions::default(),
            &Paint::default(),
        );
        let image = surface.image_snapshot();
        let _ = self.materialized.set(image);
        Ok(self
            .materialized
            .get()
            .expect("materialized raster")
            .clone())
    }
    pub fn has_materialized_pixels(&self) -> bool {
        self.materialized.get().is_some()
    }
    pub fn storage_parts(&self) -> Vec<(usize, usize)> {
        let mut parts = vec![(
            self as *const _ as usize,
            self.patches.capacity() * std::mem::size_of::<RasterPatch>()
                + std::mem::size_of::<Self>(),
        )];
        if let Some(base) = &self.base {
            parts.push((
                base.unique_id() as usize,
                base.width() as usize * base.height() as usize * 4,
            ));
        }
        for patch in &self.patches {
            parts.push((
                patch.backing.unique_id() as usize,
                patch.backing.width() as usize * patch.backing.height() as usize * 4,
            ));
        }
        if let Some(image) = self.materialized.get() {
            parts.push((
                image.unique_id() as usize,
                image.width() as usize * image.height() as usize * 4,
            ));
        }
        parts
    }
}
