//! Laid-out pages and the plain-text page view (layout.md §11).
#![deny(clippy::float_arithmetic)]

use std::fmt;

use coldopen_screenplay::{ElementKind, PageFormat, Pt};

use crate::paginate::Cursor;
use crate::text::Look;

/// Something layout noticed and handled.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diagnostic {
    /// Stable code, such as `dual_too_tall`.
    pub code: &'static str,
    /// Index of the source element, when there is one.
    pub element: Option<usize>,
    pub message: String,
}

impl Diagnostic {
    pub(crate) fn new(code: &'static str, element: Option<usize>, message: &str) -> Diagnostic {
        Diagnostic {
            code,
            element,
            message: message.to_string(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.element {
            Some(e) => write!(f, "{}@{}", self.code, e),
            None => f.write_str(self.code),
        }
    }
}

/// A page-break rule (layout.md §7.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// 1: a heading keeps two lines after it.
    Heading,
    /// 2: a name keeps two lines of its dialogue.
    Name,
    /// 3: dialogue splits with two lines on each side.
    DialogueSplit,
    /// 4: parentheticals never end or start a page alone.
    Parenthetical,
    /// 5: action splits with two lines on each side.
    ActionSplit,
    /// 6: a transition stays with the block before it.
    Transition,
    /// Splits happen only at sentence ends.
    Sentence,
    /// The element kind never splits.
    NoSplit,
}

impl Rule {
    pub fn name(self) -> &'static str {
        match self {
            Rule::Heading => "heading",
            Rule::Name => "name",
            Rule::DialogueSplit => "dialogue_split",
            Rule::Parenthetical => "parenthetical",
            Rule::ActionSplit => "action_split",
            Rule::Transition => "transition",
            Rule::Sentence => "sentence",
            Rule::NoSplit => "no_split",
        }
    }
}

/// Why a page ended (layout.md §7.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BreakCause {
    Natural,
    Explicit,
    Fallback { level: u8, rules: Vec<Rule> },
    End,
}

impl fmt::Display for BreakCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BreakCause::Natural => f.write_str("natural"),
            BreakCause::Explicit => f.write_str("explicit"),
            BreakCause::End => f.write_str("end"),
            BreakCause::Fallback { level, rules } => {
                write!(f, "fallback{level}(")?;
                for (i, r) in rules.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    f.write_str(r.name())?;
                }
                f.write_str(")")
            }
        }
    }
}

/// What a placed line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    /// Script text.
    Content(ElementKind),
    /// The (MORE) line below dialogue split across pages.
    More,
    /// The name with the continued marker at the top of a page.
    ContdName,
    PageNumber,
    SceneNumber,
    /// A title-page line.
    Title,
}

/// One cluster of a placed line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cluster {
    /// End of the cluster's text in [`PlacedLine::text`] (bytes); it starts
    /// where the previous one ends.
    pub end: u32,
    /// Grid cells it covers.
    pub cells: u8,
    pub look: Look,
}

/// A line of text at a position on the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedLine {
    /// Left edge of the first cell, from the paper's left edge.
    pub x: Pt,
    /// Baseline, from the top of the paper.
    pub y: Pt,
    pub kind: LineKind,
    /// The source element and the byte range of its plain text shown here.
    pub source: Option<(usize, u32, u32)>,
    pub text: String,
    pub clusters: Vec<Cluster>,
}

impl PlacedLine {
    /// Clusters with their text and starting cell, in order.
    pub fn cluster_iter(&self) -> impl Iterator<Item = (&str, u32, &Cluster)> {
        let mut start = 0u32;
        let mut cell = 0u32;
        self.clusters.iter().map(move |c| {
            let s = self.text.get(start as usize..c.end as usize).unwrap_or("");
            let at = cell;
            start = c.end;
            cell += u32::from(c.cells);
            (s, at, c)
        })
    }

    /// Cells the line covers.
    pub fn cells(&self) -> u32 {
        self.clusters.iter().map(|c| u32::from(c.cells)).sum()
    }
}

/// One laid-out page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// Body page number, counting from 1 after the title page; `None` for
    /// the title page.
    pub number: Option<u32>,
    pub lines: Vec<PlacedLine>,
    pub cause: BreakCause,
    pub diagnostics: Vec<Diagnostic>,
    /// Where the page starts and where the next one starts.
    pub start: Cursor,
    pub next: Cursor,
}

/// Placement of part of an element on a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Body page number.
    pub page: u32,
    /// Top of the element's space (or first line) on this page, from the
    /// body top.
    pub top: Pt,
    /// Bottom of its last line on this page.
    pub bottom: Pt,
}

/// Decides whether a character has a glyph in the bundled font, for the
/// missing-glyph rule (layout.md §11, §12).
pub trait Coverage {
    fn has_glyph(&self, c: char, look: Look) -> bool;
}

/// Coverage that accepts everything.
pub struct AllGlyphs;

impl Coverage for AllGlyphs {
    fn has_glyph(&self, _c: char, _look: Look) -> bool {
        true
    }
}

