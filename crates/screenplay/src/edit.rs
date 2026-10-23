//! Editing rich text by character offsets, as the keystroke commands do
//! (docs/specs/commands.md, "Positions in text").
//!
//! Offsets count Unicode scalar values of the printed text. Hidden runs
//! (notes and boneyard) take no offsets; an offset names the point just
//! after the printed character before it, so hidden runs sitting there
//! belong to the text that follows. Pending deletions (docs/specs/changes.md
//! §2.1) are treated like hidden runs.

use std::fmt;

use crate::text::{ChangeId, Marks, RichText, Run, Style};

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

/// What settling a change does to text (docs/specs/changes.md §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settle {
    Accept,
    Reject,
}

/// One stretch of marked text, for listing changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkedSpan {
    pub change: ChangeId,
    /// True for a pending deletion, false for a pending insertion.
    pub deleted: bool,
    pub text: String,
}

enum Cell {
    /// A character of the current text.
    C(char, Style, Marks),
    /// A hidden run or a pending deletion, kept whole.
    H(Run),
}

fn cells(rt: &RichText) -> Vec<Cell> {
    let mut out = Vec::new();
    for r in rt.runs() {
        match r {
            Run::Text { text, style, marks } if !marks.is_deleted() => {
                out.extend(text.chars().map(|c| Cell::C(c, *style, *marks)))
            }
            h => out.push(Cell::H(h.clone())),
        }
    }
    out
}

