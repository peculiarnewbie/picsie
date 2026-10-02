//! Levels and Curves adjustment layers.
//!
//! Translated from the pinned Compositor (609dbeae, MIT © 2026 Wonder Assembly LLC):
//! `CompositorTests/LevelsTests.swift` (clipping, gamma, output inversion, alpha,
//! channel order, histogram weighting, display scaling, auto and sampling),
//! `CompositorTests/AdjustmentLayerTests.swift` (global/clipped rendering, opacity and
//! mask behavior, blend and soft-mask coverage, persistence, duplication, undo) and
//! `CompositorTests/ImageAdjustmentTests.swift` (settings serialization shape).
//! The pixel kernels are exact integer ports of `LevelsPixels.c`/`BrushPixels.c` over
//! premultiplied RGBA, so translated fixtures keep their upstream byte expectations.
use picsie_core::{
    adjustment::{
        AdjustmentKind, CurvePoint, CurvesSettings, LayerAdjustment, LevelRange, LevelsAuto,
        LevelsChannel, LevelsSample, LevelsSettings, apply_tables_premul, histogram_display_scale,
        histogram_premul,
    },
    editor::Editor,
    model::{Content, Document, Layer},
    render::{self, Renderer},
};

fn paint_layer(name: &str, width: u32, height: u32, pixels: &[u8]) -> Layer {
    let image = render::rgba_image(width, height, pixels).unwrap();
    Layer::new(name, width, height, render::native_content(image))
}

fn rendered(doc: &Document) -> Vec<u8> {
    render::rgba_pixels(&Renderer::default().render(doc).unwrap().image_snapshot()).unwrap()
}

/// Upstream `LevelsTests` ramp, premultiplied: opaque gray steps plus the
/// `[64,32,0,128]` translucent probe and one clear pixel.
fn ramp() -> Vec<u8> {
    vec![
        0, 0, 0, 255, //
        64, 64, 64, 255, //
        128, 128, 128, 255, //
        255, 255, 255, 255, //
        64, 32, 0, 128, //
        0, 0, 0, 0,
    ]
}

fn apply_levels(pixels: &[u8], settings: &LevelsSettings) -> Vec<u8> {
    let mut out = pixels.to_vec();
    apply_tables_premul(&mut out, &settings.tables());
    out
}

// LevelsTests.inputClippingGammaOutputInversionAndAlpha, exact upstream bytes.
#[test]
fn input_clipping_gamma_output_inversion_and_alpha() {
    let source = ramp();
    let mut settings = LevelsSettings::default();
    settings.ranges[0] = LevelRange {
        black: 64.,
        white: 128.,
        ..LevelRange::default()
    };
    let clipped = apply_levels(&source, &settings);
    assert_eq!(
        &clipped[0..12],
        &[0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255]
    );
    let mut gamma = LevelsSettings::default();
    gamma.ranges[0] = LevelRange {
        gamma: 2.,
        ..LevelRange::default()
    };
    let brightened = apply_levels(&source, &gamma);
    assert!((brightened[4] as i32 - 128).abs() <= 1);
    assert!((brightened[8] as i32 - 181).abs() <= 1);
    assert_eq!(brightened[4 * 4 + 3], 128);
    assert_eq!(brightened[5 * 4 + 3], 0);
    assert!(brightened[16] <= 128 && brightened[17] <= 128);
    let mut inverted = LevelsSettings::default();
    inverted.ranges[0] = LevelRange {
        output_black: 255.,
        output_white: 0.,
        ..LevelRange::default()
    };
    let result = apply_levels(&source, &inverted);
    assert_eq!(result[0], 255);
    assert_eq!(result[12], 0);
    assert_eq!(&result[16..20], &[64, 96, 128, 128]);
}

// LevelsTests.channelsCoexistAndUseDocumentedOrder.
#[test]
fn channels_coexist_and_use_documented_order() {
    let mut settings = LevelsSettings::default();
    settings.channel = LevelsChannel::Red;
    settings.ranges[1] = LevelRange {
        gamma: 2.,
        ..LevelRange::default()
    };
    settings.channel = LevelsChannel::Rgb;
    settings.ranges[0] = LevelRange {
        black: 40.,
        white: 210.,
        ..LevelRange::default()
    };
    let result = apply_levels(&[64, 64, 64, 255], &settings);
    let expected = LevelRange {
        black: 40.,
        white: 210.,
        ..LevelRange::default()
    }
    .apply(
        LevelRange {
            gamma: 2.,
            ..LevelRange::default()
        }
        .apply(64. / 255.),
    );
    assert!((result[0] as f64 - expected * 255.).abs() <= 1.);
    assert_eq!(result[1], result[2]);
    assert!(result[0] > result[1]);
    let invalid = LevelRange {
        black: 300.,
        gamma: f64::NAN,
        white: -1.,
        output_black: -100.,
        output_white: 400.,
    }
    .normalized();
    assert!(invalid.black < invalid.white && invalid.gamma == 1.);
    assert_eq!((invalid.output_black, invalid.output_white), (0., 255.));
}

