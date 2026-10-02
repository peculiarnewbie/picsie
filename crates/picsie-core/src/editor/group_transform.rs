//! TransformGroup: one transform box for a folder or several selected layers.
//! Translated from Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
//!
//! Pinned sources:
//! - `Compositor/Document/LayerTransform.swift`: `TransformGroup`, `LayerTransform.following`,
//!   `LayerTransform.placing`, `TransformDrag.updated` (move/resize/rotate branches).
//! - `Compositor/Document/EditorSession.swift`: `transformsAsGroup`, `groupTransformMembers`,
//!   `groupTransformBox`, `beginTransform`, `commitTransform`, `nudgeLayer`, `pendingTransform`,
//!   `editedTransform`, `transformPixelSize`.
//! - `Compositor/Document/LayerFlip.swift`: `mirrored`, group `flipLayers`.
//! - `Compositor/UI/TransformInspector.swift`: box-backed X/Y/W/H/Scale/rotation fields.
//! - `Compositor/Rendering/TransformOverlay.swift`: one box around the members.
//! - `Compositor/Rendering/EditorCanvas.swift`: box press/drag routing, whole-pixel drag
//!   rounding, move snapping, one undo per gesture.
//!
//! Adaptations: Compositor's `CGAffineTransform` chain becomes a local 2-D affine that
//! matches `geometry::to_world` exactly (including pixel flips, which the on-canvas
//! bounds handles intentionally ignore). Layer placement is `x/y/scale/rotation/flip`
//! instead of `origin/size`, and Shift preserves proportions (`shift || lock`) per the
//! explicit local choice rather than upstream's `lock != shift`. Drag deltas keep
//! fractions like the local single-layer gesture instead of upstream's whole-pixel
//! rounding. Group distortion (Cmd-drag resampling of every member) is not ported;
//! handles resize and rotate only.
use super::*;
use crate::layer_index::LayerIndex;
use skia_safe::{self as sk};
use std::collections::{HashMap, HashSet};

/// Pre-edit state for an Apply/Cancel group transform (`TransformEdit.group`).
#[derive(Clone)]
pub(super) struct GroupState {
    /// The upright box around the members when the edit began (`TransformGroup.box`).
    pub(super) original: Layer,
    /// The box as currently drafted (`TransformEdit.draft`).
    pub(super) draft: Layer,
    /// Each member's transform when the edit began (`TransformGroup.originals`).
    pub(super) members: Vec<Layer>,
}

/// A pointer drag of the group box; the pre-edit members travel with the draft.
pub(super) struct GroupDrag {
    /// Box placement when this drag began (the persistent draft, if one is open).
    pub(super) base: Layer,
    /// Box placement when the whole edit began.
    pub(super) original: Layer,
    /// Member placements when the whole edit began.
    pub(super) members: Vec<Layer>,
    /// A persistent Apply/Cancel transaction was already open.
    pub(super) open: bool,
}

/// The upright box plus its on-canvas handle positions for the overlay.
pub struct GroupOverlay {
    pub corners: [Point; 4],
    pub handles: [Point; 8],
    pub rotate: Point,
}

/// `EditorSession.transformsAsGroup`: several layers selected, or one folder.
pub(super) fn transforms_as_group(editor: &Editor) -> bool {
    editor.selection.ids.len() > 1
        || (editor.selection.ids.len() == 1
            && editor
                .selected()
                .is_some_and(|l| matches!(l.content.as_ref(), Content::Group)))
}

