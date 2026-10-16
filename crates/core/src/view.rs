//! The document as laid out for reading and editing: the form its format
//! lays it out in (for Fountain, the normal form), where that text came
//! from, and its pages, kept up to date incrementally as the document
//! changes (docs/specs/layout.md §15).

use coldopen_fountain::SourceMap;
use coldopen_layout::{Draft, Incremental, Layout, Options, UpdateStats, ValidFormat};
use coldopen_screenplay::Script;

use crate::document::Document;
use crate::format::{FormatId, LayoutForm};

/// A laid-out document.
pub struct View {
    /// The document revision this view shows (`Session::rev`).
    pub rev: u64,
    /// The script that is laid out: for Fountain, the normal form
    /// (layout.md §4.8).
    pub norm: Script,
    /// For each normal-form element, the block index it starts in.
    pub map: Vec<usize>,
    /// Where each normal-form element's text came from in the blocks.
    pub sources: SourceMap,
    format: FormatId,
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
    /// Lays out a Fountain document.
    pub fn new(doc: &Document, rev: u64) -> View {
        View::with_format(doc, rev, FormatId::Fountain)
    }

    /// Lays out a document in the form its format gives it.
    pub fn with_format(doc: &Document, rev: u64, format: FormatId) -> View {
        let LayoutForm {
            script: norm,
            sources,
        } = format.get().layout_form(doc);
        let map = sources.elements.iter().map(|e| e.source).collect();
        let inc = Incremental::new(&norm, &ValidFormat::default(), &Options::default());
        View {
            rev,
            norm,
            map,
            sources,
            format,
            inc,
        }
    }

    /// Lays out the document again after a change, reusing what it can.
    pub fn refresh(&mut self, doc: &Document, rev: u64) {
        let LayoutForm {
            script: norm,
            sources,
        } = self.format.get().layout_form(doc);
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
