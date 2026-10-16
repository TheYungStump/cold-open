//! An open document with its undo history and journal. Edits, queries and
//! undo/redo run here; opening and saving live in the dispatcher.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use coldopen_screenplay::{ElementKind, RangeError, RichText, StyleChange, TextRepr};
use serde_json::{Value, json};

use crate::author::{Author, Clock};
use crate::command::{
    Batch, BatchItem, BlockDelete, BlockInsert, BlockJoin, BlockSetAttrs, BlockSetText,
    BlockSetType, BlockSplit, Class, Command, SceneMove, TextDelete, TextInsert, TextReplace,
    TextStyle,
};
use crate::document::{Block, BlockId, Document, word_count};
use crate::error::CommandError;
use crate::format::{FormatId, LayoutForm, Loss};
use crate::io::{Fingerprint, SourceStyle};
use crate::journal::{Journal, StepRef};
use crate::ops::{Op, Transaction};
use crate::replace::{find_in, replace_in, replace_one};
use crate::view::View;

/// Typing within this many milliseconds of the last keystroke joins the
/// same undo step (docs/specs/commands.md, "Authors and undo steps").
pub const GROUP_MS: u64 = 1500;

/// How a command runs: who sent it, and whether it may join the undo step
/// before it.
#[derive(Clone, Debug, Default)]
pub struct Exec {
    pub by: Author,
    pub merge: Merge,
    /// Replaying a journal: the entry's `seq`, which names the step it
    /// makes.
    pub seq: Option<u64>,
    /// Replaying an `undo` or `redo`: the step the entry says it reversed.
    pub step: Option<StepRef>,
}

