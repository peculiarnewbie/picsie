//! Native Skia compositor; all surfaces, filters, codecs, and preview pixels remain in Rust.
mod blends;
use crate::{
    geometry::{self, Viewport},
    model::*,
    pixel_selection::{PixelSelection, SelectionDraft},
};
use anyhow::{Result, anyhow, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use skia_safe::{self as sk, BlendMode, Canvas, Color, Image, Paint, Rect, Surface};
use std::{collections::HashMap, sync::Arc};
mod composite;
mod projection;
pub fn surface(w: u32, h: u32) -> Result<Surface> {
    dimensions(w, h)?;
    sk::surfaces::raster_n32_premul((w as i32, h as i32))
        .ok_or_else(|| anyhow!("Cannot allocate image surface"))
}
/// Straight-alpha BGRA for native image presentation. Opaque native BGRA
/// already has the required representation; other surfaces use Skia conversion.
pub fn bgra_pixels(surface: &mut Surface) -> Result<Vec<u8>> {
    if let Some(pixels) = surface.peek_pixels()
        && pixels.color_type() == sk::ColorType::BGRA8888
        && pixels.info().color_space().is_none()
        && pixels.row_bytes() == pixels.width() as usize * 4
        && pixels.compute_is_opaque()
        && let Some(bytes) = pixels.bytes()
    {
        return Ok(bytes.to_vec());
    }
    let (width, height) = (surface.width(), surface.height());
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let info = sk::ImageInfo::new(
        (width, height),
        sk::ColorType::BGRA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    ensure!(
        surface.read_pixels(&info, &mut pixels, width as usize * 4, (0, 0)),
        "Cannot read native preview pixels"
    );
    Ok(pixels)
}
pub fn color(value: &str) -> Color {
    let rgb = u32::from_str_radix(&value[1..7], 16).expect("validated hex");
    let alpha = if value.len() == 9 {
        u8::from_str_radix(&value[7..9], 16).unwrap()
    } else {
        255
    };
    Color::from_argb(alpha, (rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}
fn paint(value: &str) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true).set_color(color(value));
    p
}
fn rect(w: u32, h: u32) -> Rect {
    Rect::from_wh(w as f32, h as f32)
}
/// TypeTool's text-to-box gap, in layer pixels.
const PADDING: f32 = 12.;
fn point(p: Point) -> sk::Point {
    sk::Point::new(p.x as f32, p.y as f32)
}
fn stroke(canvas: &Canvas, points: &[Point], size: f64, opacity: f64, erase: bool, value: &str) {
    let Some(first) = points.first() else {
        return;
    };
    let mut p = paint(value);
    p.set_alpha_f(opacity as f32).set_blend_mode(if erase {
        BlendMode::DstOut
    } else {
        BlendMode::SrcOver
    });
    if points.len() == 1 {
        canvas.draw_circle(point(*first), (size / 2.) as f32, &p);
    } else {
        p.set_style(sk::paint::Style::Stroke)
            .set_stroke_width(size as f32)
            .set_stroke_cap(sk::paint::Cap::Round)
            .set_stroke_join(sk::paint::Join::Round);
        let mut path = sk::PathBuilder::new();
        path.move_to(point(*first));
        for v in &points[1..] {
            path.line_to(point(*v));
        }
        canvas.draw_path(&path.detach(), &p);
    }
}

/// Raster coverage follows Compositor's immutable grayscale mask asset model. Legacy strokes
/// are painted into this surface while old projects are read or a live stroke is previewed.
fn mask_surface(mask: &LayerMask, layer: &Layer) -> Result<Surface> {
    let mut result = surface(layer.width, layer.height)?;
    let canvas = result.canvas();
    if let Some(raster) = &mask.raster {
        let info = sk::ImageInfo::new(
            (raster.width as i32, raster.height as i32),
            sk::ColorType::Alpha8,
            sk::AlphaType::Premul,
            None,
        );
        let image = sk::images::raster_from_data(
            &info,
            sk::Data::new_copy(raster.pixels.as_slice()),
            raster.width as usize,
        )
        .ok_or_else(|| anyhow!("Cannot create grayscale mask image"))?;
        canvas.draw_image_rect(
            image,
            None,
            rect(layer.width, layer.height),
            &Paint::default(),
        );
    } else if mask.base == MaskMode::Reveal {
        canvas.clear(Color::WHITE);
    }
    for stroke_value in &mask.strokes {
        stroke(
            canvas,
            &stroke_value.points,
            stroke_value.size,
            stroke_value.opacity,
            stroke_value.mode == MaskMode::Hide,
            "#ffffff",
        );
    }
    Ok(result)
}

fn mask_image(mask: &LayerMask, layer: &Layer) -> Result<Image> {
    let mut source = mask_surface(mask, layer)?;
    let Some(placement) = mask.placement else {
        return Ok(source.image_snapshot());
    };
    if placement == MaskPlacement::of(layer) {
        return Ok(source.image_snapshot());
    }
    let width = layer.width as usize;
    let height = layer.height as usize;
    let mut rgba = vec![0u8; width * height * 4];
    let info = sk::ImageInfo::new(
        (layer.width as i32, layer.height as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    ensure!(
        source.read_pixels(&info, &mut rgba, width * 4, (0, 0)),
        "Cannot read placed mask"
    );
    let raster = MaskRaster {
        width: layer.width,
        height: layer.height,
        pixels: Arc::new(rgba.chunks_exact(4).map(|p| p[3]).collect()),
    };
    let background = mask_background(&raster);
    let placed = placement.as_layer(layer);
    let mut pixels = vec![background; width * height];
    for y in 0..height {
        for x in 0..width {
            let world = geometry::to_world(layer, Point::new(x as f64 + 0.5, y as f64 + 0.5));
            let local = geometry::to_local(&placed, world);
            let (fx, fy) = (local.x - 0.5, local.y - 0.5);
            let (sx, sy) = (fx.floor() as isize, fy.floor() as isize);
            if local.x >= 0. && local.y >= 0. && local.x < width as f64 && local.y < height as f64 {
                if placement.sampling == Sampling::Nearest {
                    pixels[y * width + x] =
                        rgba[(local.y.floor() as usize * width + local.x.floor() as usize) * 4 + 3];
                    continue;
                }
                let sample = |x: isize, y: isize| {
                    rgba[(y.clamp(0, height as isize - 1) as usize * width
                        + x.clamp(0, width as isize - 1) as usize)
                        * 4
                        + 3] as f64
                };
                let (tx, ty) = (fx - sx as f64, fy - sy as f64);
                pixels[y * width + x] = ((sample(sx, sy) * (1. - tx) + sample(sx + 1, sy) * tx)
                    * (1. - ty)
                    + (sample(sx, sy + 1) * (1. - tx) + sample(sx + 1, sy + 1) * tx) * ty)
                    .round() as u8;
            }
        }
    }
    let alpha = sk::ImageInfo::new(
        (layer.width as i32, layer.height as i32),
        sk::ColorType::Alpha8,
        sk::AlphaType::Premul,
        None,
    );
    sk::images::raster_from_data(&alpha, sk::Data::new_copy(&pixels), width)
        .ok_or_else(|| anyhow!("Cannot place grayscale mask"))
}

/// LayerMask.background: majority of the perimeter; equality reveals.
pub fn mask_background(raster: &MaskRaster) -> u8 {
    let (w, h) = (raster.width as usize, raster.height as usize);
    let (mut total, mut count) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                total += raster.pixels[y * w + x] as usize;
                count += 1;
            }
        }
    }
    if total * 2 >= count * 255 { 255 } else { 0 }
}
pub fn rasterize_mask(mask: &LayerMask, layer: &Layer) -> Result<MaskRaster> {
    let mut surface = mask_surface(mask, layer)?;
    let mut rgba = vec![0u8; layer.width as usize * layer.height as usize * 4];
    let info = sk::ImageInfo::new(
        (layer.width as i32, layer.height as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    ensure!(
        surface.read_pixels(&info, &mut rgba, layer.width as usize * 4, (0, 0)),
        "Cannot read mask coverage"
    );
    let pixels = rgba.chunks_exact(4).map(|pixel| pixel[3]).collect();
    Ok(MaskRaster {
        width: layer.width,
        height: layer.height,
        pixels: Arc::new(pixels),
    })
}
pub(crate) fn transform(c: &Canvas, l: &Layer) {
    let mid = geometry::center(l);
    c.translate((mid.x as f32, mid.y as f32));
    c.rotate(l.rotation as f32, None);
    c.scale((
        (l.scale_x * if l.flip_x { -1. } else { 1. }) as f32,
        (l.scale_y * if l.flip_y { -1. } else { 1. }) as f32,
    ));
    c.translate((-(l.width as f32) / 2., -(l.height as f32) / 2.));
}
pub(crate) fn sampling_options(sampling: Sampling) -> sk::SamplingOptions {
    match sampling {
        Sampling::Nearest => {
            sk::SamplingOptions::new(sk::FilterMode::Nearest, sk::MipmapMode::None)
        }
        Sampling::Smooth => sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
        Sampling::High => sk::SamplingOptions::from(sk::CubicResampler::catmull_rom()),
    }
}
/// Draw source pixels in document space, without appearance, masks, or clipping links.
pub fn draw_pixels(canvas: &Canvas, layer: &Layer, image: &Image) {
    canvas.save();
    transform(canvas, layer);
    canvas.draw_image_with_sampling_options(
        image,
        (0., 0.),
        sampling_options(layer.sampling),
        None,
    );
    canvas.restore();
}
fn blend(b: Blend) -> BlendMode {
    match b {
        Blend::SourceOver => BlendMode::SrcOver,
        Blend::Multiply => BlendMode::Multiply,
        Blend::Screen => BlendMode::Screen,
        Blend::Overlay => BlendMode::Overlay,
        Blend::Darken => BlendMode::Darken,
        Blend::Lighten => BlendMode::Lighten,
        Blend::SoftLight => BlendMode::SoftLight,
        Blend::HardLight => BlendMode::HardLight,
        Blend::Difference => BlendMode::Difference,
        Blend::Exclusion => BlendMode::Exclusion,
        Blend::ColorDodge => BlendMode::ColorDodge,
        Blend::ColorBurn => BlendMode::ColorBurn,
        Blend::Hue => BlendMode::Hue,
        Blend::Saturation => BlendMode::Saturation,
        Blend::Color => BlendMode::Color,
        Blend::Luminosity => BlendMode::Luminosity,
        Blend::LinearBurn
        | Blend::LinearDodge
        | Blend::VividLight
        | Blend::LinearLight
        | Blend::PinLight
        | Blend::HardMix
        | Blend::Subtract
        | Blend::Divide => BlendMode::SrcOver,
    }
}
struct SourceCache {
    layer: Layer,
    image: Image,
    used: u64,
}
#[derive(Default)]
pub struct Renderer {
    cache: HashMap<String, SourceCache>,
    cache_pixels: u64,
    clock: u64,
    stacks: HashMap<String, projection::Stack>,
    stack_pixels: u64,
    composite: Option<composite::Composite>,
    background: Option<composite::Background>,
    preview_tiles: Option<composite::PreviewTiles>,
    thumbnail_tiles: std::collections::HashMap<String, composite::PreviewTiles>,
}
impl Renderer {
    #[allow(deprecated)]
    pub fn layer_surface(&mut self, l: &Layer) -> Result<Image> {
        self.clock += 1;
        if let Some(entry) = self.cache.get_mut(&l.id) {
            let old = &entry.layer;
            if old.width == l.width
                && old.height == l.height
                && Arc::ptr_eq(&old.content, &l.content)
                && old.strokes == l.strokes
                && old.mask == l.mask
                && (l.mask.as_ref().is_none_or(|m| m.placement.is_none())
                    || MaskPlacement::of(old) == MaskPlacement::of(l))
            {
                entry.used = self.clock;
                return Ok(entry.image.clone());
            }
        }
        let mut s = surface(l.width, l.height)?;
        let c = s.canvas();
        match l.content.as_ref() {
            Content::Paint => (),
            Content::Group => (),
            Content::Shape { shape, color } => {
                let p = paint(color);
                match shape {
                    Shape::Rectangle => {
                        c.draw_rect(rect(l.width, l.height), &p);
                    }
                    Shape::Ellipse => {
                        c.draw_oval(rect(l.width, l.height), &p);
                    }
                }
            }
            Content::Gradient { from, to } => {
                let colors = [color(from), color(to)];
                let mut p = Paint::default();
                p.set_shader(sk::gradient_shader::linear(
                    ((0., 0.), (l.width as f32, l.height as f32)),
                    colors.as_slice(),
                    None,
                    sk::TileMode::Clamp,
                    None,
                    None,
                ));
                c.draw_rect(rect(l.width, l.height), &p);
            }
            Content::Text {
                text,
                font_size,
                font_family,
                color,
            } => {
                let _ = (text, font_size, font_family, color);
                let mut paragraph = crate::text::paragraph(l, None);
                // Box text word-wraps at the box width minus TypeTool's 12px padding on each
                // side; explicit line breaks still break. Overflow clips to the padded box.
                let width = (l.width as f32 - 2. * PADDING).max(1.);
                paragraph.layout(width);
                let (_, metrics) = paragraph.get_font_at(0).metrics();
                // Preserve the former Canvas2D top baseline (canvas v1.0.9 skia_c.cpp).
                let top = -paragraph.alphabetic_baseline()
                    - metrics.ascent
                    - metrics.underline_position().unwrap_or(0.)
                    - metrics.underline_thickness().unwrap_or(0.);
                c.save();
                c.clip_rect(
                    Rect::from_xywh(PADDING, PADDING, width, l.height as f32 - 2. * PADDING),
                    None,
                    false,
                );
                paragraph.paint(c, (PADDING, PADDING + top));
                c.restore();
            }
            Content::Image { data } => {
                data.draw(c, rect(l.width, l.height))?;
            }
        }
        for v in &l.strokes {
            stroke(
                c,
                &v.points,
                v.size,
                v.opacity,
                v.mode == StrokeMode::Erase,
                &v.color,
            );
        }
        if let Some(mask) = &l.mask
            && mask.enabled
        {
            let image = mask_image(mask, l)?;
            let mut p = Paint::default();
            p.set_blend_mode(BlendMode::DstIn);
            c.draw_image(&image, (0., 0.), Some(&p));
        }
        let image = s.image_snapshot();
        if let Some(old) = self.cache.remove(&l.id) {
            self.cache_pixels -= old.image.width() as u64 * old.image.height() as u64;
        }
        self.cache_pixels += image.width() as u64 * image.height() as u64;
        self.cache.insert(
            l.id.clone(),
            SourceCache {
                layer: l.clone(),
                image: image.clone(),
                used: self.clock,
            },
        );
        while self.cache_pixels > MAX_PIXELS {
            let Some(id) = self
                .cache
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            let old = self.cache.remove(&id).unwrap();
            self.cache_pixels -= old.image.width() as u64 * old.image.height() as u64;
        }
        Ok(image)
    }
    fn draw_own(&mut self, doc: &Document, l: &Layer, c: &Canvas, mode: Blend) -> Result<()> {
        self.draw_own_with_opacity(l, c, mode, doc.effective(l).1)
    }
    fn draw_own_with_opacity(
        &mut self,
        l: &Layer,
        c: &Canvas,
        mode: Blend,
        opacity: f64,
    ) -> Result<()> {
        // Adjustment layers carry no pixels of their own; the projection draws
        // them by remapping the composite beneath instead.
        if l.adjustment.is_some() {
            return Ok(());
        }
        let mut p = Paint::default();
        p.set_anti_alias(true)
            .set_alpha((opacity * 255.).round() as u8)
            .set_blend_mode(blend(mode));
        blends::apply(&mut p, mode)?;
        // The identity matrix forces Skia's floating-point color pipeline even for
        // a plain opaque fill. Skip it only when drawing is an exact pixel copy:
        // translucent/filtered/transformed draws must retain the old rounding.
        let opaque_fill = match l.content.as_ref() {
            Content::Gradient { from, to } => color(from).a() == 255 && color(to).a() == 255,
            Content::Shape {
                shape: Shape::Rectangle,
                color: value,
            } => color(value).a() == 255,
            _ => false,
        };
        let direct_fill = opaque_fill
            && opacity == 1.
            && mode == Blend::SourceOver
            && l.blur == 0.
            && l.rotation == 0.
            && l.scale_x == 1.
            && l.scale_y == 1.
            && !l.flip_x
            && !l.flip_y
            && l.x.fract() == 0.
            && l.y.fract() == 0.
            && l.mask.is_none()
            && l.mask_source_id.is_none()
            && l.parent_id.is_none()
            && l.strokes.is_empty()
            && c.local_to_device() == sk::M44::new_identity();
        if l.saturation != 1. || l.brightness != 1. || !direct_fill {
            let sat = l.saturation as f32;
            let b = l.brightness as f32;
            let r = 0.213 * (1. - sat);
            let g = 0.715 * (1. - sat);
            let blue = 0.072 * (1. - sat);
            p.set_color_filter(sk::color_filters::matrix_row_major(
                &[
                    (r + sat) * b,
                    g * b,
                    blue * b,
                    0.,
                    0.,
                    r * b,
                    (g + sat) * b,
                    blue * b,
                    0.,
                    0.,
                    r * b,
                    g * b,
                    (blue + sat) * b,
                    0.,
                    0.,
                    0.,
                    0.,
                    0.,
                    1.,
                    0.,
                ],
                None,
            ));
        }
        if l.blur > 0. {
            p.set_image_filter(sk::image_filters::blur(
                (l.blur as f32, l.blur as f32),
                None,
                None,
                None,
            ));
        }
        c.save();
        transform(c, l);
        let matrix = c.local_to_device_as_3x3();
        if l.mask.is_none()
            && l.strokes.is_empty()
            && l.blur == 0.
            && opacity == 1.
            && l.saturation == 1.
            && l.brightness == 1.
            && matrix.is_translate()
            && matrix.translate_x().fract() == 0.
            && matrix.translate_y().fract() == 0.
            && c.device_clip_bounds()
                .is_some_and(|r| r.width() as i64 * r.height() as i64 <= 260 * 260)
            && let Content::Image {
                data: crate::asset::ImageAsset::Tiled(raster),
            } = l.content.as_ref()
            && (raster.width, raster.height) == (l.width, l.height)
        {
            raster.draw(c, sampling_options(l.sampling), &p);
        } else {
            let image = self.layer_surface(l)?;
            c.draw_image_with_sampling_options(
                &image,
                (0., 0.),
                sampling_options(l.sampling),
                Some(&p),
            );
        }
        c.restore();
        Ok(())
    }
    /// LiveMaskRenderer.coverage: source alpha is independent of visibility and RGB.
    /// Resolve a dependency chain iteratively to keep scratch memory bounded.
    pub fn live_coverage(&mut self, doc: &Document, source: &str) -> Result<Vec<u8>> {
        self.coverage_in(doc, source, None)
    }
    fn coverage_in(
        &mut self,
        doc: &Document,
        source: &str,
        target: Option<&Layer>,
    ) -> Result<Vec<u8>> {
        let (width, height) = target
            .map(|l| (l.width, l.height))
            .unwrap_or((doc.width, doc.height));
        let mut chain = Vec::new();
        let mut current = Some(source);
        while let Some(id) = current {
            ensure!(
                chain.len() < 256 && !chain.iter().any(|l: &&Layer| l.id == id),
                "Invalid live mask graph"
            );
            let layer = doc
                .layers
                .iter()
                .find(|l| l.id == id)
                .ok_or_else(|| anyhow!("Missing live mask source"))?;
            chain.push(layer);
            current = layer.mask_source_id.as_deref();
        }
        let mut coverage = vec![255u8; width as usize * height as usize];
        for layer in chain.into_iter().rev() {
            let mut pixels = surface(width, height)?;
            if let Some(target) = target {
                let origin = geometry::to_local(target, Point::new(0., 0.));
                let x = geometry::to_local(target, Point::new(1., 0.));
                let y = geometry::to_local(target, Point::new(0., 1.));
                pixels.canvas().concat(&sk::Matrix::new_all(
                    (x.x - origin.x) as f32,
                    (y.x - origin.x) as f32,
                    origin.x as f32,
                    (x.y - origin.y) as f32,
                    (y.y - origin.y) as f32,
                    origin.y as f32,
                    0.,
                    0.,
                    1.,
                ));
            }
            self.draw_own(doc, layer, pixels.canvas(), Blend::SourceOver)?;
            let own = rgba_pixels(&pixels.image_snapshot())?;
            for (alpha, pixel) in coverage.iter_mut().zip(own.chunks_exact(4)) {
                *alpha = ((*alpha as u16 * pixel[3] as u16 + 127) / 255) as u8;
            }
        }
        Ok(coverage)
    }
    fn clip_folders(
        &mut self,
        index: &crate::layer_index::LayerIndex<'_>,
        layer: &Layer,
        c: &Canvas,
    ) -> Result<()> {
        let mut parent = layer.parent_id.as_deref();
        while let Some(id) = parent {
            let folder = index.layer(id).ok_or_else(|| anyhow!("Missing folder"))?;
            if let Some(mask) = &folder.mask
                && mask.enabled
            {
                let image = mask_image(mask, folder)?;
                let matrix = c.local_to_device();
                transform(c, folder);
                let shader = image
                    .to_shader(
                        (sk::TileMode::Decal, sk::TileMode::Decal),
                        sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
                        None,
                    )
                    .ok_or_else(|| anyhow!("Cannot clip folder mask"))?;
                c.clip_shader(shader, None);
                c.set_matrix(&matrix);
            }
            parent = folder.parent_id.as_deref();
        }
        Ok(())
    }
    pub fn render(&mut self, doc: &Document) -> Result<Surface> {
        let mut s = surface(doc.width, doc.height)?;
        self.draw_document(doc, s.canvas())?;
        Ok(s)
    }
    fn draw_document(&mut self, doc: &Document, c: &Canvas) -> Result<()> {
        self.draw_projection(doc, c)
    }
    /// Bake only the dependency into target pixels; preserve its raster mask and appearance.
    pub fn bake_live_mask(&mut self, doc: &Document, layer: &Layer) -> Result<Layer> {
        let Some(source) = &layer.mask_source_id else {
            return Ok(layer.clone());
        };
        // LiveMaskBaker renders dependencies in the target's source grid, including
        // recoverable pixels outside the canvas, with its inverse pixel-to-document map.
        let coverage = self.coverage_in(doc, source, Some(layer))?;
        let mut bare = layer.clone();
        bare.mask = None;
        let mut pixels = rgba_pixels(&self.layer_surface(&bare)?)?;
        for (pixel, alpha) in pixels.chunks_exact_mut(4).zip(coverage) {
            pixel[3] = ((pixel[3] as u16 * alpha as u16 + 127) / 255) as u8;
        }
        let mut result = layer.clone();
        result.content = Arc::new(native_content(rgba_image(
            layer.width,
            layer.height,
            &pixels,
        )?));
        result.strokes.clear();
        result.mask_source_id = None;
        Ok(result)
    }
    /// Destructive pixel edit: materialize the editable layer, then remove document-space
    /// selection coverage. The separate layer mask stays attached to the resulting image.
    pub fn clear_layer_selection(
        &mut self,
        layer: &Layer,
        selection: &PixelSelection,
    ) -> Result<Image> {
        let mut unmasked = layer.clone();
        unmasked.mask = None;
        let image = self.layer_surface(&unmasked)?;
        let width = layer.width as usize;
        let height = layer.height as usize;
        let info = sk::ImageInfo::new(
            (layer.width as i32, layer.height as i32),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Unpremul,
            None,
        );
        let mut pixels = vec![0u8; width * height * 4];
        ensure!(
            image.read_pixels(
                &info,
                &mut pixels,
                width * 4,
                (0, 0),
                sk::image::CachingHint::Disallow
            ),
            "Cannot read layer pixels"
        );
        let coverage = selection_coverage(selection)?;
        for y in 0..height {
            for x in 0..width {
                let world = geometry::to_world(layer, Point::new(x as f64 + 0.5, y as f64 + 0.5));
                if world.x >= 0.
                    && world.y >= 0.
                    && world.x < selection.width as f64
                    && world.y < selection.height as f64
                {
                    let value = coverage[world.y as u32 as usize * selection.width as usize
                        + world.x as u32 as usize];
                    let alpha = &mut pixels[(y * width + x) * 4 + 3];
                    *alpha = (*alpha as u16 * (255 - value as u16) / 255) as u8;
                }
            }
        }
        sk::images::raster_from_data(&info, sk::Data::new_copy(&pixels), width * 4)
            .ok_or_else(|| anyhow!("Cannot create edited image"))
    }
    pub fn preview(
        &mut self,
        doc: &Document,
        v: &Viewport,
        selection: &[String],
        show_handles: bool,
        mask_id: Option<&str>,
        crop: Option<crate::crop::CropRect>,
        pixel_selection: Option<&PixelSelection>,
        selection_draft: Option<&SelectionDraft>,
    ) -> Result<Surface> {
        let mut s = surface(
            v.width.round().max(1.) as u32,
            v.height.round().max(1.) as u32,
        )?;
        let c = s.canvas();
        // The checkerboard depends on viewport geometry, not the edited pixels.
        // EditorCanvas.renderBounds: the original canvas union the crop draft.
        let bounds = crop
            .map(|r| {
                Rect::new(
                    r.x.min(0.) as f32,
                    r.y.min(0.) as f32,
                    (r.x + r.width).max(doc.width as f64) as f32,
                    (r.y + r.height).max(doc.height as f64) as f32,
                )
            })
            .unwrap_or_else(|| rect(doc.width, doc.height));
        let background = self.preview_background(doc, v, bounds)?;
        c.draw_image(&background, (0., 0.), None);
        let o = geometry::canvas_origin(doc, v);
        c.save();
        c.translate(point(o));
        c.scale((v.zoom as f32, v.zoom as f32));
        c.clip_rect(bounds, None, false);
        if bounds != rect(doc.width, doc.height) {
            // Viewport-sized composition exposes off-canvas pixels without allocating
            // a potentially enormous union raster. Ordinary previews keep their cache.
            self.draw_document(doc, c)?;
        } else {
            if composite::tiled_preview_eligible(doc) && composite::has_tiled_rasters(doc) {
                self.draw_preview_tiles(doc, c, v)?;
            } else {
                self.retain_preview_tiles_when_unpatched(doc);
                let image = self.preview_composite(doc)?;
                c.draw_image_with_sampling_options(
                    &image,
                    (0., 0.),
                    sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
                    None,
                );
            }
        }
        c.restore();
        if let Some(selection) = pixel_selection {
            c.save();
            c.translate(point(o));
            c.scale((v.zoom as f32, v.zoom as f32));
            let mut line = paint("#ffffff");
            line.set_style(sk::paint::Style::Stroke)
                .set_stroke_width(1. / v.zoom as f32);
            c.draw_path(&selection.outline, &line);
            line.set_color(sk::Color::BLACK)
                .set_path_effect(sk::PathEffect::dash(
                    &[4. / v.zoom as f32, 4. / v.zoom as f32],
                    0.,
                ));
            c.draw_path(&selection.outline, &line);
            c.restore();
        }
        let layers: Vec<_> = doc
            .layers
            .iter()
            .filter(|l| {
                selection.contains(&l.id)
                    && l.visible
                    && !matches!(l.content.as_ref(), Content::Group)
                    // Sourceless adjustments own no placeable pixels; their row
                    // selection still shows. Mask targeting keeps its outline.
                    && (mask_id == Some(l.id.as_str()) || l.adjustment.is_none())
            })
            .collect();
        let map = |p: Point| Point::new(o.x + p.x * v.zoom, o.y + p.y * v.zoom);
        let mut all = vec![];
        for l in &layers {
            let outline = if mask_id == Some(l.id.as_str()) {
                l.mask.as_ref().and_then(|mask| {
                    (!mask.linked).then(|| {
                        mask.placement
                            .unwrap_or_else(|| MaskPlacement::of(l))
                            .as_layer(l)
                    })
                })
            } else {
                None
            };
            let l = outline.as_ref().unwrap_or(l);
            let points: Vec<_> = geometry::corners(l).into_iter().map(map).collect();
            all.extend(points.iter().copied());
            let mut p = paint(if mask_id == Some(l.id.as_str()) {
                "#62deca"
            } else {
                "#9babff"
            });
            p.set_style(sk::paint::Style::Stroke).set_stroke_width(1.);
            let mut path = sk::PathBuilder::new();
            path.move_to(point(points[0]));
            for &v in &points[1..] {
                path.line_to(point(v));
            }
            path.close();
            c.draw_path(&path.detach(), &p);
            if selection.len() == 1 && !l.locked && show_handles {
                let h = geometry::handles(l, v.zoom);
                let top = map(h[1].1);
                let rot = map(h[8].1);
                c.draw_line(point(top), point(rot), &p);
                for (kind, at) in h {
                    let at = point(map(at));
                    let fill = paint("#ffffff");
                    if kind == "rotate" {
                        c.draw_circle(at, 5., &fill);
                        c.draw_circle(at, 5., &p);
                    } else {
                        let r = Rect::from_xywh(at.x - 3., at.y - 3., 6., 6.);
                        c.draw_rect(r, &fill);
                        c.draw_rect(r, &p);
                    }
                }
            }
        }
        if layers.len() > 1 {
            let minx = all.iter().map(|p| p.x).fold(f64::INFINITY, f64::min) - 3.;
            let miny = all.iter().map(|p| p.y).fold(f64::INFINITY, f64::min) - 3.;
            let maxx = all.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max) + 3.;
            let maxy = all.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max) + 3.;
            let mut p = paint("#9babff");
            p.set_style(sk::paint::Style::Stroke)
                .set_stroke_width(1.)
                .set_path_effect(sk::PathEffect::dash(&[5., 4.], 0.));
            c.draw_rect(
                Rect::new(minx as f32, miny as f32, maxx as f32, maxy as f32),
                &p,
            );
        }
        if let Some(rect) = crop {
            let top_left = map(Point::new(rect.x, rect.y));
            let bottom_right = map(Point::new(rect.x + rect.width, rect.y + rect.height));
            let bounds = Rect::new(
                top_left.x as f32,
                top_left.y as f32,
                bottom_right.x as f32,
                bottom_right.y as f32,
            );
            // TransformOverlay.drawCrop (609dbeae): non-printing 60% surround,
            // one-point frame, 40% rule-of-thirds lines and 8-point bordered handles.
            let mut surround = sk::PathBuilder::new();
            surround.add_rect(Rect::from_wh(v.width as f32, v.height as f32), None, None);
            surround.add_rect(bounds, None, None);
            surround.set_fill_type(sk::PathFillType::EvenOdd);
            let mut shade = paint("#000000");
            shade.set_alpha_f(0.6);
            c.draw_path(&surround.detach(), &shade);
            let mut line = paint("#ffffff");
            line.set_style(sk::paint::Style::Stroke)
                .set_stroke_width(1.);
            c.draw_rect(bounds, &line);
            line.set_alpha_f(0.4);
            for index in 1..=2 {
                let fraction = index as f32 / 3.;
                let x = bounds.left + bounds.width() * fraction;
                let y = bounds.top + bounds.height() * fraction;
                c.draw_line((x, bounds.top), (x, bounds.bottom), &line);
                c.draw_line((bounds.left, y), (bounds.right, y), &line);
            }
            for (hx, hy) in [
                (0., 0.),
                (0.5, 0.),
                (1., 0.),
                (1., 0.5),
                (1., 1.),
                (0.5, 1.),
                (0., 1.),
                (0., 0.5),
            ] {
                let x = (top_left.x + (bottom_right.x - top_left.x) * hx) as f32;
                let y = (top_left.y + (bottom_right.y - top_left.y) * hy) as f32;
                let handle = Rect::from_xywh(x - 4., y - 4., 8., 8.);
                c.draw_rect(handle, &paint("#ffffff"));
                let mut border = paint("#000000");
                border
                    .set_style(sk::paint::Style::Stroke)
                    .set_stroke_width(1.);
                c.draw_rect(handle, &border);
            }
        }
        if let Some(draft) = selection_draft {
            let mut line = paint("#ffffff");
            line.set_style(sk::paint::Style::Stroke)
                .set_stroke_width(1.)
                .set_path_effect(sk::PathEffect::dash(&[4., 4.], 0.));
            if let Some((x0, y0, x1, y1)) = draft.bounds() {
                let a = map(Point::new(x0, y0));
                let b = map(Point::new(x1, y1));
                let bounds = Rect::new(a.x as f32, a.y as f32, b.x as f32, b.y as f32);
                if draft.marquee == Some(crate::pixel_selection::MarqueeKind::Ellipse) {
                    c.draw_oval(bounds, &line);
                } else if draft.marquee.is_some() {
                    c.draw_rect(bounds, &line);
                }
            }
            if draft.marquee.is_none() && draft.points.len() > 1 {
                let mut path = sk::PathBuilder::new();
                path.move_to(point(map(draft.points[0])));
                for p in &draft.points[1..] {
                    path.line_to(point(map(*p)));
                }
                c.draw_path(&path.detach(), &line);
            }
        }
        Ok(s)
    }
    pub fn export(&mut self, doc: &Document, jpeg: bool) -> Result<Vec<u8>> {
        let mut s = self.render(doc)?;
        if jpeg {
            let mut p = paint("#ffffff");
            p.set_blend_mode(BlendMode::DstOver);
            s.canvas().draw_paint(&p);
        }
        let image = s.image_snapshot();
        if !jpeg {
            let mut bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut bytes, doc.width, doc.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let ppu = (doc.resolution / 0.0254).round() as u32;
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: ppu,
                yppu: ppu,
                unit: png::Unit::Meter,
            }));
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            encoder
                .write_header()?
                .write_image_data(&rgba_pixels(&image)?)?;
            return Ok(bytes);
        }
        let mut bytes = encode(&image, true)?;
        // JFIF density uses pixels/inch. Replace the encoder's APP0 density, or add APP0.
        let dpi = doc.resolution.round() as u16;
        if bytes.get(2..4) == Some(&[0xff, 0xe0])
            && bytes.get(6..11) == Some(b"JFIF\0")
            && bytes.len() >= 18
        {
            bytes[13] = 1;
            bytes[14..16].copy_from_slice(&dpi.to_be_bytes());
            bytes[16..18].copy_from_slice(&dpi.to_be_bytes());
        } else {
            let mut app0 = vec![0xff, 0xe0, 0, 16, b'J', b'F', b'I', b'F', 0, 1, 1, 1];
            app0.extend(dpi.to_be_bytes());
            app0.extend(dpi.to_be_bytes());
            app0.extend([0, 0]);
            bytes.splice(2..2, app0);
        }
        Ok(bytes)
    }
    pub fn sample(&mut self, doc: &Document, p: Point) -> Result<[u8; 4]> {
        // ColorPalette.sampleCompositeColor: translate the displayed composite into
        // one sRGB pixel, preserving layer order, appearance and alpha.
        ensure!(
            p.x >= 0. && p.y >= 0. && p.x < doc.width as f64 && p.y < doc.height as f64,
            "Sample is outside the canvas"
        );
        let mut s = surface(1, 1)?;
        s.canvas()
            .translate((-(p.x.floor() as f32), -(p.y.floor() as f32)));
        self.draw_document(doc, s.canvas())?;
        let mut pixel = [0; 4];
        let info = sk::ImageInfo::new(
            (1, 1),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Unpremul,
            None,
        );
        ensure!(
            s.read_pixels(&info, &mut pixel, 4, (0, 0)),
            "Sample is outside the canvas"
        );
        Ok(pixel)
    }
}
/// `DocumentSelection.coverage()` from the pinned Compositor: grayscale coverage at document
/// resolution, softened by `feather / 2` where the edge fades either side of the outline.
/// CoreImage's `clampedToExtent().applyingGaussianBlur(...)` is replaced by Skia's blur with
/// `TileMode::Clamp`, which smears the edge pixels the same way before the crop.
pub fn selection_coverage(selection: &PixelSelection) -> Result<Arc<Vec<u8>>> {
    if selection.feather <= 0. {
        return Ok(selection.pixels.clone());
    }
    // DocumentSelection.clip: four Gaussian sigmas plus one antialias pixel.
    // Crop the existing immutable coverage, preserving combined/raster selections.
    let Some(bounds) = selection.coverage_bounds() else {
        return Ok(Arc::new(vec![
            0;
            selection.width as usize
                * selection.height as usize
        ]));
    };
    let (width, height) = (bounds.width as usize, bounds.height as usize);
    let mut cropped = Vec::with_capacity(width * height);
    for y in 0..height {
        let at = (bounds.y as usize + y) * selection.width as usize + bounds.x as usize;
        cropped.extend_from_slice(&selection.pixels[at..at + width]);
    }
    let alpha = sk::ImageInfo::new(
        (bounds.width as i32, bounds.height as i32),
        sk::ColorType::Alpha8,
        sk::AlphaType::Premul,
        None,
    );
    let image = sk::images::raster_from_data(&alpha, sk::Data::new_copy(&cropped), width)
        .ok_or_else(|| anyhow!("Cannot create selection coverage"))?;
    let sigma = (selection.feather / 2.) as f32;
    let filter = sk::image_filters::blur((sigma, sigma), sk::TileMode::Clamp, None, None)
        .ok_or_else(|| anyhow!("Cannot soften selection coverage"))?;
    let mut soft = Paint::default();
    soft.set_image_filter(filter);
    let mut surface = surface(bounds.width, bounds.height)?;
    surface.canvas().draw_image(image, (0., 0.), Some(&soft));
    let mut pixels = vec![0; width * height];
    ensure!(
        surface.read_pixels(&alpha, &mut pixels, width, (0, 0)),
        "Cannot read selection coverage"
    );
    let mut output = vec![0; selection.width as usize * selection.height as usize];
    for y in 0..height {
        let at = (bounds.y as usize + y) * selection.width as usize + bounds.x as usize;
        output[at..at + width].copy_from_slice(&pixels[y * width..(y + 1) * width]);
    }
    Ok(Arc::new(output))
}

