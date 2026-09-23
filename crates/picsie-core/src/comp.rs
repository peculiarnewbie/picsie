//! Compositor ProjectStore.swift package format (v8), pinned 609dbeae; MIT © 2026 Wonder Assembly LLC.
//! Picsie's bounded raster editor imports the shared raster/folder/mask subset and rejects
//! unsupported live effects, adjustments, shapes, text, guides, and linked mask sources.
use crate::{
    files::bounded_read,
    model::*,
    render::{self, Renderer},
};
use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Cursor, Write},
    path::Path,
    sync::Arc,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    version: u32,
    #[serde(rename = "colorSpace")]
    color_space: String,
    #[serde(default)]
    resolution: Option<f64>,
    document_id: String,
    width: u32,
    height: u32,
    #[serde(default)]
    active_layer_id: Option<String>,
    layers: Vec<Record>,
    #[serde(default)]
    guides: Option<serde_json::Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    id: String,
    name: String,
    #[serde(rename = "isVisible")]
    is_visible: bool,
    transform: Transform,
    image_file: Option<String>,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    is_group: Option<bool>,
    #[serde(default)]
    opacity: Option<f64>,
    #[serde(default)]
    blend_mode: Option<String>,
    #[serde(default)]
    mask_file: Option<String>,
    #[serde(default)]
    mask_enabled: Option<bool>,
    #[serde(default)]
    mask_placement: Option<Transform>,
    #[serde(default)]
    mask_linked: Option<bool>,
    #[serde(default)]
    mask_source_id: Option<String>,
    #[serde(default)]
    adjustment: Option<serde_json::Value>,
    #[serde(default)]
    shape: Option<serde_json::Value>,
    #[serde(default)]
    effects: Option<serde_json::Value>,
    #[serde(default)]
    text: Option<serde_json::Value>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Size {
    width: f64,
    height: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transform {
    origin: Point,
    size: Size,
    #[serde(default)]
    rotation: f64,
    #[serde(default)]
    flip_x: bool,
    #[serde(default)]
    flip_y: bool,
    #[serde(default = "default_sampling")]
    sampling: String,
}
fn default_sampling() -> String {
    "High quality".into()
}
impl Transform {
    fn valid(&self) -> bool {
        self.origin.x.is_finite()
            && self.origin.y.is_finite()
            && self.origin.x.abs() <= 1_000_000.
            && self.origin.y.abs() <= 1_000_000.
            && self.size.width.is_finite()
            && self.size.height.is_finite()
            && (1. ..=300_000.).contains(&self.size.width)
            && (1. ..=300_000.).contains(&self.size.height)
            && self.rotation.is_finite()
            && ["Nearest", "Smooth", "High quality"].contains(&self.sampling.as_str())
    }
    fn for_layer(layer: &Layer) -> Self {
        Self {
            origin: Point::new(layer.x, layer.y),
            size: Size {
                width: layer.width as f64 * layer.scale_x,
                height: layer.height as f64 * layer.scale_y,
            },
            rotation: layer.rotation,
            flip_x: layer.flip_x,
            flip_y: layer.flip_y,
            sampling: match layer.sampling {
                Sampling::Nearest => "Nearest",
                Sampling::Smooth => "Smooth",
                Sampling::High => "High quality",
            }
            .into(),
        }
    }
    fn for_mask(layer: &Layer, placement: MaskPlacement) -> Self {
        Self {
            origin: Point::new(placement.x, placement.y),
            size: Size {
                width: layer.width as f64 * placement.scale_x,
                height: layer.height as f64 * placement.scale_y,
            },
            rotation: placement.rotation,
            flip_x: placement.flip_x,
            flip_y: placement.flip_y,
            sampling: "High quality".into(),
        }
    }
    fn placement(&self, layer: &Layer) -> MaskPlacement {
        MaskPlacement {
            x: self.origin.x,
            y: self.origin.y,
            scale_x: self.size.width / layer.width as f64,
            scale_y: self.size.height / layer.height as f64,
            rotation: self.rotation,
            flip_x: self.flip_x,
            flip_y: self.flip_y,
        }
    }
}
fn uuid(value: &str) -> Result<String> {
    Ok(uuid::Uuid::parse_str(value)?.to_string())
}
fn filename(id: &str, mask: bool) -> Result<String> {
    let value = uuid::Uuid::parse_str(id)?.to_string().to_uppercase();
    Ok(format!("{}{}.png", value, if mask { ".mask" } else { "" }))
}
fn blend_from(value: &str) -> Result<Blend> {
    Ok(match value {
        "Normal" => Blend::SourceOver,
        "Multiply" => Blend::Multiply,
        "Screen" => Blend::Screen,
        "Overlay" => Blend::Overlay,
        "Darken" => Blend::Darken,
        "Lighten" => Blend::Lighten,
        "Soft Light" => Blend::SoftLight,
        "Hard Light" => Blend::HardLight,
        "Difference" => Blend::Difference,
        "Exclusion" => Blend::Exclusion,
        "Color Dodge" => Blend::ColorDodge,
        "Color Burn" => Blend::ColorBurn,
        "Hue" => Blend::Hue,
        "Saturation" => Blend::Saturation,
        "Color" => Blend::Color,
        "Luminosity" => Blend::Luminosity,
        _ => bail!("Unsupported Compositor blend mode: {value}"),
    })
}
fn blend_to(value: Blend) -> &'static str {
    match value {
        Blend::SourceOver => "Normal",
        Blend::Multiply => "Multiply",
        Blend::Screen => "Screen",
        Blend::Overlay => "Overlay",
        Blend::Darken => "Darken",
        Blend::Lighten => "Lighten",
        Blend::SoftLight => "Soft Light",
        Blend::HardLight => "Hard Light",
        Blend::Difference => "Difference",
        Blend::Exclusion => "Exclusion",
        Blend::ColorDodge => "Color Dodge",
        Blend::ColorBurn => "Color Burn",
        Blend::Hue => "Hue",
        Blend::Saturation => "Saturation",
        Blend::Color => "Color",
        Blend::Luminosity => "Luminosity",
    }
}
fn asset(path: &Path, name: &str) -> Result<Vec<u8>> {
    ensure!(
        name.len() <= 50 && !name.contains('/') && !name.contains('\\'),
        "Unsafe Compositor asset name"
    );
    ensure!(
        fs::symlink_metadata(path.join("images"))?
            .file_type()
            .is_dir(),
        "Unsafe Compositor asset directory"
    );
    let path = path.join("images").join(name);
    ensure!(
        fs::symlink_metadata(&path)?.file_type().is_file(),
        "Missing or unsafe Compositor asset"
    );
    let bytes = bounded_read(&path)?;
    ensure!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "Compositor assets must be PNG"
    );
    Ok(bytes)
}
fn decode_mask(bytes: &[u8]) -> Result<MaskRaster> {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
    dimensions(reader.info().width, reader.info().height)?;
    ensure!(
        reader.info().color_type == png::ColorType::Grayscale
            && reader.info().bit_depth == png::BitDepth::Eight,
        "Compositor mask must be 8-bit grayscale"
    );
    let mut buffer = vec![0; reader.output_buffer_size()];
    let output = reader.next_frame(&mut buffer)?;
    ensure!(
        output.color_type == png::ColorType::Grayscale && output.bit_depth == png::BitDepth::Eight,
        "Invalid Compositor mask"
    );
    let raster = MaskRaster {
        width: output.width,
        height: output.height,
        pixels: Arc::new(buffer[..output.buffer_size()].to_vec()),
    };
    raster.validate()?;
    Ok(raster)
}
fn encode_mask(mask: &MaskRaster) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, mask.width, mask.height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(mask.pixels.as_slice())?;
    }
    Ok(bytes)
}
pub fn open(path: &Path) -> Result<Document> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_dir(),
        "A .comp project is a directory package"
    );
    let manifest_file = path.join("manifest.json");
    ensure!(
        fs::symlink_metadata(&manifest_file)?.file_type().is_file(),
        "Missing Compositor manifest"
    );
    let bytes = bounded_read(&manifest_file)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Compositor manifest exceeds 4 MB"
    );
    let manifest: Manifest =
        serde_json::from_slice(&bytes).context("Invalid Compositor manifest")?;
    ensure!(
        manifest.format == "com.compositor.project"
            && (1..=8).contains(&manifest.version)
            && manifest.color_space == "sRGB",
        "Unsupported Compositor project"
    );
    ensure!(
        manifest
            .guides
            .as_ref()
            .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty)),
        "Alignment guides are not yet supported"
    );
    ensure!(
        manifest.layers.len() <= 100,
        "Picsie supports up to 100 layers"
    );
    let id = uuid(&manifest.document_id)?;
    let mut doc = Document::new(
        &path.file_stem().unwrap_or_default().to_string_lossy(),
        manifest.width,
        manifest.height,
    )?;
    doc.id = id;
    doc.resolution = manifest.resolution.unwrap_or(72.);
    for item in &manifest.layers {
        ensure!(item.transform.valid(), "Invalid Compositor transform");
        ensure!(
            item.name.chars().count() <= 200,
            "Compositor layer name exceeds Picsie's limit"
        );
        ensure!(
            item.mask_source_id.is_none()
                && item.adjustment.is_none()
                && item.shape.is_none()
                && item.effects.is_none()
                && item.text.is_none(),
            "This Compositor project uses live features Picsie cannot edit yet"
        );
        ensure!(
            !item.is_group.unwrap_or(false) || item.image_file.is_none(),
            "Folder cannot have an image asset"
        );
        ensure!(
            !item.is_group.unwrap_or(false) || item.mask_file.is_none(),
            "Folder masks are not yet supported"
        );
        let normalized = uuid(&item.id)?;
        let image = if let Some(name) = &item.image_file {
            ensure!(
                *name == filename(&item.id, false)?,
                "Invalid Compositor image filename"
            );
            let bytes = asset(path, name)?;
            let image = render::decode(&bytes)?;
            Some((bytes, image.width() as u32, image.height() as u32))
        } else {
            None
        };
        let (width, height) = image.as_ref().map(|v| (v.1, v.2)).unwrap_or((
            item.transform.size.width.round().max(1.) as u32,
            item.transform.size.height.round().max(1.) as u32,
        ));
        let content = if item.is_group.unwrap_or(false) {
            Content::Group
        } else if let Some((bytes, _, _)) = &image {
            Content::Image {
                data: Arc::from(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )),
            }
        } else {
            Content::Paint
        };
        let mut layer = Layer::new(&item.name, width, height, content);
        layer.id = normalized;
        layer.parent_id = item.parent_id.as_deref().map(uuid).transpose()?;
        layer.visible = item.is_visible;
        layer.x = item.transform.origin.x;
        layer.y = item.transform.origin.y;
        layer.scale_x = item.transform.size.width / width as f64;
        layer.scale_y = item.transform.size.height / height as f64;
        layer.rotation = item.transform.rotation;
        layer.flip_x = item.transform.flip_x;
        layer.flip_y = item.transform.flip_y;
        layer.sampling = match item.transform.sampling.as_str() {
            "Nearest" => Sampling::Nearest,
            "Smooth" => Sampling::Smooth,
            _ => Sampling::High,
        };
        layer.opacity = item.opacity.unwrap_or(1.);
        layer.blend = blend_from(item.blend_mode.as_deref().unwrap_or("Normal"))?;
        if let Some(name) = &item.mask_file {
            ensure!(
                manifest.version >= if item.is_group.unwrap_or(false) { 6 } else { 4 },
                "Mask requires newer Compositor version"
            );
            ensure!(
                *name == filename(&item.id, true)?,
                "Invalid Compositor mask filename"
            );
            let raster = decode_mask(&asset(path, name)?)?;
            let placement = item
                .mask_placement
                .as_ref()
                .map(|v| {
                    ensure!(v.valid(), "Invalid mask placement");
                    Ok(v.placement(&layer))
                })
                .transpose()?;
            layer.mask = Some(Arc::new(LayerMask {
                enabled: item.mask_enabled.unwrap_or(true),
                base: MaskMode::Reveal,
                raster: Some(Arc::new(raster)),
                linked: item.mask_linked.unwrap_or(true),
                placement,
                strokes: vec![],
            }));
        } else {
            ensure!(
                item.mask_enabled.is_none() && item.mask_placement.is_none(),
                "Mask metadata needs an asset"
            );
        }
        doc.layers.push(layer);
    }
    if let Some(active) = &manifest.active_layer_id {
        ensure!(
            doc.layers
                .iter()
                .any(|l| l.id == uuid(active).unwrap_or_default()),
            "Missing active layer"
        );
    }
    doc.validate()?;
    Ok(doc)
}
pub fn save(path: &Path, doc: &Document) -> Result<()> {
    doc.validate()?;
    ensure!(
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("comp")),
        "Compositor packages use the .comp extension"
    );
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    ensure!(
        !path.exists() || fs::symlink_metadata(path)?.file_type().is_dir(),
        "Existing .comp destination is not a package"
    );
    let stage = tempfile::Builder::new()
        .prefix(".picsie-comp-")
        .tempdir_in(parent)?;
    fs::create_dir(stage.path().join("images"))?;
    let mut renderer = Renderer::default();
    let mut records = Vec::new();
    for layer in &doc.layers {
        let group = matches!(layer.content.as_ref(), Content::Group);
        ensure!(
            !group || layer.mask.is_none(),
            "Folder masks are not yet supported in .comp export"
        );
        let has_image = !group
            && (!matches!(layer.content.as_ref(), Content::Paint) || !layer.strokes.is_empty());
        let image_file = if has_image {
            let name = filename(&layer.id, false)?;
            let mut unmasked = layer.clone();
            unmasked.mask = None;
            let image = renderer.layer_surface(&unmasked)?;
            let bytes = render::encode(&image, false)?;
            fs::write(stage.path().join("images").join(&name), bytes)?;
            Some(name)
        } else {
            None
        };
        let mask_file = if let Some(mask) = &layer.mask {
            let raster = if mask.strokes.is_empty() {
                mask.raster
                    .as_deref()
                    .cloned()
                    .unwrap_or(MaskRaster::solid(mask.base == MaskMode::Reveal))
            } else {
                render::rasterize_mask(mask, layer)?
            };
            let name = filename(&layer.id, true)?;
            fs::write(
                stage.path().join("images").join(&name),
                encode_mask(&raster)?,
            )?;
            Some(name)
        } else {
            None
        };
        records.push(Record {
            id: uuid::Uuid::parse_str(&layer.id)?.to_string().to_uppercase(),
            name: layer.name.clone(),
            is_visible: layer.visible,
            transform: Transform::for_layer(layer),
            image_file,
            parent_id: layer
                .parent_id
                .as_deref()
                .map(|v| uuid::Uuid::parse_str(v).map(|id| id.to_string().to_uppercase()))
                .transpose()?,
            is_group: group.then_some(true),
            opacity: Some(layer.opacity),
            blend_mode: Some(blend_to(layer.blend).into()),
            mask_file,
            mask_enabled: layer.mask.as_ref().map(|m| m.enabled),
            mask_placement: layer
                .mask
                .as_ref()
                .and_then(|m| m.placement)
                .map(|p| Transform::for_mask(layer, p)),
            mask_linked: layer.mask.as_ref().map(|m| m.linked),
            mask_source_id: None,
            adjustment: None,
            shape: None,
            effects: None,
            text: None,
        });
    }
    let manifest = Manifest {
        format: "com.compositor.project".into(),
        version: 8,
        color_space: "sRGB".into(),
        resolution: Some(doc.resolution),
        document_id: uuid::Uuid::parse_str(&doc.id)?.to_string().to_uppercase(),
        width: doc.width,
        height: doc.height,
        active_layer_id: None,
        layers: records,
        guides: None,
    };
    let bytes = serde_json::to_vec_pretty(&manifest)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Compositor manifest exceeds 4 MB"
    );
    let mut file = File::create(stage.path().join("manifest.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    // Windows refuses to rename a directory while a file inside it is still open.
    drop(file);
    let staged = stage.keep();
    let result = (|| -> Result<()> {
        if path.exists() {
            let backup = tempfile::Builder::new()
                .prefix(".picsie-comp-backup-")
                .tempdir_in(parent)?
                .keep();
            fs::remove_dir(&backup)?;
            fs::rename(path, &backup)?;
            if let Err(err) = fs::rename(&staged, path) {
                let _ = fs::rename(&backup, path);
                return Err(err.into());
            }
            let _ = fs::remove_dir_all(&backup);
        } else {
            fs::rename(&staged, path)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staged);
    }
    result
}
