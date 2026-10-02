//! Picsie model with Electropic v1 compatibility. Compositor placement semantics: see docs/compositor-port.md.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};
use ts_rs::TS;

// Project/LayerGroups, Compositor 609dbeae: 10,000 layers. MIT © 2026 Wonder Assembly LLC.
pub const MAX_LAYERS: usize = 10_000;
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
    Group,
    Shape {
        shape: Shape,
        color: String,
        /// Document pixels, fixed when a rounded rectangle is drawn; other shapes ignore it.
        /// LayerShapeStyle.cornerRadius, Compositor 609dbeae. MIT © 2026 Wonder Assembly LLC.
        #[serde(default)]
        corner_radius: f64,
        /// A line's thickness in document pixels; nil on other shapes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        line_width: Option<f64>,
        /// A line's ends as fractions of the layer box (0–1); nil on other shapes and
        /// on older lines, which ran corner to corner instead.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        line_start: Option<Point>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        line_end: Option<Point>,
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
        data: crate::asset::ImageAsset,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    Rectangle,
    Ellipse,
    Line,
}
impl Content {
    /// Fill-only constructor for tests and adapters; style fields take their defaults.
    pub fn shape(shape: Shape, color: String) -> Self {
        Content::Shape {
            shape,
            color: color.into(),
            corner_radius: 0.,
            line_width: None,
            line_start: None,
            line_end: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FontFamily {
    SansSerif,
    Serif,
    Monospace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
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
    LinearBurn,
    LinearDodge,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Subtract,
    Divide,
}
impl Blend {
    /// LayerAppearance.swift: source menu/shortcut order, retaining v1 Normal encoding.
    pub const ALL: [Self; 24] = [
        Self::SourceOver,
        Self::Darken,
        Self::Multiply,
        Self::ColorBurn,
        Self::LinearBurn,
        Self::Lighten,
        Self::Screen,
        Self::ColorDodge,
        Self::LinearDodge,
        Self::Overlay,
        Self::SoftLight,
        Self::HardLight,
        Self::VividLight,
        Self::LinearLight,
        Self::PinLight,
        Self::HardMix,
        Self::Difference,
        Self::Exclusion,
        Self::Subtract,
        Self::Divide,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub enum Sampling {
    Nearest,
    Smooth,
    High,
}
impl Default for Sampling {
    fn default() -> Self {
        Self::High
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub raster: Option<Arc<MaskRaster>>,
    #[serde(default = "default_true")]
    pub linked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub placement: Option<MaskPlacement>,
    #[ts(skip)]
    pub strokes: Vec<Arc<MaskStroke>>,
}
fn default_true() -> bool {
    true
}

/// Immutable 8-bit grayscale coverage. The base64 representation is only used on disk;
/// metadata snapshots omit this field, so no pixels cross the JS bridge.
#[derive(Clone, Debug, PartialEq)]
pub struct MaskRaster {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<Vec<u8>>,
}
impl MaskRaster {
    pub fn solid(reveal: bool) -> Self {
        Self {
            width: 1,
            height: 1,
            pixels: Arc::new(vec![if reveal { 255 } else { 0 }]),
        }
    }
    pub fn validate(&self) -> Result<()> {
        dimensions(self.width, self.height)?;
        ensure!(
            self.pixels.len() == self.width as usize * self.height as usize,
            "Invalid mask asset"
        );
        Ok(())
    }
}
impl Serialize for MaskRaster {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut value = serializer.serialize_struct("MaskRaster", 3)?;
        value.serialize_field("width", &self.width)?;
        value.serialize_field("height", &self.height)?;
        value.serialize_field(
            "data",
            &base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                self.pixels.as_slice(),
            ),
        )?;
        value.end()
    }
}
impl<'de> Deserialize<'de> for MaskRaster {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Stored {
            width: u32,
            height: u32,
            data: String,
        }
        let stored = Stored::deserialize(deserializer)?;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(stored.data)
            .map_err(serde::de::Error::custom)?;
        let raster = Self {
            width: stored.width,
            height: stored.height,
            pixels: Arc::new(bytes),
        };
        raster.validate().map_err(serde::de::Error::custom)?;
        Ok(raster)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MaskPlacement {
    #[serde(default)]
    pub sampling: Sampling,
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub flip_x: bool,
    pub flip_y: bool,
}
impl MaskPlacement {
    pub fn of(layer: &Layer) -> Self {
        Self {
            sampling: layer.sampling,
            x: layer.x,
            y: layer.y,
            scale_x: layer.scale_x,
            scale_y: layer.scale_y,
            rotation: layer.rotation,
            flip_x: layer.flip_x,
            flip_y: layer.flip_y,
        }
    }
    pub fn as_layer(self, source: &Layer) -> Layer {
        let mut layer = source.clone();
        layer.x = self.x;
        layer.y = self.y;
        layer.scale_x = self.scale_x;
        layer.scale_y = self.scale_y;
        layer.rotation = self.rotation;
        layer.flip_x = self.flip_x;
        layer.flip_y = self.flip_y;
        layer.sampling = self.sampling;
        layer
    }
    pub fn valid(self) -> bool {
        range(self.x, -100000., 100000.)
            && range(self.y, -100000., 100000.)
            && range(self.scale_x, 0.01, 100.)
            && range(self.scale_y, 0.01, 100.)
            && range(self.rotation, -360., 360.)
    }
    /// Compositor LayerTransform.following: carry an independently placed mask through a
    /// layer's affine change while dropping shear introduced by uneven scaling.
    pub fn following(self, old: &Layer, new: &Layer) -> Self {
        if old.x == new.x
            && old.y == new.y
            && old.scale_x == new.scale_x
            && old.scale_y == new.scale_y
            && old.rotation == new.rotation
            && old.flip_x == new.flip_x
            && old.flip_y == new.flip_y
        {
            return self;
        }
        if old.scale_x == new.scale_x
            && old.scale_y == new.scale_y
            && old.rotation == new.rotation
            && old.flip_x == new.flip_x
            && old.flip_y == new.flip_y
        {
            return Self {
                x: self.x + new.x - old.x,
                y: self.y + new.y - old.y,
                ..self
            };
        }
        let placed = self.as_layer(old);
        let mapped = |point: Point| {
            let world = crate::geometry::to_world(&placed, point);
            crate::geometry::to_world(new, crate::geometry::to_local(old, world))
        };
        let origin = mapped(Point::new(0., 0.));
        let px = mapped(Point::new(old.width as f64, 0.));
        let py = mapped(Point::new(0., old.height as f64));
        let mid = mapped(Point::new(old.width as f64 / 2., old.height as f64 / 2.));
        let vx = Point::new(px.x - origin.x, px.y - origin.y);
        let vy = Point::new(py.x - origin.x, py.y - origin.y);
        let sign = if self.flip_x { -1. } else { 1. };
        let angle = (vx.y * sign).atan2(vx.x * sign);
        let along = -vy.x * angle.sin() + vy.y * angle.cos();
        let rotation = angle.to_degrees();
        let rotation = rotation + ((self.rotation - rotation) / 360.).round() * 360.;
        Self {
            x: mid.x - vx.x.hypot(vx.y) / 2.,
            y: mid.y - along.abs() / 2.,
            scale_x: vx.x.hypot(vx.y) / old.width as f64,
            scale_y: along.abs() / old.height as f64,
            rotation: ((rotation + 180.).rem_euclid(360.)) - 180.,
            flip_y: along < 0.,
            ..self
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mask_source_id: Option<String>,
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
    #[serde(default)]
    pub sampling: Sampling,
    pub brightness: f64,
    pub saturation: f64,
    pub blur: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub adjustment: Option<crate::adjustment::LayerAdjustment>,
    pub content: Arc<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text_layout: Option<crate::text::TextLayout>,
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
    #[serde(default = "id")]
    #[ts(skip)]
    pub id: String,
    #[serde(default = "default_resolution")]
    pub resolution: f64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub guides: Vec<crate::placement::CanvasGuide>,
}
fn default_resolution() -> f64 {
    72.
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
    /// Cache identity for immutable resources plus every scalar metadata field.
    /// Unlike derived equality, this never walks mask pixels or stroke samples.
    fn retained_eq(&self, other: &Self) -> bool {
        // Exhaustive destructuring forces new fields to join the cache contract.
        let Self {
            id,
            parent_id,
            mask_source_id,
            name,
            visible,
            locked,
            width,
            height,
            x,
            y,
            scale_x,
            scale_y,
            rotation,
            flip_x,
            flip_y,
            opacity,
            blend,
            sampling,
            brightness,
            saturation,
            blur,
            adjustment,
            content,
            text_layout,
            strokes,
            mask,
        } = self;
        id == &other.id
            && parent_id == &other.parent_id
            && mask_source_id == &other.mask_source_id
            && name == &other.name
            && visible == &other.visible
            && locked == &other.locked
            && width == &other.width
            && height == &other.height
            && x == &other.x
            && y == &other.y
            && scale_x == &other.scale_x
            && scale_y == &other.scale_y
            && rotation == &other.rotation
            && flip_x == &other.flip_x
            && flip_y == &other.flip_y
            && opacity == &other.opacity
            && blend == &other.blend
            && sampling == &other.sampling
            && brightness == &other.brightness
            && saturation == &other.saturation
            && blur == &other.blur
            && adjustment == &other.adjustment
            && text_layout == &other.text_layout
            && Arc::ptr_eq(content, &other.content)
            && strokes.len() == other.strokes.len()
            && strokes
                .iter()
                .zip(&other.strokes)
                .all(|(a, b)| Arc::ptr_eq(a, b))
            && match (mask, &other.mask) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
    pub fn new(name: &str, width: u32, height: u32, content: Content) -> Self {
        Self {
            id: id(),
            parent_id: None,
            mask_source_id: None,
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
            sampling: Sampling::High,
            brightness: 1.,
            saturation: 1.,
            blur: 0.,
            adjustment: None,
            content: Arc::new(content),
            text_layout: None,
            strokes: vec![],
            mask: None,
        }
    }
    pub fn validate(&self) -> Result<()> {
        dimensions(self.width, self.height)?;
        if let Some(layout) = &self.text_layout {
            layout.validate()?;
        }
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
        if let Some(adjustment) = &self.adjustment {
            // Levels/Curves adjustment layers, Compositor 609dbeae LayerAdjustment.
            // They carry no pixels of their own; ProjectStore validates the
            // settings but places no geometry constraint, and canvas
            // operations translate them like every other layer. Pixel
            // placement interaction stays forbidden at the command level.
            ensure!(adjustment.is_valid(), "Invalid adjustment settings");
            ensure!(
                matches!(self.content.as_ref(), Content::Paint),
                "Adjustment layers hold no pixels"
            );
            ensure!(
                self.strokes.is_empty() && self.text_layout.is_none(),
                "Adjustment layers hold no pixels"
            );
            ensure!(
                self.brightness == 1. && self.saturation == 1. && self.blur == 0.,
                "Adjustment layers keep default appearance"
            );
        }
        match self.content.as_ref() {
            Content::Paint => (),
            Content::Group => ensure!(
                self.strokes.is_empty()
                    && self.blend == Blend::SourceOver
                    && self.brightness == 1.
                    && self.saturation == 1.
                    && self.blur == 0.,
                "Folders support visibility, opacity, and raster masks"
            ),
            Content::Shape {
                color,
                corner_radius,
                line_width,
                line_start,
                line_end,
                ..
            } => {
                ensure!(color_valid(color), "Invalid color");
                ensure!(
                    corner_radius.is_finite() && (0. ..=5000.).contains(corner_radius),
                    "Invalid shape corner radius"
                );
                if let Some(width) = line_width {
                    ensure!(
                        width.is_finite() && (1. ..=5000.).contains(width),
                        "Invalid line width"
                    );
                }
                for end in [line_start, line_end].into_iter().flatten() {
                    ensure!(
                        end.x.is_finite()
                            && end.y.is_finite()
                            && (0. ..=1.).contains(&end.x)
                            && (0. ..=1.).contains(&end.y),
                        "Invalid line endpoints"
                    );
                }
            }
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
            Content::Image { data } => data.validate()?,
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
            if let Some(raster) = &mask.raster {
                raster.validate()?;
            }
            if let Some(placement) = mask.placement {
                ensure!(placement.valid(), "Invalid mask placement");
            }
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
            Arc::make_mut(mask).raster = None;
        }
        if matches!(light.content.as_ref(), Content::Image { .. }) {
            light.content = Arc::new(Content::Image { data: "".into() });
        }
        let mut value = serde_json::to_value(light).expect("finite validated layer");
        value.as_object_mut().unwrap().remove("strokes");
        if let Some(mask) = value.get_mut("mask") {
            mask.as_object_mut().unwrap().remove("strokes");
            mask.as_object_mut().unwrap().remove("raster");
        }
        if let Some(content) = value.get_mut("content") {
            content.as_object_mut().unwrap().remove("data");
        }
        value
    }
}
impl Document {
    pub(crate) fn retained_eq(&self, other: &Self) -> bool {
        let Self {
            format,
            version,
            id,
            resolution,
            name,
            width,
            height,
            layers,
            guides,
        } = self;
        format == &other.format
            && version == &other.version
            && id == &other.id
            && resolution == &other.resolution
            && name == &other.name
            && width == &other.width
            && height == &other.height
            && guides == &other.guides
            && layers.len() == other.layers.len()
            && layers
                .iter()
                .zip(&other.layers)
                .all(|(a, b)| a.retained_eq(b))
    }
    pub fn new(name: &str, width: u32, height: u32) -> Result<Self> {
        let doc = Self {
            format: "picsie".into(),
            version: 2,
            id: id(),
            resolution: 72.,
            name: name.into(),
            width,
            height,
            layers: vec![],
            guides: vec![],
        };
        doc.validate()?;
        Ok(doc)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (self.format == "picsie" || self.format == "electropic")
                && (1..=2).contains(&self.version),
            "Unsupported Picsie project version"
        );
        dimensions(self.width, self.height)?;
        ensure!(
            self.guides.len() <= 10000
                && self
                    .guides
                    .iter()
                    .all(|g| uuid::Uuid::parse_str(&g.id).is_ok()
                        && g.position.is_finite()
                        && g.position.abs() <= 100000.),
            "Invalid alignment guides"
        );
        ensure!(
            self.guides
                .iter()
                .map(|g| &g.id)
                .collect::<HashSet<_>>()
                .len()
                == self.guides.len(),
            "Duplicate guide IDs"
        );
        ensure!(
            uuid::Uuid::parse_str(&self.id).is_ok(),
            "Invalid document ID"
        );
        ensure!(
            self.resolution.is_finite() && (1. ..=9600.).contains(&self.resolution),
            "Invalid resolution"
        );
        ensure!(
            !self.name.is_empty() && self.name.chars().count() <= 200,
            "Invalid document name"
        );
        ensure!(
            self.layers.len() <= crate::model::MAX_LAYERS,
            "The editor supports up to 10,000 layers"
        );
        let mut ids = HashSet::new();
        for layer in &self.layers {
            layer.validate()?;
            ensure!(
                self.version >= 2
                    || layer
                        .mask
                        .as_ref()
                        .and_then(|m| m.raster.as_ref())
                        .is_none(),
                "Raster masks require project version 2"
            );
            ensure!(ids.insert(&layer.id), "Layer IDs must be unique");
        }
        for layer in &self.layers {
            ensure!(
                self.version >= 2
                    || (layer.parent_id.is_none()
                        && !matches!(layer.content.as_ref(), Content::Group)),
                "Folders require project version 2"
            );
            let mut seen = HashSet::from([layer.id.as_str()]);
            let mut parent = layer.parent_id.as_deref();
            while let Some(id) = parent {
                ensure!(
                    seen.len() <= 64 && seen.insert(id),
                    "Invalid folder cycle or depth"
                );
                let folder = self
                    .layers
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .ok_or_else(|| anyhow::anyhow!("Missing folder"))?;
                ensure!(
                    matches!(folder.content.as_ref(), Content::Group),
                    "Layer parent must be a folder"
                );
                parent = folder.parent_id.as_deref();
            }
        }
        crate::live_mask::validate(self)
    }
    /// Compositor LayerHierarchy.entries: depth-first sibling order, bottom to top.
    pub fn ordered_layers(&self) -> Vec<&Layer> {
        crate::layer_index::LayerIndex::new(self).ordered_layers()
    }
    pub fn effective(&self, layer: &Layer) -> (bool, f64) {
        let mut visible = layer.visible;
        let mut opacity = layer.opacity;
        let mut parent = layer.parent_id.as_deref();
        while let Some(id) = parent {
            if let Some(folder) = self.layers.iter().find(|l| l.id == id) {
                visible &= folder.visible;
                opacity *= folder.opacity;
                parent = folder.parent_id.as_deref();
            } else {
                break;
            }
        }
        (visible, opacity)
    }
    pub fn descendants(&self, id: &str) -> HashSet<String> {
        let mut found = HashSet::new();
        let mut pending = vec![id.to_owned()];
        while let Some(parent) = pending.pop() {
            for child in self
                .layers
                .iter()
                .filter(|l| l.parent_id.as_deref() == Some(&parent))
            {
                if found.insert(child.id.clone()) {
                    pending.push(child.id.clone());
                }
            }
        }
        found
    }
    pub fn metadata(&self) -> serde_json::Value {
        serde_json::json!({"format":self.format,"version":self.version,"name":self.name,"width":self.width,"height":self.height,"resolution":self.resolution,"layers":self.layers.iter().map(Layer::metadata).collect::<Vec<_>>(),"guides":self.guides})
    }
    pub fn replace(&mut self, layer: Layer) {
        if let Some(value) = self.layers.iter_mut().find(|v| v.id == layer.id) {
            *value = layer;
        }
    }
}
pub fn demo_document() -> Document {
    let mut doc = Document::new("Color studies", 1200, 800).unwrap();
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
        Content::shape(Shape::Ellipse, "#6587ff".into()),
    );
    add(
        "Coral orbit",
        345,
        345,
        775.,
        70.,
        Content::shape(Shape::Ellipse, "#f6a484".into()),
    );
    add(
        "Edition",
        700,
        80,
        80.,
        77.,
        Content::Text {
            text: "P I C S I E   /   0 0 1".into(),
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
            text: "Make something\ncolorful.".into(),
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