// LevelsTests.histogramExcludesTransparencyAndWeightsSelection.
#[test]
fn histogram_excludes_transparency_and_weights_selection() {
    let pixels = [255, 0, 0, 255, 0, 128, 0, 128, 0, 0, 0, 0];
    let bins = histogram_premul(&pixels, None);
    assert_eq!(bins[1][255], 1.);
    assert!((bins[2][255] - 128. / 255.).abs() < 1e-9);
    let total: f64 = bins[0].iter().sum();
    assert!((total - (1. + 128. / 255.)).abs() < 1e-9);
    let selected = histogram_premul(&pixels, Some(&[255, 0, 0]));
    assert_eq!(selected[1][255], 1.);
    assert_eq!(selected[2][255], 0.);
    let empty = histogram_premul(&pixels, Some(&[0, 0, 0]));
    assert!(empty.iter().flatten().all(|v| *v == 0.));
}

// LevelsTests.histogramDisplayKeepsDistributionVisibleBesideClippingSpikes.
#[test]
fn histogram_display_keeps_distribution_visible_beside_clipping_spikes() {
    let mut bins = [100f64; 256];
    bins[255] = 100_000.;
    assert_eq!(histogram_display_scale(&bins), 400.);
    bins[0] = 200_000.;
    assert_eq!(histogram_display_scale(&bins), 400.);
    bins[128] = 500_000.;
    assert_eq!(histogram_display_scale(&bins), 400.);
    assert_eq!(bins[128], 500_000.);
    assert_eq!(histogram_display_scale(&[100.; 256]), 100.);
    assert_eq!(histogram_display_scale(&[0.; 256]), 0.);
    let mut sparse = [0f64; 256];
    sparse[255] = 50.;
    assert_eq!(histogram_display_scale(&sparse), 50.);
    sparse[0] = 100.;
    assert_eq!(histogram_display_scale(&sparse), 100.);
    sparse[128] = 200.;
    assert_eq!(histogram_display_scale(&sparse), 200.);
}

// LevelsTests.autoAlgorithmsAndEyedropperCalibration.
#[test]
fn auto_algorithms_and_eyedropper_calibration() {
    let mut bins = [[0f64; 256]; 4];
    for c in 1..4 {
        bins[c][20 * c] = 100.;
        bins[c][200 + c * 10] = 100.;
    }
    let linked = LevelsAuto::Contrast.settings(&bins);
    assert_eq!(
        (linked.ranges[0].black, linked.ranges[0].white),
        (20., 230.)
    );
    let color = LevelsAuto::Color.settings(&bins);
    assert_eq!((color.ranges[1].black, color.ranges[3].black), (20., 60.));
    assert_eq!(color.ranges[0], LevelRange::default());
    assert_eq!(
        LevelsAuto::Neutral.settings(&bins).ranges[1].gamma,
        1.,
        "symmetric fixture needs no midtone bend"
    );
    let empty = [[0f64; 256]; 4];
    for mode in [LevelsAuto::Contrast, LevelsAuto::Color, LevelsAuto::Neutral] {
        assert!(mode.settings(&empty).is_identity());
    }
    let rgb = [0.25, 0.4, 0.6];
    for mode in [LevelsSample::Black, LevelsSample::Gray, LevelsSample::White] {
        let settings = LevelsSettings::default().sampling(rgb, mode);
        let target = match mode {
            LevelsSample::Black => 0.,
            LevelsSample::White => 1.,
            LevelsSample::Gray => 0.5,
        };
        for (i, channel) in [
            LevelsChannel::Red,
            LevelsChannel::Green,
            LevelsChannel::Blue,
        ]
        .into_iter()
        .enumerate()
        {
            assert!((settings.apply(rgb[i], channel) - target).abs() < 1e-4);
        }
    }
}

// AdjustmentLayerTests.curvesIdentityAndImageCommandPreserveAlpha, adapted to the
// non-destructive layer: the default curve is the identity map, and mapping
// every tone to white keeps each pixel's alpha bit-for-bit.
#[test]
fn curves_identity_maps_every_input_to_itself() {
    let settings = CurvesSettings::default();
    assert!(settings.is_valid() && settings.is_identity());
    for i in 0..256 {
        for channel in 1..4 {
            assert!((settings.value(i as f64, channel) - i as f64).abs() < 1e-9);
        }
        let table = settings.tables();
        assert!((table[i] - i as f32 / 255.).abs() < 1e-6);
        assert!((table[256 + i] - i as f32 / 255.).abs() < 1e-6);
    }
    let mut white = CurvesSettings::default();
    white.channels[0] = vec![
        CurvePoint { x: 0., y: 255. },
        CurvePoint { x: 255., y: 255. },
    ];
    let mut pixels = vec![
        102, 178, 26, 255, //
        51, 89, 13, 128, //
        3, 6, 1, 32, //
        0, 0, 0, 0,
    ];
    apply_tables_premul(&mut pixels, &white.tables());
    assert_eq!(
        pixels,
        vec![
            255, 255, 255, 255, //
            128, 128, 128, 128, //
            32, 32, 32, 32, //
            0, 0, 0, 0,
        ]
    );
}

