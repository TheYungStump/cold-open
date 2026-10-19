//! Where text shows on the page and which text is where: the bridge
//! between the editor's caret (block id and character offset) and the
//! laid-out lines (docs/specs/editor.md §4).
//!
//! The caret lives in the document's blocks; the lines come from the
//! Fountain normal form. The normal form's source map
//! (docs/specs/fountain.md §10) joins the two.

use coldopen_core::{BlockId, Document, View};
use coldopen_layout::{Draft, LineKind, PlacedLine};
use coldopen_screenplay::{Align, ElementKind, Pt};

/// A caret position: a block and a character offset in its printed text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Caret {
    pub block: BlockId,
    pub offset: usize,
}

/// Where a caret shows: a page (index into the body pages, or 0 in the
/// draft view), the x of its left edge from the paper's left edge, and the
/// top of its line from the paper's top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spot {
    pub page: usize,
    pub x: Pt,
    pub top: Pt,
}

/// The surface the caret moves on: the pages, or the draft view.
pub enum Surface<'a> {
    Pages(&'a View),
    Draft(&'a View, &'a DraftIndex),
}

/// The draft view's lines, indexed by element.
#[derive(Debug, Default)]
pub struct DraftIndex {
    pub draft: Option<Draft>,
    /// Line indices of each element, in order.
    by_elem: Vec<Vec<usize>>,
}

impl DraftIndex {
    pub fn new(draft: Draft, elements: usize) -> DraftIndex {
        let mut by_elem = vec![Vec::new(); elements];
        for (i, l) in draft.lines.iter().enumerate() {
            if let Some((e, _, _)) = l.source
                && let Some(v) = by_elem.get_mut(e)
            {
                v.push(i);
            }
        }
        DraftIndex {
            draft: Some(draft),
            by_elem,
        }
    }
}

fn is_content(l: &PlacedLine) -> bool {
    matches!(l.kind, LineKind::Content(_)) && l.source.is_some()
}

/// The byte offset of character `offset` in `s`.
pub fn byte_of(s: &str, offset: usize) -> usize {
    s.char_indices().nth(offset).map_or(s.len(), |(b, _)| b)
}

/// The character offset of byte `byte` in `s` (rounded down to a char).
pub fn char_of(s: &str, byte: usize) -> usize {
    let mut b = byte.min(s.len());
    while !s.is_char_boundary(b) {
        b -= 1;
    }
    s[..b].chars().count()
}

