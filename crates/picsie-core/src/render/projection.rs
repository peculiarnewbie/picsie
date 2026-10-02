//! Retained clipping nodes and bounded projection drawing. Clipping math is adapted
//! from LiveMaskRenderer.swift, Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
//! GIMP's damage-driven projection informed the cache architecture; no GIMP code
//! is translated here. Cached nodes retain source identity and effective opacity.
use super::*;
use crate::layer_index::LayerIndex;
use skia_safe::RoundOut;

pub(super) struct Stack {
    layers: Vec<(Layer, f64)>,
    bounds: sk::IRect,
    image: Image,
    used: u64,
}

pub(super) struct Node {
    pub layers: std::ops::Range<usize>,
    bounds: Rect,
}
pub(super) struct Projection<'a> {
    pub index: LayerIndex<'a>,
    pub layers: Vec<&'a Layer>,
    pub nodes: Vec<Node>,
}
impl<'a> Projection<'a> {
    pub fn intersects(&self, range: std::ops::Range<usize>, bounds: Rect) -> bool {
        self.nodes[range]
            .iter()
            .any(|node| node.bounds.intersects(bounds))
    }
    pub fn new(document: &'a Document) -> Self {
        let index = LayerIndex::new(document);
        let layers: Vec<_> = index
            .ordered_layers()
            .into_iter()
            .filter(|l| index.effective(l).0 && !matches!(l.content.as_ref(), Content::Group))
            .collect();
        let mut nodes = Vec::new();
        let mut at = 0;
        while at < layers.len() {
            let layer = layers[at];
            let mut end = at + 1;
            // LiveMaskRenderer.prepareStacks (609dbeae): adjustments never anchor
            // a clipping stack, but clipped adjustments stay inside their base's
            // stack so they recolor the base before it blends over the backdrop.
            if layer.mask_source_id.is_none() && layer.adjustment.is_none() {
                while end < layers.len()
                    && layers[end].mask_source_id.as_deref() == Some(&layer.id)
                    && layers[end].parent_id == layer.parent_id
                {
                    end += 1;
                }
            }
            nodes.push(Node {
                layers: at..end,
                bounds: support(layer),
            });
            at = end;
        }
        Self {
            index,
            layers,
            nodes,
        }
    }
    pub fn has_adjustments(&self) -> bool {
        self.layers.iter().any(|l| l.adjustment.is_some())
    }
}

