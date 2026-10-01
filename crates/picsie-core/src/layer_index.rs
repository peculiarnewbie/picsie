//! Indexed hierarchy and live-mask queries, adapted from Compositor's LayerHierarchy
//! and LiveMaskGraph (609dbeae, MIT © 2026 Wonder Assembly LLC).
//! The index borrows one document; queries never clone raster-bearing layer records.
use crate::model::{Content, Document, Layer};
use std::collections::{HashMap, VecDeque};

pub struct LayerIndex<'a> {
    pub document: &'a Document,
    ids: HashMap<&'a str, usize>,
    children: HashMap<Option<&'a str>, Vec<usize>>,
    below: Vec<Option<usize>>,
    sources: Vec<Option<usize>>,
    heights: Vec<usize>,
    graph_valid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Literal compact-record graph walk from pinned LiveMaskGraph.validate,
    // with Picsie's version gate; independent of the indexed implementation.
    fn reference_valid(doc: &Document) -> bool {
        let records: HashMap<_, _> = doc.layers.iter().map(|l| (l.id.as_str(), l)).collect();
        if records.len() != doc.layers.len() {
            return false;
        }
        for layer in &doc.layers {
            let mut path = HashSet::new();
            let mut current = Some(layer.id.as_str());
            while let Some(id) = current {
                if path.len() == 256 || !path.insert(id) {
                    return false;
                }
                let Some(layer) = records.get(id) else {
                    return false;
                };
                if let Some(source) = layer.mask_source_id.as_deref() {
                    if doc.version < 2
                        || matches!(layer.content.as_ref(), Content::Group)
                        || records
                            .get(source)
                            .is_none_or(|l| matches!(l.content.as_ref(), Content::Group))
                    {
                        return false;
                    }
                }
                current = layer.mask_source_id.as_deref();
            }
        }
        true
    }

    #[test]
    fn indexed_graph_matches_pinned_walk_and_replacement_semantics() {
        let mut doc = Document::new("Graph", 8, 8).unwrap();
        for i in 0..18 {
            let mut layer = Layer::new(
                &format!("Layer {i}"),
                8,
                8,
                if i == 17 {
                    Content::Group
                } else {
                    Content::Paint
                },
            );
            layer.id = i.to_string();
            doc.layers.push(layer);
        }
        let mut random = 1729u64;
        for _ in 0..60 {
            doc.version = 2;
            for (i, layer) in doc.layers.iter_mut().enumerate() {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                layer.mask_source_id = match random % 5 {
                    0 => Some("missing".into()),
                    1 => Some(((random >> 32) % 18).to_string()),
                    2 if i > 0 => Some((i - 1).to_string()),
                    _ => None,
                };
                layer.locked = random % 11 == 0;
            }
            let index = LayerIndex::new(&doc);
            assert_eq!(index.valid_live_masks(), reference_valid(&doc));
            for source in &doc.layers {
                for target in &doc.layers {
                    let mut next = doc.clone();
                    next.layers
                        .iter_mut()
                        .find(|l| l.id == target.id)
                        .unwrap()
                        .mask_source_id = Some(source.id.clone());
                    let expected = !target.locked && reference_valid(&next);
                    assert_eq!(
                        index.can_link(&source.id, &target.id),
                        expected,
                        "source {}, target {}",
                        source.id,
                        target.id
                    );
                }
            }
        }
    }

    #[test]
    fn replacement_counts_incoming_dependents_against_256_node_limit() {
        let mut doc = Document::new("Depth", 1, 1).unwrap();
        doc.version = 2;
        for i in 0..257 {
            let mut layer = Layer::new("Node", 1, 1, Content::Paint);
            layer.id = i.to_string();
            if (1..256).contains(&i) {
                layer.mask_source_id = Some((i - 1).to_string());
            }
            doc.layers.push(layer);
        }
        let index = LayerIndex::new(&doc);
        assert!(index.valid_live_masks());
        assert!(
            !index.can_link("256", "0"),
            "would lengthen every dependent path to 257"
        );
        assert!(index.can_link("256", "1"));
        assert!(!index.can_link("255", "0"), "cycle");
        doc.layers[256].mask_source_id = Some("255".into());
        assert!(!LayerIndex::new(&doc).valid_live_masks());
        assert!(
            LayerIndex::new(&doc).can_link("0", "256"),
            "replacement repairs excessive depth"
        );
    }
}

