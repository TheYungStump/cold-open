//! Marking what an edit changed with a production revision set
//! (docs/specs/revisions.md §6).
//!
//! The edit's operations are applied one at a time, recording each block
//! they touch as it was before. Then every touched block's distributed
//! form (its text with pending insertions left out and pending deletions
//! kept: what the last distributed draft holds) is compared with what it
//! was. The characters that differ take the set's mark, a gap marks its
//! neighbour, and a change to the whole block marks the block. Characters
//! that did not change keep the marks they had, even when the edit
//! rebuilt their runs. The marking is part of the edit's own undo step.

use std::collections::{BTreeMap, BTreeSet};

use coldopen_screenplay::{Element, ElementKind, Marks, RichText, Run, Style, words};

use crate::changes::ChangeId;
use crate::document::{Block, BlockId, Document};
use crate::ops::{Op, Transaction};
use crate::revisions::RevisionId;

impl Block {
    /// Every revision set this block's marks name.
    pub fn rev_ids(&self) -> impl Iterator<Item = RevisionId> + '_ {
        self.rev.into_iter().chain(
            self.element
                .text
                .runs()
                .iter()
                .filter_map(|r| r.marks().rev),
        )
    }

    pub fn has_revisions(&self) -> bool {
        self.rev.is_some() || self.element.text.has_revisions()
    }
}

impl Document {
    /// Whether the revision table or any block differs from the default:
    /// what a Fountain or .fdx file cannot hold (docs/specs/revisions.md §9).
    pub fn has_revision_data(&self) -> bool {
        !self.revisions.is_default() || self.blocks.iter().any(Block::has_revisions)
    }

    /// What layout needs to print the revision marks of the blocks laid
    /// out as `map` gives them (docs/specs/layout.md §18): each element's
    /// block mark, and each set's header text.
    pub fn revision_marks(&self, map: &[usize]) -> coldopen_layout::RevisionMarks {
        coldopen_layout::RevisionMarks {
            elements: map
                .iter()
                .enumerate()
                .filter_map(|(i, &b)| Some((i, self.blocks.get(b)?.rev?.0)))
                .collect(),
            headers: self
                .revisions
                .sets
                .iter()
                .map(|s| (s.id.0, crate::revisions::header_text(s)))
                .collect(),
        }
    }

    /// The document with its revision table and marks reset to the default.
    pub fn without_revisions(&self) -> Document {
        let mut d = self.clone();
        d.revisions = Default::default();
        for b in &mut d.blocks {
            b.rev = None;
            b.element.text = b.element.text.without_revisions();
        }
        d
    }
}

/// What an edit's operations touched, recorded as they were applied.
#[derive(Default)]
struct Touched {
    /// Blocks that were there before the edit and that it touched, as they
    /// were before it.
    old: BTreeMap<BlockId, Block>,
    /// Blocks the edit added.
    added: BTreeSet<BlockId>,
    /// Blocks the edit removed, each with the blocks before and after it
    /// when it went.
    removed: Vec<(BlockId, Option<BlockId>, Option<BlockId>)>,
    /// Blocks the edit moved.
    moved: BTreeSet<BlockId>,
}

impl Touched {
    fn note_old(&mut self, b: &Block) {
        if !self.added.contains(&b.id) {
            self.old.entry(b.id).or_insert_with(|| b.clone());
        }
    }
}

/// Applies `tx` to `doc`, then marks what it changed with the set `rev`
/// gives for the document it left (or only keeps the marks unchanged text
/// had, when that is `None`). Returns the inverse of both, as one
/// transaction.
pub fn apply(
    tx: Transaction,
    doc: &mut Document,
    rev: impl FnOnce(&Document) -> Option<RevisionId>,
) -> Transaction {
    let mut t = Touched::default();
    let mut inverse: Vec<Op> = Vec::with_capacity(tx.0.len());
    for op in tx.0 {
        record(&op, doc, &mut t);
        inverse.push(op.apply(doc));
    }
    let rev = rev(doc);
    for op in marking(doc, &t, rev) {
        inverse.push(op.apply(doc));
    }
    inverse.reverse();
    Transaction(inverse)
}

