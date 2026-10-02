//! Non-destructive Levels/Curves adjustment layers and their editing sessions.
//!
//! Translated from the pinned Compositor (609dbeae, MIT © 2026 Wonder Assembly LLC):
//! `LayerAdjustment.swift` (`addAdjustment`, `updateAdjustment`), `AdjustmentEditing.swift`
//! (`beginAdjustmentEditing`, `previewAdjustmentEditing`, `finishAdjustmentEditing`),
//! `Levels.swift` (`beginLevels`, `updateLevels`, `cancelLevels`, `commitLevels`) and
//! `LevelsAutomatic.swift` (`autoLevels`, `sampleLevels`).
//!
//! Adaptations: upstream edits destructively through `LevelsEdit`/`FilterEdit` preview
//! images and asynchronous histogram/preview tasks. This port edits the adjustment
//! layer's metadata directly inside one history transaction (preview via
//! `History::preview`, cancel via `History::cancel`, commit via `History::commit`),
//! so every preview step stays in a single undo entry. The histogram is rendered
//! synchronously from the layers beneath when editing begins; sampling re-renders
//! the live composite below at click time instead of reading a frozen source image.
use super::*;
use crate::adjustment::{
    AdjustmentKind, LayerAdjustment, LevelsAuto, LevelsSample, LevelsSettings,
};
use crate::render;

#[derive(Clone)]
pub struct AdjustmentEdit {
    pub layer_id: String,
    pub original: LayerAdjustment,
    pub working: LayerAdjustment,
    pub preview: bool,
    pub histogram: [[f64; 256]; 4],
    pub sample_mode: Option<LevelsSample>,
}

