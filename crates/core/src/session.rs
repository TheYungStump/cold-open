//! An open document with its undo history and journal. Edits, queries and
//! undo/redo run here; opening and saving live in the dispatcher.

use std::path::PathBuf;

use coldopen_screenplay::{ElementKind, RichText, TextRepr};
use serde_json::{Value, json};

use crate::command::{
    BlockDelete, BlockInsert, BlockSetAttrs, BlockSetText, BlockSetType, Class, Command, SceneMove,
    TextReplace,
};
use crate::document::{Block, BlockId, Document, word_count};
use crate::error::CommandError;
use crate::io::SourceStyle;
use crate::journal::Journal;
use crate::ops::{Op, Transaction};
use crate::replace::replace_in;

/// One step of history: the command's name and the transaction that
/// reverses (for undo) or repeats (for redo) it.
#[derive(Clone, Debug)]
struct Step {
    name: String,
    tx: Transaction,
}

#[derive(Debug)]
pub struct Session {
    pub doc: Document,
    pub path: Option<PathBuf>,
    pub style: SourceStyle,
    pub journal: Option<Journal>,
    /// True when there are changes since the last open or save.
    pub dirty: bool,
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl Session {
    pub fn new(doc: Document, path: Option<PathBuf>, style: SourceStyle) -> Session {
        Session {
            doc,
            path,
            style,
            journal: None,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Runs a query, edit or history command.
    pub fn execute(&mut self, cmd: &Command) -> Result<Value, CommandError> {
        match cmd.class() {
            Class::Query => self.query(cmd),
            Class::Edit => self.edit(cmd),
            Class::History => self.history(cmd),
            Class::Session => Err(CommandError::new(
                "invalid_request",
                format!("{} is handled by the dispatcher", cmd.name()),
            )),
        }
    }

    fn query(&self, cmd: &Command) -> Result<Value, CommandError> {
        match cmd {
            Command::DocGet(_) => Ok(json!({
                "path": self.path.as_ref().map(|p| p.display().to_string()),
                "dirty": self.dirty,
                "title_page": self.doc.title_page,
                "blocks": self.doc.blocks,
            })),
            Command::SceneList(_) => Ok(json!({ "scenes": self.scene_list() })),
            Command::DocExport(p) => match p.format.to_ascii_lowercase().as_str() {
                "fountain" => {
                    let written = coldopen_fountain::write(&self.doc.to_script());
                    Ok(json!({
                        "format": "fountain",
                        "text": written.text,
                        "losses": self.losses_json(&written.losses),
                    }))
                }
                "json" => {
                    let content = json!({
                        "title_page": self.doc.title_page,
                        "blocks": self.doc.blocks,
                    });
                    let text = serde_json::to_string_pretty(&content)
                        .map_err(|e| CommandError::new("invalid_request", e.to_string()))?;
                    Ok(json!({ "format": "json", "text": text + "
", "losses": [] }))
                }
                other => Err(CommandError::new(
                    "unsupported_format",
                    format!(
                        "\"{other}\" is not a supported export format; use \"fountain\" or \"json\""
                    ),
                )),
            },
            _ => Err(CommandError::new("invalid_request", "not a query")),
        }
    }

    /// Writer losses with element indices turned into block ids.
    pub fn losses_json(&self, losses: &[coldopen_fountain::Loss]) -> Vec<Value> {
        losses
            .iter()
            .map(|l| {
                let id = l.element.and_then(|i| self.doc.blocks.get(i)).map(|b| b.id);
                json!({ "id": id, "code": l.code, "message": l.message })
            })
            .collect()
    }

    pub fn scene_list(&self) -> Vec<Value> {
        self.doc
            .scenes()
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let blocks = &self.doc.blocks[s.start..s.end];
                let head = &blocks[0];
                let synopsis = blocks
                    .iter()
                    .find(|b| b.element.kind == ElementKind::Synopsis)
                    .map(|b| b.element.text.plain_text());
                json!({
                    "id": head.id,
                    "index": k + 1,
                    "number": head.element.scene_number,
                    "heading": head.element.text.plain_text(),
                    "synopsis": synopsis,
                    "blocks": blocks.len(),
                    "words": blocks.iter().map(|b| word_count(&b.element)).sum::<usize>(),
                })
            })
            .collect()
    }

    fn edit(&mut self, cmd: &Command) -> Result<Value, CommandError> {
        let next_id = self.doc.next_id;
        let planned = self.plan(cmd);
        let (tx, result) = match planned {
            Ok(p) => p,
            Err(e) => {
                self.doc.next_id = next_id;
                return Err(e);
            }
        };
        if tx.is_empty() {
            self.doc.next_id = next_id;
            return Ok(result);
        }
        let inverse = tx.apply(&mut self.doc);
        if let Err(e) = self.journal_append(cmd.name(), cmd.params(), &result) {
            inverse.apply(&mut self.doc);
            self.doc.next_id = next_id;
            return Err(e);
        }
        self.undo.push(Step {
            name: cmd.name().to_string(),
            tx: inverse,
        });
        self.redo.clear();
        self.dirty = true;
        Ok(result)
    }

    fn history(&mut self, cmd: &Command) -> Result<Value, CommandError> {
        let is_undo = matches!(cmd, Command::Undo(_));
        let popped = if is_undo {
            self.undo.pop()
        } else {
            self.redo.pop()
        };
        let Some(step) = popped else {
            return Err(if is_undo {
                CommandError::new("nothing_to_undo", "there is nothing to undo")
            } else {
                CommandError::new("nothing_to_redo", "there is nothing to redo")
            });
        };
        let reverse = step.tx.apply(&mut self.doc);
        let key = if is_undo { "undone" } else { "redone" };
        let result = json!({ key: step.name });
        if let Err(e) = self.journal_append(cmd.name(), cmd.params(), &result) {
            let restore = reverse.apply(&mut self.doc);
            let back = Step {
                name: step.name,
                tx: restore,
            };
            if is_undo {
                self.undo.push(back);
            } else {
                self.redo.push(back);
            }
            return Err(e);
        }
        let back = Step {
            name: step.name,
            tx: reverse,
        };
        if is_undo {
            self.redo.push(back);
        } else {
            self.undo.push(back);
        }
        self.dirty = true;
        Ok(result)
    }

    fn journal_append(
        &mut self,
        name: &str,
        params: Value,
        result: &Value,
    ) -> Result<(), CommandError> {
        if let Some(j) = &mut self.journal {
            j.append(name, params, result).map_err(|e| {
                CommandError::new(
                    "journal_error",
                    format!(
                        "the crash journal could not be written, so the change was undone: {e}"
                    ),
                )
            })?;
        }
        Ok(())
    }

    fn index(&self, id: BlockId) -> Result<usize, CommandError> {
        self.doc
            .index_of(id)
            .ok_or(CommandError::block_not_found(id))
    }

    fn check(element: &coldopen_screenplay::Element) -> Result<(), CommandError> {
        let problems = element.problems();
        if problems.is_empty() {
            Ok(())
        } else {
            Err(CommandError::new("invalid_attr", problems.join("; "))
                .with_details(json!({ "problems": problems })))
        }
    }

    /// Turns an edit into operations and its result, without changing the
    /// document (apart from handing out ids).
    fn plan(&mut self, cmd: &Command) -> Result<(Transaction, Value), CommandError> {
        match cmd {
            Command::BlockInsert(p) => self.plan_insert(p),
            Command::BlockSetType(p) => self.plan_set_type(p),
            Command::BlockSetText(p) => self.plan_set_text(p),
            Command::BlockSetAttrs(p) => self.plan_set_attrs(p),
            Command::BlockDelete(p) => self.plan_delete(p),
            Command::TextReplace(p) => self.plan_replace(p),
            Command::SceneMove(p) => self.plan_move(p),
            _ => Err(CommandError::new("invalid_request", "not an edit")),
        }
    }

    fn plan_insert(&mut self, p: &BlockInsert) -> Result<(Transaction, Value), CommandError> {
        let element = p
            .element_repr()
            .into_element()
            .map_err(CommandError::invalid_params)?;
        Self::check(&element)?;
        let index = match (p.after, p.before) {
            (Some(_), Some(_)) => {
                return Err(CommandError::invalid_params(
                    "give \"after\" or \"before\", not both",
                ));
            }
            (Some(a), None) => self.index(a)? + 1,
            (None, Some(b)) => self.index(b)?,
            (None, None) => self.doc.blocks.len(),
        };
        let id = self.doc.allocate_id();
        Ok((
            Transaction(vec![Op::Insert {
                index,
                block: Block { id, element },
            }]),
            json!({ "id": id }),
        ))
    }

    fn replace_op(&self, index: usize, element: coldopen_screenplay::Element) -> Transaction {
        if self.doc.blocks[index].element == element {
            Transaction::default()
        } else {
            Transaction(vec![Op::Replace { index, element }])
        }
    }

    fn plan_set_type(&mut self, p: &BlockSetType) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        let mut e = self.doc.blocks[index].element.clone();
        e.kind = p.kind;
        e.clear_inapplicable();
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_set_text(&mut self, p: &BlockSetText) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        if p.text.is_none() && p.runs.is_none() {
            return Err(CommandError::invalid_params("give \"text\" or \"runs\""));
        }
        let text: RichText = TextRepr {
            text: p.text.clone(),
            runs: p.runs.clone(),
        }
        .into_rich()
        .map_err(CommandError::invalid_params)?;
        let mut e = self.doc.blocks[index].element.clone();
        e.text = text;
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_set_attrs(&mut self, p: &BlockSetAttrs) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        let mut e = self.doc.blocks[index].element.clone();
        if let Some(n) = &p.scene_number {
            e.scene_number = n.clone();
        }
        if let Some(d) = p.dual {
            e.dual = d;
        }
        if let Some(c) = p.centered {
            e.centered = c;
        }
        if let Some(d) = p.depth {
            e.depth = d;
        }
        if let Some(s) = p.space_before {
            e.space_before = s;
        }
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_delete(&mut self, p: &BlockDelete) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        Ok((Transaction(vec![Op::Remove { index }]), json!({})))
    }

    fn plan_replace(&mut self, p: &TextReplace) -> Result<(Transaction, Value), CommandError> {
        if p.find.is_empty() {
            return Err(CommandError::invalid_params("\"find\" must not be empty"));
        }
        let range = match &p.scope {
            None => 0..self.doc.blocks.len(),
            Some(s) => {
                let scene = self
                    .doc
                    .scene(s.scene)
                    .ok_or(CommandError::scene_not_found(s.scene))?;
                scene.start..scene.end
            }
        };
        let mut ops = Vec::new();
        let mut changed = Vec::new();
        let mut count = 0;
        for index in range {
            let b = &self.doc.blocks[index];
            if b.element.kind.is_raw_text() {
                continue;
            }
            if let Some((text, n)) = replace_in(&b.element.text, &p.find, &p.with, p.match_case) {
                let mut e = b.element.clone();
                e.text = text;
                if e != b.element {
                    Self::check(&e)?;
                    changed.push(b.id);
                    ops.push(Op::Replace { index, element: e });
                }
                count += n;
            }
        }
        Ok((
            Transaction(ops),
            json!({ "count": count, "blocks": changed }),
        ))
    }

    fn plan_move(&mut self, p: &SceneMove) -> Result<(Transaction, Value), CommandError> {
        let scene = self
            .doc
            .scene(p.id)
            .ok_or(CommandError::scene_not_found(p.id))?;
        let target = match p.before {
            None => self.doc.blocks.len(),
            Some(b) => {
                let t = self.index(b)?;
                let kind = self.doc.blocks[t].element.kind;
                if !matches!(kind, ElementKind::SceneHeading | ElementKind::Section) {
                    return Err(CommandError::new(
                        "scene_not_found",
                        format!("{b} is not a scene heading or section"),
                    )
                    .with_details(json!({ "id": b })));
                }
                t
            }
        };
        if target > scene.start && target < scene.end {
            return Err(CommandError::invalid_params(
                "a scene cannot move inside itself",
            ));
        }
        if target == scene.start || target == scene.end {
            return Ok((Transaction::default(), json!({})));
        }
        let len = scene.end - scene.start;
        let to = if target > scene.start {
            target - len
        } else {
            target
        };
        Ok((
            Transaction(vec![Op::Move {
                from: scene.start,
                len,
                to,
            }]),
            json!({}),
        ))
    }
}
