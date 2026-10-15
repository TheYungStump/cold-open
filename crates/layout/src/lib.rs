//! Screenplay layout on a character grid: wrapping, pagination with
//! (MORE) and (CONT'D), a plain-text page view, and PDF export.
//!
//! Everything here follows `docs/specs/layout.md`; section numbers in
//! comments (§n) refer to it. Layout is deterministic: integer arithmetic
//! only, no system fonts, no clocks, no hash-map order.
//!
//! ```
//! use coldopen_layout::{layout, Options, ValidFormat};
//! use coldopen_screenplay::{Element, ElementKind, Script};
//!
//! let script = Script {
//!     elements: vec![Element::new(ElementKind::Action, "Rain hammers the roof.")],
//!     ..Default::default()
//! };
//! let pages = layout(&script, &ValidFormat::default(), &Options::default());
//! assert_eq!(pages.pages.len(), 1);
//! ```
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod flow;
mod fonts;
mod incremental;
mod page;
mod paginate;
pub mod pdf;
mod sentence;
#[doc(hidden)]
pub mod synthetic;
mod text;
mod title;
mod wrap;

use std::collections::BTreeMap;
use std::ops::Range;

use coldopen_screenplay::{PageFormat, Pt, Script, TitleLine};

pub use fonts::Fonts;
pub use incremental::{Incremental, UpdateStats};
pub use page::{
    AllGlyphs, BreakCause, Cluster, Coverage, Diagnostic, LineKind, Page, PlacedLine, Rule, Span,
};
pub use paginate::{Cursor, Rules};
pub use text::Look;

/// A page format that passed validation (§3.3). Layout only accepts these,
/// so it never has to cope with a format it cannot lay out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidFormat(PageFormat);

impl ValidFormat {
    /// Validates a format, returning every problem found.
    pub fn new(format: PageFormat) -> Result<ValidFormat, Vec<String>> {
        let problems = format.problems();
        if problems.is_empty() {
            Ok(ValidFormat(format))
        } else {
            Err(problems)
        }
    }

    pub fn get(&self) -> &PageFormat {
        &self.0
    }
}

/// Choices that are not part of the page format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Print scene numbers beside headings (§9.2).
    pub scene_numbers: bool,
    /// Leave out the title page even when the script has one.
    pub no_title_page: bool,
    /// Space above particular elements, by element index, replacing the
    /// usual spacing (§6). Only the .fdx path uses this.
    pub overrides: BTreeMap<usize, Pt>,
    /// A title page given as positioned paragraphs (from an .fdx file),
    /// used instead of the script's title-page keys.
    pub title_lines: Option<Vec<TitleLine>>,
    /// Line starts to use instead of wrapping, by element index, as byte
    /// offsets into the element's plain text. Only the reference
    /// comparison uses this, for lines a reference printed in a font off
    /// the grid (§13); front ends never set it.
    #[doc(hidden)]
    pub line_breaks: BTreeMap<usize, Vec<u32>>,
}

/// A script prepared for pagination: its flow and format. Pages can be laid
/// out from any cursor, which is what incremental repagination and the
/// reference comparison need.
pub struct Engine {
    flow: flow::Flow,
    format: PageFormat,
    rules: Rules,
    title: Vec<Page>,
}

impl Engine {
    pub fn new(script: &Script, format: &ValidFormat, options: &Options) -> Engine {
        let format = format.get().clone();
        let flow = flow::build(
            script,
            &flow::FlowInput {
                format: &format,
                overrides: &options.overrides,
                scene_numbers: options.scene_numbers,
                line_breaks: &options.line_breaks,
            },
            None,
        );
        let title = if options.no_title_page {
            Vec::new()
        } else if let Some(lines) = options.title_lines.as_ref().filter(|l| !l.is_empty()) {
            title::from_lines(lines, &format)
        } else if !script.title_page.is_empty() {
            title::from_fields(&script.title_page, &format)
        } else {
            Vec::new()
        };
        Engine {
            flow,
            format,
            rules: Rules::default(),
            title,
        }
    }

    /// Changes the page-break rules, for experiments.
    #[doc(hidden)]
    pub fn with_rules(mut self, rules: Rules) -> Engine {
        self.rules = rules;
        self
    }

    pub fn format(&self) -> &PageFormat {
        &self.format
    }