fn record(op: &Op, doc: &Document, t: &mut Touched) {
    let at = |i: usize| doc.blocks.get(i);
    match op {
        Op::Insert { block, .. } => {
            if let Some(k) = t.removed.iter().position(|(id, ..)| *id == block.id) {
                // Taken out and put back by the same edit: it is still
                // there, and its place may have changed.
                t.removed.remove(k);
                t.moved.insert(block.id);
            } else if !t.old.contains_key(&block.id) {
                t.added.insert(block.id);
            }
        }
        Op::Remove { index } => {
            if let Some(b) = at(*index) {
                if t.added.remove(&b.id) {
                    return;
                }
                t.note_old(b);
                let prev = index.checked_sub(1).and_then(at).map(|b| b.id);
                let next = at(index + 1).map(|b| b.id);
                t.removed.push((b.id, prev, next));
            }
        }
        Op::Replace { index, .. } | Op::Mark { index, .. } | Op::Rev { index, .. } => {
            if let Some(b) = at(*index) {
                t.note_old(b);
            }
        }
        Op::Move { from, len, .. } => {
            for b in doc.blocks.iter().skip(*from).take(*len) {
                if !t.added.contains(&b.id) {
                    t.note_old(b);
                    t.moved.insert(b.id);
                }
            }
        }
        Op::Track { .. } | Op::Revisions { .. } | Op::TitlePage { .. } => {}
    }
}

/// A character of a block's distributed text, with its style and the
/// revision set that last changed it.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    ch: char,
    style: Style,
    rev: Option<RevisionId>,
}

/// Whether a block is in the distributed draft: a printing block that is
/// not a pending insertion.
fn present(b: &Block) -> bool {
    b.element.kind.is_printing() && b.marks.inserted.is_none()
}

/// The kind a block had in the distributed draft: before any pending
/// retype.
fn distributed_kind(b: &Block) -> ElementKind {
    b.marks.retyped.first().map_or(b.element.kind, |r| r.from)
}

/// The printing attributes of a block (docs/specs/revisions.md §6.1).
fn attributes(e: &Element) -> (Option<&String>, bool, bool, Option<u8>) {
    (e.scene_number.as_ref(), e.dual, e.centered, e.space_before)
}

/// The distributed text's characters: pending insertions left out, pending
/// deletions kept, hidden runs skipped.
fn cells(text: &RichText) -> Vec<Cell> {
    let mut out = Vec::new();
    for r in text.runs() {
        if let Run::Text { text, style, marks } = r
            && marks.ins.is_none()
        {
            out.extend(text.chars().map(|ch| Cell {
                ch,
                style: *style,
                rev: marks.rev,
            }));
        }
    }
    out
}

/// `text` with the revision mark of each distributed character `k` set to
/// `revs[k]`.
fn with_revs(text: &RichText, revs: &[Option<RevisionId>]) -> RichText {
    let mut runs = Vec::with_capacity(text.runs().len());
    let mut k = 0;
    for r in text.runs() {
        match r {
            Run::Text { text, style, marks } if marks.ins.is_none() => {
                for ch in text.chars() {
                    let rev = revs.get(k).copied().unwrap_or(marks.rev);
                    runs.push(Run::marked(ch.to_string(), *style, Marks { rev, ..*marks }));
                    k += 1;
                }
            }
            other => runs.push(other.clone()),
        }
    }
    RichText::new(runs)
}

