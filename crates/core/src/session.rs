//! An open document with its undo history and journal. Edits, queries and
//! undo/redo run here; opening and saving live in the dispatcher.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use coldopen_screenplay::{
    ElementKind, Marks, RangeError, RichText, Settle, StyleChange, TextRepr,
};
use serde_json::{Value, json};

use crate::author::{Author, Clock};
use crate::changes::{BlockMarks, ChangeBy, ChangeId, Moved, PrintForm, Retyped, mark_inserted};
use crate::command::{
    Batch, BatchItem, BlockDelete, BlockInsert, BlockJoin, BlockSetAttrs, BlockSetText,
    BlockSetType, BlockSplit, ChangesSettle, ChangesTrack, Class, Command, RevisionClear,
    RevisionColors, RevisionStart, SceneMove, TextDelete, TextInsert, TextReplace, TextStyle,
};
use crate::document::{Block, BlockId, Document, word_count};
use crate::error::CommandError;
use crate::format::{FormatId, LayoutForm, Loss};
use crate::io::{Fingerprint, SourceStyle};
use crate::journal::{Journal, StepRef};
use crate::ops::{Op, Transaction};
use crate::replace::{Tracked, find_in, replace_in, replace_one};
use crate::revise;
use crate::revisions::{self, RevisionId, RevisionSet};
use crate::track::{self, TrackCtx};
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
    /// Replaying: the entry's time, which a change the edit makes takes
    /// (docs/specs/journal.md, "Entries").
    pub t: Option<u64>,
    /// Replaying: the journal's `semantics`. Absent for a live command.
    pub semantics: Option<u32>,
}

impl Exec {
    pub fn by(by: Author) -> Exec {
        Exec {
            by,
            ..Exec::default()
        }
    }

    /// The semantics the command runs under: the journal's (or its last
    /// marker's) while replaying, this version's for a live command. Rules
    /// a later semantics changed keep their old form for older journals
    /// (docs/specs/journal.md, "Header").
    pub fn rules(&self) -> u32 {
        self.semantics.unwrap_or(crate::journal::SEMANTICS)
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
    /// The file was repaired when it was read (docs/specs/fdx.md §17): the
    /// first save in place keeps a copy of it as it was first.
    pub damaged: bool,
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
    /// Milliseconds since the Unix epoch, for the time of a tracked change.
    pub wall: Clock,
    /// Overrides the document's switch for agents' edits in this session:
    /// `coldopen mcp --untracked` (docs/specs/mcp.md §6.1). Not document
    /// state; every edit journals the `tracked` it took.
    pub agent_tracking: Option<bool>,
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
        Command::ChangesAccept(p) => if p.id.is_some() {
            "Accept change"
        } else {
            "Accept changes"
        }
        .into(),
        Command::ChangesReject(p) => if p.id.is_some() {
            "Reject change"
        } else {
            "Reject changes"
        }
        .into(),
        Command::ChangesTrack(_) => "Track changes".into(),
        Command::RevisionStart(_) => "Start revision".into(),
        Command::RevisionEnd(_) => "End revision".into(),
        Command::RevisionColors(_) => "Revision colors".into(),
        Command::RevisionClear(p) => if p.sets {
            "Clear revisions"
        } else {
            "Clear revision marks"
        }
        .into(),
        Command::TitleSet(_) | Command::TitleSetLines(_) => "Title page".into(),
        other => other.name().into(),
    }
}

/// The system clock in milliseconds since the Unix epoch.
pub fn wall_clock() -> Clock {
    Clock::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    })
}