// Local: curve validation rejects every malformed shape upstream rejects.
#[test]
fn curves_reject_malformed_point_lists() {
    let mut bad = CurvesSettings::default();
    bad.channels[2] = vec![CurvePoint { x: 0., y: 0. }];
    assert!(!bad.is_valid());
    bad.channels[2] = vec![
        CurvePoint { x: 0., y: 0. },
        CurvePoint { x: 128., y: 200. },
        CurvePoint { x: 128., y: 100. },
        CurvePoint { x: 255., y: 255. },
    ];
    assert!(!bad.is_valid(), "x must strictly increase");
    bad.channels[2] = vec![
        CurvePoint { x: 10., y: 0. },
        CurvePoint { x: 255., y: 255. },
    ];
    assert!(!bad.is_valid(), "first point must pin x = 0");
    let mut settings = LayerAdjustment::new(AdjustmentKind::Curves);
    settings.curves = bad;
    assert!(!settings.is_valid());
    let mut nan = LevelRange::default();
    nan.gamma = f64::INFINITY;
    let mut levels = LayerAdjustment::new(AdjustmentKind::Levels);
    levels.levels.ranges[0] = nan;
    assert!(!levels.is_valid(), "unnormalized ranges are invalid");
}

// AdjustmentLayerTests.globalAdjustmentAffectsBelowButNotAboveAndRemainsLive,
// scoped to Levels.
#[test]
fn global_levels_affect_below_but_not_above_and_remain_live() {
    let mut doc = Document::new("Adjustment", 2, 2).unwrap();
    doc.layers
        .push(paint_layer("White", 2, 2, &[255, 255, 255, 255].repeat(4)));
    let mut adjustment = Layer::new("Levels", 2, 2, Content::Paint);
    let mut value = LayerAdjustment::new(AdjustmentKind::Levels);
    value.levels.ranges[0].output_white = 0.;
    adjustment.adjustment = Some(value);
    doc.layers.push(adjustment);
    assert_eq!(rendered(&doc), vec![0, 0, 0, 255].repeat(4));
    // Upstream's cover is opaque only on its first pixel; the rest stays clear.
    doc.layers.push(paint_layer(
        "Red",
        2,
        2,
        &[
            255, 0, 0, 255, //
            255, 0, 0, 0, //
            255, 0, 0, 0, //
            255, 0, 0, 0,
        ],
    ));
    assert_eq!(&rendered(&doc)[0..4], &[255, 0, 0, 255]);
    // The layer beneath stays live: recoloring it flows through the adjustment.
    doc.layers[0] = paint_layer("Blue", 2, 2, &[0, 0, 255, 255].repeat(4));
    assert_eq!(&rendered(&doc)[4..8], &[0, 0, 0, 255]);
    doc.layers[1].visible = false;
    assert_eq!(&rendered(&doc)[4..8], &[0, 0, 255, 255]);
}

// AdjustmentLayerTests.clippedCurveChangesOnlyItsBaseAndCopyMergedMatchesExport,
// scoped to Curves. The opaque blue backdrop means every output pixel is opaque;
// the half-alpha green probe must composite as half magenta over blue.
#[test]
fn clipped_curves_change_only_their_base() {
    let mut doc = Document::new("Adjustment", 2, 2).unwrap();
    doc.layers
        .push(paint_layer("Blue", 2, 2, &[0, 0, 255, 255].repeat(4)));
    let green = paint_layer(
        "Green",
        2,
        2,
        &[
            0, 255, 0, 255, //
            0, 255, 0, 0, //
            0, 255, 0, 128, //
            0, 255, 0, 0,
        ],
    );
    let base = green.id.clone();
    doc.layers.push(green);
    let mut adjustment = Layer::new("Curves", 2, 2, Content::Paint);
    let mut value = LayerAdjustment::new(AdjustmentKind::Curves);
    value.curves.channels[0] = vec![CurvePoint { x: 0., y: 255. }, CurvePoint { x: 255., y: 0. }];
    adjustment.adjustment = Some(value);
    adjustment.mask_source_id = Some(base.clone());
    doc.layers.push(adjustment);
    doc.validate().unwrap();
    let result = rendered(&doc);
    assert_eq!(&result[0..4], &[255, 0, 255, 255]);
    assert_eq!(&result[4..8], &[0, 0, 255, 255]);
    // Half green inverted inside its stack is half magenta over blue.
    for (i, expected) in [128u8, 0, 255, 255].into_iter().enumerate() {
        assert!(
            (result[8 + i] as i32 - expected as i32).abs() <= 2,
            "pixel2: {:?}",
            &result[8..12]
        );
    }
    assert_eq!(&result[12..16], &[0, 0, 255, 255]);
    assert_eq!(
        (result[3], result[7], result[11], result[15]),
        (255, 255, 255, 255)
    );
    // Released from its base, the same curve inverts the whole composite below.
    doc.layers[2].mask_source_id = None;
    assert_eq!(&rendered(&doc)[4..8], &[255, 255, 0, 255]);
}

// Local transparent-background companion: soft alpha must survive a global
// adjustment bit-for-bit instead of compositing over itself.
#[test]
fn global_curves_preserve_soft_alpha_on_transparency() {
    let mut doc = Document::new("Adjustment", 2, 2).unwrap();
    doc.layers.push(paint_layer(
        "Green",
        2,
        2,
        &[
            0, 255, 0, 255, //
            0, 255, 0, 128, //
            0, 255, 0, 32, //
            0, 255, 0, 0,
        ],
    ));
    let mut adjustment = Layer::new("Curves", 2, 2, Content::Paint);
    let mut value = LayerAdjustment::new(AdjustmentKind::Curves);
    value.curves.channels[0] = vec![
        CurvePoint { x: 0., y: 255. },
        CurvePoint { x: 255., y: 255. },
    ];
    adjustment.adjustment = Some(value);
    doc.layers.push(adjustment);
    assert_eq!(
        rendered(&doc),
        vec![
            255, 255, 255, 255, //
            255, 255, 255, 128, //
            255, 255, 255, 32, //
            0, 0, 0, 0,
        ]
    );
}

