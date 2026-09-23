//! Native Skia compositor; all surfaces, filters, codecs, and preview pixels remain in Rust.
use crate::{
    geometry::{self, Viewport},
    model::*,
    pixel_selection::{PixelSelection, SelectionDraft},
};
use anyhow::{Result, anyhow, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use skia_safe::{self as sk, BlendMode, Canvas, Color, Image, Paint, Rect, Surface};
use std::{collections::VecDeque, sync::Arc};
pub fn surface(w: u32, h: u32) -> Result<Surface> {
    dimensions(w, h)?;
    sk::surfaces::raster_n32_premul((w as i32, h as i32))
        .ok_or_else(|| anyhow!("Cannot allocate image surface"))
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
    let mut edge_total = 0usize;
    let mut edge_count = 0usize;
    for y in 0..height {
        for x in 0..width {
            if y == 0 || y + 1 == height || x == 0 || x + 1 == width {
                edge_total += rgba[(y * width + x) * 4 + 3] as usize;
                edge_count += 1;
            }
        }
    }
    let background = if edge_total * 2 >= edge_count * 255 {
        255
    } else {
        0
    };
    let placed = placement.as_layer(layer);
    let mut pixels = vec![background; width * height];
    for y in 0..height {
        for x in 0..width {
            let world = geometry::to_world(layer, Point::new(x as f64 + 0.5, y as f64 + 0.5));
            let local = geometry::to_local(&placed, world);
            let sx = local.x.floor() as isize;
            let sy = local.y.floor() as isize;
            if sx >= 0 && sy >= 0 && sx < width as isize && sy < height as isize {
                pixels[y * width + x] = rgba[(sy as usize * width + sx as usize) * 4 + 3];
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
fn transform(c: &Canvas, l: &Layer) {
    let mid = geometry::center(l);
    c.translate((mid.x as f32, mid.y as f32));
    c.rotate(l.rotation as f32, None);
    c.scale((
        (l.scale_x * if l.flip_x { -1. } else { 1. }) as f32,
        (l.scale_y * if l.flip_y { -1. } else { 1. }) as f32,
    ));
    c.translate((-(l.width as f32) / 2., -(l.height as f32) / 2.));
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
    }
}
#[derive(Default)]
pub struct Renderer {
    cache: VecDeque<(Layer, Image)>,
}
impl Renderer {
    #[allow(deprecated)]
    pub fn layer_surface(&mut self, l: &Layer) -> Result<Image> {
        if let Some((_, image)) = self.cache.iter().find(|(old, _)| {
            old.id == l.id
                && old.width == l.width
                && old.height == l.height
                && Arc::ptr_eq(&old.content, &l.content)
                && old.strokes == l.strokes
                && old.mask == l.mask
        }) {
            return Ok(image.clone());
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
                let family = match font_family {
                    FontFamily::SansSerif => "sans-serif",
                    FontFamily::Serif => "serif",
                    FontFamily::Monospace => "monospace",
                };
                use sk::textlayout::{FontCollection, ParagraphBuilder, ParagraphStyle, TextStyle};
                let mut fonts = FontCollection::new();
                fonts.set_default_font_manager(sk::FontMgr::new(), None);
                let mut text_style = TextStyle::new();
                text_style
                    .set_font_families(&[family])
                    .set_font_size(*font_size as f32)
                    // Auto leading is 120% of the font size, as in TypeTool's autoLeading.
                    .set_height(1.2)
                    .set_color(self::color(color));
                let mut paragraph_style = ParagraphStyle::new();
                paragraph_style.set_text_style(&text_style);
                let mut builder = ParagraphBuilder::new(&paragraph_style, fonts.clone());
                builder.add_text(text);
                let mut paragraph = builder.build();
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
                let bytes = STANDARD.decode(
                    data.strip_prefix("data:image/png;base64,")
                        .ok_or_else(|| anyhow!("Invalid PNG asset"))?,
                )?;
                let image = decode(&bytes)?;
                c.draw_image_rect(image, None, rect(l.width, l.height), &Paint::default());
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
        self.cache.retain(|(v, _)| v.id != l.id);
        self.cache.push_back((l.clone(), image.clone()));
        let mut used: u64 = self
            .cache
            .iter()
            .map(|(v, _)| v.width as u64 * v.height as u64)
            .sum();
        while used > MAX_PIXELS {
            if let Some((v, _)) = self.cache.pop_front() {
                used -= v.width as u64 * v.height as u64;
            } else {
                break;
            }
        }
        Ok(image)
    }
    pub fn render(&mut self, doc: &Document) -> Result<Surface> {
        let mut s = surface(doc.width, doc.height)?;
        let c = s.canvas();
        self.cache
            .retain(|(l, _)| doc.layers.iter().any(|v| v.id == l.id));
        for l in doc.ordered_layers() {
            let (visible, opacity) = doc.effective(l);
            if !visible || opacity == 0. || matches!(l.content.as_ref(), Content::Group) {
                continue;
            }
            let image = self.layer_surface(l)?;
            let mut p = Paint::default();
            p.set_anti_alias(true)
                .set_alpha((opacity * 255.).round() as u8)
                .set_blend_mode(blend(l.blend));
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
            c.draw_image_with_sampling_options(
                &image,
                (0., 0.),
                sk::SamplingOptions::new(
                    if l.sampling == Sampling::Nearest {
                        sk::FilterMode::Nearest
                    } else {
                        sk::FilterMode::Linear
                    },
                    sk::MipmapMode::None,
                ),
                Some(&p),
            );
            c.restore();
        }
        Ok(s)
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
        c.clear(color("#15171c"));
        let o = geometry::canvas_origin(doc, v);
        c.save();
        c.translate(point(o));
        c.scale((v.zoom as f32, v.zoom as f32));
        c.clip_rect(rect(doc.width, doc.height), None, false);
        c.draw_rect(rect(doc.width, doc.height), &paint("#e2e3e6"));
        let tile = 12. / v.zoom;
        let sx = 0f64.max((-o.x / v.zoom / tile).floor()) as i32;
        let sy = 0f64.max((-o.y / v.zoom / tile).floor()) as i32;
        let ex = (doc.width as f64 / tile)
            .ceil()
            .min(((v.width - o.x) / v.zoom / tile).ceil()) as i32;
        let ey = (doc.height as f64 / tile)
            .ceil()
            .min(((v.height - o.y) / v.zoom / tile).ceil()) as i32;
        let p = paint("#bec1c7");
        for y in sy..ey {
            for x in sx..ex {
                if (x + y) % 2 == 0 {
                    c.draw_rect(
                        Rect::from_xywh(
                            (x as f64 * tile) as f32,
                            (y as f64 * tile) as f32,
                            tile as f32,
                            tile as f32,
                        ),
                        &p,
                    );
                }
            }
        }
        let image = self.render(doc)?.image_snapshot();
        c.draw_image_with_sampling_options(
            &image,
            (0., 0.),
            sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
            None,
        );
        if let Some(selection) = pixel_selection
            && let Some(bounds) = selection.coverage_bounds()
        {
            // Feathered edges fade past the stored bounds, as upstream's clip region does.
            let coverage = selection_coverage(selection)?;
            let width = bounds.width as usize;
            let height = bounds.height as usize;
            let mut rgba = vec![0u8; width * height * 4];
            for y in 0..height {
                for x in 0..width {
                    let value = coverage[(bounds.y as usize + y) * selection.width as usize
                        + bounds.x as usize
                        + x];
                    let at = (y * width + x) * 4;
                    rgba[at..at + 3].copy_from_slice(&[75, 179, 221]);
                    rgba[at + 3] = (value as u16 * 48 / 255) as u8;
                }
            }
            let info = sk::ImageInfo::new(
                (bounds.width as i32, bounds.height as i32),
                sk::ColorType::RGBA8888,
                sk::AlphaType::Unpremul,
                None,
            );
            if let Some(overlay) =
                sk::images::raster_from_data(&info, sk::Data::new_copy(&rgba), width * 4)
            {
                c.draw_image(&overlay, (bounds.x as f32, bounds.y as f32), None);
            }
        }
        c.restore();
        let layers: Vec<_> = doc
            .layers
            .iter()
            .filter(|l| {
                selection.contains(&l.id)
                    && l.visible
                    && !matches!(l.content.as_ref(), Content::Group)
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
            let mut line = paint("#ffffff");
            line.set_style(sk::paint::Style::Stroke)
                .set_stroke_width(1.5);
            c.draw_rect(bounds, &line);
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
                c.draw_rect(Rect::from_xywh(x - 3.5, y - 3.5, 7., 7.), &paint("#ffffff"));
            }
        }
        if let Some(draft) = selection_draft {
            let mut line = paint("#72cceb");
            line.set_style(sk::paint::Style::Stroke)
                .set_stroke_width(1.5)
                .set_path_effect(sk::PathEffect::dash(&[5., 4.], 0.));
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
        encode(&s.image_snapshot(), jpeg)
    }
    pub fn sample(&mut self, doc: &Document, p: Point) -> Result<[u8; 4]> {
        let mut s = self.render(doc)?;
        let mut pixel = [0; 4];
        let info = sk::ImageInfo::new(
            (1, 1),
            sk::ColorType::RGBA8888,
            sk::AlphaType::Unpremul,
            None,
        );
        ensure!(
            s.read_pixels(
                &info,
                &mut pixel,
                4,
                (p.x.floor() as i32, p.y.floor() as i32)
            ),
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
    let (width, height) = (selection.width as usize, selection.height as usize);
    let alpha = sk::ImageInfo::new(
        (selection.width as i32, selection.height as i32),
        sk::ColorType::Alpha8,
        sk::AlphaType::Premul,
        None,
    );
    let image = sk::images::raster_from_data(
        &alpha,
        sk::Data::new_copy(selection.pixels.as_slice()),
        width,
    )
    .ok_or_else(|| anyhow!("Cannot create selection coverage"))?;
    let sigma = (selection.feather / 2.) as f32;
    let filter = sk::image_filters::blur((sigma, sigma), sk::TileMode::Clamp, None, None)
        .ok_or_else(|| anyhow!("Cannot soften selection coverage"))?;
    let mut soft = Paint::default();
    soft.set_image_filter(filter);
    let mut surface = surface(selection.width, selection.height)?;
    surface.canvas().draw_image(&image, (0., 0.), Some(&soft));
    let mut rgba = vec![0u8; width * height * 4];
    let info = sk::ImageInfo::new(
        (selection.width as i32, selection.height as i32),
        sk::ColorType::RGBA8888,
        sk::AlphaType::Unpremul,
        None,
    );
    ensure!(
        surface.read_pixels(&info, &mut rgba, width * 4, (0, 0)),
        "Cannot read selection coverage"
    );
    Ok(Arc::new(
        rgba.chunks_exact(4).map(|pixel| pixel[3]).collect(),
    ))
}
pub fn clear_selected_pixels(layer: &Layer, selection: &PixelSelection) -> Result<Image> {
    Renderer::default().clear_layer_selection(layer, selection)
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