/// How a cluster prints: its characters with the cell each starts in, or
/// `None` when the base has no glyph (it prints as U+FFFD per cell).
pub(crate) fn resolve_cluster(
    text: &str,
    look: Look,
    cov: &dyn Coverage,
) -> Option<Vec<(char, u32)>> {
    let mut out = Vec::new();
    let mut cell = 0u32;
    let mut first = true;
    for c in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u32;
        let present = cov.has_glyph(c, look);
        if first {
            if !present {
                return None;
            }
            first = false;
            out.push((c, 0));
            // A zero-width first character takes a cell of its own only
            // when nothing wide joins it, so a joined character starts at
            // cell 0, inside the cluster.
            cell = w;
            continue;
        }
        if w == 0 {
            // A mark sits on the previous character; a missing mark is
            // dropped.
            if present {
                let at = out.last().map_or(0, |x| x.1);
                out.push((c, at));
            }
        } else if present {
            out.push((c, cell));
            cell += w;
        } else {
            return None;
        }
    }
    Some(out)
}

/// Renders pages as the plain-text page view. `title` is printed first
/// when present.
pub fn page_view(
    title: &[Page],
    pages: &[Page],
    format: &PageFormat,
    cov: &dyn Coverage,
) -> String {
    let mut out = String::new();
    let mut physical = 0usize;
    for page in title.iter().chain(pages.iter()) {
        physical += 1;
        out.push_str(&format!("=== page {physical}"));
        match page.number {
            None => out.push_str(" · title"),
            Some(n) => out.push_str(&format!(" · body {n} · {}", page.cause)),
        }
        for d in &page.diagnostics {
            out.push_str(&format!(" · {d}"));
        }
        out.push_str(" ===\n");
        for row in grid(page, format, cov) {
            out.push_str(&row);
            out.push('\n');
        }
    }
    out
}

/// The grid rows of one page.
fn grid(page: &Page, format: &PageFormat, cov: &dyn Coverage) -> Vec<String> {
    let lh = i64::from(format.line_height.0.max(1));
    let pitch = i64::from(format.pitch.0.max(1));
    // rows[r][c] = Some(text) for a filled cell, Some("") for the second cell
    // of a wide character.
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    for line in &page.lines {
        let y = i64::from(line.y.0);
        if y < 0 {
            continue;
        }
        let r = (y / lh) as usize;
        if rows.len() <= r {
            rows.resize(r + 1, Vec::new());
        }
        for (text, cell, cl) in line.cluster_iter() {
            let x0 = i64::from(line.x.0) + i64::from(cell) * pitch;
            match resolve_cluster(text, cl.look, cov) {
                None => {
                    for k in 0..i64::from(cl.cells) {
                        put(&mut rows[r], col(x0 + k * pitch, pitch), "\u{FFFD}");
                    }
                }
                Some(chars) => {
                    for (ch, at) in chars {
                        let c = col(x0 + i64::from(at) * pitch, pitch);
                        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                        if w == 0 {
                            append(&mut rows[r], c, ch);
                        } else {
                            put(&mut rows[r], c, &ch.to_string());
                            for k in 1..w {
                                put(&mut rows[r], c + k, "");
                            }
                        }
                    }
                }
            }
        }
    }
    let mut out = rows
        .into_iter()
        .map(|cells| {
            let mut s = String::new();
            let last = cells.iter().rposition(Option::is_some).map_or(0, |p| p + 1);
            for cell in &cells[..last] {
                match cell {
                    None => s.push(' '),
                    Some(t) => s.push_str(t),
                }
            }
            s.truncate(s.trim_end_matches(' ').len());
            s
        })
        .collect::<Vec<String>>();
    // Rows run to the last non-empty one (§11): an empty line at the foot
    // of a page prints nothing, so it leaves no row.
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

/// Column of a glyph origin: round half up of `x / pitch`.
fn col(x: i64, pitch: i64) -> usize {
    let c = (2 * x + pitch).div_euclid(2 * pitch);
    usize::try_from(c.max(0)).unwrap_or(0)
}

fn put(row: &mut Vec<Option<String>>, c: usize, s: &str) {
    if row.len() <= c {
        row.resize(c + 1, None);
    }
    row[c] = Some(s.to_string());
}

fn append(row: &mut Vec<Option<String>>, c: usize, ch: char) {
    if row.len() <= c {
        row.resize(c + 1, None);
    }
    match &mut row[c] {
        Some(s) => s.push(ch),
        None => row[c] = Some(ch.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_joined_character_after_a_lone_mark_stays_in_the_cluster() {
        // The cluster is one cell wide (only `x` has a width), so `x` must
        // print at its cell 0, not one cell on.
        let r = resolve_cluster("\u{326}\u{200D}x", Look::default(), &AllGlyphs).unwrap();
        assert_eq!(r, vec![('\u{326}', 0), ('\u{200D}', 0), ('x', 0)]);
        // A lone mark keeps its own cell.
        let r = resolve_cluster("\u{326}", Look::default(), &AllGlyphs).unwrap();
        assert_eq!(r, vec![('\u{326}', 0)]);
    }
}
