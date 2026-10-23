//! Tracked edits and settling them (docs/specs/changes.md §5 and §6): what
//! the session needs beyond the text operations in `coldopen-screenplay`.

use std::collections::{BTreeMap, BTreeSet};

use coldopen_screenplay::{ElementKind, Marks, RichText, Run, Settle, Style};
use serde_json::{Value, json};

use crate::author::Author;
use crate::changes::{ChangeBy, ChangeId};
use crate::document::{Block, BlockId, Document};
use crate::error::CommandError;
use crate::ops::Op;

/// The change a tracked edit records into, made when it is first needed
/// so an edit that changes nothing leaves the table as it was.
pub struct TrackCtx {
    pub by: ChangeBy,
    pub t: u64,
    /// The change this undo step records into, once known.
    pub change: Option<ChangeId>,
}

impl TrackCtx {
    pub fn new(author: &Author, t: u64) -> TrackCtx {
        TrackCtx {
            by: ChangeBy::of(author),
            t,
            change: None,
        }
    }

    /// Whether this author made change `c`.
    pub fn owns(&self, doc: &Document, c: ChangeId) -> bool {
        doc.changes.get(c).is_some_and(|e| e.by == self.by)
    }

    /// The step's change: the one already chosen, else `candidate` when it
    /// is this author's own, else a new one added to the table.
    pub fn change(&mut self, doc: &mut Document, candidate: Option<ChangeId>) -> ChangeId {
        if let Some(c) = self.change {
            return c;
        }
        let c = match candidate.filter(|&c| self.owns(doc, c)) {
            Some(c) => c,
            None => doc
                .changes
                .push(self.by.clone(), self.t, doc.revisions.active),
        };
        self.change = Some(c);
        c
    }
}

// ---- Words ---------------------------------------------------------------

/// Splits text into words (letters, digits and apostrophes) and single
/// other characters, as character index ranges.
fn tokens(chars: &[char]) -> Vec<(usize, usize)> {
    let word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if word(chars[i]) {
            while i < chars.len() && word(chars[i]) {
                i += 1;
            }
        } else {
            i += 1;
        }
        out.push((start, i));
    }
    out
}

/// One step of a word diff: old tokens kept, removed, or new tokens added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// The largest product of middle token counts compared word by word
/// (docs/specs/changes.md §5.1).
const MAX_PAIRS: usize = 1_000_000;

/// A word diff of two token lists: common start and end, then the longest
/// common subsequence of what is between, deletions before insertions.
fn diff(a: &[&[char]], b: &[&[char]]) -> Vec<Step> {
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf]
    {
        suf += 1;
    }
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mut steps: Vec<Step> = (0..pre).map(|i| Step::Equal(i, i)).collect();
    let (n, m) = (am.len(), bm.len());
    if n.saturating_mul(m) > MAX_PAIRS || n == 0 || m == 0 {
        steps.extend((0..n).map(|i| Step::Delete(pre + i)));
        steps.extend((0..m).map(|j| Step::Insert(pre + j)));
    } else {
        // lcs[i][j]: the longest common subsequence of am[i..] and bm[j..].
        let w = m + 1;
        let mut lcs = vec![0u32; (n + 1) * w];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i * w + j] = if am[i] == bm[j] {
                    lcs[(i + 1) * w + j + 1] + 1
                } else {
                    lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && am[i] == bm[j] {
                steps.push(Step::Equal(pre + i, pre + j));
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]) {
                steps.push(Step::Delete(pre + i));
                i += 1;
            } else {
                steps.push(Step::Insert(pre + j));
                j += 1;
            }
        }
    }
    let (ea, eb) = (a.len() - suf, b.len() - suf);
    steps.extend((0..suf).map(|k| Step::Equal(ea + k, eb + k)));
    steps
}

/// A character of the current text with its style and marks, or a run that
/// takes no offsets, anchored to the number of characters before it.
struct Laid {
    chars: Vec<(char, Style, Marks)>,
    /// (characters before it, run), in order.
    loose: Vec<(usize, Run)>,
}

fn lay(rt: &RichText) -> Laid {
    let mut chars = Vec::new();
    let mut loose = Vec::new();
    for r in rt.runs() {
        match r {
            Run::Text { text, style, marks } if !marks.is_deleted() => {
                chars.extend(text.chars().map(|c| (c, *style, *marks)));
            }
            other => loose.push((chars.len(), other.clone())),
        }
    }
    Laid { chars, loose }
}

