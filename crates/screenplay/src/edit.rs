//! Editing rich text by character offsets, as the keystroke commands do
//! (docs/specs/commands.md, "Positions in text").
//!
//! Offsets count Unicode scalar values of the printed text. Hidden runs
//! (notes and boneyard) take no offsets; an offset names the point just
//! after the printed character before it, so hidden runs sitting there
//! belong to the text that follows.

use std::fmt;

use crate::text::{RichText, Run, Style};

/// An offset or range that does not fit the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeError {
    /// The text's length in characters.
    pub len: usize,
    pub message: String,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RangeError {}

/// Styles to set (`Some(true)`), clear (`Some(false)`) or leave alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleChange {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
}

impl StyleChange {
    fn apply(self, mut s: Style) -> Style {
        if let Some(b) = self.bold {
            s.bold = b;
        }
        if let Some(i) = self.italic {
            s.italic = i;
        }
        if let Some(u) = self.underline {
            s.underline = u;
        }
        s
    }
}

enum Cell {
    C(char, Style),
    H(Run),
}

fn cells(rt: &RichText) -> Vec<Cell> {
    let mut out = Vec::new();
    for r in rt.runs() {
        match r {
            Run::Text { text, style } => out.extend(text.chars().map(|c| Cell::C(c, *style))),
            h => out.push(Cell::H(h.clone())),
        }
    }
    out
}

fn rebuild(cells: impl IntoIterator<Item = Cell>) -> RichText {
    let mut runs = Vec::new();
    let mut text = String::new();
    let mut style = Style::PLAIN;
    for cell in cells {
        match cell {
            Cell::C(c, s) => {
                if s != style && !text.is_empty() {
                    runs.push(Run::styled(std::mem::take(&mut text), style));
                }
                style = s;
                text.push(c);
            }
            Cell::H(h) => {
                if !text.is_empty() {
                    runs.push(Run::styled(std::mem::take(&mut text), style));
                }
                runs.push(h);
            }
        }
    }
    if !text.is_empty() {
        runs.push(Run::styled(text, style));
    }
    RichText::new(runs)
}

/// The cell index where text inserted at offset `at` goes: just after the
/// `at`-th printed character, ahead of any hidden runs after it.
fn insert_index(cells: &[Cell], at: usize) -> Option<usize> {
    if at == 0 {
        return Some(0);
    }
    let mut v = 0;
    for (i, c) in cells.iter().enumerate() {
        if let Cell::C(..) = c {
            v += 1;
            if v == at {
                return Some(i + 1);
            }
        }
    }
    None
}

impl RichText {
    /// The printed text's length in characters.
    pub fn char_len(&self) -> usize {
        self.runs()
            .iter()
            .map(|r| match r {
                Run::Text { text, .. } => text.chars().count(),
                _ => 0,
            })
            .sum()
    }

    fn check(&self, from: usize, to: usize) -> Result<usize, RangeError> {
        let len = self.char_len();
        if from > to {
            return Err(RangeError {
                len,
                message: format!("the range {from}..{to} runs backwards"),
            });
        }
        if to > len {
            return Err(RangeError {
                len,
                message: format!("offset {to} is past the end of the text ({len} characters)"),
            });
        }
        Ok(len)
    }

    /// The style new text takes at offset `at`: that of the printed
    /// character before it, or the one at it when `at` is 0, or plain.
    pub fn style_at(&self, at: usize) -> Style {
        let mut prev = None;
        let mut first = None;
        let mut v = 0;
        for r in self.runs() {
            if let Run::Text { text, style } = r {
                for _ in text.chars() {
                    if first.is_none() {
                        first = Some(*style);
                    }
                    v += 1;
                    if v == at {
                        prev = Some(*style);
                    }
                }
            }
        }
        if at == 0 {
            first.unwrap_or(Style::PLAIN)
        } else {
            prev.unwrap_or(Style::PLAIN)
        }
    }

    /// Inserts `text` at offset `at`, in `style` or the style at that point.
    pub fn insert(
        &self,
        at: usize,
        text: &str,
        style: Option<Style>,
    ) -> Result<RichText, RangeError> {
        self.check(at, at)?;
        let style = style.unwrap_or_else(|| self.style_at(at));
        let mut cs = cells(self);
        let i = insert_index(&cs, at).unwrap_or(cs.len());
        let new: Vec<Cell> = text.chars().map(|c| Cell::C(c, style)).collect();
        cs.splice(i..i, new);
        Ok(rebuild(cs))
    }

    /// Deletes the printed characters from `from` up to `to`. Hidden runs
    /// inside the range are kept, so they end up at `from`.
    pub fn delete(&self, from: usize, to: usize) -> Result<RichText, RangeError> {
        self.check(from, to)?;
        let mut v = 0;
        let kept = cells(self).into_iter().filter(|c| match c {
            Cell::C(..) => {
                let keep = v < from || v >= to;
                v += 1;
                keep
            }
            Cell::H(_) => true,
        });
        Ok(rebuild(kept))
    }

