//! An open document with its undo history and journal. Edits, queries and
//! undo/redo run here; opening and saving live in the dispatcher.

use std::path::PathBuf;

use coldopen_screenplay::{ElementKind, RangeError, RichText, StyleChange, TextRepr};
use serde_json::{Value, json};

use crate::author::{Author, Clock};
use crate::command::{
    Batch, BlockDelete, BlockInsert, BlockJoin, BlockSetAttrs, BlockSetText, BlockSetType,
    BlockSplit, Class, Command, SceneMove, TextDelete, TextInsert, TextReplace, TextStyle,
};
use crate::document::{Block, BlockId, Document, word_count};
use crate::error::CommandError;
use crate::io::SourceStyle;
use crate::journal::Journal;
use crate::ops::{Op, Transaction};
use crate::replace::{find_in, replace_in, replace_one};

/// Typing within this many milliseconds of the last keystroke joins the
/// same undo step (docs/specs/commands.md, "Authors and undo steps").
pub const GROUP_MS: u64 = 1500;

/// How a command runs: who sent it, and whether it may join the undo step
/// before it.
#[derive(Clone, Debug, Default)]
pub struct Exec {
    pub by: Author,
    pub merge: Merge,
}

impl Exec {
    pub fn by(by: Author) -> Exec {
        Exec {
            by,
            merge: Merge::Auto,
        }
    }
}

