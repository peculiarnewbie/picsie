//! Icons reused unchanged from src/ui/icons.tsx.
use gpui_kit::{AssetSource, SharedString};
use std::borrow::Cow;
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        match path {
            "picsie/check.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/check.svg"
            )))),
            "picsie/move.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/move.svg"
            )))),
            "picsie/brush.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/brush.svg"
            )))),
            "picsie/eraser.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/eraser.svg"
            )))),
            "picsie/rectangle.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/rectangle.svg"
            )))),
            "picsie/ellipse.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/ellipse.svg"
            )))),
            "picsie/text.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/text.svg"
            )))),
            "picsie/hand.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/hand.svg"
            )))),
            "picsie/eyedropper.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/eyedropper.svg"
            )))),
            "picsie/image.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/image.svg"
            )))),
            "picsie/crop.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/crop.svg"
            )))),
            "picsie/marquee.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/marquee.svg"
            )))),
            "picsie/lasso.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/lasso.svg"
            )))),
            "picsie/layers.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/layers.svg"
            )))),
            "picsie/eye.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/eye.svg"
            )))),
            "picsie/eyeOff.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/eyeOff.svg"
            )))),
            "picsie/lock.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/lock.svg"
            )))),
            "picsie/undo.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/undo.svg"
            )))),
            "picsie/redo.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/redo.svg"
            )))),
            "picsie/up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/up.svg"
            )))),
            "picsie/down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/down.svg"
            )))),
            "picsie/plus.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/plus.svg"
            )))),
            "picsie/minus.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/minus.svg"
            )))),
            "picsie/trash.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/trash.svg"
            )))),
            "picsie/duplicate.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/duplicate.svg"
            )))),
            "picsie/chevron.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/chevron.svg"
            )))),
            "picsie/export.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/export.svg"
            )))),
            "picsie/grip.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/grip.svg"
            )))),
            "picsie/mask.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/icons/mask.svg"
            )))),
            _ => gpui_kit::assets::Assets.load(path),
        }
    }
    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}
