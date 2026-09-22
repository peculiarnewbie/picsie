//! Compositor DocumentHistory.swift, MIT © 2026 Wonder Assembly LLC.
//! Snapshots share immutable assets. Selection is extended to the existing multi-layer UI.
use crate::model::{Content, Document, id};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};
use ts_rs::TS;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Selection {
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub anchor: Option<String>,
}
#[derive(Clone)]
struct Snapshot {
    document: Document,
    selection: Selection,
    revision: String,
}
#[derive(Clone)]
struct Entry {
    label: String,
    before: Snapshot,
    after: Snapshot,
}
pub struct History {
    pub document: Document,
    past: Vec<Entry>,
    future: Vec<Entry>,
    pub revision: String,
    saved_revision: Option<String>,
    pending: Option<Snapshot>,
    pending_label: String,
    depth: usize,
    pub entry_limit: usize,
    pub byte_limit: usize,
}
#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HistoryInfo {
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: String,
    pub redo_label: String,
    pub undo_count: usize,
    pub revision: String,
}
impl History {
    pub fn new(document: Document) -> Self {
        let revision = id();
        Self {
            document,
            past: vec![],
            future: vec![],
            saved_revision: Some(revision.clone()),
            revision,
            pending: None,
            pending_label: String::new(),
            depth: 0,
            entry_limit: 100,
            byte_limit: 256 * 1024 * 1024,
        }
    }
    pub fn info(&self) -> HistoryInfo {
        HistoryInfo {
            dirty: Some(&self.revision) != self.saved_revision.as_ref(),
            can_undo: self.depth == 0 && !self.past.is_empty(),
            can_redo: self.depth == 0 && !self.future.is_empty(),
            undo_label: self
                .past
                .last()
                .map(|e| e.label.clone())
                .unwrap_or_default(),
            redo_label: self
                .future
                .last()
                .map(|e| e.label.clone())
                .unwrap_or_default(),
            undo_count: self.past.len(),
            revision: self.revision.clone(),
        }
    }
    pub fn mark_saved(&mut self, revision: String) {
        self.saved_revision = Some(revision);
    }
    pub fn begin(&mut self, label: &str, selection: Selection) {
        if self.depth == 0 {
            self.pending = Some(Snapshot {
                document: self.document.clone(),
                selection,
                revision: self.revision.clone(),
            });
            self.pending_label = label.into();
        }
        self.depth += 1;
    }
    pub fn preview(&mut self, doc: Document) {
        assert!(self.pending.is_some());
        self.document = doc;
    }
    pub fn commit(&mut self, selection: Selection) {
        if self.depth == 0 {
            return;
        }
        self.depth -= 1;
        if self.depth != 0 {
            return;
        }
        let Some(before) = self.pending.take() else {
            return;
        };
        if before.document == self.document {
            self.document = before.document;
            return;
        }
        self.revision = id();
        self.past.push(Entry {
            label: self.pending_label.clone(),
            before,
            after: Snapshot {
                document: self.document.clone(),
                selection,
                revision: self.revision.clone(),
            },
        });
        self.future.clear();
        self.trim();
    }
    pub fn cancel(&mut self) -> Option<Selection> {
        self.depth = 0;
        self.pending.take().map(|s| {
            self.document = s.document;
            self.revision = s.revision;
            s.selection
        })
    }
    pub fn undo(&mut self) -> Option<Selection> {
        if !self.info().can_undo {
            return None;
        }
        let e = self.past.pop()?;
        let s = e.before.clone();
        self.future.push(e);
        Some(self.restore(s))
    }
    pub fn redo(&mut self) -> Option<Selection> {
        if !self.info().can_redo {
            return None;
        }
        let e = self.future.pop()?;
        let s = e.after.clone();
        self.past.push(e);
        Some(self.restore(s))
    }
    fn restore(&mut self, s: Snapshot) -> Selection {
        self.document = s.document;
        self.revision = s.revision;
        self.trim();
        s.selection
    }
    pub fn retained_bytes(&self) -> usize {
        fn assets(d: &Document) -> Vec<(usize, usize)> {
            d.layers
                .iter()
                .flat_map(|l| {
                    let mut a = vec![];
                    if let Content::Image { data } = l.content.as_ref() {
                        a.push((data.as_ptr() as usize, data.len()));
                    }
                    for s in &l.strokes {
                        a.push((Arc::as_ptr(s) as usize, 64 + s.points.len() * 16));
                    }
                    if let Some(m) = &l.mask {
                        for s in &m.strokes {
                            a.push((Arc::as_ptr(s) as usize, 64 + s.points.len() * 16));
                        }
                    }
                    a
                })
                .collect()
        }
        let mut seen: HashSet<_> = assets(&self.document)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let mut bytes = 0;
        for e in self.past.iter().chain(&self.future) {
            for s in [&e.before, &e.after] {
                for (id, len) in assets(&s.document) {
                    if seen.insert(id) {
                        bytes += len;
                    }
                }
            }
        }
        bytes
    }
    fn trim(&mut self) {
        while self.past.len() + self.future.len() > self.entry_limit
            || self.retained_bytes() > self.byte_limit
        {
            if !self.past.is_empty() {
                self.past.remove(0);
            } else if !self.future.is_empty() {
                self.future.remove(0);
            } else {
                break;
            }
        }
    }
}
