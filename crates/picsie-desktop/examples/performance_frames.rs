//! Local worker phase diagnostic, without GPUI, input observation or display timing.
//! Uses the production worker; cold composition is separate from warm navigation.
use anyhow::{Result, ensure};
use picsie_core::{
    editor::{Command, Tool},
    files,
    model::Point,
};
use serde_json::json;
use std::{path::Path, time::Instant};

#[allow(dead_code)]
#[path = "../src/engine.rs"]
mod engine;
#[allow(dead_code, unused_imports)]
#[path = "../src/state.rs"]
mod state;

fn frame(worker: &engine::Engine, sequence: u64) -> Result<engine::Frame> {
    loop {
        worker.wakeups.recv_blocking()?;
        if let Some(result) = worker.take_frame() {
            let frame = result.map_err(anyhow::Error::msg)?;
            if frame.sequence >= sequence {
                return Ok(frame);
            }
        }
    }
}

fn main() -> Result<()> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    ensure!(!paths.is_empty(), "Pass stress fixture project paths");
    let mut results = vec![];
    for path in paths {
        let document = files::open_project(Path::new(&path))?;
        let mut worker = engine::Engine::start(document);
        let sequence = worker.send(vec![
            Command::SetTool { tool: Tool::Hand },
            Command::ResizeViewport {
                width: 936.,
                height: 734.,
            },
            Command::Fit,
        ]);
        let cold = frame(&worker, sequence)?;
        let expected = serde_json::to_value(&cold.state.document)?;
        let mut samples = vec![];
        for case in ["zoom_100", "pan_100", "zoom_fit"] {
            for sample in 0..12 {
                let command = match case {
                    "zoom_100" => Command::Zoom {
                        zoom: 1.,
                        point: None,
                    },
                    "zoom_fit" => Command::Fit,
                    _ => Command::Pan {
                        delta: Point::new(if sample % 2 == 0 { 12. } else { -12. }, 0.),
                    },
                };
                // Zoom controls always begin at their opposite endpoint, untimed.
                if case != "pan_100" {
                    let reset = if case == "zoom_100" {
                        Command::Fit
                    } else {
                        Command::Zoom {
                            zoom: 1.,
                            point: None,
                        }
                    };
                    let seq = worker.send(vec![reset]);
                    frame(&worker, seq)?;
                }
                let start = Instant::now();
                let seq = worker.send(vec![command]);
                let current = frame(&worker, seq)?;
                let worker_available_ms = start.elapsed().as_secs_f64() * 1000.;
                ensure!(
                    serde_json::to_value(&current.state.document)? == expected,
                    "Navigation changed document metadata"
                );
                ensure!(
                    current.state.history.undo_count == 0,
                    "Navigation added history"
                );
                if sample >= 2 {
                    samples.push(json!({"case":case,"command_ms":current.command_ms,"render_ms":current.render_ms,"pixels_ms":current.pixels_ms,"thumbnails_ms":current.thumbnails_ms,"snapshot_ms":current.snapshot_ms,"worker_available_ms":worker_available_ms}));
                }
            }
        }
        results.push(json!({"fixture":path,"cold":{"render_ms":cold.render_ms,"thumbnails_ms":cold.thumbnails_ms,"snapshot_ms":cold.snapshot_ms},"samples":samples,"document_unchanged":true,"history_unchanged":true}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"scope":"Production worker phases only; excludes GPUI and display. Two warmups then ten measured frames per case. Cold first composition separate. Local fixtures, not upstream fixtures.","results":results})
        )?
    );
    Ok(())
}
