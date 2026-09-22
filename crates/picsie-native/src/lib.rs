//! Node-API transport only. Engine state and all long-running raster/file work stay in Rust.
use napi::{Env, Task, bindgen_prelude::*};
use napi_derive::napi;
use picsie_core::{
    editor::{Command, Editor, InitialDocument, PaintTarget, Tool},
    files::{self, Frames},
    geometry::{self, Viewport},
    model::{Document, Layer, Point, demo_document},
    render::Renderer,
};
use std::{
    path::Path,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};
fn error(e: impl std::fmt::Display) -> Error {
    Error::from_reason(e.to_string())
}
fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    m.lock().map_err(|_| error("Engine worker failed"))
}
struct Shared {
    editor: Mutex<Editor>,
    renderer: Mutex<Renderer>,
    frames: Mutex<Frames>,
    closed: AtomicBool,
}
impl Shared {
    fn new(document: Document) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            editor: Mutex::new(Editor::new(document).map_err(error)?),
            renderer: Mutex::new(Renderer::default()),
            frames: Mutex::new(Frames::new().map_err(error)?),
            closed: AtomicBool::new(false),
        }))
    }
    fn check(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            Err(error("Editor is closed"))
        } else {
            Ok(())
        }
    }
}
#[napi]
pub struct NativeEditor {
    shared: Option<Arc<Shared>>,
}
#[napi]
impl NativeEditor {
    #[napi(constructor)]
    pub fn new(options: String) -> Result<Self> {
        let init: InitialDocument = serde_json::from_str(&options).map_err(error)?;
        let doc = match init {
            InitialDocument::Demo => demo_document(),
            InitialDocument::New {
                name,
                width,
                height,
            } => Document::new(&name, width, height).map_err(error)?,
        };
        Ok(Self {
            shared: Some(Shared::new(doc)?),
        })
    }
    #[napi]
    pub fn snapshot(&self) -> Result<String> {
        let shared = self.shared()?;
        Ok(lock(&shared.editor)?.snapshot().to_string())
    }
    #[napi]
    pub fn dispatch(&self, command: String) -> Result<String> {
        if command.len() > 1024 * 1024 {
            return Err(error("Command exceeds 1 MB"));
        }
        let shared = self.shared()?;
        let command: Command = serde_json::from_str(&command).map_err(error)?;
        if matches!(command, Command::ResizeCanvas { .. }) {
            return Err(error("Use the asynchronous canvas resize API"));
        }
        let mut editor = lock(&shared.editor)?;
        editor.command(command).map_err(error)?;
        Ok(editor.snapshot().to_string())
    }
    #[napi(ts_return_type = "Promise<string>")]
    pub fn preview(&self) -> Result<AsyncTask<PreviewTask>> {
        let shared = self.shared()?;
        let editor = lock(&shared.editor)?;
        let task = PreviewTask {
            shared: shared.clone(),
            document: editor.history.document.clone(),
            viewport: editor.viewport.clone(),
            selection: editor.selection.ids.clone(),
            handles: editor.tool == Tool::Move,
            mask_id: if editor.paint_target == PaintTarget::Mask {
                editor.selected_id().map(str::to_owned)
            } else {
                None
            },
        };
        Ok(AsyncTask::new(task))
    }
    #[napi(ts_return_type = "Promise<string>")]
    pub fn save(&self, path: String) -> Result<AsyncTask<SaveTask>> {
        let shared = self.shared()?;
        let editor = lock(&shared.editor)?;
        Ok(AsyncTask::new(SaveTask {
            shared: shared.clone(),
            document: editor.history.document.clone(),
            revision: editor.history.revision.clone(),
            path,
        }))
    }
    #[napi(ts_return_type = "Promise<void>")]
    pub fn export_image(&self, path: String, jpeg: bool) -> Result<AsyncTask<ExportTask>> {
        let shared = self.shared()?;
        let document = lock(&shared.editor)?.history.document.clone();
        Ok(AsyncTask::new(ExportTask {
            shared,
            document,
            path,
            jpeg,
        }))
    }
    #[napi(ts_return_type = "Promise<string>")]
    pub fn import_images(&self, paths: Vec<String>) -> Result<AsyncTask<ImportTask>> {
        let shared = self.shared()?;
        let e = lock(&shared.editor)?;
        if e.history.document.layers.len() + paths.len() > 100 {
            return Err(error("The editor supports up to 100 layers"));
        }
        let revision = e.history.revision.clone();
        drop(e);
        Ok(AsyncTask::new(ImportTask {
            shared,
            paths,
            revision,
        }))
    }
    #[napi(ts_return_type = "Promise<string | null>")]
    pub fn sample_color(&self, x: f64, y: f64) -> Result<AsyncTask<SampleTask>> {
        let shared = self.shared()?;
        let e = lock(&shared.editor)?;
        let point = geometry::to_document(&e.history.document, &e.viewport, Point::new(x, y));
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(error("Invalid sample point"));
        }
        Ok(AsyncTask::new(SampleTask {
            shared: shared.clone(),
            document: e.history.document.clone(),
            point,
        }))
    }
    #[napi(ts_return_type = "Promise<string>")]
    pub fn resize_canvas(&self, options: String) -> Result<AsyncTask<CanvasTask>> {
        let options = serde_json::from_str(&options).map_err(error)?;
        let shared = self.shared()?;
        let e = lock(&shared.editor)?;
        Ok(AsyncTask::new(CanvasTask {
            shared: shared.clone(),
            document: e.history.document.clone(),
            revision: e.history.revision.clone(),
            options,
        }))
    }
    #[napi]
    pub fn close(&mut self) {
        if let Some(s) = self.shared.take() {
            s.closed.store(true, Ordering::Release);
        }
    }
}
impl NativeEditor {
    fn shared(&self) -> Result<Arc<Shared>> {
        let s = self
            .shared
            .as_ref()
            .ok_or_else(|| error("Editor is closed"))?;
        s.check()?;
        Ok(s.clone())
    }
}
#[napi(ts_return_type = "Promise<NativeEditor>")]
pub fn open_editor(path: String) -> AsyncTask<OpenTask> {
    AsyncTask::new(OpenTask { path })
}
pub struct OpenTask {
    path: String,
}
impl Task for OpenTask {
    type Output = Document;
    type JsValue = NativeEditor;
    fn compute(&mut self) -> Result<Document> {
        files::open_project(Path::new(&self.path)).map_err(error)
    }
    fn resolve(&mut self, _: Env, doc: Document) -> Result<NativeEditor> {
        Ok(NativeEditor {
            shared: Some(Shared::new(doc)?),
        })
    }
}
pub struct PreviewTask {
    shared: Arc<Shared>,
    document: Document,
    viewport: Viewport,
    selection: Vec<String>,
    handles: bool,
    mask_id: Option<String>,
}
impl Task for PreviewTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<String> {
        self.shared.check()?;
        let mut surface = lock(&self.shared.renderer)?
            .preview(
                &self.document,
                &self.viewport,
                &self.selection,
                self.handles,
                self.mask_id.as_deref(),
            )
            .map_err(error)?;
        let bytes = picsie_core::render::frame_bytes(&mut surface).map_err(error)?;
        self.shared.check()?;
        lock(&self.shared.frames)?.publish(&bytes).map_err(error)
    }
    fn resolve(&mut self, _: Env, path: String) -> Result<String> {
        self.shared.check()?;
        Ok(path)
    }
}
pub struct SaveTask {
    shared: Arc<Shared>,
    document: Document,
    revision: String,
    path: String,
}
impl Task for SaveTask {
    type Output = ();
    type JsValue = String;
    fn compute(&mut self) -> Result<()> {
        self.shared.check()?;
        files::save_project(Path::new(&self.path), &self.document).map_err(error)
    }
    fn resolve(&mut self, _: Env, _: ()) -> Result<String> {
        self.shared.check()?;
        let mut e = lock(&self.shared.editor)?;
        e.history.mark_saved(self.revision.clone());
        Ok(e.snapshot().to_string())
    }
}
pub struct ExportTask {
    shared: Arc<Shared>,
    document: Document,
    path: String,
    jpeg: bool,
}
impl Task for ExportTask {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> Result<()> {
        self.shared.check()?;
        let bytes = lock(&self.shared.renderer)?
            .export(&self.document, self.jpeg)
            .map_err(error)?;
        self.shared.check()?;
        files::atomic_write(Path::new(&self.path), &bytes).map_err(error)
    }
    fn resolve(&mut self, _: Env, _: ()) -> Result<()> {
        self.shared.check()
    }
}
pub struct ImportTask {
    shared: Arc<Shared>,
    paths: Vec<String>,
    revision: String,
}
impl Task for ImportTask {
    type Output = Vec<Layer>;
    type JsValue = String;
    fn compute(&mut self) -> Result<Vec<Layer>> {
        self.shared.check()?;
        self.paths
            .iter()
            .map(|p| files::import_image(Path::new(p)).map_err(error))
            .collect()
    }
    fn resolve(&mut self, _: Env, layers: Vec<Layer>) -> Result<String> {
        self.shared.check()?;
        let mut e = lock(&self.shared.editor)?;
        if e.history.revision != self.revision {
            return Err(error("Document changed during import; please retry"));
        }
        e.import_layers(layers).map_err(error)?;
        Ok(e.snapshot().to_string())
    }
}
pub struct SampleTask {
    shared: Arc<Shared>,
    document: Document,
    point: Point,
}
impl Task for SampleTask {
    type Output = Option<String>;
    type JsValue = Option<String>;
    fn compute(&mut self) -> Result<Option<String>> {
        self.shared.check()?;
        if self.point.x < 0.
            || self.point.y < 0.
            || self.point.x >= self.document.width as f64
            || self.point.y >= self.document.height as f64
        {
            return Ok(None);
        }
        let p = lock(&self.shared.renderer)?
            .sample(&self.document, self.point)
            .map_err(error)?;
        Ok(Some(format!("#{:02x}{:02x}{:02x}", p[0], p[1], p[2])))
    }
    fn resolve(&mut self, _: Env, value: Option<String>) -> Result<Option<String>> {
        self.shared.check()?;
        Ok(value)
    }
}

pub struct CanvasTask {
    shared: Arc<Shared>,
    document: Document,
    revision: String,
    options: picsie_core::canvas_size::CanvasSizeOptions,
}
impl Task for CanvasTask {
    type Output = Document;
    type JsValue = String;
    fn compute(&mut self) -> Result<Document> {
        self.shared.check()?;
        picsie_core::canvas_size::resize_canvas(&self.document, &self.options).map_err(error)
    }
    fn resolve(&mut self, _: Env, document: Document) -> Result<String> {
        self.shared.check()?;
        let mut e = lock(&self.shared.editor)?;
        if e.history.revision != self.revision {
            return Err(error("Document changed during canvas resize; please retry"));
        }
        if document != e.history.document {
            e.edit("Canvas Size", document, None).map_err(error)?;
            e.fit();
        }
        Ok(e.snapshot().to_string())
    }
}