    fn paginator(&self) -> paginate::Paginator<'_> {
        paginate::Paginator {
            flow: &self.flow,
            format: &self.format,
            rules: self.rules,
        }
    }

    /// The canonical cursor past the last block.
    pub fn end(&self) -> Cursor {
        self.paginator().end()
    }

    /// Lays out one body page starting at `cursor`. The page has no number;
    /// [`Engine::run`] numbers pages.
    pub fn page_from(&self, cursor: Cursor) -> Page {
        let b = self.paginator().page_from(cursor);
        Page {
            number: None,
            lines: b.lines,
            cause: b.cause,
            diagnostics: b.diagnostics,
            start: b.start,
            next: b.next,
        }
    }

    /// The cursor at a byte offset in an element's plain text, or `None`
    /// when the element prints nothing.
    pub fn cursor_at(&self, element: usize, byte: u32) -> Option<Cursor> {
        for (bi, b) in self.flow.blocks.iter().enumerate() {
            for (mi, m) in b.members.iter().enumerate() {
                if m.source == Some(element) {
                    let c = Cursor {
                        block: bi as u32,
                        member: mi as u32,
                        offset: m.text.cell_at_source(byte),
                    };
                    return Some(self.paginator().canonical(c));
                }
            }
        }
        None
    }

    /// The source element and byte offset in its plain text where a cursor
    /// points, or `None` at the end or at a position with no source.
    pub fn source_of(&self, cursor: Cursor) -> Option<(usize, u32)> {
        let c = self.paginator().canonical(cursor);
        let b = self.flow.blocks.get(c.block as usize)?;
        let m = b.members.get(c.member as usize)?;
        let e = m.source?;
        let byte = m
            .text
            .cells
            .get(c.offset as usize)
            .map_or(m.text.source.len() as u32, |cell| cell.src_start);
        Some((e, byte))
    }

    /// Where each line of an element starts, as byte offsets into its plain
    /// text, when it is wrapped whole (no page breaks). Empty when the
    /// element prints nothing.
    pub fn line_starts(&self, element: usize) -> Vec<u32> {
        for b in &self.flow.blocks {
            for m in &b.members {
                if m.source == Some(element) {
                    return m
                        .lines
                        .iter()
                        .map(|l| {
                            m.text
                                .cells
                                .get(l.start as usize)
                                .map_or(m.text.source.len() as u32, |c| c.src_start)
                        })
                        .collect();
                }
            }
        }
        Vec::new()
    }

    /// The draft view, with page starts from `layout`.
    pub fn draft(&self, layout: &Layout) -> Draft {
        let starts: Vec<Cursor> = layout.pages.iter().map(|p| p.start).collect();
        self.paginator().draft(&starts)
    }

    /// Paginates the whole script.
    pub fn run(&self) -> Layout {
        let p = self.paginator();
        let mut pages = Vec::new();
        let mut metas = Vec::new();
        let mut diagnostics = self.flow.diagnostics.clone();
        let mut c = p.canonical(Cursor::START);
        let end = p.end();
        while c < end {
            let body = p.page_from(c);
            if body.next <= c {
                // Cannot happen (§7.6); stop rather than loop forever.
                diagnostics.push(no_progress());
                break;
            }
            c = body.next;
            let n = pages.len() as u32 + 1;
            let (page, meta) = number_page(&p, body, n);
            pages.push(page);
            metas.push(meta);
        }
        let mut layout = Layout {
            title: self.title.clone(),
            pages,
            diagnostics,
            spans: BTreeMap::new(),
            format: self.format.clone(),
        };
        layout.finish(&metas);
        layout
    }
}

fn no_progress() -> Diagnostic {
    Diagnostic::new("no_progress", None, "pagination stopped making progress")
}

/// What a page keeps besides what [`Page`] shows: where each element
/// landed on it, and the diagnostics pagination raised there.
#[derive(Clone, Debug, Default)]
pub(crate) struct PageMeta {
    pub spans: Vec<(usize, Pt, Pt)>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A laid-out page body as page `n`, with its page number line.
pub(crate) fn number_page(
    p: &paginate::Paginator<'_>,
    body: paginate::PageBody,
    n: u32,
) -> (Page, PageMeta) {
    let mut page = Page {
        number: Some(n),
        lines: body.lines,
        cause: body.cause,
        diagnostics: Vec::new(),
        start: body.start,
        next: body.next,
    };
    set_number_line(p.format, &mut page, n);
    (
        page,
        PageMeta {
            spans: body.spans,
            diagnostics: body.diagnostics,
        },
    )
}

/// Gives a page its number, replacing any page number line it had.
pub(crate) fn set_number_line(format: &PageFormat, page: &mut Page, n: u32) {
    page.number = Some(n);
    if page
        .lines
        .last()
        .is_some_and(|l| l.kind == LineKind::PageNumber)
    {
        page.lines.pop();
    }
    let pn = &format.page_numbers;
    if pn.visible && n >= pn.first {
        let label = format!("{n}{}", pn.suffix);
        let cells = text::build_plain(&label, false, Look::default())
            .cells
            .iter()
            .map(|c| i32::from(c.width))
            .sum::<i32>();
        let x = Pt(pn.right.0 - cells * format.pitch.0);
        let y = Pt(format.header_top.0 + format.baseline.0);
        page.lines
            .push(paginate::plain_line(&label, x, y, LineKind::PageNumber));
    }
}

/// The continuous draft view of a script (layout.md §15.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    /// Every line, with `y` its baseline measured as if the whole script
    /// were one page with the format's top margin.
    pub lines: Vec<PlacedLine>,
    /// Where each body page starts: the top of its first line, from the
    /// top of the body.
    pub page_tops: Vec<Pt>,
    /// The height of everything, from the top of the body.
    pub height: Pt,
}

