//! The rules of journal semantics 1 to 3, kept so a journal written under
//! them replays exactly as it was made (docs/specs/journal.md, "Header").
//! Semantics 4 (2026-10-28, the fixes of the reviews of phases 4–5b)
//! changed them; live commands never run these. Nothing here may change:
//! `crates/core/tests/old_journals.rs` replays the phase 5b build's journal
//! through them.

use std::collections::BTreeSet;

use coldopen_screenplay::{ElementKind, Settle};

use crate::changes::ChangeId;
use crate::document::{Block, BlockId, Document};
use crate::ops::Op;
use crate::revisions::RevisionId;
use crate::track::{Selection, Settled};

/// Whether a tracked split makes the new block an insertion rather than a
/// split: before semantics 4, any split of the author's own pending
/// inserted block did, so rejecting that split deleted the text it took.
pub fn split_is_insertion(empty: bool, own_block: bool) -> bool {
    empty || own_block
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
        tags: Vec::new(),
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
        if !r.from.is_transparent() && r.space_before.is_some() {
            e.space_before = r.space_before;
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

/// Before semantics 4: the newest revision set recorded by the changes an
/// edit settled: those
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
