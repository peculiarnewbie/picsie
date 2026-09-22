//! Translated from Compositor LayerTransform.swift, MIT © 2026 Wonder Assembly LLC.
//! Source dimensions + scale retain compatibility with legacy Electropic v1. Shift preserves aspect.
use crate::model::{Document, Layer, Point};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
    pub pan: Point,
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 936.,
            height: 734.,
            zoom: 0.7,
            pan: Point::default(),
        }
    }
}
pub fn center(l: &Layer) -> Point {
    Point::new(
        l.x + l.width as f64 * l.scale_x / 2.,
        l.y + l.height as f64 * l.scale_y / 2.,
    )
}
pub fn to_local(l: &Layer, p: Point) -> Point {
    let c = center(l);
    let a = -l.rotation.to_radians();
    let (x, y) = (p.x - c.x, p.y - c.y);
    Point::new(
        (x * a.cos() - y * a.sin()) / (l.scale_x * if l.flip_x { -1. } else { 1. })
            + l.width as f64 / 2.,
        (x * a.sin() + y * a.cos()) / (l.scale_y * if l.flip_y { -1. } else { 1. })
            + l.height as f64 / 2.,
    )
}
pub fn to_world(l: &Layer, p: Point) -> Point {
    let a = l.rotation.to_radians();
    let c = center(l);
    let x = (p.x - l.width as f64 / 2.) * l.scale_x * if l.flip_x { -1. } else { 1. };
    let y = (p.y - l.height as f64 / 2.) * l.scale_y * if l.flip_y { -1. } else { 1. };
    Point::new(
        c.x + x * a.cos() - y * a.sin(),
        c.y + x * a.sin() + y * a.cos(),
    )
}
pub fn bounds_point(l: &Layer, p: Point) -> Point {
    let c = center(l);
    let a = l.rotation.to_radians();
    let x = (p.x - 0.5) * l.width as f64 * l.scale_x;
    let y = (p.y - 0.5) * l.height as f64 * l.scale_y;
    Point::new(
        c.x + x * a.cos() - y * a.sin(),
        c.y + x * a.sin() + y * a.cos(),
    )
}
pub const HANDLES: [(&str, f64, f64); 8] = [
    ("nw", 0., 0.),
    ("n", 0.5, 0.),
    ("ne", 1., 0.),
    ("e", 1., 0.5),
    ("se", 1., 1.),
    ("s", 0.5, 1.),
    ("sw", 0., 1.),
    ("w", 0., 0.5),
];
pub fn corners(l: &Layer) -> Vec<Point> {
    [(0., 0.), (1., 0.), (1., 1.), (0., 1.)]
        .map(|(x, y)| bounds_point(l, Point::new(x, y)))
        .to_vec()
}
pub fn handles(l: &Layer, zoom: f64) -> Vec<(&'static str, Point)> {
    let mut h: Vec<_> = HANDLES
        .iter()
        .map(|&(k, x, y)| (k, bounds_point(l, Point::new(x, y))))
        .collect();
    let top = h[1].1;
    let c = center(l);
    let d = top.distance(c).max(1.);
    h.push((
        "rotate",
        Point::new(
            top.x + (top.x - c.x) / d * 28. / zoom,
            top.y + (top.y - c.y) / d * 28. / zoom,
        ),
    ));
    h
}
pub fn hit_handle(l: &Layer, p: Point, zoom: f64) -> Option<&'static str> {
    handles(l, zoom)
        .into_iter()
        .filter(|(_, v)| v.distance(p) * zoom <= 6.)
        .min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p)))
        .map(|(k, _)| k)
}
pub fn resize(l: &Layer, handle: &str, p: Point, preserve: bool, from_center: bool) -> Layer {
    let &(_, hx, hy) = HANDLES
        .iter()
        .find(|h| h.0 == handle)
        .expect("resize handle");
    let (ax, ay) = if from_center {
        (0.5, 0.5)
    } else {
        (1. - hx, 1. - hy)
    };
    let anchor = bounds_point(l, Point::new(ax, ay));
    let a = l.rotation.to_radians();
    let (dx, dy) = (p.x - anchor.x, p.y - anchor.y);
    let span = if from_center { 2. } else { 1. };
    let x = (dx * a.cos() + dy * a.sin()) * span;
    let y = (-dx * a.sin() + dy * a.cos()) * span;
    let (sx, sy) = (hx * 2. - 1., hy * 2. - 1.);
    let (ow, oh) = (l.width as f64 * l.scale_x, l.height as f64 * l.scale_y);
    let rw = if sx == 0. { ow } else { x * sx };
    let rh = if sy == 0. { oh } else { y * sy };
    let (mx, my) = (rw < 0., rh < 0.);
    let (mut w, mut h) = (rw.abs().max(1.), rh.abs().max(1.));
    if preserve {
        let factor = if sx == 0. {
            h / oh
        } else if sy == 0. {
            w / ow
        } else {
            (1. / ow.min(oh)).max((x * sx * ow + y * sy * oh) / (ow * ow + oh * oh))
        };
        let bounded = factor.clamp(
            0.01 / l.scale_x.min(l.scale_y),
            100. / l.scale_x.max(l.scale_y),
        );
        w = ow * bounded;
        h = oh * bounded;
    } else {
        w = w.clamp(l.width as f64 * 0.01, l.width as f64 * 100.);
        h = h.clamp(l.height as f64 * 0.01, l.height as f64 * 100.);
    }
    let ox = (0.5 - ax) * w * if mx { -1. } else { 1. };
    let oy = (0.5 - ay) * h * if my { -1. } else { 1. };
    let mut n = l.clone();
    n.x = anchor.x + ox * a.cos() - oy * a.sin() - w / 2.;
    n.y = anchor.y + ox * a.sin() + oy * a.cos() - h / 2.;
    n.scale_x = w / l.width as f64;
    n.scale_y = h / l.height as f64;
    n.flip_x = l.flip_x ^ mx;
    n.flip_y = l.flip_y ^ my;
    n
}
pub fn rotate(l: &Layer, origin: Point, p: Point, snap: bool) -> Layer {
    let c = center(l);
    let delta = (p.y - c.y).atan2(p.x - c.x) - (origin.y - c.y).atan2(origin.x - c.x);
    let r = l.rotation + delta.to_degrees();
    let degrees = if snap { (r / 15.).round() * 15. } else { r };
    let mut n = l.clone();
    n.rotation = (degrees + 180.).rem_euclid(360.) - 180.;
    n
}
pub fn hit_test(doc: &Document, p: Point) -> Option<&Layer> {
    doc.layers.iter().rev().find(|l| {
        if !l.visible || l.locked || l.opacity == 0. {
            return false;
        }
        let p = to_local(l, p);
        p.x >= 0. && p.y >= 0. && p.x <= l.width as f64 && p.y <= l.height as f64
    })
}
pub fn canvas_origin(doc: &Document, v: &Viewport) -> Point {
    Point::new(
        (v.width - doc.width as f64 * v.zoom) / 2. + v.pan.x,
        (v.height - doc.height as f64 * v.zoom) / 2. + v.pan.y,
    )
}
pub fn to_document(doc: &Document, v: &Viewport, p: Point) -> Point {
    let o = canvas_origin(doc, v);
    Point::new((p.x - o.x) / v.zoom, (p.y - o.y) / v.zoom)
}
pub fn zoom_at(doc: &Document, v: &Viewport, p: Point, zoom: f64) -> Viewport {
    let a = to_document(doc, v, p);
    let z = zoom.clamp(0.05, 8.);
    Viewport {
        zoom: z,
        pan: Point::new(
            p.x - a.x * z - (v.width - doc.width as f64 * z) / 2.,
            p.y - a.y * z - (v.height - doc.height as f64 * z) / 2.,
        ),
        ..v.clone()
    }
}
