//! Levels and Curves adjustment panels, following Compositor's LevelsSheet and
//! CurvesControls (609dbeae, MIT © 2026 Wonder Assembly LLC). Engine math stays
//! in `picsie-core::adjustment`; this module only presents published settings,
//! histogram bins and engine-evaluated curve samples, and routes control input
//! back through typed engine commands.
use super::*;
use crate::state::AdjustmentEditInfo;
use picsie_core::adjustment::{
    AdjustmentKind, CurvesSettings, LevelsAuto, LevelsChannel, LevelsSample, LevelsSettings,
};

pub(super) const HISTOGRAM_WIDTH: u32 = 392;
pub(super) const HISTOGRAM_HEIGHT: u32 = 150;
pub(super) const HANDLES_HEIGHT: u32 = 20;
pub(super) const RAMP_HEIGHT: f32 = 14.;
pub(super) const CURVE_WIDTH: u32 = 392;
pub(super) const CURVE_HEIGHT: u32 = 260;
pub(super) const ADJUSTMENT_PANEL_WIDTH: f32 = 440.;

fn channel_color(channel: LevelsChannel) -> [u8; 3] {
    match channel {
        LevelsChannel::Rgb => [160, 160, 160],
        LevelsChannel::Red => [230, 110, 110],
        LevelsChannel::Green => [110, 200, 110],
        LevelsChannel::Blue => [110, 150, 230],
    }
}

/// Presentation raster of the published histogram bins for one channel.
/// Bins and the display peak come from the engine; only pixel filling is local.
pub(super) fn histogram_image(bins: &[f64], peak: f64, channel: LevelsChannel) -> image::RgbaImage {
    let mut image = image::RgbaImage::new(HISTOGRAM_WIDTH, HISTOGRAM_HEIGHT);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([24, 24, 24, 255]);
    }
    if peak <= 0. || bins.len() < 256 {
        return image;
    }
    let color = channel_color(channel);
    for x in 0..HISTOGRAM_WIDTH {
        let bin = bins[(x as usize * 256 / HISTOGRAM_WIDTH as usize).min(255)];
        let height = ((bin / peak).clamp(0., 1.) * HISTOGRAM_HEIGHT as f64) as u32;
        for y in 0..height {
            image.put_pixel(
                x,
                HISTOGRAM_HEIGHT - 1 - y,
                image::Rgba([color[0], color[1], color[2], 255]),
            );
        }
    }
    image
}

/// Upward triangle markers for the draggable input/output handles, drawn at
/// the exact 255-domain positions the hit test below uses.
fn paint_triangle(image: &mut image::RgbaImage, x: i32, color: [u8; 4]) {
    let (w, h) = (image.width() as i32, image.height() as i32);
    for r in 0..10 {
        let half = r * 11 / 18;
        let y = h - 1 - r;
        for dx in -half..=half {
            let xx = x + dx;
            if xx >= 0 && y >= 0 && xx < w && y < h {
                image.put_pixel(xx as u32, y as u32, image::Rgba(color));
            }
        }
    }
}

fn handle_x(value: f64) -> i32 {
    (value.clamp(0., 255.) / 255. * (HISTOGRAM_WIDTH - 1) as f64).round() as i32
}

/// Input black/gamma/white handle strip under the histogram, like LevelsSheet.
pub(super) fn levels_input_handles_image(
    black_point: f64,
    gamma: f64,
    white_point: f64,
) -> image::RgbaImage {
    let mut image = image::RgbaImage::new(HISTOGRAM_WIDTH, HANDLES_HEIGHT);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([24, 24, 24, 255]);
    }
    paint_triangle(&mut image, handle_x(black_point), [10, 10, 10, 255]);
    let position = black_point + (white_point - black_point) * 0.5f64.powf(gamma);
    paint_triangle(&mut image, handle_x(position), [150, 150, 150, 255]);
    paint_triangle(&mut image, handle_x(white_point), [245, 245, 245, 255]);
    image
}