/// For each character of `new`, the revision mark it should carry: the
/// mark of the character it came from in `old` when it did not change,
/// `rev` when it did. A change that only took characters out marks the
/// character before the gap, or after it at the start (§6.2). `None` when
/// nothing differs and no mark needs restoring. The second value is true
/// when a gap had no character to mark.
fn revs_for(
    old: &[Cell],
    new: &[Cell],
    rev: Option<RevisionId>,
) -> (Vec<Option<RevisionId>>, bool) {
    let same = |a: &Cell, b: &Cell| a.ch == b.ch && a.style == b.style;
    let mut out: Vec<Option<RevisionId>> = new.iter().map(|c| c.rev).collect();
    let keep = |i: usize, j: usize, out: &mut Vec<Option<RevisionId>>| out[j] = old[i].rev;
    let mut p = old.iter().zip(new).take_while(|(a, b)| same(a, b)).count();
    let mut q = old[p..]
        .iter()
        .rev()
        .zip(new[p..].iter().rev())
        .take_while(|(a, b)| same(a, b))
        .count();
    // Widen the changed middle to whole words, so a word that changed is
    // marked whole, not from the first character that differs.
    let word = |cs: &[Cell], k: usize| cs.get(k).is_some_and(|c| words::is_word_char(c.ch));
    while p > 0 && word(new, p - 1) && (word(new, p) || word(old, p)) {
        p -= 1;
    }
    while q > 0 && {
        let (n, o) = (new.len() - q, old.len() - q);
        word(new, n) && ((n > 0 && word(new, n - 1)) || (o > 0 && word(old, o - 1)))
    } {
        q -= 1;
    }
    for i in 0..p {
        keep(i, i, &mut out);
    }
    for k in 0..q {
        keep(old.len() - 1 - k, new.len() - 1 - k, &mut out);
    }
    let (om, nm) = (p..old.len() - q, p..new.len() - q);
    let mut gaps: Vec<usize> = Vec::new();
    let mut changed: Vec<usize> = Vec::new();
    if nm.is_empty() {
        if !om.is_empty() {
            gaps.push(p);
        }
    } else if om.is_empty() {
        changed.extend(nm.clone());
    } else {
        // Compare the middles word by word, so only the words that
        // changed take the mark.
        let oc: Vec<char> = old[om.clone()].iter().map(|c| c.ch).collect();
        let nc: Vec<char> = new[nm.clone()].iter().map(|c| c.ch).collect();
        let ow = words::split(&oc);
        let nw = words::split(&nc);
        let key = |cs: &[Cell], r: &std::ops::Range<usize>, base: usize| -> Vec<(char, Style)> {
            cs[base + r.start..base + r.end]
                .iter()
                .map(|c| (c.ch, c.style))
                .collect()
        };
        let ok: Vec<_> = ow.iter().map(|r| key(old, r, om.start)).collect();
        let nk: Vec<_> = nw.iter().map(|r| key(new, r, nm.start)).collect();
        match words::common(&ok, &nk) {
            None => changed.extend(nm.clone()),
            Some(pairs) => {
                let (mut i, mut j) = (0, 0);
                for &(pi, pj) in pairs.iter().chain(std::iter::once(&(ow.len(), nw.len()))) {
                    if j < pj {
                        for w in &nw[j..pj] {
                            changed.extend(nm.start + w.start..nm.start + w.end);
                        }
                    } else if i < pi {
                        gaps.push(nw.get(pj).map_or(nm.end, |w| nm.start + w.start));
                    }
                    if pi < ow.len() {
                        let (o, n) = (&ow[pi], &nw[pj]);
                        for k in 0..o.len() {
                            keep(om.start + o.start + k, nm.start + n.start + k, &mut out);
                        }
                    }
                    (i, j) = (pi + 1, pj + 1);
                }
            }
        }
    }
    let mut lone = false;
    if let Some(rev) = rev {
        for k in changed {
            out[k] = Some(rev);
        }
        for g in gaps {
            match g.checked_sub(1).or((g < new.len()).then_some(g)) {
                Some(k) => out[k] = Some(rev),
                None => lone = true,
            }
        }
    }
    (out, lone)
}

