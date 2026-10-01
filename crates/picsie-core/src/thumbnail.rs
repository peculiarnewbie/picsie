//! CanvasThumbnail.swift layer previews, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Skia replaces Core Graphics; BGRA stays in Rust.
use crate::{
    model::{Content, Document, Layer},
    render::{self, Renderer},
};
use anyhow::Result;
use std::{collections::HashMap, sync::Arc};
#[derive(Debug)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
pub type ThumbnailSet = Arc<Vec<(String, Arc<Thumbnail>)>>;
struct Entry {
    layer: Layer,
    canvas: (u32, u32),
    image: Arc<Thumbnail>,
}
#[derive(Default)]
pub struct Thumbnails {
    entries: HashMap<String, Entry>,
    masks: HashMap<String, Entry>,
    document: Option<Document>,
    published: ThumbnailSet,
}
fn same_mask(a: &Layer, b: &Layer) -> bool {
    let (Some(a), Some(b)) = (&a.mask, &b.mask) else {
        return false;
    };
    a.base == b.base
        && a.placement == b.placement
        && match (&a.raster, &b.raster) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
        && a.strokes.len() == b.strokes.len()
        && a.strokes
            .iter()
            .zip(&b.strokes)
            .all(|(a, b)| Arc::ptr_eq(a, b))
}
/// CanvasThumbnail.edgeTone: outermost pixels, with corners counted only once.
pub fn edge_tone(pixels: &[u8], width: u32, height: u32) -> u8 {
    let (mut total, mut count) = (0u64, 0u64);
    for y in 0..height {
        for x in 0..width {
            if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
                total += pixels[(y * width + x) as usize] as u64;
                count += 1;
            }
        }
    }
    if count == 0 {
        255
    } else {
        (total as f64 / count as f64).round() as u8
    }
}
fn same_pixels(a: &Layer, b: &Layer) -> bool {
    Arc::ptr_eq(&a.content, &b.content)
        && a.strokes.len() == b.strokes.len()
        && a.strokes
            .iter()
            .zip(&b.strokes)
            .all(|(a, b)| Arc::ptr_eq(a, b))
        && a.width == b.width
        && a.height == b.height
        && a.x == b.x
        && a.y == b.y
        && a.scale_x == b.scale_x
        && a.scale_y == b.scale_y
        && a.rotation == b.rotation
        && a.flip_x == b.flip_x
        && a.flip_y == b.flip_y
        && a.sampling == b.sampling
}
impl Thumbnails {
    /// Appearance/selection changes reuse the picture, just as NativeLayerList's ThumbnailKey does.
    /// Deleted layers release both their source reference and their small image.
    pub fn update(&mut self, doc: &Document, renderer: &mut Renderer) -> Result<ThumbnailSet> {
        if self
            .document
            .as_ref()
            .is_some_and(|old| old.retained_eq(doc))
        {
            return Ok(self.published.clone());
        }
        renderer.retain_thumbnail_sources(doc);
        let ids: HashMap<_, _> = doc.layers.iter().map(|l| (l.id.as_str(), l)).collect();
        self.entries.retain(|id, _| ids.contains_key(id.as_str()));
        self.masks
            .retain(|id, _| ids.get(id.as_str()).is_some_and(|l| l.mask.is_some()));
        let mut result = Vec::new();
        for layer in &doc.layers {
            let mut mask_thumbnail = None;
            if let Some(mask) = &layer.mask {
                let canvas = (doc.width, doc.height);
                if !self.masks.get(&layer.id).is_some_and(|e| {
                    e.canvas == canvas && same_pixels(&e.layer, layer) && same_mask(&e.layer, layer)
                }) {
                    let raster = render::rasterize_mask(mask, layer)?;
                    let tone = edge_tone(&raster.pixels, raster.width, raster.height);
                    let scale = 36. / doc.width.max(doc.height) as f64;
                    let width = (doc.width as f64 * scale).round().max(1.) as u32 * 2;
                    let height = (doc.height as f64 * scale).round().max(1.) as u32 * 2;
                    let mut surface = render::surface(width, height)?;
                    let c = surface.canvas();
                    c.clear(skia_safe::Color::from_rgb(tone, tone, tone));
                    let info = skia_safe::ImageInfo::new(
                        (raster.width as i32, raster.height as i32),
                        skia_safe::ColorType::Gray8,
                        skia_safe::AlphaType::Opaque,
                        None,
                    );
                    let image = skia_safe::images::raster_from_data(
                        &info,
                        skia_safe::Data::new_copy(raster.pixels.as_slice()),
                        raster.width as usize,
                    )
                    .ok_or_else(|| anyhow::anyhow!("Cannot render mask thumbnail"))?;
                    let placed = mask
                        .placement
                        .unwrap_or_else(|| crate::model::MaskPlacement::of(layer))
                        .as_layer(layer);
                    let scale = width as f32 / doc.width as f32;
                    c.scale((scale, scale));
                    render::draw_pixels(c, &placed, &image);
                    let pixels = render::bgra_pixels(&mut surface)?;
                    self.masks.insert(
                        layer.id.clone(),
                        Entry {
                            layer: layer.clone(),
                            canvas,
                            image: Arc::new(Thumbnail {
                                width,
                                height,
                                pixels,
                            }),
                        },
                    );
                }
                mask_thumbnail = Some((
                    format!("@mask-{}", layer.id),
                    self.masks[&layer.id].image.clone(),
                ));
            }
            if matches!(layer.content.as_ref(), Content::Group) {
                result.extend(mask_thumbnail);
                continue;
            }
            let canvas = (doc.width, doc.height);
            if !self
                .entries
                .get(&layer.id)
                .is_some_and(|entry| entry.canvas == canvas && same_pixels(&entry.layer, layer))
            {
                let scale = 36. / doc.width.max(doc.height) as f64;
                let width = (doc.width as f64 * scale).round().max(1.) as u32 * 2;
                let height = (doc.height as f64 * scale).round().max(1.) as u32 * 2;
                let mut surface = render::surface(width, height)?;
                let c = surface.canvas();
                c.clear(skia_safe::Color::from_rgb(56, 56, 56));
                let mut paint = skia_safe::Paint::default();
                paint.set_color(skia_safe::Color::from_rgb(82, 82, 82));
                for y in (0..height).step_by(12) {
                    for x in (0..width).step_by(12) {
                        if (x / 12 + y / 12) % 2 == 0 {
                            c.draw_rect(
                                skia_safe::Rect::from_xywh(x as f32, y as f32, 12., 12.),
                                &paint,
                            );
                        }
                    }
                }
                // Thumbnails show the source, before opacity, filters, masks and live clipping.
                let mut source = layer.clone();
                source.mask = None;
                let scale = width as f32 / doc.width as f32;
                c.scale((scale, scale));
                renderer.draw_thumbnail_source(doc, layer, c)?;
                let pixels = render::bgra_pixels(&mut surface)?;
                self.entries.insert(
                    layer.id.clone(),
                    Entry {
                        layer: source,
                        canvas,
                        image: Arc::new(Thumbnail {
                            width,
                            height,
                            pixels,
                        }),
                    },
                );
            }
            result.push((layer.id.clone(), self.entries[&layer.id].image.clone()));
            result.extend(mask_thumbnail);
        }
        self.document = Some(doc.clone());
        self.published = Arc::new(result);
        Ok(self.published.clone())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Shape;
    #[test]
    fn canvas_shape_and_pixel_placement_follow_source() {
        let mut doc = Document::new("Thumb", 100, 50).unwrap();
        let mut layer = Layer::new(
            "Red",
            25,
            25,
            Content::Shape {
                shape: Shape::Rectangle,
                color: "#ff0000".into(),
            },
        );
        layer.x = 25.;
        layer.mask = Some(Arc::new(crate::model::LayerMask {
            enabled: true,
            base: crate::model::MaskMode::Hide,
            linked: true,
            raster: None,
            placement: None,
            strokes: vec![],
        }));
        doc.layers.push(layer);
        let image = Thumbnails::default()
            .update(&doc, &mut Renderer::default())
            .unwrap()[0]
            .1
            .clone();
        assert_eq!((image.width, image.height), (72, 36));
        let at = |x: usize, y: usize| &image.pixels[(y * 72 + x) * 4..(y * 72 + x) * 4 + 4];
        assert_eq!(at(25, 10), [0, 0, 255, 255]);
        assert_ne!(at(5, 10), [0, 0, 255, 255]);
    }
    #[test]
    fn selection_appearance_reuses_image_geometry_and_content_invalidate() {
        let mut doc = Document::new("Thumb", 100, 100).unwrap();
        doc.layers.push(Layer::new(
            "Red",
            25,
            25,
            Content::Shape {
                shape: Shape::Rectangle,
                color: "#ff0000".into(),
            },
        ));
        let mut thumbs = Thumbnails::default();
        let mut renderer = Renderer::default();
        let first_set = thumbs.update(&doc, &mut renderer).unwrap();
        assert!(Arc::ptr_eq(
            &first_set,
            &thumbs.update(&doc, &mut renderer).unwrap()
        ));
        let first = thumbs.update(&doc, &mut renderer).unwrap()[0].1.clone();
        doc.layers[0].opacity = 0.5;
        doc.layers[0].name = "Renamed".into();
        doc.layers[0].locked = true;
        assert!(Arc::ptr_eq(
            &first,
            &thumbs.update(&doc, &mut renderer).unwrap()[0].1.clone()
        ));
        doc.layers[0].x += 10.;
        let moved = thumbs.update(&doc, &mut renderer).unwrap()[0].1.clone();
        assert!(!Arc::ptr_eq(&first, &moved));
        doc.layers[0].content = Arc::new(Content::Shape {
            shape: Shape::Rectangle,
            color: "#0000ff".into(),
        });
        let recolored = thumbs.update(&doc, &mut renderer).unwrap()[0].1.clone();
        assert!(!Arc::ptr_eq(&moved, &recolored));
        doc.layers[0].mask = Some(Arc::new(crate::model::LayerMask {
            enabled: true,
            base: crate::model::MaskMode::Hide,
            linked: true,
            raster: None,
            placement: None,
            strokes: vec![],
        }));
        assert!(Arc::ptr_eq(
            &recolored,
            &thumbs.update(&doc, &mut renderer).unwrap()[0].1.clone()
        ));
        doc.width = 200;
        assert!(!Arc::ptr_eq(
            &moved,
            &thumbs.update(&doc, &mut renderer).unwrap()[0].1.clone()
        ));
        doc.layers.clear();
        assert!(thumbs.update(&doc, &mut renderer).unwrap().is_empty());
        assert_eq!(
            first_set.len(),
            1,
            "a held publication stays immutable after deletion"
        );
        assert!(thumbs.entries.is_empty());
    }
}
