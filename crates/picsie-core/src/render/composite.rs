//! Retained preview composition. This is a Skia backend adaptation, not a GIMP code port.
//! Compositor's EditorCanvas/TiledLayerRenderer also retain unchanged pixels and clear
//! replacement regions before drawing them. See docs/compositor-port.md for scope.
use super::*;

pub(super) struct Composite {
    document: Document,
    pixels: Vec<u8>,
    image: Image,
}

pub(super) struct Background {
    document_size: (u32, u32),
    viewport: Viewport,
    image: Image,
}

enum Damage {
    Clean,
    Region(Rect),
    Full,
}

// Source pixels and coverage are immutable native resources. Their identity avoids
// scanning large mask buffers or stroke arrays just to recognize an unchanged frame.
fn same_layer(a: &Layer, b: &Layer) -> bool {
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

fn damage(before: &Document, after: &Document) -> Damage {
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
        let mut placed = a.clone();
        placed.x = b.x;
        placed.y = b.y;
        if !same_layer(&placed, b) || !simple(a) || !simple(b) {
            return Damage::Full;
        }
        // Unit-scale source images stay within their bounds. Expand outward for
        // fractional placement, antialiasing and linear sampling, then clip to pixels.
        let bounds = Rect::new(
            (a.x.min(b.x) - 2.).floor().max(0.) as f32,
            (a.y.min(b.y) - 2.).floor().max(0.) as f32,
            (a.x.max(b.x) + b.width as f64 + 2.)
                .ceil()
                .min(after.width as f64) as f32,
            (a.y.max(b.y) + b.height as f64 + 2.)
                .ceil()
                .min(after.height as f64) as f32,
        );
        if !bounds.is_empty() {
            region = Some(match region {
                Some(old) => Rect::new(
                    old.left.min(bounds.left),
                    old.top.min(bounds.top),
                    old.right.max(bounds.right),
                    old.bottom.max(bounds.bottom),
                ),
                None => bounds,
            });
        }
    }
    if let Some(region) = region {
        // Keep dependency/effect damage conservative for this first optimization.
        // Skia's filtered transformed images can round differently when the clip
        // changes, even for a stationary overlapping layer. Preserve exact pixels
        // by restricting partial redraw to an entirely untransformed simple stack.
        if before
            .layers
            .iter()
            .chain(&after.layers)
            .any(|l| !simple(l))
        {
            Damage::Full
        } else {
            Damage::Region(region)
        }
    } else {
        Damage::Clean
    }
}

impl Renderer {
    pub(super) fn preview_background(&mut self, doc: &Document, v: &Viewport) -> Result<Image> {
        if let Some(background) = &self.background
            && background.document_size == (doc.width, doc.height)
            && background.viewport == *v
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
                o.x as f32,
                (o.y + 3.) as f32,
                (doc.width as f64 * v.zoom) as f32,
                (doc.height as f64 * v.zoom) as f32,
            ),
            &shadow,
        );
        c.save();
        c.translate(point(o));
        c.scale((v.zoom as f32, v.zoom as f32));
        c.clip_rect(rect(doc.width, doc.height), None, false);
        // EditorCanvas.swift 609dbeae, MIT © 2026 Wonder Assembly LLC.
        // Retain the viewport-clipped checkerboard in the Skia background cache.
        c.draw_rect(rect(doc.width, doc.height), &paint("#4d4d4d"));
        let tile = 10. / v.zoom;
        let sx = 0f64.max((-o.x / v.zoom / tile).floor()) as i32;
        let sy = 0f64.max((-o.y / v.zoom / tile).floor()) as i32;
        let ex = (doc.width as f64 / tile)
            .ceil()
            .min(((v.width - o.x) / v.zoom / tile).ceil()) as i32;
        let ey = (doc.height as f64 / tile)
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
            image: image.clone(),
        });
        Ok(image)
    }

    pub(super) fn preview_composite(&mut self, document: &Document) -> Result<Image> {
        // Retain Rust-owned pixels rather than a Skia Surface: the Node addon can
        // move its locked Renderer between worker threads, while Surface is !Send.
        // Skia borrows this buffer only during the current call, with no unsafe Send.
        let previous = self.composite.take();
        let damage = previous
            .as_ref()
            .map_or(Damage::Full, |c| damage(&c.document, document));
        if matches!(damage, Damage::Clean) {
            let mut composite = previous.expect("clean composite exists");
            composite.document = document.clone();
            let image = composite.image.clone();
            self.composite = Some(composite);
            return Ok(image);
        }
        dimensions(document.width, document.height)?;
        let mut pixels = previous.map_or_else(Vec::new, |c| c.pixels);
        pixels.resize(document.width as usize * document.height as usize * 4, 0);
        let info =
            sk::ImageInfo::new_n32_premul((document.width as i32, document.height as i32), None);
        let image = {
            let mut working =
                sk::surfaces::wrap_pixels(&info, &mut pixels, document.width as usize * 4, None)
                    .ok_or_else(|| anyhow!("Cannot wrap retained composite pixels"))?;
            let canvas = working.canvas();
            if let Damage::Region(region) = damage {
                canvas.clip_rect(region, None, false);
            }
            canvas.clear(Color::TRANSPARENT);
            // Redraw the entire stack within the clip, including the backdrop and
            // unchanged overlapping layers. A failed redraw discards this buffer.
            self.draw_document(document, canvas)?;
            // Raster-direct snapshots own an immutable copy, so subsequent edits
            // cannot alter an image still held by a consumer on another thread.
            working.image_snapshot()
        };
        self.composite = Some(Composite {
            document: document.clone(),
            pixels,
            image: image.clone(),
        });
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
