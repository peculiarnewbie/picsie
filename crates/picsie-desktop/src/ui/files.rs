use super::*;
use picsie_core::files;
#[derive(Clone, Copy)]
pub(super) enum FileAction {
    Open,
    OpenComp,
    Import,
    Save,
    SaveAs,
    SaveComp,
    ExportPng,
    ExportJpeg,
}
impl Desktop {
    pub(super) fn file_action(
        &mut self,
        action: FileAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send(Command::FinishGesture);
        if matches!(
            action,
            FileAction::Open | FileAction::OpenComp | FileAction::Import
        ) {
            self.busy = true;
            let receiver = cx.prompt_for_paths(PathPromptOptions {
                files: !matches!(action, FileAction::OpenComp),
                directories: matches!(action, FileAction::OpenComp),
                multiple: matches!(action, FileAction::Import),
                prompt: Some(
                    match action {
                        FileAction::Import => "Import PNG, JPEG, or WebP images",
                        FileAction::OpenComp => "Open Compositor package",
                        _ => "Open Picsie or legacy project",
                    }
                    .into(),
                ),
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = receiver.await.map_err(anyhow::Error::from).and_then(|r| r);
                let _ = this.update_in(cx, |this, window, cx| {
                    this.busy = false;
                    match result {
                        Ok(Some(paths)) => {
                            if matches!(action, FileAction::Import) {
                                this.import_paths(paths, cx)
                            } else if let Some(path) = paths.into_iter().next() {
                                this.open_project(path, window, cx)
                            }
                        }
                        Ok(None) => {}
                        Err(error) => this.notice = error.to_string(),
                    }
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        if matches!(action, FileAction::Save)
            && let Some(path) = self.path.clone()
        {
            self.write_file(action, path, cx);
            return;
        }
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let extension = match action {
            FileAction::SaveComp => "comp",
            FileAction::ExportPng => "png",
            FileAction::ExportJpeg => "jpg",
            _ => "picsie",
        };
        let name = format!("{}.{extension}", state.document.name);
        let directory = self
            .path
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        self.busy = true;
        let receiver = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let result = receiver.await.map_err(anyhow::Error::from).and_then(|r| r);
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok(Some(mut path)) => {
                        let suffix = path
                            .extension()
                            .map(|s| s.to_string_lossy().to_ascii_lowercase())
                            .unwrap_or_default();
                        let accepted = match action {
                            FileAction::ExportJpeg => suffix == "jpg" || suffix == "jpeg",
                            FileAction::Save | FileAction::SaveAs => {
                                ["picsie", "electropic", "comp"].contains(&suffix.as_str())
                            }
                            _ => suffix == extension,
                        };
                        if !accepted {
                            path = PathBuf::from(format!("{}.{extension}", path.to_string_lossy()));
                        }
                        this.write_file(action, path, cx);
                    }
                    Ok(None) => {
                        this.closing = false;
                        this.close_after_save = false;
                        menus::cancel_quit(cx);
                    }
                    Err(error) => {
                        this.notice = error.to_string();
                        this.closing = false;
                        this.close_after_save = false;
                        menus::cancel_quit(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn write_file(&mut self, action: FileAction, path: PathBuf, cx: &mut Context<Self>) {
        self.busy = true;
        let operation = match action {
            FileAction::ExportPng | FileAction::ExportJpeg => Operation::Export {
                path,
                jpeg: matches!(action, FileAction::ExportJpeg),
            },
            _ => Operation::Save(path),
        };
        self.blocking(operation);
        cx.notify();
    }
    fn open_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        let source = path.clone();
        let task = cx
            .background_executor()
            .spawn(async move { files::open_project(&source) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok(document) => {
                        if let Err(error) = crate::open_editor(document, Some(path), cx) {
                            this.notice = error.to_string();
                        }
                    }
                    Err(error) => this.notice = error.to_string(),
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn import_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if !paths.is_empty() {
            self.busy = true;
            self.blocking(Operation::Import(paths));
            cx.notify();
        }
    }
    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.closing {
            return;
        }
        self.commit_active_fields(window, cx);
        self.closing = true;
        self.busy = true;
        self.blocking(Operation::Barrier);
        cx.notify();
    }
    pub(super) fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.as_ref().is_none_or(|state| !state.history.dirty) {
            window.remove_window();
            return;
        }
        self.modal = Some(Modal::Close);
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }
}
