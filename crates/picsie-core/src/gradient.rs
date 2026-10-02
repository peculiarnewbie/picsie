//! Gradient tool raster fills from Compositor's Gradient.swift, pinned 609dbeae.
//! MIT © 2026 Wonder Assembly LLC.
//!
//! The interactive gradient edits the selected layer or its mask in place: endpoints are
//! document pixels, the pending line previews without touching history, and Apply commits
//! one undo step. Colors come from the foreground/background palette; on a mask the red
//! channel carries the gray value, as upstream draws into a device-gray space.
use crate::{geometry, model::*, pixel_selection::PixelSelection, render};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

/// Linear runs from start to end; radial is centered on the start with the end on its rim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GradientShape {
    #[default]
    Linear,
    Radial,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum GradientStyle {
    ForegroundToBackground,
    #[default]
    ForegroundToTransparent,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GradientSettings {
    pub shape: GradientShape,
    pub style: GradientStyle,
    pub reversed: bool,
    pub opacity: f64,
}

impl Default for GradientSettings {
    /// GradientSettings defaults: linear, foreground-to-transparent, full opacity.
    fn default() -> Self {
        Self {
            shape: GradientShape::Linear,
            style: GradientStyle::ForegroundToTransparent,
            reversed: false,
            opacity: 1.,
        }
    }
}

impl GradientSettings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.opacity.is_finite() && (0.01..=1.).contains(&self.opacity),
            "Gradient opacity must be between 1% and 100%"
        );
        Ok(())
    }
}

/// The two ramp stops as straight-alpha RGBA, honoring style and reverse.
pub fn gradient_stops(
    settings: &GradientSettings,
    foreground: &str,
    background: &str,
) -> Result<[[u8; 4]; 2]> {
    ensure!(
        color_valid(foreground) && color_valid(background),
        "Invalid gradient color"
    );
    let rgba = |value: &str, alpha: u8| {
        let color = render::color(value);
        [color.r(), color.g(), color.b(), alpha]
    };
    let foreground_alpha = |alpha: u8| {
        let color = render::color(foreground);
        [color.r(), color.g(), color.b(), alpha]
    };
    let mut stops = match settings.style {
        GradientStyle::ForegroundToBackground => [rgba(foreground, 255), rgba(background, 255)],
        GradientStyle::ForegroundToTransparent => [foreground_alpha(255), foreground_alpha(0)],
    };
    if settings.reversed {
        stops.swap(0, 1);
    }
    Ok(stops)
}

/// GradientEdit.hasLine: endpoints at least half a document pixel apart.
pub fn has_line(start: Point, end: Point) -> bool {
    start.distance(end) >= 0.5
}

/// An uncommitted gradient on one layer or mask. Endpoints are document pixels.
/// `base` retains the unmodified target so every preview restarts from the
/// original pixels and moving the line never accumulates earlier previews,
/// as BrushStroke.fillGradient restarts from each tile's original content.
#[derive(Clone, Debug)]
pub struct GradientEdit {
    pub layer_id: String,
    pub mask: bool,
    pub start: Point,
    pub end: Point,
    pub base: Layer,
    pub base_image: GradientBase,
}

/// The unmodified target raster, captured once when the line starts.
#[derive(Clone, Debug)]
pub enum GradientBase {
    /// Layer pixels for a content edit.
    Pixels(skia_safe::Image),
    /// Grayscale coverage for a mask edit, in the placed mask grid.
    Mask(Arc<Vec<u8>>),
}

