//! Read-only metadata for the desktop UI. Pixel buffers never enter this snapshot.
use picsie_core::{
    editor::{Editor, Tool},
    geometry::Viewport,
    history::Selection,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub document: DocumentInfo,
    #[serde(skip)]
    layer_positions: std::collections::HashMap<String, usize>,
    pub view_options: picsie_core::placement::ViewOptions,
    pub displayed_guides: Vec<picsie_core::placement::CanvasGuide>,
    pub guide_drag_active: bool,
    pub text_editing: bool,
    pub text_caret_visible: bool,
    pub cursor_map: picsie_core::feedback::CursorMap,
    pub text_selection: TextSelection,
    pub current_text: LayerInfo,
    pub text_caret: Option<picsie_core::model::Point>,
    pub history: HistoryInfo,
    pub selection: Selection,
    pub viewport: Viewport,
    pub tool: Tool,
    pub color: String,
    pub blend_preview: Option<(String, picsie_core::model::Blend)>,
    pub background_color: String,
    pub mask_distortion: Option<[picsie_core::model::Point; 4]>,
    pub image_distortion: Option<[picsie_core::model::Point; 4]>,
    pub transform_target: Option<LayerInfo>,
    pub brush_size: f64,
    pub brush_opacity: f64,
    pub brush_hardness: f64,
    pub brush_smoothing: f64,
    pub paint_target: String,
    pub mask_mode: String,
    pub crop_ratio: String,
    pub crop_rect: Option<picsie_core::crop::CropRect>,
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
    pub selection_feedback: Option<picsie_core::feedback::SelectionFeedback>,
    #[serde(skip)]
    pub selection_outline: Option<picsie_core::feedback::SelectionOutline>,
    pub lasso_kind: String,
    pub wand: picsie_core::wand::WandSettings,
    pub transform_active: bool,
    pub selection_draft: bool,
    pub can_copy_pixels: bool,
    pub layer_rows: Vec<Row>,
    pub mask_source_ids: Vec<String>,
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
    pub guides: Vec<picsie_core::placement::CanvasGuide>,
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
#[derive(Clone, Deserialize, Serialize)]
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
    pub sampling: picsie_core::model::Sampling,
    pub parent_id: Option<String>,
    pub mask_source_id: Option<String>,
    pub content: Value,
    pub text_layout: Option<picsie_core::text::TextLayout>,
    pub mask: Option<Value>,
}
impl Snapshot {
    pub fn capture(editor: &Editor) -> anyhow::Result<Self> {
        let mut snapshot: Self = serde_json::from_value(editor.snapshot())?;
        snapshot.layer_positions = snapshot
            .document
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.clone(), i))
            .collect();
        snapshot.selection_feedback = editor.selection_feedback();
        snapshot.selection_outline = editor.selection_outline();
        Ok(snapshot)
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
        self.document
            .layers
            .iter()
            .any(|layer| self.is_selected(&layer.id) && !layer.locked)
    }
}
impl LayerInfo {
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
