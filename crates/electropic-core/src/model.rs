//! Electropic v1 compatibility model. Compositor placement semantics: see docs/compositor-port.md.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};
use ts_rs::TS;

pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_PIXELS: u64 = 24_000_000;
pub const MAX_FILE_BYTES: u64 = 96 * 1024 * 1024;
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn bounded(self) -> Self {
        Self::new(
            self.x.clamp(-100000., 100000.),
            self.y.clamp(-100000., 100000.),
        )
    }
    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Content {
    Paint,
    Shape {
        shape: Shape,
        color: String,
    },
    Gradient {
        from: String,
        to: String,
    },
    Text {
        text: String,
        #[serde(rename = "fontSize")]
        font_size: f64,
        #[serde(rename = "fontFamily")]
        font_family: FontFamily,
        color: String,
    },
    Image {
        #[ts(skip)]
        data: Arc<str>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    Rectangle,
    Ellipse,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FontFamily {
    SansSerif,
    Serif,
    Monospace,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Blend {
    SourceOver,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    ColorDodge,
    ColorBurn,
    Hue,
    Saturation,
    Color,
    Luminosity,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum StrokeMode {
    Paint,
    Erase,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MaskMode {
    Hide,
    Reveal,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub mode: StrokeMode,
    pub color: String,
    pub size: f64,
    pub opacity: f64,
    pub points: Vec<Point>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaskStroke {
    pub mode: MaskMode,
    pub size: f64,
    pub opacity: f64,
    pub points: Vec<Point>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct LayerMask {
    pub enabled: bool,
    pub base: MaskMode,
    #[ts(skip)]
    pub strokes: Vec<Arc<MaskStroke>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub width: u32,
    pub height: u32,
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub flip_x: bool,
    pub flip_y: bool,
    pub opacity: f64,
    pub blend: Blend,
    pub brightness: f64,
    pub saturation: f64,
    pub blur: f64,
    pub content: Arc<Content>,
    #[ts(skip)]
    pub strokes: Vec<Arc<Stroke>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mask: Option<Arc<LayerMask>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Document {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
}
pub fn dimensions(width: u32, height: u32) -> Result<()> {
    ensure!(
        (1..=MAX_DIMENSION).contains(&width) && (1..=MAX_DIMENSION).contains(&height),
        "Dimensions must be between 1 and 8192 pixels"
    );
    ensure!(
        width as u64 * height as u64 <= MAX_PIXELS,
        "Surface exceeds the 24 megapixel limit"
    );
    Ok(())
}
pub fn color_valid(s: &str) -> bool {
    (s.len() == 7 || s.len() == 9)
        && s.starts_with('#')
        && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn range(v: f64, min: f64, max: f64) -> bool {
    v.is_finite() && (min..=max).contains(&v)
}
fn stroke_valid(size: f64, opacity: f64, points: &[Point]) -> bool {
    range(size, 1., 1000.)
        && range(opacity, 0., 1.)
        && !points.is_empty()
        && points.len() <= 100000
        && points
            .iter()
            .all(|p| range(p.x, -100000., 100000.) && range(p.y, -100000., 100000.))
}
impl Layer {
    pub fn new(name: &str, width: u32, height: u32, content: Content) -> Self {
        Self {
            id: id(),
            name: name.into(),
            visible: true,
            locked: false,
            width,
            height,
            x: 0.,
            y: 0.,
            scale_x: 1.,
            scale_y: 1.,
            rotation: 0.,
            flip_x: false,
            flip_y: false,
            opacity: 1.,
            blend: Blend::SourceOver,
            brightness: 1.,
            saturation: 1.,
            blur: 0.,
            content: Arc::new(content),
            strokes: vec![],
            mask: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        dimensions(self.width, self.height)?;
        ensure!(uuid::Uuid::parse_str(&self.id).is_ok(), "Invalid layer ID");
        ensure!(
            !self.name.is_empty() && self.name.chars().count() <= 200,
            "Invalid layer name"
        );
        ensure!(
            range(self.x, -100000., 100000.)
                && range(self.y, -100000., 100000.)
                && range(self.scale_x, 0.01, 100.)
                && range(self.scale_y, 0.01, 100.)
                && range(self.rotation, -360., 360.),
            "Invalid layer transform"
        );
        ensure!(
            range(self.opacity, 0., 1.)
                && range(self.brightness, 0., 3.)
                && range(self.saturation, 0., 3.)
                && range(self.blur, 0., 100.),
            "Invalid layer appearance"
        );
        match self.content.as_ref() {
            Content::Paint => (),
            Content::Shape { color, .. } => ensure!(color_valid(color), "Invalid color"),
            Content::Gradient { from, to } => ensure!(
                color_valid(from) && color_valid(to),
                "Invalid gradient color"
            ),
            Content::Text {
                text,
                color,
                font_size,
                ..
            } => ensure!(
                text.chars().count() <= 20000 && color_valid(color) && range(*font_size, 1., 1000.),
                "Invalid text"
            ),
            Content::Image { data } => ensure!(
                data.len() <= 90_000_000
                    && data.starts_with("data:image/png;base64,")
                    && data[22..]
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'='),
                "Invalid image asset"
            ),
        }
        ensure!(
            self.strokes.len() <= 10000
                && self
                    .strokes
                    .iter()
                    .all(|s| color_valid(&s.color) && stroke_valid(s.size, s.opacity, &s.points)),
            "Invalid brush stroke"
        );
        if let Some(mask) = &self.mask {
            ensure!(
                mask.strokes.len() <= 10000
                    && mask
                        .strokes
                        .iter()
                        .all(|s| stroke_valid(s.size, s.opacity, &s.points)),
                "Invalid mask stroke"
            );
        }
        Ok(())
    }
    /// UI metadata deliberately excludes all image and brush payloads, including legacy base64.
    pub fn metadata(&self) -> serde_json::Value {
        let mut light = self.clone();
        light.strokes.clear();
        if let Some(mask) = &mut light.mask {
            Arc::make_mut(mask).strokes.clear();
        }
        if matches!(light.content.as_ref(), Content::Image { .. }) {
            light.content = Arc::new(Content::Image { data: "".into() });
        }
        let mut value = serde_json::to_value(light).expect("finite validated layer");
        value.as_object_mut().unwrap().remove("strokes");
        if let Some(mask) = value.get_mut("mask") {
            mask.as_object_mut().unwrap().remove("strokes");
        }
        if let Some(content) = value.get_mut("content") {
            content.as_object_mut().unwrap().remove("data");
        }
        value
    }
}
impl Document {
    pub fn new(name: &str, width: u32, height: u32) -> Result<Self> {
        let doc = Self {
            format: "electropic".into(),
            version: 1,
            name: name.into(),
            width,
            height,
            layers: vec![],
        };
        doc.validate()?;
        Ok(doc)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.format == "electropic" && self.version == 1,
            "Unsupported Electropic project version"
        );
        dimensions(self.width, self.height)?;
        ensure!(
            !self.name.is_empty() && self.name.chars().count() <= 200,
            "Invalid document name"
        );
        ensure!(
            self.layers.len() <= 100,
            "The editor supports up to 100 layers"
        );
        let mut ids = HashSet::new();
        for layer in &self.layers {
            layer.validate()?;
            ensure!(ids.insert(&layer.id), "Layer IDs must be unique");
        }
        Ok(())
    }
    pub fn metadata(&self) -> serde_json::Value {
        serde_json::json!({"format":self.format,"version":self.version,"name":self.name,"width":self.width,"height":self.height,"layers":self.layers.iter().map(Layer::metadata).collect::<Vec<_>>()})
    }
    pub fn replace(&mut self, layer: Layer) {
        if let Some(value) = self.layers.iter_mut().find(|v| v.id == layer.id) {
            *value = layer;
        }
    }
}
pub fn demo_document() -> Document {
    let mut doc = Document::new("Electric studies", 1200, 800).unwrap();
    let mut add = |name: &str, w, h, x, y, content| {
        let mut l = Layer::new(name, w, h, content);
        l.x = x;
        l.y = y;
        doc.layers.push(l);
    };
    add(
        "Midnight",
        1200,
        800,
        0.,
        0.,
        Content::Gradient {
            from: "#111b38".into(),
            to: "#392353".into(),
        },
    );
    add(
        "Electric blue",
        560,
        560,
        560.,
        155.,
        Content::Shape {
            shape: Shape::Ellipse,
            color: "#6587ff".into(),
        },
    );
    add(
        "Coral orbit",
        345,
        345,
        775.,
        70.,
        Content::Shape {
            shape: Shape::Ellipse,
            color: "#f6a484".into(),
        },
    );
    add(
        "Edition",
        700,
        80,
        80.,
        77.,
        Content::Text {
            text: "E L E C T R O P I C   /   0 0 1".into(),
            font_size: 19.,
            font_family: FontFamily::Monospace,
            color: "#a5b4d1".into(),
        },
    );
    add(
        "Make something",
        950,
        265,
        76.,
        278.,
        Content::Text {
            text: "Make something\nelectric.".into(),
            font_size: 94.,
            font_family: FontFamily::SansSerif,
            color: "#fff5e8".into(),
        },
    );
    add(
        "Caption",
        760,
        80,
        80.,
        683.,
        Content::Text {
            text: "A study in color, shape, and possibility.".into(),
            font_size: 23.,
            font_family: FontFamily::SansSerif,
            color: "#adb7cd".into(),
        },
    );
    doc.layers[2].blend = Blend::Screen;
    doc
}