impl GradientEdit {
    pub fn has_line(&self) -> bool {
        has_line(self.start, self.end)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientHandle {
    Start,
    End,
}

/// FillGradient composited onto the target's current pixels with the global opacity,
/// clipped to the pixel selection when one exists.
///
/// The graphics path follows BrushStroke.fillGradient through paintCanvas: a Skia
/// linear/radial shader draws in document coordinates over the placed target grid,
/// soft-clipped by the selection coverage (SelectionClip.apply), onto the retained
/// base. A mask target resolves gray through the placed mask grid; other targets
/// bake the layer to a raster, as upstream's commitRasterEdit replaces the layer's
/// pixels (a painted shape is plain pixels after).
pub fn rasterize_base(layer: &Layer, mask: bool) -> Result<GradientBase> {
    if mask {
        let Some(mask_value) = &layer.mask else {
            anyhow::bail!("Gradient needs a mask target");
        };
        let raster = render::rasterize_mask(mask_value, layer)?;
        return Ok(GradientBase::Mask(raster.pixels.clone()));
    }
    let mut bare = layer.clone();
    bare.mask = None;
    Ok(GradientBase::Pixels(
        render::Renderer::default().layer_surface(&bare)?,
    ))
}

/// Document-to-layer mapping for the gradient shader, matching the placed grid
/// (the layer itself, or its independently placed mask).
fn target_matrix(grid: &Layer) -> skia_safe::Matrix {
    let origin = geometry::to_local(grid, Point::new(0., 0.));
    let x = geometry::to_local(grid, Point::new(1., 0.));
    let y = geometry::to_local(grid, Point::new(0., 1.));
    skia_safe::Matrix::new_all(
        (x.x - origin.x) as f32,
        (y.x - origin.x) as f32,
        origin.x as f32,
        (x.y - origin.y) as f32,
        (y.y - origin.y) as f32,
        origin.y as f32,
        0.,
        0.,
        1.,
    )
}

fn coverage_image(doc: &Document, coverage: &[u8]) -> Result<skia_safe::Image> {
    let info = skia_safe::ImageInfo::new(
        (doc.width as i32, doc.height as i32),
        skia_safe::ColorType::Alpha8,
        skia_safe::AlphaType::Premul,
        None,
    );
    skia_safe::images::raster_from_data(
        &info,
        skia_safe::Data::new_copy(coverage),
        doc.width as usize,
    )
    .ok_or_else(|| anyhow::anyhow!("Cannot create gradient selection clip"))
}

/// The gradient over the target footprint, drawn in document coordinates with the
/// selection coverage applied, ready to composite over the retained base.
#[allow(deprecated)]
fn overlay(
    layer: &Layer,
    grid: &Layer,
    doc: &Document,
    selection: Option<&PixelSelection>,
    settings: &GradientSettings,
    stops: [[u8; 4]; 2],
    start: Point,
    end: Point,
) -> Result<skia_safe::Image> {
    let colors = stops.map(|stop| skia_safe::Color::from_argb(stop[3], stop[0], stop[1], stop[2]));
    let shader = match settings.shape {
        GradientShape::Linear => skia_safe::gradient_shader::linear(
            (
                skia_safe::Point::new(start.x as f32, start.y as f32),
                skia_safe::Point::new(end.x as f32, end.y as f32),
            ),
            colors.as_slice(),
            None,
            skia_safe::TileMode::Clamp,
            None,
            None,
        ),
        GradientShape::Radial => skia_safe::gradient_shader::radial(
            skia_safe::Point::new(start.x as f32, start.y as f32),
            start.distance(end) as f32,
            colors.as_slice(),
            None,
            skia_safe::TileMode::Clamp,
            None,
            None,
        ),
    };
    let _ = grid;
    let mut surface = render::surface(layer.width, layer.height)?;
    surface.canvas().clear(skia_safe::Color::TRANSPARENT);
    let canvas = surface.canvas();
    canvas.save();
    canvas.concat(&target_matrix(grid));
    let mut paint = skia_safe::Paint::default();
    paint
        .set_anti_alias(true)
        .set_alpha_f(settings.opacity.clamp(0., 1.) as f32)
        .set_shader(shader);
    canvas.draw_rect(
        skia_safe::Rect::from_wh(doc.width as f32, doc.height as f32),
        &paint,
    );
    if let Some(selection) = selection {
        let coverage = render::selection_coverage(selection)?;
        // SelectionClip.apply: soft-clip the pending draw to the coverage mask.
        let mask = coverage_image(doc, &coverage)?;
        let mut clip = skia_safe::Paint::default();
        clip.set_blend_mode(skia_safe::BlendMode::DstIn);
        canvas.draw_image_rect_with_sampling_options(
            &mask,
            Some((
                &skia_safe::Rect::from_wh(doc.width as f32, doc.height as f32),
                skia_safe::canvas::SrcRectConstraint::Fast,
            )),
            &skia_safe::Rect::from_wh(doc.width as f32, doc.height as f32),
            skia_safe::SamplingOptions::new(
                skia_safe::FilterMode::Linear,
                skia_safe::MipmapMode::None,
            ),
            &clip,
        );
    }
    canvas.restore();
    Ok(surface.image_snapshot())
}

pub fn fill_layer(
    edit: &GradientEdit,
    doc: &Document,
    selection: Option<&PixelSelection>,
    settings: &GradientSettings,
    foreground: &str,
    background: &str,
) -> Result<Layer> {
    settings.validate()?;
    ensure!(has_line(edit.start, edit.end), "Drag a gradient line first");
    ensure!(
        color_valid(foreground) && color_valid(background),
        "Invalid gradient color"
    );
    let layer = &edit.base;
    let stops = gradient_stops(settings, foreground, background)?;
    if edit.mask {
        let Some(mask) = &layer.mask else {
            return Ok(layer.clone());
        };
        if !mask.enabled {
            return Ok(layer.clone());
        }
        let GradientBase::Mask(base) = &edit.base_image else {
            anyhow::bail!("Gradient lost its mask base");
        };
        let placed = mask
            .placement
            .map(|p| p.as_layer(layer))
            .unwrap_or_else(|| layer.clone());
        let overlay = overlay(
            layer, &placed, doc, selection, settings, stops, edit.start, edit.end,
        )?;
        // Opaque gray-in-RGB surface: SrcOver resolves the ramp against the old
        // coverage in native graphics; the R channel carries the result back.
        let mut surface = render::surface(layer.width, layer.height)?;
        {
            let info = skia_safe::ImageInfo::new(
                (layer.width as i32, layer.height as i32),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Unpremul,
                None,
            );
            let mut rgba = vec![0u8; layer.width as usize * layer.height as usize * 4];
            for (pixel, gray) in rgba.chunks_exact_mut(4).zip(base.iter()) {
                pixel[0] = *gray;
                pixel[1] = *gray;
                pixel[2] = *gray;
                pixel[3] = 255;
            }
            let base_image = skia_safe::images::raster_from_data(
                &info,
                skia_safe::Data::new_copy(&rgba),
                layer.width as usize * 4,
            )
            .ok_or_else(|| anyhow::anyhow!("Cannot create gradient mask base"))?;
            surface.canvas().draw_image(&base_image, (0., 0.), None);
            surface.canvas().draw_image(&overlay, (0., 0.), None);
        }
        let info = skia_safe::ImageInfo::new(
            (layer.width as i32, layer.height as i32),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Unpremul,
            None,
        );
        let mut rgba = vec![0u8; layer.width as usize * layer.height as usize * 4];
        ensure!(
            surface.read_pixels(&info, &mut rgba, layer.width as usize * 4, (0, 0)),
            "Cannot read gradient mask"
        );
        let pixels: Vec<u8> = rgba.chunks_exact(4).map(|pixel| pixel[0]).collect();
        let mut next = layer.clone();
        let mut mask = mask.as_ref().clone();
        mask.raster = Some(Arc::new(MaskRaster {
            width: layer.width,
            height: layer.height,
            pixels: Arc::new(pixels),
        }));
        mask.strokes.clear();
        next.mask = Some(Arc::new(mask));
        return Ok(next);
    }
    let GradientBase::Pixels(base) = &edit.base_image else {
        anyhow::bail!("Gradient lost its layer base");
    };
    let overlay = overlay(
        layer, layer, doc, selection, settings, stops, edit.start, edit.end,
    )?;
    let mut surface = render::surface(layer.width, layer.height)?;
    surface.canvas().draw_image(base, (0., 0.), None);
    surface.canvas().draw_image(&overlay, (0., 0.), None);
    let mut next = layer.clone();
    next.content = Arc::new(render::native_content(surface.image_snapshot()));
    next.strokes.clear();
    Ok(next)
}
