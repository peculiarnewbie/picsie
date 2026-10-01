//! Diagnostic only: time the actual engine and desktop metadata snapshot paths.
//! No raster/UI/presentation timing or translated upstream fixture claim.
use anyhow::{Result, ensure};
use picsie_core::{
    editor::{Command, Editor, SelectionMode, Tool},
    files,
    model::Content,
};
use serde_json::json;
use std::{hint::black_box, path::Path, time::Instant};

#[allow(dead_code)]
#[path = "../src/state.rs"]
mod state;

fn main() -> Result<()> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    ensure!(!paths.is_empty(), "Pass stress fixture project paths");
    let mut results = vec![];
    for path in paths {
        let mut editor = Editor::new(files::open_project(Path::new(&path))?)?;
        editor.command(Command::SetTool { tool: Tool::Hand })?;
        editor.command(Command::ResizeViewport {
            width: 936.,
            height: 734.,
        })?;
        editor.command(Command::Fit)?;
        let groups: Vec<_> = editor
            .history
            .document
            .layers
            .iter()
            .rev()
            .filter(|layer| matches!(layer.content.as_ref(), Content::Group))
            .map(|layer| layer.id.clone())
            .collect();
        if let Some(id) = groups.first() {
            editor.select(Some(id.clone()), SelectionMode::Replace)?;
        }
        let document = serde_json::to_value(&editor.history.document)?;
        let selection = editor.selected_id().map(str::to_owned);
        for phase in ["expanded", "collapsed"] {
            if phase == "collapsed" {
                if groups.is_empty() {
                    continue;
                }
                for id in &groups {
                    editor.command(Command::ToggleGroupExpansion { id: id.clone() })?;
                }
            }
            ensure!(serde_json::to_value(&editor.history.document)? == document);
            ensure!(editor.selected_id() == selection.as_deref());
            ensure!(editor.history.info().undo_count == 0);
            let mut plain_ms = vec![];
            let mut desktop_ms = vec![];
            for sample in 0..12 {
                let start = Instant::now();
                let value = black_box(editor.snapshot());
                let plain = start.elapsed().as_secs_f64() * 1000.;
                drop(value);
                let start = Instant::now();
                let value = black_box(state::Snapshot::capture(&editor)?);
                let desktop = start.elapsed().as_secs_f64() * 1000.;
                drop(value);
                if sample >= 2 {
                    plain_ms.push(plain);
                    desktop_ms.push(desktop);
                }
            }
            let snapshot = state::Snapshot::capture(&editor)?;
            results.push(json!({
                "fixture": path, "phase": phase,
                "layers": editor.history.document.layers.len(),
                "rows": snapshot.layer_rows.len(),
                "snapshot_json_bytes": serde_json::to_vec(&editor.snapshot())?.len(),
                "editor_snapshot_ms": plain_ms, "desktop_snapshot_ms": desktop_ms,
                "document_unchanged": true, "selection_unchanged": true,
                "history_unchanged": true
            }));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope": "Actual Editor::snapshot and desktop Snapshot::capture; no rendering, UI or input latency. Fresh editor per fixture, top folder selected in both phases, two warmups then ten captures per path/phase. Local stress fixtures, not upstream ports.",
            "results": results
        }))?
    );
    Ok(())
}