fn label(cmd: &Command, by: &Author) -> String {
    match by {
        Author::Writer { .. } => writer_label(cmd),
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
            damaged: false,
            dirty: false,
            recovered: None,
            disk: None,
            id: SESSIONS.fetch_add(1, Ordering::Relaxed),
            saves: 0,
            saving: None,
            rev: 0,
            clock: Clock::default(),
            wall: wall_clock(),
            agent_tracking: None,
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
            Class::Session | Class::Output | Class::File => Err(CommandError::new(
                "invalid_request",
                format!("{} is handled by the dispatcher", cmd.name()),
            )),
        }
    }

    fn query(&mut self, cmd: &Command) -> Result<Value, CommandError> {
        match cmd {
            Command::DocGet(_) => Ok(json!({
                "path": self.path.as_ref().map(|p| p.display().to_string()),
                "format": self.format.name(),
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
                (Some(FormatId::Bundle), _) => Err(CommandError::new(
                    "unsupported_format",
                    "a Cold Open project is a zip file, not text; save it with doc.save, or use \"json\" for its blocks",
                )),
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
                    let marks = self.doc.has_revision_data().then(|| {
                        let map = self.view().map.clone();
                        self.doc.revision_marks(&map)
                    });
                    let mut layout = self.view().layout().clone();
                    if let Some(marks) = &marks {
                        layout.mark_revisions(marks);
                    }
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
            Command::RevisionList(_) => Ok(self.revision_list()),
            Command::ReportScenes(_) => {
                let list = self.scene_list();
                Ok(crate::report::scenes(&self.doc, list))
            }
            Command::ReportCharacters(_) => {
                self.view();
                let Some(view) = &self.view else {
                    return Ok(json!({ "characters": [] }));
                };
                Ok(crate::report::characters(
                    &self.doc,
                    view.layout(),
                    &view.map,
                ))
            }
            Command::ReportLocations(_) => Ok(crate::report::locations(&self.doc)),
            Command::TitleGet(_) => Ok(crate::title::get(&self.doc)),
            Command::ChangesList(p) => {
                let only = match p.scene {
                    Some(s) => {
                        let r = self.scope_range(Some(s))?;
                        Some(self.doc.blocks[r].iter().map(|b| b.id).collect())
                    }
                    None => None,
                };
                Ok(json!({ "changes": track::list(&self.doc, only.as_ref()) }))
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
        losses_json(&self.doc, losses)
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
        self.layout_as(options, PrintForm::Marked)
    }

    /// The layout of one of the document's print forms (docs/specs/
    /// changes.md §9). For the accepted form, the map's indices are into
    /// the accepted document's blocks, not the session's.
    pub fn layout_as(
        &self,
        options: &coldopen_layout::Options,
        form: PrintForm,
    ) -> (coldopen_layout::Layout, Vec<usize>) {
        let accepted;
        let doc = match form {
            PrintForm::Accepted if self.doc.has_changes() => {
                accepted = self.doc.accepted();
                &accepted
            }
            _ => &self.doc,
        };
        let LayoutForm { script, sources } = self.format.get().layout_form(&doc.marked());
        let map: Vec<usize> = sources.elements.iter().map(|e| e.source).collect();
        let options = coldopen_layout::Options {
            overrides: doc.overrides(&map),
            title_lines: crate::view::layout_options(doc).title_lines,
            ..options.clone()
        };
        let format = crate::view::page_format(doc);
        let mut layout = coldopen_layout::layout(&script, &format, &options);
        if doc.has_revision_data() {
            layout.mark_revisions(&doc.revision_marks(&map));
        }
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
        let changes = self.doc.changes.len();
        let rollback = |s: &mut Session| {
            s.doc.next_id = next_id;
            s.doc.changes.truncate(changes);
        };
        // A tracked change takes the time of its journal entry, on replay as
        // when it was made (docs/specs/journal.md, "Entries").
        let t = ex.t.unwrap_or_else(|| self.wall.now_ms());
        // An agent's edit must leave other authors' marks as they were
        // (docs/specs/changes.md §5.4).
        let by = ChangeBy::of(&ex.by);
        let before = (!ex.by.is_writer()).then(|| track::others_marks(&self.doc, &by));
        let (inverse, result, resolved) = match self.apply_edit(cmd, ex, t) {
            Ok(r) => r,
            Err(e) => {
                rollback(self);
                return Err(e);
            }
        };
        let Some(inverse) = inverse else {
            rollback(self);
            return Ok(result);
        };
        if let Some(before) = before
            && before != track::others_marks(&self.doc, &by)
        {
            inverse.apply(&mut self.doc);
            rollback(self);
            return Err(track::not_your_change(
                "this edit would remove or change another author's tracked change",
            ));
        }
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
            t,
        ) {
            inverse.apply(&mut self.doc);
            rollback(self);
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
        ex: &Exec,
        t: u64,
    ) -> Result<(Option<Transaction>, Value, Command), CommandError> {
        let mut ctx = TrackCtx::new(&ex.by, t);
        let table = self.doc.changes.len();
        if let Command::Batch(b) = cmd {
            return self.apply_batch(b, ex, &mut ctx, table);
        }
        let cmd = self.resolve(cmd, ex)?.with_revision_defaults(&self.doc, t);
        let (tx, mut result) = self.plan(&cmd, ex, &mut ctx)?;
        if tx.is_empty() {
            return Ok((None, result, cmd));
        }
        // A change the edit made but left no mark of, as when a tracked
        // delete only took back the author's own insertion, is not kept.
        if let Some(c) = ctx.change
            && !tx.0.iter().any(|op| op_names(op, c))
        {
            self.drop_change(c, table);
            ctx.change = None;
        }
        if let (Some(c), Value::Object(m)) = (ctx.change, &mut result) {
            m.insert("change".into(), json!(c));
        }
        Ok((Some(self.apply_marking(tx, &cmd)), result, cmd))
    }

    /// Applies an edit's operations. While the document has revision sets,
    /// an untracked edit or an accept is marked with the active set, and
    /// text it did not change keeps its marks (docs/specs/revisions.md §6).
    fn apply_marking(&mut self, tx: Transaction, cmd: &Command) -> Transaction {
        let untracked = cmd.tracked().is_some_and(|t| t != Some(true))
            || matches!(cmd, Command::TextStyle(_) | Command::BlockSetAttrs(_));
        let accept = matches!(cmd, Command::ChangesAccept(_));
        if self.doc.revisions.is_default() || !(untracked || accept) {
            return tx.apply(&mut self.doc);
        }
        let active = self.doc.revisions.active;
        let before: std::collections::BTreeSet<ChangeId> = if accept && active.is_none() {
            self.doc.blocks.iter().flat_map(Block::change_ids).collect()
        } else {
            Default::default()
        };
        revise::apply(tx, &mut self.doc, |doc| {
            active.or_else(|| accept.then(|| revise::settled_rev(doc, &before)).flatten())
        })
    }

    /// Takes back change `c` if this edit added it to the table.
    fn drop_change(&mut self, c: ChangeId, table: usize) {
        if c.0 > table as u64 {
            self.doc.changes.truncate(table);
        }
    }

    /// The command with the defaults that depend on the document filled
    /// in (docs/specs/journal.md, "Entries"): the style `text.insert` takes
    /// from the text around it, the type `block.split` keeps, and whether
    /// the edit is tracked (docs/specs/changes.md §7).
    fn resolve(&self, cmd: &Command, ex: &Exec) -> Result<Command, CommandError> {
        let cmd = match cmd {
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
        };
        match cmd.tracked() {
            Some(None) => {
                let tracked = match ex.semantics {
                    // Journals from before phase 5a hold no tracked edits.
                    Some(s) if s < 2 => false,
                    Some(_) => {
                        return Err(CommandError::invalid_params(format!(
                            "{} was journaled without \"tracked\"",
                            cmd.name()
                        )));
                    }
                    None => self.tracks(&ex.by),
                };
                Ok(cmd.with_tracked(tracked))
            }
            Some(Some(false)) if !ex.by.is_writer() && self.tracks(&ex.by) => {
                Err(CommandError::new(
                    "writer_only",
                    "an agent's edits are tracked changes in this script; only the writer can turn that off (tracked: false)",
                ))
            }
            _ => Ok(cmd),
        }
    }

    /// Whether an author's edits are tracked when an edit does not say.
    pub fn tracks(&self, by: &Author) -> bool {
        match (by.is_writer(), self.agent_tracking) {
            (false, Some(a)) => a,
            _ => self.doc.tracking.for_author(by),
        }
    }

    fn apply_batch(
        &mut self,
        b: &Batch,
        ex: &Exec,
        ctx: &mut TrackCtx,
        table: usize,
    ) -> Result<(Option<Transaction>, Value, Command), CommandError> {
        let mut cmds = Vec::with_capacity(b.commands.len());
        for (i, item) in b.commands.iter().enumerate() {
            let cmd = Command::from_parts(&item.command, item.params.clone()).map_err(|e| {
                at_index(
                    CommandError::invalid_params(format!("batch command {i}: {e}")),
                    i,
                )
            })?;
            // `changes.import` reads a file, so only the dispatcher runs it.
            if cmd.class() != Class::Edit
                || matches!(cmd, Command::Batch(_) | Command::ChangesImport(_))
            {
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
        let undo = |s: &mut Session, inverses: Vec<Transaction>| {
            for inv in inverses.into_iter().rev() {
                inv.apply(&mut s.doc);
            }
        };
        for (i, cmd) in cmds.iter().enumerate() {
            // Each edit's defaults depend on the document the edits before
            // it left.
            let cmd = match self.resolve(cmd, ex) {
                Ok(c) => c.with_revision_defaults(&self.doc, ctx.t),
                Err(e) => {
                    undo(self, inverses);
                    return Err(at_index(e, i));
                }
            };
            match self.plan(&cmd, ex, ctx) {
                Ok((tx, r)) => {
                    if !tx.is_empty() {
                        inverses.push(self.apply_marking(tx, &cmd));
                    }
                    results.push(r);
                    resolved.push(BatchItem::of(&cmd));
                }
                Err(e) => {
                    undo(self, inverses);
                    return Err(at_index(e, i));
                }
            }
        }
        if let Some(c) = ctx.change
            && !self
                .doc
                .blocks
                .iter()
                .any(|b| b.change_ids().any(|x| x == c))
        {
            self.drop_change(c, table);
            ctx.change = None;
        }
        // Every tracked edit of a batch records into its one change.
        if let Some(c) = ctx.change {
            for r in &mut results {
                if let Value::Object(m) = r {
                    m.insert("change".into(), json!(c));
                }
            }
        }
        let mut result = json!({ "results": results });
        if let (Some(c), Value::Object(m)) = (ctx.change, &mut result) {
            m.insert("change".into(), json!(c));
        }
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
        let t = ex.t.unwrap_or_else(|| self.wall.now_ms());
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
                by: (!step.by.is_plain_writer()).then(|| step.by.clone()),
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
        if let Err(e) = self.journal_append(
            cmd.name(),
            cmd.params(),
            &result,
            &ex.by,
            false,
            step_ref,
            t,
        ) {
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

    #[allow(clippy::too_many_arguments)]
    fn journal_append(
        &mut self,
        name: &str,
        params: Value,
        result: &Value,
        by: &Author,
        merge: bool,
        step: Option<StepRef>,
        t: u64,
    ) -> Result<(), CommandError> {
        if let Some(j) = &mut self.journal {
            j.append(name, params, result, by, merge, step, t)
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

    /// Fails with `change_pending` when the block is a pending deletion,
    /// which cannot be edited until its change is settled
    /// (docs/specs/changes.md §5.4).
    fn refuse_pending(&self, index: usize) -> Result<(), CommandError> {
        let b = &self.doc.blocks[index];
        match b.marks.deleted {
            Some(c) => Err(CommandError::new(
                "change_pending",
                format!(
                    "{} is deleted pending review ({c}); accept or reject that change first",
                    b.id
                ),
            )
            .with_details(json!({ "id": b.id, "change": c }))),
            None => Ok(()),
        }
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
    /// document (apart from handing out ids and adding to the change
    /// table, which a failed edit rolls back).
    fn plan(
        &mut self,
        cmd: &Command,
        ex: &Exec,
        ctx: &mut TrackCtx,
    ) -> Result<(Transaction, Value), CommandError> {
        let t = ctx.t;
        let tracked = cmd.tracked().flatten() == Some(true);
        let ctx = tracked.then_some(ctx);
        match cmd {
            Command::BlockInsert(p) => self.plan_insert(p, ctx),
            Command::BlockSetType(p) => self.plan_set_type(p, ctx),
            Command::BlockSetText(p) => self.plan_set_text(p, ctx),
            Command::BlockSetAttrs(p) => self.plan_set_attrs(p),
            Command::BlockDelete(p) => self.plan_delete(p, ctx, &ex.by),
            Command::TextReplace(p) => self.plan_replace(p, ctx),
            Command::SceneMove(p) => self.plan_move(p, ctx),
            Command::TextInsert(p) => self.plan_text_insert(p, ctx),
            Command::TextDelete(p) => self.plan_text_delete(p, ctx),
            Command::TextStyle(p) => self.plan_text_style(p),
            Command::BlockSplit(p) => self.plan_split(p, ctx),
            Command::BlockJoin(p) => self.plan_join(p, ctx),
            Command::ChangesAccept(p) => self.plan_settle(Settle::Accept, p, &ex.by),
            Command::ChangesReject(p) => self.plan_settle(Settle::Reject, p, &ex.by),
            Command::ChangesTrack(p) => self.plan_track(p, &ex.by),
            Command::RevisionStart(p) => self.plan_revision_start(p, &ex.by, t),
            Command::RevisionEnd(_) => self.plan_revision_end(&ex.by, t),
            Command::RevisionColors(p) => self.plan_revision_colors(p, &ex.by),
            Command::RevisionClear(p) => self.plan_revision_clear(p, &ex.by),
            Command::TitleSet(p) => {
                crate::title::plan_set(&self.doc, p, &ex.by, self.tracks(&ex.by))
            }
            Command::TitleSetLines(p) => {
                crate::title::plan_set_lines(&self.doc, p, &ex.by, self.tracks(&ex.by))
            }
            _ => Err(CommandError::new("invalid_request", "not an edit")),
        }
    }

    /// Whether change `c` is the edit's author's own.
    fn own(&self, ctx: &TrackCtx, c: ChangeId) -> bool {
        ctx.owns(&self.doc, c)
    }

    /// Whether a block's only mark is the author's own insertion: such a
    /// block can be deleted or joined outright, since nothing else about it
    /// is waiting for review (docs/specs/changes.md §5.2).
    fn only_own_insert(&self, ctx: &TrackCtx, marks: &BlockMarks) -> bool {
        marks.inserted.is_some_and(|c| self.own(ctx, c))
            && BlockMarks {
                inserted: None,
                ..marks.clone()
            }
            .is_empty()
    }

    fn plan_insert(
        &mut self,
        p: &BlockInsert,
        ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
        let mut element = p
            .element_repr()
            .into_element()
            .map_err(CommandError::invalid_params)?;
        element.text = element.text.as_input();
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
        let mut block = Block::new(id, element);
        if let Some(ctx) = ctx {
            let c = ctx.change(&mut self.doc, None);
            block.element.text = mark_inserted(&block.element.text, c);
            block.marks.inserted = Some(c);
        }
        Ok((
            Transaction(vec![Op::Insert { index, block }]),
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

    /// Operations that give block `index` this element and these marks.
    fn block_ops(
        &self,
        index: usize,
        element: coldopen_screenplay::Element,
        marks: BlockMarks,
    ) -> Transaction {
        let mut ops = self.replace_op(index, element).0;
        if self.doc.blocks[index].marks != marks {
            ops.push(Op::Mark { index, marks });
        }
        Transaction(ops)
    }

    fn plan_set_type(
        &mut self,
        p: &BlockSetType,
        ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        self.refuse_pending(index)?;
        let old = self.doc.blocks[index].element.clone();
        let mut e = old.clone();
        e.kind = p.kind;
        e.clear_inapplicable();
        Self::check(&e)?;
        let mut marks = self.doc.blocks[index].marks.clone();
        if let Some(ctx) = ctx
            && e != old
        {
            // Each retype is kept, so rejecting undoes them in order
            // (docs/specs/changes.md §5.2); the author's own new block just
            // changes type.
            let own_block = marks.inserted.is_some_and(|c| self.own(ctx, c));
            // A retype keeps the kind and attributes it changed, not text,
            // so one that changes text (into a page break, or styles into a
            // note) could not be rejected (found by the `diff` fuzz target,
            // 2026-10-26).
            if !own_block && e.text != old.text {
                return Err(CommandError::new(
                    "invalid_attr",
                    "this retype would drop the block's text or its styles, which a tracked change could not give back; delete the block and insert the new one, or make the change untracked",
                ));
            }
            if !own_block {
                let c = ctx.change(&mut self.doc, None);
                marks.retyped.push(Retyped {
                    change: c,
                    from: old.kind,
                    scene_number: old
                        .scene_number
                        .clone()
                        .filter(|_| e.scene_number.is_none()),
                    dual: old.dual && !e.dual,
                    centered: old.centered && !e.centered,
                    depth: if e.depth == 0 { old.depth } else { 0 },
                    space_before: old.space_before.filter(|_| e.space_before.is_none()),
                });
            }
        }
        Ok((self.block_ops(index, e, marks), json!({})))
    }

    fn plan_set_text(
        &mut self,
        p: &BlockSetText,
        ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        self.refuse_pending(index)?;
        if p.text.is_none() && p.runs.is_none() {
            return Err(CommandError::invalid_params("give \"text\" or \"runs\""));
        }
        let text: RichText = TextRepr {
            text: p.text.clone(),
            runs: p.runs.clone(),
        }
        .into_rich()
        .map_err(CommandError::invalid_params)?
        .as_input();
        let mut e = self.doc.blocks[index].element.clone();
        e.text = match ctx {
            Some(ctx) => {
                let old = &self.doc.blocks[index].element.text;
                // Only a change to the current text is a tracked change.
                if old.plain_text() == text.plain_text() {
                    keep_marks(old, &text)
                } else {
                    let c = ctx.change(&mut self.doc, None);
                    let old = &self.doc.blocks[index].element.text;
                    let own = |x: ChangeId| ctx.owns(&self.doc, x);
                    track::set_text(old, &text, c, &own)
                }
            }
            None => text,
        };
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_set_attrs(&mut self, p: &BlockSetAttrs) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        self.refuse_pending(index)?;
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

    fn plan_delete(
        &mut self,
        p: &BlockDelete,
        ctx: Option<&mut TrackCtx>,
        by: &Author,
    ) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        let Some(ctx) = ctx else {
            // The writer may remove a pending deletion outright; nobody else
            // edits one (docs/specs/changes.md §5.4).
            if !by.is_writer() {
                self.refuse_pending(index)?;
            }
            return Ok((Transaction(vec![Op::Remove { index }]), json!({})));
        };
        self.refuse_pending(index)?;
        let b = &self.doc.blocks[index];
        let own_block = self.only_own_insert(ctx, &b.marks);
        let all_own = b
            .element
            .text
            .runs()
            .iter()
            .all(|r| r.is_hidden() || r.marks().ins.is_some_and(|c| self.own(ctx, c)));
        if own_block && all_own {
            return Ok((Transaction(vec![Op::Remove { index }]), json!({})));
        }
        let c = ctx.change(&mut self.doc, None);
        let b = &self.doc.blocks[index];
        let mut e = b.element.clone();
        let len = e.text.char_len();
        let own = |x: ChangeId| ctx.owns(&self.doc, x);
        e.text = e
            .text
            .delete_tracked(0, len, c, &own)
            .map_err(range_error)?;
        let mut marks = b.marks.clone();
        marks.deleted = Some(c);
        Ok((self.block_ops(index, e, marks), json!({})))
    }

    fn plan_replace(
        &mut self,
        p: &TextReplace,
        mut ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
        if p.find.is_empty() {
            return Err(CommandError::invalid_params("\"find\" must not be empty"));
        }
        // The change is made only once something matches.
        let first_match = if let Some(at) = &p.at {
            if p.scope.is_some() {
                return Err(CommandError::invalid_params(
                    "give \"scope\" or \"at\", not both",
                ));
            }
            let index = self.index(at.id)?;
            let b = &self.doc.blocks[index];
            !b.element.kind.is_raw_text()
                && find_in(&b.element.text, &p.find, p.match_case).contains(&at.from)
        } else {
            let range = self.scope_range(p.scope.as_ref().map(|s| s.scene))?;
            self.doc.blocks[range].iter().any(|b| {
                !b.element.kind.is_raw_text()
                    && !find_in(&b.element.text, &p.find, p.match_case).is_empty()
            })
        };
        let change = match (&mut ctx, first_match) {
            (Some(ctx), true) => Some(ctx.change(&mut self.doc, None)),
            _ => None,
        };
        let ctx = ctx.as_deref();
        let own = |x: ChangeId| ctx.is_some_and(|c| c.owns(&self.doc, x));
        let tracked = change.map(|change| Tracked { change, own: &own });
        if let Some(at) = &p.at {
            let index = self.index(at.id)?;
            let b = &self.doc.blocks[index];
            let replaced = if b.element.kind.is_raw_text() {
                None
            } else {
                replace_one(
                    &b.element.text,
                    &p.find,
                    &p.with,
                    p.match_case,
                    at.from,
                    tracked.as_ref(),
                )
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
            if let Some((text, n)) = replace_in(
                &b.element.text,
                &p.find,
                &p.with,
                p.match_case,
                tracked.as_ref(),
            ) {
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

    fn plan_move(
        &mut self,
        p: &SceneMove,
        ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
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
        let mut ops = Vec::new();
        if let Some(ctx) = ctx {
            // Where the scene stood: before the block after it, or at the
            // end; and the blocks it took. Each move is kept, so rejecting
            // undoes them in order.
            let was = self.doc.blocks.get(scene.end).map(|b| b.id);
            let last = self.doc.blocks[scene.end - 1].id;
            let mut marks = self.doc.blocks[scene.start].marks.clone();
            let c = ctx.change(&mut self.doc, None);
            marks.moved.push(Moved {
                change: c,
                before: was,
                last,
            });
            ops.push(Op::Mark {
                index: scene.start,
                marks,
            });
        }
        ops.push(Op::Move {
            from: scene.start,
            len,
            to,
        });
        Ok((Transaction(ops), json!({})))
    }

    fn plan_text_insert(
        &mut self,
        p: &TextInsert,
        ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        self.refuse_pending(index)?;
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
        let marks = match ctx {
            Some(ctx) => {
                // Next to the author's own pending insertion: the same change.
                let (before, after) = e.text.ins_around(p.at);
                let candidate = before
                    .filter(|&c| self.own(ctx, c))
                    .or(after.filter(|&c| self.own(ctx, c)));
                Marks {
                    ins: Some(ctx.change(&mut self.doc, candidate)),
                    ..Marks::NONE
                }
            }
            None => Marks::NONE,
        };
        let style = p.style.map(Into::into);
        e.text = if p.after_deleted {
            e.text.insert_after_deleted(p.at, &p.text, style, marks)
        } else {
            e.text.insert_marked(p.at, &p.text, style, marks)
        }
        .map_err(range_error)?;
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_text_delete(
        &mut self,
        p: &TextDelete,
        ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        self.refuse_pending(index)?;
        let mut e = self.doc.blocks[index].element.clone();
        e.text = match ctx {
            Some(ctx) if p.from < p.to => {
                // Next to the author's own pending deletion: the same change.
                let candidate = e
                    .text
                    .del_touching(p.from, p.to)
                    .into_iter()
                    .find(|&c| self.own(ctx, c));
                // Check the range before a change is made for it.
                e.text.delete(p.from, p.to).map_err(range_error)?;
                let c = ctx.change(&mut self.doc, candidate);
                let own = |x: ChangeId| ctx.owns(&self.doc, x);
                e.text
                    .delete_tracked(p.from, p.to, c, &own)
                    .map_err(range_error)?
            }
            _ => e.text.delete(p.from, p.to).map_err(range_error)?,
        };
        Self::check(&e)?;
        Ok((self.replace_op(index, e), json!({})))
    }

    fn plan_text_style(&mut self, p: &TextStyle) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        self.refuse_pending(index)?;
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

    fn plan_split(
        &mut self,
        p: &BlockSplit,
        ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        self.refuse_pending(index)?;
        let original = &self.doc.blocks[index].element;
        let (head, rest) = if p.after_deleted {
            original.text.split_after_deleted(p.at)
        } else {
            original.text.split_at(p.at)
        }
        .map_err(range_error)?;
        let mut first = original.clone();
        first.text = head;
        let second = coldopen_screenplay::Element::new(p.kind.unwrap_or(original.kind), rest);
        Self::check(&first)?;
        Self::check(&second)?;
        let mut ops = self.replace_op(index, first).0;
        let id = self.doc.allocate_id().ok_or_else(ids_used_up)?;
        let mut block = Block::new(id, second);
        if let Some(ctx) = ctx {
            let own_block = self.doc.blocks[index]
                .marks
                .inserted
                .is_some_and(|c| self.own(ctx, c));
            let c = ctx.change(&mut self.doc, None);
            // Nothing moves to the new block: it is a new block, as Enter at
            // the end of a line makes (docs/specs/changes.md §5.2).
            if block.element.text.is_empty() || own_block {
                block.marks.inserted = Some(c);
            } else {
                block.marks.split = Some(c);
            }
        }
        ops.push(Op::Insert {
            index: index + 1,
            block,
        });
        Ok((Transaction(ops), json!({ "id": id })))
    }

    fn plan_join(
        &mut self,
        p: &BlockJoin,
        ctx: Option<&mut TrackCtx>,
    ) -> Result<(Transaction, Value), CommandError> {
        let index = self.index(p.id)?;
        if index == 0 {
            return Err(CommandError::invalid_params(
                "the first block has no block before it to join",
            ));
        }
        self.refuse_pending(index)?;
        self.refuse_pending(index - 1)?;
        let prev = &self.doc.blocks[index - 1];
        let at = prev.element.text.char_len();
        let prev_id = prev.id;
        // A join is checked when it is proposed, as when it is done.
        let mut merged = prev.element.clone();
        merged.text = merged.text.concat(&self.doc.blocks[index].element.text);
        Self::check(&merged)?;
        if let Some(ctx) = ctx {
            let marks = &self.doc.blocks[index].marks;
            if !self.only_own_insert(ctx, marks) {
                // Joining another's block is proposed, not done: the block
                // stays until the change is accepted.
                let mut marks = marks.clone();
                if marks.joined.is_some() {
                    return Ok((Transaction::default(), json!({ "id": prev_id, "at": at })));
                }
                marks.joined = Some(ctx.change(&mut self.doc, None));
                return Ok((
                    Transaction(vec![Op::Mark { index, marks }]),
                    json!({ "id": prev_id, "at": at }),
                ));
            }
        }
        let mut ops = self.replace_op(index - 1, merged).0;
        ops.push(Op::Remove { index });
        Ok((Transaction(ops), json!({ "id": prev_id, "at": at })))
    }

    fn plan_settle(
        &mut self,
        how: Settle,
        p: &ChangesSettle,
        by: &Author,
    ) -> Result<(Transaction, Value), CommandError> {
        if how == Settle::Accept && !by.is_writer() {
            return Err(CommandError::new(
                "writer_only",
                "only the writer may accept tracked changes; an agent may reject its own",
            ));
        }
        let given = [p.id.is_some(), p.scene.is_some(), p.all]
            .iter()
            .filter(|&&x| x)
            .count();
        if given != 1 {
            return Err(CommandError::invalid_params(
                "give exactly one of \"id\", \"scene\" and \"all\"",
            ));
        }
        let pending = self.doc.pending();
        let mine = ChangeBy::of(by);
        let is_mine = |c: &ChangeId| self.doc.changes.get(*c).is_some_and(|e| e.by == mine);
        let (changes, blocks) = if let Some(id) = p.id {
            if !pending.contains(&id) {
                return Err(CommandError::new(
                    "change_not_found",
                    format!("there is no pending change {id}"),
                )
                .with_details(json!({ "id": id })));
            }
            if !by.is_writer() && !is_mine(&id) {
                return Err(track::not_your_change(&format!(
                    "{id} is another author's change"
                )));
            }
            (std::iter::once(id).collect(), None)
        } else {
            let blocks = match p.scene {
                Some(s) => {
                    let r = self.scope_range(Some(s))?;
                    Some(
                        self.doc.blocks[r]
                            .iter()
                            .map(|b| b.id)
                            .collect::<std::collections::BTreeSet<_>>(),
                    )
                }
                None => None,
            };
            let changes = pending
                .iter()
                .copied()
                .filter(|c| by.is_writer() || is_mine(c))
                .collect();
            (changes, blocks)
        };
        let sel = track::Selection { changes, blocks };
        let out = track::settle(&self.doc, how, &sel);
        if let Some(id) = p.id
            && out.skipped.contains(&id)
        {
            return Err(CommandError::new(
                "change_conflict",
                format!(
                    "{id} cannot be carried out as it stands: a scene it moved has lost its old place, or blocks it would join no longer can be; settle it the other way, or edit the script yourself"
                ),
            )
            .with_details(json!({ "id": id })));
        }
        Ok((
            Transaction(out.ops),
            json!({
                "changes": out.settled,
                "count": out.settled.len(),
                "skipped": out.skipped,
            }),
        ))
    }

    fn plan_track(
        &mut self,
        p: &ChangesTrack,
        by: &Author,
    ) -> Result<(Transaction, Value), CommandError> {
        if !by.is_writer() {
            return Err(CommandError::new(
                "writer_only",
                "only the writer chooses which edits are tracked",
            ));
        }
        let mut tracking = self.doc.tracking;
        if let Some(w) = p.writer {
            tracking.writer = w;
        }
        if let Some(a) = p.agents {
            tracking.agents = a;
        }
        let result = json!({ "writer": tracking.writer, "agents": tracking.agents });
        if tracking == self.doc.tracking {
            return Ok((Transaction::default(), result));
        }
        Ok((Transaction(vec![Op::Track { tracking }]), result))
    }

    /// `revision.start` (docs/specs/revisions.md §5): its params already
    /// have their defaults filled in.
    fn plan_revision_start(
        &mut self,
        p: &RevisionStart,
        by: &Author,
        t: u64,
    ) -> Result<(Transaction, Value), CommandError> {
        revision_writer(by, "start a revision set")?;
        let table = &self.doc.revisions;
        if let Some(a) = table.active {
            return Err(CommandError::new(
                "revision_active",
                format!("{a} is active: end it before starting another"),
            )
            .with_details(json!({ "active": a })));
        }
        let (Some(color), Some(label), Some(date)) = (&p.color, &p.label, &p.date) else {
            return Err(CommandError::invalid_params(
                "revision.start needs a color, a label and a date",
            ));
        };
        revisions::check_name(color, "color", revisions::MAX_COLOR)
            .and_then(|()| revisions::check_name(label, "label", revisions::MAX_LABEL))
            .and_then(|()| revisions::parse_date(date).map(|_| ()))
            .map_err(CommandError::invalid_params)?;
        let mut next = table.clone();
        next.base.clear();
        let set = RevisionSet {
            id: next.next_id(),
            color: color.clone(),
            label: label.clone(),
            date: date.clone(),
            started: t,
            ended: None,
        };
        next.active = Some(set.id);
        next.sets.push(set.clone());
        Ok((
            Transaction(vec![Op::Revisions {
                revisions: Box::new(next),
            }]),
            json!({ "set": set }),
        ))
    }

    fn plan_revision_end(
        &mut self,
        by: &Author,
        t: u64,
    ) -> Result<(Transaction, Value), CommandError> {
        revision_writer(by, "end a revision set")?;
        let mut next = self.doc.revisions.clone();
        let Some(i) = next.active.and_then(|a| usize::try_from(a.0 - 1).ok()) else {
            return Err(CommandError::new(
                "revision_not_active",
                "no revision set is active",
            ));
        };
        next.active = None;
        next.base.clear();
        let Some(set) = next.sets.get_mut(i) else {
            return Err(CommandError::new(
                "revision_not_active",
                "no revision set is active",
            ));
        };
        set.ended = Some(t);
        let result = json!({ "set": set });
        Ok((
            Transaction(vec![Op::Revisions {
                revisions: Box::new(next),
            }]),
            result,
        ))
    }

    fn plan_revision_colors(
        &mut self,
        p: &RevisionColors,
        by: &Author,
    ) -> Result<(Transaction, Value), CommandError> {
        revision_writer(by, "change the revision colors")?;
        revisions::check_colors(&p.colors).map_err(CommandError::invalid_params)?;
        let result = json!({ "colors": p.colors });
        if p.colors == self.doc.revisions.colors {
            return Ok((Transaction::default(), result));
        }
        let mut next = self.doc.revisions.clone();
        next.colors = p.colors.clone();
        Ok((
            Transaction(vec![Op::Revisions {
                revisions: Box::new(next),
            }]),
            result,
        ))
    }

    fn plan_revision_clear(
        &mut self,
        p: &RevisionClear,
        by: &Author,
    ) -> Result<(Transaction, Value), CommandError> {
        revision_writer(by, "clear revision marks")?;
        if p.sets && !p.all {
            return Err(CommandError::invalid_params("\"sets\" goes with \"all\""));
        }
        let which: Box<dyn Fn(RevisionId) -> bool> = match (p.set, p.all) {
            (Some(s), false) => {
                if self.doc.revisions.get(s).is_none() {
                    return Err(CommandError::new(
                        "revision_not_found",
                        format!("no revision set {s}"),
                    ));
                }
                Box::new(move |r| r == s)
            }
            (None, true) => Box::new(|_| true),
            _ => {
                return Err(CommandError::invalid_params(
                    "give exactly one of \"set\" and \"all\"",
                ));
            }
        };
        let mut ops = Vec::new();
        let mut count = 0;
        for (index, b) in self.doc.blocks.iter().enumerate() {
            let mut touched = false;
            if b.rev.is_some_and(&which) {
                ops.push(Op::Rev { index, rev: None });
                touched = true;
            }
            if b.element
                .text
                .runs()
                .iter()
                .any(|r| r.marks().rev.is_some_and(&which))
            {
                let runs = b
                    .element
                    .text
                    .runs()
                    .iter()
                    .map(|r| match r {
                        coldopen_screenplay::Run::Text { text, style, marks }
                            if marks.rev.is_some_and(&which) =>
                        {
                            coldopen_screenplay::Run::marked(
                                text.clone(),
                                *style,
                                Marks {
                                    rev: None,
                                    ..*marks
                                },
                            )
                        }
                        other => other.clone(),
                    })
                    .collect();
                let mut element = b.element.clone();
                element.text = RichText::new(runs);
                ops.push(Op::Replace { index, element });
                touched = true;
            }
            count += usize::from(touched);
        }
        if p.sets && !self.doc.revisions.is_default() {
            ops.push(Op::Revisions {
                revisions: Box::default(),
            });
        } else if !self.doc.revisions.base.is_empty()
            && self.doc.revisions.active.is_some_and(&which)
        {
            // The active set starts afresh from the text as it now stands
            // (docs/specs/revisions.md §6.4).
            let mut next = self.doc.revisions.clone();
            next.base.clear();
            ops.push(Op::Revisions {
                revisions: Box::new(next),
            });
        }
        Ok((Transaction(ops), json!({ "count": count })))
    }

    /// `revision.list`: every set, oldest first, with the blocks holding
    /// its marks (docs/specs/revisions.md §5).
    fn revision_list(&mut self) -> Value {
        let pages = self.revision_pages();
        let table = &self.doc.revisions;
        let mut blocks = vec![0usize; table.sets.len()];
        for b in &self.doc.blocks {
            let mut seen: Vec<RevisionId> = b.rev_ids().collect();
            seen.sort();
            seen.dedup();
            for r in seen {
                if let Some(n) = usize::try_from(r.0)
                    .ok()
                    .and_then(|i| blocks.get_mut(i - 1))
                {
                    *n += 1;
                }
            }
        }
        let sets: Vec<Value> = table
            .sets
            .iter()
            .zip(blocks)
            .map(|(s, n)| {
                let mut v = json!(s);
                if let Value::Object(m) = &mut v {
                    m.insert("blocks".into(), json!(n));
                    m.insert(
                        "pages".into(),
                        json!(pages.iter().filter(|&&p| p == s.id).count()),
                    );
                }
                v
            })
            .collect();
        json!({ "active": table.active, "colors": table.colors, "sets": sets })
    }

    /// For each body page with a revision header, the set it names.
    fn revision_pages(&mut self) -> Vec<RevisionId> {
        if !self.doc.has_revision_data() {
            return Vec::new();
        }
        let map = self.view().map.clone();
        let marks = self.doc.revision_marks(&map);
        let mut layout = self.view().layout().clone();
        layout.mark_revisions(&marks);
        layout
            .pages
            .iter()
            .filter_map(|p| p.revision.map(RevisionId))
            .collect()
    }
}

/// Revision sets are the production's record: only the writer starts,
/// ends, recolors or clears them (docs/specs/revisions.md §5).
fn revision_writer(by: &Author, what: &str) -> Result<(), CommandError> {
    if by.is_writer() {
        Ok(())
    } else {
        Err(CommandError::new(
            "writer_only",
            format!("only the writer can {what}"),
        ))
    }
}

impl Command {
    /// `revision.start` with the defaults that depend on the document and
    /// the time filled in, so the journal records them
    /// (docs/specs/revisions.md §5). Other commands are returned as they are.
    fn with_revision_defaults(self, doc: &Document, t: u64) -> Command {
        match self {
            Command::RevisionStart(p) => {
                let color = p
                    .color
                    .map(|c| c.trim().to_string())
                    .unwrap_or_else(|| doc.revisions.next_color());
                let label = p.label.unwrap_or_else(|| format!("{color} Rev."));
                let date = p.date.unwrap_or_else(|| revisions::utc_date(t));
                Command::RevisionStart(RevisionStart {
                    color: Some(color),
                    label: Some(label),
                    date: Some(date),
                })
            }
            other => other,
        }
    }
}

/// Whether an operation leaves a mark of change `c`.
fn op_names(op: &Op, c: ChangeId) -> bool {
    match op {
        Op::Insert { block, .. } => block.change_ids().any(|x| x == c),
        Op::Replace { element, .. } => crate::changes::text_ids(&element.text).any(|x| x == c),
        Op::Mark { marks, .. } => marks.ids().any(|x| x == c),
        Op::Remove { .. }
        | Op::Move { .. }
        | Op::Track { .. }
        | Op::Rev { .. }
        | Op::Revisions { .. }
        | Op::RevBase { .. }
        | Op::TitlePage { .. } => false,
    }
}

/// New text whose current text is the old one's: the old marks are kept,
/// with the new text's styles, notes and boneyard (a tracked
/// `block.set_text` that changes no words is not a tracked change).
fn keep_marks(old: &RichText, new: &RichText) -> RichText {
    track::set_text(old, new, ChangeId(u64::MAX), &|_| false)
}

/// Losses as commands report them, with the id of the block each is about.
pub(crate) fn losses_json(doc: &Document, losses: &[Loss]) -> Vec<Value> {
    losses
        .iter()
        .map(|l| {
            let id = l.element.and_then(|i| doc.blocks.get(i)).map(|b| b.id);
            json!({ "id": id, "code": l.code, "class": l.class.name(), "message": l.message })
        })
        .collect()
}