// AdjustmentLayerTests.adjustmentBlendAndSoftMaskPreserveCoverage, scoped to
// Curves: a non-normal blend recolors opaque values first, then the original
// alpha comes back untouched.
#[test]
fn non_normal_adjustment_blend_restores_alpha() {
    let mut doc = Document::new("Adjustment", 2, 1).unwrap();
    doc.layers
        .push(paint_layer("Red", 2, 1, &[255, 0, 0, 255, 255, 0, 0, 128]));
    let mut adjustment = Layer::new("Curves", 2, 1, Content::Paint);
    let mut value = LayerAdjustment::new(AdjustmentKind::Curves);
    value.curves.channels[0] = vec![CurvePoint { x: 0., y: 255. }, CurvePoint { x: 255., y: 0. }];
    adjustment.adjustment = Some(value);
    adjustment.blend = picsie_core::model::Blend::Multiply;
    doc.layers.push(adjustment);
    // Multiply red by inverted red (cyan) is black; alpha stays 255 and 128.
    assert_eq!(rendered(&doc), vec![0, 0, 0, 255, 0, 0, 0, 128]);
}

// AdjustmentLayerTests.hueOpacityAndMaskPreserveOriginalPixels, scoped to Levels:
// opacity and masks gate the effect while the source layer is never rewritten.
#[test]
fn opacity_and_mask_gate_levels_without_touching_source_pixels() {
    let mut doc = Document::new("Adjustment", 2, 2).unwrap();
    doc.layers
        .push(paint_layer("Red", 2, 2, &[255, 0, 0, 255].repeat(4)));
    let before = doc.layers[0].content.clone();
    let mut adjustment = Layer::new("Levels", 2, 2, Content::Paint);
    let mut value = LayerAdjustment::new(AdjustmentKind::Levels);
    value.levels.ranges[0].output_white = 0.;
    adjustment.adjustment = Some(value);
    doc.layers.push(adjustment);
    assert_eq!(rendered(&doc), vec![0, 0, 0, 255].repeat(4));
    doc.layers[1].opacity = 0.;
    assert_eq!(&rendered(&doc)[0..4], &[255, 0, 0, 255]);
    doc.layers[1].opacity = 1.;
    use picsie_core::model::{LayerMask, MaskMode};
    doc.layers[1].mask = Some(std::sync::Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Hide,
        raster: None,
        linked: true,
        placement: None,
        strokes: vec![],
    }));
    assert_eq!(&rendered(&doc)[0..4], &[255, 0, 0, 255]);
    assert!(std::sync::Arc::ptr_eq(&doc.layers[0].content, &before));
}

// Local: identity adjustments and a disabled preview are byte-exact no-ops.
#[test]
fn identity_adjustments_change_no_pixel() {
    let mut doc = Document::new("Adjustment", 2, 2).unwrap();
    doc.layers.push(paint_layer(
        "Mixed",
        2,
        2,
        &[
            200, 40, 90, 255, 10, 200, 30, 128, 0, 0, 0, 0, 255, 255, 255, 255,
        ],
    ));
    let plain = rendered(&doc);
    for kind in [AdjustmentKind::Levels, AdjustmentKind::Curves] {
        let mut adjustment = Layer::new(kind.display(), 2, 2, Content::Paint);
        adjustment.adjustment = Some(LayerAdjustment::new(kind));
        doc.layers.push(adjustment);
        assert_eq!(rendered(&doc), plain, "{kind:?} identity must be exact");
        doc.layers.pop();
    }
}

// Local: the base blend mode still applies around a clipped adjustment, and
// stacked adjustments compose in order (two inversions cancel out).
#[test]
fn clipped_adjustments_compose_inside_their_base_stack() {
    let mut doc = Document::new("Adjustment", 2, 2).unwrap();
    doc.layers
        .push(paint_layer("Blue", 2, 2, &[0, 0, 255, 255].repeat(4)));
    let mut green = paint_layer("Green", 2, 2, &[0, 255, 0, 255].repeat(4));
    green.blend = picsie_core::model::Blend::Screen;
    let base = green.id.clone();
    doc.layers.push(green);
    let invert = || {
        let mut value = LayerAdjustment::new(AdjustmentKind::Curves);
        value.curves.channels[0] =
            vec![CurvePoint { x: 0., y: 255. }, CurvePoint { x: 255., y: 0. }];
        let mut layer = Layer::new("Curves", 2, 2, Content::Paint);
        layer.adjustment = Some(value);
        layer.mask_source_id = Some(base.clone());
        layer
    };
    doc.layers.push(invert());
    doc.validate().unwrap();
    // Screen magenta over blue stays magenta; without the curve it is cyan.
    assert_eq!(&rendered(&doc)[0..4], &[255, 0, 255, 255]);
    doc.layers.push(invert());
    doc.validate().unwrap();
    assert_eq!(&rendered(&doc)[0..4], &[0, 255, 255, 255]);
}

