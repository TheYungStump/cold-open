//! The document as laid out for reading and editing: its Fountain normal
//! form, where that text came from, and its pages, kept up to date
//! incrementally as the document changes (docs/specs/layout.md §15).

use coldopen_fountain::SourceMap;
use coldopen_layout::{Draft, Incremental, Layout, Options, UpdateStats, ValidFormat};
use coldopen_screenplay::Script;

use crate::document::Document;

/// A laid-out document.
pub struct View {
    /// The document revision this view shows (`Session::rev`).
    pub rev: u64,
    /// The Fountain normal form that is laid out (layout.md §4.8).
    pub norm: Script,
    /// For each normal-form element, the block index it starts in.
    pub map: Vec<usize>,
    /// Where each normal-form element's text came from in the blocks.
    pub sources: SourceMap,
    inc: Incremental,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View")
            .field("rev", &self.rev)
            .field("pages", &self.inc.layout().pages.len())
            .finish()
    }
}

impl View {
    pub fn new(doc: &Document, rev: u64) -> View {
        let (norm, sources) = coldopen_fountain::normalize_with_source_map(&doc.to_script());
        let map = sources.elements.iter().map(|e| e.source).collect();
        let inc = Incremental::new(&norm, &ValidFormat::default(), &Options::default());
        View {
            rev,
            norm,
            map,
            sources,
            inc,
        }
    }

    /// Lays out the document again after a change, reusing what it can.
    pub fn refresh(&mut self, doc: &Document, rev: u64) {
        let (norm, sources) = coldopen_fountain::normalize_with_source_map(&doc.to_script());
        self.map = sources.elements.iter().map(|e| e.source).collect();
        self.inc.update(&norm);
        self.norm = norm;
        self.sources = sources;
        self.rev = rev;
    }

    pub fn layout(&self) -> &Layout {
        self.inc.layout()
    }

    /// The continuous draft view.
    pub fn draft(&self) -> Draft {
        self.inc.draft()
    }

    /// What the last refresh laid out again.
    pub fn stats(&self) -> &UpdateStats {
        self.inc.stats()
    }
}
