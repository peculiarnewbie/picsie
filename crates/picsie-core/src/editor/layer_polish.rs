//! LayerAppearance, ColorPalette, LayerMask and MaskTracing, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Skia paths and legacy placement grids are adaptations.
use super::*;
pub(crate) struct MaskDistortion {
    pub original: Layer,
    pub placement: MaskPlacement,
    pub corners: [Point; 4],
}
impl Editor {
    pub(super) fn begin_mask_distortion(&mut self) -> Result<()> {
        if self.mask_distortion.is_some() {
            return Ok(());
        }
        // Group distortion is not ported; see begin_image_distortion.
        if self.group_transform.is_some() {
            return Ok(());
        }
        let Some(target) = self.independent_mask_layer() else {
            return Ok(());
        };
        if !self.layer_transform {
            self.finish_gesture()?;
            self.begin_edit("Distort Layer Mask");
            self.layer_transform = true;
        }
        self.mask_distortion = Some(MaskDistortion {
            original: self.selected().unwrap().clone(),
            placement: MaskPlacement::of(&target),
            corners: geometry::corners(&target).try_into().unwrap(),
        });
        Ok(())
    }
    pub(super) fn preview_mask_distortion(
        &mut self,
        corners: [Point; 4],
        limit: Option<f64>,
    ) -> Result<()> {
        let Some(draft) = &self.mask_distortion else {
            return Ok(());
        };
        let (raster, placement) =
            crate::distort::mask(&draft.original, draft.placement, &corners, limit)?;
        let mut layer = draft.original.clone();
        let mask = Arc::make_mut(layer.mask.as_mut().unwrap());
        mask.raster = Some(Arc::new(raster));
        mask.strokes.clear();
        mask.placement = Some(placement);
        let mut doc = self.history.document.clone();
        doc.replace(layer);
        self.history.preview(doc);
        self.mask_distortion.as_mut().unwrap().corners = corners;
        Ok(())
    }
    pub fn mask_distortion_corners(&self) -> Option<[Point; 4]> {
        self.mask_distortion.as_ref().map(|d| d.corners)
    }

