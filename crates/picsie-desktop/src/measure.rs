//! CPU transport comparison. This deliberately does not claim end-to-end display latency.
use crate::engine::{bgra, preview};
use anyhow::{Result, ensure};
use gpui_kit::RenderImage;
use picsie_core::{
    editor::{Command, Editor, SelectionMode},
    files::Frames,
    model::demo_document,
    render::{Renderer, frame_bytes},
};
use serde_json::json;
use std::{hint::black_box, time::Instant};

fn timed<T>(samples: &mut Vec<f64>, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let start = Instant::now();
    let value = f()?;
    samples.push(start.elapsed().as_secs_f64() * 1000.);
    Ok(value)
}

fn stats(values: &mut [f64]) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    json!({"median_ms": values[values.len()/2], "p95_ms":values[((values.len() as f64 * 0.95).ceil() as usize - 1).min(values.len()-1)]})
}
pub fn run(moving: bool) -> Result<()> {
    let mut results = Vec::new();
    for (width, height) in [(936, 734), (1920, 1080)] {
        let mut editor = Editor::new(demo_document())?;
        editor.command(Command::ResizeViewport {
            width: width as f64,
            height: height as f64,
        })?;
        editor.command(Command::Fit)?;
        if moving {
            editor.command(Command::Select {
                id: Some(editor.history.document.layers[1].id.clone()),
                mode: SelectionMode::Replace,
            })?;
        }
        let mut renderer = Renderer::default();
        let mut files = Frames::new()?;
        let mut render_times = Vec::new();
        let mut raw_times = Vec::new();
        let mut file_times = Vec::new();
        let mut encoded_size = 0;
        for iteration in 0..45 {
            if moving {
                editor.command(Command::Nudge {
                    delta: picsie_core::model::Point::new(
                        if iteration % 2 == 0 { 10. } else { -10. },
                        0.,
                    ),
                })?;
            }
            let mut surface = timed(&mut render_times, || preview(&editor, &mut renderer))?;
            // Alternate order to limit cache/ordering bias. Five complete iterations warm caches.
            for native in if iteration % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                if native {
                    black_box(timed(&mut raw_times, || {
                        let pixels = bgra(&mut surface)?;
                        let image = image::RgbaImage::from_raw(width, height, pixels).unwrap();
                        Ok(RenderImage::new(vec![image::Frame::new(image)]))
                    })?);
                } else {
                    black_box(timed(&mut file_times, || {
                        let encoded = frame_bytes(&mut surface)?;
                        encoded_size = encoded.len();
                        let path = files.publish(&encoded)?;
                        let loaded = std::fs::read(&path)?;
                        Ok(
                            image::load_from_memory_with_format(&loaded, image::ImageFormat::Tiff)?
                                .into_rgba8(),
                        )
                    })?);
                }
            }
            if iteration == 4 {
                render_times.clear();
                raw_times.clear();
                file_times.clear();
            }
        }
        // Verify pixel equivalence, including the channel-order boundary, on the actual demo.
        let mut surface = preview(&editor, &mut renderer)?;
        let raw = bgra(&mut surface)?;
        let rgba = image::load_from_memory_with_format(
            &frame_bytes(&mut surface)?,
            image::ImageFormat::Tiff,
        )?
        .into_rgba8();
        ensure!(
            raw.chunks_exact(4)
                .zip(rgba.as_raw().chunks_exact(4))
                .all(|(a, b)| [a[2], a[1], a[0], a[3]] == b),
            "Native frame differs from TIFF decoded pixels"
        );
        results.push(
            json!({"width":width, "height":height, "samples":40, "raw_bytes":raw.len(),
            "tiff_bytes":encoded_size, "pixels_equal":true,
            "common_engine_preview":stats(&mut render_times),
            "native_bgra_and_render_image":stats(&mut raw_times),
            "tiff_encode_publish_read_decode":stats(&mut file_times)}),
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope":"CPU transport microbenchmark; excludes Node/FFI, UI scheduling, GPU upload, presentation and scanout",
            "workload": if moving {"Alternating 10-document-pixel Electric blue nudges"} else {"Unchanged demo"},
            "profile":if cfg!(debug_assertions) {"dev"} else {"release"},
            "file_location":"picsie-core Frames: /dev/shm on Linux when available, otherwise temporary directory",
            "results":results
        }))?
    );
    Ok(())
}