/// Conservative document-space support, including interpolation and Gaussian
/// blur tails. Integer placement keeps the source sampling grid unchanged.
pub(super) fn support(layer: &Layer) -> Rect {
    let corners = geometry::corners(layer);
    let pad = (layer.blur * layer.scale_x.abs().max(layer.scale_y.abs()) * 4. + 2.) as f32;
    Rect::new(
        corners
            .iter()
            .map(|p| p.x as f32)
            .fold(f32::INFINITY, f32::min)
            .floor()
            - pad,
        corners
            .iter()
            .map(|p| p.y as f32)
            .fold(f32::INFINITY, f32::min)
            .floor()
            - pad,
        corners
            .iter()
            .map(|p| p.x as f32)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            + pad,
        corners
            .iter()
            .map(|p| p.y as f32)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            + pad,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Document {
        let mut doc = Document::new("Projection oracle", 137, 119).unwrap();
        doc.version = 2;
        let mut group = Layer::new("Folder", 137, 119, Content::Group);
        group.opacity = 0.73;
        let parent = group.id.clone();
        doc.layers.push(group);
        for i in 0..18 {
            let pixels: Vec<_> = (0..29 * 23)
                .flat_map(|j| {
                    [
                        (j * 13 % 256) as u8,
                        (j * 7 % 256) as u8,
                        109,
                        (j * 17 % 256) as u8,
                    ]
                })
                .collect();
            let mut layer = Layer::new(
                "Raster",
                29,
                23,
                native_content(rgba_image(29, 23, &pixels).unwrap()),
            );
            layer.x = (i * 17 % 110) as f64 - 12.5;
            layer.y = (i * 11 % 93) as f64 - 7.;
            layer.rotation = (i % 4) as f64 * 13.;
            layer.scale_x = 1.37;
            layer.scale_y = 0.84;
            layer.opacity = 0.8;
            layer.brightness = 1.12;
            layer.blur = if i % 6 == 0 { 1.7 } else { 0. };
            layer.blend = if i % 3 == 0 {
                Blend::Multiply
            } else {
                Blend::SourceOver
            };
            if i < 12 {
                layer.parent_id = Some(parent.clone());
            }
            if i % 3 != 0 {
                layer.mask_source_id = Some(doc.layers[doc.layers.len() - i % 3].id.clone());
            }
            doc.layers.push(layer);
        }
        doc
    }

    fn baseline(doc: &Document) -> Vec<u8> {
        let mut output = surface(doc.width, doc.height).unwrap();
        Renderer::default()
            .draw_baseline(doc, output.canvas())
            .unwrap();
        rgba_pixels(&output.image_snapshot()).unwrap()
    }

    fn assert_reference(actual: &Image, expected: &[u8], tolerance: u16) {
        let actual = rgba_pixels(actual).unwrap();
        let first = actual.iter().zip(expected).position(|(a, b)| a != b);
        let worst = actual
            .iter()
            .zip(expected)
            .enumerate()
            .max_by_key(|(_, (a, b))| a.abs_diff(**b))
            .unwrap()
            .0;
        let premul = |p: &[u8], c: usize| {
            if c == 3 {
                p[3] as u16
            } else {
                (p[c] as u16 * p[3] as u16 + 127) / 255
            }
        };
        let mut premul_max = 0;
        for (a, b) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
            for c in 0..4 {
                premul_max = premul_max.max(premul(a, c).abs_diff(premul(b, c)));
            }
        }
        assert!(
            premul_max <= tolerance,
            "first {first:?}, worst {worst}, premul {premul_max}, a {:?}, b {:?}",
            &actual[worst / 4 * 4..worst / 4 * 4 + 4],
            &expected[worst / 4 * 4..worst / 4 * 4 + 4]
        );
    }

    fn cold(doc: &Document) -> Vec<u8> {
        rgba_pixels(&Renderer::default().preview_composite(doc).unwrap()).unwrap()
    }
    fn assert_exact(actual: &Image, expected: &[u8]) {
        let pixels = rgba_pixels(actual).unwrap();
        assert!(
            pixels == expected,
            "retained projection differs from cold projection at {:?}",
            pixels.iter().zip(expected).position(|(a, b)| a != b)
        );
    }

    #[test]
    fn bounded_clipping_matches_frozen_full_document_math() {
        let mut doc = fixture();
        let mut renderer = Renderer::default();
        assert_reference(
            &renderer.render(&doc).unwrap().image_snapshot(),
            &baseline(&doc),
            1,
        );
        // Bounded scratch origins may change Skia rounding by one premultiplied
        // channel step; no interpolation, effect or opacity defaults change.
        // Hidden independent sources, chained masks, folder opacity and a changed
        // sampling mode all continue to use the same pre-rewrite pixel math.
        doc.layers[17].visible = false;
        doc.layers[18].mask_source_id = Some(doc.layers[17].id.clone());
        doc.layers[16].sampling = Sampling::Nearest;
        doc.layers[0].opacity = 0.41;
        assert_reference(
            &renderer.render(&doc).unwrap().image_snapshot(),
            &baseline(&doc),
            1,
        );
    }

    #[test]
    fn retained_damage_matches_cold_render_after_reorder_edit_delete_and_undo() {
        let original = fixture();
        let mut doc = original.clone();
        let mut renderer = Renderer::default();
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        doc.layers.swap(16, 17);
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        doc.layers[3].x += 9.;
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        doc.layers[0].opacity = 0.37;
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        doc.layers[0].mask = Some(Arc::new(LayerMask {
            enabled: true,
            base: MaskMode::Reveal,
            linked: true,
            placement: None,
            strokes: Vec::new(),
            raster: Some(Arc::new(MaskRaster {
                width: 2,
                height: 2,
                pixels: Arc::new(vec![255, 90, 12, 170]),
            })),
        }));
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        doc.layers[4].visible = false;
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        doc.layers[13].mask_source_id = Some(doc.layers[17].id.clone());
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        doc.layers.remove(18);
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        assert_exact(
            &renderer.preview_composite(&original).unwrap(),
            &cold(&original),
        );
    }

    #[test]
    fn unrelated_reorder_reuses_clipping_nodes_and_source_edit_invalidates_them() {
        let mut doc = fixture();
        let id = doc.layers[1].id.clone();
        let mut renderer = Renderer::default();
        renderer.render(&doc).unwrap();
        let original = renderer.stacks[&id].image.unique_id();
        doc.layers.swap(16, 17);
        renderer.render(&doc).unwrap();
        assert_eq!(renderer.stacks[&id].image.unique_id(), original);
        doc.layers[1].opacity = 0.3;
        renderer.render(&doc).unwrap();
        assert_ne!(renderer.stacks[&id].image.unique_id(), original);
        assert_exact(
            &renderer.render(&doc).unwrap().image_snapshot(),
            &cold(&doc),
        );
    }

    #[test]
    fn partial_projection_is_exact_across_fixed_chunk_boundaries() {
        let mut doc = fixture();
        doc.width = 777;
        doc.height = 555;
        for layer in &mut doc.layers[1..] {
            layer.x = layer.x * 4. + 207.5;
            layer.y = layer.y * 3. + 221.;
        }
        let mut renderer = Renderer::default();
        assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        for i in 1..doc.layers.len() {
            doc.layers[i].x += 31.25;
            doc.layers[i].y -= 17.5;
            assert_exact(&renderer.preview_composite(&doc).unwrap(), &cold(&doc));
        }
    }
}

