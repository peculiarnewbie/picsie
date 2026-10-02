//! Non-destructive Levels and Curves adjustment layers.
//!
//! Translated from the pinned Compositor (609dbeae, MIT © 2026 Wonder Assembly LLC):
//! `Compositor/Document/LayerAdjustment.swift` (`AdjustmentKind`, `LayerAdjustment`,
//! `addAdjustment`/`updateAdjustment`), `Compositor/Document/Levels.swift`
//! (`LevelsChannel`, `LevelRange`, `LevelsSettings`, `LevelsHistogramDisplay`,
//! `LevelsFilter` tables and histogram), `Compositor/Document/LevelsAutomatic.swift`
//! (`LevelsSample`, `LevelsAuto`, sampling calibration), `Compositor/Document/Curves.swift`
//! (`CurvePoint`, `CurvesSettings` Hermite interpolation and table application),
//! `Compositor/Rendering/LevelsPixels.c` (`levels_apply`, `levels_histogram`) and
//! `Compositor/Rendering/AdjustmentSurface.swift` (adjustments render from the pixels
//! beneath them).
//!
//! Adaptations: only the Levels and Curves kinds are ported; every other upstream
//! adjustment kind is rejected on import and documented as a gap. The pixel
//! kernels (`levels_apply`, `levels_histogram`, `layer_unpremultiply_opaque`,
//! `layer_restore_alpha`) are exact integer ports operating on premultiplied
//! RGBA, so soft-alpha rounding matches the C sources step for step.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Upstream `AdjustmentKind` raw values, scoped to this port's two supported kinds.
/// Any other `kind` string fails to decode, so foreign adjustments are rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AdjustmentKind {
    #[serde(rename = "Levels")]
    Levels,
    #[serde(rename = "Curves")]
    Curves,
}

impl AdjustmentKind {
    pub fn display(self) -> &'static str {
        match self {
            Self::Levels => "Levels",
            Self::Curves => "Curves",
        }
    }
}

/// Upstream `LevelsChannel` raw values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum LevelsChannel {
    #[serde(rename = "RGB")]
    #[default]
    Rgb,
    #[serde(rename = "Red")]
    Red,
    #[serde(rename = "Green")]
    Green,
    #[serde(rename = "Blue")]
    Blue,
}

impl LevelsChannel {
    pub const ALL: [Self; 4] = [Self::Rgb, Self::Red, Self::Green, Self::Blue];
    pub fn index(self) -> usize {
        match self {
            Self::Rgb => 0,
            Self::Red => 1,
            Self::Green => 2,
            Self::Blue => 3,
        }
    }
}

/// Upstream `LevelRange`: input black/white points, gamma and output endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LevelRange {
    pub black: f64,
    pub gamma: f64,
    pub white: f64,
    pub output_black: f64,
    pub output_white: f64,
}

impl Default for LevelRange {
    fn default() -> Self {
        Self {
            black: 0.,
            gamma: 1.,
            white: 255.,
            output_black: 0.,
            output_white: 255.,
        }
    }
}

fn clamp(value: f64, range: std::ops::RangeInclusive<f64>, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(*range.start(), *range.end())
    } else {
        fallback
    }
}

impl LevelRange {
    /// Upstream `LevelRange.normalized`.
    pub fn normalized(self) -> Self {
        let mut result = self;
        result.black = clamp(self.black, 0.0..=254.0, 0.);
        result.white = clamp(self.white, (result.black + 1.)..=255.0, 255.);
        result.gamma = clamp(self.gamma, 0.1..=9.99, 1.);
        result.output_black = clamp(self.output_black, 0.0..=255.0, 0.);
        result.output_white = clamp(self.output_white, 0.0..=255.0, 255.);
        result
    }
    /// Upstream `LevelRange.apply`: map a 0–1 input through this range.
    pub fn apply(self, value: f64) -> f64 {
        let s = self.normalized();
        let input = ((value * 255. - s.black) / (s.white - s.black)).clamp(0., 1.);
        (s.output_black + input.powf(1. / s.gamma) * (s.output_white - s.output_black)) / 255.
    }
}

/// Upstream `LevelsSettings`: one range per channel slot plus the edited channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct LevelsSettings {
    pub channel: LevelsChannel,
    pub ranges: [LevelRange; 4],
}

impl Default for LevelsSettings {
    fn default() -> Self {
        Self {
            channel: LevelsChannel::Rgb,
            ranges: [
                LevelRange::default(),
                LevelRange::default(),
                LevelRange::default(),
                LevelRange::default(),
            ],
        }
    }
}

