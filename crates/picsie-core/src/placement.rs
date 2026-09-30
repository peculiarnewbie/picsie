//! Guides.swift, CanvasRulers.swift and TransformSnap translated from Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Units here are physical viewport pixels.
use crate::{geometry, model::*};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CanvasGuide {
    pub id: String,
    pub axis: GuideAxis,
    pub position: f64,
}
impl CanvasGuide {
    pub fn offset(&mut self, x: f64, y: f64) {
        self.position += if self.axis == GuideAxis::Vertical {
            x
        } else {
            y
        };
    }
    pub fn scale(&mut self, x: f64, y: f64) {
        self.position *= if self.axis == GuideAxis::Vertical {
            x
        } else {
            y
        };
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ViewOptions {
    pub rulers: bool,
    pub guides: bool,
    pub grid: bool,
    pub lock_guides: bool,
    pub snap: bool,
    pub snap_guides: bool,
    pub snap_grid: bool,
    pub snap_layers: bool,
    pub snap_bounds: bool,
    pub auto_select: bool,
    pub show_controls: bool,
}
impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            rulers: false,
            guides: true,
            grid: false,
            lock_guides: false,
            snap: true,
            snap_guides: true,
            snap_grid: false,
            snap_layers: true,
            snap_bounds: true,
            auto_select: false,
            show_controls: true,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct GuideDrag {
    pub guide: CanvasGuide,
    pub new: bool,
}

pub fn grid_lines(length: f64) -> Vec<f64> {
    (0..=(length.max(0.) / 8.).floor() as u32)
        .map(|i| i as f64 * 8.)
        .collect()
}
pub fn ruler_step(zoom: f64) -> f64 {
    let target = 70. / zoom.max(0.0001);
    [
        1., 2., 5., 10., 20., 25., 50., 100., 200., 250., 500., 1000., 2000., 2500., 5000., 10000.,
        20000., 25000.,
    ]
    .into_iter()
    .find(|v| *v >= target)
    .unwrap_or(50000.)
}
pub fn targets(
    doc: &Document,
    view: &ViewOptions,
    excluding: &[String],
    centers: bool,
) -> (Vec<f64>, Vec<f64>) {
    if !view.snap {
        return (vec![], vec![]);
    }
    let (mut xs, mut ys) = (vec![], vec![]);
    if view.snap_bounds {
        xs.extend([0., doc.width as f64]);
        ys.extend([0., doc.height as f64]);
        if centers {
            xs.push(doc.width as f64 / 2.);
            ys.push(doc.height as f64 / 2.);
        }
    }
    if view.snap_layers {
        for layer in doc.ordered_layers().into_iter().filter(|l| {
            !matches!(l.content.as_ref(), Content::Group)
                && (!matches!(l.content.as_ref(), Content::Paint) || !l.strokes.is_empty())
                && doc.effective(l).0
                && !excluding.contains(&l.id)
        }) {
            let (left, top, right, bottom) = bounds(layer);
            xs.extend([left.round(), right.round()]);
            ys.extend([top.round(), bottom.round()]);
            if centers {
                xs.push(((left + right) / 2.).round());
                ys.push(((top + bottom) / 2.).round());
            }
        }
    }
    if view.snap_grid && view.grid {
        xs.extend(grid_lines(doc.width as f64));
        ys.extend(grid_lines(doc.height as f64));
    }
    if view.snap_guides && view.guides {
        for guide in &doc.guides {
            if guide.axis == GuideAxis::Vertical {
                xs.push(guide.position);
            } else {
                ys.push(guide.position);
            }
        }
    }
    (xs, ys)
}
pub fn bounds(layer: &Layer) -> (f64, f64, f64, f64) {
    let corners = geometry::corners(layer);
    (
        corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min),
        corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min),
        corners
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max),
        corners
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max),
    )
}
pub fn nearest(values: &[f64], targets: &[f64], tolerance: f64) -> (f64, Option<f64>) {
    let mut best: Option<(f64, f64)> = None;
    for value in values {
        for target in targets {
            let delta = target - value;
            if delta.abs() <= tolerance && best.is_none_or(|(d, _)| delta.abs() < d.abs()) {
                best = Some((delta, *target));
            }
        }
    }
    best.map(|(delta, target)| (delta, Some(target)))
        .unwrap_or((0., None))
}

