//! Presentation preferences corresponding to ContentView AppStorage and ToolDefaults.
//! Document content remains authoritative in picsie-core, never in this file.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Default, Serialize, Deserialize)]
pub struct Preferences {
    pub color_picker_position: Option<[f32; 2]>,
    pub shows_sample_ring: Option<bool>,
    pub layers_width: Option<f32>,
    pub view_options: Option<picsie_core::placement::ViewOptions>,
}
fn path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root =
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")));
    root.map(|p| p.join("picsie/ui.json"))
}
pub fn load() -> Preferences {
    path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}
pub fn update(change: impl FnOnce(&mut Preferences)) {
    let Some(path) = path() else {
        return;
    };
    let mut prefs = load();
    change(&mut prefs);
    // A failed preference write does not interrupt editing or modify the document.
    let result = (|| -> anyhow::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(&prefs)?)?;
        std::fs::rename(temporary, &path)?;
        Ok(())
    })();
    let _ = result;
}
