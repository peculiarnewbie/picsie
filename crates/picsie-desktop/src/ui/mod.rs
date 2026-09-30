//! Compositor UI adapted to GPUI Kit. Editing semantics remain in picsie-core.
mod clipboard;
mod controls;
mod dialogs;
mod files;
mod input;
mod layers;
mod layout;
pub(crate) mod menus;
mod placement;
mod polish;
mod text;
mod toolbars;
use crate::{
    engine::{Engine, Operation, Outcome},
    state::{LayerInfo, Snapshot},
};
use controls::*;
use dialogs::{ColorTarget, Modal};
use files::FileAction;
use gpui_kit::{
    component::{
        button::{Button, ButtonCustomVariant, ButtonVariants},
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
        slider::{Slider, SliderEvent, SliderState, SliderValue},
        *,
    },
    prelude::*,
    *,
};
use picsie_core::{
    editor::{Command, Tool},
    model::Document,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone)]
enum Action {
    Command(Command),
    File(FileAction),
    New,
    CanvasSize,
    ImageSize,
    Copy { merged: bool, cut: bool },
    Paste,
    Color(ColorTarget),
    ViewOption(&'static str),
    Close,
}
#[derive(Clone)]
struct Choice {
    value: String,
    label: String,
}
impl SelectItem for Choice {
    type Value = String;
    fn title(&self) -> SharedString {
        self.label.clone().into()
    }
    fn value(&self) -> &String {
        &self.value
    }
}
type ChoiceState = Entity<SelectState<SearchableVec<Choice>>>;
#[derive(Clone)]
struct LayerDrag {
    origin: Point<Pixels>,
    id: String,
    moved: bool,
    copy: bool,
    mask: bool,
    select_on_click: bool,
    destination: Option<(String, &'static str)>,
}

pub struct Desktop {
    ui_entity: WeakEntity<Self>,
    engine: Engine,
    state: Option<Snapshot>,
    state_sequence: u64,
    image: Option<Arc<RenderImage>>,
    thumbnails: HashMap<String, (Arc<picsie_core::thumbnail::Thumbnail>, Arc<RenderImage>)>,
    bounds: Bounds<Pixels>,
    physical_size: (u32, u32),
    display_scale: f32,
    guide_dragging: bool,
    text_input_sequence: u64,
    last_text_selection: (usize, usize),
    cursor_position: Option<picsie_core::model::Point>,
    cursor_modifiers: picsie_core::editor::Modifiers,
    cursor_hint: picsie_core::feedback::CursorHint,
    caret_blink: Option<Task<()>>,
    caret_last_input: Instant,
    numeric_edit: Option<&'static str>,
    focus: FocusHandle,
    layers_width: f32,
    panel_drag: Option<(f32, f32)>,
    text_palette_open: bool,
    open_palette: Option<&'static str>,
    menu_focus: Option<FocusHandle>,
    menu_text_focus: bool,
    modal_focus: FocusHandle,
    dragging: bool,
    layer_drag: Option<LayerDrag>,
    rename_layer: Option<String>,
    visibility_swiping: bool,
    list_pointer: Option<Point<Pixels>>,
    blend_index: usize,
    blend_layer: Option<String>,
    blend_scroll: ScrollHandle,
    blend_focus: FocusHandle,
    list_scroll: ScrollHandle,
    list_autoscroll: Option<Task<()>>,
    fields: HashMap<&'static str, Entity<InputState>>,
    text: Entity<TextareaState>,
    selects: HashMap<&'static str, ChoiceState>,
    sliders: HashMap<&'static str, Entity<SliderState>>,
    editing_slider: Option<&'static str>,
    edited_fields: HashSet<&'static str>,
    selected_id: Option<String>,
    notice: String,
    busy: bool,
    pending_operation: Option<u64>,
    path: Option<PathBuf>,
    modal: Option<Modal>,
    color_position: [f32; 2],
    color_panel_drag: Option<(Point<Pixels>, [f32; 2])>,
    sampling_color: bool,
    sample_original: String,
    sample_current: String,
    sample_point: Option<Point<Pixels>>,
    shows_sample_ring: bool,
    closing: bool,
    close_after_save: bool,
    sequence: u64,
    painted_sequence: u64,
    queued_at: Instant,
    frame_metrics: Value,
    probes: Arc<Mutex<HashMap<String, [f32; 4]>>>,
    trace: Option<PathBuf>,
    _subscriptions: Vec<Subscription>,
    _pump: Task<()>,
}
impl Desktop {
    pub fn new(
        document: Document,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if std::env::var_os("PICSIE_GPU_DIAGNOSTICS").is_some() {
            eprintln!("Picsie GPU: {:?}", window.gpu_specs());
        }
        let mut engine = Engine::start(document);
        let preferences = crate::preferences::load();
        if let Some(options) = preferences.view_options {
            engine.send(vec![Command::SetViewOptions { options }]);
        }
        let focus = cx.focus_handle();
        let modal_focus = cx.focus_handle();
        window.focus(&focus, cx);
        let fields: HashMap<_, _> = FIELD_KEYS
            .iter()
            .map(|key| {
                (
                    *key,
                    cx.new(|cx| {
                        InputState::new(window, cx).placeholder(if *key == "leading" {
                            "Auto"
                        } else {
                            ""
                        })
                    }),
                )
            })
            .collect();
        let text = cx.new(|cx| {
            TextareaState::new(window, cx)
                .submit_on_enter(true)
                .soft_wrap(false)
        });
        let mut selects = HashMap::new();
        for (key, options) in [
            ("blend", BLENDS),
            ("font", FONTS),
            ("mask-source", &[][..]),
            ("canvas-unit", UNITS),
            ("canvas-fill", FILLS),
            ("image-unit", IMAGE_UNITS),
            ("image-sampling", SAMPLING),
            ("wand-sample", WAND_SAMPLES),
            (
                "crop-ratio",
                &[
                    ("free", "Free"),
                    ("original", "Original"),
                    ("square", "1:1"),
                    ("fourThree", "4:3"),
                    ("sixteenNine", "16:9"),
                ][..],
            ),
        ] {
            let items = options
                .iter()
                .map(|(value, label)| Choice {
                    value: (*value).into(),
                    label: (*label).into(),
                })
                .collect::<Vec<_>>();
            selects.insert(
                key,
                cx.new(|cx| {
                    SelectState::new(SearchableVec::new(items), None, window, cx)
                        .searchable(key == "font")
                }),
            );
        }
        selects["font"].update(cx, |select, cx| {
            select.set_items(
                picsie_core::text::installed_fonts()
                    .into_iter()
                    .map(|name| Choice {
                        value: name.clone(),
                        label: name,
                    })
                    .collect::<Vec<_>>()
                    .into(),
                window,
                cx,
            );
        });
        let sliders = [
            ("opacity", 100., 1.),
            ("brightness", 300., 1.),
            ("saturation", 300., 1.),
            ("blur", 100., 0.5),
            ("hardness", 100., 1.),
            ("brush-opacity", 100., 1.),
            ("smoothing", 100., 1.),
        ]
        .into_iter()
        .map(|(key, max, step)| {
            (
                key,
                cx.new(|_| SliderState::new().min(0.).max(max).step(step)),
            )
        })
        .collect();
        let wakeups = engine.wakeups.clone();
        let pump = cx.spawn_in(window, async move |this, cx| {
            while wakeups.recv().await.is_ok() {
                if this
                    .update_in(cx, |this, window, cx| this.receive(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let trace = std::env::var_os("PICSIE_TRACE_DIR").map(|directory| {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let directory = PathBuf::from(directory);
            let _ = std::fs::create_dir_all(&directory);
            directory.join(format!("window-{id}.json"))
        });
        let mut this = Self {
            ui_entity: cx.weak_entity(),
            engine,
            state: None,
            state_sequence: 0,
            image: None,
            thumbnails: HashMap::new(),
            bounds: Bounds::default(),
            physical_size: (0, 0),
            display_scale: 1.,
            guide_dragging: false,
            text_input_sequence: 0,
            last_text_selection: (0, 0),
            cursor_position: None,
            cursor_modifiers: picsie_core::editor::Modifiers::default(),
            cursor_hint: picsie_core::feedback::CursorHint::Arrow,
            caret_blink: None,
            caret_last_input: Instant::now(),
            numeric_edit: None,
            focus,
            layers_width: preferences
                .layers_width
                .filter(|v| v.is_finite())
                .unwrap_or(252.)
                .clamp(202., 352.),
            panel_drag: None,
            text_palette_open: false,
            open_palette: None,
            menu_focus: None,
            menu_text_focus: false,
            modal_focus,
            dragging: false,
            layer_drag: None,
            rename_layer: None,
            visibility_swiping: false,
            list_pointer: None,
            blend_index: 0,
            blend_layer: None,
            blend_scroll: ScrollHandle::default(),
            blend_focus: cx.focus_handle(),
            list_scroll: ScrollHandle::default(),
            list_autoscroll: None,
            fields,
            text,
            selects,
            sliders,
            editing_slider: None,
            edited_fields: HashSet::new(),
            selected_id: None,
            notice: "Drag to move · Handles resize · Shift keeps proportions".into(),
            busy: false,
            pending_operation: None,
            path,
            modal: None,
            color_position: preferences.color_picker_position.unwrap_or([90., 125.]),
            color_panel_drag: None,
            sampling_color: false,
            sample_original: "#000000".into(),
            sample_current: "#000000".into(),
            sample_point: None,
            shows_sample_ring: preferences.shows_sample_ring.unwrap_or(true),
            closing: false,
            close_after_save: false,
            sequence: 0,
            painted_sequence: 0,
            queued_at: Instant::now(),
            frame_metrics: Value::Null,
            probes: Arc::new(Mutex::new(HashMap::new())),
            trace,
            _subscriptions: vec![],
            _pump: pump,
        };
        this.bind_controls(window, cx);
        this._subscriptions
            .push(cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    if this.dragging {
                        this.dragging = false;
                        this.send(Command::CancelGesture);
                    }
                    this.layer_drag = None;
                    this.panel_drag = None;
                    this.commit_active_fields(window, cx);
                    cx.notify();
                }
            }));
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let _ = weak.update(cx, |this, cx| this.request_close(window, cx));
            false
        });
        this
    }
    fn send(&mut self, command: Command) {
        self.engine.send(vec![command]);
    }
    fn blocking(&mut self, operation: Operation) {
        self.busy = true;
        self.pending_operation = Some(self.engine.request(operation));
    }
    fn patch(&mut self, value: Value) {
        self.send(Command::UpdateLayer { patch: value });
    }
    fn transform_patch(&mut self, patch: Value) {
        let mut commands = Vec::new();
        if !self.state.as_ref().is_some_and(|s| s.transform_active) {
            commands.push(Command::BeginTransform);
        }
        commands.push(Command::UpdateLayer { patch });
        self.engine.send(commands);
    }
    fn act(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.commit_active_fields(window, cx);
        match action {
            Action::Command(command) => {
                let command = match command {
                    Command::ExpandSelection { .. } => Command::ExpandSelection {
                        amount: number(&self.field_value("selection-amount", cx))
                            .unwrap_or(5.)
                            .clamp(1., 500.) as u32,
                    },
                    Command::ContractSelection { .. } => Command::ContractSelection {
                        amount: number(&self.field_value("selection-amount", cx))
                            .unwrap_or(5.)
                            .clamp(1., 500.) as u32,
                    },
                    Command::FeatherSelection { .. } => Command::FeatherSelection {
                        amount: number(&self.field_value("feather", cx))
                            .unwrap_or(2.)
                            .clamp(1., 250.) as u32,
                    },
                    command => command,
                };
                let updating_text = matches!(&command, Command::UpdateText { .. });
                if matches!(
                    self.open_palette,
                    Some("mask-foreground" | "mask-background")
                ) {
                    self.open_palette = None;
                }
                if let Command::UpdateLayer { patch } = &command
                    && (patch.get("flipX").is_some() || patch.get("flipY").is_some())
                {
                    self.transform_patch(patch.clone());
                } else {
                    self.send(command);
                }

                if self.state.as_ref().is_some_and(|s| s.text_editing) && updating_text {
                    self.text.update(cx, |input, cx| input.focus(window, cx));
                } else {
                    window.focus(&self.focus, cx);
                }
                // The worker's completion wake publishes the new state and image.
                // Invalidating now paints the old frame while that work is in flight.
                return;
            }
            Action::File(action) => self.file_action(action, window, cx),
            Action::New => self.open_new(window, cx),
            Action::CanvasSize => self.open_canvas_size(window, cx),
            Action::ImageSize => self.open_image_size(window, cx),
            Action::Copy { merged, cut } => self.blocking(Operation::Copy { merged, cut }),
            Action::Paste => self.paste_clipboard(window, cx),
            Action::Color(target) => self.open_color(target, window, cx),
            Action::ViewOption(key) => self.toggle_view_option(key),
            Action::Close => self.request_close(window, cx),
        }
        cx.notify();
    }
    fn receive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events = self.engine.events.try_iter().collect::<Vec<_>>();
        for event in events {
            self.apply_state(event.state, event.sequence, window, cx);
            let completed = self.pending_operation == Some(event.sequence);
            if completed {
                self.pending_operation = None;
                self.busy = false;
            }
            match event.result {
                Err(error) => {
                    self.notice = error;
                    if completed {
                        self.closing = false;
                        self.close_after_save = false;
                        menus::cancel_quit(cx);
                    }
                }
                Ok(Outcome::Saved { path, flattened }) => {
                    self.path = Some(path);
                    self.notice = if flattened {
                        "Saved .comp package; shapes, gradients, and translucent text were rasterized"
                            .into()
                    } else {
                        "Saved project".into()
                    };
                    if self.close_after_save {
                        window.remove_window();
                        return;
                    }
                }
                Ok(Outcome::Exported(path)) => {
                    self.notice = format!(
                        "Exported {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )
                }
                Ok(Outcome::Imported(count)) => {
                    self.notice = format!(
                        "Imported {count} image{}",
                        if count == 1 { "" } else { "s" }
                    )
                }
                Ok(Outcome::Sampled { color, picker }) => {
                    if let Some(color) = color {
                        self.sample_current = color.clone();
                        if picker && let Some(Modal::Color(d)) = &mut self.modal {
                            if let Some(rgb) = dialogs::color_rgb(&color) {
                                d.hsb.set_rgb(rgb);
                            }
                            self.sync_color_fields(window, cx);
                        }
                    }
                }
                Ok(Outcome::Copied { bytes, origin }) => self.store_clipboard(bytes, origin, cx),
                Ok(Outcome::Pasted) => self.notice = "Pasted image".into(),
                Ok(Outcome::ImageResized) => {
                    self.notice = "Image resized".into();
                    self.cancel_modal(window, cx);
                }
                Ok(Outcome::Resized) => {
                    self.notice = "Canvas resized".into();
                    self.cancel_modal(window, cx);
                }
                Ok(Outcome::Barrier) => {
                    if self.closing {
                        self.confirm_close(window, cx);
                    }
                }
            }
            cx.notify();
        }
        let result = self.engine.take_frame();
        if let Some(result) = result {
            match result {
                Err(error) => {
                    self.notice = error;
                    self.busy = false;
                }
                Ok(frame) => {
                    self.thumbnails.retain(|id, (_, image)| {
                        let keep = frame.thumbnails.iter().any(|(new, _)| new == id);
                        if !keep {
                            let _ = window.drop_image(image.clone());
                        }
                        keep
                    });
                    for (id, thumbnail) in frame.thumbnails {
                        if self
                            .thumbnails
                            .get(&id)
                            .is_some_and(|(old, _)| Arc::ptr_eq(old, &thumbnail))
                        {
                            continue;
                        }
                        let image = image::RgbaImage::from_raw(
                            thumbnail.width,
                            thumbnail.height,
                            thumbnail.pixels.clone(),
                        )
                        .expect("native thumbnail dimensions");
                        let image = Arc::new(RenderImage::new(vec![image::Frame::new(image)]));
                        if let Some((_, old)) = self.thumbnails.insert(id, (thumbnail, image)) {
                            let _ = window.drop_image(old);
                        }
                    }
                    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels)
                        .expect("native frame dimensions");
                    let image = Arc::new(RenderImage::new(vec![image::Frame::new(image)]));
                    if let Some(old) = self.image.replace(image) {
                        let _ = window.drop_image(old);
                    }
                    self.sequence = frame.sequence;
                    self.queued_at = frame.queued_at;
                    self.frame_metrics = json!({"command_ms":frame.command_ms,"render_ms":frame.render_ms,"pixels_ms":frame.pixels_ms,"width":frame.width,"height":frame.height});
                    self.apply_state(frame.state, frame.sequence, window, cx);
                }
            }
            cx.notify();
        }
    }
    fn apply_state(
        &mut self,
        state: Snapshot,
        sequence: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if sequence < self.state_sequence {
            return;
        }
        self.state_sequence = sequence;
        let cancel_picker = matches!(&self.modal, Some(Modal::Color(draft)) if state.paint_target == "mask"
            || (matches!(draft.target, ColorTarget::Layer) && draft.text_id.as_deref() != state.selected().map(|l| l.id.as_str())));
        if cancel_picker {
            self.cancel_modal(window, cx);
        }
        let changed = self.selected_id.as_deref() != state.selection.ids.last().map(String::as_str);
        let edit_text = self
            .state
            .as_ref()
            .is_some_and(|previous| state.text_edit_requests > previous.text_edit_requests);
        self.selected_id = state.selection.ids.last().cloned();
        window.set_window_title(&format!(
            "{}{} — Picsie",
            state.document.name,
            if state.history.dirty { " •" } else { "" }
        ));
        let tool_changed = self
            .state
            .as_ref()
            .is_none_or(|old| old.tool != state.tool || old.lasso_kind != state.lasso_kind);
        self.state = Some(state);
        self.refresh_cursor(cx);
        self.sync_caret_blink(window, cx);
        if tool_changed {
            self.notice = self.tool_hint();
        }
        self.sync_controls(changed, window, cx);
        if self.state.as_ref().is_some_and(|s| s.text_editing)
            && sequence >= self.text_input_sequence
        {
            let state = self.state.as_ref().unwrap();
            let content = state.current_text.content["text"].as_str().unwrap_or("");
            if self.text.read(cx).value().as_str() == content {
                let selection = (state.text_selection.anchor, state.text_selection.head);
                let range = self.text.read(cx).selected_range();
                if range.start != selection.0.min(selection.1)
                    || range.end != selection.0.max(selection.1)
                    || self.text.read(cx).cursor() != selection.1
                {
                    self.last_text_selection = selection;
                    self.text.update(cx, |input, cx| {
                        input.set_selected_range(selection.0..selection.1, cx)
                    });
                }
            }
        }
        if edit_text {
            self.text_palette_open = false;
            self.text.update(cx, |input, cx| input.focus(window, cx));
            self.notice = "Editing on canvas · Enter to finish · Shift+Enter for a new line · Escape to cancel".into();
        }
    }
    fn trace_paint(&mut self) {
        if self.trace.is_some() && self.painted_sequence != self.sequence {
            self.painted_sequence = self.sequence;
            self.frame_metrics["queue_to_paint_ms"] =
                json!(self.queued_at.elapsed().as_secs_f64() * 1000.);
        }
        if let Some(path) = &self.trace {
            let record = json!({"state":self.state,"sequence":self.sequence,"submittedSequence":self.engine.submitted_sequence(),"busy":self.busy,"notice":self.notice,
                "modal":self.modal.as_ref().map(Modal::name),"colorPickerPosition":self.color_position,"colorWorking":self.modal.as_ref().and_then(|m| if let Modal::Color(d)=m {Some(format!("#{}",d.hsb.hex()))}else{None}),"renameLayer":self.rename_layer,"listScroll":f32::from(self.list_scroll.offset().y),"blendIndex":self.blend_index,"samplingColor":self.sampling_color,"sampleOriginal":self.sample_original,"sampleCurrent":self.sample_current,"showsSampleRing":self.shows_sample_ring,"palette":self.open_palette,"textPalette":self.text_palette_open,"path":self.path,"controls":*self.probes.lock().unwrap(),
                "cursorHint":self.cursor_hint,"metrics":self.frame_metrics,"queue_to_paint_ms":self.frame_metrics["queue_to_paint_ms"]});
            let temporary = path.with_extension("tmp");
            if std::fs::write(&temporary, record.to_string()).is_ok() {
                let _ = std::fs::rename(temporary, path);
            }
        }
    }
}
