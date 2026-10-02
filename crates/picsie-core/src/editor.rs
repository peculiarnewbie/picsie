//! EditorSession / SelectionClipboard / TransformDrag behavior translated from the pinned Compositor.
//! MIT © 2026 Wonder Assembly LLC. Multi-selection and legacy v1 stroke reading are adaptations.
mod interaction_polish;
mod layer_polish;
mod operations;
pub mod publication;
pub use group_transform::GroupOverlay;
pub use interaction_polish::TransformField;
mod adjustments;
mod group_transform;
pub use adjustments::AdjustmentEdit;
mod placement;
mod text;
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
pub use operations::PixelClipboard;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
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
    Line,
    Gradient,
    Text,
    Hand,
    Eyedropper,
    Crop,
    Marquee,
    Lasso,
    Wand,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum LassoKind {
    Freehand,
    Polygonal,
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
    SetSelectionAntialiased {
        antialiased: bool,
    },
    SetTransformRatio {
        locked: bool,
    },
    SetTransformField {
        field: TransformField,
        value: f64,
    },
    SelectLayerPixels,
    BeginDistort,
    DistortLayer {
        corners: [Point; 4],
    },
    SetDisplayScale {
        scale: f64,
    },
    SetTextCaretVisible {
        visible: bool,
    },
    TypeOpacityDigit {
        digit: u8,
    },
    StepBrushHardness {
        increase: bool,
    },
    SelectTextUnit {
        point: Point,
        paragraph: bool,
    },
    UpdateText {
        patch: crate::text::TextPatch,
    },
    CommitText,
    CancelText,
    SetTextSelection {
        anchor: usize,
        head: usize,
    },
    MoveTextCaret {
        direction: crate::text::TextNavigation,
        extend: bool,
    },
    SetViewOptions {
        options: crate::placement::ViewOptions,
    },
    BeginGuide {
        axis: crate::placement::GuideAxis,
        point: Point,
    },
    FinishGuide {
        delete: bool,
    },
    AddGuide {
        axis: crate::placement::GuideAxis,
        position: f64,
    },
    ClearGuides,
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
    SetPaletteColor {
        color: String,
        background: bool,
    },
    SetSampledForeground {
        color: String,
    },
    SwapPaletteColors,
    ResetPaletteColors,
    SetTextColor {
        color: String,
        draft_id: Option<String>,
    },
    PreviewBlendMode {
        id: Option<String>,
        mode: Option<Blend>,
    },
    CycleBlendMode {
        forward: bool,
    },
    BeginVisibilitySwipe {
        id: String,
    },
    SwipeVisibility {
        id: String,
    },
    EndVisibilitySwipe,
    CopyMask {
        source_id: String,
        target_id: String,
    },
    LoadThumbnailSelection {
        id: String,
        mask: bool,
        mode: PixelSelectionMode,
    },
    DuplicateTo {
        target_id: String,
        side: Side,
        into: bool,
    },
    SetMaskPlacement {
        placement: MaskPlacement,
    },
    DistortMask {
        corners: [Point; 4],
    },
    SetColor {
        color: String,
    },
    SetBrush {
        size: f64,
        opacity: f64,
    },
    SetBrushTip {
        hardness: f64,
        smoothing: f64,
    },
    SetShapeCornerRadius {
        radius: f64,
    },
    SetShapeLineWidth {
        width: f64,
    },
    CycleShapeKind,
    SetGradientShape {
        shape: crate::gradient::GradientShape,
    },
    SetGradientStyle {
        style: crate::gradient::GradientStyle,
    },
    SetGradientReverse {
        reversed: bool,
    },
    SetGradientOpacity {
        opacity: f64,
    },
    CommitGradient,
    CancelGradient,
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
    LayerViaCopy,
    MergeLayers,
    AddAdjustment {
        kind: crate::adjustment::AdjustmentKind,
    },
    BeginAdjustmentEdit {
        id: String,
    },
    UpdateAdjustmentLevels {
        settings: crate::adjustment::LevelsSettings,
        preview: bool,
    },
    UpdateAdjustmentCurves {
        settings: crate::adjustment::CurvesSettings,
        preview: bool,
    },
    SetAdjustmentPreview {
        preview: bool,
    },
    AutoLevels {
        mode: crate::adjustment::LevelsAuto,
    },
    SampleLevels {
        point: Point,
        mode: crate::adjustment::LevelsSample,
    },
    SetLevelsSampleMode {
        mode: Option<crate::adjustment::LevelsSample>,
    },
    CancelAdjustmentEdit,
    CommitAdjustmentEdit,
    BeginTransform,
    CommitTransform,
    CancelTransform,
    MoveSelection {
        delta: Point,
    },
    MovePixels {
        delta: Point,
        #[serde(default)]
        duplicate: bool,
    },
    SetLassoKind {
        kind: LassoKind,
    },
    FinishSelection,
    RemoveSelectionPoint,
    SetWand {
        settings: crate::wand::WandSettings,
    },
    ResizeImage {
        options: crate::image_size::ImageSizeOptions,
    },
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
    CancelPixelSelection,
    SelectAllPixels,
    ClearSelectedPixels,
    FillSelection,
    FillBackground,
    InvertSelection,
    ExpandSelection {
        amount: u32,
    },
    ContractSelection {
        amount: u32,
    },
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
    ToggleMaskLinkFor {
        id: String,
    },
    ToggleClippingMask,
    ToggleClippingFor {
        id: String,
    },
    LinkMask {
        #[serde(rename = "sourceId")]
        source_id: String,
    },
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
    pub can_toggle_clipping: bool,
}
enum Gesture {
    TextBox {
        origin: Point,
        current: Point,
    },
    TextResize {
        origin: Point,
        handle: &'static str,
        layer: Layer,
    },
    SelectionMove {
        origin: Point,
        selection: PixelSelection,
    },
    PixelMove {
        origin: Point,
        layer: Layer,
    },
    Pan {
        origin: Point,
        pan: Point,
    },
    Move {
        origin: Point,
        layers: Vec<Layer>,
    },
    GroupTransform {
        origin: Point,
        handle: &'static str,
        handle_point: Point,
        drag: Box<group_transform::GroupDrag>,
    },
    /// A slider drag in flight: property updates preview into one undo step.
    Property,
    Transform {
        origin: Point,
        handle: &'static str,
        handle_point: Point,
        layer: Layer,
        mask: bool,
    },
    Shape {
        origin: Point,
        layer: Layer,
    },
    Brush {
        stroke: Box<crate::brush::BrushStroke>,
    },
    Crop {
        drag: CropDrag,
        before: Option<CropRect>,
    },
    MaskDistort {
        origin: Point,
        handle: &'static str,
        corners: [Point; 4],
        layer: Layer,
        free: bool,
        mask: bool,
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
    pub(super) blend_preview: Option<(String, Blend)>,
    image_distortion: Option<interaction_polish::ImageDistortion>,
    pub(super) mask_distortion: Option<layer_polish::MaskDistortion>,
    visibility_swipe: Option<bool>,
    pub background_color: String,
    pending_opacity_digit: Option<(u8, std::time::Instant)>,
    pub display_scale: f64,
    pub text_caret_visible: bool,
    text_mouse_unit: Option<(usize, usize, bool)>,
    text_session: Option<text::TextSession>,
    pub text_selection: (usize, usize),
    text_upstream: bool,
    text_vertical_x: Option<f32>,
    pub text_defaults: Layer,
    pub view_options: crate::placement::ViewOptions,
    pub snap_lines: (Vec<f64>, Vec<f64>),
    guide_drag: Option<crate::placement::GuideDrag>,
    pub history: History,
    pub selection: Selection,
    pub paint_target: PaintTarget,
    pub mask_mode: MaskMode,
    pub tool: Tool,
    pub color: String,
    pub brush_size: f64,
    pub brush_opacity: f64,
    pub brush_hardness: f64,
    pub brush_smoothing: f64,
    last_brush_point: Option<(String, bool, Point)>,
    pub viewport: Viewport,
    pub crop_rect: Option<CropRect>,
    pub crop_ratio: CropRatio,
    pub collapsed_groups: HashSet<String>,
    pub marquee_kind: MarqueeKind,
    pub selection_mode: PixelSelectionMode,
    pub selection_antialiased: bool,
    pub locks_transform_ratio: bool,
    pub lasso_kind: LassoKind,
    pub wand: crate::wand::WandSettings,
    polygon: Option<SelectionDraft>,
    polygon_cursor: Option<Point>,
    floating: Option<operations::Floating>,
    pub adjustment_edit: Option<AdjustmentEdit>,
    layer_transform: bool,
    transform_pixel_size: Option<(f64, f64)>,
    group_transform: Option<group_transform::GroupState>,
    group_cache: RefCell<group_transform::GroupCache>,
    /// Shape tool defaults. ShapeControls.swift: rounded-rectangle radius in document
    /// pixels (0 keeps corners square) and line thickness; the tool draws no
    /// fill/stroke enable switches. Compositor 609dbeae. MIT © 2026 Wonder Assembly LLC.
    pub shape_corner_radius: f64,
    pub shape_line_width: f64,
    /// Gradient tool settings and the pending line edit, if any.
    pub gradient_settings: crate::gradient::GradientSettings,
    pub gradient_edit: Option<crate::gradient::GradientEdit>,
    gradient_handle: Option<crate::gradient::GradientHandle>,
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
        if let Some(draft) = &self.polygon {
            let mut draft = draft.clone();
            if let Some(cursor) = self.polygon_cursor {
                draft.points.push(cursor);
            }
            return Some(draft);
        }
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
            selection_antialiased: true,
            locks_transform_ratio: false,
            blend_preview: None,
            mask_distortion: None,
            image_distortion: None,
            visibility_swipe: None,
            background_color: "#ffffff".into(),
            pending_opacity_digit: None,
            display_scale: 1.,
            text_caret_visible: true,
            text_mouse_unit: None,
            text_session: None,
            text_selection: (0, 0),
            text_upstream: false,
            text_vertical_x: None,
            text_defaults: text::defaults(),
            view_options: Default::default(),
            snap_lines: (vec![], vec![]),
            guide_drag: None,
            history: History::new(document),
            selection: Selection {
                ids: last.clone().into_iter().collect(),
                anchor: last,
            },
            paint_target: PaintTarget::Content,
            mask_mode: MaskMode::Hide,
            tool: Tool::Move,
            color: "#000000".into(),
            brush_size: 40.,
            brush_opacity: 1.,
            brush_hardness: 1.,
            brush_smoothing: 0.,
            last_brush_point: None,
            viewport: Viewport::default(),
            crop_rect: None,
            crop_ratio: CropRatio::Free,
            shape_corner_radius: 0.,
            shape_line_width: 4.,
            gradient_settings: crate::gradient::GradientSettings::default(),
            gradient_edit: None,
            gradient_handle: None,
            collapsed_groups: HashSet::new(),
            marquee_kind: MarqueeKind::Rectangle,
            selection_mode: PixelSelectionMode::Replace,
            lasso_kind: LassoKind::Freehand,
            wand: crate::wand::WandSettings::default(),
            polygon: None,
            polygon_cursor: None,
            floating: None,
            adjustment_edit: None,
            layer_transform: false,
            transform_pixel_size: None,
            group_transform: None,
            group_cache: RefCell::new(group_transform::GroupCache::default()),
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
    pub fn can_edit_pixels(&self) -> bool {
        self.selection.ids.len() == 1
            && self
                .history
                .pixel_selection
                .as_ref()
                .is_none_or(|s| s.bounds.is_some())
            && self.selected().is_some_and(|l| {
                l.adjustment.is_none()
                    && !l.locked
                    && self.history.document.effective(l).0
                    && if self.paint_target == PaintTarget::Mask {
                        l.mask.as_ref().is_some_and(|m| m.enabled)
                    } else {
                        !matches!(l.content.as_ref(), Content::Group)
                    }
            })
    }
    pub fn snapshot(&self) -> serde_json::Value {
        let index = crate::layer_index::LayerIndex::new(&self.history.document);
        let mut state = serde_json::json!({"document":self.history.document.metadata(),"history":self.history.info(),"selection":self.selection,"paintTarget":self.paint_target,"maskMode":self.mask_mode,"tool":self.tool,"color":self.color,"brushSize":self.brush_size,"brushOpacity":self.brush_opacity,"brushHardness":self.brush_hardness,"brushSmoothing":self.brush_smoothing,"viewport":self.viewport,"cropRect":self.crop_rect,"cropRatio":self.crop_ratio,"layerRows":self.layer_rows_indexed(&index),"maskSourceIds":self.history.document.layers.iter().filter(|l| self.selected_id().is_some_and(|target| index.can_link(&l.id, target))).map(|l| &l.id).collect::<Vec<_>>(),"canEditPixels":self.can_edit_pixels(),"canToggleClipping": self.selection.ids.len() == 1 && self.selected().is_some_and(|l| !l.locked && (l.mask_source_id.is_some() || index.clipping_source(&l.id).is_some())),"hasPixelSelection": self.history.pixel_selection.is_some(),
            "pixelSelectionBounds":self.history.pixel_selection.as_ref().and_then(|v|v.bounds.clone()),"pixelSelectionFeather":self.history.pixel_selection.as_ref().map(|v|v.feather),"marqueeKind":self.marquee_kind,"selectionMode":self.selection_mode,"textEditRequests":self.text_edit_requests,"lassoKind":self.lasso_kind,"wand":self.wand,"selectionDraft":self.polygon.is_some(),"transformActive":self.floating.is_some() || self.layer_transform,"canCopyPixels":self.can_copy_pixels(),"viewOptions":self.view_options,"displayedGuides":self.displayed_guides(),"guideDragActive":self.guide_drag.is_some(),"snapLines":{"xs":self.snap_lines.0,"ys":self.snap_lines.1},"textEditing":self.text_editing(),"textCaretVisible":self.text_caret_visible,"cursorMap":self.cursor_map(),"textSelection":{"anchor":self.text_selection.0,"head":self.text_selection.1},"currentText":self.current_text().metadata(),"textCaret":if self.text_editing() {self.selected().map(|l| {let c=crate::text::caret_with_affinity(l,self.text_selection.1.min(match l.content.as_ref(){Content::Text{text,..}=>text.len(),_=>0}),self.text_upstream);geometry::to_world(l,Point::new(c.left as f64,c.top as f64))})} else {None}});
        state["blendPreview"] = serde_json::to_value(&self.blend_preview).unwrap();
        state["canModifySelection"] = self.can_modify_selection().into();
        state["selectionEmpty"] = self
            .history
            .pixel_selection
            .as_ref()
            .is_some_and(|s| s.outline.is_empty())
            .into();
        state["selectionAntialiased"] = self.selection_antialiased.into();
        state["locksTransformRatio"] = self.locks_transform_ratio.into();
        state["transformScalePercent"] = self.transform_scale_percent().into();
        state["selectionDraftMode"] = self
            .selection_draft()
            .map(|d| serde_json::to_value(d.mode).unwrap())
            .unwrap_or(serde_json::Value::Null);
        state["backgroundColor"] = self.background_color.clone().into();
        state["shapeCornerRadius"] = self.shape_corner_radius.into();
        state["shapeLineWidth"] = self.shape_line_width.into();
        state["gradientSettings"] = serde_json::to_value(&self.gradient_settings).unwrap();
        state["gradientPending"] = self.gradient_edit.is_some().into();
        state["gradientLine"] =
            serde_json::to_value(self.gradient_edit.as_ref().and_then(|edit| {
                edit.has_line()
                    .then(|| serde_json::json!({"start": edit.start, "end": edit.end}))
            }))
            .unwrap();
        state["imageDistortion"] =
            serde_json::to_value(self.image_distortion.as_ref().map(|d| d.corners)).unwrap();
        state["maskDistortion"] = serde_json::to_value(self.mask_distortion_corners()).unwrap();
        state["transformTarget"] = self
            .independent_mask_layer()
            .or_else(|| self.selected().cloned())
            .map(|l| l.metadata())
            .unwrap_or(serde_json::Value::Null);
        state
    }
    fn layer_rows(&self) -> Vec<LayerRow> {
        self.layer_rows_indexed(&crate::layer_index::LayerIndex::new(&self.history.document))
    }
    fn layer_rows_indexed(&self, index: &crate::layer_index::LayerIndex<'_>) -> Vec<LayerRow> {
        fn visit(
            index: &crate::layer_index::LayerIndex<'_>,
            parent: Option<&str>,
            depth: u32,
            visible: bool,
            collapsed: &HashSet<String>,
            out: &mut Vec<LayerRow>,
        ) {
            for &i in index.siblings(parent).iter().rev() {
                let layer = &index.document.layers[i];
                let effective = visible && layer.visible;
                out.push(LayerRow {
                    id: layer.id.clone(),
                    depth,
                    visible: effective,
                    collapsed: collapsed.contains(&layer.id),
                    can_toggle_clipping: !layer.locked
                        && !matches!(layer.content.as_ref(), Content::Group)
                        && (layer.mask_source_id.is_some()
                            || index.clipping_source(&layer.id).is_some()),
                });
                if matches!(layer.content.as_ref(), Content::Group)
                    && !collapsed.contains(&layer.id)
                {
                    visit(index, Some(&layer.id), depth + 1, effective, collapsed, out);
                }
            }
        }
        let mut rows = Vec::new();
        visit(index, None, 0, true, &self.collapsed_groups, &mut rows);
        rows
    }
    pub fn select(&mut self, id: Option<String>, mode: SelectionMode) -> Result<()> {
        // Switching layers applies the pending gradient, as in Photoshop; reselecting
        // the gradient's own layer keeps it pending.
        if self.gradient_edit.is_some()
            && !(matches!(mode, SelectionMode::Replace)
                && id.as_deref()
                    == self
                        .gradient_edit
                        .as_ref()
                        .map(|edit| edit.layer_id.as_str()))
        {
            self.resolve_gradient()?;
        }
        self.finish_gesture()?;
        let Some(id) = id.filter(|id| self.history.document.layers.iter().any(|l| l.id == *id))
        else {
            self.single_selection(None);
            return Ok(());
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
                let rows = self.layer_rows();
                let anchor = rows
                    .iter()
                    .position(|l| Some(&l.id) == self.selection.anchor.as_ref());
                let end = rows.iter().position(|l| l.id == id);
                if let (Some(anchor), Some(end)) = (anchor, end) {
                    let span = &rows[anchor.min(end)..=anchor.max(end)];
                    self.selection.ids = self
                        .history
                        .document
                        .layers
                        .iter()
                        .filter(|l| l.id != id && span.iter().any(|row| row.id == l.id))
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
        Ok(())
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
    /// `Selection.swift::setSelection`: selection edits use normal document transactions.
    fn set_pixel_selection(
        &mut self,
        selection: Option<PixelSelection>,
        label: &str,
    ) -> Result<()> {
        self.finish_gesture()?;
        if self.history.pixel_selection == selection {
            return Ok(());
        }
        self.begin_edit(label);
        self.history.pixel_selection = selection;
        self.end_edit();
        Ok(())
    }
    /// Canvas replacement and the existing selection reset must undo together. Used by
    /// both synchronous commands and the native asynchronous resize completion.
    pub fn edit_canvas(&mut self, label: &str, doc: Document) -> Result<()> {
        doc.validate()?;
        self.finish_gesture()?;
        self.begin_edit(label);
        self.history.preview(doc);
        self.history.pixel_selection = None;
        self.reconcile();
        self.end_edit();
        self.fit();
        Ok(())
    }
    pub fn edit(
        &mut self,
        label: &str,
        doc: Document,
        selection: Option<Vec<String>>,
    ) -> Result<()> {
        doc.validate()?;
        self.finish_gesture()?;
        let mut doc = doc;
        // An immediate commit redraws scaled shapes full; a nested preview
        // (persistent transform, property drag) keeps bounded renderer previews
        // until its own commit.
        if self.history.before_document().is_none() {
            // One indexed pass over the incoming layers; the current document is
            // looked up once and only changed eligible shape layers are redrawn.
            let current: std::collections::HashMap<&str, &Layer> = self
                .history
                .document
                .layers
                .iter()
                .map(|layer| (layer.id.as_str(), layer))
                .collect();
            let mut baked_version = false;
            for layer in &mut doc.layers {
                if !matches!(layer.content.as_ref(), Content::Shape { .. }) {
                    continue;
                }
                if (layer.width as f64 * layer.scale_x).round() == layer.width as f64
                    && (layer.height as f64 * layer.scale_y).round() == layer.height as f64
                    && layer.scale_x == 1.
                    && layer.scale_y == 1.
                {
                    continue;
                }
                let Some(old) = current.get(layer.id.as_str()) else {
                    continue;
                };
                let baked_strokes = old
                    .mask
                    .as_ref()
                    .is_some_and(|mask| !mask.strokes.is_empty());
                if let Some(redrawn) = crate::shape::redraw_at_displayed_size(layer) {
                    baked_version |= baked_strokes
                        && redrawn
                            .mask
                            .as_ref()
                            .is_some_and(|mask| mask.strokes.is_empty());
                    *layer = redrawn;
                }
            }
            if baked_version {
                doc.version = 2;
            }
        }
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
        if let Some(mut target) = self.independent_mask_layer()
            && let Some(properties) = patch.as_object()
            && !properties.is_empty()
            && properties.keys().all(|k| {
                [
                    "x", "y", "scaleX", "scaleY", "rotation", "flipX", "flipY", "sampling",
                ]
                .contains(&k.as_str())
            })
        {
            let mut value = serde_json::to_value(MaskPlacement::of(&target))?;
            for (key, v) in properties {
                value[key] = v.clone();
            }
            let placement: MaskPlacement = serde_json::from_value(value)?;
            target.x = placement.x;
            self.layer_polish_command(&Command::SetMaskPlacement { placement })?;
            return Ok(());
        }
        if patch.get("blend").is_some() {
            self.blend_preview = None;
        }
        // Group Flip H/V reuses the single-layer patch contract: a flip-only patch
        // mirrors every member across the box middle as one undo (`flipLayers`).
        if self.paint_target == PaintTarget::Content
            && self.independent_mask_layer().is_none()
            && let Some(properties) = patch.as_object()
            && !properties.is_empty()
            && properties
                .keys()
                .all(|k| ["flipX", "flipY"].contains(&k.as_str()))
            && self.cached_group_box().is_some()
        {
            self.flip_group(properties.contains_key("flipX"))?;
            return Ok(());
        }
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
                    .all(|k| ["name", "visible", "locked", "opacity", "mask"].contains(&k.as_str())),
                "Folders support only name, visibility, lock, and opacity"
            );
        }
        if layer.adjustment.is_some() {
            // Adjustment layers keep document geometry and prototype appearance;
            // only identity, visibility, lock, opacity, blend and masks change.
            ensure!(
                patch.keys().all(|k| {
                    ["name", "visible", "locked", "opacity", "blend", "mask"].contains(&k.as_str())
                }),
                "Adjustment layers support only name, visibility, lock, opacity, blend, and mask"
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
            self.history.document.layers.len() + layers.len() <= crate::model::MAX_LAYERS,
            "The editor supports up to 10,000 layers"
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
    pub fn finish_gesture(&mut self) -> Result<()> {
        let Some(mut gesture) = self.gesture.take() else {
            return Ok(());
        };
        if let Gesture::Brush { stroke } = &mut gesture
            && !stroke.is_finished()
        {
            let result = stroke.finish().and_then(|()| stroke.snapshot());
            match result {
                Ok(layer) => {
                    let mut doc = self.history.document.clone();
                    if layer.mask.is_some() {
                        doc.version = 2;
                    }
                    doc.replace(layer);
                    self.history.preview(doc);
                }
                Err(error) => {
                    self.cancel_gesture();
                    return Err(error);
                }
            }
        }
        if !matches!(
            gesture,
            Gesture::Pan { .. }
                | Gesture::Crop { .. }
                | Gesture::PixelSelection { .. }
                | Gesture::PixelMove { .. }
                | Gesture::TextResize { .. }
                | Gesture::TextBox { .. }
        ) {
            self.redraw_committed_shapes();
            self.end_edit();
        }
        self.snap_lines = (vec![], vec![]);
        Ok(())
    }
    pub fn cancel_gesture(&mut self) {
        if self.text_editing() {
            self.cancel_text();
            return;
        }
        self.snap_lines = (vec![], vec![]);
        if self.guide_drag.take().is_some() {
            return;
        }
        if self.floating.is_some() || self.layer_transform {
            self.cancel_transform();
            return;
        }
        if self.polygon.take().is_some() {
            self.polygon_cursor = None;
            return;
        }
        if matches!(self.gesture, Some(Gesture::PixelSelection { .. })) {
            if let Some(Gesture::PixelSelection { before, .. }) = self.gesture.take() {
                self.history.pixel_selection = before;
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
    /// EditorSession.nextShapeName: "Rectangle 1", "Ellipse 2", … skipping names in use.
    fn next_shape_name(&self, kind: &str) -> String {
        let names: std::collections::HashSet<&str> = self
            .history
            .document
            .layers
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        let mut number = 1;
        loop {
            let candidate = format!("{kind} {number}");
            if !names.contains(candidate.as_str()) {
                return candidate;
            }
            number += 1;
        }
    }
    fn shape_kind(&self) -> Shape {
        match self.tool {
            Tool::Ellipse => Shape::Ellipse,
            Tool::Line => Shape::Line,
            _ => Shape::Rectangle,
        }
    }
    /// ShapeTool.toggleShapeKind: Shift-U (and Tab) steps Rectangle, Ellipse, Line.
    /// A drag in flight is discarded, as upstream cancels the shape first.
    fn cycle_shape_kind(&mut self) {
        if matches!(self.gesture, Some(Gesture::Shape { .. })) {
            self.cancel_gesture();
        }
        self.tool = match self.tool {
            Tool::Rectangle => Tool::Ellipse,
            Tool::Ellipse => Tool::Line,
            Tool::Line => Tool::Rectangle,
            _ => Tool::Rectangle,
        };
    }
    /// Gradient.swift: the selected layer or mask takes a gradient when it could be
    /// painted (single unlocked visible target; an explicit empty selection blocks).
    /// A pending line on the same target restarts, as beginGradient replaces it.
    fn begin_gradient(&mut self, at: Point) -> Result<()> {
        if self.tool != Tool::Gradient || !at.x.is_finite() || !at.y.is_finite() {
            return Ok(());
        }
        if !self.can_edit_pixels() {
            return Ok(());
        }
        let mask = self.paint_target == PaintTarget::Mask;
        let layer = self.selected().cloned().unwrap();
        // A pending line on another target is replaced, as upstream starts a new edit.
        self.cancel_gradient();
        let base_image = crate::gradient::rasterize_base(&layer, mask)?;
        self.begin_edit(if mask { "Gradient Mask" } else { "Gradient" });
        self.gradient_edit = Some(crate::gradient::GradientEdit {
            layer_id: layer.id.clone(),
            mask,
            start: at,
            end: at,
            base: layer,
            base_image,
        });
        self.gradient_handle = Some(crate::gradient::GradientHandle::End);
        self.refresh_gradient()
    }
    /// EditorCanvas.beginGradientDrag: grabbing a pending endpoint (within ten
    /// screen points) moves just that end; anywhere else starts a new line.
    fn begin_gradient_drag(&mut self, at: Point) -> Result<()> {
        let grab = match &self.gradient_edit {
            Some(edit)
                if edit.has_line()
                    && edit.mask == (self.paint_target == PaintTarget::Mask)
                    && Some(edit.layer_id.as_str()) == self.selected_id() =>
            {
                let threshold = 10. * self.display_scale / self.viewport.zoom.max(0.01);
                if edit.end.distance(at) <= threshold {
                    Some(crate::gradient::GradientHandle::End)
                } else if edit.start.distance(at) <= threshold {
                    Some(crate::gradient::GradientHandle::Start)
                } else {
                    None
                }
            }
            _ => None,
        };
        match grab {
            Some(handle) => {
                self.gradient_handle = Some(handle);
                Ok(())
            }
            None => self.begin_gradient(at),
        }
    }
    /// Re-renders the pending gradient from its endpoints, settings, and palette.
    /// Redrawing restarts from the base pixels, so moving the line never accumulates.
    fn refresh_gradient(&mut self) -> Result<()> {
        let Some(edit) = self.gradient_edit.clone() else {
            return Ok(());
        };
        let mut doc = self.history.document.clone();
        if edit.has_line() {
            let layer = crate::gradient::fill_layer(
                &edit,
                &self.history.document,
                self.history.pixel_selection.as_ref(),
                &self.gradient_settings,
                &self.color,
                &self.background_color,
            )?;
            doc.replace(layer);
        } else {
            doc.replace(edit.base.clone());
        }
        self.history.preview(doc);
        Ok(())
    }
    fn move_gradient(&mut self, mut at: Point, square: bool) -> Result<()> {
        let (Some(edit), Some(handle)) = (&mut self.gradient_edit, self.gradient_handle) else {
            return Ok(());
        };
        if !at.x.is_finite() || !at.y.is_finite() {
            return Ok(());
        }
        // Shift snaps the line to eighths of a turn around the other end.
        if square {
            let anchor = match handle {
                crate::gradient::GradientHandle::Start => edit.end,
                crate::gradient::GradientHandle::End => edit.start,
            };
            at = crate::shape::snap_line_end(anchor, at);
        }
        match handle {
            crate::gradient::GradientHandle::Start => edit.start = at,
            crate::gradient::GradientHandle::End => edit.end = at,
        }
        self.refresh_gradient()
    }
    /// Ends a drag; a click without a line leaves nothing pending.
    fn end_gradient_drag(&mut self) {
        self.gradient_handle = None;
        if self
            .gradient_edit
            .as_ref()
            .is_none_or(|edit| !edit.has_line())
        {
            self.cancel_gradient();
        }
    }
    /// Document without the pending gradient preview, for save and export: the
    /// preview must not leak into committed files or history until Apply.
    pub fn gradient_base_document(&self) -> Option<Document> {
        let edit = self.gradient_edit.as_ref()?;
        let mut doc = self.history.document.clone();
        doc.replace(edit.base.clone());
        Some(doc)
    }
    fn cancel_gradient(&mut self) {
        if self.gradient_edit.is_none() {
            return;
        }
        self.gradient_edit = None;
        self.gradient_handle = None;
        let selection = self.history.cancel();
        self.restore_selection(selection);
    }
    /// redrawShape at commit: scaled shape layers redraw at full displayed size
    /// as part of the committing edit, preserving document-pixel radius/width
    /// and mask footprints. Draft previews keep original geometry and rely on
    /// the bounded renderer preview instead. Shared hook for the group
    /// transform work at final integration.
    fn redraw_committed_shapes(&mut self) {
        // One in-place pass; only scaled shape layers clone (Arc contents shared),
        // untouched layers are never touched.
        let mut baked_version = false;
        {
            let layers = &mut self.history.document.layers;
            let mut index = 0;
            while index < layers.len() {
                let eligible = matches!(layers[index].content.as_ref(), Content::Shape { .. })
                    && ((layers[index].width as f64 * layers[index].scale_x).round()
                        != layers[index].width as f64
                        || (layers[index].height as f64 * layers[index].scale_y).round()
                            != layers[index].height as f64
                        || layers[index].scale_x != 1.
                        || layers[index].scale_y != 1.);
                if eligible {
                    let current = layers[index].clone();
                    let baked_strokes = current
                        .mask
                        .as_ref()
                        .is_some_and(|mask| !mask.strokes.is_empty());
                    if let Some(redrawn) = crate::shape::redraw_at_displayed_size(&current) {
                        baked_version |= baked_strokes
                            && redrawn
                                .mask
                                .as_ref()
                                .is_some_and(|mask| mask.strokes.is_empty());
                        layers[index] = redrawn;
                    }
                }
                index += 1;
            }
        }
        if baked_version {
            self.history.document.version = 2;
        }
    }
    fn commit_gradient(&mut self) -> Result<()> {
        let Some(edit) = self.gradient_edit.clone() else {
            return Ok(());
        };
        if !edit.has_line() {
            self.cancel_gradient();
            return Ok(());
        }
        let layer = crate::gradient::fill_layer(
            &edit,
            &self.history.document,
            self.history.pixel_selection.as_ref(),
            &self.gradient_settings,
            &self.color,
            &self.background_color,
        )?;
        let mut doc = self.history.document.clone();
        if edit.mask {
            doc.version = 2;
        }
        doc.replace(layer);
        self.history.preview(doc);
        self.gradient_edit = None;
        self.gradient_handle = None;
        self.end_edit();
        Ok(())
    }
    /// Switching tools, layers, or targets applies the pending gradient, as in Photoshop.
    fn resolve_gradient(&mut self) -> Result<()> {
        if self.gradient_edit.is_some() {
            self.commit_gradient()?;
        }
        Ok(())
    }
    pub fn command(&mut self, command: Command) -> Result<()> {
        if self.layer_polish_command(&command)? {
            return Ok(());
        }
        if self.visibility_swipe.is_some() {
            self.end_visibility_swipe();
        }
        // EditorSession+Brush: opacity/hardness keys do nothing during a stroke.
        if let Command::TypeOpacityDigit { digit } = command {
            ensure!(digit <= 9, "Invalid opacity digit");
            if !matches!(self.gesture, Some(Gesture::Brush { .. }))
                && matches!(
                    self.tool,
                    Tool::Move | Tool::Brush | Tool::Eraser | Tool::Gradient
                )
            {
                let now = std::time::Instant::now();
                let percent = if let Some((previous, time)) = self.pending_opacity_digit
                    && now.duration_since(time).as_secs_f64() < 0.6
                {
                    self.pending_opacity_digit = None;
                    (previous as u32 * 10 + digit as u32).max(1)
                } else {
                    self.pending_opacity_digit = Some((digit, now));
                    if digit == 0 { 100 } else { digit as u32 * 10 }
                };
                let opacity = percent as f64 / 100.;
                if matches!(self.tool, Tool::Brush | Tool::Eraser) {
                    self.brush_opacity = opacity;
                } else if self.tool == Tool::Gradient {
                    // Gradient.typeOpacityDigit: digits drive the gradient opacity.
                    self.gradient_settings.opacity = opacity;
                    self.refresh_gradient()?;
                } else {
                    let mut doc = self.history.document.clone();
                    for layer in &mut doc.layers {
                        if self.selection.ids.contains(&layer.id) && !layer.locked {
                            layer.opacity = opacity;
                        }
                    }
                    self.edit("Layer opacity", doc, None)?;
                }
            }
            return Ok(());
        }
        if let Command::StepBrushHardness { increase } = command {
            if matches!(self.tool, Tool::Brush | Tool::Eraser)
                && !matches!(self.gesture, Some(Gesture::Brush { .. }))
            {
                let quarter = self.brush_hardness * 4.;
                let step = if increase {
                    (quarter + 0.001).floor() + 1.
                } else {
                    (quarter - 0.001).ceil() - 1.
                };
                self.brush_hardness = step.clamp(0., 4.) / 4.;
            }
            return Ok(());
        }
        if let Command::SetSelectionAntialiased { antialiased } = command {
            self.selection_antialiased = antialiased;
            return Ok(());
        }
        if let Command::SetTransformRatio { locked } = command {
            self.locks_transform_ratio = locked;
            return Ok(());
        }
        // Presentation updates must never finish a later gesture when a blink was queued.
        if let Command::SetTextCaretVisible { visible } = command {
            self.text_caret_visible = visible;
            return Ok(());
        }
        // Keep the retained QuickGUI content-property contract working on the same
        // Rust draft. The native UI uses UpdateText directly.
        let text_property = self.text_editing()
            && matches!(&command, Command::UpdateLayer { patch } if patch.as_object().is_some_and(|p| p.len() == 1 && p.get("content").is_some_and(|c| c["kind"] == "text")));
        if self.text_editing()
            && !text_property
            && !matches!(&command, Command::BeginPropertyEdit { label } if label == "Edit text")
            && !matches!(
                &command,
                Command::UpdateText { .. }
                    | Command::SelectTextUnit { .. }
                    | Command::SetTextCaretVisible { .. }
                    | Command::SetTextSelection { .. }
                    | Command::MoveTextCaret { .. }
                    | Command::CommitText
                    | Command::CancelText
                    | Command::EditText { .. }
                    | Command::Pointer { .. }
                    | Command::Fit
                    | Command::Zoom { .. }
                    | Command::ResizeViewport { .. }
                    | Command::SetDisplayScale { .. }
                    | Command::Pan { .. }
                    | Command::SetViewport { .. }
                    | Command::SetViewOptions { .. }
                    | Command::FinishGesture
                    | Command::CancelGesture
            )
        {
            self.finish_text()?;
        }

        if matches!(self.gesture, Some(Gesture::Brush { .. }))
            && !matches!(
                command,
                Command::Pointer { .. } | Command::Undo | Command::Redo | Command::CancelGesture
            )
        {
            self.finish_gesture()?;
        }
        if (self.floating.is_some() || self.layer_transform)
            && !matches!(
                &command,
                Command::Pointer { .. }
                    | Command::Nudge { .. }
                    | Command::UpdateLayer { .. }
                    | Command::BeginDistort
                    | Command::DistortLayer { .. }
                    | Command::SetTransformField { .. }
                    | Command::SetTransformRatio { .. }
                    | Command::BeginPropertyEdit { .. }
                    | Command::FinishGesture
                    | Command::CancelGesture
                    | Command::CommitTransform
                    | Command::CancelTransform
                    | Command::BeginTransform
                    | Command::Undo
                    | Command::Redo
                    | Command::Zoom { .. }
                    | Command::ResizeViewport { .. }
                    | Command::SetDisplayScale { .. }
                    | Command::Pan { .. }
                    | Command::SetViewport { .. }
                    | Command::Fit
            )
        {
            self.commit_transform()?;
        }
        // A pending gradient previews inside its own history transaction. Any other
        // edit commits it first (Gradient.resolveGradient); palette and navigation
        // commands only refresh the preview or leave it alone.
        if self.gradient_edit.is_some()
            && !matches!(
                &command,
                Command::Pointer { .. }
                    | Command::SetGradientShape { .. }
                    | Command::SetGradientStyle { .. }
                    | Command::SetGradientReverse { .. }
                    | Command::SetGradientOpacity { .. }
                    | Command::CommitGradient
                    | Command::CancelGradient
                    | Command::FinishGesture
                    | Command::CancelGesture
                    | Command::Undo
                    | Command::Redo
                    | Command::Select { .. }
                    | Command::SetTool { .. }
                    | Command::SetPaintTarget { .. }
                    | Command::BeginTransform
                    | Command::SetColor { .. }
                    | Command::SetSampledForeground { .. }
                    | Command::SwapPaletteColors
                    | Command::ResetPaletteColors
                    | Command::SetBrush { .. }
                    | Command::SetBrushTip { .. }
                    | Command::TypeOpacityDigit { .. }
                    | Command::StepBrushHardness { .. }
                    | Command::Zoom { .. }
                    | Command::Pan { .. }
                    | Command::SetViewport { .. }
                    | Command::ResizeViewport { .. }
                    | Command::SetDisplayScale { .. }
                    | Command::Fit
            )
        {
            self.resolve_gradient()?;
        }
        match command {
            Command::BeginDistort => {
                if self.selected().is_some_and(|l| l.adjustment.is_some())
                    && self.independent_mask_layer().is_none()
                {
                    return Ok(());
                }
                if self.independent_mask_layer().is_some() {
                    self.begin_mask_distortion()?;
                } else {
                    self.begin_image_distortion()?;
                }
            }
            Command::DistortLayer { corners } => {
                self.begin_image_distortion()?;
                if crate::distort::usable(&corners) {
                    self.preview_image_distortion(corners, Some(2048.))?;
                }
            }
            Command::SetSelectionAntialiased { antialiased } => {
                self.selection_antialiased = antialiased
            }
            Command::SetTransformRatio { locked } => self.locks_transform_ratio = locked,
            Command::SetTransformField { field, value } => {
                if self
                    .selected()
                    .is_some_and(|l| l.adjustment.is_some() && !l.locked)
                    && self.independent_mask_layer().is_none()
                {
                    return Ok(());
                }
                self.set_transform_field(field, value)?
            }
            Command::SelectLayerPixels => {
                if let Some(id) = self.selected_id().map(str::to_owned) {
                    self.layer_polish_command(&Command::LoadThumbnailSelection {
                        id,
                        mask: false,
                        mode: PixelSelectionMode::Replace,
                    })?;
                }
            }
            Command::SetPaletteColor { .. }
            | Command::SetSampledForeground { .. }
            | Command::SwapPaletteColors
            | Command::ResetPaletteColors
            | Command::SetTextColor { .. }
            | Command::PreviewBlendMode { .. }
            | Command::CycleBlendMode { .. }
            | Command::BeginVisibilitySwipe { .. }
            | Command::SwipeVisibility { .. }
            | Command::EndVisibilitySwipe
            | Command::CopyMask { .. }
            | Command::ToggleClippingFor { .. }
            | Command::ToggleMaskLinkFor { .. }
            | Command::LoadThumbnailSelection { .. }
            | Command::DuplicateTo { .. }
            | Command::SetMaskPlacement { .. }
            | Command::DistortMask { .. } => unreachable!("handled above"),
            Command::TypeOpacityDigit { .. } | Command::StepBrushHardness { .. } => {}
            Command::SelectTextUnit { point, paragraph } => {
                if self.text_editing()
                    && let Some(layer) = self.selected()
                {
                    let point =
                        geometry::to_document(&self.history.document, &self.viewport, point);
                    let (start, end) = crate::text::unit_at(layer, point, paragraph);
                    self.text_selection = (start, end);
                    self.text_mouse_unit = Some((start, end, paragraph));
                }
            }
            Command::SetTextCaretVisible { visible } => self.text_caret_visible = visible,
            Command::SetDisplayScale { scale } => {
                ensure!(
                    scale.is_finite() && (0.5..=8.).contains(&scale),
                    "Invalid display scale"
                );
                self.display_scale = scale;
            }
            Command::UpdateText { patch } => self.update_text(patch)?,
            Command::CommitText => self.finish_text()?,
            Command::CancelText => self.cancel_text(),
            Command::MoveTextCaret { direction, extend } => self.move_text_caret(direction, extend),
            Command::SetTextSelection { anchor, head } => {
                if let Some(layer) = self.selected()
                    && let Content::Text { text, .. } = layer.content.as_ref()
                {
                    ensure!(
                        anchor <= text.len()
                            && head <= text.len()
                            && text.is_char_boundary(anchor)
                            && text.is_char_boundary(head),
                        "Invalid text selection"
                    );
                    if self.text_selection != (anchor, head) {
                        self.text_upstream = false;
                        self.text_vertical_x = None;
                    }
                    self.text_selection = (anchor, head);
                }
            }
            Command::SetViewOptions { options } => {
                self.view_options = options;
                self.snap_lines = (vec![], vec![]);
                if self.view_options.lock_guides {
                    self.guide_drag = None;
                }
            }
            Command::BeginGuide { axis, point } => self.begin_guide(axis, point)?,
            Command::FinishGuide { delete } => self.finish_guide(delete)?,
            Command::AddGuide { axis, position } => {
                ensure!(
                    position.is_finite() && position.abs() <= 100000.,
                    "Invalid guide coordinate"
                );
                if !self.view_options.lock_guides {
                    let mut doc = self.history.document.clone();
                    doc.guides.push(crate::placement::CanvasGuide {
                        id: id(),
                        axis,
                        position,
                    });
                    doc.validate()?;
                    self.view_options.guides = true;
                    self.edit("New Guide", doc, None)?;
                }
            }
            Command::ClearGuides => {
                let mut doc = self.history.document.clone();
                doc.guides.clear();
                self.edit("Clear Guides", doc, None)?;
            }
            Command::LayerViaCopy => self.layer_via_copy()?,
            Command::MergeLayers => self.merge_layers()?,
            Command::AddAdjustment { kind } => self.add_adjustment(kind)?,
            Command::BeginAdjustmentEdit { id } => self.begin_adjustment_edit(id)?,
            Command::UpdateAdjustmentLevels { settings, preview } => {
                self.update_adjustment_levels(settings, preview)?
            }
            Command::UpdateAdjustmentCurves { settings, preview } => {
                self.update_adjustment_curves(settings, preview)?
            }
            Command::SetAdjustmentPreview { preview } => self.set_adjustment_preview(preview)?,
            Command::AutoLevels { mode } => self.auto_levels(mode)?,
            Command::SampleLevels { point, mode } => self.sample_levels(point, mode)?,
            Command::SetLevelsSampleMode { mode } => self.set_levels_sample_mode(mode),
            Command::CancelAdjustmentEdit => self.cancel_adjustment_edit(),
            Command::CommitAdjustmentEdit => self.commit_adjustment_edit()?,
            Command::BeginTransform => {
                self.resolve_gradient()?;
                if self.floating.is_none()
                    && !self.layer_transform
                    && !self.begin_selection_transform(false, true)?
                {
                    if self.paint_target == PaintTarget::Content && self.begin_group_transform()? {
                    } else if self.selection.ids.len() == 1
                        && self.selected().is_some_and(|l| {
                            !l.locked
                                && (l.adjustment.is_none()
                                    || self.independent_mask_layer().is_some())
                                && (!matches!(l.content.as_ref(), Content::Group)
                                    || self.independent_mask_layer().is_some())
                        })
                    {
                        self.finish_gesture()?;
                        self.transform_pixel_size = Some(self.transform_source_size());
                        self.begin_edit("Transform Layer");
                        self.layer_transform = true;
                        self.tool = Tool::Move;
                    }
                }
            }
            Command::CommitTransform => self.commit_transform()?,
            Command::CancelTransform => self.cancel_transform(),
            Command::MovePixels { delta, duplicate } => self.move_pixels(delta, duplicate)?,
            Command::MoveSelection { delta } => {
                ensure!(
                    delta.x.is_finite() && delta.y.is_finite(),
                    "Invalid selection movement"
                );
                if let Some(selection) = self
                    .history
                    .pixel_selection
                    .as_ref()
                    .filter(|s| !s.outline.is_empty())
                {
                    self.set_pixel_selection(Some(selection.translated(delta)?), "Move Selection")?;
                }
            }
            Command::SetLassoKind { kind } => {
                self.cancel_polygon();
                self.lasso_kind = kind;
            }
            Command::FinishSelection => self.finish_polygon()?,
            Command::RemoveSelectionPoint => {
                if let Some(draft) = &mut self.polygon {
                    draft.points.pop();
                    if draft.points.is_empty() {
                        self.cancel_polygon();
                    }
                }
            }
            Command::SetWand { settings } => {
                ensure!(settings.radius <= 2, "Invalid wand sample size");
                self.wand = settings;
            }
            Command::ResizeImage { options } => {
                self.finish_gesture()?;
                let next = crate::image_size::resize(&self.history.document, &options)?;
                if next != self.history.document {
                    self.edit_canvas("Image Size", next)?;
                }
            }
            Command::Select { id, mode } => self.select(id, mode)?,
            Command::SelectAll => {
                self.finish_gesture()?;
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
                self.cancel_polygon();
                // Switching tools applies the pending gradient, as in Photoshop.
                if tool != self.tool {
                    self.resolve_gradient()?;
                }
                if matches!(self.gesture, Some(Gesture::Shape { .. })) {
                    self.cancel_gesture();
                } else {
                    self.finish_gesture()?;
                }
                if tool != Tool::Crop {
                    self.crop_rect = None;
                }
                self.tool = tool;
            }
            Command::Fit => self.fit(),
            Command::Zoom { zoom, point } => {
                ensure!(zoom.is_finite(), "Invalid zoom");
                self.finish_gesture()?;
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
                self.set_palette_color(color, false)?;
            }
            Command::SetBrush { size, opacity } => {
                ensure!(
                    (1.0..=2000.).contains(&size) && (0.0..=1.).contains(&opacity),
                    "Invalid brush settings"
                );
                self.brush_size = size;
                self.brush_opacity = opacity;
                self.refresh_gradient()?;
            }
            Command::SetBrushTip {
                hardness,
                smoothing,
            } => {
                ensure!(
                    (0.0..=1.0).contains(&hardness) && (0.0..=100.0).contains(&smoothing),
                    "Invalid brush tip"
                );
                self.brush_hardness = hardness;
                self.brush_smoothing = smoothing;
                self.refresh_gradient()?;
            }
            Command::SetShapeCornerRadius { radius } => {
                ensure!(
                    radius.is_finite(),
                    "Enter a corner radius between 0 and 5000 pixels"
                );
                self.shape_corner_radius = radius.clamp(0., 5000.);
            }
            Command::SetShapeLineWidth { width } => {
                ensure!(
                    width.is_finite(),
                    "Enter a line width between 1 and 5000 pixels"
                );
                self.shape_line_width = width.clamp(1., 5000.);
            }
            Command::CycleShapeKind => self.cycle_shape_kind(),
            Command::SetGradientShape { shape } => {
                self.gradient_settings.shape = shape;
                self.refresh_gradient()?;
            }
            Command::SetGradientStyle { style } => {
                self.gradient_settings.style = style;
                self.refresh_gradient()?;
            }
            Command::SetGradientReverse { reversed } => {
                self.gradient_settings.reversed = reversed;
                self.refresh_gradient()?;
            }
            Command::SetGradientOpacity { opacity } => {
                ensure!(
                    opacity.is_finite() && (0.01..=1.).contains(&opacity),
                    "Gradient opacity must be between 1% and 100%"
                );
                self.gradient_settings.opacity = opacity;
                self.refresh_gradient()?;
            }
            Command::CommitGradient => self.commit_gradient()?,
            Command::CancelGradient => self.cancel_gradient(),
            Command::UpdateLayer { patch } if text_property => {
                let content: Content = serde_json::from_value(patch["content"].clone())?;
                let Content::Text {
                    text,
                    font_size,
                    font_family,
                    color,
                } = content
                else {
                    unreachable!()
                };
                self.update_text(crate::text::TextPatch {
                    text: Some(text),
                    font_size: Some(font_size),
                    color: Some(color),
                    font_name: Some(
                        match font_family {
                            FontFamily::SansSerif => "sans-serif",
                            FontFamily::Serif => "serif",
                            FontFamily::Monospace => "monospace",
                        }
                        .into(),
                    ),
                    ..Default::default()
                })?;
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
                    crate::live_mask::release_detached(&mut doc);
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
                crate::live_mask::release_detached(&mut doc);
                self.edit("Move layers into folder", doc, None)?;
                if let Some(parent) = parent_id {
                    self.collapsed_groups.remove(&parent);
                }
            }
            Command::Duplicate => {
                let mut doc = self.history.document.clone();
                let mut ids = vec![];
                let roots = self.selected_roots();
                let included: HashSet<_> = roots
                    .iter()
                    .flat_map(|root| {
                        doc.descendants(&root.id)
                            .into_iter()
                            .chain([root.id.clone()])
                    })
                    .collect();
                let mapping: std::collections::HashMap<_, _> = included
                    .into_iter()
                    .map(|original| (original, id()))
                    .collect();
                let mut layers = doc.layers.clone();
                for root in roots.iter().rev() {
                    let subtree = doc.descendants(&root.id);
                    let members: Vec<_> = doc
                        .layers
                        .iter()
                        .filter(|l| l.id == root.id || subtree.contains(&l.id))
                        .cloned()
                        .collect();
                    let mut copies = Vec::new();
                    for mut copy in members {
                        copy.id = mapping[&copy.id].clone();
                        copy.parent_id = copy
                            .parent_id
                            .map(|parent| mapping.get(&parent).cloned().unwrap_or(parent));
                        copy.mask_source_id = copy
                            .mask_source_id
                            .map(|source| mapping.get(&source).cloned().unwrap_or(source));
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
                let mut renderer = render::Renderer::default();
                for layer in &mut doc.layers {
                    if !removing.contains(&layer.id)
                        && layer
                            .mask_source_id
                            .as_ref()
                            .is_some_and(|id| removing.contains(id))
                    {
                        // Clipped adjustments hold no pixels to bake; their link
                        // is released while pixel layers take the Bake path.
                        if layer.adjustment.is_some() {
                            layer.mask_source_id = None;
                        } else {
                            *layer = renderer.bake_live_mask(&self.history.document, layer)?;
                        }
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
                for id in &self.selection.ids {
                    crate::live_mask::adopt(&mut doc, id);
                }
                crate::live_mask::release_detached(&mut doc);
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
                    for id in &self.selection.ids {
                        crate::live_mask::adopt(&mut doc, id);
                    }
                    crate::live_mask::release_detached(&mut doc);
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
                // An open group draft moves as one (`nudgeLayer`); otherwise the
                // existing multi-layer translation below keeps working.
                if let Some(state) = &self.group_transform {
                    let mut draft = state.draft.clone();
                    draft.x += delta.x;
                    draft.y += delta.y;
                    draft.validate()?;
                    self.preview_group_box(draft)?;
                    return Ok(());
                }
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
                            && l.adjustment.is_none()
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
                self.finish_gesture()?;
                let doc = resize_canvas(&self.history.document, &options)?;
                if doc != self.history.document {
                    self.edit_canvas("Canvas Size", doc)?;
                }
            }
            Command::SetCropRatio { ratio } => {
                self.finish_gesture()?;
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
                self.finish_gesture()?;
                if let Some(rect) = self.crop_rect {
                    let next = crop_canvas(&self.history.document, rect)?;
                    self.edit_canvas("Crop", next)?;
                    self.crop_rect = None;
                }
            }
            Command::CancelCrop => {
                self.cancel_gesture();
                self.crop_rect = None;
            }
            Command::SetMarqueeKind { kind } => {
                self.finish_gesture()?;
                self.marquee_kind = kind;
            }
            Command::SetSelectionMode { mode } => {
                self.finish_gesture()?;
                self.selection_mode = mode;
            }
            Command::DeselectPixels => {
                self.set_pixel_selection(None, "Deselect")?;
            }
            Command::CancelPixelSelection => {
                if self.polygon.is_some() {
                    self.cancel_polygon();
                    return Ok(());
                }
                // EditorCanvas's Escape handling cancels the draft first, keeping the
                // completed selection. A subsequent Escape deselects through history.
                if matches!(self.gesture, Some(Gesture::PixelSelection { .. })) {
                    self.cancel_gesture();
                } else {
                    self.set_pixel_selection(None, "Deselect")?;
                }
            }
            Command::SelectAllPixels => {
                self.finish_gesture()?;
                let doc = &self.history.document;
                let selection = PixelSelection::all(doc.width, doc.height)?;
                self.set_pixel_selection(Some(selection), "Select All")?;
            }
            Command::BeginPropertyEdit { label } => {
                ensure!(!label.is_empty() && label.len() <= 64, "Invalid edit label");
                if self.text_editing() {
                    return Ok(());
                }
                self.finish_gesture()?;
                self.begin_edit(&label);
                self.gesture = Some(Gesture::Property);
            }
            Command::PickUnder { point } => {
                self.finish_gesture()?;
                ensure!(
                    point.x.is_finite() && point.y.is_finite(),
                    "Invalid pointer coordinate"
                );
                let p = geometry::to_document(&self.history.document, &self.viewport, point);
                self.pick_under(p);
            }
            Command::EditText { id, point } => {
                self.finish_gesture()?;
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
                    self.begin_text_edit(id)?;
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
                if let Some(current) = &self.history.pixel_selection
                    && !current.outline.is_empty()
                {
                    let mut next = current.clone();
                    // Two soft edges together spread a little less than their sum, as blurs do.
                    let softened = (current.feather * current.feather
                        + f64::from(amount) * f64::from(amount))
                    .sqrt();
                    next.feather = softened.min(250.);
                    if !next.antialiased && current.feather == 0. {
                        next = PixelSelection::from_path_with_smoothing(
                            next.width,
                            next.height,
                            next.outline,
                            next.feather,
                            next.antialiased,
                        )?;
                    }
                    self.set_pixel_selection(Some(next), "Feather Selection")?;
                }
            }
            Command::InvertSelection => {
                self.finish_gesture()?;
                if let Some(current) = &self.history.pixel_selection {
                    let next = current.inverted()?;
                    self.set_pixel_selection(Some(next), "Inverse")?;
                }
            }
            Command::ExpandSelection { amount } | Command::ContractSelection { amount } => {
                ensure!(
                    (1..=500).contains(&amount),
                    "Selection amount must be between 1 and 500 pixels"
                );
                if self.selection_draft().is_none() {
                    if let Some(current) = &self.history.pixel_selection
                        && !current.outline.is_empty()
                    {
                        let expand = matches!(command, Command::ExpandSelection { .. });
                        let next = current.resized(if expand {
                            amount as f64
                        } else {
                            -(amount as f64)
                        })?;
                        self.set_pixel_selection(
                            Some(next),
                            if expand {
                                "Expand Selection"
                            } else {
                                "Contract Selection"
                            },
                        )?;
                    }
                }
            }
            Command::FillSelection | Command::FillBackground => {
                self.finish_gesture()?;
                let background = matches!(command, Command::FillBackground);
                let color = if background {
                    &self.background_color
                } else {
                    &self.color
                };
                if self.can_edit_pixels()
                    && let Some(layer) = self.selected().cloned()
                {
                    if self.paint_target == PaintTarget::Content
                        && self.history.pixel_selection.is_none()
                        && matches!(layer.content.as_ref(), Content::Text { .. })
                    {
                        let mut next = layer.clone();
                        if let Content::Text { color: value, .. } = Arc::make_mut(&mut next.content)
                        {
                            *value = color.clone();
                        }
                        self.edit("Fill", self.with_layers(vec![next]), None)?;
                        return Ok(());
                    }
                    let next = render::fill_selected_pixels(
                        &layer,
                        self.history.pixel_selection.as_ref(),
                        &self.history.document,
                        color,
                        self.paint_target == PaintTarget::Mask,
                        if background {
                            if self.mask_mode == MaskMode::Hide {
                                MaskMode::Reveal
                            } else {
                                MaskMode::Hide
                            }
                        } else {
                            self.mask_mode
                        },
                    )?;
                    self.edit(
                        if self.paint_target == PaintTarget::Mask {
                            "Fill Mask"
                        } else {
                            "Fill"
                        },
                        {
                            let mut doc = self.history.document.clone();
                            doc.replace(next);
                            doc
                        },
                        None,
                    )?;
                }
            }
            Command::ClearSelectedPixels => {
                self.finish_gesture()?;
                if let (Some(selection), Some(layer)) =
                    (&self.history.pixel_selection, self.selected())
                {
                    if selection.bounds.is_none() {
                        return Ok(());
                    }
                    if self.can_edit_pixels() {
                        let mut next = layer.clone();
                        if self.paint_target == PaintTarget::Mask {
                            next = render::fill_selected_pixels(
                                layer,
                                Some(selection),
                                &self.history.document,
                                &self.color,
                                true,
                                if self.mask_mode == MaskMode::Hide {
                                    MaskMode::Reveal
                                } else {
                                    MaskMode::Hide
                                },
                            )?;
                        } else {
                            let data = render::clear_selected_asset(
                                &next,
                                selection,
                                &self.history.document,
                            )?;
                            next.content = Arc::new(Content::Image { data });
                            next.strokes.clear();
                        }
                        self.edit("Clear selected pixels", self.with_layers(vec![next]), None)?;
                    }
                }
            }
            Command::AddMask { base } => {
                if self.selection.ids.len() == 1
                    && self
                        .selected()
                        .is_some_and(|l| !l.locked && l.mask.is_none())
                {
                    // LayerMask.addMask: consume the selection in the same undo step,
                    // painting the inverse of the requested base into that region.
                    let mut layer = self.selected().unwrap().clone();
                    layer.mask = Some(Arc::new(LayerMask {
                        enabled: true,
                        base,
                        raster: Some(Arc::new(MaskRaster::solid(base == MaskMode::Reveal))),
                        linked: true,
                        placement: None,
                        strokes: vec![],
                    }));
                    if let Some(selection) = &self.history.pixel_selection {
                        layer = render::fill_selected_pixels(
                            &layer,
                            Some(selection),
                            &self.history.document,
                            &self.color,
                            true,
                            if base == MaskMode::Reveal {
                                MaskMode::Hide
                            } else {
                                MaskMode::Reveal
                            },
                        )?;
                    }
                    let mut doc = self.history.document.clone();
                    doc.version = 2;
                    doc.replace(layer);
                    doc.validate()?;
                    self.begin_edit("Add layer mask");
                    self.history.preview(doc);
                    self.history.pixel_selection = None;
                    self.end_edit();
                    self.paint_target = PaintTarget::Mask;
                }
            }
            Command::SetPaintTarget { target } => {
                self.finish_gesture()?;
                // Switching targets applies the pending gradient, as in Photoshop.
                if target != self.paint_target {
                    self.resolve_gradient()?;
                };
                if target == PaintTarget::Content
                    || (self.selection.ids.len() == 1
                        && self.selected().and_then(|l| l.mask.as_ref()).is_some())
                {
                    self.paint_target = target;
                }
            }
            Command::SetMaskMode { mode } => {
                self.finish_gesture()?;
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
            Command::ToggleClippingMask => {
                if self.selection.ids.len() == 1
                    && let Some(layer) = self.selected().cloned()
                    && !layer.locked
                {
                    let mut doc = self.history.document.clone();
                    let label = if layer.mask_source_id.is_some() {
                        crate::live_mask::release(&mut doc, &layer.id);
                        "Release Clipping Mask"
                    } else if let Some(source) = crate::live_mask::clipping_source(&doc, &layer.id)
                    {
                        doc.layers
                            .iter_mut()
                            .find(|l| l.id == layer.id)
                            .unwrap()
                            .mask_source_id = Some(source);
                        "Create Clipping Mask"
                    } else {
                        return Ok(());
                    };
                    doc.version = 2;
                    self.edit(label, doc, None)?;
                }
            }
            Command::LinkMask { source_id } => {
                if self.selection.ids.len() == 1
                    && let Some(target) = self.selected_id().map(str::to_owned)
                {
                    ensure!(
                        crate::live_mask::can_link(&self.history.document, &source_id, &target),
                        "Invalid live mask link"
                    );
                    let mut doc = self.history.document.clone();
                    doc.layers
                        .iter_mut()
                        .find(|l| l.id == target)
                        .unwrap()
                        .mask_source_id = Some(source_id);
                    doc.version = 2;
                    self.edit("Create Clipping Mask", doc, None)?;
                }
            }
            Command::Undo => {
                if self.adjustment_edit.is_some() {
                    self.cancel_adjustment_edit();
                }
                self.cancel_gradient();
                if self.floating.is_some() || self.layer_transform {
                    self.cancel_transform();
                    return Ok(());
                }
                if self.gesture.is_some() {
                    self.cancel_gesture();
                }
                let s = self.history.undo();
                self.restore_selection(s);
            }
            Command::Redo => {
                if self.adjustment_edit.is_some() {
                    self.cancel_adjustment_edit();
                }
                self.cancel_gradient();
                if self.floating.is_some() || self.layer_transform {
                    self.cancel_transform();
                    return Ok(());
                }
                if self.gesture.is_some() {
                    self.cancel_gesture();
                }
                let s = self.history.redo();
                self.restore_selection(s);
            }
            Command::FinishGesture => {
                if self.text_editing() {
                    self.finish_text()?;
                } else {
                    self.finish_gesture()?;
                }
            }
            Command::CancelGesture => {
                if self.gradient_edit.is_some() {
                    self.cancel_gradient();
                } else {
                    self.cancel_gesture();
                }
            }
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
        self.update_floating_selection()?;
        Ok(())
    }
    pub fn pointer(&mut self, sample: PointerSample) -> Result<()> {
        let PointerSample {
            phase,
            point: vp,
            modifiers,
        } = sample;
        if phase == Phase::Cancel {
            if self.tool == Tool::Gradient && self.gradient_handle.is_some() {
                self.end_gradient_drag();
                return Ok(());
            }
            self.cancel_gesture();
            return Ok(());
        }
        let p = geometry::to_document(&self.history.document, &self.viewport, vp);
        if self.guide_pointer(phase, p)? {
            return Ok(());
        }
        if self.text_pointer(phase, p, modifiers)? {
            return Ok(());
        }
        if self.tool == Tool::Lasso
            && self.lasso_kind == LassoKind::Polygonal
            && self.polygon.is_some()
        {
            match phase {
                Phase::Down => {
                    let draft = self.polygon.as_mut().unwrap();
                    if draft.points.len() >= 3
                        && draft.points[0].distance(p) * self.viewport.zoom <= 8.
                    {
                        self.finish_polygon()?;
                    } else {
                        draft.drag(p, false);
                    }
                }
                Phase::Move => self.polygon_cursor = Some(p),
                _ => {}
            }
            return Ok(());
        }
        if phase == Phase::Down {
            self.finish_gesture()?;
            if self.tool == Tool::Crop {
                let before = self.crop_rect;
                let original =
                    before.unwrap_or_else(|| CropRect::from_document(&self.history.document));
                let mut mode = crop::hit(original, p, self.viewport.zoom);
                // EditorCanvas.beginCropDrag: the default visible frame is not
                // a movable crop edit; drawing inside it begins a new frame.
                if original == CropRect::from_document(&self.history.document)
                    && matches!(mode, crop::DragMode::Move)
                {
                    mode = crop::DragMode::Create;
                }
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
            if matches!(self.tool, Tool::Marquee | Tool::Lasso | Tool::Wand) {
                let mode = if modifiers.alt {
                    PixelSelectionMode::Subtract
                } else if modifiers.shift {
                    PixelSelectionMode::Add
                } else {
                    self.selection_mode
                };
                let inside_selection = self
                    .history
                    .pixel_selection
                    .as_ref()
                    .is_some_and(|s| s.contains(p));
                if self.tool == Tool::Wand
                    && !((modifiers.control || modifiers.meta) && inside_selection)
                {
                    self.magic_wand(p, mode)?;
                    return Ok(());
                }
                if (mode == PixelSelectionMode::Replace || modifiers.control || modifiers.meta)
                    && self.polygon.is_none()
                    && inside_selection
                {
                    if modifiers.control || modifiers.meta {
                        if self.begin_selection_transform(modifiers.alt, false)? {
                            let layer = self.selected().unwrap().clone();
                            self.gesture = Some(Gesture::PixelMove { origin: p, layer });
                        }
                    } else {
                        let selection = self.history.pixel_selection.clone().unwrap();
                        self.begin_edit("Move Selection");
                        self.gesture = Some(Gesture::SelectionMove {
                            origin: p,
                            selection,
                        });
                    }
                    return Ok(());
                }
                if self.tool == Tool::Lasso && self.lasso_kind == LassoKind::Polygonal {
                    self.polygon = Some(SelectionDraft::new(p, None, mode));
                    self.polygon_cursor = Some(p);
                    return Ok(());
                }
                let marquee = (self.tool == Tool::Marquee).then_some(self.marquee_kind);
                self.gesture = Some(Gesture::PixelSelection {
                    draft: SelectionDraft::new(p, marquee, mode),
                    before: self.history.pixel_selection.clone(),
                });
                return Ok(());
            }
            if self.tool == Tool::Move {
                if let Some(target) = self.independent_mask_layer() {
                    let handle = self
                        .cursor_map()
                        .handles
                        .as_ref()
                        .and_then(|h| h.hit(p))
                        .or_else(|| {
                            self.mask_distortion
                                .is_none()
                                .then(|| geometry::hit_handle(&target, p, self.viewport.zoom))
                                .flatten()
                        })
                        .unwrap_or("move");
                    if self.mask_distortion.is_some()
                        || ((modifiers.control || modifiers.meta)
                            && handle != "move"
                            && handle != "rotate")
                    {
                        self.begin_mask_distortion()?;
                        let corners = self.mask_distortion.as_ref().unwrap().corners;
                        self.begin_edit("Distort Layer Mask");
                        self.gesture = Some(Gesture::MaskDistort {
                            origin: p,
                            handle,
                            corners,
                            layer: target,
                            free: modifiers.control || modifiers.meta,
                            mask: true,
                        });
                        return Ok(());
                    }
                    let layer = self.selected().unwrap().clone();
                    if let Some(handle) = geometry::hit_handle(&target, p, self.viewport.zoom)
                        && (self.view_options.show_controls || self.layer_transform)
                    {
                        let handle_point = geometry::handles(&target, self.viewport.zoom)
                            .into_iter()
                            .find(|h| h.0 == handle)
                            .unwrap()
                            .1;
                        self.begin_edit("Transform Layer Mask");
                        self.gesture = Some(Gesture::Transform {
                            origin: p,
                            handle,
                            handle_point,
                            layer: target,
                            mask: true,
                        });
                    } else {
                        let at = geometry::to_local(&target, p);
                        if at.x < 0.
                            || at.y < 0.
                            || at.x > target.width as f64
                            || at.y > target.height as f64
                        {
                            return Ok(());
                        }
                        self.begin_edit("Move Layer Mask");
                        self.gesture = Some(Gesture::MaskMove {
                            origin: p,
                            layer,
                            placement: MaskPlacement::of(&target),
                        });
                    }
                    return Ok(());
                }
                if self.selection.ids.len() == 1
                    && self.selected().is_some_and(|l| {
                        !l.locked
                            && l.adjustment.is_none()
                            && !matches!(l.content.as_ref(), Content::Group | Content::Text { .. })
                    })
                {
                    let handle = self
                        .cursor_map()
                        .handles
                        .as_ref()
                        .and_then(|h| h.hit(p))
                        .unwrap_or("move");
                    if self.image_distortion.is_some()
                        || ((modifiers.control || modifiers.meta)
                            && handle != "move"
                            && handle != "rotate")
                    {
                        self.begin_image_distortion()?;
                        if let Some(draft) = &self.image_distortion {
                            let layer = draft.original.clone();
                            let corners = draft.corners;
                            self.begin_edit("Distort Layer");
                            self.gesture = Some(Gesture::MaskDistort {
                                origin: p,
                                handle,
                                corners,
                                layer,
                                free: modifiers.control || modifiers.meta,
                                mask: false,
                            });
                            return Ok(());
                        }
                    }
                }
                // Several layers, or a folder: one box around them all, with its own
                // resize/rotate handles (TransformOverlay's group branch). A press
                // elsewhere falls through to picking and multi-layer translation.
                if self.paint_target == PaintTarget::Content
                    && (self.view_options.show_controls || self.layer_transform)
                    && let Some(box_layer) = self.edited_group_box()
                {
                    let zoom = self.viewport.zoom / self.display_scale;
                    if let Some(handle) = geometry::hit_handle(&box_layer, p, zoom) {
                        let open = self.group_transform.is_some();
                        let (base, original, members) = if open {
                            let state = self.group_transform.as_ref().unwrap();
                            (
                                state.draft.clone(),
                                state.original.clone(),
                                state.members.clone(),
                            )
                        } else {
                            let members = self.group_members();
                            let Some(bx) = group_transform::group_box(&members) else {
                                return Ok(());
                            };
                            self.begin_edit(if handle == "rotate" {
                                "Rotate layers"
                            } else {
                                "Resize layers"
                            });
                            // The draft lives in the shared group state for the
                            // length of the drag, so handles and numeric fields
                            // track it; Up closes the transaction and clears it.
                            self.transform_pixel_size =
                                Some((bx.width as f64 * bx.scale_x, bx.height as f64 * bx.scale_y));
                            self.group_transform = Some(group_transform::GroupState {
                                original: bx.clone(),
                                draft: bx.clone(),
                                members: members.as_ref().clone(),
                            });
                            self.layer_transform = true;
                            (bx.clone(), bx, members.as_ref().clone())
                        };
                        let handle_point = geometry::handles(&base, zoom)
                            .into_iter()
                            .find(|h| h.0 == handle)
                            .map(|h| h.1)
                            .unwrap_or(p);
                        self.gesture = Some(Gesture::GroupTransform {
                            origin: p,
                            handle,
                            handle_point,
                            drag: Box::new(group_transform::GroupDrag {
                                base,
                                original,
                                members,
                                open,
                            }),
                        });
                        return Ok(());
                    }
                }
                if let Some(l) = self.selected().cloned()
                    && self.selection.ids.len() == 1
                    && !l.locked
                    && l.visible
                    && (self.view_options.show_controls || self.layer_transform)
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
                        mask: false,
                    });
                    return Ok(());
                }
                let hit = geometry::hit_test(&self.history.document, p).map(|l| l.id.clone());
                if modifiers.shift {
                    if hit.is_some() {
                        self.select(hit, SelectionMode::Toggle)?;
                    }
                    return Ok(());
                }
                // Cmd/Ctrl-click walks the layers whose bounds contain the point, top to
                // bottom and back around. Upstream's Cmd-click re-picks the topmost layer;
                // cycling is a local extension so a full-canvas layer cannot bury the stack.
                if modifiers.meta {
                    self.pick_under(p);
                    return Ok(());
                }
                if self.view_options.auto_select
                    && !self.layer_transform
                    && hit.is_some()
                    && !hit
                        .as_ref()
                        .is_some_and(|id| self.selection.ids.contains(id))
                {
                    self.single_selection(hit.clone());
                }
                if !self.selected_layers().is_empty() {
                    self.begin_edit("Move layers");
                    self.gesture = Some(Gesture::Move {
                        origin: p,
                        layers: self
                            .selected_layers()
                            .into_iter()
                            .filter(|l| {
                                !l.locked
                                    && l.adjustment.is_none()
                                    && self.history.document.effective(l).0
                            })
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
            if matches!(self.tool, Tool::Rectangle | Tool::Ellipse | Tool::Line) {
                if self.history.document.layers.len() >= crate::model::MAX_LAYERS {
                    return Ok(());
                }
                // ShapeTool.beginShape: the anchor snaps to whole document pixels; a
                // rectangle takes the current radius, other shapes take none.
                let anchor = Point::new(p.x.round(), p.y.round());
                let kind = self.shape_kind();
                let label = match kind {
                    Shape::Rectangle => "Rectangle",
                    Shape::Ellipse => "Ellipse",
                    Shape::Line => "Line",
                };
                let mut l = Layer::new(
                    &self.next_shape_name(label),
                    1,
                    1,
                    Content::Shape {
                        shape: kind,
                        color: self.color.clone(),
                        corner_radius: if kind == Shape::Rectangle {
                            self.shape_corner_radius
                        } else {
                            0.
                        },
                        line_width: (kind == Shape::Line).then_some(self.shape_line_width),
                        line_start: None,
                        line_end: None,
                    },
                );
                l.x = anchor.x;
                l.y = anchor.y;
                self.begin_edit("Draw shape");
                self.history.preview(self.inserted(vec![l.clone()]));
                self.single_selection(Some(l.id.clone()));
                self.gesture = Some(Gesture::Shape {
                    origin: anchor,
                    layer: l,
                });
                return Ok(());
            }
            if self.tool == Tool::Gradient {
                // EditorCanvas.beginGradientDrag: grabbing an endpoint moves it, else a
                // new line starts. Shift snapping applies while dragging, not here.
                self.begin_gradient_drag(p)?;
                return Ok(());
            }
            if matches!(self.tool, Tool::Brush | Tool::Eraser) {
                if self.selection.ids.len() > 1
                    || self
                        .history
                        .pixel_selection
                        .as_ref()
                        .is_some_and(|s| s.bounds.is_none())
                {
                    return Ok(());
                }
                let mask = self.paint_target == PaintTarget::Mask;
                if self.selected().is_some_and(|l| {
                    l.locked
                        || !self.history.document.effective(l).0
                        || (!mask && matches!(l.content.as_ref(), Content::Group))
                }) {
                    return Ok(());
                }
                if mask
                    && self
                        .selected()
                        .and_then(|l| l.mask.as_ref())
                        .is_none_or(|m| !m.enabled)
                {
                    return Ok(());
                }
                if self.selected().is_none()
                    && self.history.document.layers.len() >= crate::model::MAX_LAYERS
                {
                    return Ok(());
                }
                self.begin_edit(if mask {
                    "Paint Mask"
                } else if self.tool == Tool::Eraser {
                    "Erase"
                } else {
                    "Brush Stroke"
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
                let layer = self.selected().unwrap().clone();
                let color = render::color(&self.color);
                let white = (self.mask_mode == MaskMode::Reveal) != (self.tool == Tool::Eraser);
                let settings = crate::brush::Settings {
                    diameter: self.brush_size,
                    hardness: self.brush_hardness,
                    opacity: self.brush_opacity,
                    smoothing: if self.tool == Tool::Brush {
                        self.brush_smoothing
                    } else {
                        0.
                    },
                    erase: !mask && self.tool == Tool::Eraser,
                    color: if mask {
                        if white { [255; 3] } else { [0; 3] }
                    } else {
                        [color.r(), color.g(), color.b()]
                    },
                };
                match crate::brush::BrushStroke::new(
                    layer,
                    &self.history.document,
                    self.history.pixel_selection.as_ref(),
                    mask,
                    settings,
                    self.viewport.zoom,
                ) {
                    Ok(mut stroke) => {
                        if modifiers.shift
                            && let Some((id, target_mask, point)) = &self.last_brush_point
                            && Some(id.as_str()) == self.selected_id()
                            && *target_mask == mask
                        {
                            if let Err(error) = stroke.input(*point) {
                                self.cancel_gesture();
                                return Err(error);
                            }
                        }
                        self.gesture = Some(Gesture::Brush {
                            stroke: Box::new(stroke),
                        })
                    }
                    Err(error) => {
                        self.cancel_gesture();
                        return Err(error);
                    }
                }
            } else {
                return Ok(());
            }
        }
        // The gradient drag is not a Gesture: the line stays pending after release,
        // with Apply/Cancel in the header, until it is committed or cancelled.
        if self.tool == Tool::Gradient && self.gradient_handle.is_some() {
            match phase {
                Phase::Move => self.move_gradient(p, modifiers.shift)?,
                Phase::Up | Phase::Cancel => self.end_gradient_drag(),
                Phase::Down => {}
            }
            return Ok(());
        }
        let Some(mut gesture) = self.gesture.take() else {
            return Ok(());
        };
        match &mut gesture {
            Gesture::TextBox { origin, current } => {
                *current = p;
                if phase == Phase::Up {
                    let size = if (p.x - origin.x).abs() < 4. && (p.y - origin.y).abs() < 4. {
                        None
                    } else {
                        Some((
                            (p.x - origin.x).abs().round().max(16.) as u32,
                            (p.y - origin.y).abs().round().max(16.) as u32,
                        ))
                    };
                    let at = if size.is_some() {
                        Point::new(origin.x.min(p.x), origin.y.min(p.y))
                    } else {
                        *origin
                    };
                    self.begin_new_text(at, size)?;
                    return Ok(());
                }
            }
            Gesture::TextResize {
                origin,
                handle,
                layer,
            } => {
                self.resize_text_box(layer, handle, *origin, p)?;
                if phase == Phase::Up {
                    self.gesture = None;
                    return Ok(());
                }
            }
            Gesture::SelectionMove { origin, selection } => {
                self.history.pixel_selection =
                    Some(selection.translated(Point::new(p.x - origin.x, p.y - origin.y))?);
            }
            Gesture::PixelMove { origin, layer } => {
                let mut next = layer.clone();
                next.x += (p.x - origin.x).round();
                next.y += (p.y - origin.y).round();
                if next.validate().is_ok() {
                    self.history.preview(self.with_layers(vec![next]));
                    self.update_floating_selection()?;
                }
            }
            Gesture::Pan { origin, pan } => {
                self.viewport.pan = Point::new(pan.x + vp.x - origin.x, pan.y + vp.y - origin.y);
            }
            Gesture::Move { origin, layers } => {
                let delta = self.snap_move(
                    layers,
                    interaction_polish::constrained_delta(
                        Point::new(p.x - origin.x, p.y - origin.y),
                        modifiers.shift,
                    ),
                    modifiers.control,
                );
                let moved = Self::moved(layers, delta);
                self.history.preview(self.with_layers(moved));
            }
            Gesture::Transform {
                origin,
                handle,
                handle_point,
                layer,
                mask,
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
                            modifiers.shift || self.locks_transform_ratio,
                            modifiers.alt,
                        )
                    };
                }
                if next.validate().is_ok() {
                    if *mask {
                        let mut original = self.selected().unwrap().clone();
                        let placement = MaskPlacement::of(&next);
                        Arc::make_mut(original.mask.as_mut().unwrap()).placement =
                            if placement == MaskPlacement::of(&original) {
                                None
                            } else {
                                Some(placement)
                            };
                        self.history.preview(self.with_layers(vec![original]));
                    } else {
                        self.history.preview(self.with_layers(vec![next]));
                    }
                }
            }
            // A group-box drag carries every member from its original placement
            // through the box-relative affine (`TransformGroup.following`).
            Gesture::GroupTransform {
                origin,
                handle,
                handle_point,
                drag,
            } => {
                let draft = if *handle == "rotate" {
                    geometry::rotate(&drag.base, *origin, p, modifiers.shift)
                } else {
                    geometry::resize(
                        &drag.base,
                        handle,
                        Point::new(
                            handle_point.x + p.x - origin.x,
                            handle_point.y + p.y - origin.y,
                        ),
                        modifiers.shift || self.locks_transform_ratio,
                        modifiers.alt,
                    )
                };
                if draft.validate().is_ok() {
                    let mut doc = self.history.document.clone();
                    group_transform::replace_all(
                        &mut doc,
                        group_transform::carry_members(&drag.members, &drag.original, &draft),
                    );
                    if doc.validate().is_ok() {
                        self.history.preview(doc);
                        if let Some(state) = &mut self.group_transform {
                            state.draft = draft;
                        }
                    }
                }
            }
            Gesture::Shape { origin, layer } => {
                let line = matches!(
                    layer.content.as_ref(),
                    Content::Shape {
                        shape: Shape::Line,
                        ..
                    }
                );
                if line {
                    // ShapeTool.dragShape's line branch: Shift snaps the angle to
                    // eighths of a turn rather than squaring a box.
                    let mut end = p;
                    if modifiers.shift {
                        end = crate::shape::snap_line_end(*origin, p);
                    }
                    // A click without a drag makes nothing, as for other shapes.
                    if (end.x - origin.x).abs() < 1. && (end.y - origin.y).abs() < 1. {
                        if phase == Phase::Up {
                            self.cancel_gesture();
                        } else {
                            self.gesture = Some(gesture);
                        }
                        return Ok(());
                    }
                    let thickness = match layer.content.as_ref() {
                        Content::Shape { line_width, .. } => line_width.unwrap_or(4.),
                        _ => 4.,
                    };
                    // The layer is the dragged box with room for the stroke's own
                    // thickness around it; the ends stay fractional so a scaled line
                    // still runs between the same two places.
                    let bounds = crate::shape::line_box(*origin, end, thickness, modifiers.alt);
                    let (x0, y0) = (bounds.x.floor(), bounds.y.floor());
                    let (x1, y1) = (
                        (bounds.x + bounds.width).ceil(),
                        (bounds.y + bounds.height).ceil(),
                    );
                    let mut l = layer.clone();
                    l.x = x0;
                    l.y = y0;
                    l.width = (x1 - x0).max(1.) as u32;
                    l.height = (y1 - y0).max(1.) as u32;
                    if let Err(error) = dimensions(l.width, l.height) {
                        self.cancel_gesture();
                        return Err(error);
                    }
                    let unit = |point: Point| {
                        Point::new((point.x - x0) / (x1 - x0), (point.y - y0) / (y1 - y0))
                    };
                    if let Content::Shape {
                        line_start,
                        line_end,
                        ..
                    } = Arc::make_mut(&mut l.content)
                    {
                        *line_start = Some(unit(*origin));
                        *line_end = Some(unit(end));
                    }
                    self.history.preview(self.with_layers(vec![l]));
                } else {
                    let Some(bounds) =
                        crate::shape::drag_box(*origin, p, modifiers.shift, modifiers.alt)
                    else {
                        if phase == Phase::Up {
                            self.cancel_gesture();
                        } else {
                            self.gesture = Some(gesture);
                        }
                        return Ok(());
                    };
                    let mut l = layer.clone();
                    l.x = bounds.x;
                    l.y = bounds.y;
                    l.width = bounds.width as u32;
                    l.height = bounds.height as u32;
                    if let Err(error) = dimensions(l.width, l.height) {
                        self.cancel_gesture();
                        return Err(error);
                    }
                    self.history.preview(self.with_layers(vec![l]));
                }
            }
            Gesture::Brush { stroke } => {
                let update = (|| -> Result<Layer> {
                    stroke.input(p)?;
                    if let (Some(id), Some(point)) = (self.selected_id(), stroke.last_point()) {
                        self.last_brush_point =
                            Some((id.to_owned(), self.paint_target == PaintTarget::Mask, point));
                    }
                    if phase == Phase::Up {
                        stroke.finish()?;
                    }
                    stroke.snapshot()
                })();
                match update {
                    Ok(layer) => {
                        let mut doc = self.history.document.clone();
                        if layer.mask.is_some() {
                            doc.version = 2;
                        }
                        doc.replace(layer);
                        self.history.preview(doc);
                    }
                    Err(error) => {
                        self.cancel_gesture();
                        return Err(error);
                    }
                }
            }
            Gesture::Crop { drag, .. } => {
                let ratio = self.crop_ratio.value(&self.history.document);
                let mut rect = drag.updated(p, ratio, modifiers.alt);
                let (xs, ys) = crate::placement::targets(
                    &self.history.document,
                    &self.view_options,
                    &[],
                    false,
                );
                if !modifiers.control {
                    rect = crop::snap(
                        rect,
                        *drag,
                        p,
                        ratio,
                        modifiers.alt,
                        (&xs, &ys),
                        10. * self.display_scale / self.viewport.zoom,
                    );
                }
                if rect.validate().is_ok() {
                    self.crop_rect = Some(rect);
                }
            }
            Gesture::MaskDistort {
                origin,
                handle,
                corners,
                layer,
                free,
                mask,
            } => {
                let mut moved = *corners;
                let delta = interaction_polish::constrained_delta(
                    Point::new(p.x - origin.x, p.y - origin.y),
                    modifiers.shift,
                );
                if *handle == "move" {
                    for corner in &mut moved {
                        corner.x += delta.x;
                        corner.y += delta.y;
                    }
                } else if *free && *handle != "rotate" {
                    let affected: &[usize] = match *handle {
                        "nw" => &[0],
                        "n" => &[0, 1],
                        "ne" => &[1],
                        "e" => &[1, 2],
                        "se" => &[2],
                        "s" => &[2, 3],
                        "sw" => &[3],
                        "w" => &[3, 0],
                        _ => &[],
                    };
                    for i in affected {
                        moved[*i].x += delta.x;
                        moved[*i].y += delta.y;
                    }
                } else {
                    let next = if *handle == "rotate" {
                        geometry::rotate(layer, *origin, p, modifiers.shift)
                    } else {
                        let at = geometry::handles(layer, self.viewport.zoom)
                            .into_iter()
                            .find(|h| h.0 == *handle)
                            .unwrap()
                            .1;
                        geometry::resize(
                            layer,
                            handle,
                            Point::new(at.x + p.x - origin.x, at.y + p.y - origin.y),
                            modifiers.shift,
                            modifiers.alt,
                        )
                    };
                    for (point, old) in moved.iter_mut().zip(corners.iter()) {
                        *point = geometry::to_world(&next, geometry::to_local(layer, *old));
                    }
                }
                if crate::distort::usable(&moved) {
                    if *mask {
                        self.preview_mask_distortion(moved, Some(2048.))?;
                    } else {
                        self.preview_image_distortion(moved, Some(2048.))?;
                    }
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
                    let selection =
                        pixel_selection::finish(&self.history.document, before.as_ref(), draft)?
                            .map(|s| s.with_antialiasing(self.selection_antialiased))
                            .transpose()?;
                    let label = if draft.bounds().is_none() {
                        "Deselect"
                    } else {
                        match draft.marquee {
                            Some(MarqueeKind::Rectangle) => "Rectangular Marquee",
                            Some(MarqueeKind::Ellipse) => "Elliptical Marquee",
                            None => "Lasso",
                        }
                    };
                    self.set_pixel_selection(selection, label)?;
                }
            }
            // Property drags are driven by slider commands, not the canvas pointer.
            Gesture::Property => {}
        }
        let keep_open = matches!(&gesture, Gesture::GroupTransform { drag, .. } if drag.open);
        let finishing_group =
            matches!(&gesture, Gesture::GroupTransform { drag, .. } if !drag.open);
        self.gesture = Some(gesture);
        self.update_floating_selection()?;
        if phase == Phase::Up {
            if keep_open {
                // A persistent Apply/Cancel edit stays open; the drag folds into it.
                self.gesture = None;
            } else {
                self.finish_gesture()?;
                if finishing_group {
                    // A handle drag is one undo and leaves no draft behind.
                    self.group_transform = None;
                    self.layer_transform = false;
                    self.transform_pixel_size = None;
                }
            }
            if self.floating.as_ref().is_some_and(|f| !f.persistent) {
                self.commit_transform()?;
            }
        }
        Ok(())
    }
}
