//! EditorSession / SelectionClipboard / TransformDrag behavior translated from the pinned Compositor.
//! MIT © 2026 Wonder Assembly LLC. Existing multi-selection and v1 brush storage are adaptations.
use crate::{
    canvas_size::{CanvasSizeOptions, resize_canvas},
    geometry::{self, Viewport},
    history::{History, Selection},
    model::*,
};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
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
    gesture: Option<Gesture>,
}
impl Editor {
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
    fn single_selection(&mut self, id: Option<String>) {
        self.selection = Selection {
            ids: id.clone().into_iter().collect(),
            anchor: id,
        };
        self.paint_target = PaintTarget::Content;
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"document":self.history.document.metadata(),"history":self.history.info(),"selection":self.selection,"paintTarget":self.paint_target,"maskMode":self.mask_mode,"tool":self.tool,"color":self.color,"brushSize":self.brush_size,"brushOpacity":self.brush_opacity,"viewport":self.viewport})
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
        let mut doc = self.history.document.clone();
        doc.replace(next);
        self.edit("Edit layer", doc, None)
    }
    fn inserted(&self, layers: Vec<Layer>) -> Document {
        let mut doc = self.history.document.clone();
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
        self.edit(label, self.inserted(layers), Some(selection))
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
        for l in layers {
            doc.replace(l);
        }
        doc
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
            && !matches!(g, Gesture::Pan { .. })
        {
            self.end_edit();
        }
    }
    pub fn cancel_gesture(&mut self) {
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
                strokes: vec![],
            }))
        };
        let mut doc = self.history.document.clone();
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
            Command::Duplicate => {
                let mut doc = self.history.document.clone();
                let mut ids = vec![];
                let mut layers = vec![];
                for l in doc.layers {
                    layers.push(l.clone());
                    if self.selection.ids.contains(&l.id) {
                        let mut n = l;
                        n.id = id();
                        n.name = format!("{} copy", n.name.chars().take(190).collect::<String>());
                        n.locked = false;
                        ids.push(n.id.clone());
                        layers.push(n);
                    }
                }
                if !ids.is_empty() {
                    doc.layers = layers;
                    self.edit("Duplicate layers", doc, Some(ids))?;
                }
            }
            Command::Remove => {
                let mut doc = self.history.document.clone();
                doc.layers
                    .retain(|l| l.locked || !self.selection.ids.contains(&l.id));
                self.edit("Delete layers", doc, None)?;
            }
            Command::Reorder { direction } => {
                ensure!(
                    direction == 1 || direction == -1,
                    "Invalid reorder direction"
                );
                let mut doc = self.history.document.clone();
                let mut indices: Vec<_> = (0..doc.layers.len()).collect();
                if direction == 1 {
                    indices.reverse();
                }
                let moving: Vec<_> = self
                    .selected_layers()
                    .into_iter()
                    .filter(|l| !l.locked)
                    .map(|l| l.id)
                    .collect();
                for i in indices {
                    let other = i as isize + direction as isize;
                    if other >= 0
                        && (other as usize) < doc.layers.len()
                        && moving.contains(&doc.layers[i].id)
                        && !moving.contains(&doc.layers[other as usize].id)
                    {
                        doc.layers.swap(i, other as usize);
                    }
                }
                self.edit("Reorder layers", doc, None)?;
            }
            Command::ReorderTo { target_id, side } => {
                let moving: Vec<_> = self
                    .selected_layers()
                    .into_iter()
                    .filter(|l| !l.locked)
                    .collect();
                if moving.is_empty() || moving.iter().any(|l| l.id == target_id) {
                    return Ok(());
                }
                let mut doc = self.history.document.clone();
                doc.layers.retain(|l| !moving.iter().any(|v| v.id == l.id));
                if let Some(at) = doc.layers.iter().position(|l| l.id == target_id) {
                    let at = at + usize::from(matches!(side, Side::Above));
                    doc.layers.splice(at..at, moving);
                    self.edit("Reorder layers", doc, None)?;
                }
            }
            Command::Nudge { delta } => {
                ensure!(
                    delta.x.is_finite() && delta.y.is_finite(),
                    "Invalid movement"
                );
                let layers: Vec<_> = self
                    .selected_layers()
                    .into_iter()
                    .filter(|l| !l.locked && l.visible)
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
                    self.fit();
                }
            }
            Command::AddMask { base } => {
                if self.selection.ids.len() == 1
                    && self
                        .selected()
                        .is_some_and(|l| !l.locked && l.mask.is_none())
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
            if self.tool == Tool::Hand {
                self.gesture = Some(Gesture::Pan {
                    origin: vp,
                    pan: self.viewport.pan,
                });
                return Ok(());
            }
            if self.tool == Tool::Move {
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
        }
        self.gesture = Some(gesture);
        if phase == Phase::Up {
            self.finish_gesture();
        }
        Ok(())
    }
}
