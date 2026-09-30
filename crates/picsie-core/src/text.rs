//! TypeTool.swift / InlineTextEditor.swift adapted from Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. SkParagraph replaces AppKit text layout.
use crate::{geometry, model::*, render};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use skia_safe::Rect;
use skia_safe::{self as sk, textlayout::*};
use ts_rs::TS;
pub const PADDING: f32 = 12.;
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TextNavigation {
    Up,
    Down,
    LineStart,
    LineEnd,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct TextLayout {
    pub font_name: String,
    pub alignment: TextAlignment,
    pub tracking: f64,
    pub leading: f64,
    pub point: bool,
}
#[derive(Clone, Debug, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextPatch {
    #[ts(optional)]
    pub text: Option<String>,
    #[ts(optional)]
    pub font_size: Option<f64>,
    #[ts(optional)]
    pub font_name: Option<String>,
    #[ts(optional)]
    pub alignment: Option<TextAlignment>,
    #[ts(optional)]
    pub tracking: Option<f64>,
    #[ts(optional)]
    pub leading: Option<f64>,
    #[ts(optional)]
    pub color: Option<String>,
}
impl Default for TextLayout {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            alignment: TextAlignment::Left,
            tracking: 0.,
            leading: 0.,
            point: false,
        }
    }
}
impl TextLayout {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.font_name.is_empty()
                && self.font_name.len() <= 200
                && self.tracking.is_finite()
                && (-100. ..=1000.).contains(&self.tracking)
                && self.leading.is_finite()
                && (0. ..=5000.).contains(&self.leading),
            "Invalid text layout"
        );
        Ok(())
    }
}
pub fn family(layer: &Layer) -> &str {
    if let Some(layout) = &layer.text_layout {
        return &layout.font_name;
    }
    match layer.content.as_ref() {
        Content::Text {
            font_family: FontFamily::Serif,
            ..
        } => "serif",
        Content::Text {
            font_family: FontFamily::Monospace,
            ..
        } => "monospace",
        _ => "sans-serif",
    }
}
pub fn installed_fonts() -> Vec<String> {
    let mut names: Vec<_> = font_faces().iter().map(|f| f.0.clone()).collect();
    names.extend(["sans-serif".into(), "serif".into(), "monospace".into()]);
    names.sort_by_key(|s| s.to_lowercase());
    names.dedup();
    names
}
fn font_faces() -> &'static Vec<(String, String, sk::FontStyle)> {
    static FACES: std::sync::OnceLock<Vec<(String, String, sk::FontStyle)>> =
        std::sync::OnceLock::new();
    FACES.get_or_init(|| {
        let mgr = sk::FontMgr::new();
        let mut faces = vec![];
        for family in mgr.family_names() {
            faces.push((family.clone(), family.clone(), sk::FontStyle::normal()));
            let mut styles = mgr.match_family(&family);
            for i in 0..styles.count() {
                let (style, name) = styles.style(i);
                if let Some(face) = styles.new_typeface(i) {
                    let name = face
                        .post_script_name()
                        .unwrap_or_else(|| format!("{} {}", family, name.unwrap_or_default()));
                    faces.push((name, family.clone(), style));
                }
            }
        }
        faces
    })
}
pub fn paragraph(layer: &Layer, width: Option<f32>) -> Paragraph {
    let Content::Text {
        text,
        font_size,
        color,
        ..
    } = layer.content.as_ref()
    else {
        unreachable!()
    };
    let layout = layer.text_layout.clone().unwrap_or_default();
    let mut fonts = FontCollection::new();
    fonts.set_default_font_manager(sk::FontMgr::new(), None);
    let mut style = TextStyle::new();
    let name = family(layer);
    let face = font_faces().iter().find(|f| f.0 == name);
    style
        .set_font_families(&[face.map(|f| f.1.as_str()).unwrap_or(name)])
        .set_font_style(face.map(|f| f.2).unwrap_or_else(sk::FontStyle::normal))
        .set_font_size(*font_size as f32)
        .set_height(if layout.leading > 0. {
            (layout.leading / font_size) as f32
        } else {
            1.2
        })
        .set_letter_spacing(layout.tracking as f32)
        .set_color(render::color(color));
    let mut ps = ParagraphStyle::new();
    ps.set_text_style(&style)
        .set_text_align(match layout.alignment {
            TextAlignment::Left => TextAlign::Left,
            TextAlignment::Center => TextAlign::Center,
            TextAlignment::Right => TextAlign::Right,
        });
    let mut builder = ParagraphBuilder::new(&ps, fonts);
    builder.add_text(text);
    let mut paragraph = builder.build();
    paragraph.layout(width.unwrap_or(100000.));
    paragraph
}
pub fn grow_point(layer: &mut Layer) -> Result<()> {
    if !layer.text_layout.as_ref().is_some_and(|l| l.point) {
        return Ok(());
    }
    let paragraph = paragraph(layer, None);
    let Content::Text { font_size, .. } = layer.content.as_ref() else {
        return Ok(());
    };
    let leading = layer.text_layout.as_ref().unwrap().leading;
    let h = if leading > 0. {
        leading
    } else {
        font_size * 1.2
    };
    let width = (paragraph.max_intrinsic_width() as f64 + 24. + font_size * 0.1)
        .ceil()
        .max(16.);
    let height = (paragraph.height() as f64).max(h).ceil() + 24.;
    ensure!(
        width <= MAX_DIMENSION as f64 && height <= MAX_DIMENSION as f64,
        "Text exceeds the 8192-pixel limit"
    );
    dimensions(width as u32, height as u32)?;
    let anchor = geometry::bounds_point(layer, Point::default());
    let old = layer.clone();
    layer.width = width as u32;
    layer.height = height as u32;
    let moved = geometry::bounds_point(layer, Point::default());
    layer.x += anchor.x - moved.x;
    layer.y += anchor.y - moved.y;
    if let Some(mask) = &old.mask {
        let mut mask = mask.as_ref().clone();
        if mask.placement.is_none() {
            mask.placement = Some(MaskPlacement::of(&old));
        }
        layer.mask = Some(std::sync::Arc::new(mask));
    }
    Ok(())
}
pub fn top_offset(paragraph: &Paragraph) -> f32 {
    let (_, metrics) = paragraph.get_font_at(0).metrics();
    // An empty paragraph has no glyph font at index zero. Subtracting its baseline
    // from zero metrics would place the native input above the new text box.
    if metrics.ascent == 0. {
        return 0.;
    }
    -paragraph.alphabetic_baseline()
        - metrics.ascent
        - metrics.underline_position().unwrap_or(0.)
        - metrics.underline_thickness().unwrap_or(0.)
}
pub fn utf16_at(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    text[..byte].encode_utf16().count()
}
pub fn byte_at(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= utf16 {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}
pub fn hit(layer: &Layer, world: Point) -> usize {
    let Content::Text { text, .. } = layer.content.as_ref() else {
        return 0;
    };
    let p = geometry::to_local(layer, world);
    let para = paragraph(layer, Some((layer.width as f32 - 24.).max(1.)));
    let pos = para.get_glyph_position_at_coordinate((
        p.x as f32 - PADDING,
        p.y as f32 - PADDING - top_offset(&para),
    ));
    byte_at(text, pos.position.max(0) as usize)
}
pub fn caret(layer: &Layer, byte: usize) -> Rect {
    caret_with_affinity(layer, byte, false)
}
/// NSTextView double/triple-click semantics, adapted using Skia's Unicode word
/// boundaries. Paragraph selection includes its terminating newline.
pub fn unit_at(layer: &Layer, world: Point, paragraph_unit: bool) -> (usize, usize) {
    let Content::Text { text, .. } = layer.content.as_ref() else {
        return (0, 0);
    };
    let byte = hit(layer, world);
    if paragraph_unit {
        let start = text[..byte].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let end = text[byte..]
            .find('\n')
            .map(|i| byte + i + 1)
            .unwrap_or(text.len());
        (start, end)
    } else {
        let para = paragraph(layer, Some((layer.width as f32 - 24.).max(1.)));
        let range = para.get_word_boundary(utf16_at(text, byte) as u32);
        (byte_at(text, range.start), byte_at(text, range.end))
    }
}
pub fn caret_with_affinity(layer: &Layer, byte: usize, upstream: bool) -> Rect {
    let Content::Text {
        text, font_size, ..
    } = layer.content.as_ref()
    else {
        return Rect::default();
    };
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    let para = paragraph(layer, Some((layer.width as f32 - 24.).max(1.)));
    let index = utf16_at(text, byte);
    let total = text.encode_utf16().count();
    let boxes = para.get_rects_for_range(
        if upstream && index > 0 {
            index - 1..index
        } else {
            index..(index + 1).min(total)
        },
        RectHeightStyle::Max,
        RectWidthStyle::Tight,
    );
    let h = layer
        .text_layout
        .as_ref()
        .map(|v| v.leading)
        .filter(|v| *v > 0.)
        .unwrap_or(font_size * 1.2) as f32;
    let mut rect = if let Some(b) = boxes.first() {
        Rect::from_xywh(
            if upstream ^ (b.direct == TextDirection::RTL) {
                b.rect.right
            } else {
                b.rect.left
            },
            b.rect.top,
            1.,
            b.rect.height(),
        )
    } else if index > 0 {
        para.get_rects_for_range(
            index - 1..index,
            RectHeightStyle::Max,
            RectWidthStyle::Tight,
        )
        .last()
        .map(|b| Rect::from_xywh(b.rect.right, b.rect.top, 1., b.rect.height()))
        .unwrap_or(Rect::from_xywh(0., 0., 1., h))
    } else {
        Rect::from_xywh(0., 0., 1., h)
    };
    if !upstream && byte > 0 && text[..byte].ends_with('\n') {
        rect = Rect::from_xywh(0., rect.top + h, 1., h);
    }
    rect.offset((PADDING, PADDING + top_offset(&para)));
    rect
}
/// Use the rendered paragraph's rows, so native input navigation follows soft wraps,
/// tracking and leading instead of the hidden input's logical newline layout.
pub fn navigate(
    layer: &Layer,
    byte: usize,
    upstream: bool,
    direction: TextNavigation,
    preferred_x: Option<f32>,
) -> (usize, bool, Option<f32>) {
    let Content::Text { text, .. } = layer.content.as_ref() else {
        return (0, false, None);
    };
    let para = paragraph(layer, Some((layer.width as f32 - 24.).max(1.)));
    let lines = para.get_line_metrics();
    if lines.is_empty() {
        return (0, false, None);
    }
    let caret = caret_with_affinity(layer, byte, upstream);
    let center = caret.center_y() - PADDING - top_offset(&para);
    let row = lines
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let distance = |l: &LineMetrics<'_>| {
                ((l.baseline + (l.descent - l.ascent) / 2.) as f32 - center).abs()
            };
            distance(a).total_cmp(&distance(b))
        })
        .map(|(i, _)| i)
        .unwrap_or(0);
    let x = preferred_x.unwrap_or(caret.left - PADDING);
    let (target, x, preferred) = match direction {
        TextNavigation::Up => (row.saturating_sub(1), x, Some(x)),
        TextNavigation::Down => ((row + 1).min(lines.len() - 1), x, Some(x)),
        TextNavigation::LineStart => (row, -100000., None),
        TextNavigation::LineEnd => (row, 100000., None),
    };
    let line = &lines[target];
    let y = (line.baseline + (line.descent - line.ascent) / 2.) as f32;
    let position = para.get_glyph_position_at_coordinate((x, y));
    (
        byte_at(text, position.position.max(0) as usize),
        position.affinity == Affinity::Upstream,
        preferred,
    )
}
pub fn overset(layer: &Layer) -> bool {
    if !matches!(layer.content.as_ref(), Content::Text { .. }) {
        return false;
    }
    let para = paragraph(layer, Some((layer.width as f32 - 24.).max(1.)));
    para.height() > (layer.height as f32 - 24.).max(1.) + 0.5
}