impl Renderer {
    fn stack_image(
        &mut self,
        index: &LayerIndex<'_>,
        layers: &[&Layer],
        clip: Rect,
    ) -> Result<Option<(Image, sk::IRect)>> {
        let full: sk::IRect = support(layers[0]).round_out();
        let Some(full) = sk::IRect::intersect(
            &full,
            &sk::IRect::from_wh(index.document.width as i32, index.document.height as i32),
        ) else {
            return Ok(None);
        };
        if !Rect::from_irect(full).intersects(clip) {
            return Ok(None);
        }
        // Cache the complete base footprint for reuse after pan/zoom. Extremely
        // large transformed footprints use just the current clip instead.
        let bounds = if full.width() as i64 * full.height() as i64 <= MAX_PIXELS as i64 {
            full
        } else {
            sk::IRect::intersect(&full, &clip.round_out()).unwrap()
        };
        self.clock += 1;
        if let Some(stack) = self.stacks.get_mut(&layers[0].id)
            && stack.bounds == bounds
            && stack.layers.len() == layers.len()
            && stack
                .layers
                .iter()
                .zip(layers)
                .all(|((old, opacity), next)| {
                    composite::same_layer(old, next) && *opacity == index.effective(next).1
                })
        {
            stack.used = self.clock;
            return Ok(Some((stack.image.clone(), bounds)));
        }
        let mut base = surface(bounds.width() as u32, bounds.height() as u32)?;
        base.canvas()
            .translate((-bounds.left as f32, -bounds.top as f32));
        self.draw_own_with_opacity(
            layers[0],
            base.canvas(),
            Blend::SourceOver,
            index.effective(layers[0]).1,
        )?;
        let mut pixels = rgba_pixels(&base.image_snapshot())?;
        let coverage: Vec<_> = pixels
            .chunks_exact_mut(4)
            .map(|pixel| {
                let alpha = pixel[3];
                pixel[3] = 255;
                alpha
            })
            .collect();
        base.canvas().clear(Color::TRANSPARENT);
        base.canvas().draw_image(
            rgba_image(bounds.width() as u32, bounds.height() as u32, &pixels)?,
            (bounds.left as f32, bounds.top as f32),
            None,
        );
        for layer in &layers[1..] {
            self.draw_own_with_opacity(
                layer,
                base.canvas(),
                layer.blend,
                index.effective(layer).1,
            )?;
        }
        pixels = rgba_pixels(&base.image_snapshot())?;
        for (pixel, alpha) in pixels.chunks_exact_mut(4).zip(coverage) {
            pixel[3] = alpha;
        }
        let image = rgba_image(bounds.width() as u32, bounds.height() as u32, &pixels)?;
        let stack = Stack {
            layers: layers
                .iter()
                .map(|layer| ((*layer).clone(), index.effective(layer).1))
                .collect(),
            bounds,
            image: image.clone(),
            used: self.clock,
        };
        if let Some(old) = self.stacks.remove(&layers[0].id) {
            self.stack_pixels -= old.image.width() as u64 * old.image.height() as u64;
        }
        self.stack_pixels += image.width() as u64 * image.height() as u64;
        self.stacks.insert(layers[0].id.clone(), stack);
        while self.stack_pixels > MAX_PIXELS {
            let Some(id) = self
                .stacks
                .iter()
                .min_by_key(|(_, stack)| stack.used)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            let old = self.stacks.remove(&id).unwrap();
            self.stack_pixels -= old.image.width() as u64 * old.image.height() as u64;
        }
        Ok(Some((image, bounds)))
    }