/// A tracked `block.set_text` (docs/specs/changes.md §5.1): the new text,
/// with words it shares with the old current text keeping their marks, its
/// own words marked inserted, the old words it drops kept as deletions
/// (or dropped, where they were the author's own insertion), and the old
/// text's pending deletions where they stood. Notes and boneyard come from
/// the new text.
pub fn set_text(
    old: &RichText,
    new: &RichText,
    change: ChangeId,
    own: &dyn Fn(ChangeId) -> bool,
) -> RichText {
    let o = lay(old);
    let n = lay(new);
    let oc: Vec<char> = o.chars.iter().map(|c| c.0).collect();
    let nc: Vec<char> = n.chars.iter().map(|c| c.0).collect();
    let ot = tokens(&oc);
    let nt = tokens(&nc);
    let ow: Vec<&[char]> = ot.iter().map(|&(a, b)| &oc[a..b]).collect();
    let nw: Vec<&[char]> = nt.iter().map(|&(a, b)| &nc[a..b]).collect();

    let mut out: Vec<Run> = Vec::new();
    // Old pending deletions are put back before the old character they
    // stood before; new notes before the new character they stand before.
    let old_dels: Vec<&(usize, Run)> = o.loose.iter().filter(|(_, r)| !r.is_hidden()).collect();
    let mut od = 0;
    let mut nh = n.loose.iter().filter(|(_, r)| r.is_hidden()).peekable();
    let mut put_old = |out: &mut Vec<Run>, at: usize| {
        while od < old_dels.len() && old_dels[od].0 <= at {
            out.push(old_dels[od].1.clone());
            od += 1;
        }
    };
    let mut put_new = |out: &mut Vec<Run>, at: usize| {
        while let Some((pos, r)) = nh.next_if(|(pos, _)| *pos <= at) {
            let _ = pos;
            out.push(r.clone());
        }
    };
    for step in diff(&ow, &nw) {
        match step {
            Step::Equal(i, j) => {
                let (a, b) = ot[i];
                let (c, _) = nt[j];
                for k in 0..b - a {
                    put_old(&mut out, a + k);
                    put_new(&mut out, c + k);
                    let (_, _, marks) = o.chars[a + k];
                    let (ch, style, _) = n.chars[c + k];
                    out.push(Run::marked(ch.to_string(), style, marks));
                }
            }
            Step::Delete(i) => {
                let (a, b) = ot[i];
                for k in a..b {
                    put_old(&mut out, k);
                    let (ch, style, marks) = o.chars[k];
                    if marks.ins.is_some_and(own) {
                        continue;
                    }
                    out.push(Run::marked(
                        ch.to_string(),
                        style,
                        Marks {
                            del: Some(change),
                            ..marks
                        },
                    ));
                }
            }
            Step::Insert(j) => {
                let (c, d) = nt[j];
                for k in c..d {
                    put_new(&mut out, k);
                    let (ch, style, _) = n.chars[k];
                    out.push(Run::marked(
                        ch.to_string(),
                        style,
                        Marks {
                            ins: Some(change),
                            ..Marks::NONE
                        },
                    ));
                }
            }
        }
    }
    put_old(&mut out, usize::MAX);
    put_new(&mut out, usize::MAX);
    RichText::new(out)
}

// ---- Who may change whose marks -------------------------------------------

/// Every mark of a change another author made, with what it marks: the
/// text a change inserted or deleted in each block, and each block mark.
/// An agent's edit must leave this as it was (docs/specs/changes.md §5.4).
pub fn others_marks(doc: &Document, by: &ChangeBy) -> BTreeMap<(ChangeId, BlockId, u8), String> {
    let mut out: BTreeMap<(ChangeId, BlockId, u8), String> = BTreeMap::new();
    let other = |c: ChangeId| doc.changes.get(c).is_none_or(|e| e.by != *by);
    for b in &doc.blocks {
        for r in b.element.text.runs() {
            if let Run::Text { text, marks, .. } = r {
                if let Some(c) = marks.ins.filter(|&c| other(c)) {
                    out.entry((c, b.id, 0)).or_default().push_str(text);
                }
                if let Some(c) = marks.del.filter(|&c| other(c)) {
                    out.entry((c, b.id, 1)).or_default().push_str(text);
                }
            }
        }
        let m = &b.marks;
        let mut put = |c: Option<ChangeId>, kind: u8, what: String| {
            if let Some(c) = c.filter(|&c| other(c)) {
                out.insert((c, b.id, kind), what);
            }
        };
        put(m.inserted, 2, String::new());
        put(m.deleted, 3, String::new());
        for r in &m.retyped {
            put(
                Some(r.change),
                4,
                serde_json::to_string(r).unwrap_or_default(),
            );
        }
        put(m.split, 5, String::new());
        put(m.joined, 6, String::new());
        for r in &m.moved {
            put(
                Some(r.change),
                7,
                serde_json::to_string(r).unwrap_or_default(),
            );
        }
    }
    out
}

