//! EditorSession / SelectionClipboard / TransformDrag behavior translated from the pinned Compositor.
//! MIT © 2026 Wonder Assembly LLC. Existing multi-selection and v1 brush storage are adaptations.
use crate::{
    canvas_size::{CanvasSizeOptions, crop_canvas, resize_canvas},
    crop::{self, CropDrag, CropRatio, CropRect},
    geometry::{self, Viewport},
    history::{History, Selection},
    model::*,
    pixel_selection::{self, MarqueeKind, PixelSelection, PixelSelectionMode, SelectionDraft},
    render,
};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use ts_rs::TS;
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Move,
    Brush,
    Eraser,
    Rectangle,
    Ellipse,
    Text,
    Hand,
    Eyedropper,
    Crop,
    Marquee,
    Lasso,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PaintTarget {
    Content,
    Mask,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum SelectionMode {
    Replace,
    Toggle,
    Range,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, TS)]
pub struct Modifiers {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub control: bool,
    #[serde(default)]
    pub meta: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct PointerSample {
    pub phase: Phase,
    pub point: Point,
    #[serde(default)]
    pub modifiers: Modifiers,
}
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InitialDocument {
    Demo,
    New {
        name: String,
        width: u32,
        height: u32,
    },
}
#[derive(Clone, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Command {
    Select {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        mode: SelectionMode,
    },
    SelectAll,
    SetTool {
        tool: Tool,
    },
    Fit,
    Zoom {
        zoom: f64,
        #[serde(default)]
        #[ts(optional)]
        point: Option<Point>,
    },
    ResizeViewport {
        width: f64,
        height: f64,
    },
    Pan {
        delta: Point,
    },
    SetViewport {
        viewport: Viewport,
    },
    SetColor {
        color: String,
    },
    SetBrush {
        size: f64,
        opacity: f64,
    },
    UpdateLayer {
        #[ts(type = "Partial<Layer>")]
        patch: serde_json::Value,
    },
    AddPaintLayer,
    AddGradient,
    AddGroup,
    GroupSelected,
    ToggleGroupExpansion {
        id: String,
    },
    MoveToGroup {
        #[serde(rename = "parentId", default)]
        #[ts(optional)]
        parent_id: Option<String>,
    },
    Duplicate,
    Remove,
    Reorder {
        direction: i8,
    },
    ReorderTo {
        #[serde(rename = "targetId")]
        target_id: String,
        side: Side,
    },
    Nudge {
        delta: Point,
    },
    ResizeCanvas {
        options: CanvasSizeOptions,
    },
    SetCropRatio {
        ratio: CropRatio,
    },
    CommitCrop,
    CancelCrop,
    SetMarqueeKind {
        kind: MarqueeKind,
    },
    SetSelectionMode {
        mode: PixelSelectionMode,
    },
    DeselectPixels,
    SelectAllPixels,
    ClearSelectedPixels,
    FeatherSelection {
        amount: u32,
    },
    EditText {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        point: Option<Point>,
    },
    BeginPropertyEdit {
        label: String,
    },
    PickUnder {
        point: Point,
    },
    AddMask {
        base: MaskMode,
    },
    SetPaintTarget {
        target: PaintTarget,
    },
    SetMaskMode {
        mode: MaskMode,
    },
    ResetMask {
        base: MaskMode,
    },
    RemoveMask,
    ToggleMaskLink,
    Undo,
    Redo,
    FinishGesture,
    CancelGesture,
    Pointer {
        samples: Vec<PointerSample>,
    },
}
#[derive(Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Above,
    Below,
}
#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LayerRow {
    pub id: String,
    pub depth: u32,
    pub visible: bool,
    pub collapsed: bool,
}
#[derive(Clone)]
enum Gesture {
    Pan {
        origin: Point,
        pan: Point,
    },
    Move {
        origin: Point,
        layers: Vec<Layer>,
    },
    /// A slider drag in flight: property updates preview into one undo step.
    Property,
    Transform {
        origin: Point,
        handle: &'static str,
        handle_point: Point,
        layer: Layer,
    },
    Shape {
        origin: Point,
        layer: Layer,
    },
    Stroke {
        layer: Layer,
        stroke: Stroke,
    },
    Mask {
        layer: Layer,
        stroke: MaskStroke,
    },
    Crop {
        drag: CropDrag,
        before: Option<CropRect>,
    },
    MaskMove {
        origin: Point,
        layer: Layer,
        placement: MaskPlacement,
    },
    PixelSelection {
        draft: SelectionDraft,
        before: Option<PixelSelection>,
    },
}
pub struct Editor {
    pub history: History,
    pub selection: Selection,
    pub paint_target: PaintTarget,
    pub mask_mode: MaskMode,
    pub tool: Tool,
    pub color: String,
    pub brush_size: f64,
    pub brush_opacity: f64,
    pub viewport: Viewport,
    pub crop_rect: Option<CropRect>,
    pub crop_ratio: CropRatio,
    pub collapsed_groups: HashSet<String>,
    pub pixel_selection: Option<PixelSelection>,
    pub marquee_kind: MarqueeKind,
    pub selection_mode: PixelSelectionMode,
    /// Bumped whenever an edit-text request selects a layer, so the UI can focus its editor.
    pub text_edit_requests: u64,
    gesture: Option<Gesture>,
}
impl Editor {
    fn next_folder_name(doc: &Document) -> String {
        let mut number = 1;
        loop {
            let candidate = format!("Folder {number}");
            if !doc.layers.iter().any(|l| l.name == candidate) {
                return candidate;
            }
            number += 1;
        }
    }
    pub fn selection_draft(&self) -> Option<SelectionDraft> {
        if let Some(Gesture::PixelSelection { draft, .. }) = &self.gesture {
            Some(draft.clone())
        } else {
            None
        }
    }
    pub fn new(document: Document) -> Result<Self> {
        document.validate()?;
        let last = document.layers.last().map(|l| l.id.clone());
        let mut e = Self {
            history: History::new(document),
            selection: Selection {
                ids: last.clone().into_iter().collect(),
                anchor: last,
            },
            paint_target: PaintTarget::Content,
            mask_mode: MaskMode::Hide,
            tool: Tool::Move,
            color: "#a5b4fc".into(),
            brush_size: 24.,
            brush_opacity: 1.,
            viewport: Viewport::default(),
            crop_rect: None,
            crop_ratio: CropRatio::Free,
            collapsed_groups: HashSet::new(),
            pixel_selection: None,
            marquee_kind: MarqueeKind::Rectangle,
            selection_mode: PixelSelectionMode::Replace,
            text_edit_requests: 0,
            gesture: None,
        };
        e.fit();
        Ok(e)
    }
    pub fn selected_id(&self) -> Option<&str> {
        self.selection.ids.last().map(String::as_str)
    }
    pub fn selected(&self) -> Option<&Layer> {
        self.history
            .document
            .layers
            .iter()
            .find(|l| Some(l.id.as_str()) == self.selected_id())
    }
    pub fn selected_layers(&self) -> Vec<Layer> {
        self.history
            .document
            .layers
            .iter()
            .filter(|l| self.selection.ids.contains(&l.id))
            .cloned()
            .collect()
    }
    fn selected_roots(&self) -> Vec<Layer> {
        let doc = &self.history.document;
        let selected: HashSet<_> = self.selection.ids.iter().map(String::as_str).collect();
        doc.layers
            .iter()
            .filter(|layer| {
                if !selected.contains(layer.id.as_str()) {
                    return false;
                }
                let mut parent = layer.parent_id.as_deref();
                while let Some(id) = parent {
                    if selected.contains(id) {
                        return false;
                    }
                    parent = doc
                        .layers
                        .iter()
                        .find(|l| l.id == id)
                        .and_then(|l| l.parent_id.as_deref());
                }
                true
            })
            .cloned()
            .collect()
    }
    fn single_selection(&mut self, id: Option<String>) {
        self.selection = Selection {
            ids: id.clone().into_iter().collect(),
            anchor: id,
        };
        self.paint_target = PaintTarget::Content;
    }
    /// Cmd/Ctrl-click walks the layers whose bounds contain the point, top to bottom and back
    /// around. Upstream's Cmd-click re-picks the topmost layer; cycling is a local extension
    /// so a full-canvas layer cannot bury the stack.
    fn pick_under(&mut self, p: Point) {
        let stack: Vec<String> = geometry::hit_layers(&self.history.document, p)
            .into_iter()
            .map(|l| l.id.clone())
            .collect();
        if stack.is_empty() {
            return;
        }
        let next = match self
            .selection
            .ids
            .last()
            .and_then(|active| stack.iter().position(|id| id == active))
        {
            Some(at) => stack[(at + 1) % stack.len()].clone(),
            None => stack[0].clone(),
        };
        self.single_selection(Some(next));
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"document":self.history.document.metadata(),"history":self.history.info(),"selection":self.selection,"paintTarget":self.paint_target,"maskMode":self.mask_mode,"tool":self.tool,"color":self.color,"brushSize":self.brush_size,"brushOpacity":self.brush_opacity,"viewport":self.viewport,"cropRect":self.crop_rect,"cropRatio":self.crop_ratio,"layerRows":self.layer_rows(),"pixelSelectionBounds":self.pixel_selection.as_ref().and_then(|v|v.bounds.clone()),"pixelSelectionFeather":self.pixel_selection.as_ref().map(|v|v.feather),"marqueeKind":self.marquee_kind,"selectionMode":self.selection_mode,"textEditRequests":self.text_edit_requests})
    }
    fn layer_rows(&self) -> Vec<LayerRow> {
        fn visit(
            doc: &Document,
            parent: Option<&str>,
            depth: u32,
            visible: bool,
            collapsed: &HashSet<String>,
            out: &mut Vec<LayerRow>,
        ) {
            for layer in doc
                .layers
                .iter()
                .rev()
                .filter(|l| l.parent_id.as_deref() == parent)
            {
                let effective = visible && layer.visible;
                out.push(LayerRow {
                    id: layer.id.clone(),
                    depth,
                    visible: effective,
                    collapsed: collapsed.contains(&layer.id),
                });
                if matches!(layer.content.as_ref(), Content::Group)
                    && !collapsed.contains(&layer.id)
                {
                    visit(doc, Some(&layer.id), depth + 1, effective, collapsed, out);
                }
            }
        }
        let mut rows = Vec::new();
        visit(
            &self.history.document,
            None,
            0,
            true,
            &self.collapsed_groups,
            &mut rows,
        );
        rows
    }
    pub fn select(&mut self, id: Option<String>, mode: SelectionMode) {
        self.finish_gesture();
        let Some(id) = id.filter(|id| self.history.document.layers.iter().any(|l| l.id == *id))
        else {
            self.single_selection(None);
            return;
        };
        match mode {
            SelectionMode::Toggle => {
                if self.selection.ids.contains(&id) {
                    self.selection.ids.retain(|v| v != &id);
                } else {
                    self.selection.ids.push(id.clone());
                }
                self.selection.anchor = Some(id);
            }
            SelectionMode::Range => {
                let layers = &self.history.document.layers;
                let anchor = layers
                    .iter()
                    .position(|l| Some(&l.id) == self.selection.anchor.as_ref());
                let end = layers.iter().position(|l| l.id == id).unwrap();
                if let Some(anchor) = anchor {
                    self.selection.ids = layers[anchor.min(end)..=anchor.max(end)]
                        .iter()
                        .filter(|l| l.id != id)
                        .map(|l| l.id.clone())
                        .chain([id.clone()])
                        .collect();
                } else {
                    self.single_selection(Some(id));
                }
            }
            SelectionMode::Replace => self.single_selection(Some(id)),
        };
        self.paint_target = PaintTarget::Content;
    }
    pub fn fit(&mut self) {
        let doc = &self.history.document;
        self.viewport.zoom = 1f64
            .min((self.viewport.width - 80.) / doc.width as f64)
            .min((self.viewport.height - 80.) / doc.height as f64)
            .max(0.05);
        self.viewport.pan = Point::default();
    }
    pub fn begin_edit(&mut self, label: &str) {
        self.history.begin(label, self.selection.clone());
    }
    pub fn end_edit(&mut self) {
        self.history.commit(self.selection.clone());
    }
    pub fn edit(
        &mut self,
        label: &str,
        doc: Document,
        selection: Option<Vec<String>>,
    ) -> Result<()> {
        doc.validate()?;
        self.finish_gesture();
        self.begin_edit(label);
        self.history.preview(doc);
        if let Some(ids) = selection {
            self.selection.anchor = ids.last().cloned();
            self.selection.ids = ids;
            self.paint_target = PaintTarget::Content;
        }
        self.reconcile();
        self.end_edit();
        Ok(())
    }
    fn reconcile(&mut self) {
        let had = !self.selection.ids.is_empty();
        self.selection
            .ids
            .retain(|id| self.history.document.layers.iter().any(|l| l.id == *id));
        if had && self.selection.ids.is_empty() {
            self.single_selection(self.history.document.layers.last().map(|l| l.id.clone()));
        }
        if self.selection.ids.len() != 1 || self.selected().and_then(|l| l.mask.as_ref()).is_none()
        {
            self.paint_target = PaintTarget::Content;
        }
    }
    pub fn update_layer(&mut self, patch: serde_json::Value) -> Result<()> {
        let Some(layer) = self.selected() else {
            return Ok(());
        };
        let patch = patch
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Expected layer properties"))?;
        // Only metadata properties are writable across the bridge. Assets and stroke buffers are opaque.
        let allowed = [
            "name",
            "visible",
            "locked",
            "x",
            "y",
            "scaleX",
            "scaleY",
            "rotation",
            "flipX",
            "flipY",
            "opacity",
            "blend",
            "sampling",
            "brightness",
            "saturation",
            "blur",
            "content",
            "mask",
        ];
        ensure!(
            patch.keys().all(|k| allowed.contains(&k.as_str())),
            "Unsupported layer property"
        );
        if layer.locked && patch.keys().any(|k| k != "locked" && k != "visible") {
            return Ok(());
        }
        if matches!(layer.content.as_ref(), Content::Group) {
            ensure!(
                patch
                    .keys()
                    .all(|k| ["name", "visible", "locked", "opacity"].contains(&k.as_str())),
                "Folders support only name, visibility, lock, and opacity"
            );
        }
        // Property validation never serializes image bytes or brush point arrays on the UI thread.
        let mut properties = layer.clone();
        properties.content = Arc::new(Content::Paint);
        properties.strokes.clear();
        properties.mask = None;
        let mut value = serde_json::to_value(&properties)?;
        for (key, v) in patch {
            if key == "mask" {
                let Some(mask) = &layer.mask else {
                    bail!("Add a mask before editing it");
                };
                let mut m =
                    serde_json::json!({"enabled": mask.enabled, "base": mask.base, "strokes": []});
                let enabled = v
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| anyhow::anyhow!("Expected mask enabled flag"))?;
                m["enabled"] = enabled.into();
                value[key] = m;
            } else if key == "content" {
                ensure!(
                    v.get("kind").and_then(|v| v.as_str()) != Some("image"),
                    "Image assets can only be imported"
                );
                value[key] = v.clone();
            } else {
                value[key] = v.clone();
            }
        }
        let mut next: Layer = serde_json::from_value(value)?;
        next.validate()?;
        if !patch.contains_key("content") {
            next.content = layer.content.clone();
        }
        next.strokes = layer.strokes.clone();
        if patch.contains_key("mask") {
            let mut mask = layer
                .mask
                .as_ref()
                .expect("checked mask exists")
                .as_ref()
                .clone();
            mask.enabled = next.mask.as_ref().expect("validated mask patch").enabled;
            next.mask = Some(Arc::new(mask));
        } else {
            next.mask = layer.mask.clone();
        }
        Self::follow_mask(layer, &mut next);
        let mut doc = self.history.document.clone();
        doc.replace(next);
        // A slider drag previews every step into one undo entry, as `beginOpacityEdit` /
        // `finishOpacityEdit` bracket upstream appearance drags.
        if matches!(self.gesture, Some(Gesture::Property)) {
            self.history.preview(doc);
            return Ok(());
        }
        self.edit("Edit layer", doc, None)
    }
    fn inserted(&self, layers: Vec<Layer>) -> Document {
        let mut doc = self.history.document.clone();
        let parent_id = self.selected().and_then(|selected| {
            if matches!(selected.content.as_ref(), Content::Group) {
                Some(selected.id.clone())
            } else {
                selected.parent_id.clone()
            }
        });
        let layers = layers
            .into_iter()
            .map(|mut layer| {
                layer.parent_id = parent_id.clone();
                layer
            })
            .collect::<Vec<_>>();
        let at = doc
            .layers
            .iter()
            .position(|l| Some(l.id.as_str()) == self.selected_id())
            .map(|n| n + 1)
            .unwrap_or(doc.layers.len());
        doc.layers.splice(at..at, layers);
        doc
    }
    pub fn add_layers(&mut self, layers: Vec<Layer>) -> Result<()> {
        if layers.is_empty() {
            return Ok(());
        }
        ensure!(
            self.history.document.layers.len() + layers.len() <= 100,
            "The editor supports up to 100 layers"
        );
        let selection = vec![layers.last().unwrap().id.clone()];
        let label = if layers.len() == 1 {
            "Add layer"
        } else {
            "Add layers"
        };
        self.edit(label, self.inserted(layers), Some(selection))?;
        if let Some(parent) = self.selected().and_then(|l| l.parent_id.clone()) {
            self.collapsed_groups.remove(&parent);
        }
        Ok(())
    }
    pub fn import_layers(&mut self, mut layers: Vec<Layer>) -> Result<()> {
        let doc = &self.history.document;
        for l in &mut layers {
            let scale = 1f64
                .min(doc.width as f64 / l.width as f64)
                .min(doc.height as f64 / l.height as f64);
            l.scale_x = scale;
            l.scale_y = scale;
            l.x = (doc.width as f64 - l.width as f64 * scale) / 2.;
            l.y = (doc.height as f64 - l.height as f64 * scale) / 2.;
        }
        self.add_layers(layers)
    }
    fn with_layers(&self, layers: Vec<Layer>) -> Document {
        let mut doc = self.history.document.clone();
        let original = self
            .history
            .before_document()
            .unwrap_or(&self.history.document);
        for mut l in layers {
            if let Some(old) = original.layers.iter().find(|v| v.id == l.id) {
                Self::follow_mask(old, &mut l);
            }
            doc.replace(l);
        }
        doc
    }
    fn follow_mask(old: &Layer, next: &mut Layer) {
        let before = MaskPlacement::of(old);
        let after = MaskPlacement::of(next);
        let transformed = next.clone();
        if let Some(mask) = &mut next.mask {
            if mask
                .raster
                .as_ref()
                .is_some_and(|v| v.width == 1 && v.height == 1)
            {
                Arc::make_mut(mask).placement = None;
                return;
            }
            if before == after {
                return;
            }
            let mask = Arc::make_mut(mask);
            if mask.linked {
                if let Some(placed) = mask.placement {
                    mask.placement = Some(placed.following(old, &transformed));
                }
            } else if mask.placement.is_none() {
                mask.placement = Some(before);
            }
        }
    }
    fn moved(layers: &[Layer], delta: Point) -> Vec<Layer> {
        let dx = layers
            .iter()
            .map(|l| -100000. - l.x)
            .fold(f64::NEG_INFINITY, f64::max)
            .max(layers.iter().map(|l| 100000. - l.x).fold(delta.x, f64::min));
        let dy = layers
            .iter()
            .map(|l| -100000. - l.y)
            .fold(f64::NEG_INFINITY, f64::max)
            .max(layers.iter().map(|l| 100000. - l.y).fold(delta.y, f64::min));
        layers
            .iter()
            .map(|l| {
                let mut n = l.clone();
                n.x += dx;
                n.y += dy;
                n
            })
            .collect()
    }
    pub fn finish_gesture(&mut self) {
        if let Some(g) = self.gesture.take()
            && !matches!(
                g,
                Gesture::Pan { .. } | Gesture::Crop { .. } | Gesture::PixelSelection { .. }
            )
        {
            self.end_edit();
        }
    }
    pub fn cancel_gesture(&mut self) {
        if matches!(self.gesture, Some(Gesture::PixelSelection { .. })) {
            if let Some(Gesture::PixelSelection { before, .. }) = self.gesture.take() {
                self.pixel_selection = before;
                return;
            }
        }
        if let Some(Gesture::Crop { before, .. }) = self.gesture.take() {
            self.crop_rect = before;
            return;
        }
        let selection = self.history.cancel();
        self.gesture = None;
        self.restore_selection(selection);
    }
    fn restore_selection(&mut self, selection: Option<Selection>) {
        if let Some(selection) = selection {
            let keep = self.paint_target == PaintTarget::Mask
                && self.selection.ids.last() == selection.ids.last();
            self.selection = selection;
            self.paint_target = if keep && self.selected().and_then(|l| l.mask.as_ref()).is_some() {
                PaintTarget::Mask
            } else {
                PaintTarget::Content
            };
        }
    }
    fn mask_edit(&mut self, base: Option<MaskMode>, remove: bool) -> Result<()> {
        let Some(l) = self.selected() else {
            return Ok(());
        };
        if l.locked {
            return Ok(());
        }
        let mut layer = l.clone();
        layer.mask = if remove {
            None
        } else {
            Some(Arc::new(LayerMask {
                enabled: l.mask.as_ref().map(|m| m.enabled).unwrap_or(true),
                base: base.unwrap_or(MaskMode::Reveal),
                raster: Some(Arc::new(MaskRaster::solid(
                    base.unwrap_or(MaskMode::Reveal) == MaskMode::Reveal,
                ))),
                linked: l.mask.as_ref().map(|m| m.linked).unwrap_or(true),
                placement: l.mask.as_ref().and_then(|m| m.placement),
                strokes: vec![],
            }))
        };
        let mut doc = self.history.document.clone();
        doc.version = 2;
        doc.replace(layer);
        self.edit("Edit mask", doc, None)
    }
    pub fn command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Select { id, mode } => self.select(id, mode),
            Command::SelectAll => {
                self.finish_gesture();
                self.selection.ids = self
                    .history
                    .document
                    .layers
                    .iter()
                    .map(|l| l.id.clone())
                    .collect();
                self.selection.anchor = self.selection.ids.first().cloned();
                self.paint_target = PaintTarget::Content;
            }
            Command::SetTool { tool } => {
                self.finish_gesture();
                if tool != Tool::Crop {
                    self.crop_rect = None;
                }
                self.tool = tool;
            }
            Command::Fit => self.fit(),
            Command::Zoom { zoom, point } => {
                ensure!(zoom.is_finite(), "Invalid zoom");
                self.finish_gesture();
                self.viewport = geometry::zoom_at(
                    &self.history.document,
                    &self.viewport,
                    point.unwrap_or(Point::new(
                        self.viewport.width / 2.,
                        self.viewport.height / 2.,
                    )),
                    zoom,
                );
            }
            Command::ResizeViewport { width, height } => {
                ensure!(
                    width.is_finite() && height.is_finite() && width <= 8192. && height <= 8192.,
                    "Invalid viewport"
                );
                self.viewport.width = width.max(100.);
                self.viewport.height = height.max(100.);
            }
            Command::Pan { delta } => {
                ensure!(delta.x.is_finite() && delta.y.is_finite(), "Invalid pan");
                self.viewport.pan.x += delta.x;
                self.viewport.pan.y += delta.y;
            }
            Command::SetViewport { viewport } => {
                ensure!(
                    viewport.width.is_finite()
                        && viewport.height.is_finite()
                        && viewport.zoom.is_finite()
                        && viewport.pan.x.is_finite()
                        && viewport.pan.y.is_finite()
                        && (1.0..=8192.).contains(&viewport.width)
                        && (1.0..=8192.).contains(&viewport.height)
                        && (0.05..=8.).contains(&viewport.zoom),
                    "Invalid viewport"
                );
                self.viewport = viewport;
            }
            Command::SetColor { color } => {
                ensure!(
                    color.len() == 7 && color_valid(&color),
                    "Enter a six-digit hex color"
                );
                self.color = color;
            }
            Command::SetBrush { size, opacity } => {
                ensure!(
                    (1.0..=1000.).contains(&size) && (0.0..=1.).contains(&opacity),
                    "Invalid brush settings"
                );
                self.brush_size = size;
                self.brush_opacity = opacity;
            }
            Command::UpdateLayer { patch } => self.update_layer(patch)?,
            Command::AddPaintLayer => self.add_layers(vec![Layer::new(
                "Paint layer",
                self.history.document.width,
                self.history.document.height,
                Content::Paint,
            )])?,
            Command::AddGradient => self.add_layers(vec![Layer::new(
                "Gradient",
                self.history.document.width,
                self.history.document.height,
                Content::Gradient {
                    from: self.color.clone(),
                    to: "#171c32".into(),
                },
            )])?,
            Command::AddGroup => {
                let mut folder = Layer::new(
                    "Folder",
                    self.history.document.width,
                    self.history.document.height,
                    Content::Group,
                );
                folder.name = Self::next_folder_name(&self.history.document);
                self.add_layers(vec![folder])?;
            }
            Command::GroupSelected => {
                let selected: HashSet<_> = self.selection.ids.iter().cloned().collect();
                if !selected.is_empty() {
                    let mut doc = self.history.document.clone();
                    // A selected folder carries its descendants. Only roots are reparented.
                    let roots: HashSet<_> = selected
                        .iter()
                        .filter(|id| {
                            let mut parent = doc
                                .layers
                                .iter()
                                .find(|l| &l.id == *id)
                                .and_then(|l| l.parent_id.as_deref());
                            while let Some(pid) = parent {
                                if selected.contains(pid) {
                                    return false;
                                }
                                parent = doc
                                    .layers
                                    .iter()
                                    .find(|l| l.id == pid)
                                    .and_then(|l| l.parent_id.as_deref());
                            }
                            true
                        })
                        .cloned()
                        .collect();
                    let ordered: Vec<_> = doc
                        .ordered_layers()
                        .into_iter()
                        .filter(|l| roots.contains(&l.id))
                        .map(|l| l.id.clone())
                        .collect();
                    let ancestors = |id: &str| {
                        let mut path = Vec::new();
                        let mut parent = doc
                            .layers
                            .iter()
                            .find(|l| l.id == id)
                            .and_then(|l| l.parent_id.clone());
                        while let Some(pid) = parent {
                            parent = doc
                                .layers
                                .iter()
                                .find(|l| l.id == pid)
                                .and_then(|l| l.parent_id.clone());
                            path.push(Some(pid));
                        }
                        path.push(None);
                        path
                    };
                    let parent = ordered
                        .first()
                        .and_then(|first| {
                            ancestors(first).into_iter().find(|candidate| {
                                ordered.iter().all(|id| ancestors(id).contains(candidate))
                            })
                        })
                        .flatten();
                    let mut folder = Layer::new("Folder", doc.width, doc.height, Content::Group);
                    folder.parent_id = parent.clone();
                    folder.name = Self::next_folder_name(&doc);
                    let folder_id = folder.id.clone();
                    let branches: HashSet<_> = ordered
                        .iter()
                        .map(|id| {
                            let mut branch = id.clone();
                            while let Some(next) = doc
                                .layers
                                .iter()
                                .find(|l| l.id == branch)
                                .and_then(|l| l.parent_id.clone())
                            {
                                if Some(&next) == parent.as_ref() {
                                    break;
                                }
                                branch = next;
                            }
                            branch
                        })
                        .collect();
                    let top = doc
                        .layers
                        .iter()
                        .rposition(|l| branches.contains(&l.id))
                        .unwrap();
                    doc.layers.insert(top + 1, folder);
                    for layer in &mut doc.layers {
                        if roots.contains(&layer.id) {
                            layer.parent_id = Some(folder_id.clone());
                        }
                    }
                    doc.version = 2;
                    self.edit("Group layers", doc, Some(vec![folder_id]))?;
                    if let Some(parent) = parent {
                        self.collapsed_groups.remove(&parent);
                    }
                }
            }
            Command::ToggleGroupExpansion { id } => {
                if self
                    .history
                    .document
                    .layers
                    .iter()
                    .any(|l| l.id == id && matches!(l.content.as_ref(), Content::Group))
                {
                    if !self.collapsed_groups.remove(&id) {
                        if self.selected_id().is_some_and(|selected| {
                            self.history.document.descendants(&id).contains(selected)
                        }) {
                            self.single_selection(Some(id.clone()));
                        }
                        self.collapsed_groups.insert(id);
                    }
                }
            }
            Command::MoveToGroup { parent_id } => {
                let mut doc = self.history.document.clone();
                if let Some(parent) = &parent_id {
                    ensure!(
                        doc.layers.iter().any(
                            |l| l.id == *parent && matches!(l.content.as_ref(), Content::Group)
                        ),
                        "Choose a folder"
                    );
                    ensure!(
                        !self.selection.ids.contains(parent),
                        "A folder cannot contain itself"
                    );
                }
                let selected: std::collections::HashSet<_> =
                    self.selected_roots().into_iter().map(|l| l.id).collect();
                if selected.is_empty() {
                    return Ok(());
                }
                let mut moving = Vec::new();
                doc.layers.retain(|layer| {
                    if selected.contains(&layer.id) {
                        let mut moved = layer.clone();
                        moved.parent_id = parent_id.clone();
                        moving.push(moved);
                        false
                    } else {
                        true
                    }
                });
                doc.layers.extend(moving);
                doc.version = 2;
                self.edit("Move layers into folder", doc, None)?;
                if let Some(parent) = parent_id {
                    self.collapsed_groups.remove(&parent);
                }
            }
            Command::Duplicate => {
                let mut doc = self.history.document.clone();
                let mut ids = vec![];
                let roots = self.selected_roots();
                let mut layers = doc.layers.clone();
                for root in roots.iter().rev() {
                    let subtree = doc.descendants(&root.id);
                    let members: Vec<_> = doc
                        .layers
                        .iter()
                        .filter(|l| l.id == root.id || subtree.contains(&l.id))
                        .cloned()
                        .collect();
                    let mapping: std::collections::HashMap<_, _> =
                        members.iter().map(|l| (l.id.clone(), id())).collect();
                    let mut copies = Vec::new();
                    for mut copy in members {
                        copy.id = mapping[&copy.id].clone();
                        copy.parent_id = copy
                            .parent_id
                            .map(|parent| mapping.get(&parent).cloned().unwrap_or(parent));
                        copy.name =
                            format!("{} copy", copy.name.chars().take(190).collect::<String>());
                        copy.locked = false;
                        if copy.id == mapping[&root.id] {
                            ids.push(copy.id.clone());
                        }
                        copies.push(copy);
                    }
                    let at = layers.iter().position(|l| l.id == root.id).unwrap() + 1;
                    layers.splice(at..at, copies);
                }
                if !ids.is_empty() {
                    doc.layers = layers;
                    self.edit("Duplicate layers", doc, Some(ids))?;
                }
            }
            Command::Remove => {
                let mut doc = self.history.document.clone();
                let mut removing = std::collections::HashSet::new();
                for id in &self.selection.ids {
                    if doc.layers.iter().any(|l| l.id == *id && !l.locked) {
                        removing.insert(id.clone());
                        removing.extend(doc.descendants(id));
                    }
                }
                doc.layers.retain(|l| !removing.contains(&l.id));
                self.edit("Delete layers", doc, None)?;
            }
            Command::Reorder { direction } => {
                ensure!(
                    direction == 1 || direction == -1,
                    "Invalid reorder direction"
                );
                let mut doc = self.history.document.clone();
                let moving: HashSet<_> = self
                    .selected_roots()
                    .into_iter()
                    .filter(|l| !l.locked)
                    .map(|l| l.id)
                    .collect();
                let parents: HashSet<_> = doc
                    .layers
                    .iter()
                    .filter(|l| moving.contains(&l.id))
                    .map(|l| l.parent_id.clone())
                    .collect();
                for parent in parents {
                    let siblings: Vec<_> = doc
                        .layers
                        .iter()
                        .enumerate()
                        .filter(|(_, l)| l.parent_id == parent)
                        .map(|(i, _)| i)
                        .collect();
                    let positions: Vec<_> = if direction == 1 {
                        (0..siblings.len()).rev().collect()
                    } else {
                        (0..siblings.len()).collect()
                    };
                    for position in positions {
                        let other = position as isize + direction as isize;
                        if other >= 0
                            && (other as usize) < siblings.len()
                            && moving.contains(&doc.layers[siblings[position]].id)
                            && !moving.contains(&doc.layers[siblings[other as usize]].id)
                        {
                            doc.layers
                                .swap(siblings[position], siblings[other as usize]);
                        }
                    }
                }
                self.edit("Reorder layers", doc, None)?;
            }
            Command::ReorderTo { target_id, side } => {
                let moving: Vec<_> = self
                    .selected_roots()
                    .into_iter()
                    .filter(|l| !l.locked)
                    .collect();
                if moving.is_empty() || moving.iter().any(|l| l.id == target_id) {
                    return Ok(());
                }
                let mut doc = self.history.document.clone();
                let Some(target) = doc.layers.iter().find(|l| l.id == target_id).cloned() else {
                    return Ok(());
                };
                ensure!(
                    !moving
                        .iter()
                        .any(|l| doc.descendants(&l.id).contains(&target_id)),
                    "A folder cannot be moved into itself"
                );
                doc.layers.retain(|l| !moving.iter().any(|v| v.id == l.id));
                if let Some(at) = doc.layers.iter().position(|l| l.id == target_id) {
                    let at = at + usize::from(matches!(side, Side::Above));
                    doc.layers.splice(
                        at..at,
                        moving.into_iter().map(|mut l| {
                            l.parent_id = target.parent_id.clone();
                            l
                        }),
                    );
                    self.edit("Reorder layers", doc, None)?;
                    if let Some(parent) = target.parent_id {
                        self.collapsed_groups.remove(&parent);
                    }
                }
            }
            Command::Nudge { delta } => {
                ensure!(
                    delta.x.is_finite() && delta.y.is_finite(),
                    "Invalid movement"
                );
                let mut moving_ids = HashSet::new();
                for root in self.selected_roots() {
                    moving_ids.insert(root.id.clone());
                    moving_ids.extend(self.history.document.descendants(&root.id));
                }
                let layers: Vec<_> = self
                    .history
                    .document
                    .layers
                    .iter()
                    .filter(|l| {
                        moving_ids.contains(&l.id)
                            && !l.locked
                            && l.visible
                            && !matches!(l.content.as_ref(), Content::Group)
                    })
                    .cloned()
                    .collect();
                if !layers.is_empty() {
                    self.edit(
                        "Move layers",
                        self.with_layers(Self::moved(&layers, delta)),
                        None,
                    )?;
                }
            }
            Command::ResizeCanvas { options } => {
                self.finish_gesture();
                let doc = resize_canvas(&self.history.document, &options)?;
                if doc != self.history.document {
                    self.edit("Canvas Size", doc, None)?;
                    self.pixel_selection = None;
                    self.fit();
                }
            }
            Command::SetCropRatio { ratio } => {
                self.finish_gesture();
                self.crop_ratio = ratio;
                if let Some(r) = ratio.value(&self.history.document) {
                    let current = self
                        .crop_rect
                        .unwrap_or_else(|| CropRect::from_document(&self.history.document));
                    let next = CropRect {
                        y: current.y + (current.height - current.width / r) / 2.,
                        height: current.width / r,
                        ..current
                    }
                    .snapped();
                    if next.validate().is_ok() {
                        self.crop_rect = Some(next);
                    }
                }
            }
            Command::CommitCrop => {
                self.finish_gesture();
                if let Some(rect) = self.crop_rect {
                    let next = crop_canvas(&self.history.document, rect)?;
                    self.edit("Crop", next, None)?;
                    self.crop_rect = None;
                    self.pixel_selection = None;
                    self.fit();
                }
            }
            Command::CancelCrop => {
                self.cancel_gesture();
                self.crop_rect = None;
            }
            Command::SetMarqueeKind { kind } => {
                self.finish_gesture();
                self.marquee_kind = kind;
            }
            Command::SetSelectionMode { mode } => {
                self.finish_gesture();
                self.selection_mode = mode;
            }
            Command::DeselectPixels => {
                self.finish_gesture();
                self.pixel_selection = None;
            }
            Command::SelectAllPixels => {
                self.finish_gesture();
                let doc = &self.history.document;
                self.pixel_selection = Some(PixelSelection {
                    width: doc.width,
                    height: doc.height,
                    pixels: Arc::new(vec![255; doc.width as usize * doc.height as usize]),
                    bounds: Some(pixel_selection::SelectionBounds {
                        x: 0,
                        y: 0,
                        width: doc.width,
                        height: doc.height,
                    }),
                    feather: 0.,
                });
            }
            Command::BeginPropertyEdit { label } => {
                ensure!(!label.is_empty() && label.len() <= 64, "Invalid edit label");
                self.finish_gesture();
                self.begin_edit(&label);
                self.gesture = Some(Gesture::Property);
            }
            Command::PickUnder { point } => {
                self.finish_gesture();
                ensure!(
                    point.x.is_finite() && point.y.is_finite(),
                    "Invalid pointer coordinate"
                );
                let p = geometry::to_document(&self.history.document, &self.viewport, point);
                self.pick_under(p);
            }
            Command::EditText { id, point } => {
                self.finish_gesture();
                let named = id.and_then(|id| {
                    self.history
                        .document
                        .layers
                        .iter()
                        .find(|l| l.id == id && matches!(l.content.as_ref(), Content::Text { .. }))
                        .map(|l| l.id.clone())
                });
                // TypeTool's `beginTextGesture` edits the text under the click instead of
                // adding another layer; the topmost live text wins.
                let target = named.or_else(|| {
                    point.and_then(|p| {
                        let p = geometry::to_document(&self.history.document, &self.viewport, p);
                        geometry::hit_layers(&self.history.document, p)
                            .into_iter()
                            .find(|l| matches!(l.content.as_ref(), Content::Text { .. }))
                            .map(|l| l.id.clone())
                    })
                });
                if let Some(id) = target {
                    self.single_selection(Some(id));
                    self.text_edit_requests += 1;
                }
            }
            Command::FeatherSelection { amount } => {
                // `confirmSelectionAmount`'s 1...250 range for Feather, then
                // `featherSelection(by:)`: stacked feathers combine like the blurs they are.
                ensure!(
                    (1..=250).contains(&amount),
                    "Feather must be between 1 and 250 pixels"
                );
                // `canModifySelection` needs a non-empty selection and no outline in progress.
                if self.selection_draft().is_some() {
                    return Ok(());
                }
                if let Some(current) = &self.pixel_selection
                    && current.bounds.is_some()
                {
                    let mut next = current.clone();
                    // Two soft edges together spread a little less than their sum, as blurs do.
                    let softened = (current.feather * current.feather
                        + f64::from(amount) * f64::from(amount))
                    .sqrt();
                    next.feather = 250f64.min(softened);
                    self.pixel_selection = Some(next);
                }
            }
            Command::ClearSelectedPixels => {
                self.finish_gesture();
                if let (Some(selection), Some(layer)) = (&self.pixel_selection, self.selected()) {
                    if selection.bounds.is_none() {
                        return Ok(());
                    }
                    if !layer.locked && !matches!(layer.content.as_ref(), Content::Group) {
                        let mut next = layer.clone();
                        let image = render::clear_selected_pixels(&next, selection)?;
                        next.content = Arc::new(render::png_content(&image)?);
                        next.strokes.clear();
                        self.edit("Clear selected pixels", self.with_layers(vec![next]), None)?;
                    }
                }
            }
            Command::AddMask { base } => {
                if self.selection.ids.len() == 1
                    && self.selected().is_some_and(|l| {
                        !l.locked
                            && l.mask.is_none()
                            && !matches!(l.content.as_ref(), Content::Group)
                    })
                {
                    self.mask_edit(Some(base), false)?;
                    self.paint_target = PaintTarget::Mask;
                    self.tool = Tool::Brush;
                }
            }
            Command::SetPaintTarget { target } => {
                self.finish_gesture();
                if target == PaintTarget::Content
                    || (self.selection.ids.len() == 1
                        && self.selected().and_then(|l| l.mask.as_ref()).is_some())
                {
                    self.paint_target = target;
                    if target == PaintTarget::Mask {
                        self.tool = Tool::Brush;
                    }
                }
            }
            Command::SetMaskMode { mode } => {
                self.finish_gesture();
                self.mask_mode = mode;
            }
            Command::ResetMask { base } => {
                if self.selected().and_then(|l| l.mask.as_ref()).is_some() {
                    self.mask_edit(Some(base), false)?;
                }
            }
            Command::RemoveMask => {
                if self.selected().and_then(|l| l.mask.as_ref()).is_some() {
                    self.mask_edit(None, true)?;
                    self.paint_target = PaintTarget::Content;
                }
            }
            Command::ToggleMaskLink => {
                if let Some(layer) = self.selected().cloned() {
                    if !layer.locked {
                        if let Some(mask) = &layer.mask {
                            let mut next = layer.clone();
                            let mut mask = mask.as_ref().clone();
                            mask.linked = !mask.linked;
                            next.mask = Some(Arc::new(mask));
                            self.edit("Link layer mask", self.with_layers(vec![next]), None)?;
                        }
                    }
                }
            }
            Command::Undo => {
                if self.gesture.is_some() {
                    self.cancel_gesture();
                }
                let s = self.history.undo();
                self.restore_selection(s);
            }
            Command::Redo => {
                if self.gesture.is_some() {
                    self.cancel_gesture();
                }
                let s = self.history.redo();
                self.restore_selection(s);
            }
            Command::FinishGesture => self.finish_gesture(),
            Command::CancelGesture => self.cancel_gesture(),
            Command::Pointer { samples } => {
                ensure!(samples.len() <= 4096, "Too many pointer samples");
                for sample in samples {
                    ensure!(
                        sample.point.x.is_finite() && sample.point.y.is_finite(),
                        "Invalid pointer coordinate"
                    );
                    self.pointer(sample)?;
                }
            }
        }
        Ok(())
    }
    pub fn pointer(&mut self, sample: PointerSample) -> Result<()> {
        let PointerSample {
            phase,
            point: vp,
            modifiers,
        } = sample;
        if phase == Phase::Cancel {
            self.cancel_gesture();
            return Ok(());
        }
        let p = geometry::to_document(&self.history.document, &self.viewport, vp);
        if phase == Phase::Down {
            self.finish_gesture();
            if self.tool == Tool::Crop {
                let before = self.crop_rect;
                let original =
                    before.unwrap_or_else(|| CropRect::from_document(&self.history.document));
                let mode = crop::hit(original, p, self.viewport.zoom);
                self.gesture = Some(Gesture::Crop {
                    drag: CropDrag {
                        start: p,
                        original,
                        mode,
                    },
                    before,
                });
                return Ok(());
            }
            if self.tool == Tool::Hand {
                self.gesture = Some(Gesture::Pan {
                    origin: vp,
                    pan: self.viewport.pan,
                });
                return Ok(());
            }
            if matches!(self.tool, Tool::Marquee | Tool::Lasso) {
                let mode = if modifiers.alt {
                    PixelSelectionMode::Subtract
                } else if modifiers.shift {
                    PixelSelectionMode::Add
                } else {
                    self.selection_mode
                };
                let marquee = (self.tool == Tool::Marquee).then_some(self.marquee_kind);
                self.gesture = Some(Gesture::PixelSelection {
                    draft: SelectionDraft::new(p, marquee, mode),
                    before: self.pixel_selection.clone(),
                });
                return Ok(());
            }
            if self.tool == Tool::Move {
                if self.paint_target == PaintTarget::Mask
                    && self.selection.ids.len() == 1
                    && let Some(layer) = self.selected().cloned()
                    && !layer.locked
                    && layer.visible
                    && let Some(mask) = &layer.mask
                    && !mask.linked
                {
                    let placement = mask.placement.unwrap_or_else(|| MaskPlacement::of(&layer));
                    let at = geometry::to_local(&placement.as_layer(&layer), p);
                    if at.x < 0.
                        || at.y < 0.
                        || at.x > layer.width as f64
                        || at.y > layer.height as f64
                    {
                        return Ok(());
                    }
                    self.begin_edit("Move layer mask");
                    self.gesture = Some(Gesture::MaskMove {
                        origin: p,
                        layer,
                        placement,
                    });
                    return Ok(());
                }
                if let Some(l) = self.selected().cloned()
                    && self.selection.ids.len() == 1
                    && !l.locked
                    && l.visible
                    && let Some(handle) = geometry::hit_handle(&l, p, self.viewport.zoom)
                {
                    self.begin_edit(if handle == "rotate" {
                        "Rotate layer"
                    } else {
                        "Resize layer"
                    });
                    let handle_point = geometry::handles(&l, self.viewport.zoom)
                        .into_iter()
                        .find(|h| h.0 == handle)
                        .unwrap()
                        .1;
                    self.gesture = Some(Gesture::Transform {
                        origin: p,
                        handle,
                        handle_point,
                        layer: l,
                    });
                    return Ok(());
                }
                let hit = geometry::hit_test(&self.history.document, p).map(|l| l.id.clone());
                if modifiers.shift {
                    if hit.is_some() {
                        self.select(hit, SelectionMode::Toggle);
                    }
                    return Ok(());
                }
                // Cmd/Ctrl-click walks the layers whose bounds contain the point, top to
                // bottom and back around. Upstream's Cmd-click re-picks the topmost layer;
                // cycling is a local extension so a full-canvas layer cannot bury the stack.
                if modifiers.meta || modifiers.control {
                    self.pick_under(p);
                    return Ok(());
                }
                if !hit
                    .as_ref()
                    .is_some_and(|id| self.selection.ids.contains(id))
                {
                    self.single_selection(hit.clone());
                }
                if hit.is_some() {
                    self.begin_edit("Move layers");
                    self.gesture = Some(Gesture::Move {
                        origin: p,
                        layers: self
                            .selected_layers()
                            .into_iter()
                            .filter(|l| !l.locked && l.visible)
                            .collect(),
                    });
                }
                return Ok(());
            }
            if p.x < 0.
                || p.y < 0.
                || p.x > self.history.document.width as f64
                || p.y > self.history.document.height as f64
            {
                return Ok(());
            }
            if self.tool == Tool::Text {
                // TypeTool's `beginTextGesture`: a click on live text edits it.
                let existing = geometry::hit_layers(&self.history.document, p)
                    .into_iter()
                    .find(|l| matches!(l.content.as_ref(), Content::Text { .. }))
                    .map(|l| l.id.clone());
                if let Some(id) = existing {
                    self.single_selection(Some(id));
                    self.text_edit_requests += 1;
                    self.tool = Tool::Move;
                    return Ok(());
                }
                let mut l = Layer::new(
                    "Text",
                    800.min(self.history.document.width),
                    240.min(self.history.document.height),
                    Content::Text {
                        text: "Your text".into(),
                        font_size: 64.,
                        font_family: FontFamily::SansSerif,
                        color: self.color.clone(),
                    },
                );
                l.x = p.x;
                l.y = p.y;
                self.add_layers(vec![l])?;
                self.tool = Tool::Move;
                return Ok(());
            }
            if matches!(self.tool, Tool::Rectangle | Tool::Ellipse) {
                if self.history.document.layers.len() >= 100 {
                    return Ok(());
                }
                let ellipse = self.tool == Tool::Ellipse;
                let mut l = Layer::new(
                    if ellipse { "Ellipse" } else { "Rectangle" },
                    1,
                    1,
                    Content::Shape {
                        shape: if ellipse {
                            Shape::Ellipse
                        } else {
                            Shape::Rectangle
                        },
                        color: self.color.clone(),
                    },
                );
                l.x = p.x;
                l.y = p.y;
                self.begin_edit("Draw shape");
                self.history.preview(self.inserted(vec![l.clone()]));
                self.single_selection(Some(l.id.clone()));
                self.gesture = Some(Gesture::Shape {
                    origin: p,
                    layer: l,
                });
                return Ok(());
            }
            if matches!(self.tool, Tool::Brush | Tool::Eraser) {
                if self.selection.ids.len() > 1 {
                    return Ok(());
                }
                if self.paint_target == PaintTarget::Mask {
                    let Some(l) = self.selected().cloned() else {
                        return Ok(());
                    };
                    let Some(mask) = &l.mask else {
                        return Ok(());
                    };
                    if !mask.enabled || l.locked || !l.visible || mask.strokes.len() >= 10000 {
                        return Ok(());
                    }
                    let mode = if self.tool == Tool::Eraser {
                        if self.mask_mode == MaskMode::Hide {
                            MaskMode::Reveal
                        } else {
                            MaskMode::Hide
                        }
                    } else {
                        self.mask_mode
                    };
                    let stroke = MaskStroke {
                        mode,
                        size: self.brush_size,
                        opacity: self.brush_opacity,
                        points: vec![geometry::to_local(&l, p).bounded()],
                    };
                    self.begin_edit(if mode == MaskMode::Hide {
                        "Hide on mask"
                    } else {
                        "Reveal on mask"
                    });
                    self.gesture = Some(Gesture::Mask { layer: l, stroke });
                } else {
                    if self
                        .selected()
                        .is_some_and(|l| l.locked || !l.visible || l.strokes.len() >= 10000)
                    {
                        return Ok(());
                    }
                    if self.selected().is_none() && self.history.document.layers.len() >= 100 {
                        return Ok(());
                    }
                    self.begin_edit(if self.tool == Tool::Brush {
                        "Brush stroke"
                    } else {
                        "Erase stroke"
                    });
                    if self.selected().is_none() {
                        let l = Layer::new(
                            "Paint layer",
                            self.history.document.width,
                            self.history.document.height,
                            Content::Paint,
                        );
                        let mut doc = self.history.document.clone();
                        doc.layers.push(l.clone());
                        self.history.preview(doc);
                        self.single_selection(Some(l.id));
                    }
                    let l = self.selected().unwrap().clone();
                    let stroke = Stroke {
                        mode: if self.tool == Tool::Brush {
                            StrokeMode::Paint
                        } else {
                            StrokeMode::Erase
                        },
                        color: self.color.clone(),
                        size: self.brush_size,
                        opacity: self.brush_opacity,
                        points: vec![geometry::to_local(&l, p).bounded()],
                    };
                    self.gesture = Some(Gesture::Stroke { layer: l, stroke });
                }
            } else {
                return Ok(());
            }
        }
        let Some(mut gesture) = self.gesture.take() else {
            return Ok(());
        };
        match &mut gesture {
            Gesture::Pan { origin, pan } => {
                self.viewport.pan = Point::new(pan.x + vp.x - origin.x, pan.y + vp.y - origin.y);
            }
            Gesture::Move { origin, layers } => {
                let moved = Self::moved(layers, Point::new(p.x - origin.x, p.y - origin.y));
                self.history.preview(self.with_layers(moved));
            }
            Gesture::Transform {
                origin,
                handle,
                handle_point,
                layer,
            } => {
                let mut next = layer.clone();
                if p != *origin {
                    next = if *handle == "rotate" {
                        geometry::rotate(layer, *origin, p, modifiers.shift)
                    } else {
                        geometry::resize(
                            layer,
                            handle,
                            Point::new(
                                handle_point.x + p.x - origin.x,
                                handle_point.y + p.y - origin.y,
                            ),
                            modifiers.shift,
                            modifiers.alt,
                        )
                    };
                }
                if next.validate().is_ok() {
                    self.history.preview(self.with_layers(vec![next]));
                }
            }
            Gesture::Shape { origin, layer } => {
                let end = Point::new(
                    p.x.clamp(0., self.history.document.width as f64),
                    p.y.clamp(0., self.history.document.height as f64),
                );
                let mut l = layer.clone();
                l.x = origin.x.min(end.x);
                l.y = origin.y.min(end.y);
                l.width = (end.x - origin.x).abs().round().max(1.) as u32;
                l.height = (end.y - origin.y).abs().round().max(1.) as u32;
                self.history.preview(self.with_layers(vec![l]));
            }
            Gesture::Stroke { layer, stroke } => {
                let local = geometry::to_local(layer, p).bounded();
                if local.distance(*stroke.points.last().unwrap()) > 0.5
                    && stroke.points.len() < 100000
                {
                    stroke.points.push(local);
                }
                let mut l = layer.clone();
                l.strokes.push(Arc::new(stroke.clone()));
                self.history.preview(self.with_layers(vec![l]));
            }
            Gesture::Mask { layer, stroke } => {
                let local = geometry::to_local(layer, p).bounded();
                if local.distance(*stroke.points.last().unwrap()) > 0.5
                    && stroke.points.len() < 100000
                {
                    stroke.points.push(local);
                }
                let mut l = layer.clone();
                Arc::make_mut(l.mask.as_mut().unwrap())
                    .strokes
                    .push(Arc::new(stroke.clone()));
                self.history.preview(self.with_layers(vec![l]));
            }
            Gesture::Crop { drag, .. } => {
                let ratio = self.crop_ratio.value(&self.history.document);
                let mut rect = drag.updated(p, ratio, modifiers.alt);
                let mut xs = vec![0., self.history.document.width as f64];
                let mut ys = vec![0., self.history.document.height as f64];
                for layer in self.history.document.layers.iter().filter(|l| l.visible) {
                    let corners = geometry::corners(layer);
                    xs.extend([
                        corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
                        corners
                            .iter()
                            .map(|p| p.x)
                            .fold(f64::NEG_INFINITY, f64::max),
                    ]);
                    ys.extend([
                        corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
                        corners
                            .iter()
                            .map(|p| p.y)
                            .fold(f64::NEG_INFINITY, f64::max),
                    ]);
                }
                rect = crop::snap(rect, *drag, p, ratio, (&xs, &ys), 6. / self.viewport.zoom);
                if rect.validate().is_ok() {
                    self.crop_rect = Some(rect);
                }
            }
            Gesture::MaskMove {
                origin,
                layer,
                placement,
            } => {
                let mut next = layer.clone();
                let mut mask = next.mask.as_ref().unwrap().as_ref().clone();
                let mut moved = *placement;
                moved.x += p.x - origin.x;
                moved.y += p.y - origin.y;
                if moved.valid() {
                    mask.placement = if moved == MaskPlacement::of(layer) {
                        None
                    } else {
                        Some(moved)
                    };
                    next.mask = Some(Arc::new(mask));
                    self.history.preview(self.with_layers(vec![next]));
                }
            }
            Gesture::PixelSelection { draft, before } => {
                draft.drag(p, modifiers.shift && draft.marquee.is_some());
                if phase == Phase::Up {
                    self.pixel_selection =
                        pixel_selection::finish(&self.history.document, before.as_ref(), draft)?;
                }
            }
            // Property drags are driven by slider commands, not the canvas pointer.
            Gesture::Property => {}
        }
        self.gesture = Some(gesture);
        if phase == Phase::Up {
            if matches!(self.gesture, Some(Gesture::Mask { .. })) {
                let mut layer = self.selected().expect("active mask layer").clone();
                let mut mask = layer.mask.as_ref().expect("active mask").as_ref().clone();
                mask.raster = Some(Arc::new(render::rasterize_mask(&mask, &layer)?));
                mask.strokes.clear();
                layer.mask = Some(Arc::new(mask));
                let mut doc = self.with_layers(vec![layer]);
                doc.version = 2;
                self.history.preview(doc);
            }
            self.finish_gesture();
        }
        Ok(())
    }
}
