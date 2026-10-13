//! Primitive document operations. Every change to a [`Document`] goes
//! through [`Op::apply`], which returns the exact inverse; undo and redo are
//! built from those inverses.

use coldopen_screenplay::Element;

use crate::document::{Block, Document};

#[derive(Clone, Debug, PartialEq, Eq)]
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
            _ => Op::Move {
                from: 0,
                len: 0,
                to: 0,
            },
        }
    }
}

/// A group of operations applied as one undoable step.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
                .map(|i| Block {
                    id: BlockId(i),
                    element: Element::new(ElementKind::Action, format!("{i}")),
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
                block: Block {
                    id: BlockId(9),
                    element: Element::new(ElementKind::Dialogue, "x"),
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