/// Retained native rulers. Labels rotate with the vertical ruler, as CanvasRulerNSView.
#[derive(Default)]
pub struct Rulers {
    key: Option<[u64; 9]>,
    images: Vec<(String, std::sync::Arc<crate::thumbnail::Thumbnail>)>,
}
impl Rulers {
    pub fn update(
        &mut self,
        doc: &Document,
        viewport: &geometry::Viewport,
        scale: f64,
        enabled: bool,
    ) -> anyhow::Result<Vec<(String, std::sync::Arc<crate::thumbnail::Thumbnail>)>> {
        if !enabled {
            self.key = None;
            self.images.clear();
            return Ok(vec![]);
        }
        let key = [
            doc.width as u64,
            doc.height as u64,
            viewport.width.to_bits(),
            viewport.height.to_bits(),
            viewport.zoom.to_bits(),
            viewport.pan.x.to_bits(),
            viewport.pan.y.to_bits(),
            scale.to_bits(),
            1,
        ];
        if self.key != Some(key) {
            let mut images = vec![];
            for axis in [GuideAxis::Horizontal, GuideAxis::Vertical] {
                let horizontal = axis == GuideAxis::Horizontal;
                let length = if horizontal {
                    viewport.width
                } else {
                    viewport.height
                };
                let thickness = (18. * scale).round().max(1.) as u32;
                let (w, h) = if horizontal {
                    (length.round().max(1.) as u32, thickness)
                } else {
                    (thickness, length.round().max(1.) as u32)
                };
                let mut surface = crate::render::surface(w, h)?;
                let c = surface.canvas();
                c.clear(skia_safe::Color::from_rgb(51, 51, 51));
                let origin = geometry::canvas_origin(doc, viewport);
                let offset = if horizontal { origin.x } else { origin.y };
                let step = ruler_step(viewport.zoom / scale);
                let minor = step / 10.;
                let first = (-offset / viewport.zoom / minor).floor() * minor;
                let last = ((length - offset) / viewport.zoom / minor).ceil() * minor;
                let typeface = skia_safe::FontMgr::new()
                    .match_family_style("monospace", skia_safe::FontStyle::normal())
                    .ok_or_else(|| anyhow::anyhow!("No ruler font"))?;
                let font = skia_safe::Font::new(typeface, (8. * scale) as f32);
                let mut paint = skia_safe::Paint::default();
                paint.set_color(skia_safe::Color::from_rgb(158, 158, 158));
                let mut value = first;
                while value <= last + 0.001 {
                    let pos = (offset + value * viewport.zoom) as f32;
                    let major = (value / step - (value / step).round()).abs() < 0.001;
                    let mid = (value / (step / 2.) - (value / (step / 2.)).round()).abs() < 0.001;
                    let tick = (if major {
                        8.
                    } else if mid {
                        5.
                    } else {
                        3.
                    }) * scale as f32;
                    let r = if horizontal {
                        skia_safe::Rect::from_xywh(pos, thickness as f32 - tick, scale as f32, tick)
                    } else {
                        skia_safe::Rect::from_xywh(thickness as f32 - tick, pos, tick, scale as f32)
                    };
                    c.draw_rect(r, &paint);
                    if major {
                        let label = if value.round() == 0. {
                            "0".to_owned()
                        } else {
                            format!("{:.0}", value)
                        };
                        paint.set_color(skia_safe::Color::from_rgb(199, 199, 199));
                        if horizontal {
                            c.draw_str(
                                label,
                                (pos + 2. * scale as f32, 8. * scale as f32),
                                &font,
                                &paint,
                            );
                        } else {
                            c.save();
                            c.translate((8. * scale as f32, pos + 2. * scale as f32));
                            c.rotate(90., None);
                            c.draw_str(label, (0., 0.), &font, &paint);
                            c.restore();
                        }
                        paint.set_color(skia_safe::Color::from_rgb(158, 158, 158));
                    }
                    value += minor;
                }
                images.push((
                    if horizontal {
                        "@ruler-horizontal"
                    } else {
                        "@ruler-vertical"
                    }
                    .into(),
                    std::sync::Arc::new(crate::thumbnail::Thumbnail {
                        width: w,
                        height: h,
                        pixels: crate::render::bgra_pixels(&mut surface)?,
                    }),
                ));
            }
            self.images = images;
            self.key = Some(key);
        }
        Ok(self.images.clone())
    }
}