/// Output black/white handle strip under the output ramp, like LevelsSheet.
pub(super) fn levels_output_handles_image(out_black: f64, out_white: f64) -> image::RgbaImage {
    let mut image = image::RgbaImage::new(HISTOGRAM_WIDTH, HANDLES_HEIGHT);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([24, 24, 24, 255]);
    }
    paint_triangle(&mut image, handle_x(out_black), [10, 10, 10, 255]);
    paint_triangle(&mut image, handle_x(out_white), [245, 245, 245, 255]);
    image
}

/// Presentation raster of the engine-evaluated curve polyline and its control
/// points. Geometry is published by the engine; only pixel filling is local.
pub(super) fn curve_image(
    samples: &[[f64; 2]],
    points: &[picsie_core::adjustment::CurvePoint],
    selected: Option<usize>,
) -> image::RgbaImage {
    let mut image = image::RgbaImage::new(CURVE_WIDTH, CURVE_HEIGHT);
    for pixel in image.pixels_mut() {
        *pixel = image::Rgba([18, 18, 18, 255]);
    }
    let px = |x: f64| (x / 255. * (CURVE_WIDTH - 1) as f64).round() as i32;
    let py = |y: f64| ((1. - y / 255.) * (CURVE_HEIGHT - 1) as f64).round() as i32;
    for i in 0..=4 {
        let gx = (i * (CURVE_WIDTH - 1) / 4) as u32;
        let gy = (i * (CURVE_HEIGHT - 1) / 4) as u32;
        for y in 0..CURVE_HEIGHT {
            image.put_pixel(gx, y, image::Rgba([48, 48, 48, 255]));
        }
        for x in 0..CURVE_WIDTH {
            image.put_pixel(x, gy, image::Rgba([48, 48, 48, 255]));
        }
    }
    let mut dot = |x: i32, y: i32, color: [u8; 4]| {
        dot_pixel(&mut image, x, y, color);
    };
    let mut previous: Option<(i32, i32)> = None;
    for sample in samples {
        let at = (px(sample[0]), py(sample[1]));
        if let Some((x0, y0)) = previous {
            let steps = (at.0 - x0).abs().max((at.1 - y0).abs()).max(1);
            for i in 0..=steps {
                dot(
                    x0 + (at.0 - x0) * i / steps,
                    y0 + (at.1 - y0) * i / steps,
                    [235, 235, 235, 255],
                );
            }
        }
        previous = Some(at);
    }
    for (i, point) in points.iter().enumerate() {
        let color = if selected == Some(i) {
            [117, 167, 217, 255]
        } else {
            [235, 235, 235, 255]
        };
        // CurvesControls marks handles with 8-point round dots.
        dot_circle(&mut image, px(point.x), py(point.y), color);
    }
    image
}

fn dot_pixel(image: &mut image::RgbaImage, x: i32, y: i32, color: [u8; 4]) {
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (xx, yy) = (x + dx, y + dy);
            if xx >= 0 && yy >= 0 && xx < CURVE_WIDTH as i32 && yy < CURVE_HEIGHT as i32 {
                image.put_pixel(xx as u32, yy as u32, image::Rgba(color));
            }
        }
    }
}

fn dot_circle(image: &mut image::RgbaImage, x: i32, y: i32, color: [u8; 4]) {
    for dy in -4..=4 {
        for dx in -4..=4 {
            if dx * dx + dy * dy > 20 {
                continue;
            }
            let (xx, yy) = (x + dx, y + dy);
            if xx >= 0 && yy >= 0 && xx < CURVE_WIDTH as i32 && yy < CURVE_HEIGHT as i32 {
                image.put_pixel(xx as u32, yy as u32, image::Rgba(color));
            }
        }
    }
}

fn sample_mode_name(mode: LevelsSample) -> &'static str {
    match mode {
        LevelsSample::Black => "black",
        LevelsSample::Gray => "gray",
        LevelsSample::White => "white",
    }
}