/// The operations that mark what the recorded edit changed.
fn marking(doc: &Document, t: &Touched, rev: Option<RevisionId>) -> Vec<Op> {
    let mut ops = Vec::new();
    // Where each block is now.
    let index: BTreeMap<BlockId, usize> = doc
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id, i))
        .collect();
    // New marks, by index: the block's text, and its block mark.
    let mut texts: BTreeMap<usize, RichText> = BTreeMap::new();
    let mut whole: BTreeSet<usize> = BTreeSet::new();
    // Gaps: a block whose last character (or first, with `false`) marks a
    // place where something went.
    let mut gaps: Vec<usize> = Vec::new();
    let mut scenes: Vec<usize> = Vec::new();

    for (i, b) in doc.blocks.iter().enumerate() {
        if t.added.contains(&b.id) {
            if present(b) {
                whole_or_text(b, i, rev, &mut texts, &mut whole);
            } else if b.element.kind == ElementKind::PageBreak {
                gaps.push(i);
            }
            continue;
        }
        let Some(old) = t.old.get(&b.id) else {
            continue;
        };
        match (present(old), present(b)) {
            (false, true) => whole_or_text(b, i, rev, &mut texts, &mut whole),
            (true, false) => gaps.push(i),
            (false, false) => {}
            (true, true) => {
                if distributed_kind(old) != distributed_kind(b)
                    || attributes(&old.element) != attributes(&b.element)
                {
                    whole.insert(i);
                }
                if old.marks.moved.len() > b.marks.moved.len() {
                    scenes.push(i);
                }
                if old.marks.split.is_some() && b.marks.split.is_none() {
                    // An accepted split: the place it split.
                    if let Some(k) = i.checked_sub(1) {
                        gaps.push(k);
                    }
                }
                let (oc, nc) = (cells(&old.element.text), cells(&b.element.text));
                if oc != nc {
                    let (revs, lone) = revs_for(&oc, &nc, rev);
                    if lone {
                        whole.insert(i);
                    }
                    let current: Vec<Option<RevisionId>> = nc.iter().map(|c| c.rev).collect();
                    if revs != current {
                        texts.insert(i, with_revs(&b.element.text, &revs));
                    }
                }
            }
        }
    }
    if rev.is_some() {
        for id in &t.moved {
            if let Some(&i) = index.get(id)
                && present(&doc.blocks[i])
            {
                whole.insert(i);
            }
        }
        for (_, prev, next) in &t.removed {
            let at = prev
                .and_then(|p| index.get(&p).copied())
                .map(|i| (i, true))
                .or_else(|| {
                    next.and_then(|n| index.get(&n).copied())
                        .map(|i| (i, false))
                });
            if let Some((i, before)) = at {
                gap_at(doc, i, before, rev, &mut texts, &mut whole);
            }
        }
        for i in gaps {
            gap_at(doc, i, true, rev, &mut texts, &mut whole);
        }
        for i in scenes {
            let end = doc.blocks[i + 1..]
                .iter()
                .position(|b| {
                    matches!(
                        b.element.kind,
                        ElementKind::SceneHeading | ElementKind::Section
                    )
                })
                .map_or(doc.blocks.len(), |k| i + 1 + k);
            whole.extend((i..end).filter(|&k| present(&doc.blocks[k])));
        }
    }
    for (i, text) in texts {
        let mut element = doc.blocks[i].element.clone();
        element.text = text;
        ops.push(Op::Replace { index: i, element });
    }
    if let Some(rev) = rev {
        for i in whole {
            if doc.blocks[i].rev != Some(rev) {
                ops.push(Op::Rev {
                    index: i,
                    rev: Some(rev),
                });
            }
        }
    }
    ops
}

/// Marks all of a block's distributed text, or the block when it has none.
fn whole_or_text(
    b: &Block,
    i: usize,
    rev: Option<RevisionId>,
    texts: &mut BTreeMap<usize, RichText>,
    whole: &mut BTreeSet<usize>,
) {
    let Some(rev) = rev else {
        return;
    };
    let n = cells(&b.element.text).len();
    if n == 0 {
        whole.insert(i);
    } else {
        texts.insert(i, with_revs(&b.element.text, &vec![Some(rev); n]));
    }
}