/// A paginated script.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// Title pages, unnumbered and not counted; empty when there is none.
    /// A title that does not fit continues on further pages (§9.3).
    pub title: Vec<Page>,
    /// Body pages; page `n` is `pages[n - 1]`.
    pub pages: Vec<Page>,
    /// Diagnostics about the script as a whole (page diagnostics are on
    /// each page).
    pub diagnostics: Vec<Diagnostic>,
    spans: BTreeMap<usize, Vec<Span>>,
    format: PageFormat,
}

impl Layout {
    pub fn format(&self) -> &PageFormat {
        &self.format
    }

    /// Collects where elements landed and shows each script diagnostic on
    /// the page where its element starts.
    pub(crate) fn finish(&mut self, metas: &[PageMeta]) {
        let mut spans: BTreeMap<usize, Vec<Span>> = BTreeMap::new();
        for (k, m) in metas.iter().enumerate() {
            let n = k as u32 + 1;
            for (e, top, bottom) in &m.spans {
                let v = spans.entry(*e).or_default();
                match v.last_mut() {
                    Some(s) if s.page == n => {
                        s.top = s.top.min(*top);
                        s.bottom = s.bottom.max(*bottom);
                    }
                    _ => v.push(Span {
                        page: n,
                        top: *top,
                        bottom: *bottom,
                    }),
                }
            }
        }
        for (page, m) in self.pages.iter_mut().zip(metas) {
            page.diagnostics.clone_from(&m.diagnostics);
        }
        for d in &self.diagnostics {
            let page = d
                .element
                .and_then(|e| spans.get(&e))
                .and_then(|v| v.first())
                .map_or(1, |s| s.page);
            if let Some(p) = self.pages.get_mut(page as usize - 1) {
                p.diagnostics.push(d.clone());
            }
        }
        for p in &mut self.pages {
            p.diagnostics.sort();
        }
        self.spans = spans;
    }

    /// The plain-text page view (§11).
    pub fn page_view(&self, coverage: &dyn Coverage) -> String {
        page::page_view(&self.title, &self.pages, &self.format, coverage)
    }

    /// Where an element landed, page by page.
    pub fn spans(&self, element: usize) -> &[Span] {
        self.spans.get(&element).map_or(&[], Vec::as_slice)
    }

    /// The body page an element starts on.
    pub fn first_page(&self, element: usize) -> Option<u32> {
        self.spans(element).first().map(|s| s.page)
    }

    /// Whole lines in the body.
    pub fn body_lines(&self) -> u32 {
        let lh = self.format.line_height.0.max(1);
        u32::try_from(self.format.body_height().0 / lh)
            .unwrap_or(1)
            .max(1)
    }

    /// The length in eighths of a page of the elements in `range`, such as a
    /// scene (§10). Zero when nothing in the range prints.
    pub fn eighths(&self, range: Range<usize>) -> u32 {
        let lh = i64::from(self.format.line_height.0.max(1));
        let mut per_page: BTreeMap<u32, (Pt, Pt)> = BTreeMap::new();
        for e in range {
            for s in self.spans(e) {
                let v = per_page.entry(s.page).or_insert((s.top, s.bottom));
                v.0 = v.0.min(s.top);
                v.1 = v.1.max(s.bottom);
            }
        }
        if per_page.is_empty() {
            return 0;
        }
        let rows: i64 = per_page
            .values()
            .map(|(t, b)| (i64::from(b.0 - t.0) + lh - 1) / lh)
            .sum();
        let body = i64::from(self.body_lines());
        let eighths = (rows * 8 + body - 1) / body;
        u32::try_from(eighths.max(1)).unwrap_or(u32::MAX)
    }
}

/// Lays out a script.
pub fn layout(script: &Script, format: &ValidFormat, options: &Options) -> Layout {
    Engine::new(script, format, options).run()
}