    fn coverage_region(
        &mut self,
        index: &LayerIndex<'_>,
        source: &str,
        bounds: sk::IRect,
    ) -> Result<Image> {
        let mut chain = Vec::new();
        let mut current = Some(source);
        while let Some(id) = current {
            ensure!(
                chain.len() < 256 && !chain.iter().any(|l: &&Layer| l.id == id),
                "Invalid live mask graph"
            );
            let layer = index
                .layer(id)
                .ok_or_else(|| anyhow!("Missing live mask source"))?;
            chain.push(layer);
            current = layer.mask_source_id.as_deref();
        }
        let (width, height) = (bounds.width() as u32, bounds.height() as u32);
        let mut coverage = vec![255u8; width as usize * height as usize];
        for layer in chain.into_iter().rev() {
            let mut own = surface(width, height)?;
            own.canvas()
                .translate((-bounds.left as f32, -bounds.top as f32));
            self.draw_own_with_opacity(
                layer,
                own.canvas(),
                Blend::SourceOver,
                index.effective(layer).1,
            )?;
            for (alpha, pixel) in coverage
                .iter_mut()
                .zip(rgba_pixels(&own.image_snapshot())?.chunks_exact(4))
            {
                *alpha = ((*alpha as u16 * pixel[3] as u16 + 127) / 255) as u8;
            }
        }
        alpha_image(width, height, &coverage)
    }

