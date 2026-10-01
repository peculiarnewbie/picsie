//! Local workload benchmark, not a translated upstream test or production engine.
//! Uses existing editor commands. Paired GIMP workloads live in scripts/perf/.
use anyhow::{Result, ensure};
use picsie_core::{
    crop::CropRect,
    editor::{Command, Editor, Modifiers, Phase, PointerSample, Tool},
    files,
    geometry::Viewport,
    image_size::ImageSizeOptions,
    model::{Document, Layer, Point, Sampling},
    pixel_selection::PixelSelection,
    render::{self, Renderer},
};
use serde_json::json;
use skia_safe::{PathBuilder, Rect};
use std::{fs, hint::black_box, path::Path, time::Instant};

const CASES: [&str; 9] = [
    "selection-fill",
    "selection-clear",
    "selection-fill-feather20",
    "selection-clear-feather20",
    "crop-retained",
    "image-resize-half-linear",
    "image-resize-half-nearest",
    "brush-hard100",
    "eraser-hard100",
];
const SIZES: [(u32, u32); 2] = [(1200, 800), (3600, 2400)];

fn fixture(width: u32, height: u32) -> Result<Document> {
    let grid: u32 = std::env::var("PICSIE_PERF_GRID")
        .unwrap_or_else(|_| "13".into())
        .parse()?;
    ensure!(
        grid > 0 && grid <= 1000,
        "Fixture grid must be between 1 and 1000"
    );
    let mut pixels = [101, 135, 255, 255].repeat(width as usize * height as usize);
    // Include high-frequency content away from the selection and brush trajectory.
    for y in height * 3 / 4..height {
        for x in width * 3 / 4..width {
            let value = if (x / grid + y / grid) % 2 == 0 {
                40
            } else {
                220
            };
            let offset = ((y * width + x) * 4) as usize;
            pixels[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
        }
    }
    let mut doc = Document::new("Performance behaviors", width, height)?;
    doc.layers.push(Layer::new(
        "Raster",
        width,
        height,
        render::native_content(render::rgba_image(width, height, &pixels)?),
    ));
    Ok(doc)
}

fn prepare(folder: &Path) -> Result<()> {
    fs::create_dir_all(folder)?;
    for (width, height) in SIZES {
        let doc = fixture(width, height)?;
        let image = Renderer::default().render(&doc)?.image_snapshot();
        fs::write(
            folder.join(format!("{width}.png")),
            render::encode(&image, false)?,
        )?;
        files::save_project(&folder.join(format!("{width}.picsie")), &doc)?;
    }
    Ok(())
}

fn setup(doc: &Document, case: &str) -> Result<Editor> {
    let mut editor = Editor::new(doc.clone())?;
    editor.viewport = Viewport {
        width: doc.width as f64,
        height: doc.height as f64,
        zoom: 1.,
        pan: Point::default(),
    };
    editor.color = "#e53935".into();
    if case.starts_with("selection-") {
        let mut path = PathBuilder::new();
        path.add_rect(
            Rect::from_xywh(
                doc.width as f32 / 12.,
                doc.height as f32 / 8.,
                doc.width as f32 / 2.,
                doc.height as f32 / 2.,
            ),
            None,
            None,
        );
        editor.history.pixel_selection = Some(PixelSelection::from_path(
            doc.width,
            doc.height,
            path.detach(),
            if case.ends_with("feather20") { 20. } else { 0. },
        )?);
    }
    if case == "crop-retained" {
        editor.crop_rect = Some(CropRect {
            x: doc.width as f64 / 12.,
            y: doc.height as f64 / 8.,
            width: doc.width as f64 * 3. / 4.,
            height: doc.height as f64 * 3. / 4.,
        });
    }
    if case.starts_with("brush-") || case.starts_with("eraser-") {
        editor.tool = if case.starts_with("brush-") {
            Tool::Brush
        } else {
            Tool::Eraser
        };
        editor.brush_size = 100.;
        editor.brush_hardness = 1.;
        editor.brush_opacity = 1.;
        editor.brush_smoothing = 0.;
    }
    Ok(editor)
}

fn perform(editor: &mut Editor, case: &str) -> Result<()> {
    let doc = &editor.history.document;
    let (width, height) = (doc.width, doc.height);
    let command = match case {
        "selection-fill" | "selection-fill-feather20" => Command::FillSelection,
        "selection-clear" | "selection-clear-feather20" => Command::ClearSelectedPixels,
        "crop-retained" => Command::CommitCrop,
        "image-resize-half-linear" | "image-resize-half-nearest" => Command::ResizeImage {
            options: ImageSizeOptions {
                width: width / 2,
                height: height / 2,
                resolution: 96.,
                sampling: if case.ends_with("nearest") {
                    Sampling::Nearest
                } else {
                    Sampling::Smooth
                },
            },
        },
        "brush-hard100" | "eraser-hard100" => {
            // Same 61 straight-line control points as GIMP's public paint API.
            // The editor currently publishes a layer snapshot at every pointer sample.
            let mut samples: Vec<_> = (0..=60)
                .map(|i| PointerSample {
                    phase: if i == 0 { Phase::Down } else { Phase::Move },
                    point: Point::new(
                        width as f64 / 12. + i as f64 * width as f64 / 120.,
                        height as f64 / 2.,
                    ),
                    modifiers: Modifiers::default(),
                })
                .collect();
            samples.push(PointerSample {
                phase: Phase::Up,
                point: samples.last().unwrap().point,
                modifiers: Modifiers::default(),
            });
            Command::Pointer { samples }
        }
        _ => anyhow::bail!("Unknown workload: {case}"),
    };
    editor.command(command)
}

fn validate(editor: &Editor, case: &str, pixels: &[u8], original: &Document) -> Result<()> {
    let doc = &editor.history.document;
    ensure!(pixels.len() == doc.width as usize * doc.height as usize * 4);
    ensure!(
        editor.history.info().undo_count == 1,
        "Workload must commit one edit"
    );
    let (x, y) = (doc.width / 3, doc.height / 2);
    let at = ((y * doc.width + x) * 4) as usize;
    let center = &pixels[at..at + 4];
    if case.starts_with("selection-fill") || case.starts_with("brush-") {
        ensure!(
            center == [229, 57, 53, 255],
            "Paint did not affect stroke/selection interior: {center:?}"
        );
    } else if case.starts_with("selection-clear") || case.starts_with("eraser-") {
        ensure!(center[3] == 0, "Clear/erase did not remove interior alpha");
    } else if case == "crop-retained" {
        ensure!(doc.width == original.width * 3 / 4 && doc.height == original.height * 3 / 4);
        ensure!(doc.layers[0].width == original.width && doc.layers[0].height == original.height);
        ensure!(doc.layers[0].x == -(original.width as f64 / 12.));
    } else {
        ensure!(doc.width == original.width / 2 && doc.height == original.height / 2);
        ensure!(doc.layers[0].width == doc.width && doc.layers[0].height == doc.height);
    }
    ensure!(
        pixels[..4] == [101, 135, 255, 255],
        "Outside region was unexpectedly changed"
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() >= 3,
        "Usage: performance_behaviors prepare FIXTURES | run OUTPUT WARMUPS SAMPLES [CASE]"
    );
    let folder = Path::new(&args[2]);
    if args[1] == "prepare" {
        return prepare(folder);
    }
    ensure!(args[1] == "run");
    fs::create_dir_all(folder)?;
    let warmups: usize = args.get(3).map_or(Ok(5), |v| v.parse())?;
    let samples: usize = args.get(4).map_or(Ok(16), |v| v.parse())?;
    ensure!(samples > 0);
    let mut results = vec![];
    for (width, height) in SIZES {
        let original = fixture(width, height)?;
        for case in CASES {
            if args.get(5).is_some_and(|wanted| wanted != case) {
                continue;
            }
            let mut rows = vec![];
            for i in 0..warmups + samples {
                let mut editor = setup(&original, case)?;
                let mut renderer = Renderer::default();
                let started = Instant::now();
                perform(&mut editor, case)?;
                let command_ms = started.elapsed().as_secs_f64() * 1000.;
                let output = renderer.render(&editor.history.document)?.image_snapshot();
                let pixels = render::rgba_pixels(&output)?;
                let total_ms = started.elapsed().as_secs_f64() * 1000.;
                validate(&editor, case, &pixels, &original)?;
                if i == warmups {
                    fs::write(folder.join(format!("{width}-{case}.rgba")), &pixels)?;
                }
                black_box(&pixels);
                if i >= warmups {
                    rows.push(json!({"command_ms":command_ms,"total_ms":total_ms}));
                }
            }
            results.push(json!({"width":width,"height":height,"case":case,"samples":rows}));
            eprintln!("Completed {width} {case}");
        }
    }
    println!(
        "{}",
        json!({"app":"picsie","scope":"Existing editor command with history plus complete document RGBA render/read; excludes UI/GPU/file encoding. Fresh editor sharing immutable original pixels each sample; setup and assertions outside timer.","results":results})
    );
    Ok(())
}