// Local: stack position decides what an adjustment sees, including folders.
#[test]
fn adjustment_stack_position_decides_its_source() {
    let white = || paint_layer("White", 2, 2, &[255, 255, 255, 255].repeat(4));
    let blacken = || {
        let mut value = LayerAdjustment::new(AdjustmentKind::Levels);
        value.levels.ranges[0].output_white = 0.;
        let mut layer = Layer::new("Levels", 2, 2, Content::Paint);
        layer.adjustment = Some(value);
        layer
    };
    let mut below = Document::new("Below", 2, 2).unwrap();
    below.layers.push(white());
    below.layers.push(blacken());
    let folder = Layer::new("Folder", 2, 2, Content::Group);
    let folder_id = folder.id.clone();
    below.layers.push(folder);
    let mut inner = white();
    inner.name = "Inner".into();
    inner.parent_id = Some(folder_id.clone());
    below.layers.push(inner);
    assert_eq!(&rendered(&below)[0..4], &[255, 255, 255, 255]);
    let mut above = Document::new("Above", 2, 2).unwrap();
    above.layers.push(white());
    let folder = Layer::new("Folder", 2, 2, Content::Group);
    let folder_id = folder.id.clone();
    above.layers.push(folder);
    let mut inner = white();
    inner.name = "Inner".into();
    inner.parent_id = Some(folder_id);
    above.layers.push(inner);
    above.layers.push(blacken());
    assert_eq!(&rendered(&above)[0..4], &[0, 0, 0, 255]);
}

// AdjustmentLayerTests.adjustmentPersistsDuplicatesAndUndoRestoresSettings,
// scoped to Curves through the editor's undo, duplication and file round-trips.
// Merged and exported pixels must match the live composite.
#[test]
fn curves_persist_duplicate_merge_export_and_undo_through_the_editor() {
    let mut doc = Document::new("Adjustment", 2, 2).unwrap();
    doc.layers
        .push(paint_layer("White", 2, 2, &[255, 255, 255, 255].repeat(4)));
    let mut editor = Editor::new(doc).unwrap();
    editor.viewport = picsie_core::geometry::Viewport {
        width: 2.,
        height: 2.,
        zoom: 1.,
        pan: picsie_core::model::Point::new(0., 0.),
    };
    editor.add_adjustment(AdjustmentKind::Curves).unwrap();
    let id = editor.selected_id().unwrap().to_owned();
    let mut value = LayerAdjustment::new(AdjustmentKind::Curves);
    value.curves.channels[0].insert(1, CurvePoint { x: 128., y: 190. });
    let undo_count = editor.history.info().undo_count;
    editor
        .update_adjustment_curves(value.curves.clone(), true)
        .unwrap();
    editor.commit_adjustment_edit().unwrap();
    // Creation ("New …") was captured above; the edit commits one more entry.
    assert_eq!(editor.history.info().undo_count, undo_count + 1);
    editor.command(picsie_core::editor::Command::Undo).unwrap();
    assert_eq!(
        editor
            .selected()
            .unwrap()
            .adjustment
            .as_ref()
            .unwrap()
            .curves,
        CurvesSettings::default()
    );
    editor.command(picsie_core::editor::Command::Redo).unwrap();
    assert_eq!(
        editor
            .selected()
            .unwrap()
            .adjustment
            .as_ref()
            .unwrap()
            .curves,
        value.curves
    );
    editor
        .command(picsie_core::editor::Command::Duplicate)
        .unwrap();
    assert_eq!(
        editor
            .selected()
            .unwrap()
            .adjustment
            .as_ref()
            .unwrap()
            .curves,
        value.curves
    );
    // Merging bakes the live adjustment into plain pixels.
    editor
        .command(picsie_core::editor::Command::Select {
            id: Some(id.clone()),
            mode: picsie_core::editor::SelectionMode::Replace,
        })
        .unwrap();
    let live = rendered(&editor.history.document);
    editor
        .command(picsie_core::editor::Command::MergeLayers)
        .unwrap();
    // Merge Down bakes the original into plain pixels; the duplicated copy
    // above it remains a live adjustment, so the composite cannot change.
    assert_eq!(editor.history.document.layers.len(), 2);
    assert!(editor.history.document.layers[0].adjustment.is_none());
    assert_eq!(
        editor.history.document.layers[1]
            .adjustment
            .as_ref()
            .unwrap()
            .curves,
        value.curves
    );
    assert_eq!(rendered(&editor.history.document), live);
    // Exported bytes match the composited canvas.
    let bytes = Renderer::default()
        .export(&editor.history.document, false)
        .unwrap();
    let decoded = render::decode(&bytes).unwrap();
    assert_eq!(render::rgba_pixels(&decoded).unwrap(), live);
    let directory = tempfile::tempdir().unwrap();
    let picsie = directory.path().join("curves.picsie");
    picsie_core::files::save_project(&picsie, &editor.history.document).unwrap();
    let reopened = picsie_core::files::open_project(&picsie).unwrap();
    assert_eq!(rendered(&reopened), live);
    let package = directory.path().join("curves.comp");
    picsie_core::files::save_project(&package, &editor.history.document).unwrap();
    let loaded = picsie_core::files::open_project(&package).unwrap();
    assert_eq!(rendered(&loaded), live);
    for doc in [&reopened, &loaded] {
        let surviving = doc
            .layers
            .iter()
            .find(|l| l.adjustment.is_some())
            .unwrap()
            .adjustment
            .clone()
            .unwrap();
        assert_eq!(surviving.curves, value.curves);
    }
    // A reopened package stays editable: histogram, preview and commit work.
    let mut again = Editor::new(loaded).unwrap();
    again.viewport = picsie_core::geometry::Viewport {
        width: 2.,
        height: 2.,
        zoom: 1.,
        pan: picsie_core::model::Point::new(0., 0.),
    };
    let edit_id = again
        .history
        .document
        .layers
        .iter()
        .find(|l| l.adjustment.is_some())
        .unwrap()
        .id
        .clone();
    again.begin_adjustment_edit(edit_id).unwrap();
    assert!(again.adjustment_edit.is_some());
    again.commit_adjustment_edit().unwrap();
    assert_eq!(rendered(&again.history.document), rendered(&reopened));
}