    /// Changes the styles of the printed characters from `from` up to `to`.
    pub fn restyle(
        &self,
        from: usize,
        to: usize,
        change: StyleChange,
    ) -> Result<RichText, RangeError> {
        self.check(from, to)?;
        let mut v = 0;
        let changed = cells(self).into_iter().map(|c| match c {
            Cell::C(ch, s) => {
                let s = if v >= from && v < to {
                    change.apply(s)
                } else {
                    s
                };
                v += 1;
                Cell::C(ch, s)
            }
            h => h,
        });
        Ok(rebuild(changed))
    }

    /// Splits at offset `at`. Hidden runs at the split point go with the
    /// second part, except at the very end, where they stay with the first.
    pub fn split_at(&self, at: usize) -> Result<(RichText, RichText), RangeError> {
        let len = self.check(at, at)?;
        let mut cs = cells(self);
        let i = if at == len {
            cs.len()
        } else {
            insert_index(&cs, at).unwrap_or(cs.len())
        };
        let rest = cs.split_off(i);
        Ok((rebuild(cs), rebuild(rest)))
    }

    /// This text followed by `other`, hidden runs included.
    pub fn concat(&self, other: &RichText) -> RichText {
        let mut runs = self.runs().to_vec();
        runs.extend(other.runs().iter().cloned());
        RichText::new(runs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Style = Style {
        bold: true,
        italic: false,
        underline: false,
    };

    fn note(s: &str) -> Run {
        Run::Note(s.into())
    }

    #[test]
    fn insert_takes_the_style_before() {
        let rt = RichText::new(vec![Run::plain("ab"), Run::styled("cd", B)]);
        let t = rt.insert(4, "e", None).unwrap();
        assert_eq!(t.runs(), &[Run::plain("ab"), Run::styled("cde", B)]);
        let t = rt.insert(2, "x", None).unwrap();
        assert_eq!(t.runs(), &[Run::plain("abx"), Run::styled("cd", B)]);
        let t = rt.insert(0, "x", None).unwrap();
        assert_eq!(t.plain_text(), "xabcd");
        let t = RichText::empty().insert(0, "hi", None).unwrap();
        assert_eq!(t, RichText::plain("hi"));
        let t = rt.insert(1, "Z", Some(B)).unwrap();
        assert_eq!(
            t.runs(),
            &[
                Run::plain("a"),
                Run::styled("Z", B),
                Run::plain("b"),
                Run::styled("cd", B)
            ]
        );
    }

    #[test]
    fn insert_goes_before_hidden_runs_at_the_point() {
        let rt = RichText::new(vec![Run::plain("word"), note("n"), Run::plain(" more")]);
        let t = rt.insert(4, "s", None).unwrap();
        assert_eq!(
            t.runs(),
            &[Run::plain("words"), note("n"), Run::plain(" more")]
        );
        let lead = RichText::new(vec![note("n"), Run::plain("x")]);
        let t = lead.insert(0, "a", None).unwrap();
        assert_eq!(t.runs(), &[Run::plain("a"), note("n"), Run::plain("x")]);
    }

    #[test]
    fn offsets_count_characters() {
        let rt = RichText::plain("café ñ");
        assert_eq!(rt.char_len(), 6);
        let t = rt.insert(4, "!", None).unwrap();
        assert_eq!(t.plain_text(), "café! ñ");
        assert!(rt.insert(7, "x", None).is_err());
        assert!(rt.delete(3, 2).is_err());
    }

    #[test]
    fn delete_keeps_hidden_runs() {
        let rt = RichText::new(vec![Run::plain("ab"), note("n"), Run::plain("cd")]);
        let t = rt.delete(1, 3).unwrap();
        assert_eq!(t.runs(), &[Run::plain("a"), note("n"), Run::plain("d")]);
        let t = rt.delete(0, 4).unwrap();
        assert_eq!(t.runs(), &[note("n")]);
        assert_eq!(rt.delete(2, 2).unwrap(), rt);
    }

    #[test]
    fn restyle_sets_and_clears() {
        let rt = RichText::plain("abcd");
        let bold = StyleChange {
            bold: Some(true),
            ..Default::default()
        };
        let t = rt.restyle(1, 3, bold).unwrap();
        assert_eq!(
            t.runs(),
            &[Run::plain("a"), Run::styled("bc", B), Run::plain("d")]
        );
        let unbold = StyleChange {
            bold: Some(false),
            ..Default::default()
        };
        assert_eq!(t.restyle(0, 4, unbold).unwrap(), rt);
    }

    #[test]
    fn split_and_concat_round_trip() {
        let rt = RichText::new(vec![
            Run::plain("ab"),
            note("n"),
            Run::styled("cd", B),
            note("m"),
        ]);
        for at in 0..=rt.char_len() {
            let (a, b) = rt.split_at(at).unwrap();
            assert_eq!(a.concat(&b), rt, "at {at}");
            assert_eq!(a.char_len(), at);
        }
        let (a, b) = rt.split_at(2).unwrap();
        assert_eq!(a.runs(), &[Run::plain("ab")]);
        assert_eq!(b.runs(), &[note("n"), Run::styled("cd", B), note("m")]);
        let (a, b) = rt.split_at(4).unwrap();
        assert_eq!(a, rt);
        assert!(b.is_empty());
    }
}
