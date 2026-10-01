//! Diagnostic only: time the existing public calls used by Editor::snapshot.
//! The query loops follow editor.rs; no query algorithm or app behavior changes.
use anyhow::{Result, ensure};
use picsie_core::{
    editor::{Editor, SelectionMode},
    files, live_mask,
    model::Content,
};
use serde_json::json;
use std::{hint::black_box, path::Path, time::Instant};

fn measure<T>(mut query: impl FnMut() -> T) -> Vec<f64> {
    (0..12)
        .filter_map(|sample| {
            let start = Instant::now();
            let value = black_box(query());
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            drop(value);
            (sample >= 2).then_some(elapsed)
        })
        .collect()
}

fn main() -> Result<()> {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    ensure!(!paths.is_empty(), "Pass stress fixture project paths");
    let mut results = vec![];
    for path in paths {
        let mut editor = Editor::new(files::open_project(Path::new(&path))?)?;
        if let Some(group) = editor
            .history
            .document
            .layers
            .iter()
            .rev()
            .find(|layer| matches!(layer.content.as_ref(), Content::Group))
        {
            editor.select(Some(group.id.clone()), SelectionMode::Replace)?;
        }
        let doc = &editor.history.document;
        let before = serde_json::to_value(doc)?;
        let selected = editor.selected_id();
        let document_metadata_ms = measure(|| doc.metadata());
        // Historical public-query diagnostic for snapshot maskSourceIds.
        // The production snapshot now shares one LayerIndex across candidates;
        // these public wrappers build an index per call, so this is not its cost.
        let clipping_candidates_ms = measure(|| {
            doc.layers
                .iter()
                .filter(|layer| {
                    selected.is_some_and(|target| live_mask::can_link(doc, &layer.id, target))
                })
                .map(|layer| &layer.id)
                .collect::<Vec<_>>()
        });
        // Same per-row predicate as layer_rows, across all expanded layers.
        // Production rows also share one index instead of rebuilding it per call.
        // This excludes tree traversal, row serialization and other snapshot fields.
        let expanded_row_clipping_ms = measure(|| {
            doc.layers
                .iter()
                .map(|layer| {
                    !layer.locked
                        && !matches!(layer.content.as_ref(), Content::Group)
                        && (layer.mask_source_id.is_some()
                            || live_mask::clipping_source(doc, &layer.id).is_some())
                })
                .collect::<Vec<_>>()
        });
        ensure!(serde_json::to_value(doc)? == before);
        ensure!(editor.selected_id() == selected);
        ensure!(editor.history.info().undo_count == 0);
        results.push(json!({"fixture":path,"layers":doc.layers.len(),
            "document_metadata_ms":document_metadata_ms,
            "clipping_candidates_ms":clipping_candidates_ms,
            "expanded_row_clipping_ms":expanded_row_clipping_ms,
            "document_unchanged":true,"selection_unchanged":true,"history_unchanged":true}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope":"Historical public metadata/can_link/clipping_source diagnostic, no rendering or UI. Query wrappers rebuild indexes per call; production snapshots share one index. Components timed separately; do not assume their sum equals a complete snapshot. Two warmups then ten captures; local stress fixtures, not upstream ports.",
            "results":results
        }))?
    );
    Ok(())
}