/// `EditorSession.groupTransformMembers`: the visible pixel layers selected and inside
/// selected folders. A selected parent covers its descendants, so selecting both a
/// folder and its child still transforms the child once, from its original placement.
///
/// The hierarchy comes from the borrowed `LayerIndex`: parent hops and the
/// effective-visibility query are hash lookups, so deep nesting costs O(depth)
/// per layer instead of the nested linear scan of repeated `Document::effective`
/// calls. Membership semantics are unchanged.
///
/// Locked members are excluded: upstream has no locked field on its layers, but the
/// local engine excludes locked layers from every other multi-layer move (Nudge,
/// canvas translation), and group resize/rotate/numeric/flip must not bypass that
/// choice. Locking is per-layer, matching the existing move semantics: children of
/// a locked folder still transform, exactly as Nudge moves them. Hidden layers
/// (directly or via a hidden ancestor) stay out through effective visibility, and
/// blank paint layers stay out, matching upstream's `asset != nil` requirement.
///
/// Cross-branch integration (levels): that branch adds
/// `Layer.adjustment: Option<Adjustment>` over sourceless `Content::Paint`.
/// Such adjustment layers carry no pixels (upstream `asset == nil`) and must be
/// excluded alongside blank paint below when the branches combine; the field does
/// not exist on this branch, so this is documented, not implemented, here.
pub(super) fn group_members(index: &LayerIndex, selection: &Selection) -> Vec<Layer> {
    let doc = index.document;
    let grouped = selection.ids.len() > 1
        || (selection.ids.len() == 1
            && doc.layers.iter().any(|l| {
                selection.ids.contains(&l.id) && matches!(l.content.as_ref(), Content::Group)
            }));
    if !grouped {
        return vec![];
    }
    let selected: HashSet<&str> = selection.ids.iter().map(String::as_str).collect();
    doc.layers
        .iter()
        .filter(|layer| {
            if matches!(layer.content.as_ref(), Content::Group) {
                return false;
            }
            if layer.locked {
                return false;
            }
            if matches!(layer.content.as_ref(), Content::Paint) && layer.strokes.is_empty() {
                return false;
            }
            if !index.effective(layer).0 {
                return false;
            }
            let mut current = Some(*layer);
            for _ in 0..64 {
                let Some(node) = current else {
                    return false;
                };
                if selected.contains(node.id.as_str()) {
                    return true;
                }
                current = node.parent_id.as_deref().and_then(|id| index.layer(id));
            }
            false
        })
        .cloned()
        .collect()
}

/// `EditorSession.groupTransformBox`: the upright box around the members' rotated
/// corners. The pseudo layer keeps exact displayed dimensions through its scales, so
/// `geometry::resize`/`rotate`/`handles` operate on the box unchanged.
pub(super) fn group_box(members: &[Layer]) -> Option<Layer> {
    let corners: Vec<Point> = members.iter().flat_map(geometry::corners).collect();
    if corners.is_empty() {
        return None;
    }
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &corners {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let w = (max_x - min_x).max(1.);
    let h = (max_y - min_y).max(1.);
    if !(w.is_finite() && h.is_finite()) {
        return None;
    }
    // Normalize negative zero so numeric fields never show "-0".
    let normalize = |v: f64| if v == 0. { 0. } else { v };
    let (min_x, min_y) = (normalize(min_x), normalize(min_y));
    // Never validated or persisted; dimensions stay inside layer limits by construction.
    let width = w.round().clamp(1., 8192.) as u32;
    let height = h.round().clamp(1., 8192.) as u32;
    let mut box_layer = Layer::new("Group", width, height, Content::Group);
    // A stable, valid identifier: the pseudo layer is never persisted or validated
    // into a document, but geometry aside it must satisfy `Layer::validate`.
    box_layer.id = "00000000-0000-0000-0000-000000000000".to_owned();
    box_layer.x = min_x;
    box_layer.y = min_y;
    box_layer.scale_x = w / width as f64;
    box_layer.scale_y = h / height as f64;
    Some(box_layer)
}

/// Carry every member from its original placement to `draft`: box-relative
/// `following` plus linked-mask placement, skipping invalid members like
/// upstream's commit loop.
///
/// SHAPES INTEGRATION HOOK: members keep their vector content untouched here.
/// If the shapes branch caches document-pixel rasterization (corner radius, line
/// width), its redraw must hook into this carry path. It runs on every pointer
/// sample (the `GroupTransform` gesture arm in `editor.rs`), on every
/// numeric/nudge edit of a persistent draft (`preview_group_box` below), and the
/// mirror loop in `flip_group` needs the same hook. Deliberately not implemented
/// on this branch.
pub(super) fn carry_members(members: &[Layer], original: &Layer, draft: &Layer) -> Vec<Layer> {
    let mut carried = Vec::with_capacity(members.len());
    for member in members {
        let Some(mut next) = follow_member(member, original, draft) else {
            continue;
        };
        Editor::follow_mask(member, &mut next);
        if next.validate().is_err() {
            continue;
        }
        carried.push(next);
    }
    carried
}
/// One indexed pass replacing members by id: O(layers + members) instead of one
/// linear `Document.replace` per member on every pointer sample.
pub(super) fn replace_all(doc: &mut Document, layers: Vec<Layer>) {
    if layers.is_empty() {
        return;
    }
    let positions: HashMap<String, usize> = doc
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| (l.id.clone(), i))
        .collect();
    for layer in layers {
        if let Some(&i) = positions.get(&layer.id) {
            doc.layers[i] = layer;
        }
    }
}

