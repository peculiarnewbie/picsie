//! LiveLayerMask.swift and LiveMaskGraph, pinned Compositor 609dbeae.
//! MIT © 2026 Wonder Assembly LLC. Commands use Picsie's multi-selection history.
use crate::model::{Content, Document};
use anyhow::{Result, ensure};
use std::collections::{HashMap, HashSet};

pub fn validate(doc: &Document) -> Result<()> {
    let records: HashMap<_, _> = doc.layers.iter().map(|l| (l.id.as_str(), l)).collect();
    for layer in &doc.layers {
        let mut current = Some(layer.id.as_str());
        let mut path = HashSet::new();
        while let Some(id) = current {
            ensure!(
                path.len() < 256 && path.insert(id),
                "Invalid live mask cycle or depth"
            );
            let record = records
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("Missing live mask source"))?;
            if let Some(source) = record.mask_source_id.as_deref() {
                ensure!(doc.version >= 2, "Live masks require project version 2");
                ensure!(
                    !matches!(record.content.as_ref(), Content::Group),
                    "Folders cannot have live mask links"
                );
                let source = records
                    .get(source)
                    .ok_or_else(|| anyhow::anyhow!("Missing live mask source"))?;
                ensure!(
                    !matches!(source.content.as_ref(), Content::Group),
                    "Folders cannot supply live masks"
                );
            }
            current = record.mask_source_id.as_deref();
        }
    }
    Ok(())
}
pub fn can_link(doc: &Document, source: &str, target: &str) -> bool {
    let mut next = doc.clone();
    let Some(layer) = next.layers.iter_mut().find(|l| l.id == target) else {
        return false;
    };
    if layer.locked {
        return false;
    }
    layer.mask_source_id = Some(source.to_owned());
    next.version = 2;
    validate(&next).is_ok()
}
pub fn clipping_source(doc: &Document, target: &str) -> Option<String> {
    let layer = doc.layers.iter().find(|l| l.id == target)?;
    if layer.locked || matches!(layer.content.as_ref(), Content::Group) {
        return None;
    }
    let siblings: Vec<_> = doc
        .layers
        .iter()
        .filter(|l| l.parent_id == layer.parent_id)
        .collect();
    let at = siblings.iter().position(|l| l.id == target)?;
    let below = *siblings.get(at.checked_sub(1)?)?;
    if matches!(below.content.as_ref(), Content::Group) {
        return None;
    }
    let source = below.mask_source_id.as_ref().unwrap_or(&below.id);
    can_link(doc, source, target).then(|| source.clone())
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