fn rebuild(cells: impl IntoIterator<Item = Cell>) -> RichText {
    let mut runs = Vec::new();
    let mut text = String::new();
    let mut style = Style::PLAIN;
    let mut marks = Marks::NONE;
    for cell in cells {
        match cell {
            Cell::C(c, s, m) => {
                if (s != style || m != marks) && !text.is_empty() {
                    runs.push(Run::marked(std::mem::take(&mut text), style, marks));
                }
                style = s;
                marks = m;
                text.push(c);
            }
            Cell::H(h) => {
                if !text.is_empty() {
                    runs.push(Run::marked(std::mem::take(&mut text), style, marks));
                }
                runs.push(h);
            }
        }
    }
    if !text.is_empty() {
        runs.push(Run::marked(text, style, marks));
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

/// Where text goes at offset `at` when it goes after the pending deletions
/// there, and any hidden runs among them, rather than before them
/// (docs/specs/changes.md §2.1).
fn index_past_deleted(cells: &[Cell], at: usize) -> Option<usize> {
    let mut i = insert_index(cells, at)?;
    let mut j = i;
    while let Some(Cell::H(r)) = cells.get(j) {
        j += 1;
        if r.is_deleted() {
            i = j;
        }
    }
    Some(i)
}

impl RichText {
    /// The printed text's length in characters.
    pub fn char_len(&self) -> usize {
        self.runs()
            .iter()
            .map(|r| match r {
                Run::Text { text, marks, .. } if !marks.is_deleted() => text.chars().count(),
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
            if let Run::Text { text, style, marks } = r
                && !marks.is_deleted()
            {
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
        self.insert_marked(at, text, style, Marks::NONE)
    }

    /// Inserts `text` at offset `at` with the given marks, in `style` or the
    /// style at that point.
    pub fn insert_marked(
        &self,
        at: usize,
        text: &str,
        style: Option<Style>,
        marks: Marks,
    ) -> Result<RichText, RangeError> {
        self.insert_beside(at, false, text, style, marks)
    }

    /// [`RichText::insert_marked`], with the text going after any pending
    /// deletion at `at` rather than before it, where a caret drawn after
    /// the struck text types (docs/specs/changes.md §2.1).
    pub fn insert_after_deleted(
        &self,
        at: usize,
        text: &str,
        style: Option<Style>,
        marks: Marks,
    ) -> Result<RichText, RangeError> {
        self.insert_beside(at, true, text, style, marks)
    }

    fn insert_beside(
        &self,
        at: usize,
        past: bool,
        text: &str,
        style: Option<Style>,
        marks: Marks,
    ) -> Result<RichText, RangeError> {
        self.check(at, at)?;
        let style = style.unwrap_or_else(|| self.style_at(at));
        let mut cs = cells(self);
        let i = if past {
            index_past_deleted(&cs, at)
        } else {
            insert_index(&cs, at)
        }
        .unwrap_or(cs.len());
        let new: Vec<Cell> = text.chars().map(|c| Cell::C(c, style, marks)).collect();
        cs.splice(i..i, new);
        Ok(rebuild(cs))
    }

    /// Deletes the printed characters from `from` up to `to`. Hidden runs
    /// and pending deletions inside the range are kept, so they end up at
    /// `from`.
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

    /// A tracked deletion of the printed characters from `from` up to `to`
    /// (docs/specs/changes.md §5.1): a character inserted by a change for
    /// which `own` is true is removed outright, and every other one is
    /// marked deleted by `change`.
    pub fn delete_tracked(
        &self,
        from: usize,
        to: usize,
        change: ChangeId,
        own: &dyn Fn(ChangeId) -> bool,
    ) -> Result<RichText, RangeError> {
        self.check(from, to)?;
        let mut v = 0;
        let mut out = Vec::new();
        for c in cells(self) {
            match c {
                Cell::C(ch, s, m) => {
                    let inside = v >= from && v < to;
                    v += 1;
                    if !inside {
                        out.push(Cell::C(ch, s, m));
                    } else if m.ins.is_some_and(own) {
                        // Never in the accepted text: gone.
                    } else {
                        out.push(Cell::H(Run::marked(
                            ch.to_string(),
                            s,
                            Marks {
                                del: Some(change),
                                ..m
                            },
                        )));
                    }
                }
                h => out.push(h),
            }
        }
        Ok(rebuild(out))
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
            Cell::C(ch, s, m) => {
                let s = if v >= from && v < to {
                    change.apply(s)
                } else {
                    s
                };
                v += 1;
                Cell::C(ch, s, m)
            }
            h => h,
        });
        Ok(rebuild(changed))
    }

    /// Splits at offset `at`. Hidden runs at the split point go with the
    /// second part, except at the very end, where they stay with the first.
    pub fn split_at(&self, at: usize) -> Result<(RichText, RichText), RangeError> {
        self.split_beside(at, false)
    }

    /// [`RichText::split_at`], with any pending deletion at `at` kept in the
    /// first part rather than starting the second (docs/specs/changes.md
    /// §2.1).
    pub fn split_after_deleted(&self, at: usize) -> Result<(RichText, RichText), RangeError> {
        self.split_beside(at, true)
    }

    fn split_beside(&self, at: usize, past: bool) -> Result<(RichText, RichText), RangeError> {
        let len = self.check(at, at)?;
        let mut cs = cells(self);
        let i = if at == len {
            cs.len()
        } else if past {
            index_past_deleted(&cs, at).unwrap_or(cs.len())
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

    /// The insertion marks next to offset `at`: of the printed character
    /// before it, and of the one at it.
    pub fn ins_around(&self, at: usize) -> (Option<ChangeId>, Option<ChangeId>) {
        let mut before = None;
        let mut after = None;
        let mut v = 0;
        for r in self.runs() {
            if let Run::Text { text, marks, .. } = r
                && !marks.is_deleted()
            {
                for _ in text.chars() {
                    if v + 1 == at {
                        before = marks.ins;
                    }
                    if v == at {
                        after = marks.ins;
                    }
                    v += 1;
                }
            }
        }
        (before, after)
    }

    /// The changes of pending deletions that sit at offset `from` or at
    /// offset `to`, in text order.
    pub fn del_touching(&self, from: usize, to: usize) -> Vec<ChangeId> {
        let mut out = Vec::new();
        let mut v = 0;
        for r in self.runs() {
            match r {
                Run::Text { text, marks, .. } if !marks.is_deleted() => v += text.chars().count(),
                Run::Text { marks, .. } => {
                    if (v == from || v == to)
                        && let Some(d) = marks.del
                    {
                        out.push(d);
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The text with the marks of the changes `which` picks settled, as
    /// accepting or rejecting them does (docs/specs/changes.md §6). Other
    /// marks stay.
    pub fn settle(&self, how: Settle, which: &dyn Fn(ChangeId) -> bool) -> RichText {
        let mut runs = Vec::with_capacity(self.runs().len());
        for r in self.runs() {
            match r {
                Run::Text { text, style, marks } => {
                    let ins = marks.ins.filter(|&c| which(c));
                    let del = marks.del.filter(|&c| which(c));
                    let removed = match how {
                        Settle::Accept => del.is_some(),
                        Settle::Reject => ins.is_some(),
                    };
                    if removed {
                        continue;
                    }
                    let mut m = *marks;
                    if ins.is_some() {
                        m.ins = None;
                    }
                    if del.is_some() {
                        m.del = None;
                    }
                    runs.push(Run::marked(text.clone(), *style, m));
                }
                h => runs.push(h.clone()),
            }
        }
        RichText::new(runs)
    }

    /// The text with every mark removed and every pending deletion left out:
    /// the current text, as accepting every change leaves it.
    pub fn accepted(&self) -> RichText {
        self.settle(Settle::Accept, &|_| true)
    }

    /// The text as a print form shows it: pending deletions count as
    /// printed text, so they take their place on the page, struck through
    /// (docs/specs/changes.md §9).
    pub fn shown(&self) -> RichText {
        let runs = self
            .runs()
            .iter()
            .map(|r| match r {
                Run::Text { text, style, marks } if marks.del.is_some() => Run::marked(
                    text.clone(),
                    *style,
                    Marks {
                        show: true,
                        ..*marks
                    },
                ),
                other => other.clone(),
            })
            .collect();
        RichText::new(runs)
    }

    /// The text as a command's input stands for it: a run marked deleted is
    /// left out and insertion marks are dropped, since marks are only ever
    /// made by tracked edits (docs/specs/commands.md, "Blocks in JSON").
    pub fn as_input(&self) -> RichText {
        let runs = self
            .runs()
            .iter()
            .filter(|r| !matches!(r, Run::Text { marks, .. } if marks.del.is_some()))
            .map(|r| match r {
                Run::Text { text, style, .. } => Run::styled(text.clone(), *style),
                other => other.clone(),
            })
            .collect();
        RichText::new(runs)
    }

    /// The byte, in the text as a print form shows it (pending deletions
    /// included), where offset `at` of the current text falls: just after
    /// the `at`-th printed character, before any deletion there, as text
    /// typed there would go.
    pub fn shown_byte(&self, at: usize) -> usize {
        let mut v = 0;
        let mut byte = 0;
        for r in self.runs() {
            let Run::Text { text, marks, .. } = r else {
                continue;
            };
            if marks.del.is_some() {
                if v == at {
                    return byte;
                }
                byte += text.len();
                continue;
            }
            let n = text.chars().count();
            if at < v + n {
                return byte
                    + text
                        .char_indices()
                        .nth(at - v)
                        .map_or(text.len(), |(i, _)| i);
            }
            v += n;
            byte += text.len();
        }
        byte
    }

    /// The offset of the current text a byte of the shown text (pending
    /// deletions included) belongs to: inside a deletion, the offset just
    /// before it.
    pub fn offset_at_shown_byte(&self, at: usize) -> usize {
        let mut v = 0;
        let mut byte = 0;
        for r in self.runs() {
            if let Run::Text { text, marks, .. } = r {
                if byte + text.len() > at {
                    if marks.del.is_some() {
                        return v;
                    }
                    return v + text
                        .char_indices()
                        .take_while(|(i, _)| *i < at - byte)
                        .count();
                }
                if marks.del.is_none() {
                    v += text.chars().count();
                }
                byte += text.len();
            }
        }
        v
    }

    /// The bytes, in the text as a print form shows it, of the pending
    /// deletions sitting at offset `at` of the current text: the struck
    /// text a caret there can stand before or after (docs/specs/editor.md
    /// §4). 0 when there is none.
    pub fn deleted_at(&self, at: usize) -> usize {
        let mut v = 0;
        let mut bytes = 0;
        for r in self.runs() {
            let Run::Text { text, marks, .. } = r else {
                continue;
            };
            if marks.del.is_some() {
                if v == at {
                    bytes += text.len();
                }
                continue;
            }
            if v >= at && !text.is_empty() {
                break;
            }
            v += text.chars().count();
        }
        bytes
    }

    /// The text's length in bytes as a print form shows it.
    pub fn shown_len(&self) -> usize {
        self.runs()
            .iter()
            .map(|r| match r {
                Run::Text { text, .. } => text.len(),
                _ => 0,
            })
            .sum()
    }

    /// Where each marked stretch of the text starts, as an offset of the
    /// current text with its change, in order. A pending deletion takes no
    /// offsets, so it starts where it sits. A stretch running on with the
    /// same change is listed once.
    pub fn mark_offsets(&self) -> Vec<(usize, ChangeId)> {
        let mut out = Vec::new();
        let mut v = 0;
        let mut prev: Option<ChangeId> = None;
        for r in self.runs() {
            let Run::Text { text, marks, .. } = r else {
                continue;
            };
            let here = marks.del.or(marks.ins);
            if let Some(c) = here
                && prev != Some(c)
            {
                out.push((v, c));
            }
            prev = here;
            if marks.del.is_none() {
                v += text.chars().count();
            }
        }
        out
    }

    /// The change a caret at offset `at` of the current text touches: the
    /// marked text just before it, else the marked text just after it (a
    /// pending deletion sitting at `at` is drawn after the caret).
    pub fn change_at(&self, at: usize) -> Option<ChangeId> {
        let mut v = 0;
        let mut after = None;
        for r in self.runs() {
            let Run::Text { text, marks, .. } = r else {
                continue;
            };
            if let Some(d) = marks.del {
                if v == at && after.is_none() {
                    after = Some(d);
                }
                continue;
            }
            let n = text.chars().count();
            if let Some(c) = marks.ins {
                if v < at && at <= v + n {
                    return Some(c);
                }
                if v <= at && at < v + n && after.is_none() {
                    after = Some(c);
                }
            }
            v += n;
            if v > at && after.is_some() {
                break;
            }
        }
        after
    }

    /// The first change marked between offsets `from` and `to` of the
    /// current text, a pending deletion at either end included.
    pub fn first_change_in(&self, from: usize, to: usize) -> Option<ChangeId> {
        let mut v = 0;
        for r in self.runs() {
            let Run::Text { text, marks, .. } = r else {
                continue;
            };
            if let Some(d) = marks.del {
                if from <= v && v <= to {
                    return Some(d);
                }
                continue;
            }
            let n = text.chars().count();
            if let Some(c) = marks.ins
                && v < to
                && from < v + n
            {
                return Some(c);
            }
            v += n;
            if v > to {
                break;
            }
        }
        None
    }

    /// The marked stretches of the text, in order.
    pub fn marked_spans(&self) -> Vec<MarkedSpan> {
        let mut out = Vec::new();
        for r in self.runs() {
            if let Run::Text { text, marks, .. } = r {
                if let Some(c) = marks.ins {
                    out.push(MarkedSpan {
                        change: c,
                        deleted: false,
                        text: text.clone(),
                    });
                }
                if let Some(c) = marks.del {
                    out.push(MarkedSpan {
                        change: c,
                        deleted: true,
                        text: text.clone(),
                    });
                }
            }
        }
        out
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

    fn ins(c: u64) -> Marks {
        Marks {
            ins: Some(ChangeId(c)),
            ..Marks::NONE
        }
    }

    fn del(c: u64) -> Marks {
        Marks {
            del: Some(ChangeId(c)),
            ..Marks::NONE
        }
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
    fn text_can_go_after_the_struck_text_at_its_offset() {
        // "abc", struck "XYZ", a note, struck "UV", "def": both deletions
        // sit at offset 3.
        let rt = RichText::new(vec![
            Run::plain("abc"),
            Run::marked("XYZ", Style::PLAIN, del(1)),
            note("n"),
            Run::marked("UV", Style::PLAIN, del(2)),
            Run::plain("def"),
        ]);
        assert_eq!(rt.deleted_at(3), 5);
        assert_eq!(rt.deleted_at(2), 0);
        assert_eq!(rt.deleted_at(4), 0);
        assert_eq!(rt.shown_byte(3), 3);
        let t = rt.insert(3, "!", None).unwrap();
        assert_eq!(t.runs()[0], Run::plain("abc!"));
        let t = rt.insert_after_deleted(3, "!", None, Marks::NONE).unwrap();
        assert_eq!(t.runs()[3], Run::marked("UV", Style::PLAIN, del(2)));
        assert_eq!(t.runs()[4], Run::plain("!def"));
        let (head, rest) = rt.split_after_deleted(3).unwrap();
        assert_eq!(head.runs().len(), 4);
        assert_eq!(rest, RichText::plain("def"));
        let (head, rest) = rt.split_at(3).unwrap();
        assert_eq!(head, RichText::plain("abc"));
        assert_eq!(rest.runs().len(), 4);
        // Struck text at the start and at the end.
        let edges = RichText::new(vec![
            Run::marked("old", Style::PLAIN, del(1)),
            Run::plain("mid"),
            Run::marked("end", Style::PLAIN, del(2)),
        ]);
        assert_eq!(edges.deleted_at(0), 3);
        assert_eq!(edges.deleted_at(3), 3);
        let t = edges
            .insert_after_deleted(0, "a", None, Marks::NONE)
            .unwrap();
        assert_eq!(t.runs()[1], Run::plain("amid"));
        let t = edges
            .insert_after_deleted(3, "z", None, Marks::NONE)
            .unwrap();
        assert_eq!(t.runs().last(), Some(&Run::plain("z")));
        // No struck text there: the same as an insertion.
        assert_eq!(
            edges.insert_after_deleted(1, "x", None, Marks::NONE),
            edges.insert(1, "x", None)
        );
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

    #[test]
    fn pending_deletions_take_no_offsets() {
        let rt = RichText::new(vec![
            Run::plain("the "),
            Run::marked("big ", Style::PLAIN, del(1)),
            Run::plain("dog"),
        ]);
        assert_eq!(rt.plain_text(), "the dog");
        assert_eq!(rt.char_len(), 7);
        // Text typed where the deletion sits goes before it.
        let t = rt.insert(4, "red ", None).unwrap();
        assert_eq!(t.plain_text(), "the red dog");
        assert_eq!(t.runs()[0], Run::plain("the red "));
        assert!(t.runs()[1].is_deleted());
        // An untracked delete keeps the deletion, as it keeps notes.
        let t = rt.delete(0, 7).unwrap();
        assert_eq!(t.runs(), &[Run::marked("big ", Style::PLAIN, del(1))]);
        assert!(t.has_marks());
    }

    #[test]
    fn tracked_deletes_remove_own_insertions_and_mark_the_rest() {
        let rt = RichText::new(vec![
            Run::plain("ab"),
            Run::marked("cd", Style::PLAIN, ins(1)),
            Run::marked("ef", Style::PLAIN, ins(2)),
        ]);
        let own = |c: ChangeId| c == ChangeId(1);
        let t = rt.delete_tracked(1, 5, ChangeId(3), &own).unwrap();
        assert_eq!(t.plain_text(), "af");
        assert_eq!(
            t.runs(),
            &[
                Run::plain("a"),
                Run::marked("b", Style::PLAIN, del(3)),
                Run::marked(
                    "e",
                    Style::PLAIN,
                    Marks {
                        ins: Some(ChangeId(2)),
                        del: Some(ChangeId(3)),
                        show: false,
                        rev: None,
                    }
                ),
                Run::marked("f", Style::PLAIN, ins(2)),
            ]
        );
        // Accepting change 2 keeps its text, which change 3 still deletes.
        let a = t.settle(Settle::Accept, &|c| c == ChangeId(2));
        assert_eq!(a.plain_text(), "af");
        // Rejecting change 2 takes its text away, deleted or not.
        let r = t.settle(Settle::Reject, &|c| c == ChangeId(2));
        assert_eq!(r.plain_text(), "a");
        assert_eq!(r.runs().len(), 2);
        // Accepting the deletion removes the struck text.
        assert_eq!(
            t.settle(Settle::Accept, &|c| c == ChangeId(3)).plain_text(),
            "af"
        );
        assert_eq!(
            t.settle(Settle::Reject, &|c| c == ChangeId(3)).plain_text(),
            "abef"
        );
        assert_eq!(t.accepted(), RichText::new(vec![Run::plain("af")]));
    }

    #[test]
    fn marks_around_a_point_and_a_range() {
        let rt = RichText::new(vec![
            Run::marked("ab", Style::PLAIN, ins(1)),
            Run::marked("x", Style::PLAIN, del(2)),
            Run::plain("cd"),
        ]);
        assert_eq!(rt.ins_around(2), (Some(ChangeId(1)), None));
        assert_eq!(rt.ins_around(1), (Some(ChangeId(1)), Some(ChangeId(1))));
        assert_eq!(rt.ins_around(0), (None, Some(ChangeId(1))));
        assert_eq!(rt.del_touching(2, 3), vec![ChangeId(2)]);
        assert_eq!(rt.del_touching(0, 1), Vec::<ChangeId>::new());
    }

    #[test]
    fn the_caret_finds_the_change_it_touches() {
        // "ab" + inserted "cd" (c_1) + "e" + struck "xy" (c_2) + "f"
        let rt = RichText::new(vec![
            Run::plain("ab"),
            Run::marked("cd", Style::PLAIN, ins(1)),
            Run::plain("e"),
            Run::marked("xy", Style::PLAIN, del(2)),
            Run::plain("f"),
        ]);
        let c = |n| Some(ChangeId(n));
        assert_eq!(rt.change_at(0), None);
        assert_eq!(rt.change_at(2), c(1), "just after it");
        assert_eq!(rt.change_at(3), c(1));
        assert_eq!(rt.change_at(4), c(1), "just before it");
        assert_eq!(rt.change_at(5), c(2), "a deletion sits after the caret");
        assert_eq!(rt.change_at(6), None);
        assert_eq!(rt.mark_offsets(), vec![(2, ChangeId(1)), (5, ChangeId(2))]);
        assert_eq!(rt.first_change_in(0, 1), None);
        assert_eq!(rt.first_change_in(0, 3), c(1));
        assert_eq!(rt.first_change_in(4, 6), c(2));
        assert_eq!(rt.first_change_in(5, 5), c(2));
        // A deletion at the start, and two changes side by side.
        let lead = RichText::new(vec![
            Run::marked("x", Style::PLAIN, del(3)),
            Run::marked("y", Style::PLAIN, ins(4)),
        ]);
        assert_eq!(lead.change_at(0), c(3));
        assert_eq!(lead.change_at(1), c(4));
        assert_eq!(
            lead.mark_offsets(),
            vec![(0, ChangeId(3)), (0, ChangeId(4))]
        );
        assert_eq!(RichText::plain("plain").mark_offsets(), vec![]);
    }

    #[test]
    fn offsets_map_to_the_shown_text_and_back() {
        // "a" + struck "bc" + "de"
        let rt = RichText::new(vec![
            Run::plain("a"),
            Run::marked("bc", Style::PLAIN, del(1)),
            Run::plain("de"),
        ]);
        assert_eq!(rt.shown_byte(0), 0);
        assert_eq!(rt.shown_byte(1), 1, "before the deletion");
        assert_eq!(rt.shown_byte(2), 4);
        assert_eq!(rt.shown_byte(3), 5);
        assert_eq!(rt.shown_len(), 5);
        assert_eq!(rt.offset_at_shown_byte(0), 0);
        assert_eq!(rt.offset_at_shown_byte(1), 1);
        assert_eq!(rt.offset_at_shown_byte(2), 1, "inside the deletion");
        assert_eq!(rt.offset_at_shown_byte(3), 1);
        assert_eq!(rt.offset_at_shown_byte(4), 2);
        assert_eq!(rt.offset_at_shown_byte(5), 3);
        // A deletion at the start.
        let lead = RichText::new(vec![
            Run::marked("x", Style::PLAIN, del(1)),
            Run::plain("y"),
        ]);
        assert_eq!(lead.shown_byte(0), 0);
        assert_eq!(lead.shown_byte(1), 2);
        assert_eq!(lead.offset_at_shown_byte(0), 0);
        assert_eq!(lead.offset_at_shown_byte(1), 0);
        // With no marks, bytes and characters as before.
        let plain = RichText::plain("café");
        assert_eq!(plain.shown_byte(4), 5);
        assert_eq!(plain.offset_at_shown_byte(5), 4);
    }

    #[test]
    fn shown_text_prints_deletions_and_input_drops_them() {
        let rt = RichText::new(vec![
            Run::plain("a"),
            Run::marked("b", Style::PLAIN, del(1)),
            Run::marked("c", Style::PLAIN, ins(2)),
        ]);
        assert_eq!(rt.plain_text(), "ac");
        assert_eq!(rt.shown().plain_text(), "abc");
        assert_eq!(rt.shown().char_len(), 3);
        assert_eq!(rt.as_input(), RichText::plain("ac"));
        let spans = rt.marked_spans();
        assert_eq!(spans.len(), 2);
        assert!(spans[0].deleted && spans[0].text == "b");
    }
}
