//! Text as grid cells (layout.md §2): clusters with their display width,
//! style, and position in the source text.
#![deny(clippy::float_arithmetic)]

use coldopen_screenplay::{Marks, RichText, Run, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Character styles a printed cell can have, and its tracked-change mark
/// (layout.md §17).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Look {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// The cell is a pending deletion, printed struck through.
    pub strike: bool,
    /// The change that inserted or deleted the cell (the deletion's, when
    /// it has both), as its number. Whoever draws the cell gives the
    /// change's author a color; layout itself knows no authors.
    pub change: Option<u64>,
}

impl Look {
    pub(crate) fn from_style(s: Style) -> Look {
        Look {
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            ..Look::default()
        }
    }

    /// The look of a run with these marks.
    pub(crate) fn marked(self, m: Marks) -> Look {
        Look {
            strike: m.del.is_some(),
            change: m.del.or(m.ins).map(|c| c.0),
            ..self
        }
    }

    pub(crate) fn union(self, o: Look) -> Look {
        Look {
            bold: self.bold || o.bold,
            italic: self.italic || o.italic,
            underline: self.underline || o.underline,
            strike: self.strike || o.strike,
            change: self.change.or(o.change),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellKind {
    Glyph,
    Space,
    Newline,
}

/// One grid cell: a cluster, a space or a line break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    /// Byte range in [`Text::printed`].
    pub start: u32,
    pub end: u32,
    /// Byte range in [`Text::source`].
    pub src_start: u32,
    pub src_end: u32,
    /// Cells covered: 1 or 2 for glyphs and spaces, 0 for a line break.
    pub width: u8,
    pub kind: CellKind,
    pub look: Look,
}

/// A paragraph's text, ready to wrap.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Text {
    /// What prints, after capitals.
    pub printed: String,
    /// The visible source text: the element's plain text with hidden runs
    /// removed. Byte offsets in cells and cursors refer to it.
    pub source: String,
    pub cells: Vec<Cell>,
}

impl Text {
    pub(crate) fn len(&self) -> u32 {
        self.cells.len() as u32
    }

    pub(crate) fn cell_text(&self, c: &Cell) -> &str {
        self.printed
            .get(c.start as usize..c.end as usize)
            .unwrap_or("")
    }

    /// The cell index at or after a byte offset in the source.
    pub(crate) fn cell_at_source(&self, offset: u32) -> u32 {
        self.cells.partition_point(|c| c.src_start < offset) as u32
    }

    /// True when the cell is a letter or digit (for hyphen breaks).
    pub(crate) fn is_alnum(&self, i: usize) -> bool {
        self.cells.get(i).is_some_and(|c| {
            c.kind == CellKind::Glyph
                && self
                    .printed
                    .get(c.start as usize..c.end as usize)
                    .and_then(|s| s.chars().next())
                    .is_some_and(char::is_alphanumeric)
        })
    }

    pub(crate) fn is_alpha(&self, i: usize) -> bool {
        self.cells.get(i).is_some_and(|c| {
            c.kind == CellKind::Glyph
                && self
                    .printed
                    .get(c.start as usize..c.end as usize)
                    .and_then(|s| s.chars().next())
                    .is_some_and(char::is_alphabetic)
        })
    }

    pub(crate) fn is_hyphen(&self, i: usize) -> bool {
        self.cells
            .get(i)
            .is_some_and(|c| c.kind == CellKind::Glyph && self.cell_text(c) == "-")
    }
}

/// What building a text noticed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Notes {
    pub control_chars: bool,
}

struct Builder {
    text: Text,
    caps: bool,
    extra: Look,
    notes: Notes,
    /// The cluster being collected: source start, chars, look.
    cluster: Option<(u32, String, Look)>,
    after_zwj: bool,
}

impl Builder {
    fn new(caps: bool, extra: Look) -> Builder {
        Builder {
            text: Text::default(),
            caps,
            extra,
            notes: Notes::default(),
            cluster: None,
            after_zwj: false,
        }
    }

    fn push_cell(
        &mut self,
        printed: &str,
        src_start: u32,
        src_end: u32,
        width: u8,
        kind: CellKind,
        look: Look,
    ) {
        let start = self.text.printed.len() as u32;
        self.text.printed.push_str(printed);
        let end = self.text.printed.len() as u32;
        self.text.cells.push(Cell {
            start,
            end,
            src_start,
            src_end,
            width,
            kind,
            look,
        });
    }

    fn flush(&mut self) {
        if let Some((src_start, chars, look)) = self.cluster.take() {
            let src_end = src_start + chars.len() as u32;
            let printed = if self.caps {
                chars.to_uppercase()
            } else {
                chars
            };
            let width = UnicodeWidthStr::width(printed.as_str()).clamp(1, 255) as u8;
            self.push_cell(&printed, src_start, src_end, width, CellKind::Glyph, look);
        }
        self.after_zwj = false;
    }

