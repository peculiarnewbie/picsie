//! LiveLayerMask.swift and LiveMaskGraph, pinned Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Commands use Picsie's multi-selection history.
use crate::{
    layer_index::LayerIndex,
    model::{Content, Document},
};
use anyhow::{Result, ensure};
use std::collections::{HashMap, HashSet};

pub fn validate(doc: &Document) -> Result<()> {
    ensure!(
        LayerIndex::new(doc).valid_live_masks(),
        "Invalid live mask graph, source, version or depth"
    );
    Ok(())
}
pub fn can_link(doc: &Document, source: &str, target: &str) -> bool {
    LayerIndex::new(doc).can_link(source, target)
}
pub fn clipping_source(doc: &Document, target: &str) -> Option<String> {
    LayerIndex::new(doc)
        .clipping_source(target)
        .map(str::to_owned)
}
pub fn release(doc: &mut Document, target: &str) {
    let Some(layer) = doc.layers.iter().find(|l| l.id == target) else {
        return;
    };
    let Some(source) = layer.mask_source_id.clone() else {
        return;
    };
    let parent = layer.parent_id.clone();
    let ids: HashSet<_> = doc
        .layers
        .iter()
        .filter(|l| l.parent_id == parent)
        .skip_while(|l| l.id != target)
        .take_while(|l| l.id == target || l.mask_source_id.as_ref() == Some(&source))
        .map(|l| l.id.clone())
        .collect();
    for layer in &mut doc.layers {
        if ids.contains(&layer.id) {
            layer.mask_source_id = None;
        }
    }
}
pub fn adopt(doc: &mut Document, target: &str) {
    let Some(layer) = doc.layers.iter().find(|l| l.id == target) else {
        return;
    };
    if matches!(layer.content.as_ref(), Content::Group) {
        return;
    }
    let siblings: Vec<_> = doc
        .layers
        .iter()
        .filter(|l| l.parent_id == layer.parent_id)
        .collect();
    let Some(at) = siblings.iter().position(|l| l.id == target) else {
        return;
    };
    if at == 0 || at + 1 >= siblings.len() {
        return;
    }
    let Some(source) = siblings[at + 1].mask_source_id.clone() else {
        return;
    };
    let below = siblings[at - 1];
    if source != target
        && (below.id == source || below.mask_source_id.as_ref() == Some(&source))
        && can_link(doc, &source, target)
    {
        doc.layers
            .iter_mut()
            .find(|l| l.id == target)
            .unwrap()
            .mask_source_id = Some(source);
    }
}
pub fn release_detached(doc: &mut Document) {
    let mut bases: HashMap<Option<String>, Option<String>> = HashMap::new();
    for layer in &mut doc.layers {
        let base = bases.entry(layer.parent_id.clone()).or_default();
        if let Some(source) = &layer.mask_source_id {
            if Some(source) != base.as_ref() {
                layer.mask_source_id = None;
                *base = Some(layer.id.clone());
            }
        } else {
            *base = (!matches!(layer.content.as_ref(), Content::Group)).then(|| layer.id.clone());
        }
    }
}