    pub(super) fn retain_projection_sources(&mut self, index: &LayerIndex<'_>) {
        self.cache.retain(|id, entry| {
            let keep = index.layer(id).is_some();
            if !keep {
                self.cache_pixels -= entry.image.width() as u64 * entry.image.height() as u64;
            }
            keep
        });
        self.stacks.retain(|id, entry| {
            let keep = index.layer(id).is_some();
            if !keep {
                self.stack_pixels -= entry.image.width() as u64 * entry.image.height() as u64;
            }
            keep
        });
    }
    pub(super) fn draw_projection(&mut self, doc: &Document, canvas: &Canvas) -> Result<()> {
        let plan = Projection::new(doc);
        self.retain_projection_sources(&plan.index);
        if plan.has_adjustments() {
            // Adjustment layers read the composite beneath them, which a bare
            // canvas cannot provide. Render the adjusted document, then place it
            // under the caller's transform and clip.
            let image = self.composite_adjusted(&plan)?;
            canvas.draw_image(&image, (0., 0.), None);
            return Ok(());
        }
        self.draw_projection_range(&plan, 0..plan.nodes.len(), canvas)
    }
    /// One atomic compositing node: a shared-alpha clipping stack or a single
    /// layer with its live-source clip. Adjustment singletons are handled by
    /// the caller, which owns the surface beneath them.
    fn draw_node(&mut self, plan: &Projection<'_>, node: &Node, canvas: &Canvas) -> Result<()> {
        let Some(clip) = canvas.local_clip_bounds() else {
            return Ok(());
        };
        let index = &plan.index;
        let layers = &plan.layers;
        let at = node.layers.start;
        let end = node.layers.end;
        let layer = layers[at];
        if !node.bounds.intersects(clip) {
            return Ok(());
        }
        canvas.save();
        self.clip_folders(index, layer, canvas)?;
        if end > at + 1 {
            if let Some((image, bounds)) = self.stack_image(&index, &layers[at..end], clip)? {
                let mut paint = Paint::default();
                paint.set_blend_mode(blend(layer.blend));
                blends::apply(&mut paint, layer.blend)?;
                canvas.draw_image(image, (bounds.left as f32, bounds.top as f32), Some(&paint));
            }
        } else {
            if let Some(source) = &layer.mask_source_id {
                let mut region = support(layer);
                region.intersect(clip);
                let region = region.round_out();
                let coverage = self.coverage_region(index, source, region)?;
                let matrix = sk::Matrix::translate((region.left as f32, region.top as f32));
                let shader = coverage
                    .to_shader(
                        (sk::TileMode::Decal, sk::TileMode::Decal),
                        sk::SamplingOptions::default(),
                        &matrix,
                    )
                    .ok_or_else(|| anyhow!("Cannot clip live mask"))?;
                canvas.clip_shader(shader, None);
            }
            self.draw_own_with_opacity(layer, canvas, layer.blend, index.effective(layer).1)?;
        }
        canvas.restore();
        Ok(())
    }
    pub(super) fn draw_projection_range(
        &mut self,
        plan: &Projection<'_>,
        range: std::ops::Range<usize>,
        canvas: &Canvas,
    ) -> Result<()> {
        // No in-repo caller reaches this with adjustments: preview_composite
        // takes its once-per-frame bypass above and draw_projection branches
        // before ranging. The fallback keeps direct callers correct (one full
        // composite per call, never per chunk).
        if plan.has_adjustments() {
            let image = self.composite_adjusted(plan)?;
            canvas.draw_image(&image, (0., 0.), None);
            return Ok(());
        }
        for node in &plan.nodes[range] {
            self.draw_node(plan, node, canvas)?;
        }
        Ok(())
    }
    /// AdjustmentSurface-style full-document composite (609dbeae): every layer
    /// draws in stack order into one owned surface, and each adjustment layer
    /// remaps the straight-alpha pixels beneath it through its Levels/Curves
    /// tables before layers above continue. Own masks, clipping links, folder
    /// masks, opacity and blend modes follow LiveMaskRenderer.adjust.
    pub(super) fn composite_adjusted(&mut self, plan: &Projection<'_>) -> Result<Image> {
        let doc = plan.index.document;
        let mut accum = surface(doc.width, doc.height)?;
        accum.canvas().clear(Color::TRANSPARENT);
        self.retain_projection_sources(&plan.index);
        for node in &plan.nodes {
            let layer = plan.layers[node.layers.start];
            if layer.adjustment.is_some() {
                // LiveMaskRenderer.drawComposite: a clipped adjustment outside its
                // base's stack draws nothing; only sourceless layers adjust the
                // accumulated composite.
                if layer.mask_source_id.is_none() {
                    self.apply_global_adjustment(&plan.index, layer, &mut accum)?;
                }
            } else if node.layers.len() > 1
                && plan.layers[node.layers.clone()]
                    .iter()
                    .any(|l| l.adjustment.is_some())
            {
                self.render_adjusted_stack(plan, node, accum.canvas())?;
            } else {
                self.draw_node(plan, node, accum.canvas())?;
            }
        }
        Ok(accum.image_snapshot())
    }
    /// Blend two opaque premultiplied images with a layer blend mode. Colors
    /// stay opaque throughout; the caller restores alpha afterwards, exactly
    /// like `LiveMaskRenderer.adjust`'s blend path.
    fn blend_opaque(
        base: &[u8],
        top: &[u8],
        width: u32,
        height: u32,
        mode: Blend,
    ) -> Result<Vec<u8>> {
        let base_image = premul_image(width, height, base)?;
        let top_image = premul_image(width, height, top)?;
        let mut blend_surface = surface(width, height)?;
        blend_surface
            .canvas()
            .draw_image(&base_image, (0., 0.), None);
        let mut paint = Paint::default();
        paint.set_blend_mode(blend(mode));
        blends::apply(&mut paint, mode)?;
        blend_surface
            .canvas()
            .draw_image(&top_image, (0., 0.), Some(&paint));
        premul_pixels(&blend_surface.image_snapshot())
    }
    /// Grayscale coverage of an adjustment layer's own mask in document pixels.
    /// Adjustment layers always span the document, so the mask image maps 1:1.
    fn adjustment_mask_coverage(
        mask: &LayerMask,
        layer: &Layer,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        let coverage = mask_image(mask, layer)?;
        let info = sk::ImageInfo::new(
            (width as i32, height as i32),
            sk::ColorType::Alpha8,
            sk::AlphaType::Premul,
            None,
        );
        let mut bytes = vec![0u8; width as usize * height as usize];
        ensure!(
            coverage.read_pixels(
                &info,
                &mut bytes,
                width as usize,
                (0, 0),
                sk::image::CachingHint::Disallow
            ),
            "Cannot read adjustment mask"
        );
        Ok(bytes)
    }
    /// LiveMaskRenderer.adjust (609dbeae) for a sourceless adjustment layer:
    /// remap the accumulated composite through the Levels/Curves tables, then
    /// replace those pixels through the layer's own mask, folder masks,
    /// opacity and blend mode. Non-normal modes blend opaque unpremultiplied
    /// colors with the original alpha restored afterwards, so translucent
    /// coverage is never composited over itself.
    fn apply_global_adjustment(
        &mut self,
        index: &LayerIndex<'_>,
        layer: &Layer,
        surface: &mut Surface,
    ) -> Result<()> {
        let adjustment = layer
            .adjustment
            .as_ref()
            .ok_or_else(|| anyhow!("Missing adjustment"))?;
        let opacity = index.effective(layer).1;
        if opacity <= 0. {
            return Ok(());
        }
        if adjustment.is_identity() && layer.blend == Blend::SourceOver {
            return Ok(());
        }
        let (width, height) = (surface.width() as u32, surface.height() as u32);
        let info = sk::ImageInfo::new(
            (width as i32, height as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let mut below = vec![0u8; width as usize * height as usize * 4];
        ensure!(
            surface.read_pixels(&info, &mut below, width as usize * 4, (0, 0)),
            "Cannot read adjusted pixels"
        );
        let tables = adjustment.tables();
        let mut mapped = below.clone();
        crate::adjustment::apply_tables_premul(&mut mapped, &tables);
        if layer.blend != Blend::SourceOver {
            let alpha: Vec<u8> = below.chunks_exact(4).map(|p| p[3]).collect();
            let mut opaque_base = below.clone();
            crate::adjustment::unpremultiply_opaque(&mut opaque_base);
            let mut opaque_top = mapped.clone();
            crate::adjustment::unpremultiply_opaque(&mut opaque_top);
            mapped = Self::blend_opaque(&opaque_base, &opaque_top, width, height, layer.blend)?;
            crate::adjustment::restore_alpha(&mut mapped, &alpha);
        }
        // CoreImage blend-with-mask merging: mix by opacity, keeping alpha;
        // masks and folder coverage clip the replacement draw itself.
        let amount = (opacity * 255.).round().clamp(0., 255.) as u32;
        let mut output = below;
        if amount < 255 {
            for (out, src) in output.chunks_exact_mut(4).zip(mapped.chunks_exact(4)) {
                for c in 0..4 {
                    out[c] = ((src[c] as u32 * amount + out[c] as u32 * (255 - amount) + 127) / 255)
                        as u8;
                }
            }
        } else {
            output = mapped;
        }
        let image = premul_image(width, height, &output)?;
        let canvas = surface.canvas();
        canvas.save();
        self.clip_folders(index, layer, canvas)?;
        if let Some(mask) = &layer.mask
            && mask.enabled
        {
            let coverage = mask_image(mask, layer)?;
            let shader = coverage
                .to_shader(
                    (sk::TileMode::Decal, sk::TileMode::Decal),
                    sk::SamplingOptions::default(),
                    &sk::Matrix::new_identity(),
                )
                .ok_or_else(|| anyhow!("Cannot clip adjustment mask"))?;
            canvas.clip_shader(shader, None);
        }
        // The output already holds the final composite: replace, never
        // source-over, so equal alpha never thickens.
        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::Src);
        canvas.draw_image(&image, (0., 0.), Some(&paint));
        canvas.restore();
        Ok(())
    }
    /// A clipping stack containing adjustment children, following
    /// LiveMaskRenderer.drawComposite: the base renders into a group surface,
    /// alpha is set aside while children (including adjustments) recolor its
    /// opaque colors, then the original alpha is restored once before the
    /// group blends over the backdrop with the base's blend mode.
    fn render_adjusted_stack(
        &mut self,
        plan: &Projection<'_>,
        node: &Node,
        canvas: &Canvas,
    ) -> Result<()> {
        let index = &plan.index;
        let doc = index.document;
        let (width, height) = (doc.width, doc.height);
        let base = plan.layers[node.layers.start];
        let mut group = surface(width, height)?;
        group.canvas().clear(Color::TRANSPARENT);
        self.draw_own_with_opacity(
            base,
            group.canvas(),
            Blend::SourceOver,
            index.effective(base).1,
        )?;
        let info = sk::ImageInfo::new(
            (width as i32, height as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Premul,
            None,
        );
        let mut pixels = read_surface_pixels(&mut group, &info, width, height)?;
        let alpha: Vec<u8> = pixels.chunks_exact(4).map(|p| p[3]).collect();
        crate::adjustment::unpremultiply_opaque(&mut pixels);
        write_surface_pixels(&mut group, &pixels, width, height)?;
        for child in &plan.layers[node.layers.start + 1..node.layers.end] {
            if let Some(adjustment) = &child.adjustment {
                let child_opacity = index.effective(child).1;
                if child_opacity > 0.
                    && (!adjustment.is_identity() || child.blend != Blend::SourceOver)
                {
                    let mut current = read_surface_pixels(&mut group, &info, width, height)?;
                    let mut mapped = current.clone();
                    crate::adjustment::apply_tables_premul(&mut mapped, &adjustment.tables());
                    if child.blend != Blend::SourceOver {
                        mapped = Self::blend_opaque(&current, &mapped, width, height, child.blend)?;
                    }
                    let coverage = if child.mask.as_ref().is_some_and(|m| m.enabled) {
                        Self::adjustment_mask_coverage(
                            child.mask.as_ref().unwrap(),
                            child,
                            width,
                            height,
                        )?
                    } else {
                        vec![255u8; width as usize * height as usize]
                    };
                    let amount = (child_opacity * 255.).round().clamp(0., 255.) as u32;
                    for ((out, src), &cov) in current
                        .chunks_exact_mut(4)
                        .zip(mapped.chunks_exact(4))
                        .zip(coverage.iter())
                    {
                        let k = (amount * cov as u32 + 127) / 255;
                        for c in 0..3 {
                            out[c] =
                                ((src[c] as u32 * k + out[c] as u32 * (255 - k) + 127) / 255) as u8;
                        }
                    }
                    write_surface_pixels(&mut group, &current, width, height)?;
                }
            } else {
                self.draw_own_with_opacity(
                    child,
                    group.canvas(),
                    child.blend,
                    index.effective(child).1,
                )?;
            }
        }
        let mut pixels = read_surface_pixels(&mut group, &info, width, height)?;
        crate::adjustment::restore_alpha(&mut pixels, &alpha);
        let image = premul_image(width, height, &pixels)?;
        canvas.save();
        self.clip_folders(index, base, canvas)?;
        let mut paint = Paint::default();
        paint.set_blend_mode(blend(base.blend));
        blends::apply(&mut paint, base.blend)?;
        canvas.draw_image(&image, (0., 0.), Some(&paint));
        canvas.restore();
        Ok(())
    }
}

// Frozen pre-rewrite renderer is a pixel oracle, confined to tests. Its full
// document allocations are intentionally retained here to detect changed math.
#[cfg(test)]
impl Renderer {
    fn draw_baseline(&mut self, doc: &Document, c: &Canvas) -> Result<()> {
        let layers: Vec<_> = doc
            .ordered_layers()
            .into_iter()
            .filter(|l| doc.effective(l).0 && !matches!(l.content.as_ref(), Content::Group))
            .collect();
        let mut index = 0;
        while index < layers.len() {
            let layer = layers[index];
            c.save();
            self.clip_folders(&crate::layer_index::LayerIndex::new(doc), layer, c)?;
            let mut end = index + 1;
            if layer.mask_source_id.is_none() {
                while end < layers.len()
                    && layers[end].mask_source_id.as_deref() == Some(&layer.id)
                    && layers[end].parent_id == layer.parent_id
                {
                    end += 1;
                }
            }
            if end > index + 1 {
                // LiveMaskRenderer.drawComposite: blend colors on an opaque base, then
                // restore its original alpha once. Repeated source-over thickens soft edges.
                let mut base = surface(doc.width, doc.height)?;
                self.draw_own(doc, layer, base.canvas(), Blend::SourceOver)?;
                let mut pixels = rgba_pixels(&base.image_snapshot())?;
                let coverage: Vec<_> = pixels
                    .chunks_exact_mut(4)
                    .map(|p| {
                        let a = p[3];
                        p[3] = 255;
                        a
                    })
                    .collect();
                base.canvas().clear(Color::TRANSPARENT);
                base.canvas().draw_image(
                    rgba_image(doc.width, doc.height, &pixels)?,
                    (0., 0.),
                    None,
                );
                for child in &layers[index + 1..end] {
                    self.draw_own(doc, child, base.canvas(), child.blend)?;
                }
                pixels = rgba_pixels(&base.image_snapshot())?;
                for (pixel, alpha) in pixels.chunks_exact_mut(4).zip(coverage) {
                    pixel[3] = alpha;
                }
                let mut p = Paint::default();
                p.set_blend_mode(blend(layer.blend));
                blends::apply(&mut p, layer.blend)?;
                c.draw_image(
                    rgba_image(doc.width, doc.height, &pixels)?,
                    (0., 0.),
                    Some(&p),
                );
            } else {
                if let Some(source) = &layer.mask_source_id {
                    let coverage = self.live_coverage(doc, source)?;
                    let image = alpha_image(doc.width, doc.height, &coverage)?;
                    let shader = image
                        .to_shader(
                            (sk::TileMode::Decal, sk::TileMode::Decal),
                            sk::SamplingOptions::default(),
                            None,
                        )
                        .ok_or_else(|| anyhow!("Cannot clip live mask"))?;
                    c.clip_shader(shader, None);
                }
                self.draw_own(doc, layer, c, layer.blend)?;
            }
            c.restore();
            index = end;
        }
        Ok(())
    }
}