pub fn not_your_change(what: &str) -> CommandError {
    CommandError::new(
        "not_your_change",
        format!("{what}; an agent may not change or settle another author's tracked changes"),
    )
}

// ---- Settling --------------------------------------------------------------

/// The changes a settle names, and where (docs/specs/changes.md §6).
pub struct Selection {
    pub changes: BTreeSet<ChangeId>,
    /// The blocks whose marks are settled: all, or a scene's.
    pub blocks: Option<BTreeSet<BlockId>>,
}

/// What settling leaves, as operations on the document and the result.
pub struct Settled {
    pub ops: Vec<Op>,
    pub settled: Vec<ChangeId>,
    pub skipped: Vec<ChangeId>,
}

/// Plans accepting or rejecting the selected changes. The document is not
/// changed: the operations are worked out on a copy.
pub fn settle(doc: &Document, how: Settle, sel: &Selection) -> Settled {
    let mut work = doc.clone();
    let mut ops = Vec::new();
    let picked = |c: ChangeId| sel.changes.contains(&c);
    let in_scope = |id: BlockId| sel.blocks.as_ref().is_none_or(|s| s.contains(&id));
    let mut settled: BTreeSet<ChangeId> = BTreeSet::new();
    let mut skipped = Vec::new();

    // 1. Text marks, and block marks that only come off, in any order.
    for index in 0..work.blocks.len() {
        let b = &work.blocks[index];
        if !in_scope(b.id) {
            continue;
        }
        for c in b.change_ids() {
            if picked(c) {
                settled.insert(c);
            }
        }
        let text = b.element.text.settle(how, &picked);
        if text != b.element.text {
            let mut e = b.element.clone();
            e.text = text;
            apply(&mut work, Op::Replace { index, element: e }, &mut ops);
        }
        let mut m = work.blocks[index].marks.clone();
        let off = |c: &Option<ChangeId>| c.is_some_and(picked);
        match how {
            Settle::Accept => {
                if off(&m.inserted) {
                    m.inserted = None;
                }
                m.retyped.retain(|r| !picked(r.change));
                if off(&m.split) {
                    m.split = None;
                }
                m.moved.retain(|r| !picked(r.change));
            }
            Settle::Reject => {
                if off(&m.deleted) {
                    m.deleted = None;
                }
                if off(&m.joined) {
                    m.joined = None;
                }
            }
        }
        if m != work.blocks[index].marks {
            apply(&mut work, Op::Mark { index, marks: m }, &mut ops);
        }
    }

    match how {
        Settle::Accept => {
            // 2. Deleted blocks go, then joined blocks join the block kept
            // before them, as the accepted form joins them.
            remove_where(&mut work, &mut ops, &|b| {
                in_scope(b.id) && b.marks.deleted.is_some_and(picked)
            });
            skipped.extend(join_where(&mut work, &mut ops, &|b| {
                in_scope(b.id) && b.marks.joined.is_some_and(picked)
            }));
        }
        Settle::Reject => {
            // 2. Structure is put back change by change, newest first, as
            // undo would: each change's moves, then its inserted blocks,
            // then its splits, then its retypes. A later change that
            // reshaped a scene is undone before an earlier move of it.
            let mut structural: Vec<ChangeId> = work
                .blocks
                .iter()
                .filter(|b| in_scope(b.id))
                .flat_map(|b| {
                    let m = &b.marks;
                    [m.inserted, m.split]
                        .into_iter()
                        .flatten()
                        .chain(m.moved.iter().map(|x| x.change))
                        .chain(m.retyped.iter().map(|x| x.change))
                        .collect::<Vec<_>>()
                })
                .filter(|&c| picked(c))
                .collect();
            structural.sort_unstable_by(|a, b| b.cmp(a));
            structural.dedup();
            for c in structural {
                if !undo_moves(&mut work, &mut ops, c, &in_scope) {
                    skipped.push(c);
                }
                remove_where(&mut work, &mut ops, &|b| {
                    in_scope(b.id) && b.marks.inserted == Some(c)
                });
                skipped.extend(join_where(&mut work, &mut ops, &|b| {
                    in_scope(b.id) && b.marks.split == Some(c)
                }));
                restore_retypes(&mut work, &mut ops, c, &in_scope);
            }
        }
    }

    let skipped_set: BTreeSet<ChangeId> = skipped.iter().copied().collect();
    let settled = settled
        .into_iter()
        .filter(|c| !skipped_set.contains(c) || has_other_marks(doc, *c))
        .collect();
    Settled {
        ops,
        settled,
        skipped,
    }
}