// Local: preview/cancel/apply, auto, sampling and validation workflows.
#[test]
fn levels_preview_cancel_apply_auto_and_sample() {
    let mut doc = Document::new("Levels", 4, 1).unwrap();
    doc.layers.push(paint_layer(
        "Ramp",
        4,
        1,
        &[
            0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255, 64, 64, 64, 255,
        ],
    ));
    let mut editor = Editor::new(doc).unwrap();
    editor.viewport = picsie_core::geometry::Viewport {
        width: 4.,
        height: 2.,
        zoom: 1.,
        pan: picsie_core::model::Point::new(0., 0.),
    };
    let pristine = editor.history.document.clone();
    let count = editor.history.info().undo_count;
    editor.add_adjustment(AdjustmentKind::Levels).unwrap();
    assert!(editor.adjustment_edit.is_some());
    // The edit histogram reads the composite below; every channel row sees
    // the black pixel, so the RGB mean row holds a full count of one.
    let edit = editor.adjustment_edit.as_ref().unwrap();
    assert_eq!(edit.histogram[0][0], 1.);
    assert_eq!(edit.histogram[1][255], 1.);
    // Preview off shows the original document even though working values differ.
    let mut settings = LevelsSettings::default();
    settings.ranges[0].output_white = 0.;
    editor
        .update_adjustment_levels(settings.clone(), true)
        .unwrap();
    assert_eq!(&rendered(&editor.history.document)[0..4], &[0, 0, 0, 255]);
    editor.set_adjustment_preview(false).unwrap();
    assert_eq!(rendered(&editor.history.document), rendered(&pristine));
    editor.set_adjustment_preview(true).unwrap();
    // Cancel restores the created adjustment with no new undo entry; its
    // pixels still match the pristine canvas.
    editor.cancel_adjustment_edit();
    assert_eq!(editor.history.document.layers.len(), 2);
    assert_eq!(
        editor.history.document.layers[1].adjustment,
        Some(LayerAdjustment::new(AdjustmentKind::Levels))
    );
    assert_eq!(rendered(&editor.history.document), rendered(&pristine));
    assert_eq!(editor.history.info().undo_count, count + 1);
    // Committing identity settings adds no undo entry.
    editor
        .begin_adjustment_edit(editor.history.document.layers[1].id.clone())
        .unwrap();
    editor.commit_adjustment_edit().unwrap();
    assert_eq!(editor.history.info().undo_count, count + 1);
    // Automatic levels stretch to the shared endpoints actually present.
    editor
        .begin_adjustment_edit(editor.history.document.layers[1].id.clone())
        .unwrap();
    editor.auto_levels(LevelsAuto::Contrast).unwrap();
    let auto = editor
        .adjustment_edit
        .as_ref()
        .unwrap()
        .working
        .levels
        .clone();
    assert_eq!((auto.ranges[0].black, auto.ranges[0].white), (0., 255.));
    // Sampling the dark gray pixel as gray bends gamma away from 1.
    editor
        .sample_levels(picsie_core::model::Point::new(3.5, 0.5), LevelsSample::Gray)
        .unwrap();
    let gamma = editor
        .adjustment_edit
        .as_ref()
        .unwrap()
        .working
        .levels
        .ranges[1]
        .gamma;
    assert!(gamma > 1. && gamma < 9.99);
    // Out-of-document samples are rejected without changes.
    let before = editor
        .adjustment_edit
        .as_ref()
        .unwrap()
        .working
        .levels
        .clone();
    editor
        .sample_levels(
            picsie_core::model::Point::new(99., 99.),
            LevelsSample::Black,
        )
        .unwrap();
    assert_eq!(
        editor.adjustment_edit.as_ref().unwrap().working.levels,
        before
    );
    editor.commit_adjustment_edit().unwrap();
    assert_eq!(editor.history.info().undo_count, count + 2);
    // Undo of the edit step returns to the created adjustment, not the
    // pre-creation document; its pixels still match the pristine canvas.
    editor.command(picsie_core::editor::Command::Undo).unwrap();
    assert_eq!(editor.history.document.layers.len(), 2);
    assert_eq!(
        editor.history.document.layers[1].adjustment,
        Some(LayerAdjustment::new(AdjustmentKind::Levels))
    );
    assert_eq!(rendered(&editor.history.document), rendered(&pristine));
}