impl LevelsSettings {
    pub fn is_identity(&self) -> bool {
        self.ranges
            .iter()
            .all(|r| r.normalized() == LevelRange::default())
    }
    /// Upstream order: per-channel range first, then the composite RGB range.
    pub fn apply(&self, value: f64, channel: LevelsChannel) -> f64 {
        self.ranges[0].apply(self.ranges[channel.index()].apply(value))
    }
    /// Per-channel 256-entry lookup tables in R, G, B order, 0–1 entries.
    pub fn tables(&self) -> [f32; 768] {
        let mut tables = [0f32; 768];
        for (slot, channel) in [
            LevelsChannel::Red,
            LevelsChannel::Green,
            LevelsChannel::Blue,
        ]
        .into_iter()
        .enumerate()
        {
            for i in 0..256 {
                tables[slot * 256 + i] = self.apply(i as f64 / 255., channel) as f32;
            }
        }
        tables
    }
}

/// Upstream `LevelsSample` eyedropper modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum LevelsSample {
    Black,
    Gray,
    White,
}

/// Upstream `LevelsAuto` automatic modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum LevelsAuto {
    Contrast,
    Color,
    Neutral,
}

impl LevelsAuto {
    /// Upstream `LevelsAuto.settings(histogram:)`. `histogram` is the four
    /// 256-bin alpha-weighted distributions from [`histogram_rgba`].
    pub fn settings(self, histogram: &[[f64; 256]; 4]) -> LevelsSettings {
        fn endpoints(bins: &[f64; 256]) -> Option<(f64, f64)> {
            let total: f64 = bins.iter().sum();
            if total <= 0. {
                return None;
            }
            let mut sum = 0.;
            let mut low = 0;
            for (i, bin) in bins.iter().enumerate() {
                sum += bin;
                if sum > total * 0.001 {
                    low = i;
                    break;
                }
            }
            sum = 0.;
            let mut high = 255;
            for (i, bin) in bins.iter().enumerate().rev() {
                sum += bin;
                if sum > total * 0.001 {
                    high = i;
                    break;
                }
            }
            if low < high {
                Some((low as f64, high as f64))
            } else {
                None
            }
        }
        let mut result = LevelsSettings::default();
        if self == Self::Contrast {
            let limits: Vec<_> = histogram[1..]
                .iter()
                .filter_map(|bins| endpoints(bins))
                .collect();
            let low = limits.iter().map(|v| v.0).min_by(f64::total_cmp);
            let high = limits.iter().map(|v| v.1).max_by(f64::total_cmp);
            if let (Some(low), Some(high)) = (low, high)
                && low < high
            {
                result.ranges[0] = LevelRange {
                    black: low,
                    white: high,
                    ..LevelRange::default()
                };
            }
        } else {
            for c in 1..4 {
                let Some((low, high)) = endpoints(&histogram[c]) else {
                    continue;
                };
                let mut range = LevelRange {
                    black: low,
                    white: high,
                    ..LevelRange::default()
                };
                if self == Self::Neutral {
                    let total: f64 = histogram[c].iter().sum();
                    let mean: f64 = histogram[c]
                        .iter()
                        .enumerate()
                        .map(|(i, bin)| range.apply(i as f64 / 255.) * bin)
                        .sum::<f64>()
                        / total;
                    if mean > 0. && mean < 1. {
                        range.gamma = (mean.ln() / 0.5f64.ln()).clamp(0.1, 9.99);
                    }
                }
                result.ranges[c] = range;
            }
        }
        result
    }
}

impl LevelsSettings {
    /// Upstream `LevelsSettings.sampling`: calibrate all three channels from one
    /// unpremultiplied original RGB sample so it maps to black, white or mid gray.
    pub fn sampling(&self, rgb: [f64; 3], mode: LevelsSample) -> Self {
        let mut result = self.clone();
        result.ranges[0] = LevelRange::default();
        for c in 1..4 {
            let mut range = result.ranges[c];
            let v = rgb[c - 1] * 255.;
            match mode {
                LevelsSample::Black => range.black = v.clamp(0., range.white - 1.),
                LevelsSample::White => range.white = v.clamp(range.black + 1., 255.),
                LevelsSample::Gray => {
                    let fraction = (v - range.black) / (range.white - range.black);
                    if fraction > 0. && fraction < 1. {
                        range.gamma = fraction.ln() / 0.5f64.ln();
                    }
                }
            }
            range.output_black = 0.;
            range.output_white = 255.;
            result.ranges[c] = range.normalized();
        }
        result
    }
}