    fn push_char(&mut self, c: char, look: Look) {
        let at = self.text.source.len() as u32;
        self.text.source.push(c);
        let look = look.union(self.extra);
        match c {
            '\n' => {
                self.flush();
                self.push_cell("\n", at, at + 1, 0, CellKind::Newline, look);
            }
            ' ' | '\t' => {
                self.flush();
                self.push_cell(" ", at, at + c.len_utf8() as u32, 1, CellKind::Space, look);
            }
            c if c.is_control() => {
                self.flush();
                self.notes.control_chars = true;
            }
            c => {
                let zero = UnicodeWidthChar::width(c).unwrap_or(0) == 0;
                let joins = self.cluster.is_some() && (zero || self.after_zwj);
                if joins {
                    if let Some((_, chars, _)) = &mut self.cluster {
                        chars.push(c);
                    }
                } else {
                    self.flush();
                    self.cluster = Some((at, c.to_string(), look));
                }
                self.after_zwj = c == '\u{200D}';
            }
        }
    }

    fn finish(mut self) -> (Text, Notes) {
        self.flush();
        (self.text, self.notes)
    }
}

/// Builds cells from rich text. Hidden runs are skipped; `caps` prints in
/// capitals; `extra` is added to every run's style.
pub(crate) fn build(rich: &RichText, caps: bool, extra: Look) -> (Text, Notes) {
    let mut b = Builder::new(caps, extra);
    for run in rich.runs() {
        // A pending deletion prints only in a print form that shows it
        // (docs/specs/layout.md §17); otherwise it is not part of the text.
        if let Run::Text { text, style, marks } = run
            && !marks.is_deleted()
        {
            let look = Look::from_style(*style).marked(*marks);
            for c in text.chars() {
                b.push_char(c, look);
            }
        }
    }
    b.finish()
}

/// Builds cells from a plain string, such as an inserted (MORE) line.
pub(crate) fn build_plain(s: &str, caps: bool, look: Look) -> Text {
    let mut b = Builder::new(caps, Look::default());
    for c in s.chars() {
        b.push_char(c, look);
    }
    b.finish().0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths(t: &Text) -> Vec<(String, u8)> {
        t.cells
            .iter()
            .map(|c| (t.cell_text(c).to_string(), c.width))
            .collect()
    }

    #[test]
    fn clusters_and_widths() {
        let (t, n) = build(
            &RichText::plain("e\u{301}x 日\u{1F469}\u{200D}\u{1F52C}"),
            false,
            Look::default(),
        );
        assert_eq!(
            widths(&t),
            vec![
                ("e\u{301}".to_string(), 1),
                ("x".to_string(), 1),
                (" ".to_string(), 1),
                ("日".to_string(), 2),
                ("\u{1F469}\u{200D}\u{1F52C}".to_string(), 2),
            ]
        );
        assert!(!n.control_chars);
    }

    #[test]
    fn smart_quotes_are_one_cell() {
        let (t, _) = build(&RichText::plain("“it’s”"), false, Look::default());
        assert!(t.cells.iter().all(|c| c.width == 1));
        assert_eq!(t.cells.len(), 6);
    }

    #[test]
    fn controls_tabs_and_newlines() {
        let (t, n) = build(&RichText::plain("a\u{7}b\tc\nd"), false, Look::default());
        let kinds: Vec<_> = t.cells.iter().map(|c| c.kind).collect();
        use CellKind::*;
        assert_eq!(kinds, vec![Glyph, Glyph, Space, Glyph, Newline, Glyph]);
        assert!(n.control_chars);
        assert_eq!(t.source, "a\u{7}b\tc\nd");
    }

    #[test]
    fn capitals_keep_source_offsets() {
        let (t, _) = build(&RichText::plain("straße"), true, Look::default());
        assert_eq!(t.printed, "STRASSE");
        let last = t.cells.last().unwrap();
        assert_eq!(t.cell_text(last), "E");
        assert_eq!(
            &t.source[last.src_start as usize..last.src_end as usize],
            "e"
        );
        let ss = t.cells.iter().find(|c| t.cell_text(c) == "SS").unwrap();
        assert_eq!(ss.width, 2);
    }

    #[test]
    fn hidden_runs_are_skipped() {
        let rich = RichText::new(vec![
            Run::plain("a"),
            Run::Note("n".into()),
            Run::styled(
                "b",
                Style {
                    bold: true,
                    italic: false,
                    underline: false,
                },
            ),
        ]);
        let (t, _) = build(&rich, false, Look::default());
        assert_eq!(t.source, "ab");
        assert!(t.cells[1].look.bold);
        assert!(!t.cells[0].look.bold);
    }

    #[test]
    fn orphan_zero_width_is_one_cell() {
        let (t, _) = build(&RichText::plain("\u{301}a"), false, Look::default());
        assert_eq!(widths(&t)[0].1, 1);
        assert_eq!(t.cells.len(), 2);
    }
}