// Local: foreign adjustment kinds stay rejected in .comp packages.
#[test]
fn comp_rejects_unsupported_adjustment_kinds() {
    let mut doc = Document::new("Foreign", 2, 2).unwrap();
    doc.layers
        .push(paint_layer("White", 2, 2, &[255, 255, 255, 255].repeat(4)));
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("foreign.comp");
    picsie_core::files::save_project(&package, &doc).unwrap();
    let manifest = std::fs::read(package.join("manifest.json")).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    json["layers"][0]["adjustment"] = serde_json::json!({
        "kind": "Exposure",
        "hue": 0, "saturation": 0, "lightness": 0, "colorize": false,
        "levels": LevelsSettings::default(),
        "curves": CurvesSettings::default(),
        "exposureSettings": { "exposure": 1, "offset": 0, "gamma": 1 },
    });
    json["version"] = serde_json::json!(8);
    std::fs::write(
        package.join("manifest.json"),
        serde_json::to_vec(&json).unwrap(),
    )
    .unwrap();
    assert!(picsie_core::files::open_project(&package).is_err());
}

// ImageAdjustmentTests.settingsSaveAndOlderAdjustmentsStillOpen, scoped: the new
// adjustment JSON omits every unported settings block, and legacy layers decode.
#[test]
fn adjustment_json_omits_unported_blocks_and_legacy_decodes() {
    let levels = LayerAdjustment::new(AdjustmentKind::Levels);
    let json = serde_json::to_string(&levels).unwrap();
    for key in ["exposureSettings", "gradientMapSettings", "grainSettings"] {
        assert!(!json.contains(key));
    }
    assert_eq!(
        serde_json::from_str::<LayerAdjustment>(&json).unwrap(),
        levels
    );
    // A pre-adjustment .picsie layer without the field still opens. Real
    // project files always carry their (possibly empty) stroke list.
    let raw = r#"{"id":"00000000-0000-4000-8000-000000000000","name":"Old","visible":true,"locked":false,"width":2,"height":2,"x":0.0,"y":0.0,"scaleX":1.0,"scaleY":1.0,"rotation":0.0,"flipX":false,"flipY":false,"opacity":1.0,"blend":"source-over","sampling":"High","brightness":1.0,"saturation":1.0,"blur":0.0,"content":{"kind":"paint"},"strokes":[]}"#;
    let layer: Layer = serde_json::from_str(raw).unwrap();
    assert!(layer.adjustment.is_none());
    layer.validate().unwrap();
}

// Local: source-less adjustments refuse every pixel-placement command while
// their masks stay editable. canTransform upstream requires a pixel asset,
// which these layers never have.
#[test]
fn sourceless_adjustments_refuse_pixel_placement_but_keep_masks() {
    use picsie_core::editor::{Command, SelectionMode};
    use picsie_core::model::MaskMode;
    let mut doc = Document::new("Guard", 8, 8).unwrap();
    doc.layers
        .push(paint_layer("Red", 8, 8, &[255, 0, 0, 255].repeat(64)));
    let mut editor = Editor::new(doc).unwrap();
    editor.add_adjustment(AdjustmentKind::Levels).unwrap();
    let id = editor.selected_id().unwrap().to_owned();
    let plain = editor.history.document.clone();
    let count = editor.history.info().undo_count;
    // Geometry, pixels and sampling commands are no-ops with no undo entries.
    for command in [
        Command::BeginTransform,
        Command::SetTransformField {
            field: picsie_core::editor::TransformField::X,
            value: 30.,
        },
        Command::BeginDistort,
        Command::Nudge {
            delta: picsie_core::model::Point::new(3., 4.),
        },
        Command::FillSelection,
        Command::ClearSelectedPixels,
        Command::SelectLayerPixels,
    ] {
        editor.command(command).unwrap();
    }
    assert_eq!(editor.history.document.layers[1], plain.layers[1]);
    assert_eq!(editor.history.info().undo_count, count);
    // Brush strokes cannot target the adjustment either.
    assert!(!editor.can_edit_pixels());
    // Masks remain first-class: add, paint-target and mask-only patches work.
    editor
        .command(Command::AddMask {
            base: MaskMode::Reveal,
        })
        .unwrap();
    assert!(editor.history.document.layers[1].mask.is_some());
    editor
        .command(Command::UpdateLayer {
            patch: serde_json::json!({"opacity": 0.5}),
        })
        .unwrap();
    assert_eq!(editor.history.document.layers[1].opacity, 0.5);
    // Geometry patches stay rejected even though masks are allowed.
    assert!(
        editor
            .command(Command::UpdateLayer {
                patch: serde_json::json!({"x": 9.}),
            })
            .is_err()
    );
    assert_eq!(editor.history.document.layers[1].x, 0.);
    // Canvas operations keep the adjustment valid and full-document.
    editor
        .command(Command::ResizeCanvas {
            options: picsie_core::canvas_size::CanvasSizeOptions {
                width: 10,
                height: 6,
                anchor: 4,
                fill: None,
            },
        })
        .unwrap();
    let layer = editor
        .history
        .document
        .layers
        .iter()
        .find(|l| l.id == id)
        .unwrap();
    assert_eq!(
        (layer.width, layer.height, layer.x, layer.y),
        (10, 6, 0., 0.)
    );
    editor
        .command(Command::ResizeImage {
            options: picsie_core::image_size::ImageSizeOptions {
                width: 4,
                height: 4,
                resolution: 72.,
                sampling: picsie_core::model::Sampling::Nearest,
            },
        })
        .unwrap();
    let layer = editor
        .history
        .document
        .layers
        .iter()
        .find(|l| l.id == id)
        .unwrap();
    assert_eq!(
        (layer.width, layer.height, layer.x, layer.y),
        (4, 4, 0., 0.)
    );
    assert!(matches!(
        layer.content.as_ref(),
        picsie_core::model::Content::Paint
    ));
    editor.history.document.validate().unwrap();
    // Deleting a clipped base releases (never bakes) the adjustment link.
    let mut doc = Document::new("Release", 2, 2).unwrap();
    doc.layers
        .push(paint_layer("Blue", 2, 2, &[0, 0, 255, 255].repeat(4)));
    let base = doc.layers[0].id.clone();
    let mut editor = Editor::new(doc).unwrap();
    editor.add_adjustment(AdjustmentKind::Curves).unwrap();
    let adjustment = editor.selected_id().unwrap().to_owned();
    editor.commit_adjustment_edit().unwrap();
    editor
        .command(Command::Select {
            id: Some(adjustment.clone()),
            mode: SelectionMode::Replace,
        })
        .unwrap();
    editor.command(Command::ToggleClippingMask).unwrap();
    assert_eq!(
        editor
            .history
            .document
            .layers
            .iter()
            .find(|l| l.id == adjustment)
            .unwrap()
            .mask_source_id
            .as_deref(),
        Some(base.as_str())
    );
    editor
        .command(Command::Select {
            id: Some(base.clone()),
            mode: SelectionMode::Replace,
        })
        .unwrap();
    editor.command(Command::Remove).unwrap();
    let layer = editor
        .history
        .document
        .layers
        .iter()
        .find(|l| l.id == adjustment)
        .unwrap();
    assert!(layer.mask_source_id.is_none());
    assert!(matches!(
        layer.content.as_ref(),
        picsie_core::model::Content::Paint
    ));
    editor.history.document.validate().unwrap();
}