/// GPUI RenderImage consumes straight-alpha BGRA (see `engine::bgra`); chart
/// painters above work in RGBA, so swizzle exactly once at this upload
/// boundary to preserve source channel colors and the selected-point blue.
fn bgra_chart_image(mut buffer: image::RgbaImage) -> image::RgbaImage {
    for pixel in buffer.pixels_mut() {
        let red_channel = pixel[0];
        pixel[0] = pixel[2];
        pixel[2] = red_channel;
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::{bgra_chart_image, curve_image, histogram_image};
    use picsie_core::adjustment::{CurvePoint, LevelsChannel};

    #[test]
    fn chart_upload_presents_source_channel_colors() {
        // Source red (230,110,110) must reach the BGRA consumer unswapped.
        let mut bins = vec![0f64; 256];
        bins[200] = 10.;
        let uploaded = bgra_chart_image(histogram_image(&bins, 10., LevelsChannel::Red));
        assert_eq!(uploaded.get_pixel(307, 0).0, [110, 110, 230, 255]);
        // The selected curve point keeps its source blue, not swapped orange.
        let uploaded = bgra_chart_image(curve_image(
            &[[0., 0.], [255., 255.]],
            &[CurvePoint { x: 128., y: 128. }],
            Some(0),
        ));
        assert_eq!(uploaded.get_pixel(196, 129).0, [217, 167, 117, 255]);
    }
}

impl Desktop {
    /// Hash of everything the charts draw, so charts rebuild only on change.
    /// All 1,024 histogram bins, display peaks, settings and the selected
    /// curve point participate: the charts are tiny metadata rasters.
    pub(super) fn adjustment_chart_key(&self, state: &Snapshot) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let edit = state.adjustment_edit.as_ref()?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        edit.layer_id.hash(&mut hash);
        (edit.kind == AdjustmentKind::Levels).hash(&mut hash);
        edit.levels.channel.index().hash(&mut hash);
        for range in &edit.levels.ranges {
            for v in [
                range.black,
                range.gamma,
                range.white,
                range.output_black,
                range.output_white,
            ] {
                v.to_bits().hash(&mut hash);
            }
        }
        edit.curves.channel.index().hash(&mut hash);
        for points in &edit.curves.channels {
            points.len().hash(&mut hash);
            for p in points {
                p.x.to_bits().hash(&mut hash);
                p.y.to_bits().hash(&mut hash);
            }
        }
        for bins in &edit.histogram {
            for v in bins.iter() {
                v.to_bits().hash(&mut hash);
            }
        }
        for peak in &edit.histogram_peak {
            peak.to_bits().hash(&mut hash);
        }
        // The curve chart highlights the selected point; selection alone must
        // rebuild it.
        self.curves_selected.hash(&mut hash);
        Some(hash.finish())
    }

    /// (Re)build histogram/curve chart images when the edit changes. Called
    /// from `receive`, which owns the window needed to retire old images.
    pub(super) fn sync_adjustment_charts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = self
            .state
            .as_ref()
            .and_then(|state| self.adjustment_chart_key(state));
        if key == self.adjustment_chart_key {
            return;
        }
        for (_, old) in self.adjustment_charts.drain() {
            let _ = window.drop_image(old);
        }
        self.adjustment_chart_key = key;
        let Some(state) = &self.state else { return };
        let Some(edit) = &state.adjustment_edit else {
            return;
        };
        let mut insert = |name: &'static str, buffer: image::RgbaImage| {
            self.adjustment_charts.insert(
                name,
                Arc::new(RenderImage::new(vec![image::Frame::new(bgra_chart_image(
                    buffer,
                ))])),
            );
        };
        if edit.kind == AdjustmentKind::Levels {
            let channel = edit.levels.channel;
            let bins = edit
                .histogram
                .get(channel.index())
                .cloned()
                .unwrap_or_default();
            let peak = edit
                .histogram_peak
                .get(channel.index())
                .copied()
                .unwrap_or(0.);
            insert("histogram", histogram_image(&bins, peak, channel));
            let current = edit.levels.ranges[channel.index()];
            insert(
                "levels-input-handles",
                levels_input_handles_image(current.black, current.gamma, current.white),
            );
            insert(
                "levels-output-handles",
                levels_output_handles_image(current.output_black, current.output_white),
            );
        } else {
            let points = edit.curves.channels[edit.curves.channel.index()].clone();
            insert(
                "curve",
                curve_image(&edit.curve_samples, &points, self.curves_selected),
            );
        }
        let _ = cx;
    }

    /// Sidebar reopen affordance while idle. The active editor lives in the
    /// floating adjustment panel, like upstream's modeless Levels/Curves UI.
    pub(super) fn adjustment_section(&self, state: &Snapshot, cx: &Context<Self>) -> Option<Div> {
        if state.adjustment_edit.is_some() {
            return None;
        }
        let layer = state.selected().filter(|l| l.adjustment.is_some())?;
        let kind = match layer.adjustment {
            Some(AdjustmentKind::Levels) => "Levels",
            Some(AdjustmentKind::Curves) => "Curves",
            None => return None,
        };
        Some(section("Adjustment").child(self.command_button(
            "adjustment-edit",
            &format!("Edit {kind}…"),
            Command::BeginAdjustmentEdit {
                id: layer.id.clone(),
            },
            layer.locked,
            false,
            cx,
        )))
    }

    fn levels_editor(
        &self,
        state: &Snapshot,
        edit: &AdjustmentEditInfo,
        cx: &Context<Self>,
    ) -> Div {
        let locked = state.layer(&edit.layer_id).is_none_or(|l| l.locked);
        // LevelsSheet layout: channel picker, 150pt histogram with input
        // handles, Black/Gamma/White fields, output ramp with output handles,
        // output fields, sample eyedroppers, Auto, Preview/Reset, Cancel/Apply.
        let mut editor = column().gap(px(12.)).child(
            self.probe(
                "levels-channel-row",
                Select::new(&self.selects["levels-channel"])
                    .small()
                    .h(px(26.))
                    .bg(rgb(0x202020))
                    .text_size(px(12.))
                    .w(px(180.))
                    .disabled(locked || self.busy),
            )
            .flex_shrink_0(),
        );
        if let Some(chart) = self.adjustment_charts.get("histogram") {
            editor = editor.child(
                self.probe(
                    "levels-histogram",
                    img(chart.clone())
                        .w(px(HISTOGRAM_WIDTH as f32))
                        .h(px(HISTOGRAM_HEIGHT as f32))
                        .rounded(px(3.)),
                ),
            );
        }
        if let Some(handles) = self.adjustment_charts.get("levels-input-handles") {
            editor = editor.child(
                self.probe(
                    "levels-input-handles",
                    img(handles.clone())
                        .w(px(HISTOGRAM_WIDTH as f32))
                        .h(px(HANDLES_HEIGHT as f32)),
                )
                .cursor(CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        this.commit_active_fields(window, cx);
                        this.levels_down(event.position, false, window, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        if !edit.preview {
            editor = editor.child(hint("Preview off: showing the original pixels."));
        }
        editor = editor
            .child(
                row()
                    .child(self.field("levels-black", "Black", locked))
                    .child(self.field("levels-gamma", "Gamma", locked))
                    .child(self.field("levels-white", "White", locked)),
            )
            .child(
                div().flex_shrink_0().child(
                    div()
                        .w(px(HISTOGRAM_WIDTH as f32))
                        .h(px(RAMP_HEIGHT))
                        .rounded(px(2.))
                        .bg(gpui_kit::linear_gradient(
                            90.,
                            gpui_kit::linear_color_stop(rgb(0x000000), 0.),
                            gpui_kit::linear_color_stop(rgb(0xffffff), 1.),
                        )),
                ),
            );
        if let Some(handles) = self.adjustment_charts.get("levels-output-handles") {
            editor = editor.child(
                self.probe(
                    "levels-output-handles",
                    img(handles.clone())
                        .w(px(HISTOGRAM_WIDTH as f32))
                        .h(px(HANDLES_HEIGHT as f32)),
                )
                .cursor(CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        this.commit_active_fields(window, cx);
                        this.levels_down(event.position, true, window, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        editor = editor
            .child(
                row()
                    .child(self.field("levels-out-black", "Output black", locked))
                    .child(self.field("levels-out-white", "Output white", locked)),
            )
            .child(
                row()
                    .gap(px(4.))
                    .child(label("Sample"))
                    .child(self.command_button(
                        "levels-sample-black",
                        "Black",
                        Command::SetLevelsSampleMode {
                            mode: if edit.sample_mode == Some(LevelsSample::Black) {
                                None
                            } else {
                                Some(LevelsSample::Black)
                            },
                        },
                        locked,
                        edit.sample_mode == Some(LevelsSample::Black),
                        cx,
                    ))
                    .child(self.command_button(
                        "levels-sample-gray",
                        "Gray",
                        Command::SetLevelsSampleMode {
                            mode: if edit.sample_mode == Some(LevelsSample::Gray) {
                                None
                            } else {
                                Some(LevelsSample::Gray)
                            },
                        },
                        locked,
                        edit.sample_mode == Some(LevelsSample::Gray),
                        cx,
                    ))
                    .child(self.command_button(
                        "levels-sample-white",
                        "White",
                        Command::SetLevelsSampleMode {
                            mode: if edit.sample_mode == Some(LevelsSample::White) {
                                None
                            } else {
                                Some(LevelsSample::White)
                            },
                        },
                        locked,
                        edit.sample_mode == Some(LevelsSample::White),
                        cx,
                    )),
            );
        if let Some(mode) = edit.sample_mode {
            editor = editor.child(hint(format!(
                "Click the canvas to set {} from the layers below.",
                sample_mode_name(mode)
            )));
        }
        editor = editor
            .child(
                row()
                    .gap(px(4.))
                    .child(label("Auto"))
                    .child(self.command_button(
                        "levels-auto-contrast",
                        "Contrast",
                        Command::AutoLevels {
                            mode: LevelsAuto::Contrast,
                        },
                        locked,
                        false,
                        cx,
                    ))
                    .child(self.command_button(
                        "levels-auto-color",
                        "Color",
                        Command::AutoLevels {
                            mode: LevelsAuto::Color,
                        },
                        locked,
                        false,
                        cx,
                    ))
                    .child(self.command_button(
                        "levels-auto-neutral",
                        "Neutral",
                        Command::AutoLevels {
                            mode: LevelsAuto::Neutral,
                        },
                        locked,
                        false,
                        cx,
                    )),
            )
            .child(
                row()
                    .child(self.command_button(
                        "levels-preview",
                        if edit.preview {
                            "Preview on"
                        } else {
                            "Preview off"
                        },
                        Command::SetAdjustmentPreview {
                            preview: !edit.preview,
                        },
                        false,
                        edit.preview,
                        cx,
                    ))
                    .child(self.command_button(
                        "levels-reset",
                        "Reset",
                        Command::UpdateAdjustmentLevels {
                            settings: LevelsSettings::default(),
                            preview: edit.preview,
                        },
                        locked,
                        false,
                        cx,
                    )),
            )
            .child(hint("Underlying pixels · alpha-weighted histogram"))
            .child(
                row()
                    .child(self.command_button(
                        "levels-cancel",
                        "Cancel",
                        Command::CancelAdjustmentEdit,
                        false,
                        false,
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(self.command_button(
                        "levels-apply",
                        "Apply",
                        Command::CommitAdjustmentEdit,
                        locked,
                        false,
                        cx,
                    )),
            );
        editor
    }

    fn curves_editor(
        &self,
        state: &Snapshot,
        edit: &AdjustmentEditInfo,
        cx: &Context<Self>,
    ) -> Div {
        let channel = edit.curves.channel;
        let points = edit.curves.channels[channel.index()].clone();
        let locked = state.layer(&edit.layer_id).is_none_or(|l| l.locked);
        // CurvesControls layout: channel picker, 260pt graph, add/drag hint,
        // point readout with interior-only removal, reset, Cancel/Apply.
        let mut editor = column().gap(px(12.)).child(
            self.probe(
                "curves-channel-row",
                Select::new(&self.selects["curves-channel"])
                    .small()
                    .h(px(26.))
                    .bg(rgb(0x202020))
                    .text_size(px(12.))
                    .w(px(180.))
                    .disabled(locked || self.busy),
            )
            .flex_shrink_0(),
        );
        if let Some(chart) = self.adjustment_charts.get("curve") {
            editor = editor.child(
                self.probe(
                    "curves-canvas",
                    img(chart.clone())
                        .w(px(CURVE_WIDTH as f32))
                        .h(px(CURVE_HEIGHT as f32))
                        .rounded(px(3.)),
                )
                .cursor(CursorStyle::Crosshair)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        this.curves_down(event.position, window, cx);
                        cx.stop_propagation();
                    }),
                ),
            );
        }
        editor = editor
            .child(hint("Click to add a point. Drag to adjust."))
            .child(
                row()
                    .child(
                        div().flex_1().child(
                            self.curves_selected
                                .and_then(|i| points.get(i))
                                .map(|p| {
                                    format!(
                                        "Input {} · Output {}",
                                        p.x.round() as i32,
                                        p.y.round() as i32
                                    )
                                })
                                .unwrap_or_default(),
                        ),
                    )
                    .child(
                        self.command_button(
                            "curves-remove",
                            "Remove point",
                            Command::UpdateAdjustmentCurves {
                                settings: remove_curve_point(&edit.curves, self.curves_selected),
                                preview: edit.preview,
                            },
                            locked
                                || !self
                                    .curves_selected
                                    .is_some_and(|i| i > 0 && i + 1 < points.len()),
                            false,
                            cx,
                        ),
                    ),
            )
            .child(
                row()
                    .child(self.command_button(
                        "curves-reset",
                        "Reset curve",
                        Command::UpdateAdjustmentCurves {
                            settings: reset_curve_channel(&edit.curves),
                            preview: edit.preview,
                        },
                        locked,
                        false,
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(self.command_button(
                        "curves-preview",
                        if edit.preview {
                            "Preview on"
                        } else {
                            "Preview off"
                        },
                        Command::SetAdjustmentPreview {
                            preview: !edit.preview,
                        },
                        false,
                        edit.preview,
                        cx,
                    )),
            )
            .child(
                row()
                    .child(self.command_button(
                        "curves-cancel",
                        "Cancel",
                        Command::CancelAdjustmentEdit,
                        false,
                        false,
                        cx,
                    ))
                    .child(div().flex_1())
                    .child(self.command_button(
                        "curves-apply",
                        "Apply",
                        Command::CommitAdjustmentEdit,
                        locked,
                        false,
                        cx,
                    )),
            );
        editor
    }

    /// Curves canvas press: grab the nearest point or add one, following
    /// CurvesControls' 14-unit hit test, 32-point cap and neighbor clamping.
    pub(super) fn curves_down(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(edit) = self.state.as_ref().and_then(|s| s.adjustment_edit.clone()) else {
            return;
        };
        if edit.kind != AdjustmentKind::Curves {
            return;
        }
        if self
            .state
            .as_ref()
            .and_then(|s| s.layer(&edit.layer_id))
            .is_some_and(|l| l.locked)
        {
            return;
        }
        let Some([left, top, width, height]) =
            self.probes.lock().unwrap().get("curves-canvas").copied()
        else {
            return;
        };
        if width <= 0. || height <= 0. {
            return;
        }
        let x = ((f32::from(position.x) - left) / width * 255.).clamp(0., 255.) as f64;
        let y = (1. - (f32::from(position.y) - top) / height).clamp(0., 1.) as f64 * 255.;
        let channel = edit.curves.channel.index();
        let mut points = edit.curves.channels[channel].clone();
        let nearest = points
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                ((a.x - x).hypot(a.y - y)).total_cmp(&(b.x - x).hypot(b.y - y))
            })
            .filter(|(_, p)| (p.x - x).hypot(p.y - y) < 14.)
            .map(|(i, _)| i);
        let index = if let Some(i) = nearest {
            i
        } else if points.len() < 32
            && x > 1.
            && x < 254.
            && points.iter().all(|p| (p.x - x).abs() > 1.)
        {
            points.push(picsie_core::adjustment::CurvePoint { x, y });
            points.sort_by(|a, b| a.x.total_cmp(&b.x));
            points.iter().position(|p| p.x == x).unwrap_or(0)
        } else {
            return;
        };
        self.commit_active_fields(window, cx);
        self.curves_drag = Some(index);
        self.curves_selected = Some(index);
        // The inserted point set goes out in the same command; the drag keeps
        // working on this local copy so asynchronous published state can never
        // drop a motion that outruns the worker round trip.
        self.curves_working = Some(points);
        self.curves_move_to(index, x, y);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn curves_move(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.curves_drag else { return };
        let Some(edit) = self.state.as_ref().and_then(|s| s.adjustment_edit.clone()) else {
            return;
        };
        if edit.kind != AdjustmentKind::Curves {
            return;
        }
        let Some([left, top, width, height]) =
            self.probes.lock().unwrap().get("curves-canvas").copied()
        else {
            return;
        };
        if width <= 0. || height <= 0. {
            return;
        }
        let x = ((f32::from(position.x) - left) / width * 255.).clamp(0., 255.) as f64;
        let y = (1. - (f32::from(position.y) - top) / height).clamp(0., 1.) as f64 * 255.;
        self.curves_move_to(drag, x, y);
        let _ = (window, cx);
    }

    fn curves_move_to(&mut self, index: usize, x: f64, y: f64) {
        let Some(edit) = self.state.as_ref().and_then(|s| s.adjustment_edit.clone()) else {
            return;
        };
        let channel = edit.curves.channel.index();
        // Prefer the drag-local working copy, which already holds inserted
        // points the published snapshot may not have caught up with yet.
        let mut points = self
            .curves_working
            .clone()
            .unwrap_or_else(|| edit.curves.channels[channel].clone());
        if index >= points.len() {
            return;
        }
        points[index].y = y.clamp(0., 255.);
        if index > 0 && index + 1 < points.len() {
            points[index].x = x.clamp(points[index - 1].x + 1., points[index + 1].x - 1.);
        }
        self.curves_working = Some(points.clone());
        let mut settings = edit.curves;
        settings.channels[channel] = points;
        self.send(Command::UpdateAdjustmentCurves {
            settings,
            preview: edit.preview,
        });
    }

    /// Modeless floating panel for the open edit, following LevelsSheet's
    /// 440pt content and CurvesControls' 260pt graph. The canvas stays
    /// interactive (sampling eyedropper included); Cancel/Apply finish the
    /// single engine transaction like the source OK/Cancel row.
    pub(super) fn adjustment_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.state.as_ref()?;
        let edit = state.adjustment_edit.as_ref()?.clone();
        let (title, content) = match edit.kind {
            AdjustmentKind::Levels => ("Levels", self.levels_editor(state, &edit, cx)),
            AdjustmentKind::Curves => ("Curves", self.curves_editor(state, &edit, cx)),
        };
        let popup = column()
            .w(px(ADJUSTMENT_PANEL_WIDTH))
            .p(px(24.))
            .gap(px(12.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(LINE))
            .bg(rgb(PANEL))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(
                self.probe("adjustment-panel-title", div())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.adjustment_panel_drag =
                                Some((event.position, this.adjustment_position));
                            cx.stop_propagation();
                        }),
                    )
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(content)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        this.send(Command::CancelAdjustmentEdit);
                        cx.stop_propagation();
                    }
                    "enter" if window.focused_input(cx).is_none() => {
                        this.send(Command::CommitAdjustmentEdit);
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }));
        Some(
            self.probe("adjustment-panel", popup)
                .occlude()
                .absolute()
                .left(px(self.adjustment_position[0]))
                .top(px(self.adjustment_position[1]))
                .into_any_element(),
        )
    }

    /// LevelsSheet handle press: snap to the nearest input/output handle.
    pub(super) fn levels_down(
        &mut self,
        position: Point<Pixels>,
        output: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let Some(edit) = self.state.as_ref().and_then(|s| s.adjustment_edit.clone()) else {
            return;
        };
        if edit.kind != AdjustmentKind::Levels {
            return;
        }
        if self
            .state
            .as_ref()
            .and_then(|s| s.layer(&edit.layer_id))
            .is_none_or(|l| l.locked)
        {
            return;
        }
        let probe = if output {
            "levels-output-handles"
        } else {
            "levels-input-handles"
        };
        let Some([left, _, width, _]) = self.probes.lock().unwrap().get(probe).copied() else {
            return;
        };
        if width <= 0. {
            return;
        }
        let x = ((f32::from(position.x) - left) / width * 255.).clamp(0., 255.) as f64;
        let range = edit.levels.ranges[edit.levels.channel.index()];
        let gamma_position = range.black + (range.white - range.black) * 0.5f64.powf(range.gamma);
        let candidates = if output {
            vec![
                (LevelsHandle::OutBlack, range.output_black),
                (LevelsHandle::OutWhite, range.output_white),
            ]
        } else {
            vec![
                (LevelsHandle::Black, range.black),
                (LevelsHandle::Gamma, gamma_position),
                (LevelsHandle::White, range.white),
            ]
        };
        let Some((handle, _)) = candidates
            .into_iter()
            .min_by(|(_, a), (_, b)| (a - x).abs().total_cmp(&(b - x).abs()))
            .filter(|(_, v)| (v - x).abs() < 14.)
        else {
            return;
        };
        self.commit_active_fields(window, cx);
        self.levels_drag = Some(handle);
        self.levels_apply_handle(handle, x);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn levels_move(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.levels_drag else {
            return;
        };
        let probe = match handle {
            LevelsHandle::OutBlack | LevelsHandle::OutWhite => "levels-output-handles",
            _ => "levels-input-handles",
        };
        let Some([left, _, width, _]) = self.probes.lock().unwrap().get(probe).copied() else {
            return;
        };
        if width <= 0. {
            return;
        }
        let x = ((f32::from(position.x) - left) / width * 255.).clamp(0., 255.) as f64;
        self.levels_apply_handle(handle, x);
        let _ = (window, cx);
    }

    /// LevelsSheet handle drag math: black/white clamp against each other and
    /// round; gamma inverts the 50%-position logarithm.
    fn levels_apply_handle(&mut self, handle: LevelsHandle, x: f64) {
        let Some(edit) = self.state.as_ref().and_then(|s| s.adjustment_edit.clone()) else {
            return;
        };
        let mut settings = edit.levels;
        let range = &mut settings.ranges[settings.channel.index()];
        match handle {
            LevelsHandle::Black => range.black = (range.white - 1.).min(x.round()).max(0.),
            LevelsHandle::White => range.white = (range.black + 1.).max(x.round()).min(255.),
            LevelsHandle::Gamma => {
                let fraction =
                    ((x - range.black) / (range.white - range.black)).clamp(0.001, 0.999);
                range.gamma = (fraction.ln() / 0.5f64.ln()).clamp(0.1, 9.99);
            }
            LevelsHandle::OutBlack => range.output_black = x.round().clamp(0., 255.),
            LevelsHandle::OutWhite => range.output_white = x.round().clamp(0., 255.),
        }
        self.send(Command::UpdateAdjustmentLevels {
            settings,
            preview: edit.preview,
        });
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum LevelsHandle {
    Black,
    Gamma,
    White,
    OutBlack,
    OutWhite,
}

fn remove_curve_point(settings: &CurvesSettings, selected: Option<usize>) -> CurvesSettings {
    let mut settings = settings.clone();
    let channel = settings.channel.index();
    if let Some(i) = selected
        && i > 0
        && i + 1 < settings.channels[channel].len()
    {
        settings.channels[channel].remove(i);
    }
    settings
}

fn reset_curve_channel(settings: &CurvesSettings) -> CurvesSettings {
    let mut settings = settings.clone();
    let channel = settings.channel.index();
    settings.channels[channel] = vec![
        picsie_core::adjustment::CurvePoint { x: 0., y: 0. },
        picsie_core::adjustment::CurvePoint { x: 255., y: 255. },
    ];
    settings
}
