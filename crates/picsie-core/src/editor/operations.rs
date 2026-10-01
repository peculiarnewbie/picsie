//! SelectionClipboard.swift / FloatingSelection.swift / LayerMerge.swift, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Skia and immutable native assets replace CoreGraphics.
use super::*;
use skia_safe::{self as sk, Image, Matrix, Rect};

#[derive(Clone)]
pub struct PixelClipboard {
    pub image: Image,
    pub origin: Point,
}
pub(super) struct Floating {
    source: Layer,
    original: Layer,
    pub(super) selection: PixelSelection,
    before: Document,
    active: Selection,
    pixels: PixelClipboard,
    cleared: Layer,
    pub(super) persistent: bool,
}
pub(super) fn layer_matrix(layer: &Layer) -> Matrix {
    let p = geometry::to_world(layer, Point::default());
    let x = geometry::to_world(layer, Point::new(1., 0.));
    let y = geometry::to_world(layer, Point::new(0., 1.));
    Matrix::new_all(
        (x.x - p.x) as f32,
        (y.x - p.x) as f32,
        p.x as f32,
        (x.y - p.y) as f32,
        (y.y - p.y) as f32,
        p.y as f32,
        0.,
        0.,
        1.,
    )
}
impl Editor {
    pub fn transform_active(&self) -> bool {
        self.floating.is_some() || self.layer_transform
    }
    pub(super) fn cancel_polygon(&mut self) {
        self.polygon = None;
        self.polygon_cursor = None;
    }
    pub(super) fn finish_polygon(&mut self) -> Result<()> {
        if let Some(draft) = self.polygon.take() {
            self.polygon_cursor = None;
            let selection = pixel_selection::finish(
                &self.history.document,
                self.history.pixel_selection.as_ref(),
                &draft,
            )?;
            let selection = selection
                .map(|s| s.with_antialiasing(self.selection_antialiased))
                .transpose()?;
            self.set_pixel_selection(
                selection,
                if draft.bounds().is_some() {
                    "Polygonal Lasso"
                } else {
                    "Deselect"
                },
            )?;
        }
        Ok(())
    }
    pub(super) fn magic_wand(&mut self, point: Point, mode: PixelSelectionMode) -> Result<()> {
        let doc = &self.history.document;
        let mut renderer = render::Renderer::default();
        let image = if self.wand.sample_all_layers {
            renderer.render(doc)?.image_snapshot()
        } else {
            let mut output = render::surface(doc.width, doc.height)?;
            if let Some(layer) = self.selected()
                && !matches!(layer.content.as_ref(), Content::Group)
            {
                let mut bare = layer.clone();
                bare.mask = None;
                render::draw_pixels(output.canvas(), &bare, &renderer.layer_surface(&bare)?);
            }
            output.image_snapshot()
        };
        let mut pixels = render::rgba_pixels(&image)?;
        for p in pixels.chunks_exact_mut(4) {
            for c in 0..3 {
                p[c] = ((p[c] as u16 * p[3] as u16 + 127) / 255) as u8;
            }
        }
        let Some(outline) = crate::wand::select(&pixels, doc.width, doc.height, point, self.wand)?
        else {
            return Ok(());
        };
        let selected = if let Some(old) = &self.history.pixel_selection {
            old.combined(&outline, mode)?
        } else {
            if mode == PixelSelectionMode::Subtract {
                return Ok(());
            }
            PixelSelection::from_path(doc.width, doc.height, outline, 0.)?
        };
        self.set_pixel_selection(
            Some(selected.with_antialiasing(self.selection_antialiased)?),
            "Magic Wand",
        )
    }
    pub fn can_copy_pixels(&self) -> bool {
        self.can_edit_pixels()
            && self.selected().is_some_and(|l| {
                self.paint_target == PaintTarget::Mask
                    || !matches!(l.content.as_ref(), Content::Paint)
                    || !l.strokes.is_empty()
            })
    }
    /// Active content ignores its opacity, effects and mask; Copy Merged reads the visible composite.
    pub fn copy_pixels(&mut self, merged: bool) -> Result<Option<PixelClipboard>> {
        self.finish_gesture()?;
        if !merged && !self.can_copy_pixels() {
            return Ok(None);
        }
        let doc = &self.history.document;
        let region = if let Some(sel) = &self.history.pixel_selection {
            if sel.bounds.is_none() {
                return Ok(None);
            }
            let b = sel.outline.bounds();
            let grow = (sel.feather * 2.).ceil() as f32;
            Rect::new(
                (b.left - grow + 0.001).floor().max(0.),
                (b.top - grow + 0.001).floor().max(0.),
                (b.right + grow - 0.001).ceil().min(doc.width as f32),
                (b.bottom + grow - 0.001).ceil().min(doc.height as f32),
            )
        } else {
            Rect::from_wh(doc.width as f32, doc.height as f32)
        };
        if region.width() < 1. || region.height() < 1. {
            return Ok(None);
        }
        let mut output = render::surface(region.width() as u32, region.height() as u32)?;
        output.canvas().translate((-region.left, -region.top));
        if merged {
            let image = render::Renderer::default().render(doc)?.image_snapshot();
            output.canvas().draw_image(image, (0., 0.), None);
        } else if let Some(layer) = self.selected() {
            let mut renderer = render::Renderer::default();
            if self.paint_target == PaintTarget::Mask {
                let mask = layer.mask.as_ref().unwrap();
                let raster = render::rasterize_mask(mask, layer)?;
                let mut rgba = Vec::with_capacity(raster.pixels.len() * 4);
                for &v in raster.pixels.iter() {
                    rgba.extend([v, v, v, 255]);
                }
                let image = render::rgba_image(raster.width, raster.height, &rgba)?;
                let placed = mask
                    .placement
                    .map(|p| p.as_layer(layer))
                    .unwrap_or_else(|| layer.clone());
                let background = if mask.placement.is_some() {
                    raster.pixels[0]
                } else {
                    0
                };
                output
                    .canvas()
                    .clear(sk::Color::from_rgb(background, background, background));
                render::draw_pixels(output.canvas(), &placed, &image);
            } else {
                let mut bare = layer.clone();
                bare.mask = None;
                render::draw_pixels(output.canvas(), layer, &renderer.layer_surface(&bare)?);
            }
        }
        if let Some(sel) = &self.history.pixel_selection {
            let coverage = render::selection_coverage(sel)?;
            let mut rgba = render::rgba_pixels(&output.image_snapshot())?;
            for (i, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                let x = region.left as usize + i % region.width() as usize;
                let y = region.top as usize + i / region.width() as usize;
                pixel[3] = ((pixel[3] as u32 * coverage[y * doc.width as usize + x] as u32 + 127)
                    / 255) as u8;
            }
            return Ok(Some(PixelClipboard {
                image: render::rgba_image(region.width() as u32, region.height() as u32, &rgba)?,
                origin: Point::new(region.left as f64, region.top as f64),
            }));
        }
        Ok(Some(PixelClipboard {
            image: output.image_snapshot(),
            origin: Point::new(region.left as f64, region.top as f64),
        }))
    }
    pub fn paste_pixels(&mut self, image: Image, origin: Option<Point>, label: &str) -> Result<()> {
        self.commit_transform()?;
        self.finish_gesture()?;
        ensure!(
            self.history.document.layers.len() < crate::model::MAX_LAYERS,
            "The editor supports up to 10,000 layers"
        );
        dimensions(image.width() as u32, image.height() as u32)?;
        let origin = origin.unwrap_or_else(|| {
            Point::new(
                ((self.history.document.width as f64 - image.width() as f64) / 2.).floor(),
                ((self.history.document.height as f64 - image.height() as f64) / 2.).floor(),
            )
        });
        let mut number = 1;
        while self
            .history
            .document
            .layers
            .iter()
            .any(|l| l.name == format!("Layer {number}"))
        {
            number += 1;
        }
        let mut layer = Layer::new(
            &format!("Layer {number}"),
            image.width() as u32,
            image.height() as u32,
            render::native_content(image),
        );
        layer.x = origin.x;
        layer.y = origin.y;
        let selected = layer.id.clone();
        let doc = self.inserted(vec![layer]);
        doc.validate()?;
        self.begin_edit(label);
        self.history.preview(doc);
        self.single_selection(Some(selected));
        self.history.pixel_selection = None;
        self.end_edit();
        Ok(())
    }
    pub fn layer_via_copy(&mut self) -> Result<()> {
        if self.history.pixel_selection.is_none() {
            return self.command(Command::Duplicate);
        }
        if let Some(pixels) = self.copy_pixels(false)? {
            self.paste_pixels(pixels.image, Some(pixels.origin), "Layer via Copy")?;
        }
        Ok(())
    }
    pub fn begin_selection_transform(&mut self, duplicate: bool, persistent: bool) -> Result<bool> {
        if self.floating.is_some()
            || self.paint_target == PaintTarget::Mask
            || !self.can_edit_pixels()
        {
            return Ok(false);
        }
        let Some(selection) = self
            .history
            .pixel_selection
            .clone()
            .filter(|s| s.bounds.is_some())
        else {
            return Ok(false);
        };
        let Some(pixels) = self.copy_pixels(false)? else {
            return Ok(false);
        };
        if !persistent
            && !render::rgba_pixels(&pixels.image)?
                .chunks_exact(4)
                .any(|p| p[3] != 0)
        {
            return Ok(false);
        }
        let source = self.selected().unwrap().clone();
        let mut cleared = source.clone();
        if !duplicate {
            cleared.content = Arc::new(Content::Image {
                data: render::clear_selected_asset(&source, &selection, &self.history.document)?,
            });
            cleared.strokes.clear();
        }
        let mut layer = Layer::new(
            "Floating Selection",
            pixels.image.width() as u32,
            pixels.image.height() as u32,
            render::native_content(pixels.image.clone()),
        );
        layer.x = pixels.origin.x;
        layer.y = pixels.origin.y;
        layer.parent_id = source.parent_id.clone();
        layer.opacity = source.opacity;
        layer.blend = source.blend;
        let before = self.history.document.clone();
        let active = self.selection.clone();
        let mut preview = before.clone();
        preview.replace(cleared.clone());
        let at = preview
            .layers
            .iter()
            .position(|l| l.id == source.id)
            .unwrap()
            + 1;
        preview.layers.insert(at, layer.clone());
        preview.validate()?;
        self.begin_edit(if persistent {
            "Transform Selection"
        } else if duplicate {
            "Duplicate Pixels"
        } else {
            "Move Pixels"
        });
        self.history.preview(preview);
        self.single_selection(Some(layer.id.clone()));
        if persistent {
            self.tool = Tool::Move;
        }
        self.floating = Some(Floating {
            source,
            original: layer,
            selection,
            before,
            active,
            pixels,
            cleared,
            persistent,
        });
        Ok(true)
    }
    pub(super) fn update_floating_selection(&mut self) -> Result<()> {
        if let Some(floating) = &self.floating
            && let Some(current) = self.selected()
        {
            let original = layer_matrix(&floating.original).invert().unwrap();
            let matrix = Matrix::concat(&layer_matrix(current), &original);
            self.history.pixel_selection = Some(floating.selection.transformed(&matrix)?);
        }
        Ok(())
    }
    pub fn cancel_transform(&mut self) {
        self.image_distortion = None;
        if self.layer_transform {
            self.mask_distortion = None;
            self.gesture = None;
            self.layer_transform = false;
            self.transform_pixel_size = None;
            let selected = self.history.cancel();
            self.restore_selection(selected);
            return;
        }
        if let Some(floating) = self.floating.take() {
            self.gesture = None;
            self.history.document = floating.before;
            self.history.pixel_selection = Some(floating.selection);
            self.selection = floating.active;
            let selection = self.history.cancel();
            self.restore_selection(selection);
        }
    }
    pub fn commit_transform(&mut self) -> Result<()> {
        let distorted = self.image_distortion.is_some();
        if let Some(corners) = self.image_distortion.as_ref().map(|d| d.corners) {
            self.finish_gesture()?;
            self.preview_image_distortion(corners, None)?;
            self.image_distortion = None;
        }
        if self.layer_transform {
            self.finish_gesture()?;
            if let Some(draft) = &self.mask_distortion {
                self.preview_mask_distortion(draft.corners, None)?;
            }
            self.mask_distortion = None;
            self.layer_transform = false;
            self.transform_pixel_size = None;
            self.end_edit();
            return Ok(());
        }
        if self.floating.is_none() {
            return Ok(());
        }
        self.finish_gesture()?;
        let current = self.selected().unwrap().clone();
        let floating = self.floating.take().unwrap();
        if !distorted && MaskPlacement::of(&current) == MaskPlacement::of(&floating.original) {
            self.history.document = floating.before;
            self.history.pixel_selection = Some(floating.selection);
            self.selection = floating.active;
            self.end_edit();
            return Ok(());
        }
        let result = (|| -> Result<Document> {
            let corners = geometry::corners(&current);
            let rect = Rect::new(
                corners
                    .iter()
                    .map(|p| p.x as f32)
                    .fold(f32::INFINITY, f32::min),
                corners
                    .iter()
                    .map(|p| p.y as f32)
                    .fold(f32::INFINITY, f32::min),
                corners
                    .iter()
                    .map(|p| p.x as f32)
                    .fold(f32::NEG_INFINITY, f32::max),
                corners
                    .iter()
                    .map(|p| p.y as f32)
                    .fold(f32::NEG_INFINITY, f32::max),
            );
            let (mut merged, dx, dy) = render::expanded_layer(&floating.cleared, rect)?;
            let mut base = floating.cleared.clone();
            base.mask = None;
            let image = render::Renderer::default().layer_surface(&base)?;
            let mut output = render::surface(merged.width, merged.height)?;
            output
                .canvas()
                .draw_image(image, (dx as f32, dy as f32), None);
            let inverse = layer_matrix(&merged)
                .invert()
                .ok_or_else(|| anyhow::anyhow!("Invalid transform"))?;
            output.canvas().concat(&inverse);
            if distorted {
                let mut bare = current.clone();
                bare.mask = None;
                let pixels = render::Renderer::default().layer_surface(&bare)?;
                render::draw_pixels(output.canvas(), &current, &pixels);
            } else {
                render::draw_pixels(output.canvas(), &current, &floating.pixels.image);
            }
            merged.content = Arc::new(render::native_content(output.image_snapshot()));
            merged.strokes.clear();
            // FloatingMerge grows an attached mask with white coverage in newly exposed source pixels.
            if let Some(mask) = &floating.source.mask
                && mask.placement.is_none()
                && (merged.width != floating.source.width
                    || merged.height != floating.source.height)
            {
                let raster = render::rasterize_mask(mask, &floating.source)?;
                let mut pixels = vec![255; merged.width as usize * merged.height as usize];
                for y in 0..floating.source.height {
                    for x in 0..floating.source.width {
                        let value = raster.pixels[((y as u64 * raster.height as u64
                            / floating.source.height as u64)
                            * raster.width as u64
                            + x as u64 * raster.width as u64 / floating.source.width as u64)
                            as usize];
                        pixels[(y as usize + dy as usize) * merged.width as usize
                            + x as usize
                            + dx as usize] = value;
                    }
                }
                let mut mask = mask.as_ref().clone();
                mask.raster = Some(Arc::new(MaskRaster {
                    width: merged.width,
                    height: merged.height,
                    pixels: Arc::new(pixels),
                }));
                mask.placement = None;
                mask.strokes.clear();
                merged.mask = Some(Arc::new(mask));
            }
            let mut doc = floating.before.clone();
            doc.replace(merged);
            doc.validate()?;
            Ok(doc)
        })();
        match result {
            Ok(doc) => {
                self.history.preview(doc);
                self.single_selection(Some(floating.source.id));
                self.end_edit();
                Ok(())
            }
            Err(error) => {
                self.history.document = floating.before;
                self.history.pixel_selection = Some(floating.selection);
                self.selection = floating.active;
                let selected = self.history.cancel();
                self.restore_selection(selected);
                Err(error)
            }
        }
    }
    pub fn move_pixels(&mut self, delta: Point, duplicate: bool) -> Result<()> {
        ensure!(
            delta.x.is_finite() && delta.y.is_finite(),
            "Invalid pixel movement"
        );
        if self.begin_selection_transform(duplicate, false)? {
            let layer = self.selected().unwrap();
            let patch =
                serde_json::json!({"x":layer.x + delta.x.round(), "y":layer.y + delta.y.round()});
            if let Err(error) = self.update_layer(patch) {
                self.cancel_transform();
                return Err(error);
            }
            self.update_floating_selection()?;
            self.commit_transform()?;
        }
        Ok(())
    }
    pub fn merge_layers(&mut self) -> Result<()> {
        self.commit_transform()?;
        self.finish_gesture()?;
        let doc = &self.history.document;
        let Some(active) = self.selected().cloned() else {
            return Ok(());
        };
        let mut included = HashSet::new();
        let mut label = "Merge Layers";
        if self.selection.ids.len() > 1 {
            for id in &self.selection.ids {
                included.insert(id.clone());
                included.extend(doc.descendants(id));
            }
        } else if matches!(active.content.as_ref(), Content::Group) {
            included.insert(active.id.clone());
            included.extend(doc.descendants(&active.id));
            label = "Merge Group";
        } else {
            let at = doc.layers.iter().position(|l| l.id == active.id).unwrap();
            let Some(below) = doc.layers[..at]
                .iter()
                .rev()
                .find(|l| l.parent_id == active.parent_id)
            else {
                return Ok(());
            };
            if matches!(below.content.as_ref(), Content::Group) {
                return Ok(());
            }
            included.extend([active.id.clone(), below.id.clone()]);
            label = "Merge Down";
        }
        if !doc
            .layers
            .iter()
            .any(|l| included.contains(&l.id) && !matches!(l.content.as_ref(), Content::Group))
        {
            return Ok(());
        }
        if doc
            .layers
            .iter()
            .any(|l| included.contains(&l.id) && l.locked)
        {
            return Ok(());
        }
        let mut subset = doc.clone();
        subset.layers.retain(|l| included.contains(&l.id));
        for l in &mut subset.layers {
            if l.parent_id
                .as_ref()
                .is_some_and(|id| !included.contains(id))
            {
                l.parent_id = None;
            }
            if l.mask_source_id
                .as_ref()
                .is_some_and(|id| !included.contains(id))
            {
                l.mask_source_id = None;
            }
        }
        let image = render::Renderer::default()
            .render(&subset)?
            .image_snapshot();
        let (image, origin) = trim_image(&image)?;
        let anchor = doc
            .layers
            .iter()
            .rposition(|l| self.selection.ids.contains(&l.id))
            .unwrap();
        let top = &doc.layers[anchor];
        let mut merged = Layer::new(
            &top.name,
            image.width() as u32,
            image.height() as u32,
            render::native_content(image),
        );
        merged.x = origin.x;
        merged.y = origin.y;
        merged.parent_id = top.parent_id.clone().filter(|id| !included.contains(id));
        if label == "Merge Down" {
            merged.name = doc
                .layers
                .iter()
                .find(|l| included.contains(&l.id) && l.id != active.id)
                .unwrap()
                .name
                .clone();
        }
        let slot = doc.layers[..anchor]
            .iter()
            .filter(|l| !included.contains(&l.id))
            .count();
        let selected = merged.id.clone();
        let mut next = doc.clone();
        next.layers.retain(|l| !included.contains(&l.id));
        for l in &mut next.layers {
            if l.mask_source_id
                .as_ref()
                .is_some_and(|id| included.contains(id))
            {
                l.mask_source_id = Some(selected.clone());
            }
        }
        next.layers.insert(slot, merged);
        self.edit(label, next, Some(vec![selected]))
    }
}
fn trim_image(image: &Image) -> Result<(Image, Point)> {
    let pixels = render::rgba_pixels(image)?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
    for (i, p) in pixels.chunks_exact(4).enumerate() {
        if p[3] > 0 {
            left = left.min(i % w);
            top = top.min(i / w);
            right = right.max(i % w + 1);
            bottom = bottom.max(i / w + 1);
        }
    }
    if right == 0 {
        return Ok((render::surface(1, 1)?.image_snapshot(), Point::default()));
    }
    let mut out = render::surface((right - left) as u32, (bottom - top) as u32)?;
    out.canvas()
        .draw_image(image, (-(left as f32), -(top as f32)), None);
    Ok((out.image_snapshot(), Point::new(left as f64, top as f64)))
}