impl<'a> Surface<'a> {
    fn view(&self) -> &'a View {
        match self {
            Surface::Pages(v) | Surface::Draft(v, _) => v,
        }
    }

    fn pitch(&self) -> Pt {
        self.view().layout().format().pitch
    }

    pub fn line_height(&self) -> Pt {
        self.view().layout().format().line_height
    }

    fn baseline(&self) -> Pt {
        self.view().layout().format().baseline
    }

    /// The lines of a laid-out element, with their pages, in order.
    fn lines_of(&self, elem: usize) -> Vec<(usize, &'a PlacedLine)> {
        match self {
            Surface::Pages(v) => {
                let layout = v.layout();
                let mut out = Vec::new();
                let mut seen = Vec::new();
                for s in layout.spans(elem) {
                    let p = s.page as usize - 1;
                    if seen.contains(&p) {
                        continue;
                    }
                    seen.push(p);
                    if let Some(page) = layout.pages.get(p) {
                        for l in &page.lines {
                            if is_content(l) && l.source.is_some_and(|(e, _, _)| e == elem) {
                                out.push((p, l));
                            }
                        }
                    }
                }
                out
            }
            Surface::Draft(_, d) => {
                let Some(draft) = &d.draft else {
                    return Vec::new();
                };
                d.by_elem
                    .get(elem)
                    .map(|v| v.iter().map(|&i| (0, &draft.lines[i])).collect())
                    .unwrap_or_default()
            }
        }
    }

    /// The content lines on a page, top to bottom.
    pub fn lines_on(&self, page: usize) -> Vec<&'a PlacedLine> {
        match self {
            Surface::Pages(v) => v
                .layout()
                .pages
                .get(page)
                .map(|p| p.lines.iter().filter(|l| is_content(l)).collect())
                .unwrap_or_default(),
            Surface::Draft(_, d) => d
                .draft
                .as_ref()
                .map(|dr| dr.lines.iter().filter(|l| is_content(l)).collect())
                .unwrap_or_default(),
        }
    }

    pub fn pages(&self) -> usize {
        match self {
            Surface::Pages(v) => v.layout().pages.len(),
            Surface::Draft(..) => 1,
        }
    }

    fn top_of(&self, l: &PlacedLine) -> Pt {
        Pt(l.y.0 - self.baseline().0)
    }

    /// Where a caret shows.
    pub fn spot(&self, doc: &Document, caret: Caret) -> Option<Spot> {
        let view = self.view();
        let bi = doc.index_of(caret.block)?;
        // The pages show pending deletions, so the source map counts bytes
        // of the text as shown (editor.md §4).
        let rich = &doc.blocks[bi].element.text;
        let text = rich.shown().plain_text();
        let byte = rich.shown_byte(caret.offset);
        let Some((elem, nbyte)) = view.sources.to_norm(bi, byte as u32) else {
            return self.fallback_spot(doc, bi);
        };
        // Whitespace the normal form trimmed shows as extra cells after the
        // text before it (editor.md §4).
        let extra = match view.sources.to_source(elem, nbyte) {
            Some((b, back)) if b == bi && (back as usize) < byte && byte <= text.len() => {
                text[back as usize..byte].chars().count() as i32
            }
            _ => 0,
        };
        let lines = self.lines_of(elem);
        if lines.is_empty() {
            return self.fallback_spot(doc, bi);
        }
        let (page, line) = lines
            .iter()
            .rev()
            .find(|(_, l)| l.source.is_some_and(|(_, s, _)| s <= nbyte))
            .copied()
            .unwrap_or(lines[0]);
        let mut cells = 0i32;
        for c in &line.clusters {
            if c.src >= nbyte {
                break;
            }
            cells += i32::from(c.cells);
        }
        Some(Spot {
            page,
            x: Pt(line.x.0 + (cells + extra) * self.pitch().0),
            top: self.top_of(line),
        })
    }

    /// For a block with no lines (an empty block, or one holding only
    /// spaces, both of which layout drops, or one that never prints): where
    /// its first line will go. That is below the nearest block before it
    /// that has lines, after its kind's usual space above (none for
    /// dialogue and parentheticals, which sit right under the line before),
    /// at its kind's column: the left edge plus any spaces typed, the right
    /// edge for a right-aligned kind such as a transition, or the middle.
    fn fallback_spot(&self, doc: &Document, bi: usize) -> Option<Spot> {
        let block = &doc.blocks[bi].element;
        let format = self.view().layout().format();
        let text = block.text.plain_text();
        let spaces = if text.trim().is_empty() {
            text.chars().count() as i32
        } else {
            0
        };
        let typed = spaces * self.pitch().0;
        let (x, usual_space) = match format.elements.get(block.kind) {
            Some(f) => {
                let x = match f.align {
                    Align::Left => f.left.0 + f.first_indent.0 + typed,
                    Align::Right => f.right.0,
                    Align::Center => (f.left.0 + f.right.0) / 2,
                };
                (Pt(x), f.space_before.0)
            }
            None => (Pt(format.elements.action.left.0 + typed), 0),
        };
        for prev in (0..bi).rev() {
            let len = doc.blocks[prev].element.text.shown_len();
            if let Some((elem, _)) = self.view().sources.to_norm(prev, len as u32)
                && let Some(&(page, last)) = self.lines_of(elem).last()
            {
                // The space above it, as layout gives it (layout.md §6):
                // none inside a speech, else the block's own or the usual.
                let prev_kind = doc.blocks[prev].element.kind;
                let space = match block.space_before {
                    Some(n) => i32::from(n) * self.line_height().0,
                    None if block.kind.default_gap(Some(prev_kind)) == 0 => 0,
                    None => usual_space,
                };
                return Some(Spot {
                    page,
                    x,
                    top: Pt(self.top_of(last).0 + self.line_height().0 + space),
                });
            }
        }
        Some(Spot {
            page: 0,
            x,
            top: format.margin_top,
        })
    }

    /// The caret nearest to a point on a page (x from the paper's left
    /// edge, y from its top).
    pub fn hit(&self, doc: &Document, page: usize, x: Pt, y: Pt) -> Option<Caret> {
        let lines = self.lines_on(page);
        let lh = self.line_height();
        let line = lines
            .iter()
            .min_by_key(|l| {
                let top = self.top_of(l);
                if y.0 < top.0 {
                    top.0 - y.0
                } else if y.0 >= top.0 + lh.0 {
                    y.0 - top.0 - lh.0 + 1
                } else {
                    0
                }
            })
            .copied()?;
        self.caret_in_line(doc, line, x)
    }

    /// The caret at x on a line.
    pub fn caret_in_line(&self, doc: &Document, line: &PlacedLine, x: Pt) -> Option<Caret> {
        let (elem, start, end) = line.source?;
        let pitch = self.pitch().0.max(1);
        let rel = x.0 - line.x.0;
        let mut cell_x = 0i32;
        let mut nbyte = end;
        for c in &line.clusters {
            let w = i32::from(c.cells) * pitch;
            if rel < cell_x + w / 2 {
                nbyte = c.src;
                break;
            }
            cell_x += w;
        }
        if line.clusters.is_empty() {
            nbyte = start;
        }
        let (bi, byte) = self.view().sources.to_source(elem, nbyte)?;
        let b = doc.blocks.get(bi)?;
        Some(Caret {
            block: b.id,
            offset: b.element.text.offset_at_shown_byte(byte as usize),
        })
    }

    /// The caret one line above or below, keeping `goal_x`.
    pub fn vertical(&self, doc: &Document, from: Spot, down: bool, goal_x: Pt) -> Option<Caret> {
        let lines = self.lines_on(from.page);
        let target = if down {
            lines
                .iter()
                .filter(|l| self.top_of(l).0 > from.top.0)
                .min_by_key(|l| self.top_of(l).0)
        } else {
            lines
                .iter()
                .filter(|l| self.top_of(l).0 < from.top.0)
                .max_by_key(|l| self.top_of(l).0)
        };
        if let Some(l) = target {
            return self.caret_in_line(doc, l, goal_x);
        }
        // Off the page: the first or last line of the next or previous one.
        let next = if down {
            from.page + 1
        } else {
            from.page.checked_sub(1)?
        };
        let lines = self.lines_on(next);
        let l = if down { lines.first() } else { lines.last() }?;
        self.caret_in_line(doc, l, goal_x)
    }

    /// The caret at the start or end of the visual line holding `spot`.
    pub fn line_edge(&self, doc: &Document, spot: Spot, end: bool) -> Option<Caret> {
        let line = self
            .lines_on(spot.page)
            .into_iter()
            .filter(|l| self.top_of(l) == spot.top)
            .min_by_key(|l| (l.x.0 - spot.x.0).abs())?;
        let x = if end {
            Pt(i32::MAX / 2)
        } else {
            Pt(i32::MIN / 2)
        };
        self.caret_in_line(doc, line, x)
    }
}

/// Kinds the caret can stand in on a surface: everything in the draft
/// view, only what prints on the pages.
pub fn editable(kind: ElementKind, draft: bool) -> bool {
    draft || (kind.is_printing())
}