/// Marks a gap next to block `i`: the last character of the nearest
/// printing block at or before it (`before`), or the first of the nearest
/// one at or after it, or that block itself when it has no text. With no
/// printing block that way, the other way is tried.
fn gap_at(
    doc: &Document,
    i: usize,
    before: bool,
    rev: Option<RevisionId>,
    texts: &mut BTreeMap<usize, RichText>,
    whole: &mut BTreeSet<usize>,
) {
    let Some(rev) = rev else {
        return;
    };
    let back = (0..=i.min(doc.blocks.len().saturating_sub(1)))
        .rev()
        .find(|&k| present(&doc.blocks[k]));
    let fwd = (i..doc.blocks.len()).find(|&k| present(&doc.blocks[k]));
    let pick = if before {
        back.map(|k| (k, true)).or(fwd.map(|k| (k, false)))
    } else {
        fwd.map(|k| (k, false)).or(back.map(|k| (k, true)))
    };
    let Some((k, last)) = pick else {
        return;
    };
    let text = texts
        .get(&k)
        .cloned()
        .unwrap_or_else(|| doc.blocks[k].element.text.clone());
    let cs = cells(&text);
    if cs.is_empty() {
        whole.insert(k);
        return;
    }
    let mut revs: Vec<Option<RevisionId>> = cs.iter().map(|c| c.rev).collect();
    let at = if last { cs.len() - 1 } else { 0 };
    revs[at] = Some(rev);
    texts.insert(k, with_revs(&text, &revs));
}

/// The newest revision set recorded by the changes an edit settled: those
/// the blocks it touched named before, and no longer name. A change
/// accepted with no set active is marked with it (docs/specs/revisions.md
/// §6.3).
pub fn settled_rev(doc: &Document, before: &BTreeSet<ChangeId>) -> Option<RevisionId> {
    let still: BTreeSet<ChangeId> = doc.blocks.iter().flat_map(Block::change_ids).collect();
    before
        .iter()
        .filter(|c| !still.contains(c))
        .filter_map(|&c| doc.changes.get(c).and_then(|e| e.rev))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cellv(s: &str, rev: Option<u64>) -> Vec<Cell> {
        s.chars()
            .map(|ch| Cell {
                ch,
                style: Style::PLAIN,
                rev: rev.map(RevisionId),
            })
            .collect()
    }

    fn marked(new: &str, revs: &[Option<RevisionId>]) -> String {
        new.chars()
            .zip(revs)
            .map(|(c, r)| {
                if r.is_some() {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect()
    }

    #[test]
    fn typing_marks_only_the_typed_characters() {
        let r = Some(RevisionId(1));
        let (revs, _) = revs_for(&cellv("the cat", None), &cellv("the fat cat", None), r);
        assert_eq!(marked("the fat cat", &revs), "the FAT cat");
    }

    #[test]
    fn a_deletion_marks_the_character_before_the_gap() {
        let r = Some(RevisionId(1));
        // The gap falls after "the ": the space before it is marked.
        let (revs, lone) = revs_for(&cellv("the fat cat", None), &cellv("the cat", None), r);
        assert_eq!(revs.iter().filter(|r| r.is_some()).count(), 1);
        assert_eq!(revs[3], r);
        assert!(!lone);
        // A word that lost a letter changed: it is marked whole.
        let (revs, _) = revs_for(&cellv("xcat", None), &cellv("cat", None), r);
        assert_eq!(marked("cat", &revs), "CAT");
        let (_, lone) = revs_for(&cellv("x", None), &cellv("", None), r);
        assert!(lone);
    }

    #[test]
    fn scattered_changes_mark_only_their_words() {
        let r = Some(RevisionId(2));
        let (revs, _) = revs_for(
            &cellv("one two three four", None),
            &cellv("one too three for", None),
            r,
        );
        assert_eq!(marked("one too three for", &revs), "one TOO three FOR");
    }

    #[test]
    fn unchanged_text_keeps_its_old_marks() {
        let old: Vec<Cell> = cellv("ab", Some(1))
            .into_iter()
            .chain(cellv(" cd", None))
            .collect();
        // The edit rebuilt the runs, losing the mark on "ab".
        let (revs, _) = revs_for(&old, &cellv("ab cd!", None), None);
        assert_eq!(
            revs,
            vec![
                Some(RevisionId(1)),
                Some(RevisionId(1)),
                None,
                None,
                None,
                None
            ]
        );
    }
}