    pub(super) fn set_palette_color(&mut self, color: String, background: bool) -> Result<()> {
        ensure!(
            color.len() == 7 && color_valid(&color),
            "Enter a six-digit hex color"
        );
        if matches!(self.gesture, Some(Gesture::Brush { .. })) {
            return Ok(());
        }
        if self.paint_target == PaintTarget::Mask {
            let white = color.eq_ignore_ascii_case("#ffffff");
            self.mask_mode = if white != background {
                MaskMode::Reveal
            } else {
                MaskMode::Hide
            };
        } else if background {
            self.background_color = color;
        } else {
            self.color = color.clone();
            if self.tool == Tool::Text && self.text_editing() {
                self.update_text(crate::text::TextPatch {
                    color: Some(color),
                    ..Default::default()
                })?;
            }
        }
        Ok(())
    }
    pub(super) fn end_visibility_swipe(&mut self) {
        if self.visibility_swipe.take().is_some() {
            self.end_edit();
        }
    }
    pub fn independent_mask_layer(&self) -> Option<Layer> {
        let layer = self.selected()?;
        let mask = layer.mask.as_ref()?;
        (self.paint_target == PaintTarget::Mask
            && self.selection.ids.len() == 1
            && !mask.linked
            && !layer.locked)
            .then(|| {
                mask.placement
                    .unwrap_or_else(|| MaskPlacement::of(layer))
                    .as_layer(layer)
            })
    }
    fn load_thumbnail_selection(
        &mut self,
        id: &str,
        mask: bool,
        mode: pixel_selection::PixelSelectionMode,
    ) -> Result<()> {
        self.finish_text()?;
        self.finish_gesture()?;
        self.commit_transform()?;
        let Some(layer) = self
            .history
            .document
            .layers
            .iter()
            .find(|l| l.id == id)
            .cloned()
        else {
            return Ok(());
        };
        let (coverage, width, height, placement) = if mask {
            let Some(mask) = &layer.mask else {
                return Ok(());
            };
            let raster = if mask.strokes.is_empty()
                && let Some(raster) = &mask.raster
            {
                raster.as_ref().clone()
            } else {
                render::rasterize_mask(mask, &layer)?
            };
            // rasterize_mask returns the mask's own grid, independent of placement.
            (
                raster
                    .pixels
                    .iter()
                    .map(|v| u8::from(*v < 128))
                    .collect::<Vec<_>>(),
                raster.width,
                raster.height,
                mask.placement
                    .unwrap_or_else(|| MaskPlacement::of(&layer))
                    .as_layer(&layer),
            )
        } else {
            if matches!(layer.content.as_ref(), Content::Group) {
                return Ok(());
            }
            let mut bare = layer.clone();
            bare.mask = None;
            let image = render::Renderer::default().layer_surface(&bare)?;
            let pixels = render::rgba_pixels(&image)?;
            (
                pixels
                    .chunks_exact(4)
                    .map(|p| u8::from(p[3] >= 128))
                    .collect(),
                image.width() as u32,
                image.height() as u32,
                layer.clone(),
            )
        };
        let Some(outline) = crate::wand::outline(&coverage, width, height)? else {
            return Ok(());
        };
        let a = geometry::to_world(&placement, Point::new(0., 0.));
        let b = geometry::to_world(&placement, Point::new(placement.width as f64, 0.));
        let c = geometry::to_world(&placement, Point::new(0., placement.height as f64));
        let matrix = skia_safe::Matrix::new_all(
            ((b.x - a.x) / width as f64) as f32,
            ((c.x - a.x) / height as f64) as f32,
            a.x as f32,
            ((b.y - a.y) / width as f64) as f32,
            ((c.y - a.y) / height as f64) as f32,
            a.y as f32,
            0.,
            0.,
            1.,
        );
        let outline = outline.with_transform(&matrix);
        let doc = &self.history.document;
        let next = if let Some(old) = &self.history.pixel_selection {
            Some(old.combined(&outline, mode)?)
        } else if mode == pixel_selection::PixelSelectionMode::Subtract {
            None
        } else {
            Some(
                PixelSelection::all(doc.width, doc.height)?
                    .combined(&outline, pixel_selection::PixelSelectionMode::Replace)?,
            )
        };
        let next = next
            .map(|s| s.with_antialiasing(self.selection_antialiased))
            .transpose()?;
        self.set_pixel_selection(
            next,
            if mask {
                "Load Mask Selection"
            } else {
                "Load Layer Selection"
            },
        )
    }
    pub(super) fn layer_polish_command(&mut self, command: &Command) -> Result<bool> {
        match command {
            Command::SetColor { color } => {
                self.set_palette_color(color.clone(), false)?;
                self.refresh_gradient()?;
            }
            Command::SetPaletteColor { color, background } => {
                self.set_palette_color(color.clone(), *background)?;
                self.refresh_gradient()?;
            }
            // EditorCanvas.sampleColor assigns the image foreground even with a mask targeted;
            // the black/white mask palette remains independent.
            Command::SetSampledForeground { color } => {
                ensure!(
                    color.len() == 7 && color_valid(color),
                    "Enter a six-digit hex color"
                );
                if !matches!(self.gesture, Some(Gesture::Brush { .. })) {
                    self.color = color.clone();
                }
                self.refresh_gradient()?;
            }
            Command::SetTextColor { color, draft_id } => {
                ensure!(
                    color.len() == 7 && color_valid(color),
                    "Enter a six-digit hex color"
                );
                if self.tool == Tool::Text
                    && self.text_session.as_ref().map(|s| &s.id) == draft_id.as_ref()
                {
                    if draft_id.is_some() {
                        self.update_text(crate::text::TextPatch {
                            color: Some(color.clone()),
                            ..Default::default()
                        })?;
                    } else if let Content::Text { color: value, .. } =
                        Arc::make_mut(&mut self.text_defaults.content)
                    {
                        *value = color.clone();
                    }
                    if self.paint_target != PaintTarget::Mask {
                        self.color = color.clone();
                    }
                }
            }
            Command::SwapPaletteColors => {
                if matches!(self.gesture, Some(Gesture::Brush { .. })) {
                    return Ok(true);
                }
                if self.paint_target == PaintTarget::Mask {
                    self.mask_mode = if self.mask_mode == MaskMode::Hide {
                        MaskMode::Reveal
                    } else {
                        MaskMode::Hide
                    };
                } else {
                    let old = self.color.clone();
                    self.set_palette_color(self.background_color.clone(), false)?;
                    self.background_color = old;
                }
                self.refresh_gradient()?;
            }
            Command::ResetPaletteColors => {
                if matches!(self.gesture, Some(Gesture::Brush { .. })) {
                    return Ok(true);
                }
                if self.paint_target == PaintTarget::Mask {
                    self.mask_mode = MaskMode::Hide;
                } else {
                    self.set_palette_color("#000000".into(), false)?;
                    self.background_color = "#ffffff".into();
                }
                self.refresh_gradient()?;
            }
            Command::PreviewBlendMode { id, mode } => {
                self.blend_preview = id
                    .as_ref()
                    .zip(mode.as_ref())
                    .filter(|(id, _)| {
                        self.selected_id() == Some(id.as_str())
                            && self.selection.ids.len() == 1
                            && self.selected().is_some_and(|l| {
                                !l.locked && !matches!(l.content.as_ref(), Content::Group)
                            })
                    })
                    .map(|(id, mode)| (id.clone(), *mode));
            }
            Command::CycleBlendMode { forward } => {
                self.blend_preview = None;
                self.finish_text()?;
                self.finish_gesture()?;
                if self.selection.ids.len() == 1
                    && let Some(layer) = self.selected()
                    && !layer.locked
                    && !matches!(layer.content.as_ref(), Content::Group)
                {
                    let i = Blend::ALL
                        .iter()
                        .position(|m| *m == layer.blend)
                        .unwrap_or(0);
                    let mode = Blend::ALL[(i + if *forward { 1 } else { 23 }) % 24];
                    let mut next = layer.clone();
                    next.blend = mode;
                    self.edit("Layer Blend Mode", self.with_layers(vec![next]), None)?;
                }
            }
            Command::BeginVisibilitySwipe { id } => {
                self.end_visibility_swipe();
                self.finish_text()?;
                self.finish_gesture()?;
                self.commit_transform()?;
                if let Some(layer) = self.history.document.layers.iter().find(|l| l.id == *id) {
                    let visible = !layer.visible;
                    self.begin_edit("Layer Visibility");
                    self.visibility_swipe = Some(visible);
                    self.layer_polish_command(&Command::SwipeVisibility { id: id.clone() })?;
                }
            }
            Command::SwipeVisibility { id } => {
                if let Some(visible) = self.visibility_swipe {
                    let mut doc = self.history.document.clone();
                    if let Some(layer) = doc.layers.iter_mut().find(|l| l.id == *id) {
                        layer.visible = visible;
                        self.history.preview(doc);
                    }
                }
            }
            Command::EndVisibilitySwipe => self.end_visibility_swipe(),
            Command::ToggleMaskLinkFor { id } => {
                self.finish_gesture()?;
                self.commit_transform()?;
                if let Some(layer) = self.history.document.layers.iter().find(|l| l.id == *id)
                    && !layer.locked
                    && layer.mask.is_some()
                {
                    let mut next = layer.clone();
                    let mask = Arc::make_mut(next.mask.as_mut().unwrap());
                    mask.linked = !mask.linked;
                    self.edit("Link layer mask", self.with_layers(vec![next]), None)?;
                }
            }
            Command::ToggleClippingFor { id } => {
                self.finish_text()?;
                self.finish_gesture()?;
                self.commit_transform()?;
                if let Some(layer) = self.history.document.layers.iter().find(|l| l.id == *id)
                    && !layer.locked
                    && !matches!(layer.content.as_ref(), Content::Group)
                {
                    let mut doc = self.history.document.clone();
                    let label = if layer.mask_source_id.is_some() {
                        crate::live_mask::release(&mut doc, id);
                        "Release Clipping Mask"
                    } else if let Some(source) = crate::live_mask::clipping_source(&doc, id) {
                        doc.layers
                            .iter_mut()
                            .find(|l| l.id == *id)
                            .unwrap()
                            .mask_source_id = Some(source);
                        "Create Clipping Mask"
                    } else {
                        return Ok(true);
                    };
                    doc.version = 2;
                    self.edit(label, doc, None)?;
                }
            }
            Command::CopyMask {
                source_id,
                target_id,
            } => {
                self.finish_text()?;
                self.finish_gesture()?;
                self.commit_transform()?;
                let doc = &self.history.document;
                if source_id == target_id {
                    return Ok(true);
                }
                let Some(from) = doc.layers.iter().find(|l| l.id == *source_id) else {
                    return Ok(true);
                };
                let Some(mask) = &from.mask else {
                    return Ok(true);
                };
                let mut copied = mask.as_ref().clone();
                let placement = copied.placement.unwrap_or_else(|| MaskPlacement::of(from));
                let Some(target) = doc.layers.iter().find(|l| {
                    l.id == *target_id && !l.locked && !matches!(l.content.as_ref(), Content::Group)
                }) else {
                    return Ok(true);
                };
                // Normalize differing legacy width/height grids into the destination's scale units.
                let mut target = target.clone();
                let mut placement = placement;
                placement.scale_x *= from.width as f64 / target.width as f64;
                placement.scale_y *= from.height as f64 / target.height as f64;
                copied.placement = Some(placement);
                let label = if target.mask.is_some() {
                    "Replace Layer Mask"
                } else {
                    "Copy Layer Mask"
                };
                target.mask = Some(Arc::new(copied));
                let mut next = doc.clone();
                next.replace(target);
                next.version = 2;
                self.edit(label, next, Some(vec![target_id.clone()]))?;
                self.paint_target = PaintTarget::Mask;
            }
            Command::LoadThumbnailSelection { id, mask, mode } => {
                self.load_thumbnail_selection(id, *mask, *mode)?
            }
            Command::DuplicateTo {
                target_id,
                side,
                into,
            } => {
                self.finish_text()?;
                self.finish_gesture()?;
                self.commit_transform()?;
                self.begin_edit("Duplicate Layers");
                let result = (|| {
                    self.command(Command::Duplicate)?;
                    self.command(if *into {
                        Command::MoveToGroup {
                            parent_id: Some(target_id.clone()),
                        }
                    } else {
                        Command::ReorderTo {
                            target_id: target_id.clone(),
                            side: *side,
                        }
                    })
                })();
                self.end_edit();
                result?;
            }
            Command::Nudge { delta } if self.independent_mask_layer().is_some() => {
                ensure!(
                    delta.x.is_finite() && delta.y.is_finite(),
                    "Invalid movement"
                );
                let mut placement = MaskPlacement::of(&self.independent_mask_layer().unwrap());
                placement.x += delta.x;
                placement.y += delta.y;
                self.layer_polish_command(&Command::SetMaskPlacement { placement })?;
            }
            Command::DistortMask { corners } => {
                if crate::distort::usable(corners) && self.independent_mask_layer().is_some() {
                    self.begin_mask_distortion()?;
                    self.preview_mask_distortion(*corners, Some(2048.))?;
                }
            }
            Command::SetMaskPlacement { placement } => {
                ensure!(placement.valid(), "Invalid mask placement");
                if let Some(target) = self.independent_mask_layer() {
                    if let Some(draft) = &self.mask_distortion {
                        let next = placement.as_layer(&target);
                        let corners = draft
                            .corners
                            .map(|p| geometry::to_world(&next, geometry::to_local(&target, p)));
                        self.preview_mask_distortion(corners, Some(2048.))?;
                        return Ok(true);
                    }
                    let mut layer = self.selected().unwrap().clone();
                    let mask = Arc::make_mut(layer.mask.as_mut().unwrap());
                    mask.placement = if *placement == MaskPlacement::of(self.selected().unwrap()) {
                        None
                    } else {
                        Some(*placement)
                    };
                    if matches!(self.gesture, Some(Gesture::Property)) {
                        self.history.preview(self.with_layers(vec![layer]));
                    } else {
                        self.edit("Transform Layer Mask", self.with_layers(vec![layer]), None)?;
                    }
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
impl Editor {
    pub fn draw_distortion(&self, canvas: &skia_safe::Canvas) {
        let Some(corners) = self.distortion_corners() else {
            return;
        };
        let origin = geometry::canvas_origin(&self.history.document, &self.viewport);
        let map = |p: Point| {
            skia_safe::Point::new(
                (origin.x + p.x * self.viewport.zoom) as f32,
                (origin.y + p.y * self.viewport.zoom) as f32,
            )
        };
        let mut paint = skia_safe::Paint::default();
        paint
            .set_color(if self.mask_distortion.is_some() {
                skia_safe::Color::from_rgb(98, 222, 202)
            } else {
                skia_safe::Color::from_rgb(155, 171, 255)
            })
            .set_style(skia_safe::paint::Style::Stroke)
            .set_stroke_width(1.);
        let mut path = skia_safe::PathBuilder::new();
        path.move_to(map(corners[0]));
        for p in &corners[1..] {
            path.line_to(map(*p));
        }
        path.close();
        canvas.draw_path(&path.detach(), &paint);
        let mut fill = skia_safe::Paint::default();
        fill.set_color(skia_safe::Color::WHITE);
        if let Some(handles) = self.cursor_map().handles {
            // TransformOverlayGeometry(corners:) has eight handles and no rotation stalk.
            for point in &handles.points {
                let p = map(*point);
                let rect = skia_safe::Rect::from_xywh(p.x - 3.5, p.y - 3.5, 7., 7.);
                canvas.draw_rect(rect, &fill);
                canvas.draw_rect(rect, &paint);
            }
        }
    }
}

impl Editor {
    /// A canvas-only menu preview, never a history entry or an export mutation.
    pub fn preview_document(&self) -> Document {
        let mut doc = self.history.document.clone();
        if let Some((id, mode)) = &self.blend_preview
            && self.selected_id() == Some(id.as_str())
        {
            if let Some(layer) = doc.layers.iter_mut().find(|l| l.id == *id) {
                layer.blend = *mode;
            }
        }
        doc
    }
}