/// Applies an operation to the working copy and keeps it.
fn apply(work: &mut Document, op: Op, ops: &mut Vec<Op>) {
    ops.push(op.clone());
    let _ = op.apply(work);
}

/// Removes every block `which` picks, from the last to the first.
fn remove_where(work: &mut Document, ops: &mut Vec<Op>, which: &dyn Fn(&Block) -> bool) {
    for index in (0..work.blocks.len()).rev() {
        if which(&work.blocks[index]) {
            apply(work, Op::Remove { index }, ops);
        }
    }
}

/// Joins every block `which` picks to the block before it, from the first
/// to the last, so a run of joined blocks gathers into the block before it
/// as the accepted form gathers them. A block with nothing before it, or
/// whose join would break a model rule (the block before it has since
/// become a note, say), stays as it is: its join mark goes, and its change
/// is returned as skipped.
fn join_where(
    work: &mut Document,
    ops: &mut Vec<Op>,
    which: &dyn Fn(&Block) -> bool,
) -> Vec<ChangeId> {
    let mut skipped = Vec::new();
    let mut index = 0;
    while index < work.blocks.len() {
        if !which(&work.blocks[index]) {
            index += 1;
            continue;
        }
        let merged = index.checked_sub(1).and_then(|p| {
            crate::changes::joined(&work.blocks[p].element, &work.blocks[index].element.text)
        });
        if let Some(prev) = merged {
            apply(
                work,
                Op::Replace {
                    index: index - 1,
                    element: prev,
                },
                ops,
            );
            apply(work, Op::Remove { index }, ops);
        } else {
            let mut marks = work.blocks[index].marks.clone();
            skipped.extend(marks.joined.take());
            skipped.extend(marks.split.take());
            apply(work, Op::Mark { index, marks }, ops);
            index += 1;
        }
    }
    skipped
}

