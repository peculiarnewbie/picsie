use crate::{model::*, render};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};
pub fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    let info = file.metadata()?;
    ensure!(
        info.is_file() && info.len() <= MAX_FILE_BYTES,
        "Choose a file smaller than 96 MB"
    );
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "File grew beyond the 96 MB limit"
    );
    Ok(bytes)
}
pub fn parse_project(text: &str) -> Result<Document> {
    ensure!(text.len() as u64 <= MAX_FILE_BYTES, "Project exceeds 96 MB");
    let doc: Document = serde_json::from_str(text).context("Invalid Picsie project")?;
    doc.validate()?;
    for l in &doc.layers {
        if let Content::Image { data } = l.content.as_ref() {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD.decode(&data[22..])?;
            render::decode(&bytes)?;
        }
    }
    Ok(doc)
}
pub fn open_project(path: &Path) -> Result<Document> {
    parse_project(std::str::from_utf8(&bounded_read(path)?)?)
}
/// Same-directory replacement preserves the old file on failed writes. Sync before rename.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn save_project(path: &Path, doc: &Document) -> Result<()> {
    doc.validate()?;
    let bytes = serde_json::to_vec(doc)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "Project exceeds the 96 MB file limit"
    );
    atomic_write(path, &bytes)
}
pub fn import_image(path: &Path) -> Result<Layer> {
    let bytes = bounded_read(path)?;
    ensure!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            || bytes.starts_with(&[255, 216])
            || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP")),
        "Supported image formats are PNG, JPEG, and WebP"
    );
    let image = render::decode(&bytes)?;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(200)
        .collect::<String>();
    Ok(Layer::new(
        &name,
        image.width() as u32,
        image.height() as u32,
        render::png_content(&image)?,
    ))
}
/// Native presentation resources live with the window and are bounded to eight recent frames.
pub struct Frames {
    directory: tempfile::TempDir,
    sequence: u64,
    paths: std::collections::VecDeque<std::path::PathBuf>,
}
impl Frames {
    pub fn new() -> Result<Self> {
        let mut builder = tempfile::Builder::new();
        builder.prefix("picsie-frames-");
        let directory = if cfg!(target_os = "linux") && Path::new("/dev/shm").is_dir() {
            builder
                .tempdir_in("/dev/shm")
                .or_else(|_| builder.tempdir())?
        } else {
            builder.tempdir()?
        };
        Ok(Self {
            directory,
            sequence: 0,
            paths: Default::default(),
        })
    }
    pub fn publish(&mut self, bytes: &[u8]) -> Result<String> {
        self.sequence += 1;
        let path = self
            .directory
            .path()
            .join(format!("{}.tiff", self.sequence));
        fs::write(&path, bytes)?;
        self.paths.push_back(path.clone());
        while self.paths.len() > 8 {
            if let Some(old) = self.paths.pop_front() {
                let _ = fs::remove_file(old);
            }
        }
        Ok(path.to_string_lossy().into_owned())
    }
}
