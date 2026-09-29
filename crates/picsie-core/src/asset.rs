//! Immutable native image ownership. Encode only at the project persistence boundary.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ImageAsset {
    Encoded(Arc<str>),
    Raster(Arc<skia_safe::Image>),
}
impl From<String> for ImageAsset {
    fn from(value: String) -> Self {
        Self::Encoded(value.into())
    }
}
impl From<&str> for ImageAsset {
    fn from(value: &str) -> Self {
        Self::Encoded(value.into())
    }
}
impl ImageAsset {
    pub fn image(&self) -> Result<skia_safe::Image> {
        match self {
            Self::Raster(image) => Ok(image.as_ref().clone()),
            Self::Encoded(data) => {
                self.validate()?;
                crate::render::decode(&STANDARD.decode(&data[22..])?)
            }
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Encoded(data) => ensure!(
                data.len() <= 90_000_000
                    && data.starts_with("data:image/png;base64,")
                    && data[22..]
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b)),
                "Invalid image asset"
            ),
            Self::Raster(image) => {
                crate::model::dimensions(image.width() as u32, image.height() as u32)?
            }
        }
        Ok(())
    }
    fn encoded(&self) -> Result<Arc<str>> {
        match self {
            Self::Encoded(data) => Ok(data.clone()),
            Self::Raster(image) => Ok(format!(
                "data:image/png;base64,{}",
                STANDARD.encode(crate::render::encode(image, false)?)
            )
            .into()),
        }
    }
    pub fn storage(&self) -> (usize, usize) {
        match self {
            Self::Encoded(data) => (data.as_ptr() as usize, data.len()),
            Self::Raster(image) => (
                Arc::as_ptr(image) as usize,
                image.width() as usize * image.height() as usize * 4,
            ),
        }
    }
}
impl PartialEq for ImageAsset {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Encoded(a), Self::Encoded(b)) => a == b,
            (Self::Raster(a), Self::Raster(b)) => {
                Arc::ptr_eq(a, b) || a.unique_id() == b.unique_id()
            }
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
        Ok(Self::Encoded(Arc::<str>::deserialize(deserializer)?))
    }
}