/// Memoized member set + box for one document state and selection. The engine
/// derives group geometry from the overlay, cursor feedback, inspector target and
/// scale readout several times per frame. Members live in an immutable shared
/// `Arc`: repeated queries clone only the pointer, and a box-only getter avoids
/// touching the member list at all. The key is the selection plus a `retained_eq`
/// document check — the same cheap scalar/identity comparison the shared metadata
/// publisher already pays per capture — so unchanged frames reuse everything while
/// any edit, undo or selection change recomputes exactly once. No pixels are ever
/// walked: member clones share their immutable image/mask resources.
#[derive(Clone, Default)]
pub(super) struct GroupCache {
    selection: Selection,
    document: Option<Document>,
    members: Arc<Vec<Layer>>,
    box_layer: Option<Layer>,
}
impl GroupCache {
    fn valid(&self, selection: &Selection, doc: &Document) -> bool {
        &self.selection == selection && self.document.as_ref().is_some_and(|d| d.retained_eq(doc))
    }
    /// The cached box alone: no member clone, only one shallow layer copy.
    fn box_for(&self, selection: &Selection, doc: &Document) -> Option<Layer> {
        self.valid(selection, doc)
            .then(|| self.box_layer.clone())
            .flatten()
    }
    /// The shared member list: an `Arc` clone, never an element copy.
    fn members_for(&self, selection: &Selection, doc: &Document) -> Option<Arc<Vec<Layer>>> {
        self.valid(selection, doc).then(|| self.members.clone())
    }
    fn store(&mut self, selection: &Selection, doc: &Document) -> (Arc<Vec<Layer>>, Option<Layer>) {
        // One borrowed index per invalidation: hierarchy and visibility queries
        // inside are hash lookups, not nested linear scans.
        let index = LayerIndex::new(doc);
        let members = Arc::new(group_members(&index, selection));
        let box_layer = group_box(&members);
        self.selection = selection.clone();
        self.document = Some(doc.clone());
        self.members = members.clone();
        self.box_layer = box_layer.clone();
        (members, box_layer)
    }
}
/// 2-D affine map; `of` matches `BrushRaster.pixelToDocument` (unit square to document,
/// flips included) so `following` agrees with `geometry::to_world`.
#[derive(Clone, Copy)]
struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}
impl Affine {
    fn of(layer: &Layer) -> Self {
        let angle = layer.rotation.to_radians();
        let (sin, cos) = angle.sin_cos();
        let dw = layer.width as f64 * layer.scale_x;
        let dh = layer.height as f64 * layer.scale_y;
        let sx = if layer.flip_x { -1. } else { 1. };
        let sy = if layer.flip_y { -1. } else { 1. };
        let a = dw * sx * cos;
        let b = dw * sx * sin;
        let c = -dh * sy * sin;
        let d = dh * sy * cos;
        let center = geometry::center(layer);
        Self {
            a,
            b,
            c,
            d,
            tx: center.x - (a + c) / 2.,
            ty: center.y - (b + d) / 2.,
        }
    }
    fn inverted(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        Some(Self {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            tx: (self.c * self.ty - self.d * self.tx) / det,
            ty: (self.b * self.tx - self.a * self.ty) / det,
        })
    }
    /// Apply `self`, then `next` (`CGAffineTransform.concatenating` order).
    fn then(&self, next: &Self) -> Self {
        Self {
            a: self.a * next.a + self.b * next.c,
            b: self.a * next.b + self.b * next.d,
            c: self.c * next.a + self.d * next.c,
            d: self.c * next.b + self.d * next.d,
            tx: self.tx * next.a + self.ty * next.c + next.tx,
            ty: self.tx * next.b + self.ty * next.d + next.ty,
        }
    }
    fn apply(&self, p: Point) -> Point {
        Point::new(
            self.a * p.x + self.c * p.y + self.tx,
            self.b * p.x + self.d * p.y + self.ty,
        )
    }
}