impl<'a> LayerIndex<'a> {
    pub fn new(document: &'a Document) -> Self {
        let n = document.layers.len();
        let ids: HashMap<_, _> = document
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.as_str(), i))
            .collect();
        let mut children: HashMap<_, Vec<usize>> = HashMap::new();
        let mut below = vec![None; n];
        let mut sources = vec![None; n];
        let mut incoming = vec![0; n];
        let mut graph_valid = ids.len() == n;
        for (i, layer) in document.layers.iter().enumerate() {
            let siblings = children.entry(layer.parent_id.as_deref()).or_default();
            below[i] = siblings.last().copied();
            siblings.push(i);
            if let Some(source) = layer.mask_source_id.as_deref() {
                sources[i] = ids.get(source).copied();
                graph_valid &= document.version >= 2
                    && !matches!(layer.content.as_ref(), Content::Group)
                    && sources[i].is_some_and(|j| {
                        !matches!(document.layers[j].content.as_ref(), Content::Group)
                    });
                if let Some(j) = sources[i] {
                    incoming[j] += 1;
                }
            }
        }
        // A functional graph is a forest iff removing its leaves consumes every
        // node. Accumulate the longest incoming path at the same time. This is
        // the depth budget for replacing a target's edge, including its dependents.
        let mut heights = vec![1usize; n];
        let mut pending: VecDeque<_> = incoming
            .iter()
            .enumerate()
            .filter_map(|(i, &count)| (count == 0).then_some(i))
            .collect();
        let mut consumed = 0;
        while let Some(i) = pending.pop_front() {
            consumed += 1;
            if let Some(j) = sources[i] {
                heights[j] = heights[j].max(heights[i] + 1);
                incoming[j] -= 1;
                if incoming[j] == 0 {
                    pending.push_back(j);
                }
            }
        }
        graph_valid &= consumed == n && heights.iter().all(|&height| height <= 256);
        Self {
            document,
            ids,
            children,
            below,
            sources,
            heights,
            graph_valid,
        }
    }

    pub fn layer(&self, id: &str) -> Option<&'a Layer> {
        self.ids.get(id).map(|&i| &self.document.layers[i])
    }

    pub fn siblings(&self, parent: Option<&str>) -> &[usize] {
        let parent = match parent {
            Some(id) => {
                let Some(&i) = self.ids.get(id) else {
                    return &[];
                };
                Some(self.document.layers[i].id.as_str())
            }
            None => None,
        };
        self.children.get(&parent).map_or(&[], Vec::as_slice)
    }

    pub fn ordered_layers(&self) -> Vec<&'a Layer> {
        fn visit<'a>(index: &LayerIndex<'a>, parent: Option<&str>, out: &mut Vec<&'a Layer>) {
            for &i in index.siblings(parent) {
                let layer = &index.document.layers[i];
                out.push(layer);
                if matches!(layer.content.as_ref(), Content::Group) {
                    visit(index, Some(&layer.id), out);
                }
            }
        }
        let mut out = Vec::with_capacity(self.document.layers.len());
        visit(self, None, &mut out);
        out
    }

    pub fn effective(&self, layer: &Layer) -> (bool, f64) {
        let mut result = (layer.visible, layer.opacity);
        let mut parent = layer.parent_id.as_deref();
        while let Some(folder) = parent.and_then(|id| self.layer(id)) {
            result.0 &= folder.visible;
            result.1 *= folder.opacity;
            parent = folder.parent_id.as_deref();
        }
        result
    }

    pub fn valid_live_masks(&self) -> bool {
        self.graph_valid
    }

    pub fn can_link(&self, source: &str, target: &str) -> bool {
        let (Some(&source), Some(&target)) = (self.ids.get(source), self.ids.get(target)) else {
            return false;
        };
        let layers = &self.document.layers;
        if source == target
            || layers[target].locked
            || matches!(layers[source].content.as_ref(), Content::Group)
            || matches!(layers[target].content.as_ref(), Content::Group)
        {
            return false;
        }
        if !self.graph_valid {
            // Preserve replacement semantics even for a caller inspecting an
            // invalid graph: replacing this edge may repair it. No document copy.
            return self.validate_replacement(source, target);
        }
        let mut current = Some(source);
        let mut depth = self.heights[target];
        while let Some(i) = current {
            depth += 1;
            if i == target || depth > 256 {
                return false;
            }
            current = self.sources[i];
        }
        true
    }

    fn validate_replacement(&self, source: usize, target: usize) -> bool {
        if self.ids.len() != self.document.layers.len() {
            return false;
        }
        for start in 0..self.document.layers.len() {
            let mut path = Vec::new();
            let mut current = Some(start);
            while let Some(i) = current {
                if path.len() == 256 || path.contains(&i) {
                    return false;
                }
                path.push(i);
                let layer = &self.document.layers[i];
                current = if i == target {
                    Some(source)
                } else {
                    if layer.mask_source_id.is_some() && self.sources[i].is_none() {
                        return false;
                    }
                    self.sources[i]
                };
                if let Some(j) = current
                    && (matches!(layer.content.as_ref(), Content::Group)
                        || matches!(self.document.layers[j].content.as_ref(), Content::Group))
                {
                    return false;
                }
            }
        }
        true
    }

    pub fn clipping_source(&self, target: &str) -> Option<&'a str> {
        let &i = self.ids.get(target)?;
        let layer = &self.document.layers[i];
        if layer.locked || matches!(layer.content.as_ref(), Content::Group) {
            return None;
        }
        let below = &self.document.layers[self.below[i]?];
        if matches!(below.content.as_ref(), Content::Group) {
            return None;
        }
        let source = below.mask_source_id.as_deref().unwrap_or(&below.id);
        self.can_link(source, target).then_some(source)
    }
}