/// Moves the scenes change `c` moved back to where they stood. Returns
/// false when a move could not be undone because its place is gone; its
/// mark then stays.
fn undo_moves(
    work: &mut Document,
    ops: &mut Vec<Op>,
    c: ChangeId,
    in_scope: &dyn Fn(BlockId) -> bool,
) -> bool {
    let moves: Vec<(BlockId, Option<BlockId>, BlockId)> = work
        .blocks
        .iter()
        .filter(|b| in_scope(b.id))
        .flat_map(|b| {
            b.marks
                .moved
                .iter()
                .filter(|mv| mv.change == c)
                .map(|mv| (b.id, mv.before, mv.last))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut all = true;
    for (heading, before, last) in moves {
        let Some(start) = work.index_of(heading) else {
            all = false;
            continue;
        };
        // The blocks the move took: up to `last`, or the scene as it now
        // stands when `last` is gone.
        let end = match work.index_of(last) {
            Some(l) if l >= start => l + 1,
            _ => match work.scene(heading) {
                Some(s) => s.end,
                None => {
                    all = false;
                    continue;
                }
            },
        };
        let scene = crate::document::Scene { start, end };
        let target = match before {
            None => Some(work.blocks.len()),
            Some(b) => work.index_of(b).filter(|&t| {
                matches!(
                    work.blocks[t].element.kind,
                    ElementKind::SceneHeading | ElementKind::Section
                ) && (t <= scene.start || t >= scene.end)
            }),
        };
        let Some(target) = target else {
            all = false;
            continue;
        };
        let index = scene.start;
        let mut marks = work.blocks[index].marks.clone();
        marks.moved.retain(|mv| mv.change != c);
        apply(work, Op::Mark { index, marks }, ops);
        if target != scene.start && target != scene.end {
            let len = scene.end - scene.start;
            let to = if target > scene.start {
                target - len
            } else {
                target
            };
            apply(
                work,
                Op::Move {
                    from: scene.start,
                    len,
                    to,
                },
                ops,
            );
        }
    }
    all
}

/// Gives the blocks change `c` retyped their type and kept attributes back.
fn restore_retypes(
    work: &mut Document,
    ops: &mut Vec<Op>,
    c: ChangeId,
    in_scope: &dyn Fn(BlockId) -> bool,
) {
    for index in 0..work.blocks.len() {
        let b = &work.blocks[index];
        if !in_scope(b.id) {
            continue;
        }
        let Some(r) = b
            .marks
            .retyped
            .iter()
            .rev()
            .find(|r| r.change == c)
            .cloned()
        else {
            continue;
        };
        let mut e = b.element.clone();
        e.kind = r.from;
        e.clear_inapplicable();
        if r.from == ElementKind::SceneHeading {
            e.scene_number = r.scene_number.clone();
        }
        if r.from == ElementKind::Character {
            e.dual = r.dual;
        }
        if r.from == ElementKind::Action {
            e.centered = r.centered;
        }
        if r.from == ElementKind::Section && r.depth > 0 {
            e.depth = r.depth;
        }
        let mut marks = b.marks.clone();
        marks.retyped.retain(|x| x.change != c);
        if e != work.blocks[index].element {
            apply(work, Op::Replace { index, element: e }, ops);
        }
        apply(work, Op::Mark { index, marks }, ops);
    }
}

/// Whether change `c` marks anything besides a move.
fn has_other_marks(doc: &Document, c: ChangeId) -> bool {
    doc.blocks.iter().any(|b| {
        crate::changes::text_ids(&b.element.text).any(|x| x == c)
            || [
                b.marks.inserted,
                b.marks.deleted,
                b.marks.split,
                b.marks.joined,
            ]
            .contains(&Some(c))
            || b.marks.retyped.iter().any(|r| r.change == c)
    })
}

// ---- Listing ---------------------------------------------------------------

/// The longest excerpt `changes.list` gives, in characters.
const EXCERPT: usize = 200;

fn excerpt(s: &str) -> String {
    let mut out: String = s.chars().take(EXCERPT).collect();
    if s.chars().count() > EXCERPT {
        out.push('…');
    }
    out
}

/// The pending changes, as `changes.list` lists them (docs/specs/changes.md
/// §8), optionally only those with a mark in the blocks `only` names.
pub fn list(doc: &Document, only: Option<&BTreeSet<BlockId>>) -> Vec<Value> {
    #[derive(Default)]
    struct Row {
        kinds: BTreeSet<u8>,
        blocks: Vec<BlockId>,
        first: Option<usize>,
        inserted: Vec<String>,
        deleted: Vec<String>,
    }
    const KINDS: [&str; 8] = [
        "insert",
        "delete",
        "block_insert",
        "block_delete",
        "retype",
        "split",
        "join",
        "move",
    ];
    let mut rows: BTreeMap<ChangeId, Row> = BTreeMap::new();
    let mut order: Vec<ChangeId> = Vec::new();
    for (index, b) in doc.blocks.iter().enumerate() {
        let mut here: Vec<(ChangeId, u8)> = Vec::new();
        let mut ins: BTreeMap<ChangeId, String> = BTreeMap::new();
        let mut del: BTreeMap<ChangeId, String> = BTreeMap::new();
        for span in b.element.text.marked_spans() {
            here.push((span.change, if span.deleted { 1 } else { 0 }));
            let m = if span.deleted { &mut del } else { &mut ins };
            m.entry(span.change).or_default().push_str(&span.text);
        }
        let m = &b.marks;
        for (c, k) in [(m.inserted, 2), (m.deleted, 3), (m.split, 5), (m.joined, 6)] {
            if let Some(c) = c {
                here.push((c, k));
            }
        }
        here.extend(m.retyped.iter().map(|r| (r.change, 4)));
        here.extend(m.moved.iter().map(|r| (r.change, 7)));
        for (c, k) in here {
            let row = rows.entry(c).or_insert_with(|| {
                order.push(c);
                Row::default()
            });
            row.kinds.insert(k);
            if row.blocks.last() != Some(&b.id) {
                row.blocks.push(b.id);
            }
            row.first.get_or_insert(index);
        }
        for (c, t) in ins {
            if let Some(r) = rows.get_mut(&c) {
                r.inserted.push(t);
            }
        }
        for (c, t) in del {
            if let Some(r) = rows.get_mut(&c) {
                r.deleted.push(t);
            }
        }
    }
    let scenes = doc.scenes();
    order
        .into_iter()
        .filter_map(|c| {
            let row = rows.remove(&c)?;
            if let Some(only) = only
                && !row.blocks.iter().any(|b| only.contains(b))
            {
                return None;
            }
            let first = row.first.unwrap_or(0);
            let scene = scenes
                .iter()
                .rev()
                .find(|s| s.start <= first)
                .filter(|s| first < s.end)
                .map(|s| doc.blocks[s.start].id);
            let entry = doc.changes.get(c);
            Some(json!({
                "id": c,
                "by": entry.map(|e| &e.by),
                "t": entry.map_or(0, |e| e.t),
                "kinds": row.kinds.iter().map(|&k| KINDS[k as usize]).collect::<Vec<_>>(),
                "blocks": row.blocks,
                "scene": scene,
                "inserted": excerpt(&row.inserted.join(" ")),
                "deleted": excerpt(&row.deleted.join(" ")),
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn set_text_marks_only_the_words_that_changed() {
        let old = RichText::plain("She walks home slowly.");
        let new = RichText::plain("She runs home.");
        let none = |_: ChangeId| false;
        let t = set_text(&old, &new, ChangeId(1), &none);
        assert_eq!(t.plain_text(), "She runs home.");
        assert_eq!(
            t.runs(),
            &[
                Run::plain("She "),
                Run::marked("walks", Style::PLAIN, del(1)),
                Run::marked("runs", Style::PLAIN, ins(1)),
                Run::plain(" home"),
                Run::marked(" slowly", Style::PLAIN, del(1)),
                Run::plain("."),
            ]
        );
    }

    #[test]
    fn set_text_keeps_marks_on_shared_words_and_old_deletions() {
        let old = RichText::new(vec![
            Run::marked("Rain ", Style::PLAIN, ins(1)),
            Run::marked("hard ", Style::PLAIN, del(2)),
            Run::plain("falls."),
        ]);
        let new = RichText::plain("Rain falls now.");
        let none = |_: ChangeId| false;
        let t = set_text(&old, &new, ChangeId(3), &none);
        assert_eq!(t.plain_text(), "Rain falls now.");
        assert_eq!(
            t.runs(),
            &[
                Run::marked("Rain ", Style::PLAIN, ins(1)),
                Run::marked("hard ", Style::PLAIN, del(2)),
                Run::plain("falls"),
                Run::marked(" now", Style::PLAIN, ins(3)),
                Run::plain("."),
            ]
        );
        // The author's own insertion is dropped, not struck.
        let mine = |c: ChangeId| c == ChangeId(1);
        let t = set_text(&old, &RichText::plain("falls."), ChangeId(1), &mine);
        assert_eq!(
            t.runs(),
            &[
                Run::marked("hard ", Style::PLAIN, del(2)),
                Run::plain("falls."),
            ]
        );
    }

    #[test]
    fn set_text_takes_new_styles_and_notes() {
        let b = Style {
            bold: true,
            ..Style::PLAIN
        };
        let old = RichText::plain("a b");
        let new = RichText::new(vec![
            Run::styled("a", b),
            Run::Note("n".into()),
            Run::plain(" b"),
        ]);
        let none = |_: ChangeId| false;
        let t = set_text(&old, &new, ChangeId(1), &none);
        assert_eq!(
            t, new,
            "a restyle with a note added is not a tracked text change"
        );
    }

    #[test]
    fn a_huge_middle_falls_back_to_one_deletion_and_one_insertion() {
        let a: String = (0..1500).map(|i| format!("w{i} ")).collect();
        let b: String = (0..1500).map(|i| format!("x{i} ")).collect();
        let none = |_: ChangeId| false;
        let t = set_text(
            &RichText::plain(a.clone()),
            &RichText::plain(b.clone()),
            ChangeId(1),
            &none,
        );
        assert_eq!(t.plain_text(), b);
        assert_eq!(t.marked_spans().len(), 2);
    }
}
