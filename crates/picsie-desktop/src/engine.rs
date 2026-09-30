//! Desktop transport only. The worker owns the existing editor and all raster/file work.
use crate::state::Snapshot;
use anyhow::{Result, ensure};
use picsie_core::{
    editor::{Command, Editor, PaintTarget, Tool},
    files, geometry,
    model::{Content, Document, Point},
    render::Renderer,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};

pub enum Operation {
    Commands(Vec<Command>),
    Save(PathBuf),
    Export {
        path: PathBuf,
        jpeg: bool,
    },
    Import(Vec<PathBuf>),
    Sample(Point),
    ContentField {
        key: &'static str,
        value: serde_json::Value,
    },
    ForegroundFill {
        key: &'static str,
    },
    BrushStep(f64),
    BrushField {
        key: &'static str,
        value: f64,
    },
    ZoomBy {
        factor: f64,
        point: Option<Point>,
    },
    ResizeCanvas(picsie_core::canvas_size::CanvasSizeOptions),
    Barrier,
}
pub enum Outcome {
    Saved { path: PathBuf, flattened: bool },
    Exported(PathBuf),
    Imported(usize),
    Sampled,
    Barrier,
    Resized,
}
pub struct Completion {
    pub sequence: u64,
    pub result: Result<Outcome, String>,
    pub state: Snapshot,
}
pub struct Frame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub state: Snapshot,
    pub sequence: u64,
    pub queued_at: Instant,
    pub command_ms: f64,
    pub render_ms: f64,
    pub pixels_ms: f64,
}
struct Request {
    operation: Operation,
    sequence: u64,
    queued_at: Instant,
}
enum Message {
    Request(Request),
    FrameConsumed,
}
/// Frame delivery is bounded; completion/error delivery is reliable and ordered.
pub struct Engine {
    sender: mpsc::Sender<Message>,
    pub events: mpsc::Receiver<Completion>,
    latest: Arc<Mutex<Option<Result<Frame, String>>>>,
    /// A single pending wake covers all unread frames and reliable completions.
    pub wakeups: async_channel::Receiver<()>,
    sequence: u64,
}
pub fn preview(editor: &Editor, renderer: &mut Renderer) -> Result<skia_safe::Surface> {
    renderer.preview(
        &editor.history.document,
        &editor.viewport,
        &editor.selection.ids,
        editor.tool == Tool::Move,
        if editor.paint_target == PaintTarget::Mask {
            editor.selected_id()
        } else {
            None
        },
        if editor.tool == Tool::Crop {
            Some(editor.crop_rect.unwrap_or_else(|| {
                picsie_core::crop::CropRect::from_document(&editor.history.document)
            }))
        } else {
            None
        },
        editor.history.pixel_selection.as_ref(),
        editor.selection_draft().as_ref(),
    )
}
/// GPUI RenderImage takes straight-alpha BGRA bytes on all supported backends.
pub fn bgra(surface: &mut skia_safe::Surface) -> Result<Vec<u8>> {
    picsie_core::render::bgra_pixels(surface)
}
fn operate(
    editor: &mut Editor,
    renderer: &mut Renderer,
    operation: Operation,
) -> Result<Option<Outcome>> {
    match operation {
        Operation::Commands(commands) => {
            for command in commands {
                editor.command(command)?;
            }
            Ok(None)
        }
        Operation::Save(path) => {
            editor.finish_gesture()?;
            let revision = editor.history.revision.clone();
            files::save_project(&path, &editor.history.document)?;
            editor.history.mark_saved(revision);
            let flattened = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("comp"))
                && editor.history.document.layers.iter().any(|l| {
                    matches!(
                        l.content.as_ref(),
                        Content::Text { .. } | Content::Shape { .. } | Content::Gradient { .. }
                    )
                });
            Ok(Some(Outcome::Saved { path, flattened }))
        }
        Operation::Export { path, jpeg } => {
            editor.finish_gesture()?;
            let bytes = renderer.export(&editor.history.document, jpeg)?;
            files::atomic_write(&path, &bytes)?;
            Ok(Some(Outcome::Exported(path)))
        }
        Operation::Import(paths) => {
            editor.finish_gesture()?;
            ensure!(
                editor.history.document.layers.len() + paths.len() <= 100,
                "The editor supports up to 100 layers"
            );
            let layers = paths
                .iter()
                .map(|path| files::import_image(path))
                .collect::<Result<Vec<_>>>()?;
            editor.import_layers(layers)?;
            Ok(Some(Outcome::Imported(paths.len())))
        }
        Operation::Sample(point) => {
            let point = geometry::to_document(&editor.history.document, &editor.viewport, point);
            ensure!(
                point.x.is_finite() && point.y.is_finite(),
                "Invalid sample point"
            );
            if point.x >= 0.
                && point.y >= 0.
                && point.x < editor.history.document.width as f64
                && point.y < editor.history.document.height as f64
            {
                let pixel = renderer.sample(&editor.history.document, point)?;
                editor.command(Command::SetColor {
                    color: format!("#{:02x}{:02x}{:02x}", pixel[0], pixel[1], pixel[2]),
                })?;
            }
            Ok(Some(Outcome::Sampled))
        }
        Operation::ContentField { key, value } => {
            if let Some(layer) = editor.selected() {
                let mut content = layer.metadata()["content"].clone();
                content[key] = value;
                editor.command(Command::UpdateLayer {
                    patch: serde_json::json!({"content":content}),
                })?;
            }
            Ok(None)
        }
        Operation::ForegroundFill { key } => {
            let value = editor.color.clone().into();
            operate(editor, renderer, Operation::ContentField { key, value })
        }
        Operation::BrushStep(delta) => {
            let value = (editor.brush_size + delta).clamp(1., 2000.);
            operate(
                editor,
                renderer,
                Operation::BrushField { key: "size", value },
            )
        }
        Operation::BrushField { key, value } => {
            match key {
                "size" => editor.command(Command::SetBrush {
                    size: value,
                    opacity: editor.brush_opacity,
                })?,
                "opacity" => editor.command(Command::SetBrush {
                    size: editor.brush_size,
                    opacity: value,
                })?,
                "hardness" => editor.command(Command::SetBrushTip {
                    hardness: value,
                    smoothing: editor.brush_smoothing,
                })?,
                "smoothing" => editor.command(Command::SetBrushTip {
                    hardness: editor.brush_hardness,
                    smoothing: value,
                })?,
                _ => anyhow::bail!("Unknown brush control"),
            }
            Ok(None)
        }
        Operation::ZoomBy { factor, point } => {
            editor.command(Command::Zoom {
                zoom: editor.viewport.zoom * factor,
                point,
            })?;
            Ok(None)
        }
        Operation::ResizeCanvas(options) => {
            editor.command(Command::ResizeCanvas { options })?;
            Ok(Some(Outcome::Resized))
        }
        Operation::Barrier => {
            editor.finish_gesture()?;
            Ok(Some(Outcome::Barrier))
        }
    }
}
impl Engine {
    pub fn start(document: Document) -> Self {
        let (sender, receiver) = mpsc::channel::<Message>();
        let (events_tx, events) = mpsc::channel();
        let (wake, wakeups) = async_channel::bounded(1);
        let latest = Arc::new(Mutex::new(None));
        let output = latest.clone();
        std::thread::Builder::new()
            .name("picsie-engine".into())
            .spawn(move || {
                let mut editor = match Editor::new(document) {
                    Ok(editor) => editor,
                    Err(error) => {
                        *output.lock().unwrap() = Some(Err(error.to_string()));
                        let _ = wake.try_send(());
                        return;
                    }
                };
                let mut renderer = Renderer::default();
                let mut sequence = 0;
                let mut pending_since = None;
                let mut command_ms = 0.;
                while let Ok(first) = receiver.recv() {
                    let started = Instant::now();
                    // Retain every pointer sample and terminal event. Only presentation is coalesced.
                    let requests = std::iter::once(first).chain(receiver.try_iter().take(4095));
                    for message in requests {
                        let Message::Request(request) = message else {
                            continue;
                        };
                        pending_since.get_or_insert(request.queued_at);
                        sequence = request.sequence;
                        let result = operate(&mut editor, &mut renderer, request.operation);
                        if !matches!(result, Ok(None)) {
                            let result = result
                                .map(|value| value.expect("operation result"))
                                .map_err(|e| e.to_string());
                            match Snapshot::capture(&editor) {
                                Ok(state) => {
                                    let _ = events_tx.send(Completion {
                                        sequence,
                                        result,
                                        state,
                                    });
                                }
                                Err(error) => {
                                    *output.lock().unwrap() = Some(Err(error.to_string()));
                                }
                            }
                            // Completion is observable without waiting for the next preview.
                            let _ = wake.try_send(());
                        }
                    }
                    command_ms += started.elapsed().as_secs_f64() * 1000.;
                    let Some(queued_at) = pending_since else {
                        // A consumed frame with no newer commands needs no replacement.
                        command_ms = 0.;
                        continue;
                    };
                    if output.lock().unwrap().is_some() {
                        // Keep applying commands and publishing reliable outcomes, but
                        // don't prepare another full image until the UI takes this one.
                        continue;
                    }
                    let result = (|| -> Result<Frame> {
                        let started = Instant::now();
                        let mut surface = preview(&editor, &mut renderer)?;
                        let render_ms = started.elapsed().as_secs_f64() * 1000.;
                        let started = Instant::now();
                        let pixels = bgra(&mut surface)?;
                        Ok(Frame {
                            pixels,
                            width: surface.width() as u32,
                            height: surface.height() as u32,
                            pixels_ms: started.elapsed().as_secs_f64() * 1000.,
                            render_ms,
                            command_ms,
                            state: Snapshot::capture(&editor)?,
                            sequence,
                            queued_at,
                        })
                    })()
                    .map_err(|e| e.to_string());
                    *output.lock().unwrap() = Some(result);
                    pending_since = None;
                    command_ms = 0.;
                    // Notify after publishing. A full channel means the UI is already due
                    // to drain both queues; only redundant wakes may be coalesced.
                    let _ = wake.try_send(());
                }
            })
            .expect("start engine worker");
        Self {
            sender,
            events,
            latest,
            wakeups,
            sequence: 0,
        }
    }
    pub fn submitted_sequence(&self) -> u64 {
        self.sequence
    }
    pub fn request(&mut self, operation: Operation) -> u64 {
        self.sequence += 1;
        let _ = self.sender.send(Message::Request(Request {
            operation,
            sequence: self.sequence,
            queued_at: Instant::now(),
        }));
        self.sequence
    }
    pub fn take_frame(&self) -> Option<Result<Frame, String>> {
        let frame = self.latest.lock().unwrap().take();
        if frame.is_some() {
            // Reliable demand wake: deferred final edits must render even when no
            // new pointer/command arrives. This does not advance the edit sequence.
            let _ = self.sender.send(Message::FrameConsumed);
        }
        frame
    }
    pub fn send(&mut self, commands: Vec<Command>) -> u64 {
        self.request(Operation::Commands(commands))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use picsie_core::editor::{Modifiers, Phase, PointerSample};
    use serde_json::json;
    use std::time::Duration;

    fn worker() -> Engine {
        let mut worker = Engine::start(Document::new("Desktop transport", 64, 48).unwrap());
        worker.send(vec![
            Command::ResizeViewport {
                width: 128.,
                height: 128.,
            },
            Command::Fit,
        ]);
        worker
    }
    fn complete(worker: &Engine, sequence: u64) -> Completion {
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(15))
            .expect("worker completion");
        assert_eq!(
            event.sequence, sequence,
            "every completion must be delivered in request order"
        );
        event
    }
    fn flush(worker: &mut Engine) -> Snapshot {
        let sequence = worker.request(Operation::Barrier);
        let event = complete(worker, sequence);
        assert!(matches!(event.result, Ok(Outcome::Barrier)));
        event.state
    }
    fn notifications(worker: &Engine) -> mpsc::Receiver<()> {
        let wakeups = worker.wakeups.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            while wakeups.recv_blocking().is_ok() {
                if sender.send(()).is_err() {
                    break;
                }
            }
        });
        receiver
    }
    #[test]
    fn notifications_deliver_frames_and_failures_then_sleep_and_disconnect() {
        let mut worker = worker();
        let wakeups = notifications(&worker);
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing/document.picsie");
        let sequences: Vec<_> = (0..20)
            .map(|_| worker.request(Operation::Save(missing.clone())))
            .collect();
        let mut completed = Vec::new();
        let mut frame_sequence = 0;
        while completed.len() < sequences.len() || frame_sequence < sequences[19] {
            wakeups
                .recv_timeout(Duration::from_secs(15))
                .expect("worker must wake its consumer");
            for event in worker.events.try_iter() {
                assert!(event.result.is_err());
                completed.push(event.sequence);
            }
            if let Some(frame) = worker.take_frame() {
                frame_sequence = frame.unwrap().sequence;
            }
        }
        assert_eq!(
            completed, sequences,
            "coalescing must retain every file failure"
        );
        // A completion and its frame may each have scheduled a wake. Drain any
        // in-flight notification, then require the idle worker to stay silent.
        let quiet_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match wakeups.recv_timeout(Duration::from_millis(50)) {
                Ok(()) => assert!(Instant::now() < quiet_deadline, "idle worker keeps waking"),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(error) => panic!("worker disconnected early: {error}"),
            }
        }
        drop(worker);
        assert!(matches!(
            wakeups.recv_timeout(Duration::from_secs(15)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }
    #[test]
    fn startup_failure_also_wakes_the_consumer() {
        let mut document = Document::new("Invalid", 64, 48).unwrap();
        document.width = 0;
        let worker = Engine::start(document);
        notifications(&worker)
            .recv_timeout(Duration::from_secs(15))
            .expect("startup failure wake");
        assert!(worker.take_frame().unwrap().is_err());
    }
    #[test]
    fn slow_consumer_preserves_edits_and_gets_final_frame_without_more_input() {
        let mut worker = worker();
        let wakeups = notifications(&worker);
        let initial_sequence = loop {
            wakeups.recv_timeout(Duration::from_secs(15)).unwrap();
            if let Some(frame) = worker.latest.lock().unwrap().as_ref() {
                break frame.as_ref().unwrap().sequence;
            }
        };
        // Leave that frame unconsumed while edits and a reliable barrier complete.
        worker.send(vec![Command::AddGradient]);
        for _ in 0..32 {
            worker.send(vec![Command::Nudge {
                delta: Point::new(1., 0.),
            }]);
        }
        let sequence = worker.request(Operation::Barrier);
        let completion = complete(&worker, sequence);
        assert_eq!(completion.state.selected().unwrap().x, 32.);
        assert_eq!(
            worker
                .latest
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .sequence,
            initial_sequence,
            "a waiting frame must not be repeatedly replaced"
        );
        assert_eq!(
            worker.take_frame().unwrap().unwrap().sequence,
            initial_sequence
        );
        // No new editing command: taking the old frame must wake deferred rendering.
        let final_frame = loop {
            wakeups.recv_timeout(Duration::from_secs(15)).unwrap();
            if let Some(frame) = worker.take_frame() {
                break frame.unwrap();
            }
        };
        assert_eq!(final_frame.sequence, sequence);
        assert_eq!(final_frame.state.selected().unwrap().x, 32.);
        assert_eq!(worker.submitted_sequence(), sequence);
        assert!(worker.take_frame().is_none());
    }
    #[test]
    fn queued_controls_resolve_against_latest_editor_state() {
        let mut worker = worker();
        worker.send(vec![Command::AddGradient]);
        worker.request(Operation::ContentField {
            key: "from",
            value: json!("#112233"),
        });
        worker.request(Operation::ContentField {
            key: "to",
            value: json!("#abcdef"),
        });
        worker.request(Operation::BrushField {
            key: "size",
            value: 30.,
        });
        worker.request(Operation::BrushField {
            key: "opacity",
            value: 0.4,
        });
        worker.request(Operation::BrushField {
            key: "hardness",
            value: 0.2,
        });
        worker.request(Operation::BrushField {
            key: "smoothing",
            value: 75.,
        });
        for _ in 0..5 {
            worker.request(Operation::BrushStep(5.));
        }
        let state = flush(&mut worker);
        assert_eq!(state.selected().unwrap().content["from"], "#112233");
        assert_eq!(state.selected().unwrap().content["to"], "#abcdef");
        assert_eq!(
            (
                state.brush_size,
                state.brush_opacity,
                state.brush_hardness,
                state.brush_smoothing
            ),
            (55., 0.4, 0.2, 75.)
        );
        worker.send(vec![Command::SetColor {
            color: "#fedcba".into(),
        }]);
        worker.request(Operation::ForegroundFill { key: "to" });
        assert_eq!(
            flush(&mut worker).selected().unwrap().content["to"],
            "#fedcba"
        );
    }
    #[test]
    fn property_drag_is_one_undo_even_across_frames() {
        let mut worker = worker();
        worker.send(vec![Command::AddGradient]);
        let before = flush(&mut worker).history.undo_count;
        worker.send(vec![Command::BeginPropertyEdit {
            label: "Edit layer opacity".into(),
        }]);
        for step in 1..=20 {
            worker.send(vec![Command::UpdateLayer {
                patch: json!({"opacity": step as f64 / 25.}),
            }]);
        }
        let after = flush(&mut worker);
        assert_eq!(after.history.undo_count, before + 1);
        assert_eq!(after.selected().unwrap().opacity, 0.8);
        worker.send(vec![Command::Undo]);
        assert_eq!(flush(&mut worker).selected().unwrap().opacity, 1.);
    }
    #[test]
    fn pointer_queue_and_file_completions_survive_frame_coalescing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("drawing.picsie");
        let png = directory.path().join("drawing.png");
        let jpeg = directory.path().join("drawing.jpg");
        let comp = directory.path().join("drawing.comp");
        let mut worker = worker();
        worker.send(vec![
            Command::AddPaintLayer,
            Command::SetTool { tool: Tool::Brush },
        ]);
        for (phase, x) in std::iter::once((Phase::Down, 45.))
            .chain((46..75).map(|x| (Phase::Move, x as f64)))
            .chain(std::iter::once((Phase::Up, 75.)))
        {
            worker.send(vec![Command::Pointer {
                samples: vec![PointerSample {
                    phase,
                    point: Point::new(x, 64.),
                    modifiers: Modifiers::default(),
                }],
            }]);
        }
        let saving = worker.request(Operation::Save(path.clone()));
        let exporting = worker.request(Operation::Export {
            path: png.clone(),
            jpeg: false,
        });
        let jpeg_export = worker.request(Operation::Export {
            path: jpeg.clone(),
            jpeg: true,
        });
        let packaging = worker.request(Operation::Save(comp.clone()));
        let imported = worker.request(Operation::Import(vec![png.clone()]));
        let saved = complete(&worker, saving);
        assert!(matches!(saved.result, Ok(Outcome::Saved { .. })));
        assert!(!saved.state.history.dirty);
        assert_eq!(
            saved.state.history.undo_count, 2,
            "paint layer plus one complete stroke"
        );
        assert!(matches!(
            complete(&worker, exporting).result,
            Ok(Outcome::Exported(_))
        ));
        assert!(matches!(
            complete(&worker, jpeg_export).result,
            Ok(Outcome::Exported(_))
        ));
        assert!(matches!(
            complete(&worker, packaging).result,
            Ok(Outcome::Saved { .. })
        ));
        let result = complete(&worker, imported);
        assert!(matches!(result.result, Ok(Outcome::Imported(1))));
        assert!(result.state.history.dirty);
        let document = files::open_project(&path).unwrap();
        assert_eq!(
            (document.width, document.height, document.layers.len()),
            (64, 48, 1)
        );
        assert_eq!(files::open_project(&comp).unwrap().layers.len(), 1);
        assert_eq!(std::fs::read(&png).unwrap()[..8], *b"\x89PNG\r\n\x1a\n");
        assert_eq!(std::fs::read(&jpeg).unwrap()[..2], [0xff, 0xd8]);
    }
    #[test]
    fn failed_save_does_not_mark_saved_or_break_later_commands() {
        let directory = tempfile::tempdir().unwrap();
        let mut worker = worker();
        worker.send(vec![Command::AddPaintLayer]);
        let sequence = worker.request(Operation::Save(
            directory.path().join("missing/failed.picsie"),
        ));
        let failed = complete(&worker, sequence);
        assert!(failed.result.is_err());
        assert!(failed.state.history.dirty);
        worker.send(vec![Command::Undo]);
        assert!(flush(&mut worker).document.layers.is_empty());
    }
}
