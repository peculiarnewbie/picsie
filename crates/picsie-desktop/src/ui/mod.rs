//! UI port of src/ui/*.tsx. Editing semantics remain in picsie-core.
mod controls;
mod dialogs;
mod files;
mod input;
mod layout;
pub(crate) mod menus;
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
        select::{Select, SelectEvent, SelectItem, SelectState},
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
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Action {
    Command(Command),
    File(FileAction),
    New,
    CanvasSize,
    Color(ColorTarget),
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
type ChoiceState = Entity<SelectState<Vec<Choice>>>;
#[derive(Clone)]
struct LayerDrag {
    origin: Point<Pixels>,
    id: String,
    moved: bool,
    select_on_click: bool,
    destination: Option<(String, &'static str)>,
}

pub struct Desktop {
    engine: Engine,
    state: Option<Snapshot>,
    state_sequence: u64,
    image: Option<Arc<RenderImage>>,
    bounds: Bounds<Pixels>,
    physical_size: (u32, u32),
    focus: FocusHandle,
    inspector_scroll: ScrollHandle,
    modal_focus: FocusHandle,
    dragging: bool,
    layer_drag: Option<LayerDrag>,
    fields: HashMap<&'static str, Entity<InputState>>,
    text: Entity<TextareaState>,
    text_editing: bool,
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
        let engine = Engine::start(document);
        let focus = cx.focus_handle();
        let modal_focus = cx.focus_handle();
        window.focus(&focus, cx);
        let fields = FIELD_KEYS
            .iter()
            .map(|key| (*key, cx.new(|cx| InputState::new(window, cx))))
            .collect();
        let text = cx.new(|cx| TextareaState::new(window, cx).submit_on_enter(true));
        let mut selects = HashMap::new();
        for (key, options) in [
            ("blend", BLENDS),
            ("font", FONTS),
            ("mask-source", &[][..]),
            ("canvas-unit", UNITS),
            ("canvas-fill", FILLS),
        ] {
            let items = options
                .iter()
                .map(|(value, label)| Choice {
                    value: (*value).into(),
                    label: (*label).into(),
                })
                .collect::<Vec<_>>();
            selects.insert(key, cx.new(|cx| SelectState::new(items, None, window, cx)));
        }
        let sliders = [
            ("opacity", 100., 1.),
            ("brightness", 300., 1.),
            ("saturation", 300., 1.),
            ("blur", 100., 0.5),
        ]
        .into_iter()
        .map(|(key, max, step)| {
            (
                key,
                cx.new(|_| SliderState::new().min(0.).max(max).step(step)),
            )
        })
        .collect();
        let pump = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
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
            engine,
            state: None,
            state_sequence: 0,
            image: None,
            bounds: Bounds::default(),
            physical_size: (0, 0),
            focus,
            inspector_scroll: ScrollHandle::new(),
            modal_focus,
            dragging: false,
            layer_drag: None,
            fields,
            text,
            text_editing: false,
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
                self.send(command);
                window.focus(&self.focus, cx);
            }
            Action::File(action) => self.file_action(action, window, cx),
            Action::New => self.open_new(window, cx),
            Action::CanvasSize => self.open_canvas_size(window, cx),
            Action::Color(target) => self.open_color(target, window, cx),
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
                        "Saved .comp package; live shapes, gradients, and text were rasterized"
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
                Ok(Outcome::Sampled) => {}
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
        let result = self.engine.latest.lock().unwrap().take();
        if let Some(result) = result {
            match result {
                Err(error) => {
                    self.notice = error;
                    self.busy = false;
                }
                Ok(frame) => {
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
        self.state = Some(state);
        self.sync_controls(changed, window, cx);
        if edit_text {
            // Color, optional selection, layers, properties, masks, transform, text.
            let index = 5 + usize::from(self.state.as_ref().is_some_and(|s| {
                s.has_pixel_selection || matches!(s.tool, Tool::Marquee | Tool::Lasso)
            }));
            self.inspector_scroll.scroll_to_item(index);
            self.text.update(cx, |input, cx| input.focus(window, cx));
            self.notice = "Editing text · changes are applied live".into();
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
                "modal":self.modal.as_ref().map(Modal::name),"path":self.path,"controls":*self.probes.lock().unwrap(),
                "metrics":self.frame_metrics,"queue_to_paint_ms":self.frame_metrics["queue_to_paint_ms"]});
            let temporary = path.with_extension("tmp");
            if std::fs::write(&temporary, record.to_string()).is_ok() {
                let _ = std::fs::rename(temporary, path);
            }
        }
    }
}
