//! Read-only native publication. Document/row resources are shared across viewport updates.
//! Compositor CanvasViewport / NativeLayerList, 609dbeae, MIT © 2026 Wonder Assembly LLC.
//! Typed publication and cache lifetime are Rust transport adaptations, not Swift ports.
use crate::{
    editor::{Editor, PaintTarget, Tool},
    geometry::Viewport,
    history::Selection,
};
use crate::{
    layer_index::LayerIndex,
    model::{Content, Document, Layer},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustmentEditInfo {
    pub layer_id: String,
    pub kind: crate::adjustment::AdjustmentKind,
    pub levels: crate::adjustment::LevelsSettings,
    pub curves: crate::adjustment::CurvesSettings,
    pub preview: bool,
    pub histogram: Vec<Vec<f64>>,
    pub histogram_peak: Vec<f64>,
    pub curve_samples: Vec<[f64; 2]>,
    pub sample_mode: Option<crate::adjustment::LevelsSample>,
}

impl AdjustmentEditInfo {
    fn of(edit: &crate::editor::AdjustmentEdit) -> Self {
        let histogram_peak = edit
            .histogram
            .iter()
            .map(|bins| crate::adjustment::histogram_display_scale(bins))
            .collect();
        // Engine-evaluated curve polyline for the working channel, so the UI
        // draws presentation geometry without reimplementing Hermite math.
        let mut curve_samples = Vec::new();
        if edit.working.kind == crate::adjustment::AdjustmentKind::Curves {
            let channel = edit.working.curves.channel.index();
            for i in 0..=64 {
                let x = i as f64 * 255. / 64.;
                curve_samples.push([x, edit.working.curves.value(x, channel)]);
            }
        }
        Self {
            layer_id: edit.layer_id.clone(),
            kind: edit.working.kind,
            levels: edit.working.levels.clone(),
            curves: edit.working.curves.clone(),
            preview: edit.preview,
            histogram: edit.histogram.iter().map(|bins| bins.to_vec()).collect(),
            histogram_peak,
            curve_samples,
            sample_mode: edit.sample_mode,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub document: Arc<DocumentInfo>,
    #[serde(skip)]
    layer_positions: Arc<HashMap<String, usize>>,
    pub view_options: crate::placement::ViewOptions,
    pub displayed_guides: Vec<crate::placement::CanvasGuide>,
    pub guide_drag_active: bool,
    pub text_editing: bool,
    pub text_caret_visible: bool,
    pub cursor_map: crate::feedback::CursorMap,
    pub text_selection: TextSelection,
    pub current_text: Arc<LayerInfo>,
    pub text_caret: Option<crate::model::Point>,
    pub history: HistoryInfo,
    pub selection: Selection,
    pub viewport: Viewport,
    pub tool: Tool,
    pub color: String,
    pub blend_preview: Option<(String, crate::model::Blend)>,
    pub background_color: String,
    pub mask_distortion: Option<[crate::model::Point; 4]>,
    pub image_distortion: Option<[crate::model::Point; 4]>,
    pub transform_target: Option<LayerInfo>,
    /// Authoritative group-box capability: the shared transform box (draft while
    /// editing, live otherwise) when the selection is group-transformable, else
    /// none. The UI enables group numeric fields, ratio lock and flips from this
    /// instead of guessing from the selection, so an empty folder or a fully
    /// locked/hidden selection correctly disables them. Bounds always describe a
    /// valid box the engine would actually transform.
    pub group_box: Option<GroupBoxInfo>,
    pub brush_size: f64,
    pub brush_opacity: f64,
    pub brush_hardness: f64,
    pub brush_smoothing: f64,
    pub paint_target: String,
    pub mask_mode: String,
    pub crop_ratio: String,
    pub crop_rect: Option<crate::crop::CropRect>,
    pub pixel_selection_feather: Option<f64>,
    pub marquee_kind: String,
    pub selection_mode: String,
    pub selection_antialiased: bool,
    pub can_modify_selection: bool,
    pub selection_empty: bool,
    pub locks_transform_ratio: bool,
    pub transform_scale_percent: f64,
    pub selection_draft_mode: Option<String>,
    #[serde(skip)]
    pub selection_feedback: Option<crate::feedback::SelectionFeedback>,
    #[serde(skip)]
    pub selection_outline: Option<crate::feedback::SelectionOutline>,
    pub lasso_kind: String,
    pub wand: crate::wand::WandSettings,
    pub transform_active: bool,
    pub selection_draft: bool,
    pub can_copy_pixels: bool,
    pub layer_rows: Arc<Vec<Row>>,
    pub mask_source_ids: Arc<Vec<String>>,
    pub adjustment_edit: Option<AdjustmentEditInfo>,
    pub can_edit_pixels: bool,
    pub can_toggle_clipping: bool,
    pub has_pixel_selection: bool,
    pub pixel_selection_bounds: Option<Value>,
    pub text_edit_requests: u64,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct TextSelection {
    pub anchor: usize,
    pub head: usize,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct DocumentInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub resolution: f64,
    pub layers: Vec<LayerInfo>,
    pub guides: Vec<crate::placement::CanvasGuide>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryInfo {
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_count: usize,
    pub undo_label: String,
    pub redo_label: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub depth: u32,
    pub visible: bool,
    pub collapsed: bool,
    pub can_toggle_clipping: bool,
}
#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupBoxInfo {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}
#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub x: f64,
    pub y: f64,
    pub width: u32,
    pub height: u32,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub flip_x: bool,
    pub flip_y: bool,
    pub opacity: f64,
    pub brightness: f64,
    pub saturation: f64,
    pub blur: f64,
    pub blend: String,
    pub sampling: crate::model::Sampling,
    pub parent_id: Option<String>,
    pub mask_source_id: Option<String>,
    pub adjustment: Option<crate::adjustment::AdjustmentKind>,
    pub content: Value,
    pub text_layout: Option<crate::text::TextLayout>,
    pub mask: Option<Value>,
}
impl LayerInfo {
    fn of(layer: &Layer) -> Self {
        // Never serialize an image, raster mask, or stroke payload to get metadata.
        let content = if matches!(layer.content.as_ref(), Content::Image { .. }) {
            serde_json::json!({"kind":"image"})
        } else {
            serde_json::to_value(layer.content.as_ref()).expect("validated content")
        };
        let mask = layer.mask.as_ref().map(|mask| {
            let mut value =
                serde_json::json!({"enabled":mask.enabled,"base":mask.base,"linked":mask.linked});
            if let Some(placement) = mask.placement {
                value["placement"] = serde_json::to_value(placement).unwrap();
            }
            value
        });
        Self {
            id: layer.id.clone(),
            name: layer.name.clone(),
            visible: layer.visible,
            locked: layer.locked,
            x: layer.x,
            y: layer.y,
            width: layer.width,
            height: layer.height,
            scale_x: layer.scale_x,
            scale_y: layer.scale_y,
            rotation: layer.rotation,
            flip_x: layer.flip_x,
            flip_y: layer.flip_y,
            opacity: layer.opacity,
            brightness: layer.brightness,
            saturation: layer.saturation,
            blur: layer.blur,
            blend: enum_name(layer.blend),
            sampling: layer.sampling,
            parent_id: layer.parent_id.clone(),
            mask_source_id: layer.mask_source_id.clone(),
            adjustment: layer.adjustment.as_ref().map(|a| a.kind),
            content,
            text_layout: layer.text_layout.clone(),
            mask,
        }
    }
    pub fn kind(&self) -> &str {
        self.content["kind"].as_str().unwrap_or("")
    }
    pub fn fill(&self) -> Option<&str> {
        self.content[if self.kind() == "gradient" {
            "from"
        } else {
            "color"
        }]
        .as_str()
    }
}

fn enum_name(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

impl Snapshot {
    /// A one-off capture. Workers retain a SnapshotPublisher between captures.
    pub fn capture(editor: &Editor) -> anyhow::Result<Self> {
        SnapshotPublisher::default().capture(editor)
    }
    pub fn selected(&self) -> Option<&LayerInfo> {
        self.selection.ids.last().and_then(|id| self.layer(id))
    }
    pub fn layer(&self, id: &str) -> Option<&LayerInfo> {
        self.layer_positions
            .get(id)
            .and_then(|&i| self.document.layers.get(i))
    }
    pub fn is_selected(&self, id: &str) -> bool {
        self.selection.ids.iter().any(|selected| selected == id)
    }
    pub fn unlocked_selection(&self) -> bool {
        self.selection
            .ids
            .iter()
            .any(|id| self.layer(id).is_some_and(|l| !l.locked))
    }
    /// Inspector data is independent of viewport, cursor geometry and animation.
    /// Document changes conservatively refresh all controls, including live edits.
    pub fn same_controls(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.document, &other.document)
            && self.selection == other.selection
            && self.tool == other.tool
            && self.paint_target == other.paint_target
            && self.color == other.color
            && self.brush_size == other.brush_size
            && self.brush_opacity == other.brush_opacity
            && self.brush_hardness == other.brush_hardness
            && self.brush_smoothing == other.brush_smoothing
            && self.wand.tolerance == other.wand.tolerance
            && self.wand.radius == other.wand.radius
            && self.crop_ratio == other.crop_ratio
            && self.current_text == other.current_text
            && self.text_editing == other.text_editing
            && self.text_edit_requests == other.text_edit_requests
            && self.transform_target == other.transform_target
            && self.group_box == other.group_box
            && self.transform_scale_percent == other.transform_scale_percent
            && self.adjustment_edit == other.adjustment_edit
            && Arc::ptr_eq(&self.mask_source_ids, &other.mask_source_ids)
    }
}

/// Immutable metadata publication, owned by the engine worker. Compare actual
/// state, including pending gestures; history revision alone misses live edits.
#[derive(Default)]
pub struct SnapshotPublisher {
    previous_document: Option<Document>,
    document: Option<Arc<DocumentInfo>>,
    layer_positions: Arc<HashMap<String, usize>>,
    collapsed: HashSet<String>,
    rows: Arc<Vec<Row>>,
    target: Option<String>,
    mask_sources: Arc<Vec<String>>,
    target_can_toggle_clipping: bool,
    text: Option<(Layer, Arc<LayerInfo>)>,
}
impl SnapshotPublisher {
    pub fn capture(&mut self, editor: &Editor) -> anyhow::Result<Snapshot> {
        let doc = &editor.history.document;
        let changed = self
            .previous_document
            .as_ref()
            .is_none_or(|old| !old.retained_eq(doc));
        let rows_changed = changed || self.collapsed != editor.collapsed_groups;
        let target = editor.selected_id();
        let sources_changed = changed || self.target.as_deref() != target;
        if changed {
            self.document = Some(Arc::new(DocumentInfo {
                name: doc.name.clone(),
                width: doc.width,
                height: doc.height,
                resolution: doc.resolution,
                layers: doc.layers.iter().map(LayerInfo::of).collect(),
                guides: doc.guides.clone(),
            }));
            self.layer_positions = Arc::new(
                doc.layers
                    .iter()
                    .enumerate()
                    .map(|(i, l)| (l.id.clone(), i))
                    .collect(),
            );
            self.previous_document = Some(doc.clone());
        }
        if rows_changed || sources_changed {
            let index = LayerIndex::new(doc);
            if rows_changed {
                self.rows = Arc::new(
                    editor
                        .layer_rows_indexed(&index)
                        .into_iter()
                        .map(|row| Row {
                            id: row.id,
                            depth: row.depth,
                            visible: row.visible,
                            collapsed: row.collapsed,
                            can_toggle_clipping: row.can_toggle_clipping,
                        })
                        .collect(),
                );
                self.collapsed = editor.collapsed_groups.clone();
            }
            if sources_changed {
                self.mask_sources = Arc::new(
                    doc.layers
                        .iter()
                        .filter(|l| target.is_some_and(|t| index.can_link(&l.id, t)))
                        .map(|l| l.id.clone())
                        .collect(),
                );
                self.target = target.map(str::to_owned);
                self.target_can_toggle_clipping = editor.selected().is_some_and(|l| {
                    !l.locked
                        && (l.mask_source_id.is_some() || index.clipping_source(&l.id).is_some())
                });
            }
        }
        let text = editor.current_text();
        if self.text.as_ref().is_none_or(|(old, _)| old != text) {
            self.text = Some((text.clone(), Arc::new(LayerInfo::of(text))));
        }
        let selection = editor.history.pixel_selection.as_ref();
        let history = editor.history.info();
        Ok(Snapshot {
            document: self.document.as_ref().unwrap().clone(),
            layer_positions: self.layer_positions.clone(),
            view_options: editor.view_options.clone(),
            displayed_guides: editor.displayed_guides(),
            guide_drag_active: editor.guide_drag.is_some(),
            text_editing: editor.text_editing(),
            text_caret_visible: editor.text_caret_visible,
            cursor_map: editor.cursor_map(),
            text_selection: TextSelection {
                anchor: editor.text_selection.0,
                head: editor.text_selection.1,
            },
            current_text: self.text.as_ref().unwrap().1.clone(),
            text_caret: if editor.text_editing() {
                editor.selected().map(|l| {
                    let count = match l.content.as_ref() {
                        Content::Text { text, .. } => text.len(),
                        _ => 0,
                    };
                    let c = crate::text::caret_with_affinity(
                        l,
                        editor.text_selection.1.min(count),
                        editor.text_upstream,
                    );
                    crate::geometry::to_world(
                        l,
                        crate::model::Point::new(c.left as f64, c.top as f64),
                    )
                })
            } else {
                None
            },
            history: HistoryInfo {
                dirty: history.dirty,
                can_undo: history.can_undo,
                can_redo: history.can_redo,
                undo_count: history.undo_count,
                undo_label: history.undo_label,
                redo_label: history.redo_label,
            },
            selection: editor.selection.clone(),
            viewport: editor.viewport.clone(),
            tool: editor.tool,
            color: editor.color.clone(),
            blend_preview: editor.blend_preview.clone(),
            background_color: editor.background_color.clone(),
            mask_distortion: editor.mask_distortion_corners(),
            image_distortion: editor.image_distortion.as_ref().map(|d| d.corners),
            transform_target: editor
                .independent_mask_layer()
                .or_else(|| editor.selected().cloned())
                .map(|l| LayerInfo::of(&l))
                .map(|single| {
                    // Numeric fields describe the shared box for a folder or
                    // multi-selection (`TransformInspector` over the group draft).
                    // Mask placement keeps its own target.
                    if editor.paint_target == PaintTarget::Content
                        && let Some(box_layer) = editor.edited_group_box()
                    {
                        LayerInfo::of(&box_layer)
                    } else {
                        single
                    }
                }),
            group_box: editor.edited_group_box().map(|b| GroupBoxInfo {
                x: b.x,
                y: b.y,
                width: b.width as f64 * b.scale_x,
                height: b.height as f64 * b.scale_y,
                rotation: b.rotation,
            }),
            brush_size: editor.brush_size,
            brush_opacity: editor.brush_opacity,
            brush_hardness: editor.brush_hardness,
            brush_smoothing: editor.brush_smoothing,
            paint_target: enum_name(editor.paint_target),
            mask_mode: enum_name(editor.mask_mode),
            crop_ratio: enum_name(editor.crop_ratio),
            crop_rect: editor.crop_rect,
            pixel_selection_feather: selection.map(|s| s.feather),
            marquee_kind: enum_name(editor.marquee_kind),
            selection_mode: enum_name(editor.selection_mode),
            selection_antialiased: editor.selection_antialiased,
            can_modify_selection: editor.can_modify_selection(),
            selection_empty: selection.is_some_and(|s| s.outline.is_empty()),
            locks_transform_ratio: editor.locks_transform_ratio,
            transform_scale_percent: editor.transform_scale_percent(),
            selection_draft_mode: editor.selection_draft().map(|d| enum_name(d.mode)),
            selection_feedback: editor.selection_feedback(),
            selection_outline: editor.selection_outline(),
            lasso_kind: enum_name(editor.lasso_kind),
            wand: editor.wand,
            transform_active: editor.transform_active(),
            selection_draft: editor.polygon.is_some(),
            can_copy_pixels: editor.can_copy_pixels(),
            layer_rows: self.rows.clone(),
            mask_source_ids: self.mask_sources.clone(),
            adjustment_edit: editor.adjustment_edit.as_ref().map(AdjustmentEditInfo::of),
            can_edit_pixels: editor.can_edit_pixels(),
            can_toggle_clipping: self.target_can_toggle_clipping && editor.selection.ids.len() == 1,
            has_pixel_selection: selection.is_some(),
            pixel_selection_bounds: selection
                .and_then(|s| s.bounds.as_ref())
                .map(|b| serde_json::to_value(b).unwrap()),
            text_edit_requests: editor.text_edit_requests,
        })
    }
}
