//! TransformInspector / LayerTransform and Selection.swift, Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Typed commands keep transform math in Rust.
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TransformField {
    X,
    Y,
    Width,
    Height,
    Rotation,
    ScalePercent,
}

pub(super) fn constrained_delta(mut delta: Point, shift: bool) -> Point {
    if shift {
        if delta.x.abs() >= delta.y.abs() {
            delta.y = 0.;
        } else {
            delta.x = 0.;
        }
    }
    delta
}
impl Editor {
    // TransformInspector's pixelSize: an asset's own pixels; blank/mask geometry
    // before this edit, frozen so repeated percentage typing doesn't compound.
    pub(super) fn transform_source_size(&self) -> (f64, f64) {
        let target = self
            .independent_mask_layer()
            .or_else(|| self.selected().cloned());
        let Some(target) = target else {
            return (1., 1.);
        };
        if self.independent_mask_layer().is_none()
            && matches!(target.content.as_ref(), Content::Image { .. })
        {
            (target.width as f64, target.height as f64)
        } else {
            (
                target.width as f64 * target.scale_x,
                target.height as f64 * target.scale_y,
            )
        }
    }
    pub fn transform_scale_percent(&self) -> f64 {
        let Some(target) = self
            .independent_mask_layer()
            .or_else(|| self.selected().cloned())
        else {
            return 100.;
        };
        let size = self
            .transform_pixel_size
            .unwrap_or_else(|| self.transform_source_size());
        target.width as f64 * target.scale_x / size.0.max(1.) * 100.
    }
    pub(super) fn set_transform_field(&mut self, field: TransformField, value: f64) -> Result<()> {
        ensure!(value.is_finite(), "Transform value must be finite");
        if self.distortion_corners().is_some() {
            return Ok(());
        }
        if !self.transform_active() {
            self.command(Command::BeginTransform)?;
        }
        let Some(target) = self
            .independent_mask_layer()
            .or_else(|| self.selected().cloned())
        else {
            return Ok(());
        };
        if !self.transform_active() || target.locked || self.selection.ids.len() != 1 {
            return Ok(());
        }
        let mut next = target.clone();
        match field {
            TransformField::X => next.x = value,
            TransformField::Y => next.y = value,
            TransformField::Rotation => next.rotation = value % 360.,
            TransformField::Width | TransformField::Height => {
                if value < 1. {
                    return Ok(());
                }
                if matches!(field, TransformField::Width) {
                    if self.locks_transform_ratio {
                        next.scale_y *= value / (next.width as f64 * next.scale_x);
                    }
                    next.scale_x = value / next.width as f64;
                } else {
                    if self.locks_transform_ratio {
                        next.scale_x *= value / (next.height as f64 * next.scale_y);
                    }
                    next.scale_y = value / next.height as f64;
                }
            }
            TransformField::ScalePercent => {
                if value <= 0. {
                    return Ok(());
                }
                let center = geometry::center(&target);
                let pixels = self
                    .transform_pixel_size
                    .unwrap_or_else(|| self.transform_source_size());
                next.scale_x = pixels.0 * value / 100. / next.width as f64;
                next.scale_y = pixels.1 * value / 100. / next.height as f64;
                next.x = center.x - next.width as f64 * next.scale_x / 2.;
                next.y = center.y - next.height as f64 * next.scale_y / 2.;
            }
        }
        next.validate()?;
        self.update_layer(serde_json::json!({"x":next.x,"y":next.y,"scaleX":next.scale_x,"scaleY":next.scale_y,"rotation":next.rotation}))
    }
}

pub(super) struct ImageDistortion {
    pub original: Layer,
    pub corners: [Point; 4],
}
impl Editor {
    pub fn distortion_corners(&self) -> Option<[Point; 4]> {
        self.mask_distortion_corners()
            .or_else(|| self.image_distortion.as_ref().map(|d| d.corners))
    }
    pub(super) fn begin_image_distortion(&mut self) -> Result<()> {
        if self.image_distortion.is_some() {
            return Ok(());
        }
        if !self.transform_active() {
            self.command(Command::BeginTransform)?;
        }
        let Some(original) = self.selected().cloned() else {
            return Ok(());
        };
        if !self.transform_active()
            || original.locked
            || matches!(
                original.content.as_ref(),
                Content::Group | Content::Text { .. }
            )
        {
            return Ok(());
        }
        let corners = geometry::corners(&original).try_into().unwrap();
        self.image_distortion = Some(ImageDistortion { original, corners });
        Ok(())
    }
    pub(super) fn preview_image_distortion(
        &mut self,
        corners: [Point; 4],
        limit: Option<f64>,
    ) -> Result<()> {
        let Some(draft) = &self.image_distortion else {
            return Ok(());
        };
        let layer = crate::distort::image(&draft.original, &corners, limit)?;
        // Selection.swift/DistortWarp.mapPath: a convex perspective maps the
        // floating outline in document coordinates; folded shapes have no such map.
        if let Some(floating) = &self.floating
            && crate::distort::convex(&corners)
        {
            let mut original = draft.original.clone();
            original.flip_x = false;
            original.flip_y = false;
            let inverse = operations::layer_matrix(&original).invert().unwrap();
            let unit =
                skia_safe::Matrix::scale((1. / original.width as f32, 1. / original.height as f32));
            let map = skia_safe::Matrix::concat(
                &crate::distort::homography(&corners),
                &skia_safe::Matrix::concat(&unit, &inverse),
            );
            self.history.pixel_selection = Some(floating.selection.transformed(&map)?);
        }
        let mut doc = self.history.document.clone();
        doc.replace(layer);
        self.history.preview(doc);
        self.image_distortion.as_mut().unwrap().corners = corners;
        Ok(())
    }
}

impl Editor {
    pub fn can_modify_selection(&self) -> bool {
        self.history
            .pixel_selection
            .as_ref()
            .is_some_and(|s| !s.outline.is_empty())
            && self.selection_draft().is_none()
    }
}