pub fn clear_selected_pixels(layer: &Layer, selection: &PixelSelection) -> Result<Image> {
    Renderer::default().clear_layer_selection(layer, selection)
}
pub(crate) fn clear_selected_asset(
    layer: &Layer,
    selection: &PixelSelection,
    doc: &Document,
) -> Result<crate::asset::ImageAsset> {
    let coverage = selection_coverage(selection)?;
    let area = selection
        .coverage_bounds()
        .map(|b| Rect::from_xywh(b.x as f32, b.y as f32, b.width as f32, b.height as f32))
        .unwrap_or(Rect::default());
    edit_replacement_tiles(
        layer,
        layer,
        0,
        0,
        doc,
        Some(&coverage),
        area,
        |pixel, amount| {
            pixel[3] = (pixel[3] as u16 * (255 - amount as u16) / 255) as u8;
        },
    )
}

/// BrushStroke.paintCanvas: visit only source tiles intersecting the selection,
/// retain the immutable base and publish replacement pixels without flattening.
fn edit_replacement_tiles(
    original: &Layer,
    next: &Layer,
    dx: i32,
    dy: i32,
    doc: &Document,
    coverage: Option<&[u8]>,
    area: Rect,
    edit: impl Fn(&mut [u8], u8),
) -> Result<crate::asset::ImageAsset> {
    use crate::{
        asset::ImageAsset,
        raster_snapshot::{RasterPatch, RasterSnapshot},
    };
    let source = if let Content::Image { data } = original.content.as_ref()
        && original.strokes.is_empty()
    {
        Some(data.clone())
    } else if matches!(original.content.as_ref(), Content::Paint) && original.strokes.is_empty() {
        None
    } else {
        let mut bare = original.clone();
        bare.mask = None;
        Some(ImageAsset::Raster(Arc::new(
            Renderer::default().layer_surface(&bare)?,
        )))
    };
    let points = [
        (area.left, area.top),
        (area.right, area.top),
        (area.right, area.bottom),
        (area.left, area.bottom),
    ]
    .map(|(x, y)| geometry::to_local(next, Point::new(x as f64, y as f64)));
    let affected = sk::IRect::new(
        (points
            .iter()
            .map(|p| p.x)
            .fold(f64::INFINITY, f64::min)
            .floor() as i32
            - 1)
        .max(0),
        (points
            .iter()
            .map(|p| p.y)
            .fold(f64::INFINITY, f64::min)
            .floor() as i32
            - 1)
        .max(0),
        (points
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil() as i32
            + 1)
        .min(next.width as i32),
        (points
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil() as i32
            + 1)
        .min(next.height as i32),
    );
    let mut patches = vec![];
    let mapping = geometry::PixelMapping::new(next);
    if !area.is_empty() && !affected.is_empty() {
        for ty in affected.top / 256..=(affected.bottom - 1) / 256 {
            for tx in affected.left / 256..=(affected.right - 1) / 256 {
                let tile = sk::IRect::new(
                    tx * 256,
                    ty * 256,
                    ((tx + 1) * 256).min(next.width as i32),
                    ((ty + 1) * 256).min(next.height as i32),
                );
                let mut surface = surface(tile.width() as u32, tile.height() as u32)?;
                if let Some(source) = &source {
                    let c = surface.canvas();
                    c.translate(((dx - tile.left) as f32, (dy - tile.top) as f32));
                    source.draw(c, rect(original.width, original.height))?;
                }
                let mut pixels = rgba_pixels(&surface.image_snapshot())?;
                for y in tile.top..tile.bottom {
                    for x in tile.left..tile.right {
                        let world = mapping.world(Point::new(x as f64 + 0.5, y as f64 + 0.5));
                        let amount = edit_coverage(doc, coverage, world);
                        if amount != 0 {
                            edit(
                                &mut pixels[(((y - tile.top) * tile.width() + x - tile.left) * 4)
                                    as usize..][..4],
                                amount,
                            );
                        }
                    }
                }
                patches.push(RasterPatch::new(
                    tile.with_offset((-dx, -dy)),
                    Arc::new(rgba_image(
                        tile.width() as u32,
                        tile.height() as u32,
                        &pixels,
                    )?),
                ));
            }
        }
    }
    Ok(ImageAsset::Tiled(Arc::new(RasterSnapshot::replacing(
        source.as_ref(),
        (original.width, original.height),
        patches,
        sk::IRect::from_xywh(-dx, -dy, next.width as i32, next.height as i32),
    )?)))
}
pub fn native_content(image: Image) -> Content {
    Content::Image {
        data: crate::asset::ImageAsset::Raster(Arc::new(image)),
    }
}
pub fn rgba_pixels(image: &Image) -> Result<Vec<u8>> {
    let info = sk::ImageInfo::new(
        (image.width(), image.height()),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    let mut pixels = vec![0; image.width() as usize * image.height() as usize * 4];
    ensure!(
        image.read_pixels(
            &info,
            &mut pixels,
            image.width() as usize * 4,
            (0, 0),
            sk::image::CachingHint::Disallow
        ),
        "Cannot read native image"
    );
    Ok(pixels)
}
/// Premultiplied RGBA bytes for the adjustment kernels, which port
/// `LevelsPixels.c` and `BrushPixels.c` step for step. This is a channel
/// swizzle of the surface's native layout, not a color conversion.
pub fn premul_pixels(image: &Image) -> Result<Vec<u8>> {
    let info = sk::ImageInfo::new(
        (image.width(), image.height()),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0; image.width() as usize * image.height() as usize * 4];
    ensure!(
        image.read_pixels(
            &info,
            &mut pixels,
            image.width() as usize * 4,
            (0, 0),
            sk::image::CachingHint::Disallow
        ),
        "Cannot read premultiplied image"
    );
    Ok(pixels)
}
pub fn premul_image(width: u32, height: u32, pixels: &[u8]) -> Result<Image> {
    dimensions(width, height)?;
    ensure!(
        pixels.len() == width as usize * height as usize * 4,
        "Premultiplied pixels do not match their dimensions"
    );
    let info = sk::ImageInfo::new(
        (width as i32, height as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Premul,
        None,
    );
    sk::images::raster_from_data(&info, sk::Data::new_copy(pixels), width as usize * 4)
        .ok_or_else(|| anyhow!("Cannot create premultiplied image"))
}
pub(crate) fn read_surface_pixels(
    surface: &mut Surface,
    info: &sk::ImageInfo,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    ensure!(
        surface.read_pixels(info, &mut pixels, width as usize * 4, (0, 0)),
        "Cannot read surface pixels"
    );
    Ok(pixels)
}
pub(crate) fn write_surface_pixels(
    surface: &mut Surface,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<()> {
    let image = premul_image(width, height, pixels)?;
    surface.canvas().clear(Color::TRANSPARENT);
    let mut paint = Paint::default();
    paint.set_blend_mode(BlendMode::Src);
    surface.canvas().draw_image(&image, (0., 0.), Some(&paint));
    Ok(())
}
pub fn rgba_image(width: u32, height: u32, pixels: &[u8]) -> Result<Image> {
    dimensions(width, height)?;
    let info = sk::ImageInfo::new(
        (width as i32, height as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    sk::images::raster_from_data(&info, sk::Data::new_copy(pixels), width as usize * 4)
        .ok_or_else(|| anyhow!("Cannot create native image"))
}
/// Preserve the source's pixel grid while extending it to a document-space edit region.
/// BrushStroke.init / paintTransform, adapted to Picsie's source dimensions and scale.
pub fn expanded_layer(layer: &Layer, area: Rect) -> Result<(Layer, i32, i32)> {
    let points = [
        (area.left, area.top),
        (area.right, area.top),
        (area.right, area.bottom),
        (area.left, area.bottom),
    ]
    .map(|(x, y)| geometry::to_local(layer, Point::new(x as f64, y as f64)));
    let left = points.iter().map(|p| p.x.floor()).fold(0., f64::min) as i32;
    let top = points.iter().map(|p| p.y.floor()).fold(0., f64::min) as i32;
    let right = points
        .iter()
        .map(|p| p.x.ceil())
        .fold(layer.width as f64, f64::max) as i32;
    let bottom = points
        .iter()
        .map(|p| p.y.ceil())
        .fold(layer.height as f64, f64::max) as i32;
    let (width, height) = ((right - left) as u32, (bottom - top) as u32);
    dimensions(width, height)?;
    let center = geometry::to_world(
        layer,
        Point::new((left + right) as f64 / 2., (top + bottom) as f64 / 2.),
    );
    let mut result = layer.clone();
    result.width = width;
    result.height = height;
    result.x = center.x - width as f64 * layer.scale_x / 2.;
    result.y = center.y - height as f64 * layer.scale_y / 2.;
    if let Some(mask) = &mut result.mask {
        let mask = Arc::make_mut(mask);
        let mut placement = mask.placement.unwrap_or(MaskPlacement::of(layer));
        placement.scale_x *= layer.width as f64 / width as f64;
        placement.scale_y *= layer.height as f64 / height as f64;
        mask.placement = Some(placement);
    }
    Ok((result, -left, -top))
}
/// SelectionEdits.fillSelection / BrushStroke.fill: solid source-over, clipped in document
/// coordinates, with no selection covering the canvas and an empty selection touching nothing.
pub fn fill_selected_pixels(
    layer: &Layer,
    selection: Option<&PixelSelection>,
    doc: &Document,
    value: &str,
    is_mask: bool,
    mode: MaskMode,
) -> Result<Layer> {
    if !is_mask
        && selection.is_none()
        && let Content::Text {
            text,
            font_size,
            font_family,
            ..
        } = layer.content.as_ref()
    {
        let mut next = layer.clone();
        next.content = Arc::new(Content::Text {
            text: text.clone(),
            font_size: *font_size,
            font_family: *font_family,
            color: value.into(),
        });
        return Ok(next);
    }
    let coverage = selection.map(selection_coverage).transpose()?;
    if is_mask {
        let Some(mask) = &layer.mask else {
            return Ok(layer.clone());
        };
        if !mask.enabled {
            return Ok(layer.clone());
        }
        let mut raster = rasterize_mask(mask, layer)?;
        let placed = mask
            .placement
            .map(|p| p.as_layer(layer))
            .unwrap_or_else(|| layer.clone());
        let pixels = Arc::make_mut(&mut raster.pixels);
        let mapping = geometry::PixelMapping::new(&placed);
        for y in 0..layer.height {
            for x in 0..layer.width {
                let world = mapping.world(Point::new(x as f64 + 0.5, y as f64 + 0.5));
                let amount = edit_coverage(doc, coverage.as_deref().map(Vec::as_slice), world);
                let index = (y * layer.width + x) as usize;
                let target = if mode == MaskMode::Reveal { 255 } else { 0 };
                pixels[index] =
                    ((pixels[index] as u32 * (255 - amount as u32) + target * amount as u32 + 127)
                        / 255) as u8;
            }
        }
        let mut next = layer.clone();
        let mut mask = mask.as_ref().clone();
        mask.raster = Some(Arc::new(raster));
        mask.strokes.clear();
        next.mask = Some(Arc::new(mask));
        return Ok(next);
    }
    let area = selection
        .and_then(PixelSelection::coverage_bounds)
        .map(|b| Rect::from_xywh(b.x as f32, b.y as f32, b.width as f32, b.height as f32))
        .unwrap_or(rect(doc.width, doc.height));
    let (mut next, dx, dy) = expanded_layer(layer, area)?;
    let fill = color(value);
    let data = edit_replacement_tiles(
        layer,
        &next,
        dx,
        dy,
        doc,
        coverage.as_deref().map(Vec::as_slice),
        area,
        |pixel, amount| {
            composite_pixel(
                pixel,
                [fill.r(), fill.g(), fill.b()],
                amount as f64 / 255.,
                false,
            )
        },
    )?;
    next.content = Arc::new(Content::Image { data });
    next.strokes.clear();
    Ok(next)
}
pub fn edit_coverage(doc: &Document, coverage: Option<&[u8]>, point: Point) -> u8 {
    if point.x < 0. || point.y < 0. || point.x >= doc.width as f64 || point.y >= doc.height as f64 {
        return 0;
    }
    coverage.map_or(255, |c| {
        c[(point.y as u32 * doc.width + point.x as u32) as usize]
    })
}
pub fn composite_pixel(pixel: &mut [u8], color: [u8; 3], amount: f64, erase: bool) {
    if amount == 0. {
        return;
    }
    if amount == 1. && !erase {
        pixel[..3].copy_from_slice(&color);
        pixel[3] = 255;
        return;
    }
    let alpha = pixel[3] as f64 / 255.;
    if erase {
        pixel[3] = (alpha * (1. - amount) * 255.).round() as u8;
        return;
    }
    let combined = amount + alpha * (1. - amount);
    if combined > 0. {
        for i in 0..3 {
            pixel[i] = ((color[i] as f64 * amount + pixel[i] as f64 * alpha * (1. - amount))
                / combined)
                .round() as u8;
        }
    }
    pixel[3] = (combined * 255.).round() as u8;
}
pub fn decode(bytes: &[u8]) -> Result<Image> {
    let data = sk::Data::new_copy(bytes);
    let codec = sk::Codec::from_data(data).ok_or_else(|| anyhow!("Invalid image data"))?;
    let info = codec.info();
    dimensions(info.width() as u32, info.height() as u32)?;
    Image::from_encoded(sk::Data::new_copy(bytes)).ok_or_else(|| anyhow!("Cannot decode image"))
}
#[allow(deprecated)]
pub fn encode(image: &Image, jpeg: bool) -> Result<Vec<u8>> {
    image
        .encode_to_data_with_quality(
            if jpeg {
                sk::EncodedImageFormat::JPEG
            } else {
                sk::EncodedImageFormat::PNG
            },
            92,
        )
        .map(|d| d.as_bytes().to_vec())
        .ok_or_else(|| anyhow!("Image encoding failed"))
}
pub fn png_content(image: &Image) -> Result<Content> {
    Ok(Content::Image {
        data: format!(
            "data:image/png;base64,{}",
            STANDARD.encode(encode(image, false)?)
        )
        .into(),
    })
}
/// File-backed native frame buffer: uncompressed RGBA TIFF, supported by QuickGUI's image loader.
/// QuickGUI 0.1.6 Image accepts paths, not shared textures. No PNG/base64 or JS pixel arrays.
pub fn frame_bytes(s: &mut Surface) -> Result<Vec<u8>> {
    let (w, h) = (s.width(), s.height());
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let info = sk::ImageInfo::new(
        (w, h),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    ensure!(
        s.read_pixels(&info, &mut pixels, w as usize * 4, (0, 0)),
        "Cannot read preview pixels"
    );
    let mut output = std::io::Cursor::new(Vec::new());
    tiff::encoder::TiffEncoder::new(&mut output)?
        .write_image::<tiff::encoder::colortype::RGBA8>(w as u32, h as u32, &pixels)?;
    Ok(output.into_inner())
}

fn alpha_image(width: u32, height: u32, pixels: &[u8]) -> Result<Image> {
    let info = sk::ImageInfo::new(
        (width as i32, height as i32),
        sk::ColorType::Alpha8,
        sk::AlphaType::Premul,
        None,
    );
    sk::images::raster_from_data(&info, sk::Data::new_copy(pixels), width as usize)
        .ok_or_else(|| anyhow!("Cannot create coverage image"))
}