impl Exec {
    pub fn by(by: Author) -> Exec {
        Exec {
            by,
            ..Exec::default()
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
    /// The seq of the journal entry that made the step, while that entry
    /// is in the journal; `None` for a step made before the journal last
    /// started again (docs/specs/journal.md, "Undo and redo entries").
    origin: Option<u64>,
}

/// Hands out session ids, unique in the process.
static SESSIONS: AtomicU64 = AtomicU64::new(1);

/// Unsaved work that `doc.open` brought back from a crash journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recovered {
    /// The session's `rev` when recovery finished, so equal to `rev` for as
    /// long as nothing has changed since.
    pub rev: u64,
    /// Whether any of it is a writer's work, which an agent may not set
    /// aside (docs/specs/mcp.md §7).
    pub writer: bool,
}

#[derive(Debug)]
pub struct Session {
    pub doc: Document,
    pub path: Option<PathBuf>,
    /// The file format the document was read from or last saved as; a
    /// save in place writes it again.
    pub format: FormatId,
    pub style: SourceStyle,
    pub journal: Option<Journal>,
    /// True when there are changes since the last open or save.
    pub dirty: bool,
    /// Set while the document holds work recovered from a crash journal
    /// that has not been saved since.
    pub recovered: Option<Recovered>,
    /// The file as this session last read or wrote it, or `None` if it
    /// had no file then. A save in place that finds anything else there
    /// refuses with `file_changed`.
    pub disk: Option<Fingerprint>,
    /// Unique in the process, so a save prepared for one session can tell
    /// it is not committing into another (docs/specs/journal.md, "Saving
    /// off the UI thread").
    id: u64,
    /// Saves prepared so far; each prepared save carries its number.
    pub(crate) saves: u64,
    /// The number of the prepared save being written, if one is. While it
    /// is set, no other save may start.
    pub(crate) saving: Option<u64>,
    /// Edits, undos, redos and reverts applied since the document was
    /// opened or created. Never goes down.
    pub rev: u64,
    pub clock: Clock,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The laid-out document, refreshed when `rev` moves on.
    view: Option<View>,
}

/// Every block id has been handed out (only a damaged document gets here).
fn ids_used_up() -> CommandError {
    CommandError::new(
        "internal_error",
        "this document has used every block id; save it and open it again",
    )
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
    pub fn new(
        doc: Document,
        path: Option<PathBuf>,
        format: FormatId,
        style: SourceStyle,
    ) -> Session {
        Session {
            doc,
            path,
            format,
            style,
            journal: None,
            dirty: false,
            recovered: None,
            disk: None,
            id: SESSIONS.fetch_add(1, Ordering::Relaxed),
            saves: 0,
            saving: None,
            rev: 0,
            clock: Clock::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            view: None,
        }
    }

    /// This session's id, unique in the process.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Changes the file format, as a save as another format does. The
    /// document is laid out again in that format's form.
    pub fn set_format(&mut self, format: FormatId) {
        if self.format != format {
            self.format = format;
            self.view = None;
        }
    }

    /// The laid-out document, brought up to date first. Only the pages an
    /// edit can change are laid out again (docs/specs/layout.md §15).
    pub fn view(&mut self) -> &View {
        let rev = self.rev;
        if let Some(v) = &mut self.view
            && v.rev != rev
        {
            v.refresh(&self.doc, rev);
        }
        let (doc, format) = (&self.doc, self.format);
        self.view
            .get_or_insert_with(|| View::with_format(doc, rev, format))
    }

    /// The laid-out document as last brought up to date, which may be
    /// behind the document. `Dispatcher::view` refreshes it.
    pub fn cached_view(&self) -> Option<&View> {
        self.view.as_ref()
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

    fn query(&mut self, cmd: &Command) -> Result<Value, CommandError> {
        match cmd {
            Command::DocGet(_) => Ok(json!({
                "path": self.path.as_ref().map(|p| p.display().to_string()),
                "dirty": self.dirty,
                "rev": self.rev,
                "title_page": self.doc.title_page,
                "blocks": self.doc.blocks,
            })),
            Command::SceneList(_) => Ok(json!({ "scenes": self.scene_list() })),
            Command::DocExport(p) => match (
                FormatId::from_name(&p.format),
                p.format.to_ascii_lowercase().as_str(),
            ) {
                (Some(id), _) => {
                    // Text is UTF-8 with LF line ends, whatever the file uses.
                    let written = id.get().write(&self.doc, SourceStyle::default());
                    let text = String::from_utf8(written.bytes)
                        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
                    Ok(json!({
                        "format": id.name(),
                        "text": text,
                        "losses": self.losses_json(&written.losses),
                    }))
                }
                (None, "json") => {
                    let content = json!({
                        "title_page": self.doc.title_page,
                        "blocks": self.doc.blocks,
                    });
                    let text = serde_json::to_string_pretty(&content)
                        .map_err(|e| CommandError::new("invalid_request", e.to_string()))?;
                    Ok(json!({ "format": "json", "text": text + "\n", "losses": [] }))
                }
                (None, "pages") => {
                    let layout = self.view().layout();
                    let text = match coldopen_layout::Fonts::new() {
                        Ok(fonts) => layout.page_view(&fonts),
                        Err(_) => layout.page_view(&coldopen_layout::AllGlyphs),
                    };
                    Ok(json!({ "format": "pages", "text": text, "losses": [] }))
                }
                (None, other) => Err(CommandError::new(
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
    pub fn losses_json(&self, losses: &[Loss]) -> Vec<Value> {
        losses
            .iter()
            .map(|l| {
                let id = l.element.and_then(|i| self.doc.blocks.get(i)).map(|b| b.id);
                json!({ "id": id, "code": l.code, "message": l.message })
            })
            .collect()
    }

    /// Lays out the document with its page format, title lines and spacing
    /// (the defaults for a Fountain script), in the form its format gives it: for Fountain the normal form, so the pages are the
    /// same after a save and reload (docs/specs/layout.md §4.8). The second
    /// value maps each laid-out element to the index of the block it came
    /// from.
    pub fn layout(
        &self,
        options: &coldopen_layout::Options,
    ) -> (coldopen_layout::Layout, Vec<usize>) {
        let LayoutForm { script, sources } = self.format.get().layout_form(&self.doc);
        let map: Vec<usize> = sources.elements.iter().map(|e| e.source).collect();
        let options = coldopen_layout::Options {
            overrides: self.doc.overrides(&map),
            title_lines: crate::view::layout_options(&self.doc).title_lines,
            ..options.clone()
        };
        let format = crate::view::page_format(&self.doc);
        let layout = coldopen_layout::layout(&script, &format, &options);
        (layout, map)
    }

    pub fn scene_list(&mut self) -> Vec<Value> {
        self.view();
        let Some(view) = &self.view else {
            return Vec::new();
        };
        let (layout, map) = (view.layout(), &view.map);
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
        let (inverse, result, resolved) = match self.apply_edit(cmd) {
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
        // The journal records what was done, defaults filled in, so a
        // later change to a default never changes what an old entry means.
        if let Err(e) = self.journal_append(
            resolved.name(),
            resolved.params(),
            &result,
            &ex.by,
            merge,
            None,
        ) {
            inverse.apply(&mut self.doc);
            self.doc.next_id = next_id;
            return Err(e);
        }
        let group = if ex.by.is_writer() {
            self.group_of(cmd, now)
        } else {
            None
        };
        let origin = self.journaled_seq().or(ex.seq);
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
                origin,
            }),
        }
        self.redo.clear();
        self.dirty = true;
        self.rev += 1;
        Ok(result)
    }

    /// Applies an edit and returns the transaction that reverses it, or
    /// `None` when nothing changed, its result, and the command as done:
    /// with the defaults it took filled in.
    fn apply_edit(
        &mut self,
        cmd: &Command,
    ) -> Result<(Option<Transaction>, Value, Command), CommandError> {
        if let Command::Batch(b) = cmd {
            return self.apply_batch(b);
        }
        let cmd = self.resolve(cmd);
        let (tx, result) = self.plan(&cmd)?;
        if tx.is_empty() {
            Ok((None, result, cmd))
        } else {
            Ok((Some(tx.apply(&mut self.doc)), result, cmd))
        }
    }

    /// The command with the defaults that depend on the document filled
    /// in (docs/specs/journal.md, "Entries"): the style `text.insert` takes
    /// from the text around it, and the type `block.split` keeps.
    fn resolve(&self, cmd: &Command) -> Command {
        match cmd {
            Command::TextInsert(p) if p.style.is_none() => {
                let style = self
                    .doc
                    .get(p.id)
                    .filter(|b| p.at <= b.element.text.char_len())
                    .map(|b| b.element.text.style_at(p.at).into());
                Command::TextInsert(TextInsert { style, ..p.clone() })
            }
            Command::BlockSplit(p) if p.kind.is_none() => Command::BlockSplit(BlockSplit {
                kind: self.doc.get(p.id).map(|b| b.element.kind),
                ..p.clone()
            }),
            other => other.clone(),
        }
    }

    fn apply_batch(
        &mut self,
        b: &Batch,
    ) -> Result<(Option<Transaction>, Value, Command), CommandError> {
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
        let mut resolved = Vec::with_capacity(cmds.len());
        for (i, cmd) in cmds.iter().enumerate() {
            // Each edit's defaults depend on the document the edits before
            // it left.
            let cmd = self.resolve(cmd);
            match self.plan(&cmd) {
                Ok((tx, r)) => {
                    if !tx.is_empty() {
                        inverses.push(tx.apply(&mut self.doc));
                    }
                    results.push(r);
                    resolved.push(BatchItem::of(&cmd));
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
        let done = Command::Batch(Batch {
            commands: resolved,
            label: b.label.clone(),
        });
        if inverses.is_empty() {
            return Ok((None, result, done));
        }
        let combined = Transaction(inverses.into_iter().rev().flat_map(|t| t.0).collect());
        Ok((Some(combined), result, done))
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
        let key = if is_undo { "undone" } else { "redone" };
        // Replaying an undo or redo of a step made before the journal's
        // header: the rebuilt history cannot hold that step, so the entry
        // carries it (review finding C1).
        if let Some(StepRef::Before {
            tx,
            command,
            label,
            by,
        }) = &ex.step
        {
            let reverse = tx.clone().apply(&mut self.doc);
            self.push_reversed(
                is_undo,
                Step {
                    command: command.clone(),
                    label: label.clone(),
                    by: by.clone().unwrap_or_default(),
                    tx: reverse,
                    group: None,
                    origin: ex.seq,
                },
            );
            return Ok(json!({ key: command, "label": label }));
        }
        let stack = if is_undo { &self.undo } else { &self.redo };
        let Some(top) = stack.last() else {
            return Err(if is_undo {
                CommandError::new("nothing_to_undo", "there is nothing to undo")
            } else {
                CommandError::new("nothing_to_redo", "there is nothing to redo")
            });
        };
        if let Some(StepRef::Seq(seq)) = &ex.step
            && top.origin != Some(*seq)
        {
            return Err(CommandError::new(
                "diverged",
                format!("replay reached a different step than the one entry {seq} made"),
            ));
        }
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
        // The entry names the step, or carries it when it was made before
        // the journal last started again (docs/specs/journal.md, "Undo and
        // redo entries").
        let step_ref = match step.origin {
            Some(seq) => Some(StepRef::Seq(seq)),
            None => self.journal.is_some().then(|| StepRef::Before {
                tx: step.tx.clone(),
                command: step.command.clone(),
                label: step.label.clone(),
                by: (!step.by.is_writer()).then(|| step.by.clone()),
            }),
        };
        let Step {
            command,
            label,
            by,
            tx,
            group,
            origin,
        } = step;
        let reverse = tx.apply(&mut self.doc);
        let result = json!({ key: command, "label": label });
        if let Err(e) =
            self.journal_append(cmd.name(), cmd.params(), &result, &ex.by, false, step_ref)
        {
            let restore = reverse.apply(&mut self.doc);
            let back = Step {
                command,
                label,
                by,
                tx: restore,
                group,
                origin,
            };
            if is_undo {
                self.undo.push(back);
            } else {
                self.redo.push(back);
            }
            return Err(e);
        }
        // A step from before the header enters the journal at this entry.
        let origin = origin.or_else(|| self.journaled_seq()).or(ex.seq);
        self.push_reversed(
            is_undo,
            Step {
                command,
                label,
                by,
                tx: reverse,
                group: None,
                origin,
            },
        );
        Ok(result)
    }

    /// Puts a step that was just undone on the redo stack, or a redone one
    /// on the undo stack.
    fn push_reversed(&mut self, is_undo: bool, step: Step) {
        if is_undo {
            self.redo.push(step);
        } else {
            self.undo.push(step);
        }
        // Undo and redo end any typing group: the next keystroke starts a
        // new step.
        if let Some(top) = self.undo.last_mut() {
            top.group = None;
        }
        self.dirty = true;
        self.rev += 1;
    }

    /// The seq of the entry last written to the journal, if there is one.
    fn journaled_seq(&self) -> Option<u64> {
        self.journal
            .as_ref()
            .map(Journal::entries)
            .filter(|&n| n > 0)
    }

    /// Called when the journal starts again from the current document
    /// (after a save): the entries that made the steps in the history are
    /// gone, so an undo or redo of one must carry the step itself, and a
    /// typing burst never joins a step across the restart.
    pub fn journal_restarted(&mut self) {
        for s in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            s.origin = None;
        }
        if let Some(top) = self.undo.last_mut() {
            top.group = None;
        }
    }

    fn journal_append(
        &mut self,
        name: &str,
        params: Value,
        result: &Value,
        by: &Author,
        merge: bool,
        step: Option<StepRef>,
    ) -> Result<(), CommandError> {
        if let Some(j) = &mut self.journal {
            j.append(name, params, result, by, merge, step)
                .map_err(|e| {
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
        let id = self.doc.allocate_id().ok_or_else(ids_used_up)?;
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
        let id = self.doc.allocate_id().ok_or_else(ids_used_up)?;
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