/// Upstream `CurvePoint`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CurvePoint {
    pub x: f64,
    pub y: f64,
}

/// Upstream `CurvesSettings`: four channels of 2–32 strictly increasing points
/// pinned at x = 0 and x = 255.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct CurvesSettings {
    pub channel: LevelsChannel,
    pub channels: [Vec<CurvePoint>; 4],
}

impl Default for CurvesSettings {
    fn default() -> Self {
        let diagonal = vec![CurvePoint { x: 0., y: 0. }, CurvePoint { x: 255., y: 255. }];
        Self {
            channel: LevelsChannel::Rgb,
            channels: [
                diagonal.clone(),
                diagonal.clone(),
                diagonal.clone(),
                diagonal,
            ],
        }
    }
}

impl CurvesSettings {
    pub fn is_valid(&self) -> bool {
        self.channels.len() == 4
            && self.channels.iter().all(|points| {
                (2..=32).contains(&points.len())
                    && points.first().is_some_and(|p| p.x == 0.)
                    && points.last().is_some_and(|p| p.x == 255.)
                    && points.iter().all(|p| {
                        p.x.is_finite()
                            && p.y.is_finite()
                            && (0.0..=255.).contains(&p.x)
                            && (0.0..=255.).contains(&p.y)
                    })
                    && points
                        .iter()
                        .zip(points.iter().skip(1))
                        .all(|(a, b)| a.x < b.x)
            })
    }
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
    /// Upstream shape-preserving cubic Hermite interpolation (no overshoot).
    pub fn value(&self, x: f64, channel: usize) -> f64 {
        let p = &self.channels[channel];
        if p.len() < 2 {
            return x.clamp(0., 255.);
        }
        let i = p
            .iter()
            .rposition(|point| point.x <= x)
            .unwrap_or(0)
            .min(p.len() - 2);
        let d: Vec<f64> = p
            .iter()
            .zip(p.iter().skip(1))
            .map(|(a, b)| (b.y - a.y) / (b.x - a.x))
            .collect();
        let slope = |j: usize| {
            if j == 0 {
                d[0]
            } else if j == p.len() - 1 {
                *d.last().unwrap()
            } else if d[j - 1] * d[j] <= 0. {
                0.
            } else {
                2. / (1. / d[j - 1] + 1. / d[j])
            }
        };
        let h = p[i + 1].x - p[i].x;
        let t = ((x - p[i].x) / h).clamp(0., 1.);
        let y = (2. * t * t * t - 3. * t * t + 1.) * p[i].y
            + (t * t * t - 2. * t * t + t) * h * slope(i)
            + (-2. * t * t * t + 3. * t * t) * p[i + 1].y
            + (t * t * t - t * t) * h * slope(i + 1);
        y.clamp(0., 255.)
    }
    /// Upstream table order: each channel's curve first, then the RGB curve.
    pub fn tables(&self) -> [f32; 768] {
        let mut tables = [0f32; 768];
        for channel in 1..4 {
            for i in 0..256 {
                tables[(channel - 1) * 256 + i] =
                    (self.value(self.value(i as f64, channel), 0) / 255.) as f32;
            }
        }
        tables
    }
}

/// Upstream `LayerAdjustment`, scoped to Levels and Curves. The legacy scalar
/// fields are always serialized so Compositor's synthesized decoder (which
/// requires present keys) keeps reading our packages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct LayerAdjustment {
    pub kind: AdjustmentKind,
    #[serde(default)]
    pub hue: f64,
    #[serde(default)]
    pub saturation: f64,
    #[serde(default)]
    pub lightness: f64,
    #[serde(default)]
    pub colorize: bool,
    #[serde(default)]
    pub levels: LevelsSettings,
    #[serde(default)]
    pub curves: CurvesSettings,
}

impl LayerAdjustment {
    pub fn new(kind: AdjustmentKind) -> Self {
        Self {
            kind,
            hue: 0.,
            saturation: 0.,
            lightness: 0.,
            colorize: false,
            levels: LevelsSettings::default(),
            curves: CurvesSettings::default(),
        }
    }
    /// Upstream `LayerAdjustment.isValid`, scoped to the ported kinds.
    pub fn is_valid(&self) -> bool {
        self.hue.is_finite()
            && self.saturation.is_finite()
            && self.lightness.is_finite()
            && self.levels.ranges.len() == 4
            && self.levels.ranges.iter().all(|r| *r == r.normalized())
            && self.curves.is_valid()
    }
    pub fn is_identity(&self) -> bool {
        match self.kind {
            AdjustmentKind::Levels => self.levels.is_identity(),
            AdjustmentKind::Curves => self.curves.is_identity(),
        }
    }
    pub fn tables(&self) -> [f32; 768] {
        match self.kind {
            AdjustmentKind::Levels => self.levels.tables(),
            AdjustmentKind::Curves => self.curves.tables(),
        }
    }
}

