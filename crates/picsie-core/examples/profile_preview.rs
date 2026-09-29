//! Release-only stage timings for the existing demo preview; no presentation/UI work.
use anyhow::Result;
use picsie_core::{
    editor::{Command, Editor},
    model::demo_document,
    render::Renderer,
};
use std::{hint::black_box, time::Instant};

fn measure(name: &str, mut work: impl FnMut() -> Result<()>) -> Result<()> {
    let mut samples = Vec::new();
    for i in 0..45 {
        let start = Instant::now();
        work()?;
        if i >= 5 {
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{name}: median={:.3} ms p95={:.3} ms",
        samples[20], samples[37]
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut editor = Editor::new(demo_document())?;
    editor.command(Command::ResizeViewport {
        width: 936.,
        height: 734.,
    })?;
    editor.command(Command::Fit)?;
    let mut renderer = Renderer::default();
    measure("document composite", || {
        black_box(renderer.render(&editor.history.document)?);
        Ok(())
    })?;
    measure("full preview", || {
        black_box(renderer.preview(
            &editor.history.document,
            &editor.viewport,
            &editor.selection.ids,
            true,
            None,
            None,
            None,
            None,
        )?);
        Ok(())
    })?;
    for layer in &editor.history.document.layers {
        let mut doc = editor.history.document.clone();
        doc.layers = vec![layer.clone()];
        measure(&format!("isolated layer: {}", layer.name), || {
            black_box(renderer.render(&doc)?);
            Ok(())
        })?;
    }
    Ok(())
}