fn same_placement(a: &Layer, b: &Layer) -> bool {
    a.x == b.x
        && a.y == b.y
        && a.scale_x == b.scale_x
        && a.scale_y == b.scale_y
        && a.rotation == b.rotation
        && a.flip_x == b.flip_x
        && a.flip_y == b.flip_y
}

/// `LayerTransform.following`: this member's placement carried along as its box moves
/// from `old` to `new`. A plain move carries exactly; anything else goes through the
/// box-relative affine with shear dropped, the horizontal flip kept, and the rotation
/// taken nearest the member's own (`placing`).
///
/// Intentional local adaptation: upstream keeps that nearest rotation unbounded, but
/// local layers validate to ±360° (`Layer::validate`), so the result is normalized to
/// ±180° — the same angle, and the only way a carried member stays committable.
/// Without this, a member near ±360° carried across the wrap would validate-fail and
/// be skipped.
pub(super) fn follow_member(member: &Layer, old: &Layer, new: &Layer) -> Option<Layer> {
    if same_placement(old, new) {
        return Some(member.clone());
    }
    let (old_w, old_h) = (
        old.width as f64 * old.scale_x,
        old.height as f64 * old.scale_y,
    );
    let (new_w, new_h) = (
        new.width as f64 * new.scale_x,
        new.height as f64 * new.scale_y,
    );
    if old_w == new_w
        && old_h == new_h
        && old.rotation == new.rotation
        && old.flip_x == new.flip_x
        && old.flip_y == new.flip_y
    {
        let mut moved = member.clone();
        moved.x += new.x - old.x;
        moved.y += new.y - old.y;
        return moved.validate().is_ok().then_some(moved);
    }
    let map = Affine::of(member)
        .then(&Affine::of(old).inverted()?)
        .then(&Affine::of(new));
    if ![map.a, map.b, map.c, map.d, map.tx, map.ty]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }
    let sign = if member.flip_x { -1. } else { 1. };
    let angle = (map.b * sign).atan2(map.a * sign);
    let along = -map.c * angle.sin() + map.d * angle.cos();
    let middle = map.apply(Point::new(0.5, 0.5));
    let dw = map.a.hypot(map.b);
    let dh = along.abs();
    if !(dw.is_finite() && dh.is_finite()) || member.width == 0 || member.height == 0 {
        return None;
    }
    let degrees = angle.to_degrees();
    let rotation = degrees + ((member.rotation - degrees) / 360.).round() * 360.;
    let mut next = member.clone();
    next.scale_x = dw / member.width as f64;
    next.scale_y = dh / member.height as f64;
    next.rotation = (rotation + 180.).rem_euclid(360.) - 180.;
    next.flip_y = along < 0.;
    next.x = middle.x - dw / 2.;
    next.y = middle.y - dh / 2.;
    next.validate().is_ok().then_some(next)
}

