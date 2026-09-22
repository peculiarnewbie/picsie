//! Native Skia compositor; all surfaces, filters, codecs, and preview pixels remain in Rust.
use crate::{
    geometry::{self, Viewport},
    model::*,
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
                    .set_height(1.0)
                    .set_color(self::color(color));
                let mut paragraph_style = ParagraphStyle::new();
                paragraph_style.set_text_style(&text_style);
                for (index, line) in text.split('\n').enumerate() {
                    if line.is_empty() {
                        continue;
                    }
                    let mut builder = ParagraphBuilder::new(&paragraph_style, fonts.clone());
                    builder.add_text(line);
                    let mut paragraph = builder.build();
                    // Match Canvas fillText: no wrapping; the layer surface clips overflow.
                    paragraph.layout(100_000.);
                    let (_, metrics) = paragraph.get_font_at(0).metrics();
                    // Preserve the former Canvas2D top baseline (canvas v1.0.9 skia_c.cpp).
                    let top = -paragraph.alphabetic_baseline()
                        - metrics.ascent
                        - metrics.underline_position().unwrap_or(0.)
                        - metrics.underline_thickness().unwrap_or(0.);
                    paragraph.paint(c, (0., top + index as f32 * *font_size as f32 * 1.2));
                }
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
            let mut m = surface(l.width, l.height)?;
            let mc = m.canvas();
            if mask.base == MaskMode::Reveal {
                mc.clear(Color::WHITE);
            }
            for v in &mask.strokes {
                stroke(
                    mc,
                    &v.points,
                    v.size,
                    v.opacity,
                    v.mode == MaskMode::Hide,
                    "#ffffff",
                );
            }
            let image = m.image_snapshot();
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
        for l in &doc.layers {
            if !l.visible || l.opacity == 0. {
                continue;
            }
            let image = self.layer_surface(l)?;
            let mut p = Paint::default();
            p.set_anti_alias(true)
                .set_alpha((l.opacity * 255.).round() as u8)
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
                sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::None),
                Some(&p),
            );
            c.restore();
        }
        Ok(s)
    }
    pub fn preview(
        &mut self,
        doc: &Document,
        v: &Viewport,
        selection: &[String],
        show_handles: bool,
        mask_id: Option<&str>,
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
        c.restore();
        let layers: Vec<_> = doc
            .layers
            .iter()
            .filter(|l| selection.contains(&l.id) && l.visible)
            .collect();
        let map = |p: Point| Point::new(o.x + p.x * v.zoom, o.y + p.y * v.zoom);
        let mut all = vec![];
        for l in &layers {
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