impl Editor {
    fn adjustment_target(&self, id: &str) -> Result<Layer> {
        self.history
            .document
            .layers
            .iter()
            .find(|l| l.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Adjustment layer is gone"))
    }
    /// Straight-alpha RGBA of everything drawn beneath `id` in stack order,
    /// mirroring `beginAdjustmentEditing`'s hidden-above sampling image.
    fn below_composite(&self, id: &str) -> Result<(Vec<u8>, u32, u32)> {
        let doc = &self.history.document;
        let ordered = doc.ordered_layers();
        let pos = ordered
            .iter()
            .position(|l| l.id == id)
            .ok_or_else(|| anyhow::anyhow!("Adjustment layer is gone"))?;
        let below: HashSet<&str> = ordered[..pos].iter().map(|l| l.id.as_str()).collect();
        let mut temp = doc.clone();
        for layer in &mut temp.layers {
            // AdjustmentEditing hides the adjustment and everything above it
            // from the sampling image, but keeps group records visible for
            // live-mask references and group ancestry.
            if !below.contains(layer.id.as_str())
                && !matches!(layer.content.as_ref(), Content::Group)
            {
                layer.visible = false;
            }
        }
        let image = render::Renderer::default().render(&temp)?.image_snapshot();
        // Premultiplied bytes for the exact `LevelsPixels.c` histogram port.
        let pixels = render::premul_pixels(&image)?;
        Ok((pixels, doc.width, doc.height))
    }
    /// AdjustmentEditing builds its LevelsEdit with a nil selection: the
    /// histogram is the alpha-weighted underlying composite, never weighted
    /// by the active pixel selection (the adjustment itself applies globally).
    /// The `histogram_premul` kernel stays selection-capable for destructive
    /// and general consumers; only this editing context passes nil coverage.
    fn adjustment_histogram(&self, id: &str) -> Result<[[f64; 256]; 4]> {
        let (pixels, _, _) = self.below_composite(id)?;
        Ok(crate::adjustment::histogram_premul(&pixels, None))
    }
    /// `addAdjustment`: a document-sized adjustment layer above the selection,
    /// filed inside the active folder like any inserted layer.
    pub fn add_adjustment(&mut self, kind: AdjustmentKind) -> Result<()> {
        self.finish_gesture()?;
        self.commit_transform()?;
        self.finish_text()?;
        ensure!(
            self.history.document.layers.len() < crate::model::MAX_LAYERS,
            "The editor supports up to 10,000 layers"
        );
        if self.adjustment_edit.is_some() {
            self.cancel_adjustment_edit();
        }
        let doc = &self.history.document;
        let mut layer = Layer::new(kind.display(), doc.width, doc.height, Content::Paint);
        layer.adjustment = Some(LayerAdjustment::new(kind));
        let id = layer.id.clone();
        let inserted = self.inserted(vec![layer]);
        inserted.validate()?;
        self.begin_edit(&format!("New {} Adjustment", kind.display()));
        self.history.preview(inserted);
        self.single_selection(Some(id.clone()));
        self.paint_target = PaintTarget::Content;
        if let Some(parent) = self.selected().and_then(|l| l.parent_id.clone()) {
            self.collapsed_groups.remove(&parent);
        }
        self.end_edit();
        self.begin_adjustment_edit(id)
    }
    /// `beginAdjustmentEditing`: snapshot the working values and histogram, then
    /// open one undo transaction that previews every change until commit/cancel.
    pub fn begin_adjustment_edit(&mut self, id: String) -> Result<()> {
        self.finish_gesture()?;
        self.finish_text()?;
        if self
            .adjustment_edit
            .as_ref()
            .is_some_and(|edit| edit.layer_id == id)
        {
            return Ok(());
        }
        if self.adjustment_edit.is_some() {
            self.cancel_adjustment_edit();
        }
        let layer = self.adjustment_target(&id)?;
        let Some(adjustment) = layer.adjustment.clone() else {
            return Ok(());
        };
        if layer.locked {
            return Ok(());
        }
        let histogram = self.adjustment_histogram(&id)?;
        self.begin_edit(&format!("Edit {} Adjustment", adjustment.kind.display()));
        self.single_selection(Some(id.clone()));
        self.paint_target = PaintTarget::Content;
        self.adjustment_edit = Some(AdjustmentEdit {
            layer_id: id,
            original: adjustment.clone(),
            working: adjustment,
            preview: true,
            histogram,
            sample_mode: None,
        });
        Ok(())
    }
    fn write_adjustment(&mut self) -> Result<()> {
        let Some(edit) = self.adjustment_edit.clone() else {
            return Ok(());
        };
        let mut doc = self.history.document.clone();
        let Some(layer) = doc.layers.iter_mut().find(|l| l.id == edit.layer_id) else {
            return Ok(());
        };
        // Locking mid-edit freezes the preview; the working values are kept
        // for Apply (which equally no-ops) or the next unlock.
        if layer.locked {
            return Ok(());
        }
        layer.adjustment = Some(if edit.preview {
            edit.working.clone()
        } else {
            edit.original.clone()
        });
        doc.validate()?;
        self.history.preview(doc);
        Ok(())
    }
    /// `updateLevels`: live levels preview inside the open edit transaction.
    pub fn update_adjustment_levels(
        &mut self,
        settings: LevelsSettings,
        preview: bool,
    ) -> Result<()> {
        let Some(edit) = self.adjustment_edit.clone() else {
            return Ok(());
        };
        if edit.working.kind != AdjustmentKind::Levels {
            return Ok(());
        }
        let mut settings = settings;
        for range in &mut settings.ranges {
            *range = range.normalized();
        }
        let mut edit = edit;
        edit.working.levels = settings;
        edit.preview = preview;
        self.adjustment_edit = Some(edit);
        self.write_adjustment()
    }
    /// Curves twin of `updateLevels`, routed through the shared filter panel upstream.
    pub fn update_adjustment_curves(
        &mut self,
        settings: crate::adjustment::CurvesSettings,
        preview: bool,
    ) -> Result<()> {
        let Some(edit) = self.adjustment_edit.clone() else {
            return Ok(());
        };
        if edit.working.kind != AdjustmentKind::Curves {
            return Ok(());
        }
        ensure!(settings.is_valid(), "Invalid curve points");
        let mut edit = edit;
        edit.working.curves = settings;
        edit.preview = preview;
        self.adjustment_edit = Some(edit);
        self.write_adjustment()
    }
    /// `previewAdjustmentEditing`: show the working values or the original.
    pub fn set_adjustment_preview(&mut self, preview: bool) -> Result<()> {
        if let Some(edit) = self.adjustment_edit.clone() {
            let mut edit = edit;
            edit.preview = preview;
            self.adjustment_edit = Some(edit);
            self.write_adjustment()?;
        }
        Ok(())
    }
    /// `autoLevels`: derive settings from the edit histogram.
    pub fn auto_levels(&mut self, mode: LevelsAuto) -> Result<()> {
        let Some(edit) = self.adjustment_edit.clone() else {
            return Ok(());
        };
        if edit.working.kind != AdjustmentKind::Levels {
            return Ok(());
        }
        let mut edit = edit;
        edit.working.levels = mode.settings(&edit.histogram);
        edit.sample_mode = None;
        self.adjustment_edit = Some(edit);
        self.write_adjustment()
    }
    /// `sampleLevels`: calibrate from the unpremultiplied pixel under the point.
    /// Transparent pixels are rejected, exactly like upstream.
    pub fn sample_levels(&mut self, point: Point, mode: LevelsSample) -> Result<()> {
        let Some(edit) = self.adjustment_edit.clone() else {
            return Ok(());
        };
        if edit.working.kind != AdjustmentKind::Levels {
            return Ok(());
        }
        let doc = self.history.document.clone();
        let at = geometry::to_document(&doc, &self.viewport, point);
        if !(at.x >= 0. && at.y >= 0. && at.x < doc.width as f64 && at.y < doc.height as f64) {
            return Ok(());
        }
        let (pixels, width, _) = self.below_composite(&edit.layer_id)?;
        let (x, y) = (at.x.floor() as usize, at.y.floor() as usize);
        let pixel = &pixels[(y * width as usize + x) * 4..][..4];
        // Upstream rejects transparent samples; straight RGB comes from the
        // premultiplied source exactly like `sampleLevels`.
        let Some(rgb) = crate::adjustment::straight_rgb(pixel) else {
            return Ok(());
        };
        let mut edit = edit;
        edit.working.levels = edit.working.levels.sampling(rgb, mode);
        self.adjustment_edit = Some(edit);
        self.write_adjustment()
    }
    pub fn set_levels_sample_mode(&mut self, mode: Option<LevelsSample>) {
        if let Some(edit) = self.adjustment_edit.clone() {
            let mut edit = edit;
            edit.sample_mode = mode;
            self.adjustment_edit = Some(edit);
        }
    }
    /// `finishAdjustmentEditing(commit: false)`: restore the original settings.
    pub fn cancel_adjustment_edit(&mut self) {
        if self.adjustment_edit.is_none() {
            return;
        }
        self.adjustment_edit = None;
        let selection = self.history.cancel();
        self.restore_selection(selection);
    }
    /// `finishAdjustmentEditing(commit: true)`: keep the working settings as one undo entry.
    pub fn commit_adjustment_edit(&mut self) -> Result<()> {
        let Some(edit) = self.adjustment_edit.clone() else {
            return Ok(());
        };
        if self
            .history
            .document
            .layers
            .iter()
            .all(|l| l.id != edit.layer_id)
        {
            self.cancel_adjustment_edit();
            return Ok(());
        }
        let mut edit = edit;
        edit.preview = true;
        self.adjustment_edit = Some(edit);
        self.write_adjustment()?;
        self.adjustment_edit = None;
        self.end_edit();
        Ok(())
    }
}