/// `LayerTransform.mirrored`: flip one member across the box axis for group Flip H/V
/// (`flipLayers`); the angle turns the other way and the middle crosses the axis.
pub(super) fn mirror_member(member: &Layer, horizontal: bool, axis: f64) -> Layer {
    let center = geometry::center(member);
    let mut next = member.clone();
    if horizontal {
        next.flip_x = !next.flip_x;
        next.x = 2. * axis - center.x - member.width as f64 * member.scale_x / 2.;
    } else {
        next.flip_y = !next.flip_y;
        next.y = 2. * axis - center.y - member.height as f64 * member.scale_y / 2.;
    }
    next.rotation = -member.rotation;
    next
}

impl Editor {
    /// Cached member set for the current selection and document. The returned
    /// `Arc` is shared: repeated queries (including box-only ones below) never
    /// copy member layers. Mask-target editing never uses the shared box.
    /// Exposed for UI/tests that need the member list without starting a draft.
    pub fn group_members(&self) -> Arc<Vec<Layer>> {
        if self.paint_target != PaintTarget::Content || !transforms_as_group(self) {
            return Arc::new(vec![]);
        }
        if let Some(hit) = self
            .group_cache
            .borrow()
            .members_for(&self.selection, &self.history.document)
        {
            return hit;
        }
        self.group_cache
            .borrow_mut()
            .store(&self.selection, &self.history.document)
            .0
    }
    /// The cached box alone, without cloning any member.
    pub(super) fn cached_group_box(&self) -> Option<Layer> {
        if self.paint_target != PaintTarget::Content || !transforms_as_group(self) {
            return None;
        }
        if let Some(hit) = self
            .group_cache
            .borrow()
            .box_for(&self.selection, &self.history.document)
        {
            return Some(hit);
        }
        self.group_cache
            .borrow_mut()
            .store(&self.selection, &self.history.document)
            .1
    }
    /// The box an edit would carry: the persistent draft, else the live box.
    /// Mirrors `editedTransform`/`pendingTransform` for the group case.
    pub fn edited_group_box(&self) -> Option<Layer> {
        if let Some(state) = &self.group_transform {
            return Some(state.draft.clone());
        }
        self.cached_group_box()
    }
    /// Overlay geometry for the canvas: the draft while editing, else the live box.
    /// Shown for the Move tool while controls are visible or a transform is active,
    /// like `TransformOverlay.geometry`'s group branch.
    pub fn group_overlay(&self) -> Option<GroupOverlay> {
        if self.tool != Tool::Move
            || !(self.view_options.show_controls || self.transform_active())
            || self.paint_target == PaintTarget::Mask
            || self.distortion_corners().is_some()
        {
            return None;
        }
        let box_layer = self.edited_group_box()?;
        let corners: [Point; 4] = geometry::corners(&box_layer).try_into().ok()?;
        let handles = geometry::handles(&box_layer, self.viewport.zoom / self.display_scale);
        let points: Vec<Point> = handles.into_iter().map(|(_, p)| p).collect();
        Some(GroupOverlay {
            corners,
            handles: points[..8].try_into().ok()?,
            rotate: points[8],
        })
    }
    /// Canvas overlay for the group box: one outline with eight resize handles
    /// and a rotation stalk (`TransformOverlay` drawing its group geometry).
    /// Returns whether anything was drawn, so the caller can suppress the
    /// per-layer outlines underneath it.
    pub fn draw_group_overlay(&self, canvas: &sk::Canvas) -> bool {
        let Some(overlay) = self.group_overlay() else {
            return false;
        };
        let origin = geometry::canvas_origin(&self.history.document, &self.viewport);
        let map = |p: Point| {
            sk::Point::new(
                (origin.x + p.x * self.viewport.zoom) as f32,
                (origin.y + p.y * self.viewport.zoom) as f32,
            )
        };
        let mut paint = sk::Paint::default();
        paint
            .set_color(sk::Color::from_rgb(155, 171, 255))
            .set_style(skia_safe::paint::Style::Stroke)
            .set_stroke_width(1.);
        let mut path = sk::PathBuilder::new();
        path.move_to(map(overlay.corners[0]));
        for p in &overlay.corners[1..] {
            path.line_to(map(*p));
        }
        path.close();
        canvas.draw_path(&path.detach(), &paint);
        let mut fill = sk::Paint::default();
        fill.set_color(sk::Color::WHITE);
        canvas.draw_line(map(overlay.handles[1]), map(overlay.rotate), &paint);
        for point in &overlay.handles {
            let p = map(*point);
            let rect = sk::Rect::from_xywh(p.x - 3., p.y - 3., 6., 6.);
            canvas.draw_rect(rect, &fill);
            canvas.draw_rect(rect, &paint);
        }
        let at = map(overlay.rotate);
        canvas.draw_circle(at, 5., &fill);
        canvas.draw_circle(at, 5., &paint);
        true
    }
    /// Open the persistent Apply/Cancel group edit (`beginTransform`'s group branch).
    /// One transaction; every preview lands in it until commit or cancel.
    pub(super) fn begin_group_transform(&mut self) -> Result<bool> {
        if self.group_transform.is_some() {
            return Ok(true);
        }
        if self.floating.is_some() {
            return Ok(false);
        }
        let members = self.group_members();
        let Some(box_layer) = group_box(&members) else {
            return Ok(false);
        };
        self.finish_gesture()?;
        self.transform_pixel_size = Some((
            box_layer.width as f64 * box_layer.scale_x,
            box_layer.height as f64 * box_layer.scale_y,
        ));
        self.begin_edit("Transform Layers");
        self.group_transform = Some(GroupState {
            original: box_layer.clone(),
            draft: box_layer,
            // The once-per-transform member copy; queries share the Arc.
            members: members.as_ref().clone(),
        });
        self.layer_transform = true;
        self.tool = Tool::Move;
        Ok(true)
    }
    /// Carry every member from its original placement to `draft` in one preview.
    /// Invalid members are skipped like upstream's commit loop.
    pub(super) fn preview_group_box(&mut self, draft: Layer) -> Result<()> {
        let Some(state) = &self.group_transform else {
            return Ok(());
        };
        let w = draft.width as f64 * draft.scale_x;
        let h = draft.height as f64 * draft.scale_y;
        if !draft.x.is_finite() || !draft.y.is_finite() || !w.is_finite() || !h.is_finite() {
            return Ok(());
        }
        if w < 1. || h < 1. {
            return Ok(());
        }
        let original = state.original.clone();
        let members = state.members.clone();
        let mut doc = self.history.document.clone();
        // Member replacement is one indexed pass (see `replace_all`); shapes keep
        // vector content here — the shapes-branch redraw hooks into `carry_members`.
        replace_all(&mut doc, carry_members(&members, &original, &draft));
        doc.validate()?;
        self.history.preview(doc);
        if let Some(state) = &mut self.group_transform {
            state.draft = draft;
        }
        Ok(())
    }
    /// Mirror every member across the box middle as one undo step (`flipLayers`).
    /// An open draft commits first, as upstream does.
    pub(super) fn flip_group(&mut self, horizontal: bool) -> Result<()> {
        if self.group_transform.is_some() {
            self.commit_transform()?;
        }
        self.finish_gesture()?;
        let members = self.group_members();
        let Some(box_layer) = group_box(&members) else {
            return Ok(());
        };
        let center = geometry::center(&box_layer);
        let axis = if horizontal { center.x } else { center.y };
        let mut doc = self.history.document.clone();
        let mut carried = Vec::with_capacity(members.len());
        for member in members.iter() {
            // Mirror path: same shapes-branch redraw hook as `carry_members`.
            let mut next = mirror_member(member, horizontal, axis);
            Self::follow_mask(member, &mut next);
            next.validate()?;
            carried.push(next);
        }
        replace_all(&mut doc, carried);
        self.edit(
            if horizontal {
                "Flip Horizontal"
            } else {
                "Flip Vertical"
            },
            doc,
            None,
        )
    }
}