/// Whether a keystroke command joins the typing step before it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Merge {
    /// Decide by the grouping rules and the clock.
    #[default]
    Auto,
    /// Replaying a journal: do what the entry recorded.
    Recorded(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupKind {
    Insert,
    Delete,
}

/// The open typing group on top of the undo stack.
#[derive(Clone, Copy, Debug)]
struct Group {
    kind: GroupKind,
    block: BlockId,
    index: usize,
    /// Insert: where the next insert must start. Delete: where the last
    /// delete started.
    mark: usize,
    last_ms: u64,
}

/// One step of history: what it was, who made it, and the transaction that
/// reverses (for undo) or repeats (for redo) it.
#[derive(Clone, Debug)]
struct Step {
    command: String,
    label: String,
    by: Author,
    tx: Transaction,
    group: Option<Group>,
}

#[derive(Debug)]
pub struct Session {
    pub doc: Document,
    pub path: Option<PathBuf>,
    pub style: SourceStyle,
    pub journal: Option<Journal>,
    /// True when there are changes since the last open or save.
    pub dirty: bool,
    /// Edits, undos, redos and reverts applied since the document was
    /// opened or created. Never goes down.
    pub rev: u64,
    pub clock: Clock,
    undo: Vec<Step>,
    redo: Vec<Step>,
}

fn range_error(e: RangeError) -> CommandError {
    CommandError::new("invalid_range", e.message).with_details(json!({ "len": e.len }))
}

/// Adds `index` to an error's details, for `batch`.
fn at_index(mut e: CommandError, index: usize) -> CommandError {
    match &mut e.details {
        Some(Value::Object(m)) => {
            m.insert("index".into(), json!(index));
        }
        _ => e.details = Some(json!({ "index": index })),
    }
    e
}

/// A writer's label for a command (docs/specs/commands.md).
fn writer_label(cmd: &Command) -> String {
    match cmd {
        Command::BlockInsert(_) => "Insert block".into(),
        Command::BlockSetType(_) => "Change element".into(),
        Command::BlockSetText(_) => "Edit text".into(),
        Command::BlockSetAttrs(_) => "Change attributes".into(),
        Command::BlockDelete(_) => "Delete block".into(),
        Command::TextReplace(_) => "Replace".into(),
        Command::SceneMove(_) => "Move scene".into(),
        Command::TextInsert(_) => "Typing".into(),
        Command::TextDelete(_) => "Delete".into(),
        Command::TextStyle(_) => "Style".into(),
        Command::BlockSplit(_) => "Split block".into(),
        Command::BlockJoin(_) => "Join blocks".into(),
        Command::Batch(b) => b.label.clone().unwrap_or_else(|| "Batch".into()),
        other => other.name().into(),
    }
}

fn label(cmd: &Command, by: &Author) -> String {
    match by {
        Author::Writer => writer_label(cmd),
        Author::Agent { name, tool } => {
            let tool = tool.clone().unwrap_or_else(|| cmd.name().replace('.', "_"));
            format!("{name} · {tool}")
        }
    }
}

impl Session {
    pub fn new(doc: Document, path: Option<PathBuf>, style: SourceStyle) -> Session {
        Session {
            doc,
            path,
            style,
            journal: None,
            dirty: false,
            rev: 0,
            clock: Clock::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Runs a query, edit or history command as a writer.
    pub fn execute(&mut self, cmd: &Command) -> Result<Value, CommandError> {
        self.execute_with(cmd, &Exec::default())
    }

    /// Runs a query, edit or history command.
    pub fn execute_with(&mut self, cmd: &Command, ex: &Exec) -> Result<Value, CommandError> {
        match cmd.class() {
            Class::Query => self.query(cmd),
            Class::Edit => self.edit(cmd, ex),
            Class::History => self.history(cmd, ex),
            Class::Session | Class::Output => Err(CommandError::new(
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
                "rev": self.rev,
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
                    Ok(json!({ "format": "json", "text": text + "\n", "losses": [] }))
                }
                "pages" => {
                    let (layout, _) = self.layout(&coldopen_layout::Options::default());
                    let text = match coldopen_layout::Fonts::new() {
                        Ok(fonts) => layout.page_view(&fonts),
                        Err(_) => layout.page_view(&coldopen_layout::AllGlyphs),
                    };
                    Ok(json!({ "format": "pages", "text": text, "losses": [] }))
                }
                other => Err(CommandError::new(
                    "unsupported_format",
                    format!(
                        "\"{other}\" is not a supported export format; use \"fountain\", \"json\" or \"pages\""
                    ),
                )),
            },
            Command::TextFind(p) => {
                if p.find.is_empty() {
                    return Err(CommandError::invalid_params("\"find\" must not be empty"));
                }
                let range = self.scope_range(p.scope.as_ref().map(|s| s.scene))?;
                let len = p.find.chars().count();
                let mut matches = Vec::new();
                for b in &self.doc.blocks[range] {
                    if b.element.kind.is_raw_text() {
                        continue;
                    }
                    for s in find_in(&b.element.text, &p.find, p.match_case) {
                        matches.push(json!({ "id": b.id, "from": s, "to": s + len }));
                    }
                }
                Ok(json!({ "matches": matches }))
            }
            Command::TextComplete(p) => {
                let index = self.index(p.id)?;
                let len = self.doc.blocks[index].element.text.char_len();
                if p.at > len {
                    return Err(CommandError::new(
                        "invalid_range",
                        format!(
                            "offset {} is past the end of the text ({len} characters)",
                            p.at
                        ),
                    )
                    .with_details(json!({ "len": len })));
                }
                let c = crate::complete::complete(&self.doc, index, p.at);
                serde_json::to_value(c)
                    .map_err(|e| CommandError::new("internal_error", e.to_string()))
            }
            Command::HistoryList(_) => {
                let list = |steps: &[Step]| -> Vec<Value> {
                    steps
                        .iter()
                        .rev()
                        .map(|s| json!({ "command": s.command, "label": s.label, "by": s.by }))
                        .collect()
                };
                Ok(json!({ "undo": list(&self.undo), "redo": list(&self.redo) }))
            }
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

    /// Lays out the document with the default page format. Layout runs on
    /// the Fountain normal form, so the pages are the same after a save and
    /// reload (docs/specs/layout.md §4.8). The second value maps each laid-out
    /// element to the index of the block it came from.
    pub fn layout(
        &self,
        options: &coldopen_layout::Options,
    ) -> (coldopen_layout::Layout, Vec<usize>) {
        let (script, map) = coldopen_fountain::normalize_with_map(&self.doc.to_script());
        let layout =
            coldopen_layout::layout(&script, &coldopen_layout::ValidFormat::default(), options);
        (layout, map)
    }

    pub fn scene_list(&self) -> Vec<Value> {
        let (layout, map) = self.layout(&coldopen_layout::Options::default());
        // Laid-out elements whose source block falls in [start, end).
        let elements = |start: usize, end: usize| {
            map.partition_point(|&b| b < start)..map.partition_point(|&b| b < end)
        };
        self.doc
            .scenes()
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let range = elements(s.start, s.end);
                let page = range.clone().find_map(|e| layout.first_page(e));
                let eighths = layout.eighths(range);
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
                    "page": page,
                    "eighths": eighths,
                })
            })
            .collect()
    }

    /// The undo steps, newest first: (command, label, author).
    pub fn undo_steps(&self) -> impl Iterator<Item = (&str, &str, &Author)> {
        self.undo
            .iter()
            .rev()
            .map(|s| (s.command.as_str(), s.label.as_str(), &s.by))
    }

    /// The redo steps, newest first: (command, label, author).
    pub fn redo_steps(&self) -> impl Iterator<Item = (&str, &str, &Author)> {
        self.redo
            .iter()
            .rev()
            .map(|s| (s.command.as_str(), s.label.as_str(), &s.by))
    }

    fn edit(&mut self, cmd: &Command, ex: &Exec) -> Result<Value, CommandError> {
        let next_id = self.doc.next_id;
        let (inverse, result) = match self.apply_edit(cmd) {
            Ok(r) => r,
            Err(e) => {
                self.doc.next_id = next_id;
                return Err(e);
            }
        };
        let Some(inverse) = inverse else {
            self.doc.next_id = next_id;
            return Ok(result);
        };
        let now = self.clock.now_ms();
        let merge = self.should_merge(cmd, ex, now, &inverse);
        if let Err(e) = self.journal_append(cmd.name(), cmd.params(), &result, &ex.by, merge) {
            inverse.apply(&mut self.doc);
            self.doc.next_id = next_id;
            return Err(e);
        }
        let group = if ex.by.is_writer() {
            self.group_of(cmd, now)
        } else {
            None
        };
        match self.undo.last_mut() {
            Some(top) if merge => {
                // The step already restores the block as it was before the
                // group started; only the group's end moves.
                top.group = group;
                top.command = "typing".into();
            }
            _ => self.undo.push(Step {
                command: cmd.name().to_string(),
                label: label(cmd, &ex.by),
                by: ex.by.clone(),
                tx: inverse,
                group,
            }),
        }
        self.redo.clear();
        self.dirty = true;
        self.rev += 1;
        Ok(result)
    }

    /// Applies an edit and returns the transaction that reverses it, or
    /// `None` when nothing changed.
    fn apply_edit(&mut self, cmd: &Command) -> Result<(Option<Transaction>, Value), CommandError> {
        if let Command::Batch(b) = cmd {
            return self.apply_batch(b);
        }
        let (tx, result) = self.plan(cmd)?;
        if tx.is_empty() {
            Ok((None, result))
        } else {
            Ok((Some(tx.apply(&mut self.doc)), result))
        }
    }

    fn apply_batch(&mut self, b: &Batch) -> Result<(Option<Transaction>, Value), CommandError> {
        let mut cmds = Vec::with_capacity(b.commands.len());
        for (i, item) in b.commands.iter().enumerate() {
            let cmd = Command::from_parts(&item.command, item.params.clone()).map_err(|e| {
                at_index(
                    CommandError::invalid_params(format!("batch command {i}: {e}")),
                    i,
                )
            })?;
            if cmd.class() != Class::Edit || matches!(cmd, Command::Batch(_)) {
                return Err(at_index(
                    CommandError::invalid_params(format!(
                        "batch command {i} ({}) is not an edit; a batch holds edits only",
                        cmd.name()
                    )),
                    i,
                ));
            }
            cmds.push(cmd);
        }
        let mut inverses: Vec<Transaction> = Vec::new();
        let mut results = Vec::with_capacity(cmds.len());
        for (i, cmd) in cmds.iter().enumerate() {
            match self.plan(cmd) {
                Ok((tx, r)) => {
                    if !tx.is_empty() {
                        inverses.push(tx.apply(&mut self.doc));
                    }
                    results.push(r);
                }
                Err(e) => {
                    for inv in inverses.into_iter().rev() {
                        inv.apply(&mut self.doc);
                    }
                    return Err(at_index(e, i));
                }
            }
        }
        let result = json!({ "results": results });
        if inverses.is_empty() {
            return Ok((None, result));
        }
        let combined = Transaction(inverses.into_iter().rev().flat_map(|t| t.0).collect());
        Ok((Some(combined), result))
    }

    /// The typing group a writer's keystroke command opens or continues.
    fn group_of(&self, cmd: &Command, now: u64) -> Option<Group> {
        let (kind, id, mark) = match cmd {
            Command::TextInsert(p) => (GroupKind::Insert, p.id, p.at + p.text.chars().count()),
            Command::TextDelete(p) => (GroupKind::Delete, p.id, p.from),
            _ => return None,
        };
        Some(Group {
            kind,
            block: id,
            index: self.doc.index_of(id)?,
            mark,
            last_ms: now,
        })
    }

    /// Whether a keystroke command joins the typing step on top of the undo
    /// stack (docs/specs/commands.md, "Grouping").
    fn should_merge(&self, cmd: &Command, ex: &Exec, now: u64, inverse: &Transaction) -> bool {
        if ex.merge == Merge::Recorded(false) || !ex.by.is_writer() {
            return false;
        }
        let Some(top) = self.undo.last() else {
            return false;
        };
        let Some(g) = top.group else {
            return false;
        };
        if !top.by.same_actor(&ex.by) {
            return false;
        }
        let continues = match cmd {
            Command::TextInsert(p) => {
                g.kind == GroupKind::Insert && g.block == p.id && g.mark == p.at
            }
            Command::TextDelete(p) => {
                g.kind == GroupKind::Delete
                    && g.block == p.id
                    && (p.to == g.mark || p.from == g.mark)
            }
            _ => false,
        };
        // Grouped steps keep only the first inverse, which is right only
        // when every command in the group replaces the same block.
        let same_block =
            matches!(inverse.0.as_slice(), [Op::Replace { index, .. }] if *index == g.index);
        let in_time = match ex.merge {
            Merge::Auto => now.saturating_sub(g.last_ms) < GROUP_MS,
            Merge::Recorded(_) => true,
        };
        continues && same_block && in_time
    }

    fn history(&mut self, cmd: &Command, ex: &Exec) -> Result<Value, CommandError> {
        let is_undo = matches!(cmd, Command::Undo(_));
        let stack = if is_undo { &self.undo } else { &self.redo };
        let Some(top) = stack.last() else {
            return Err(if is_undo {
                CommandError::new("nothing_to_undo", "there is nothing to undo")
            } else {
                CommandError::new("nothing_to_redo", "there is nothing to redo")
            });
        };
        if !ex.by.is_writer() && !top.by.same_actor(&ex.by) {
            return Err(CommandError::new(
                "not_your_step",
                format!(
                    "the step to {} (\"{}\") was made by someone else; an agent may only {} its own steps",
                    if is_undo { "undo" } else { "redo" },
                    top.label,
                    if is_undo { "undo" } else { "redo" },
                ),
            )
            .with_details(json!({ "by": top.by, "label": top.label })));
        }
        let popped = if is_undo {
            self.undo.pop()
        } else {
            self.redo.pop()
        };
        let Some(step) = popped else {
            return Err(CommandError::new(
                "internal_error",
                "history changed underneath",
            ));
        };
        let reverse = step.tx.apply(&mut self.doc);
        let key = if is_undo { "undone" } else { "redone" };
        let result = json!({ key: step.command, "label": step.label });
        if let Err(e) = self.journal_append(cmd.name(), cmd.params(), &result, &ex.by, false) {
            let restore = reverse.apply(&mut self.doc);
            let back = Step {
                tx: restore,
                ..step
            };
            if is_undo {
                self.undo.push(back);
            } else {
                self.redo.push(back);
            }
            return Err(e);
        }
        let back = Step {
            tx: reverse,
            group: None,
            ..step
        };
        if is_undo {
            self.redo.push(back);
        } else {
            self.undo.push(back);
        }
        // Undo and redo end any typing group: the next keystroke starts a
        // new step.
        if let Some(top) = self.undo.last_mut() {
            top.group = None;
        }
        self.dirty = true;
        self.rev += 1;
        Ok(result)
    }

    fn journal_append(
        &mut self,
        name: &str,
        params: Value,
        result: &Value,
        by: &Author,
        merge: bool,
    ) -> Result<(), CommandError> {
        if let Some(j) = &mut self.journal {
            j.append(name, params, result, by, merge).map_err(|e| {
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

    /// Block indices of a scene, or the whole document.
    fn scope_range(&self, scene: Option<BlockId>) -> Result<std::ops::Range<usize>, CommandError> {
        match scene {
            None => Ok(0..self.doc.blocks.len()),
            Some(id) => {
                let s = self
                    .doc
                    .scene(id)
                    .ok_or(CommandError::scene_not_found(id))?;
                Ok(s.start..s.end)
            }
        }
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
            Command::TextInsert(p) => self.plan_text_insert(p),
            Command::TextDelete(p) => self.plan_text_delete(p),
            Command::TextStyle(p) => self.plan_text_style(p),
            Command::BlockSplit(p) => self.plan_split(p),
            Command::BlockJoin(p) => self.plan_join(p),
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
        if let Some(at) = &p.at {
            if p.scope.is_some() {
                return Err(CommandError::invalid_params(
                    "give \"scope\" or \"at\", not both",
                ));
            }
            let index = self.index(at.id)?;
            let b = &self.doc.blocks[index];
            let replaced = if b.element.kind.is_raw_text() {
                None
            } else {
                replace_one(&b.element.text, &p.find, &p.with, p.match_case, at.from)
            };
            let Some(text) = replaced else {
                return Ok((Transaction::default(), json!({ "count": 0, "blocks": [] })));
            };
            let mut e = b.element.clone();
            e.text = text;
            Self::check(&e)?;
            let id = b.id;
            let changed: Vec<BlockId> = if e == b.element { Vec::new() } else { vec![id] };
            return Ok((
                self.replace_op(index, e),
                json!({ "count": 1, "blocks": changed }),
            ));
        }
        let range = self.scope_range(p.scope.as_ref().map(|s| s.scene))?;
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

    fn plan_text_insert(&mut self, p: &TextInsert) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        let mut e = self.doc.blocks[index].element.clone();
        if e.kind == ElementKind::PageBreak {
            return Err(CommandError::new(
                "invalid_attr",
                "a page break has no text",
            ));
        }
        if e.kind.is_raw_text() && p.style.is_some_and(|s| s != Default::default()) {
            return Err(CommandError::new(
                "invalid_attr",
                format!("{} text is plain: no styles", e.kind),
            ));
        }
        let len = e.text.char_len();
        if p.at > len {
            return Err(range_error(RangeError {
                len,
                message: format!(
                    "offset {} is past the end of the text ({len} characters)",
                    p.at
                ),
            }));
        }
        if p.text.is_empty() {
            return Ok((Transaction::default(), json!({})));
        }
        e.text = e
            .text
            .insert(p.at, &p.text, p.style.map(Into::into))
            .map_err(range_error)?;
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_text_delete(&mut self, p: &TextDelete) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        let mut e = self.doc.blocks[index].element.clone();
        e.text = e.text.delete(p.from, p.to).map_err(range_error)?;
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_text_style(&mut self, p: &TextStyle) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        let mut e = self.doc.blocks[index].element.clone();
        if e.kind.is_raw_text() {
            return Err(CommandError::new(
                "invalid_attr",
                format!("{} text is plain: no styles", e.kind),
            ));
        }
        let change = StyleChange {
            bold: p.bold,
            italic: p.italic,
            underline: p.underline,
        };
        e.text = e.text.restyle(p.from, p.to, change).map_err(range_error)?;
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_split(&mut self, p: &BlockSplit) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        let original = &self.doc.blocks[index].element;
        let (head, rest) = original.text.split_at(p.at).map_err(range_error)?;
        let mut first = original.clone();
        first.text = head;
        let second = coldopen_screenplay::Element::new(p.kind.unwrap_or(original.kind), rest);
        Self::check(&first)?;
        Self::check(&second)?;
        let mut ops = self.replace_op(index, first).0;
        let id = self.doc.allocate_id();
        ops.push(Op::Insert {
            index: index + 1,
            block: Block {
                id,
                element: second,
            },
        });
        Ok((Transaction(ops), json!({ "id": id })))
    }

    fn plan_join(&mut self, p: &BlockJoin) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        if index == 0 {
            return Err(CommandError::invalid_params(
                "the first block has no block before it to join",
            ));
        }
        let prev = &self.doc.blocks[index - 1];
        let at = prev.element.text.char_len();
        let mut merged = prev.element.clone();
        merged.text = merged.text.concat(&self.doc.blocks[index].element.text);
        Self::check(&merged)?;
        let prev_id = prev.id;
        let mut ops = self.replace_op(index - 1, merged).0;
        ops.push(Op::Remove { index });
        Ok((Transaction(ops), json!({ "id": prev_id, "at": at })))
    }
}