// Local, from AdjustmentEditing.swift: the adjustment edit histogram reads the
// alpha-weighted composite below with a nil selection, so an active marquee
// must not change it or the derived Auto settings. Group records stay visible
// as hierarchy/live-mask references, so nesting preserves the sampling input.
#[test]
fn adjustment_histogram_ignores_active_selection_and_keeps_groups() {
    use picsie_core::editor::{Command, SelectionMode};
    let mut doc = Document::new("Sampling", 4, 1).unwrap();
    doc.layers.push(paint_layer(
        "Ramp",
        4,
        1,
        &[
            0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255, 64, 64, 64, 255,
        ],
    ));
    let folder = Layer::new("Folder", 4, 1, Content::Group);
    let folder_id = folder.id.clone();
    doc.layers.push(folder);
    let mut inner = paint_layer("Inner", 4, 1, &[255, 255, 255, 255].repeat(4));
    inner.parent_id = Some(folder_id.clone());
    doc.layers.push(inner);
    let mut editor = Editor::new(doc).unwrap();
    editor.viewport = picsie_core::geometry::Viewport {
        width: 4.,
        height: 1.,
        zoom: 1.,
        pan: picsie_core::model::Point::new(0., 0.),
    };
    // Nest the adjustment inside the folder: it still samples the full below.
    editor
        .command(Command::Select {
            id: Some(folder_id.clone()),
            mode: SelectionMode::Replace,
        })
        .unwrap();
    editor.add_adjustment(AdjustmentKind::Levels).unwrap();
    let plain_histogram = editor.adjustment_edit.as_ref().unwrap().histogram;
    // Marquee a single pixel, then reopen: identical histogram and Auto.
    editor.cancel_adjustment_edit();
    editor.command(Command::SelectAllPixels).unwrap();
    assert!(editor.history.pixel_selection.is_some());
    let target = editor
        .history
        .document
        .layers
        .iter()
        .find(|l| l.adjustment.is_some())
        .unwrap()
        .id
        .clone();
    editor.begin_adjustment_edit(target).unwrap();
    let selected_histogram = editor.adjustment_edit.as_ref().unwrap().histogram;
    assert_eq!(selected_histogram, plain_histogram);
    editor.auto_levels(LevelsAuto::Contrast).unwrap();
    let with_selection = editor
        .adjustment_edit
        .as_ref()
        .unwrap()
        .working
        .levels
        .clone();
    editor.cancel_adjustment_edit();
    editor.command(Command::DeselectPixels).unwrap();
    let target = editor
        .history
        .document
        .layers
        .iter()
        .find(|l| l.adjustment.is_some())
        .unwrap()
        .id
        .clone();
    editor.begin_adjustment_edit(target).unwrap();
    editor.auto_levels(LevelsAuto::Contrast).unwrap();
    assert_eq!(
        editor.adjustment_edit.as_ref().unwrap().working.levels,
        with_selection
    );
}