/// Exact port of `levels_apply` (`LevelsPixels.c`): remap each channel through
/// 256-entry 0–1 tables with linear interpolation on premultiplied RGBA.
/// Channels are divided by alpha before the lookup and multiplied back after,
/// so alpha is preserved exactly and soft edges never pass through the
/// conversion twice. Transparent pixels are skipped.
pub fn apply_tables_premul(pixels: &mut [u8], tables: &[f32; 768]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as f32;
        if alpha == 0. {
            continue;
        }
        for c in 0..3 {
            let x = (pixel[c] as f32 * 255. / alpha).min(255.);
            let lo = x as usize;
            let hi = (lo + 1).min(255);
            let table = &tables[c * 256..(c + 1) * 256];
            let result = table[lo] + (table[hi] - table[lo]) * (x - lo as f32);
            pixel[c] = (result * alpha).round().min(alpha).max(0.) as u8;
        }
    }
}

/// Exact port of `levels_histogram` (`LevelsPixels.c`): per-channel 256-bin
/// counts over unpremultiplied values, weighted by alpha (and by `coverage`
/// when given). Bin 0 is the mean of the three channel histograms, exactly
/// like upstream's RGB row.
pub fn histogram_premul(pixels: &[u8], coverage: Option<&[u8]>) -> [[f64; 256]; 4] {
    let mut bins = [[0f64; 256]; 4];
    let count = pixels.len() / 4;
    for i in 0..count {
        let pixel = &pixels[i * 4..i * 4 + 4];
        if pixel[3] == 0 {
            continue;
        }
        let weight = pixel[3] as f64 / 255. * coverage.map_or(1., |c| c[i] as f64 / 255.);
        for c in 0..3 {
            let value = ((pixel[c] as f64 * 255. / pixel[3] as f64).round().min(255.)) as usize;
            bins[c + 1][value] += weight;
            bins[0][value] += weight / 3.;
        }
    }
    bins
}

/// Exact port of `layer_unpremultiply_opaque` (`BrushPixels.c`): divide color
/// by alpha with half-up rounding so blending works on opaque colors.
pub fn unpremultiply_opaque(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        for c in 0..3 {
            let v = if alpha == 0 {
                0
            } else {
                (pixel[c] as u32 * 255 + alpha / 2) / alpha
            };
            pixel[c] = v.min(255) as u8;
        }
        pixel[3] = 255;
    }
}

/// Exact port of `layer_restore_alpha` (`BrushPixels.c`): fold the saved
/// coverage back over blended opaque colors.
pub fn restore_alpha(pixels: &mut [u8], alpha: &[u8]) {
    for (pixel, &a) in pixels.chunks_exact_mut(4).zip(alpha.iter()) {
        for c in 0..3 {
            pixel[c] = ((pixel[c] as u32 * a as u32 + 127) / 255) as u8;
        }
        pixel[3] = a;
    }
}

/// Straight 0–1 RGB of a premultiplied pixel, for eyedropper calibration.
pub fn straight_rgb(pixel: &[u8]) -> Option<[f64; 3]> {
    let alpha = pixel[3] as f64;
    if alpha <= 0. {
        return None;
    }
    let mut rgb = [0.; 3];
    for c in 0..3 {
        rgb[c] = (pixel[c] as f64 * 255. / alpha).min(255.) / 255.;
    }
    Some(rgb)
}

/// Upstream `LevelsHistogramDisplay.scale`: display-only vertical scaling that
/// keeps linear bin ratios but caps isolated spikes at four times the 95th
/// percentile interior height.
pub fn histogram_display_scale(bins: &[f64; 256]) -> f64 {
    let peak = bins
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.)
        .fold(0f64, f64::max);
    if peak <= 0. {
        return 0.;
    }
    let mut interior: Vec<f64> = bins[1..255]
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.)
        .collect();
    if interior.is_empty() {
        return peak;
    }
    interior.sort_by(f64::total_cmp);
    let typical = interior[(interior.len() - 1) as usize * 95 / 100];
    peak.min(typical * 4.)
}
