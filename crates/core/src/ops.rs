//! Primitive document operations. Every change to a [`Document`] goes
//! through [`Op::apply`], which returns the exact inverse; undo and redo are
//! built from those inverses.

use coldopen_screenplay::{Element, TitleLine, TitlePage};
use serde::{Deserialize, Serialize};

use crate::changes::{BlockMarks, Tracking};
use crate::document::{Block, Document};
use crate::revisions::{RevisionId, Revisions};

/// Serialized, as in a journal entry that carries a step from before the
/// journal's header (docs/specs/journal.md, "Undo and redo entries").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    Insert {
        index: usize,
        block: Block,
    },
    Remove {
        index: usize,
    },
    Replace {
        index: usize,
        element: Element,
    },
    /// Removes `len` blocks starting at `from`, then inserts them at `to`
    /// (an index into the list after removal).
    Move {
        from: usize,
        len: usize,
        to: usize,
    },
    /// Replaces a block's structural tracked-change marks (2026-10-18).
    Mark {
        index: usize,
        marks: BlockMarks,
    },
    /// Replaces the document's tracking switches (2026-10-18).
    Track {
        tracking: Tracking,
    },
    /// Replaces a block's revision mark (2026-10-22).
    Rev {
        index: usize,
        rev: Option<RevisionId>,
    },
    /// Replaces the revision table (2026-10-22).
    Revisions {
        revisions: Box<Revisions>,
    },
    /// Replaces the title page, both its keys and its lines (2026-10-22).
    TitlePage {
        title_page: TitlePage,
        title_lines: Vec<TitleLine>,
    },
}

impl Op {
    /// Applies the operation and returns its inverse. Callers validate
    /// indices first; an out-of-range operation is ignored and returns a
    /// no-op inverse rather than panicking.
    pub fn apply(self, doc: &mut Document) -> Op {
        let n = doc.blocks.len();
        match self {
            Op::Insert { index, block } => {
                let index = index.min(n);
                doc.blocks.insert(index, block);
                Op::Remove { index }
            }
            Op::Remove { index } if index < n => {
                let block = doc.blocks.remove(index);
                Op::Insert { index, block }
            }
            Op::Replace { index, element } if index < n => {
                let old = std::mem::replace(&mut doc.blocks[index].element, element);
                Op::Replace {
                    index,
                    element: old,
                }
            }
            Op::Move { from, len, to } if from + len <= n && to + len <= n => {
                let moved: Vec<Block> = doc.blocks.drain(from..from + len).collect();
                doc.blocks.splice(to..to, moved);
                Op::Move {
                    from: to,
                    len,
                    to: from,
                }
            }
            Op::Mark { index, marks } if index < n => {
                let old = std::mem::replace(&mut doc.blocks[index].marks, marks);
                Op::Mark { index, marks: old }
            }
            Op::Track { tracking } => {
                let old = std::mem::replace(&mut doc.tracking, tracking);
                Op::Track { tracking: old }
            }
            Op::Rev { index, rev } if index < n => {
                let old = std::mem::replace(&mut doc.blocks[index].rev, rev);
                Op::Rev { index, rev: old }
            }
            Op::Revisions { revisions } => {
                let old = std::mem::replace(&mut doc.revisions, *revisions);
                Op::Revisions {
                    revisions: Box::new(old),
                }
            }
            Op::TitlePage {
                title_page,
                title_lines,
            } => Op::TitlePage {
                title_page: std::mem::replace(&mut doc.title_page, title_page),
                title_lines: std::mem::replace(&mut doc.title_lines, title_lines),
            },
            _ => Op::Move {
                from: 0,
                len: 0,
                to: 0,
            },
        }
    }
}

/// A group of operations applied as one undoable step.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction(pub Vec<Op>);

impl Transaction {
    /// Applies all operations in order and returns the inverse transaction.
    pub fn apply(self, doc: &mut Document) -> Transaction {
        let mut inverse: Vec<Op> = self.0.into_iter().map(|op| op.apply(doc)).collect();
        inverse.reverse();
        Transaction(inverse)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::BlockId;
    use coldopen_screenplay::ElementKind;

    fn doc(n: u64) -> Document {
        Document {
            blocks: (1..=n)
                .map(|i| {
                    Block::new(
                        BlockId(i),
                        Element::new(ElementKind::Action, format!("{i}")),
                    )
                })
                .collect(),
            next_id: n + 1,
            ..Default::default()
        }
    }

    fn ids(d: &Document) -> Vec<u64> {
        d.blocks.iter().map(|b| b.id.0).collect()
    }

    #[test]
    fn inverses_restore_the_document() {
        let ops = vec![
            Op::Insert {
                index: 1,
                block: Block::new(BlockId(9), Element::new(ElementKind::Dialogue, "x")),
            },
            Op::Mark {
                index: 0,
                marks: BlockMarks {
                    deleted: Some(crate::changes::ChangeId(1)),
                    ..Default::default()
                },
            },
            Op::Track {
                tracking: Tracking {
                    writer: true,
                    agents: false,
                },
            },
            Op::Remove { index: 0 },
            Op::Replace {
                index: 2,
                element: Element::new(ElementKind::Character, "Y"),
            },
            Op::Move {
                from: 0,
                len: 2,
                to: 3,
            },
        ];
        let original = doc(5);
        let mut d = original.clone();
        let inverse = Transaction(ops).apply(&mut d);
        assert_ne!(d, original);
        inverse.apply(&mut d);
        assert_eq!(d, original);
    }

    #[test]
    fn move_semantics() {
        let mut d = doc(5);
        Op::Move {
            from: 0,
            len: 2,
            to: 3,
        }
        .apply(&mut d);
        assert_eq!(ids(&d), vec![3, 4, 5, 1, 2]);
    }
}
