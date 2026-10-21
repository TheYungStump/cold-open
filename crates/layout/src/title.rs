//! The title page (layout.md §9.3).
#![deny(clippy::float_arithmetic)]

use coldopen_screenplay::{
    Align, ElementFormat, ElementKind, PageFormat, Pt, RichText, TitleLine, TitlePage,
};

use crate::flow::Geom;
use crate::page::{BreakCause, Cluster, Diagnostic, LineKind, Page, PlacedLine};
use crate::paginate::Cursor;
use crate::text::{self, Look, Text};
use crate::wrap::{self, Line};

struct Placer<'a> {
    format: &'a PageFormat,
    /// Lines of each title page; row `r` of the title is row `r % rows`
    /// of page `r / rows` (layout.md §9.3).
    pages: Vec<Vec<PlacedLine>>,
    overflow: bool,
}

impl<'a> Placer<'a> {
    fn new(format: &'a PageFormat) -> Self {
        Placer {
            format,
            pages: vec![Vec::new()],
            overflow: false,
        }
    }

    fn rows(&self) -> i32 {
        let lh = self.format.line_height.0.max(1);
        self.format.body_height().0 / lh
    }

    /// Places `t`'s wrapped lines starting at body row `row`; returns the
    /// row after the last one.
    fn put(&mut self, t: &Text, g: &Geom, row: i32) -> i32 {
        let ls = wrap::wrap(t, 0, true, g.measure);
        let mut r = row;
        for l in &ls {
            self.put_line(t, l, g, r);
            r = r.saturating_add(1);
        }
        r
    }

    fn put_line(&mut self, t: &Text, l: &Line, g: &Geom, row: i32) {
        if row < 0 {
            self.overflow = true;
            return;
        }
        let cells = t.cells.get(l.start as usize..l.end as usize).unwrap_or(&[]);
        if cells.is_empty() {
            // An empty line takes its row but prints nothing, so it never
            // starts a page of its own.
            return;
        }
        let rows = self.rows().max(1);
        let (page, row) = ((row / rows) as usize, row % rows);
        if self.pages.len() <= page {
            self.pages.resize_with(page + 1, Vec::new);
        }
        let lh = self.format.line_height.0;
        let y = Pt(self.format.margin_top.0 + row * lh + self.format.baseline.0);
        let mut text = String::new();
        let mut clusters = Vec::new();
        for c in cells {
            text.push_str(t.cell_text(c));
            clusters.push(Cluster {
                end: text.len() as u32,
                cells: c.width,
                look: c.look,
                src: 0,
            });
        }
        self.pages[page].push(PlacedLine {
            x: g.line_x(l),
            y,
            kind: LineKind::Title,
            source: None,
            text,
            clusters,
        });
    }
}

fn geom(format: &PageFormat, left: Pt, right: Pt, align: Align) -> Geom {
    let mut f = ElementFormat {
        left,
        right,
        first_indent: Pt::ZERO,
        align,
        ..format.elements.action.clone()
    };
    if f.right.0 <= f.left.0 {
        f.right = Pt(f.left.0 + format.pitch.0.max(1));
    }
    f.cells = None;
    f.paginate_as = ElementKind::Action;
    Geom::of(&f, format)
}

fn values(tp: &TitlePage, key: &str) -> Vec<RichText> {
    tp.fields
        .iter()
        .filter(|f| {
            let k = f.key.trim().to_lowercase();
            k == key || (key == "author" && k == "authors")
        })
        .flat_map(|f| f.values.iter().cloned())
        .collect()
}

fn finish(p: Placer<'_>) -> Vec<Page> {
    let total = p.pages.len();
    p.pages
        .into_iter()
        .enumerate()
        .map(|(i, lines)| {
            let mut diagnostics = Vec::new();
            if i == 0 && p.overflow {
                diagnostics.push(Diagnostic::new(
                    "title_overflow",
                    None,
                    "title-page lines above the top of the page were left out",
                ));
            }
            if i == 0 && total > 1 {
                diagnostics.push(Diagnostic::new(
                    "title_continued",
                    None,
                    "the title did not fit on one page and continues on the next",
                ));
            }
            Page {
                number: None,
                lines,
                cause: BreakCause::End,
                diagnostics,
                start: Cursor::START,
                next: Cursor::START,
            }
        })
        .collect()
}

/// Title pages from Fountain keys.
pub(crate) fn from_fields(tp: &TitlePage, format: &PageFormat) -> Vec<Page> {
    let a = &format.elements.action;
    let center = geom(format, a.left, a.right, Align::Center);
    let left = geom(format, a.left, a.right, Align::Left);
    let right = geom(format, a.left, a.right, Align::Right);
    let mut p = Placer::new(format);
    let mut row = p.rows() / 3;
    for v in values(tp, "title") {
        let t = text::build(&v, true, Look::default()).0;
        row = p.put(&t, &center, row);
    }
    for key in ["credit", "author", "source"] {
        let vs = values(tp, key);
        if vs.is_empty() {
            continue;
        }
        row += 1;
        for v in vs {
            let t = text::build(&v, false, Look::default()).0;
            row = p.put(&t, &center, row);
        }
    }
    // Bottom blocks end at the last body row.
    for (keys, g) in [
        (["draft date", "contact"], &left),
        (["notes", "copyright"], &right),
    ] {
        // In source order, whatever the key (§9.3).
        let texts: Vec<Text> = tp
            .fields
            .iter()
            .filter(|f| keys.contains(&f.key.trim().to_lowercase().as_str()))
            .flat_map(|f| f.values.iter())
            .map(|v| text::build(v, false, Look::default()).0)
            .collect();
        let total: i32 = texts
            .iter()
            .map(|t| wrap::wrap(t, 0, true, g.measure).len() as i32)
            .sum();
        let mut r = p.rows() - total;
        for t in &texts {
            r = p.put(t, g, r);
        }
    }
    finish(p)
}

