//! Immutable native image ownership. Encode only at the project persistence boundary.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::{Arc, OnceLock};

#[derive(Debug)]
pub struct EncodedAsset {
    data: Arc<str>,
    decoded: OnceLock<skia_safe::Image>,
}

#[derive(Clone, Debug)]
pub enum ImageAsset {
    Encoded(Arc<EncodedAsset>),
    Raster(Arc<skia_safe::Image>),
    Tiled(Arc<crate::raster_snapshot::RasterSnapshot>),
}
impl From<String> for ImageAsset {
    fn from(value: String) -> Self {
        Self::Encoded(Arc::new(EncodedAsset {
            data: value.into(),
            decoded: OnceLock::new(),
        }))
    }
}
impl From<&str> for ImageAsset {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}
impl ImageAsset {
    pub fn image(&self) -> Result<skia_safe::Image> {
        match self {
            Self::Raster(image) => Ok(image.as_ref().clone()),
            Self::Tiled(raster) => raster.image(),
            Self::Encoded(data) => {
                if let Some(image) = data.decoded.get() {
                    return Ok(image.clone());
                }
                self.validate()?;
                // Retain actual decoded pixels instead of relying on Skia's
                // global decoder cache to keep a large source alive between draws.
                let image = crate::render::decode(&STANDARD.decode(&data.data[22..])?)?
                    .make_raster_image(None, skia_safe::image::CachingHint::Disallow)
                    .ok_or_else(|| anyhow::anyhow!("Cannot decode image asset pixels"))?;
                let _ = data.decoded.set(image);
                Ok(data.decoded.get().expect("decoded immutable image").clone())
            }
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Encoded(data) => ensure!(
                data.data.len() <= 90_000_000
                    && data.data.starts_with("data:image/png;base64,")
                    && data.data[22..]
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b)),
                "Invalid image asset"
            ),
            Self::Raster(image) => {
                crate::model::dimensions(image.width() as u32, image.height() as u32)?
            }
            Self::Tiled(raster) => crate::model::dimensions(raster.width, raster.height)?,
        }
        Ok(())
    }
    fn encoded(&self) -> Result<Arc<str>> {
        match self {
            Self::Encoded(data) => Ok(data.data.clone()),
            Self::Raster(_) | Self::Tiled(_) => Ok(format!(
                "data:image/png;base64,{}",
                STANDARD.encode(crate::render::encode(&self.image()?, false)?)
            )
            .into()),
        }
    }
    pub fn storage(&self) -> (usize, usize) {
        match self {
            Self::Encoded(data) => (Arc::as_ptr(data) as usize, data.data.len()),
            Self::Raster(image) => (
                image.unique_id() as usize,
                image.width() as usize * image.height() as usize * 4,
            ),
            Self::Tiled(raster) => (
                Arc::as_ptr(raster) as usize,
                raster.storage_parts().iter().map(|(_, n)| n).sum(),
            ),
        }
    }
    pub fn storage_parts(&self) -> Vec<(usize, usize)> {
        match self {
            Self::Tiled(raster) => raster.storage_parts(),
            Self::Encoded(data) => {
                let mut parts = vec![self.storage()];
                if let Some(image) = data.decoded.get() {
                    parts.push((
                        image.unique_id() as usize,
                        image.width() as usize * image.height() as usize * 4,
                    ));
                }
                parts
            }
            _ => vec![self.storage()],
        }
    }
    pub fn draw(&self, canvas: &skia_safe::Canvas, target: skia_safe::Rect) -> Result<()> {
        if let Self::Tiled(raster) = self {
            canvas.save();
            canvas.translate((target.left, target.top));
            canvas.scale((
                target.width() / raster.width as f32,
                target.height() / raster.height as f32,
            ));
            raster.draw(
                canvas,
                skia_safe::SamplingOptions::default(),
                &skia_safe::Paint::default(),
            );
            canvas.restore();
        } else {
            canvas.draw_image_rect(self.image()?, None, target, &skia_safe::Paint::default());
        }
        Ok(())
    }
}
impl PartialEq for ImageAsset {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Encoded(a), Self::Encoded(b)) => Arc::ptr_eq(a, b) || a.data == b.data,
            (Self::Raster(a), Self::Raster(b)) => {
                Arc::ptr_eq(a, b) || a.unique_id() == b.unique_id()
            }
            (Self::Tiled(a), Self::Tiled(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
impl Serialize for ImageAsset {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.encoded()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for ImageAsset {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}