/// Positioned title lines that print exactly as [`from_fields`] prints
/// the Fountain keys: one line per value, with its block's alignment, the
/// action column, and the rows above it as space (docs/specs/fdx.md
/// §14.3). [`from_lines`] lays them out on the same rows.
pub fn title_lines(tp: &TitlePage, format: &PageFormat) -> Vec<TitleLine> {
    let a = &format.elements.action;
    let rows = {
        let lh = format.line_height.0.max(1);
        format.body_height().0 / lh
    };
    // Each value with its alignment and the row it starts on, in the
    // order and rows `from_fields` gives them.
    let mut placed: Vec<(RichText, Align, i32)> = Vec::new();
    let height = |v: &RichText, caps: bool, align: Align| -> i32 {
        let g = geom(format, a.left, a.right, align);
        let t = text::build(v, caps, Look::default()).0;
        wrap::wrap(&t, 0, true, g.measure).len() as i32
    };
    let mut row = rows / 3;
    for v in values(tp, "title") {
        let v = upper(&v);
        let h = height(&v, false, Align::Center);
        placed.push((v, Align::Center, row));
        row += h;
    }
    for key in ["credit", "author", "source"] {
        let vs = values(tp, key);
        if vs.is_empty() {
            continue;
        }
        row += 1;
        for v in vs {
            let h = height(&v, false, Align::Center);
            placed.push((v, Align::Center, row));
            row += h;
        }
    }
    // The bottom blocks end at the last row. Stacked paragraphs cannot
    // share rows, so with both, the right one goes just above the left.
    let mut bottom = rows;
    for (keys, align) in [
        (["draft date", "contact"], Align::Left),
        (["notes", "copyright"], Align::Right),
    ] {
        let vs: Vec<RichText> = tp
            .fields
            .iter()
            .filter(|f| keys.contains(&f.key.trim().to_lowercase().as_str()))
            .flat_map(|f| f.values.iter().cloned())
            .collect();
        let total: i32 = vs.iter().map(|v| height(v, false, align)).sum();
        bottom -= total;
        let mut r = bottom;
        for v in vs {
            let h = height(&v, false, align);
            placed.push((v, align, r));
            r += h;
        }
    }
    // `from_lines` starts the first line on row 0 and moves each later one
    // down by its space; lines must also come in row order.
    placed.sort_by_key(|p| p.2);
    let lh = format.line_height.0.max(1);
    let mut out = Vec::new();
    let mut row = 0i32;
    for (text, align, at) in placed {
        if at < 0 {
            continue;
        }
        if out.is_empty() && at > 0 {
            out.push(TitleLine {
                text: RichText::empty(),
                align: Align::Left,
                left: a.left,
                right: a.right,
                space_before: Pt::ZERO,
            });
            row = 1;
        }
        let gap = (at - row).max(0);
        let h = height(&text, false, align);
        out.push(TitleLine {
            space_before: if out.is_empty() {
                Pt::ZERO
            } else {
                Pt(gap.saturating_mul(lh).min(format.body_height().0))
            },
            text,
            align,
            left: a.left,
            right: a.right,
        });
        row = row.max(at) + h;
    }
    out
}

/// Rich text in capitals, as the title prints.
fn upper(v: &RichText) -> RichText {
    RichText::new(
        v.runs()
            .iter()
            .map(|r| match r {
                coldopen_screenplay::Run::Text { text, style, marks } => {
                    coldopen_screenplay::Run::marked(text.to_uppercase(), *style, *marks)
                }
                other => other.clone(),
            })
            .collect(),
    )
}

/// Title pages from positioned paragraphs, as an .fdx file stores it.
pub(crate) fn from_lines(lines: &[TitleLine], format: &PageFormat) -> Vec<Page> {
    let mut p = Placer::new(format);
    let lh = i64::from(format.line_height.0.max(1));
    let mut row = 0i32;
    for (i, tl) in lines.iter().enumerate() {
        if i > 0 {
            // A space taller than a page moves to the next page and no
            // further, so hostile values cannot make blank pages.
            let step = (i64::from(tl.space_before.0.max(0)) + lh - 1) / lh;
            let step = step.min(i64::from(p.rows()));
            row = row.saturating_add(i32::try_from(step).unwrap_or(i32::MAX));
        }
        let g = geom(format, tl.left, tl.right, tl.align);
        let t = text::build(&tl.text, false, Look::default()).0;
        row = p.put(&t, &g, row);
    }
    finish(p)
}
