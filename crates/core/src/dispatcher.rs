//! The dispatcher: the one place every front end sends commands. It owns
//! the open session and handles opening, saving, closing and recovery
//! (docs/specs/commands.md, docs/specs/journal.md).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::author::{Author, Clock};
use crate::command::{
    ChangesSettle, Class, Command, DocClose, DocNew, DocOpen, DocSave, ExportFile, ExportPdf,
    FileRepair, Recover, RecoveredDelete, RecoveredList, RecoveredRestore, RevisionClear,
};
use crate::document::Document;
use crate::error::CommandError;
use crate::flush::{Durability, FlushStatus};
use crate::format::{FormatId, ReadOut, unsupported};
use crate::io::{self, Fingerprint, SourceStyle};
use crate::journal::{self, Journal, OpenError};
use crate::session::{Exec, Merge, Session};
use crate::view::View;

/// How a dispatcher treats files.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Keep a crash journal next to each open document.
    pub journal: bool,
    /// Flush every journal entry to disk, not just to the OS.
    pub fsync: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            journal: true,
            fsync: true,
        }
    }
}

#[derive(Debug, Default)]
pub struct Dispatcher {
    session: Option<Session>,
    opts: Options,
    clock: Clock,
    /// Overrides the durability `opts.fsync` implies.
    durability: Option<Durability>,
    /// Overrides each document's switch for agents' edits
    /// (`Session::agent_tracking`).
    agent_tracking: Option<bool>,
    /// The writer `execute` runs commands as: their name is what their
    /// tracked changes carry (docs/specs/commands.md, "Authors").
    writer: Author,
}

/// A save in place, prepared on the UI thread so the file can be written
/// on another (docs/specs/journal.md, "Saving off the UI thread").
#[derive(Clone, Debug)]
pub struct PreparedSave {
    pub target: PathBuf,
    pub bytes: Vec<u8>,
    format: String,
    losses: Vec<Value>,
    rev: u64,
    fingerprint: Fingerprint,
    /// The session it was prepared for, and its number among that
    /// session's saves: only that session's outstanding save may commit.
    session: u64,
    number: u64,
    /// The damaged file kept before this save, if it was the first save
    /// in place of a repaired file.
    original: Option<PathBuf>,
    /// What the save did before it was taken, such as accepting tracked
    /// changes (`changes_accepted`), for the commit's warnings.
    notes: Vec<Value>,
    /// Waits for the journal's flush of the save marker (a deferred
    /// journal only); see [`PreparedSave::wait_for_journal`].
    journal: Option<crate::flush::FlushWaiter>,
}

impl PreparedSave {
    /// Step 4's end, on the thread that wrote the file: waits, for at most
    /// two seconds, until the journal is flushed up to the save marker.
    /// The commit then starts the journal again with no flush of it in
    /// flight. On Linux, cutting a file back waits for the writes in
    /// flight to it, and on a stalled disk that held the UI thread for
    /// 76 ms (docs/specs/journal.md, "Saving off the UI thread",
    /// 2026-10-21).
    pub fn wait_for_journal(&self) {
        if let Some(w) = &self.journal {
            w.wait(std::time::Duration::from_secs(2));
        }
    }

    /// The same wait, as a handle to move to the thread that writes.
    pub fn journal_waiter(&self) -> Option<crate::flush::FlushWaiter> {
        self.journal.clone()
    }
}

/// A save in place taken on the UI thread, to be written out on another
/// (docs/specs/journal.md, "Saving off the UI thread"): a snapshot of the
/// document and what the save needs to know about the session.
#[derive(Clone, Debug)]
pub struct SaveJob {
    doc: Document,
    style: SourceStyle,
    path: PathBuf,
    params: DocSave,
    /// What the session last read or wrote, for the check that no other
    /// program changed the file.
    disk: Option<Fingerprint>,
    damaged: bool,
    rev: u64,
    session: u64,
    number: u64,
    notes: Vec<Value>,
}

/// Which save a [`SaveJob`] is, so a caller whose worker never answered
/// can still end it with [`RenderedSave::failed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveTicket {
    session: u64,
    number: u64,
}

impl SaveJob {
    pub fn ticket(&self) -> SaveTicket {
        SaveTicket {
            session: self.session,
            number: self.number,
        }
    }

    /// Step 2, on any thread: writes the document out in its format and
    /// checks it can be saved: no loss without `allow_lossy`, and a file no
    /// other program changed. The first save in place of a repaired file
    /// keeps a copy of it first. Nothing else touches the disk.
    pub fn render(self) -> RenderedSave {
        let result = (|| {
            let r = render_doc(&self.doc, self.style, Some(&self.path), &self.params)?;
            refuse_if_changed_from(self.disk, &r.target, self.params.overwrite)?;
            let original = if self.damaged {
                keep_original(&r.target)?
            } else {
                None
            };
            Ok(PreparedSave {
                fingerprint: Fingerprint::of(&r.bytes),
                target: r.target,
                bytes: r.bytes,
                format: r.format.name().to_string(),
                losses: r.losses,
                rev: self.rev,
                session: self.session,
                number: self.number,
                original,
                notes: self.notes,
                journal: None,
            })
        })();
        RenderedSave {
            session: self.session,
            number: self.number,
            result,
        }
    }
}

/// A [`SaveJob`] rendered, or the reason it can't be saved, for
/// [`Dispatcher::save_mark`].
#[derive(Clone, Debug)]
pub struct RenderedSave {
    session: u64,
    number: u64,
    result: Result<PreparedSave, CommandError>,
}

impl RenderedSave {
    /// The outcome of a save whose rendering never came back, such as a
    /// worker that could not start: the save ends with `error`.
    pub fn failed(ticket: SaveTicket, error: CommandError) -> RenderedSave {
        RenderedSave {
            session: ticket.session,
            number: ticket.number,
            result: Err(error),
        }
    }
}

/// What opening a document would make of its crash journal, worked out
/// without touching the disk (docs/specs/journal.md, "Recovery on open").
enum Assessment {
    /// The header cannot be read.
    Unreadable(String),
    Replayed(Box<Replayed>),
}

/// A crash journal replayed as far as it can be trusted.
struct Replayed {
    session: Session,
    /// Bytes and entries up to the last trusted entry.
    valid_len: u64,
    applied: u64,
    /// Commands replayed; markers are not counted.
    commands: usize,
    /// The newest replayed entry's time.
    last: u64,
    /// Why replay stopped early, if it did.
    problem: Option<String>,
    /// True when the journal holds work the file does not.
    recover: bool,
    external_change: bool,
    /// Whether any of the work is a writer's.
    writer: bool,
    /// Set when the header names another computer or another path: the
    /// journal was synced or copied here. Details for `journal_foreign`.
    foreign: Option<Value>,
    /// Warnings recovery adds, such as `change_data_missing`.
    notes: Vec<Value>,
    /// The semantics the last trusted entries were replayed under: the
    /// header's, or a marker's after it.
    semantics: u32,
}

impl Replayed {
    /// What a copy of a journal that replay stopped early is called.
    fn aside_tag(msg: &str) -> &'static str {
        if msg.contains("different result") || msg.contains("different step") {
            "diverged"
        } else {
            "corrupt"
        }
    }
}

/// The state replay reached.
struct Replay {
    session: Session,
    known_disk: Vec<Fingerprint>,
    /// Entries trusted, markers included, and the bytes they end at.
    applied: usize,
    valid_len: u64,
    commands: usize,
    last: u64,
    writer: bool,
    problem: Option<String>,
    /// The entry after the trusted ones was applied, then gave a
    /// different result.
    diverged: bool,
    /// The computer and path of the newest adoption marker, which stand
    /// for the header's from there on.
    adopted: Option<(String, String)>,
    /// The semantics in force: the header's, then each marker's.
    semantics: u32,
}

/// A document written out for saving, before anything touches the disk.
struct Rendered {
    target: PathBuf,
    format: FormatId,
    bytes: Vec<u8>,
    losses: Vec<Value>,
}

fn warning(code: &str, message: impl Into<String>) -> Value {
    json!({ "code": code, "message": message.into() })
}

/// A warning about work set aside: the path is in the message for the CLI
/// and JSON clients, and in `details.copy` for front ends that show their
/// own words instead (docs/specs/journal.md, "Recovered versions").
fn set_aside(code: &str, message: String, copy: &Path) -> Value {
    json!({
        "code": code,
        "message": message,
        "details": { "copy": copy.display().to_string() },
    })
}

fn locked(path: &Path) -> CommandError {
    CommandError::new(
        "doc_locked",
        format!("{} is open in another Cold Open process", path.display()),
    )
    .with_details(json!({ "path": path.display().to_string() }))
}

fn open_journal(
    path: &Path,
    format: FormatId,
    durability: Durability,
) -> Result<(Journal, Vec<u8>), CommandError> {
    let jpath = journal::path_for(path);
    let (mut j, contents) = Journal::open_locked(&jpath, durability).map_err(|e| match e {
        OpenError::Locked => locked(path),
        OpenError::Io(e) => CommandError::io(&jpath, &e),
    })?;
    j.set_identity(Some(this_host().to_string()), Some(canonical(path)));
    j.set_format(format);
    Ok((j, contents))
}

/// This computer's name, as journal headers record it.
fn this_host() -> &'static str {
    static HOST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HOST.get_or_init(|| gethostname::gethostname().to_string_lossy().into_owned())
}

/// A script's canonical path, as journal headers record it, also for a
/// script whose file does not exist yet.
fn canonical(path: &Path) -> String {
    fs::canonicalize(path)
        .or_else(|_| {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            fs::canonicalize(parent).map(|d| d.join(path.file_name().unwrap_or_default()))
        })
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

impl Dispatcher {
    pub fn new(opts: Options) -> Dispatcher {
        Dispatcher {
            session: None,
            opts,
            clock: Clock::default(),
            durability: None,
            agent_tracking: None,
            writer: Author::writer(),
        }
    }

    /// Names the writer whose tracked changes `execute` records.
    pub fn set_writer_name(&mut self, name: Option<String>) {
        self.writer = Author::Writer { name };
    }

    /// The writer `execute` runs commands as.
    pub fn writer(&self) -> &Author {
        &self.writer
    }

    /// Tracks agents' edits, or not, whatever each document's switch says,
    /// as `coldopen mcp --untracked` asks (docs/specs/mcp.md §6.1). Applies
    /// to documents opened from now on.
    pub fn set_agent_tracking(&mut self, tracked: Option<bool>) {
        self.agent_tracking = tracked;
        if let Some(s) = &mut self.session {
            s.agent_tracking = tracked;
        }
    }

    /// Chooses how journals reach the disk; the desktop app uses
    /// [`Durability::APP`]. Applies to journals opened from now on.
    pub fn set_durability(&mut self, d: Durability) {
        self.durability = Some(d);
    }

    fn durability(&self) -> Durability {
        self.durability.unwrap_or(if self.opts.fsync {
            Durability::EveryEntry
        } else {
            Durability::OsOnly
        })
    }

    /// The open document laid out and up to date, for drawing it.
    pub fn view(&mut self) -> Option<&View> {
        Some(self.session.as_mut()?.view())
    }

    /// The open document's layout as last brought up to date by
    /// [`Dispatcher::view`].
    pub fn cached_view(&self) -> Option<&View> {
        self.session.as_ref()?.cached_view()
    }

    /// The open journal's flushing state, in deferred mode.
    pub fn journal_status(&self) -> Option<FlushStatus> {
        self.session.as_ref()?.journal.as_ref()?.status()
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// The open journal's bytes, read through its lock, for tests and
    /// diagnostics: on Windows the file cannot be read from outside while
    /// it is locked.
    #[doc(hidden)]
    pub fn journal_bytes(&mut self) -> Option<Vec<u8>> {
        self.session.as_mut()?.journal.as_mut()?.contents().ok()
    }

    /// Replaces the clock used to group typing into undo steps, for tests.
    pub fn set_clock(&mut self, clock: Clock) {
        if let Some(s) = &mut self.session {
            s.clock = clock.clone();
        }
        self.clock = clock;
    }

    fn new_session(
        &self,
        doc: Document,
        path: Option<PathBuf>,
        format: FormatId,
        style: SourceStyle,
    ) -> Session {
        let mut s = Session::new(doc, path, format, style);
        s.clock = self.clock.clone();
        s.agent_tracking = self.agent_tracking;
        s
    }

    /// Runs one command as a writer.
    pub fn execute(&mut self, cmd: Command) -> Result<Value, CommandError> {
        let writer = self.writer.clone();
        self.execute_as(cmd, &writer)
    }

    /// Runs one command for an author.
    pub fn execute_as(&mut self, cmd: Command, by: &Author) -> Result<Value, CommandError> {
        match cmd {
            Command::DocNew(p) => self.doc_new(p),
            Command::DocOpen(p) => self.doc_open(p, by),
            Command::DocSave(p) => self.doc_save(p, by),
            Command::DocClose(p) => self.doc_close(p, by),
            Command::DocRevert(_) => self.doc_revert(by),
            Command::ExportPdf(p) => self.export_pdf(p, by),
            Command::ExportFdx(p) => self.export_file(FormatId::Fdx, p, by),
            Command::ExportFountain(p) => self.export_file(FormatId::Fountain, p, by),
            Command::FileRepair(p) => self.file_repair(p, by),
            Command::RecoveredList(p) => self.recovered_list(p),
            Command::RecoveredRestore(p) => self.recovered_restore(p, by),
            Command::RecoveredDelete(p) => self.recovered_delete(p, by),
            Command::Diff(p) => self.diff(p, by),
            Command::ChangesImport(p) => self.changes_import(p, by),
            other => self
                .session
                .as_mut()
                .ok_or_else(CommandError::no_document)?
                .execute_with(&other, &Exec::by(by.clone())),
        }
    }

    /// Ends the session at end of input. Unsaved work stays in the journal
    /// for recovery; a clean session removes its journal.
    pub fn shutdown(&mut self) {
        if let Some(s) = self.session.take()
            && !s.dirty
            && let Some(j) = s.journal
        {
            let _ = j.delete();
        }
    }

    /// Ends the session, throwing away unsaved changes. A journal that
    /// holds any is copied aside first, and the copy's path is returned
    /// (docs/specs/journal.md, "Closing"). If no copy can be made, the
    /// journal is left in place, as after a crash.
    pub fn discard(&mut self) -> Option<PathBuf> {
        match self.keep_copy() {
            Ok(kept) => {
                if let Some(s) = self.session.take()
                    && let Some(j) = s.journal
                {
                    let _ = j.delete();
                }
                kept
            }
            Err(_) => {
                self.session = None;
                None
            }
        }
    }

    /// Ends the session, throwing away unsaved changes that the caller can
    /// make again, such as a `coldopen run` batch, without keeping a copy.
    /// A session that holds recovered work is discarded as by
    /// [`Dispatcher::discard`], with a copy, because that work was never
    /// the caller's.
    pub fn discard_rerunnable(&mut self) -> Option<PathBuf> {
        if self.session.as_ref().is_some_and(|s| s.recovered.is_some()) {
            return self.discard();
        }
        if let Some(s) = self.session.take()
            && let Some(j) = s.journal
        {
            let _ = j.delete();
        }
        None
    }

    /// Copies the journal to `<journal>.discarded-<ms>` when the session
    /// holds unsaved work.
    fn keep_copy(&mut self) -> std::io::Result<Option<PathBuf>> {
        let Some(s) = &mut self.session else {
            return Ok(None);
        };
        let dirty = s.dirty;
        let Some(j) = &mut s.journal else {
            return Ok(None);
        };
        if !dirty && j.entries() == 0 {
            return Ok(None);
        }
        let contents = j.contents()?;
        j.copy_aside(&contents, "discarded").map(Some)
    }

    /// Closes a clean session before opening another one.
    fn close_for_switch(&mut self) -> Result<(), CommandError> {
        self.refuse_if_dirty()?;
        self.shutdown();
        Ok(())
    }

    fn refuse_if_dirty(&self) -> Result<(), CommandError> {
        if self.session.as_ref().is_some_and(|s| s.dirty) {
            return Err(CommandError::new(
                "unsaved_changes",
                "the open document has unsaved changes; save it or close it with discard",
            ));
        }
        Ok(())
    }

    fn doc_new(&mut self, p: DocNew) -> Result<Value, CommandError> {
        // Every check runs before the open document is closed or anything
        // is written (docs/specs/journal.md).
        self.refuse_if_dirty()?;
        let path = p.path.as_ref().map(PathBuf::from);
        let mut journal = None;
        let mut format = FormatId::default();
        if let Some(path) = &path {
            if path.exists() {
                return Err(CommandError::new(
                    "already_exists",
                    format!("{} already exists; open it instead", path.display()),
                ));
            }
            format = FormatId::for_path(path).ok_or_else(|| unsupported(path))?;
            if let Some(own) = self.session.as_ref().and_then(|s| s.path.as_deref())
                && same_path(own, path)
            {
                return Err(CommandError::new(
                    "already_exists",
                    format!("{} is already open", path.display()),
                ));
            }
            if self.opts.journal {
                // A document made with doc.new and never saved lives only
                // in its journal; never start over on top of one.
                let (j, contents) = open_journal(path, format, self.durability())?;
                if !contents.is_empty() {
                    return Err(CommandError::new(
                        "already_exists",
                        format!(
                            "{} has a crash journal, which may hold work that was never saved; \
                             open it with doc.open to recover that work, or with recover \"discard\" \
                             to set it aside",
                            path.display()
                        ),
                    )
                    .with_details(json!({
                        "path": path.display().to_string(),
                        "journal": j.path().display().to_string(),
                    })));
                }
                journal = Some(j);
            }
        }

        self.shutdown();
        let mut doc = Document {
            next_id: 1,
            doc_id: (format == FormatId::Bundle).then(crate::bundle::new_doc_id),
            ..Default::default()
        };
        if let Some(kind) = p.start
            && let Some(id) = doc.allocate_id()
        {
            doc.blocks.push(crate::document::Block::new(
                id,
                coldopen_screenplay::Element::new(kind, ""),
            ));
        }
        let mut warnings = Vec::new();
        if path.is_none() {
            warnings.push(warning(
                "unjournaled",
                "an untitled document is not protected by a crash journal until it is saved",
            ));
        }
        let mut session = self.new_session(doc, path, format, SourceStyle::default());
        if let Some(mut j) = journal {
            j.start(&session.doc, None)
                .map_err(|e| CommandError::io(j.path(), &e))?;
            session.journal = Some(j);
        }
        let path = session.path.as_ref().map(|p| p.display().to_string());
        self.session = Some(session);
        Ok(json!({ "path": path, "warnings": warnings }))
    }

    fn doc_open(&mut self, p: DocOpen, by: &Author) -> Result<Value, CommandError> {
        self.close_for_switch()?;
        let path = PathBuf::from(&p.path);
        let format = FormatId::for_path(&path).ok_or_else(|| unsupported(&path))?;
        // A file that was never saved can still have a crash journal
        // (docs/specs/journal.md, "A file that does not exist").
        let (bytes, missing) = match fs::read(&path) {
            Ok(b) => (b, false),
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && self.opts.journal
                    && journal::path_for(&path).exists() =>
            {
                (Vec::new(), true)
            }
            Err(e) => return Err(CommandError::io(&path, &e)),
        };
        let ReadOut {
            doc: disk_doc,
            style,
            mut warnings,
            repaired,
        } = format.get().read(&bytes)?;
        let mut session = self.new_session(disk_doc.clone(), Some(path.clone()), format, style);
        let mut recovered = Value::Null;

        if self.opts.journal {
            let (mut j, contents) = open_journal(&path, format, self.durability())?;
            let mut resumed = false;
            if !contents.is_empty() {
                match p.recover {
                    Recover::Discard => {
                        if !by.is_writer()
                            && let Assessment::Replayed(r) =
                                self.assess(&path, format, style, &contents, (&bytes, missing))
                            && r.recover
                            && r.writer
                        {
                            return Err(writers_work("set aside"));
                        }
                        let aside = j
                            .copy_aside(&contents, "discarded")
                            .map_err(|e| CommandError::io(j.path(), &e))?;
                        warnings.push(set_aside(
                            "journal_discarded",
                            format!("unsaved changes were set aside in {}", aside.display()),
                            &aside,
                        ));
                    }
                    Recover::Auto | Recover::Apply => {
                        let jio = |e: std::io::Error, j: &Journal| CommandError::io(j.path(), &e);
                        match self.assess(&path, format, style, &contents, (&bytes, missing)) {
                            Assessment::Unreadable(msg) => {
                                let aside =
                                    j.copy_aside(&contents, "corrupt").map_err(|e| jio(e, &j))?;
                                warnings.push(set_aside(
                                    "journal_unreadable",
                                    format!("{msg}; it was kept in {}", aside.display()),
                                    &aside,
                                ));
                            }
                            Assessment::Replayed(r) => {
                                // A journal synced or copied from elsewhere
                                // is offered, never applied on its own.
                                if r.recover
                                    && p.recover == Recover::Auto
                                    && let Some(details) = r.foreign
                                {
                                    return Err(foreign_journal(&path, details));
                                }
                                if let Some(msg) = &r.problem {
                                    let aside = j
                                        .copy_aside(&contents, Replayed::aside_tag(msg))
                                        .map_err(|e| jio(e, &j))?;
                                    warnings.push(set_aside(
                                        "journal_partial",
                                        format!(
                                            "recovered {} of the journal's changes; {msg}; the journal was kept in {}",
                                            r.commands,
                                            aside.display()
                                        ),
                                        &aside,
                                    ));
                                }
                                if r.recover {
                                    warnings.extend(r.notes.iter().cloned());
                                    j.resume(r.valid_len, r.applied).map_err(|e| jio(e, &j))?;
                                    // The writer chose to recover a journal
                                    // from somewhere else: it is this
                                    // computer's now, so a crash here does
                                    // not ask again.
                                    if r.foreign.is_some() {
                                        j.append_adopted().map_err(|e| jio(e, &j))?;
                                    }
                                    // Its header still says what its entries
                                    // mean; the ones made from now on mean
                                    // what this version makes them mean.
                                    if r.semantics < journal::SEMANTICS {
                                        j.append_semantics().map_err(|e| jio(e, &j))?;
                                    }
                                    recovered = json!({
                                        "commands": r.commands,
                                        "external_change": r.external_change,
                                        "last": r.last,
                                    });
                                    let mut replay = r.session;
                                    replay.dirty = true;
                                    replay.recovered = Some(crate::session::Recovered {
                                        rev: replay.rev,
                                        writer: r.writer,
                                    });
                                    session = replay;
                                    resumed = true;
                                }
                            }
                        }
                    }
                }
            }
            if !resumed {
                let disk = (!missing).then(|| Fingerprint::of(&bytes));
                j.start(&disk_doc, disk)
                    .map_err(|e| CommandError::io(j.path(), &e))?;
            }
            session.journal = Some(j);
        }

        session.disk = (!missing).then(|| Fingerprint::of(&bytes));
        session.damaged = repaired.is_some();
        let result = json!({
            "path": path.display().to_string(),
            "format": format.name(),
            "blocks": session.doc.blocks.len(),
            "scenes": session.doc.scenes().len(),
            "warnings": warnings,
            "recovered": recovered,
            "repaired": repaired,
        });
        self.session = Some(session);
        Ok(result)
    }

    /// Replays the first `limit` entries of a journal read back, stopping
    /// at the first that cannot be trusted.
    fn replay(
        &self,
        read: &journal::ReadBack,
        path: &Path,
        (format, style): (FormatId, SourceStyle),
        limit: usize,
    ) -> Replay {
        let mut r = Replay {
            session: self.new_session(
                read.header.doc.clone(),
                Some(path.to_path_buf()),
                format,
                style,
            ),
            // Fingerprints the file may legitimately have: the header's, and
            // those of the saves the journal records.
            known_disk: read.header.disk.into_iter().collect(),
            applied: 0,
            commands: 0,
            last: 0,
            writer: false,
            valid_len: read.header_len(),
            problem: read.problem.clone(),
            diverged: false,
            adopted: None,
            semantics: read.header.semantics,
        };
        for (k, entry) in read.entries.iter().enumerate().take(limit) {
            if let Some(fp) = entry.saved_disk() {
                r.known_disk.push(fp);
                r.applied = k + 1;
                r.valid_len = read.entry_end(k);
                continue;
            }
            if let Some(who) = entry.adopted() {
                r.adopted = Some(who);
                r.applied = k + 1;
                r.valid_len = read.entry_end(k);
                continue;
            }
            // A semantics marker: the entries after it mean what that
            // version made them mean (docs/specs/journal.md, "Semantics
            // markers"). Markers only ever go up, to a version this one
            // knows.
            if let Some(n) = entry.semantics() {
                if n < r.semantics || n > journal::SEMANTICS {
                    r.problem = Some(format!(
                        "entry {} says the entries after it mean semantics {n}, which this version cannot replay after semantics {}",
                        entry.seq, r.semantics
                    ));
                    break;
                }
                r.semantics = n;
                r.applied = k + 1;
                r.valid_len = read.entry_end(k);
                continue;
            }
            let ex = Exec {
                by: entry.by.clone().unwrap_or_default(),
                merge: Merge::Recorded(entry.merge),
                seq: Some(entry.seq),
                step: entry.step.clone(),
                t: Some(entry.t),
                semantics: Some(r.semantics),
            };
            let outcome = Command::from_parts(&entry.command, entry.params.clone())
                .map_err(|e| format!("entry {} cannot be read as a command: {e}", entry.seq))
                .and_then(|cmd| {
                    // Only edits, undo and redo are ever journaled; anything
                    // else is a damaged or hand-made journal, and replaying
                    // it must never write files.
                    if matches!(cmd.class(), Class::Edit | Class::History) {
                        Ok(cmd)
                    } else {
                        Err(format!("entry {} is not an edit", entry.seq))
                    }
                })
                .and_then(|cmd| {
                    r.session
                        .execute_with(&cmd, &ex)
                        .map_err(|e| format!("entry {} failed on replay: {e}", entry.seq))
                })
                .and_then(|result| {
                    if reproduces(&entry.result, &result) {
                        Ok(())
                    } else {
                        r.diverged = true;
                        Err(format!(
                            "entry {} gave a different result on replay",
                            entry.seq
                        ))
                    }
                });
            if let Err(msg) = outcome {
                r.problem = Some(msg);
                break;
            }
            r.applied = k + 1;
            r.commands += 1;
            r.last = entry.t;
            r.writer |= ex.by.is_writer();
            r.valid_len = read.entry_end(k);
        }
        r
    }

    /// Replays a crash journal and judges whether it holds work the file
    /// does not, without touching the disk. `doc.open` acts on the answer;
    /// "save as" uses it to refuse a target with unsaved work.
    fn assess(
        &self,
        path: &Path,
        format: FormatId,
        style: SourceStyle,
        contents: &[u8],
        (disk_bytes, missing): (&[u8], bool),
    ) -> Assessment {
        let mut read = match journal::read(contents) {
            Ok(r) => r,
            Err(msg) => return Assessment::Unreadable(msg),
        };
        // A bundle's journal names its change table, which the bundle on
        // disk holds; and a bundle that is another document, or none, is a
        // change by another program (docs/specs/journal.md, "Recovery on
        // open").
        let mut notes = Vec::new();
        let mut elsewhere = false;
        if read.header.format == FormatId::Bundle && !missing {
            let found = crate::bundle::identity(disk_bytes);
            let table = found.as_ref().and_then(|(_, t)| t.as_ref());
            if read.header.changes_ref.is_some() && !table.is_some_and(|t| read.use_table(t)) {
                notes.push(warning(
                    "change_data_missing",
                    "the project on disk does not hold the change table the crash journal names; the changes it lacks show the author \"Unknown\"",
                ));
                elsewhere = true;
            }
            let disk_id = found.as_ref().and_then(|(id, _)| id.as_deref());
            if found.is_none()
                || (read.header.doc_id.is_some() && disk_id != read.header.doc_id.as_deref())
            {
                elsewhere = true;
            }
        }

        let mut r = self.replay(&read, path, (format, style), read.entries.len());
        if r.diverged {
            // The diverged entry was applied before its result was compared.
            // Replay the trusted prefix again, so the document holds exactly
            // what the journal will be cut to (review finding F9).
            let trusted = self.replay(&read, path, (format, style), r.applied);
            r.session = trusted.session;
            r.adopted = trusted.adopted;
            r.semantics = trusted.semantics;
        }
        let Replay {
            session: mut replay,
            known_disk,
            applied,
            commands,
            last,
            mut writer,
            valid_len,
            problem,
            adopted,
            semantics,
            ..
        } = r;

        // The file already holds the replayed document only if writing it
        // is lossless and gives the same bytes. A lossy write can match the
        // file's text while the document differs, so it always recovers.
        // It is written in the format the header names, so recovery compares
        // like with like.
        let in_file = |doc: &Document| {
            let written = read.header.format.get().write(doc, style);
            !written.is_lossy() && written.bytes == disk_bytes
        };
        let external_change = elsewhere
            || if missing {
                !known_disk.is_empty()
            } else {
                !known_disk.contains(&Fingerprint::of(disk_bytes))
            };
        let recover = if missing {
            true
        } else if commands == 0 {
            // With no entries, the header's document is in the file when the
            // file is the version the header or a marker names, or holds the
            // same document. Otherwise the header is the only copy of the
            // last save (a rename lost in a power cut), so it is recovered.
            external_change && !in_file(&replay.doc)
        } else {
            !in_file(&replay.doc)
        };
        // Work in the header alone has no author; it is treated as the
        // writer's, which is the cautious choice. So is a header that is
        // itself unsaved work, a restored copy, whoever made the entries
        // after it (2026-10-28, review finding GP4).
        writer |= commands == 0 || read.header.unsaved;
        replay.journal = None;
        // A journal this computer took over at this path is its own.
        let (host, at) = match &adopted {
            Some((h, p)) => (Some(h.as_str()), Some(p.as_str())),
            None => (read.header.host.as_deref(), read.header.path.as_deref()),
        };
        let here = canonical(path);
        let foreign = (host.is_some_and(|h| h != this_host()) || at.is_some_and(|p| p != here))
            .then(|| json!({ "host": host, "path": at }));
        Assessment::Replayed(Box::new(Replayed {
            foreign,
            notes,
            session: replay,
            valid_len,
            applied: applied as u64,
            commands,
            last,
            problem,
            recover,
            external_change,
            writer,
            semantics,
        }))
    }

    /// Writes the document out for saving and checks it can be saved, without
    /// touching the disk.
    fn render(session: &Session, p: &DocSave) -> Result<Rendered, CommandError> {
        render_doc(&session.doc, session.style, session.path.as_deref(), p)
    }

    fn doc_save(&mut self, p: DocSave, by: &Author) -> Result<Value, CommandError> {
        if p.overwrite && !by.is_writer() {
            return Err(writer_only(
                "replace a file that is not the open document, or that another program changed",
            ));
        }
        if p.allow_lossy && !by.is_writer() {
            // What Fountain drops is the writer's call: their own save asks
            // "Save anyway?" (docs/specs/editor.md §11).
            return Err(writer_only("save with losses (allow_lossy)"));
        }
        let opts = self.opts;
        let durability = self.durability();
        refuse_while_saving(
            self.session
                .as_ref()
                .ok_or_else(CommandError::no_document)?,
        )?;
        let accepted = self.accept_for_save(&p)?;
        let session = self
            .session
            .as_ref()
            .ok_or_else(CommandError::no_document)?;
        let Rendered {
            target,
            format,
            mut bytes,
            losses,
        } = Self::render(session, &p)?;
        let same_file = session
            .path
            .as_ref()
            .is_some_and(|old| same_path(old, &target));
        // A bundle first written at a path gets an identity of its own
        // (docs/specs/bundle.md §8); a save in place keeps it.
        let new_id = (format == FormatId::Bundle
            && (!same_file || session.format != FormatId::Bundle || session.doc.doc_id.is_none()))
        .then(crate::bundle::new_doc_id);
        if let Some(id) = &new_id {
            let mut doc = session.doc.clone();
            doc.doc_id = Some(id.clone());
            bytes = format.get().write(&doc, session.style).bytes;
        }
        if same_file {
            refuse_if_changed(session, &target, p.overwrite)?;
        } else if !p.overwrite && target.exists() {
            return Err(exists(&target));
        }
        let original = if same_file && session.damaged {
            keep_original(&target)?
        } else {
            None
        };
        // Save as: the target's lock is taken, and its journal judged,
        // before anything is written (docs/specs/journal.md, "Saving").
        let claimed = if opts.journal && !same_file {
            Some(self.claim_target(&target, format, durability)?)
        } else {
            None
        };
        if let Err(e) = io::atomic_write(&target, &bytes) {
            if let Some((j, true)) = claimed {
                // The empty journal this save made is not needed.
                let _ = j.delete();
            }
            return Err(CommandError::io(&target, &e));
        }
        let fingerprint = Fingerprint::of(&bytes);

        let session = self
            .session
            .as_mut()
            .ok_or_else(CommandError::no_document)?;
        if let Some(id) = new_id {
            session.doc.doc_id = Some(id);
        }
        // A conversion leaves the document in the new format's form before
        // the journal starts again from it.
        if session.format != format {
            crate::format::adopt(&mut session.doc, format);
        }
        if same_file {
            if let Some(j) = &mut session.journal {
                j.set_format(format);
                j.start(&session.doc, Some(fingerprint))
                    .map_err(|e| CommandError::io(j.path(), &e))?;
            }
        } else if let Some((mut j, _)) = claimed {
            j.start(&session.doc, Some(fingerprint))
                .map_err(|e| CommandError::io(j.path(), &e))?;
            if let Some(old) = session.journal.replace(j) {
                let _ = old.delete();
            }
        }
        session.journal_restarted();
        session.path = Some(target.clone());
        session.dirty = false;
        session.recovered = None;
        session.disk = Some(fingerprint);
        session.style = saved_style(session.style);
        session.set_format(format);
        session.damaged &= !same_file;
        Ok(json!({
            "path": target.display().to_string(),
            "format": format.name(),
            "bytes": bytes.len(),
            "losses": losses,
            "warnings": accepted
                .into_iter()
                .chain(original_warnings(original.as_deref()))
                .collect::<Vec<_>>(),
        }))
    }

    /// "Save anyway" to a format that cannot hold tracked changes
    /// (docs/specs/commands.md, `doc.save`, 2026-10-21): the document takes
    /// the form the file will hold, as a converting save does. The version
    /// with its changes marked is set aside first, as a Recovered version
    /// (docs/specs/journal.md), then every pending change is accepted as
    /// one undoable step, journaled like any other. The same holds for
    /// revision sets and marks (docs/specs/revisions.md §9), reset as one
    /// more step. Returns the warnings that say so, none when the save
    /// keeps everything or there is nothing to take out.
    fn accept_for_save(&mut self, p: &DocSave) -> Result<Vec<Value>, CommandError> {
        let Some(session) = self.session.as_mut() else {
            return Ok(Vec::new());
        };
        let target = p.path.as_deref().map(Path::new).or(session.path.as_deref());
        let format = match (&p.format, target) {
            (Some(f), _) => FormatId::from_name(f),
            (None, Some(t)) => FormatId::for_path(t),
            (None, None) => None,
        };
        // A bundle keeps the marks; a format that is not known fails when
        // the save renders, as before.
        let changes = session.doc.has_changes();
        let revisions = session.doc.has_revision_data();
        if !p.allow_lossy || format.is_none_or(|f| f == FormatId::Bundle) || !(changes || revisions)
        {
            return Ok(Vec::new());
        }
        let count = session.doc.pending().len();
        let sets = session.doc.revisions.sets.len();
        let marked = session
            .doc
            .blocks
            .iter()
            .filter(|b| b.has_revisions())
            .count();
        let copy = match &mut session.journal {
            Some(j) => {
                let path = j.path().to_path_buf();
                let jio = |e: std::io::Error| CommandError::io(&path, &e);
                let contents = j.contents().map_err(jio)?;
                Some(j.copy_aside(&contents, "marked").map_err(jio)?)
            }
            None => None,
        };
        let name = format.map_or("this format", |f| f.title());
        let mut warnings = Vec::new();
        if changes {
            let accept = Command::ChangesAccept(ChangesSettle {
                all: true,
                ..ChangesSettle::default()
            });
            session.execute_with(&accept, &Exec::by(self.writer.clone()))?;
            warnings.push(accepted_warning(name, count, copy.as_deref()));
        }
        if revisions {
            // Accepting may have marked text: the reset takes it out too.
            let clear = Command::RevisionClear(RevisionClear {
                all: true,
                sets: true,
                ..RevisionClear::default()
            });
            session.execute_with(&clear, &Exec::by(self.writer.clone()))?;
            warnings.push(cleared_warning(name, sets, marked, copy.as_deref()));
        }
        Ok(warnings)
    }

    /// Locks the journal of a "save as" target and judges what it holds
    /// exactly as opening the target would. Where the open would recover
    /// work, the save is refused; where it would copy the journal aside,
    /// so does this. Returns the locked journal, and whether it was empty.
    fn claim_target(
        &self,
        target: &Path,
        format: FormatId,
        durability: Durability,
    ) -> Result<(Journal, bool), CommandError> {
        let (j, contents) = open_journal(target, format, durability)?;
        if contents.is_empty() {
            return Ok((j, true));
        }
        let jio = |e: std::io::Error| CommandError::io(j.path(), &e);
        let (disk, missing) = match fs::read(target) {
            Ok(b) => (b, false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), true),
            Err(e) => return Err(CommandError::io(target, &e)),
        };
        let style = format.get().style(&disk);
        match self.assess(target, format, style, &contents, (&disk, missing)) {
            Assessment::Unreadable(_) => {
                j.copy_aside(&contents, "corrupt").map_err(jio)?;
            }
            Assessment::Replayed(r) if r.recover => {
                return Err(CommandError::new(
                    "unsaved_changes",
                    format!(
                        "{} has unsaved changes from a session that ended without saving; \
                         open it to recover them, or save under another name",
                        target.display()
                    ),
                )
                .with_details(json!({
                    "path": target.display().to_string(),
                    "journal": j.path().display().to_string(),
                })));
            }
            Assessment::Replayed(r) => {
                if let Some(msg) = &r.problem {
                    j.copy_aside(&contents, Replayed::aside_tag(msg))
                        .map_err(jio)?;
                }
            }
        }
        Ok((j, false))
    }

    /// Step 1 of a save in place off the UI thread (docs/specs/journal.md,
    /// "Saving off the UI thread"): takes the session's save token and a
    /// snapshot of the document, and nothing else, so it costs the UI thread
    /// about a copy of the document. The caller runs [`SaveJob::render`] on
    /// any thread, hands the outcome to [`Dispatcher::save_mark`], runs
    /// `io::atomic_write(&prepared.target, &prepared.bytes)` on any thread,
    /// and hands that outcome to [`Dispatcher::save_commit`]. "Save as" (a
    /// `path` other than the document's) uses `doc.save`.
    pub fn save_prepare(&mut self, p: &DocSave) -> Result<SaveJob, CommandError> {
        let session = self
            .session
            .as_mut()
            .ok_or_else(CommandError::no_document)?;
        if let (Some(given), Some(own)) = (&p.path, &session.path)
            && !same_path(Path::new(given), own)
        {
            return Err(CommandError::invalid_params(
                "a save off the UI thread keeps the document's path; use doc.save to save as",
            ));
        }
        let Some(path) = session.path.clone() else {
            return Err(CommandError::new(
                "no_path",
                "this document has no file yet; give a path",
            ));
        };
        refuse_while_saving(session)?;
        let notes = self.accept_for_save(p)?.into_iter().collect();
        let session = self
            .session
            .as_mut()
            .ok_or_else(CommandError::no_document)?;
        session.saves += 1;
        session.saving = Some(session.saves);
        Ok(SaveJob {
            doc: session.doc.clone(),
            style: session.style,
            path,
            params: p.clone(),
            disk: session.disk,
            damaged: session.damaged,
            rev: session.rev,
            session: session.id(),
            number: session.saves,
            notes,
        })
    }

    /// Step 3 of a save in place, back on the UI thread: appends a
    /// `doc.saved` marker for the bytes about to be written, before they
    /// are. The marker's place among the entries does not matter: replay
    /// always starts from the header, and markers only name versions of the
    /// file the journal knows. If rendering failed (a lossy format without
    /// `allow_lossy`, a file another program changed), the save ends here
    /// with that error and its token is given back. A save whose script was
    /// closed while the worker rendered it still goes ahead, as the writer
    /// asked: its journal is gone, so it gets no marker, and the commit
    /// reports `save_superseded`.
    pub fn save_mark(&mut self, rendered: RenderedSave) -> Result<PreparedSave, CommandError> {
        let RenderedSave {
            session: id,
            number,
            result,
        } = rendered;
        let session = self
            .session
            .as_mut()
            .filter(|s| s.id() == id && s.saving == Some(number));
        let Some(session) = session else {
            return result;
        };
        let mut prepared = match result {
            Ok(p) => p,
            Err(e) => {
                session.saving = None;
                return Err(e);
            }
        };
        // A broken journal protects nothing, so it gets no marker; the
        // commit starts it again, which is the only way out of that state.
        if let Some(j) = &mut session.journal
            && !j.is_broken()
        {
            if let Err(e) = j.append_saved(prepared.fingerprint) {
                session.saving = None;
                return Err(CommandError::new(
                    "journal_error",
                    format!("the crash journal could not be written, so nothing was saved: {e}"),
                ));
            }
            j.flush_now();
            prepared.journal = j.flush_waiter();
        }
        Ok(prepared)
    }

    /// Step 3 of a save in place: records how the write went, which ends
    /// the save. If nothing was edited since [`Dispatcher::save_prepare`],
    /// the journal starts again from the saved document; otherwise it keeps
    /// every edit and the document stays unsaved. A save prepared for a
    /// session that has since been closed, reopened or reverted commits
    /// nothing (`save_superseded`).
    pub fn save_commit(
        &mut self,
        prepared: PreparedSave,
        written: std::io::Result<()>,
    ) -> Result<Value, CommandError> {
        let session = self
            .session
            .as_mut()
            .filter(|s| s.id() == prepared.session && s.saving == Some(prepared.number));
        let Some(session) = session else {
            let name = prepared.target.file_name().map_or_else(
                || prepared.target.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            return Err(match written {
                Ok(()) => CommandError::new(
                    "save_superseded",
                    format!(
                        "a save of {name} started before the script was closed or reopened has finished; the file holds the version saved then"
                    ),
                ),
                Err(e) => CommandError::io(&prepared.target, &e),
            }
            .with_details(json!({ "path": prepared.target.display().to_string() })));
        };
        session.saving = None;
        written.map_err(|e| CommandError::io(&prepared.target, &e))?;
        // The file now holds these bytes, whether or not edits came since.
        session.disk = Some(prepared.fingerprint);
        session.style = saved_style(session.style);
        session.damaged = false;
        let current = session.rev == prepared.rev;
        if current {
            if let Some(j) = &mut session.journal {
                j.start(&session.doc, Some(prepared.fingerprint))
                    .map_err(|e| CommandError::io(j.path(), &e))?;
            }
            session.journal_restarted();
            session.dirty = false;
            session.recovered = None;
        } else if let Some(j) = &session.journal {
            j.flush_now();
        }
        Ok(json!({
            "path": prepared.target.display().to_string(),
            "format": prepared.format,
            "bytes": prepared.bytes.len(),
            "losses": prepared.losses,
            "dirty": session.dirty,
            "warnings": prepared
                .notes
                .into_iter()
                .chain(original_warnings(prepared.original.as_deref()))
                .collect::<Vec<_>>(),
        }))
    }

    /// Lays out the open document and writes it as a PDF. The document and
    /// its journal are untouched (docs/specs/commands.md, "Output").
    /// `diff` (docs/specs/diff.md §5): two drafts compared, and with `pdf`
    /// B with the changes marked. The open document is never changed.
    fn diff(&mut self, p: crate::command::DiffParams, by: &Author) -> Result<Value, CommandError> {
        if p.overwrite && !by.is_writer() {
            return Err(writer_only("replace an existing file"));
        }
        let b_path = PathBuf::from(&p.b);
        let (b_doc, b_format) = crate::diff::read_draft(&b_path)?;
        let read_a;
        let (a_doc, a_format, a_path) = match &p.a {
            Some(a) => {
                let path = PathBuf::from(a);
                read_a = crate::diff::read_draft(&path)?;
                (&read_a.0, read_a.1, Some(path))
            }
            None => {
                let s = self
                    .session
                    .as_ref()
                    .ok_or_else(CommandError::no_document)?;
                (&s.doc, s.format, s.path.clone())
            }
        };
        let mut out = crate::diff::result(
            &crate::diff::Named {
                doc: a_doc,
                format: a_format,
                path: a_path.as_deref(),
            },
            &crate::diff::Named {
                doc: &b_doc,
                format: b_format,
                path: Some(&b_path),
            },
        );
        let Some(pdf) = &p.pdf else {
            return Ok(out);
        };
        let target = PathBuf::from(pdf);
        let is_pdf = target
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        if !is_pdf {
            return Err(CommandError::new(
                "unsupported_format",
                format!("{} is not a .pdf path", target.display()),
            ));
        }
        let open = self.session.as_ref().and_then(|s| s.path.clone());
        let taken = [
            a_path.clone(),
            Some(b_path.clone()),
            open.clone(),
            open.as_deref().map(journal::path_for),
        ];
        if taken.iter().flatten().any(|t| same_path(t, &target)) {
            return Err(CommandError::new(
                "would_overwrite_document",
                format!(
                    "{} is one of the drafts, the open document or its crash journal; choose another path",
                    target.display()
                ),
            )
            .with_details(json!({ "path": target.display().to_string() })));
        }
        if !p.overwrite && target.exists() {
            return Err(exists(&target));
        }
        // B with the changes marked: A with B brought in as one author's
        // tracked changes, printed in the marked print form (diff.md §4.3).
        let doc = crate::diff::marked(
            a_doc,
            a_format,
            &b_doc,
            b_format,
            &crate::diff::default_name(&b_path),
        )?;
        let marked = Session::new(doc, None, a_format, SourceStyle::default());
        let (layout, _) = marked.layout_as(
            &coldopen_layout::Options::default(),
            crate::changes::PrintForm::Marked,
        );
        let fonts = coldopen_layout::Fonts::new()
            .map_err(|e| CommandError::new("internal_error", format!("bundled fonts: {e}")))?;
        let written = coldopen_layout::pdf::write(
            &layout,
            &fonts,
            &coldopen_layout::pdf::PdfOptions {
                change_slots: marked.doc.change_slots(),
                ..Default::default()
            },
        )
        .map_err(|e| CommandError::new("internal_error", e.to_string()))?;
        io::atomic_write(&target, &written.bytes).map_err(|e| CommandError::io(&target, &e))?;
        out["pdf"] = json!(target.display().to_string());
        Ok(out)
    }

    /// `changes.import` (docs/specs/diff.md §6): the draft at `path` brought
    /// into the open document as one writer's tracked edits, run as one
    /// labelled `batch`, so the journal holds the edits and replay never
    /// reads the draft.
    fn changes_import(
        &mut self,
        p: crate::command::ChangesImport,
        by: &Author,
    ) -> Result<Value, CommandError> {
        if !by.is_writer() {
            return Err(writer_only("bring a draft in as changes"));
        }
        let session = self
            .session
            .as_mut()
            .ok_or_else(CommandError::no_document)?;
        if session.doc.has_changes() {
            return Err(CommandError::new(
                "change_pending",
                "this script has tracked changes waiting; accept or reject them before bringing in a draft",
            ));
        }
        let path = PathBuf::from(&p.path);
        let (b_doc, b_format) = crate::diff::read_draft(&path)?;
        let name = p
            .name
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| crate::diff::default_name(&path));
        let (items, skipped) =
            crate::diff::import_edits(&session.doc, session.format, &b_doc, b_format);
        if items.is_empty() {
            return Ok(json!({ "change": null, "edits": 0, "skipped": skipped }));
        }
        let edits = items.len();
        let batch = Command::Batch(crate::command::Batch {
            commands: items,
            label: Some(format!("Changes from {name}")),
        });
        // A batch's tracked edits are one change (docs/specs/changes.md).
        let r = session.execute_with(&batch, &Exec::by(Author::named_writer(name)))?;
        let change = r["results"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|x| x.get("change"))
            .cloned()
            .unwrap_or(Value::Null);
        Ok(json!({ "change": change, "edits": edits, "skipped": skipped }))
    }

    fn export_pdf(&mut self, p: ExportPdf, by: &Author) -> Result<Value, CommandError> {
        if p.overwrite && !by.is_writer() {
            return Err(writer_only("replace an existing file"));
        }
        let session = self
            .session
            .as_ref()
            .ok_or_else(CommandError::no_document)?;
        let target = PathBuf::from(&p.path);
        let is_pdf = target
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        if !is_pdf {
            return Err(CommandError::new(
                "unsupported_format",
                format!("{} is not a .pdf path", target.display()),
            ));
        }
        if let Some(doc) = &session.path
            && (same_path(doc, &target) || same_path(&journal::path_for(doc), &target))
        {
            return Err(CommandError::new(
                "would_overwrite_document",
                format!(
                    "{} is the open document or its crash journal; choose another path",
                    target.display()
                ),
            )
            .with_details(json!({ "path": target.display().to_string() })));
        }
        if !p.overwrite && target.exists() {
            return Err(exists(&target));
        }
        let options = coldopen_layout::Options {
            scene_numbers: p.scene_numbers,
            no_title_page: !p.title_page,
            ..Default::default()
        };
        let (layout, map) = session.layout_as(&options, p.changes);
        let fonts = coldopen_layout::Fonts::new()
            .map_err(|e| CommandError::new("internal_error", format!("bundled fonts: {e}")))?;
        let title = session.doc.title_page.get("title").map(|f| {
            f.values
                .iter()
                .map(|v| v.plain_text())
                .collect::<Vec<_>>()
                .join(" ")
        });
        let out = coldopen_layout::pdf::write(
            &layout,
            &fonts,
            &coldopen_layout::pdf::PdfOptions {
                watermark: p.watermark.clone(),
                title,
                change_slots: session.doc.change_slots(),
                revision_fills: match p.revision_pages {
                    crate::command::RevisionPages::White => Default::default(),
                    crate::command::RevisionPages::Colored => session
                        .doc
                        .revisions
                        .sets
                        .iter()
                        .map(|s| (s.id.0, crate::revisions::shades(&s.color).paper))
                        .collect(),
                },
            },
        )
        .map_err(|e| CommandError::new("internal_error", e.to_string()))?;
        io::atomic_write(&target, &out.bytes).map_err(|e| CommandError::io(&target, &e))?;

        let mut warnings: Vec<Value> = Vec::new();
        for w in &out.warnings {
            let message = match w.code {
                "missing_glyph" => {
                    "the bundled font has no glyph for these characters; they print as a box"
                }
                _ => "the bundled font has no glyph for these accents; they are left out",
            };
            warnings.push(json!({ "code": w.code, "message": message, "chars": w.chars }));
        }
        for d in &layout.diagnostics {
            let id = d
                .element
                .and_then(|e| map.get(e))
                .and_then(|&b| session.doc.blocks.get(b))
                .map(|b| b.id);
            warnings.push(json!({ "code": d.code, "message": d.message, "id": id }));
        }
        Ok(json!({
            "path": target.display().to_string(),
            "pages": out.pages,
            "bytes": out.bytes.len(),
            "warnings": warnings,
        }))
    }

    /// `export.fdx` and `export.fountain`: a copy of the document in that
    /// format; the session is not changed (docs/specs/commands.md).
    fn export_file(
        &mut self,
        format: FormatId,
        p: ExportFile,
        by: &Author,
    ) -> Result<Value, CommandError> {
        if p.overwrite && !by.is_writer() {
            return Err(writer_only("replace an existing file"));
        }
        if p.allow_lossy && !by.is_writer() {
            return Err(writer_only("write with losses (allow_lossy)"));
        }
        let session = self
            .session
            .as_ref()
            .ok_or_else(CommandError::no_document)?;
        let target = PathBuf::from(&p.path);
        if FormatId::for_path(&target) != Some(format) {
            return Err(CommandError::new(
                "unsupported_format",
                format!(
                    "{} is not a .{} path",
                    target.display(),
                    format.get().extensions().first().copied().unwrap_or("")
                ),
            ));
        }
        refuse_document_or_journal(session.path.as_deref(), &target)?;
        if !p.overwrite && target.exists() {
            return Err(exists(&target));
        }
        let written = format.get().write(&session.doc, SourceStyle::default());
        let losses = session.losses_json(&written.losses);
        if blocking(&losses) && !p.allow_lossy {
            return Err(CommandError::new(
                "lossy_format",
                format!(
                    "{} cannot hold everything in this document; nothing was written",
                    format.title()
                ),
            )
            .with_details(json!({ "losses": losses })));
        }
        io::atomic_write(&target, &written.bytes).map_err(|e| CommandError::io(&target, &e))?;
        Ok(json!({
            "path": target.display().to_string(),
            "format": format.name(),
            "bytes": written.bytes.len(),
            "losses": losses,
        }))
    }

    /// `file.repair`: a repaired copy of an .fdx file; the input is never
    /// written (docs/specs/fdx.md §17).
    fn file_repair(&mut self, p: FileRepair, by: &Author) -> Result<Value, CommandError> {
        if p.overwrite && !by.is_writer() {
            return Err(writer_only("replace an existing file"));
        }
        let input = PathBuf::from(&p.path);
        let out = PathBuf::from(&p.out);
        if FormatId::for_path(&out) != Some(FormatId::Fdx) {
            return Err(CommandError::new(
                "unsupported_format",
                format!("{} is not an .fdx path", out.display()),
            ));
        }
        if same_path(&input, &out) || canonical(&input) == canonical(&out) {
            return Err(CommandError::new(
                "would_overwrite_input",
                format!(
                    "{} is the file being repaired; write the repaired copy somewhere else",
                    out.display()
                ),
            )
            .with_details(json!({ "path": out.display().to_string() })));
        }
        let open = self.session.as_ref().and_then(|s| s.path.as_deref());
        refuse_document_or_journal(open, &out)?;
        if !p.overwrite && out.exists() {
            return Err(exists(&out));
        }
        let bytes = fs::read(&input).map_err(|e| CommandError::io(&input, &e))?;
        let doc = coldopen_fdx::read(&bytes).map_err(|e| {
            CommandError::new(
                "unsupported_format",
                format!("{} cannot be repaired: {e}", input.display()),
            )
        })?;
        let text = doc.write_back().text;
        let out_bytes = coldopen_fdx::encode(
            &text,
            coldopen_fdx::TextEncoding {
                // What is not UTF-8 or UTF-16 is written as UTF-8.
                encoding: match doc.encoding.encoding {
                    e @ (coldopen_fdx::Encoding::Utf16Le | coldopen_fdx::Encoding::Utf16Be) => e,
                    _ => coldopen_fdx::Encoding::Utf8,
                },
                bom: doc.encoding.bom,
            },
        );
        io::atomic_write(&out, &out_bytes).map_err(|e| CommandError::io(&out, &e))?;
        Ok(json!({
            "out": out.display().to_string(),
            "bytes": out_bytes.len(),
            "repaired": doc.repaired(),
            "issues": doc.report,
        }))
    }

    /// `recovered.list` (docs/specs/commands.md).
    fn recovered_list(&mut self, p: RecoveredList) -> Result<Value, CommandError> {
        let script = match p.path {
            Some(path) => PathBuf::from(path),
            None => self
                .session
                .as_ref()
                .ok_or_else(CommandError::no_document)?
                .path
                .clone()
                .ok_or_else(|| {
                    CommandError::new("no_path", "this document has no file, so no copies")
                })?,
        };
        let copies: Vec<Value> = crate::recovered::of_script(&script)
            .iter()
            .map(|c| {
                let mut row = json!({
                    "copy": c.copy.display().to_string(),
                    "script": c.script.display().to_string(),
                    "kind": c.kind,
                    "at": c.at,
                    "unread": false,
                    "readable": crate::recovered::readable(c),
                });
                // How much of a journal copy a restore brings back
                // (docs/specs/commands.md, `recovered.list`, 2026-10-28).
                if c.kind != crate::recovered::Kind::Original
                    && let Ok(r) = crate::recovered::read_counted(c)
                {
                    row["entries"] = json!(r.entries);
                    row["replayed"] = json!(r.replayed);
                    if let Some(p) = r.problem {
                        row["problem"] = json!(p);
                    }
                }
                row
            })
            .collect();
        Ok(json!({ "copies": copies }))
    }

    /// `recovered.restore`: a copy's document as a new, unsaved script
    /// journaled at `path`; nothing is written over anything
    /// (docs/specs/journal.md, "Recovered versions").
    fn recovered_restore(
        &mut self,
        p: RecoveredRestore,
        by: &Author,
    ) -> Result<Value, CommandError> {
        if !by.is_writer() {
            return Err(writer_only("restore a recovered version"));
        }
        self.refuse_if_dirty()?;
        let copy = recovered_copy(&p.copy)?;
        let path = PathBuf::from(&p.path);
        if path.exists() {
            return Err(CommandError::new(
                "already_exists",
                format!("{} already exists; restore to a new path", path.display()),
            ));
        }
        let format = FormatId::for_path(&path).ok_or_else(|| unsupported(&path))?;
        let read = crate::recovered::read_counted(&copy).map_err(|why| {
            CommandError::new(
                "copy_unreadable",
                format!("{} cannot be read: {why}", copy.copy.display()),
            )
            .with_details(json!({ "copy": copy.copy.display().to_string() }))
        })?;
        // A copy that replays only in part restores with less work than it
        // holds, which the writer must be told (2026-10-28).
        let mut warnings = Vec::new();
        if let Some(why) = &read.problem {
            warnings.push(json!({
                "code": "journal_partial",
                "message": format!(
                    "only {} of the copy's {} changes could be brought back; {why}; the copy is kept as it is",
                    read.replayed, read.entries
                ),
                "details": {
                    "copy": copy.copy.display().to_string(),
                    "replayed": read.replayed,
                    "entries": read.entries,
                },
            }));
        }
        let doc = read.doc;
        let mut journal = None;
        if self.opts.journal {
            let (mut j, contents) = open_journal(&path, format, self.durability())?;
            if !contents.is_empty() {
                return Err(CommandError::new(
                    "already_exists",
                    format!(
                        "{} has a crash journal; restore to another path",
                        path.display()
                    ),
                )
                .with_details(json!({ "journal": j.path().display().to_string() })));
            }
            j.set_unsaved(true);
            journal = Some(j);
        }
        self.shutdown();
        let mut session = self.new_session(doc, Some(path.clone()), format, SourceStyle::default());
        session.dirty = true;
        if let Some(mut j) = journal {
            j.start(&session.doc, None)
                .map_err(|e| CommandError::io(j.path(), &e))?;
            session.journal = Some(j);
        }
        let result = json!({
            "path": path.display().to_string(),
            "format": format.name(),
            "blocks": session.doc.blocks.len(),
            "scenes": session.doc.scenes().len(),
            "warnings": warnings,
        });
        self.session = Some(session);
        Ok(result)
    }

    /// `recovered.delete`: one set-aside copy, and nothing else.
    fn recovered_delete(&mut self, p: RecoveredDelete, by: &Author) -> Result<Value, CommandError> {
        if !by.is_writer() {
            return Err(writer_only("delete a recovered version"));
        }
        let copy = recovered_copy(&p.copy)?;
        fs::remove_file(&copy.copy).map_err(|e| CommandError::io(&copy.copy, &e))?;
        Ok(json!({}))
    }

    fn doc_close(&mut self, p: DocClose, by: &Author) -> Result<Value, CommandError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(CommandError::no_document)?;
        if session.dirty && !p.discard {
            return Err(CommandError::new(
                "unsaved_changes",
                "the document has unsaved changes; save it or close with discard",
            ));
        }
        if session.dirty && holds_writers_work(session, by) {
            return Err(writers_work("discard"));
        }
        let jpath = session.journal.as_ref().map(|j| j.path().to_path_buf());
        // Nothing is deleted unless its copy was made.
        let kept = self
            .keep_copy()
            .map_err(|e| CommandError::io(jpath.as_deref().unwrap_or(Path::new("journal")), &e))?;
        if let Some(s) = self.session.take()
            && let Some(j) = s.journal
        {
            let _ = j.delete();
        }
        let warnings: Vec<Value> = kept
            .iter()
            .map(|k| {
                set_aside(
                    "journal_discarded",
                    format!("unsaved changes were set aside in {}", k.display()),
                    k,
                )
            })
            .collect();
        Ok(json!({ "warnings": warnings }))
    }

    /// Reads the file again, dropping unsaved changes and undo history. The
    /// journal is copied aside first when it holds entries.
    fn doc_revert(&mut self, by: &Author) -> Result<Value, CommandError> {
        let clock = self.clock.clone();
        let session = self
            .session
            .as_mut()
            .ok_or_else(CommandError::no_document)?;
        if session.dirty && holds_writers_work(session, by) {
            return Err(writers_work("revert"));
        }
        let Some(path) = session.path.clone() else {
            return Err(CommandError::new(
                "no_path",
                "this document has never been saved, so there is no file to revert to",
            ));
        };
        let bytes = fs::read(&path).map_err(|e| CommandError::io(&path, &e))?;
        let format = session.format;
        let ReadOut {
            doc,
            style,
            mut warnings,
            repaired,
        } = format.get().read(&bytes)?;
        // Ids are never reused within an open document, and agents may still
        // hold old ones: the file's blocks get fresh ids, so an old id fails
        // rather than naming another block (review finding F12).
        let doc = doc.renumbered_from(session.doc.next_id).ok_or_else(|| {
            CommandError::new(
                "internal_error",
                "this document has used every block id; close it and open it again",
            )
        })?;
        // A recovered header alone holds unsaved work with no entries, so
        // being dirty is enough to keep a copy (docs/specs/journal.md).
        let dirty = session.dirty;
        if let Some(j) = &mut session.journal {
            let mut restart = || -> std::io::Result<()> {
                if dirty || j.entries() > 0 {
                    let contents = j.contents()?;
                    let aside = j.copy_aside(&contents, "discarded")?;
                    warnings.push(set_aside(
                        "journal_discarded",
                        format!("unsaved changes were set aside in {}", aside.display()),
                        &aside,
                    ));
                }
                j.start(&doc, Some(Fingerprint::of(&bytes)))
            };
            restart().map_err(|e| CommandError::io(j.path(), &e))?;
        }
        let mut fresh = Session::new(doc, Some(path.clone()), format, style);
        fresh.damaged = repaired.is_some();
        fresh.clock = clock;
        // `coldopen mcp --untracked` holds for the session, revert or not.
        fresh.agent_tracking = session.agent_tracking;
        fresh.journal = session.journal.take();
        fresh.rev = session.rev + 1;
        fresh.disk = Some(Fingerprint::of(&bytes));
        let result = json!({
            "path": path.display().to_string(),
            "blocks": fresh.doc.blocks.len(),
            "scenes": fresh.doc.scenes().len(),
            "warnings": warnings,
        });
        *session = fresh;
        Ok(result)
    }

    /// Handles one request envelope and returns the response envelope.
    pub fn handle(&mut self, request: &Value) -> Value {
        let (id, cmd) = Dispatcher::read_request(request);
        let outcome = cmd.and_then(|cmd| self.execute(cmd));
        Dispatcher::response(id, outcome)
    }

    /// Reads a request envelope: the `id` to echo, and the command or the
    /// error to answer with. Front ends that check a command before running
    /// it, such as the app's agent link, use this and [`Dispatcher::response`].
    pub fn read_request(request: &Value) -> (Value, Result<Command, CommandError>) {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        if !matches!(id, Value::Null | Value::Number(_) | Value::String(_)) {
            return (
                Value::Null,
                Err(CommandError::new(
                    "invalid_request",
                    "\"id\" must be a number, string or null",
                )),
            );
        }
        let cmd = Dispatcher::parse_request(request);
        (id, cmd)
    }

    /// The response envelope for a request's id and outcome.
    pub fn response(id: Value, outcome: Result<Value, CommandError>) -> Value {
        match outcome {
            Ok(result) => json!({ "v": 1, "id": id, "ok": true, "result": result }),
            Err(e) => failure(id, e),
        }
    }

    fn parse_request(request: &Value) -> Result<Command, CommandError> {
        let Some(obj) = request.as_object() else {
            return Err(CommandError::new(
                "invalid_request",
                "a request must be a JSON object",
            ));
        };
        if let Some(k) = obj
            .keys()
            .find(|k| !matches!(k.as_str(), "v" | "id" | "command" | "params"))
        {
            return Err(CommandError::new(
                "invalid_request",
                format!("unknown request field \"{k}\""),
            ));
        }
        match obj.get("v") {
            None => {}
            Some(v) if v.as_u64() == Some(1) => {}
            Some(v) => {
                return Err(CommandError::new(
                    "unsupported_version",
                    format!("envelope version {v} is not supported; use 1"),
                ));
            }
        }
        let Some(name) = obj.get("command").and_then(Value::as_str) else {
            return Err(CommandError::new(
                "invalid_request",
                "\"command\" must be a string",
            ));
        };
        if !Command::NAMES.contains(&name) {
            return Err(CommandError::new(
                "unknown_command",
                format!("there is no command \"{name}\""),
            )
            .with_details(json!({ "commands": Command::NAMES.as_slice() })));
        }
        let params = match obj.get("params") {
            None | Some(Value::Null) => json!({}),
            Some(p @ Value::Object(_)) => p.clone(),
            Some(_) => {
                return Err(CommandError::invalid_params("\"params\" must be an object"));
            }
        };
        Command::from_parts(name, params).map_err(CommandError::invalid_params)
    }

    /// Handles one line of NDJSON and returns one line of NDJSON.
    pub fn handle_line(&mut self, line: &str) -> String {
        let response = match serde_json::from_str::<Value>(line) {
            Ok(v) => self.handle(&v),
            Err(e) => failure(
                Value::Null,
                CommandError::new("invalid_request", format!("not valid JSON: {e}")),
            ),
        };
        to_line(&response)
    }
}

#[derive(serde::Serialize)]
struct Line<'a> {
    v: &'a Value,
    id: &'a Value,
    ok: &'a Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<&'a Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a Value>,
}

/// One response as a line of JSON, with `v`, `id` and `ok` first so it
/// reads like the spec.
pub fn to_line(response: &Value) -> String {
    let line = Line {
        v: &response["v"],
        id: &response["id"],
        ok: &response["ok"],
        result: response.get("result"),
        error: response.get("error"),
    };
    serde_json::to_string(&line).unwrap_or_else(|_| response.to_string())
}

fn failure(id: Value, e: CommandError) -> Value {
    json!({ "v": 1, "id": id, "ok": false, "error": e })
}

/// A path that names a set-aside copy, which must be there.
fn recovered_copy(path: &str) -> Result<crate::recovered::Copy, CommandError> {
    let p = PathBuf::from(path);
    let copy = crate::recovered::parse(&p).ok_or_else(|| {
        CommandError::invalid_params(format!(
            "{path} is not a recovered version (a copy recovered.list names)"
        ))
    })?;
    if !p.is_file() {
        return Err(CommandError::io(
            &p,
            &std::io::Error::from(std::io::ErrorKind::NotFound),
        ));
    }
    Ok(copy)
}

/// The document a journal copy holds, replayed as recovery replays it,
/// and the format its header names; nothing on disk is touched.
pub(crate) fn replay_copy(
    script: &Path,
    contents: &[u8],
) -> Result<crate::recovered::Read, String> {
    let read = journal::read(contents)?;
    let format = read.header.format;
    // Commands, not markers: what `recovered.commands` counts on open.
    let entries = read
        .entries
        .iter()
        .filter(|e| e.saved_disk().is_none() && e.adopted().is_none() && e.semantics().is_none())
        .count();
    match Dispatcher::default().assess(
        script,
        format,
        SourceStyle::default(),
        contents,
        (&[], true),
    ) {
        Assessment::Unreadable(why) => Err(why),
        Assessment::Replayed(r) => Ok(crate::recovered::Read {
            doc: r.session.doc,
            format,
            entries,
            replayed: r.commands,
            problem: r.problem,
        }),
    }
}

/// An output may never be written over the open document or its journal.
fn refuse_document_or_journal(open: Option<&Path>, target: &Path) -> Result<(), CommandError> {
    if let Some(doc) = open
        && (same_path(doc, target) || same_path(&journal::path_for(doc), target))
    {
        return Err(CommandError::new(
            "would_overwrite_document",
            format!(
                "{} is the open document or its crash journal; choose another path",
                target.display()
            ),
        )
        .with_details(json!({ "path": target.display().to_string() })));
    }
    Ok(())
}

/// Keeps a copy of a repaired file as it is, before the first save over it
/// (docs/specs/journal.md, "Damaged originals"). A file that has gone has
/// nothing to keep.
/// Writes a document out for saving and checks it can be saved, without
/// touching the disk: to `p.path`, or to `own`, the document's own path.
fn render_doc(
    doc: &Document,
    style: SourceStyle,
    own: Option<&Path>,
    p: &DocSave,
) -> Result<Rendered, CommandError> {
    let target = match (&p.path, own) {
        (Some(p), _) => PathBuf::from(p),
        (None, Some(p)) => p.to_path_buf(),
        (None, None) => {
            return Err(CommandError::new(
                "no_path",
                "this document has no file yet; give a path",
            ));
        }
    };
    let format = match &p.format {
        Some(f) => FormatId::from_name(f).ok_or_else(|| {
            let f = f.to_ascii_lowercase();
            CommandError::new(
                "unsupported_format",
                format!("\"{f}\" is not a supported format in this version; use \"fountain\""),
            )
        })?,
        None => FormatId::for_path(&target).ok_or_else(|| unsupported(&target))?,
    };
    // A file whose extension names another format would open as that
    // format and fail, so the two must agree.
    if let Some(named) = FormatId::for_path(&target)
        && named != format
    {
        return Err(CommandError::new(
            "unsupported_format",
            format!(
                "{} is a {} path; it cannot be saved as {}",
                target.display(),
                named.name(),
                format.name()
            ),
        ));
    }

    let written = format.get().write(doc, style);
    let mut losses = crate::session::losses_json(doc, &written.losses);
    // The file is written as UTF-8, which other programs show wrongly
    // if they still read it as Windows-1252 (review finding F6).
    if style.encoding == io::Encoding::Windows1252 {
        losses.push(json!({
            "id": null,
            "code": "reencoded",
            "class": "lost",
            "message": "the file was not UTF-8 and was read as Windows-1252; it will be saved as UTF-8",
        }));
    }
    if blocking(&losses) && !p.allow_lossy {
        return Err(CommandError::new(
            "lossy_format",
            format!(
                "{} cannot hold everything in this document; nothing was saved",
                format.title()
            ),
        )
        .with_details(json!({ "losses": losses })));
    }
    Ok(Rendered {
        target,
        format,
        bytes: written.bytes,
        losses,
    })
}

fn keep_original(target: &Path) -> Result<Option<PathBuf>, CommandError> {
    let bytes = match fs::read(target) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CommandError::io(target, &e)),
    };
    let copy = journal::original_path(target);
    io::write_new(&copy, &bytes).map_err(|e| CommandError::io(&copy, &e))?;
    Ok(Some(copy))
}

fn original_warnings(original: Option<&Path>) -> Vec<Value> {
    original
        .map(|copy| {
            json!({
                "code": "original_kept",
                "message": format!(
                    "the file was repaired when it was opened; a copy of it as it was is kept in {}",
                    copy.display()
                ),
                "details": { "copy": copy.display().to_string() },
            })
        })
        .into_iter()
        .collect()
}

/// A save in place refuses when the file is no longer what this session
/// last read or wrote, unless the writer chose to overwrite it
/// (docs/specs/journal.md, "Saving"). A file that has gone is simply put
/// back.
fn refuse_if_changed(
    session: &Session,
    target: &Path,
    overwrite: bool,
) -> Result<(), CommandError> {
    refuse_if_changed_from(session.disk, target, overwrite)
}

/// [`refuse_if_changed`] for a save whose session last read or wrote the
/// bytes `disk` names.
fn refuse_if_changed_from(
    disk: Option<Fingerprint>,
    target: &Path,
    overwrite: bool,
) -> Result<(), CommandError> {
    if overwrite {
        return Ok(());
    }
    let now = match fs::read(target) {
        Ok(b) => Fingerprint::of(&b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(CommandError::io(target, &e)),
    };
    if disk == Some(now) {
        return Ok(());
    }
    let name = target.file_name().map_or_else(
        || target.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    Err(CommandError::new(
        "file_changed",
        format!(
            "{name} was changed by another program since Cold Open last read or saved it; saving now would replace those changes. Reload it, save it under another name, or save with overwrite"
        ),
    )
    .with_details(json!({ "path": target.display().to_string() })))
}

fn foreign_journal(path: &Path, details: Value) -> CommandError {
    let host = details["host"].as_str().unwrap_or("an unknown computer");
    let at = details["path"].as_str().unwrap_or("an unknown path");
    CommandError::new(
        "journal_foreign",
        format!(
            "{} has unsaved changes in a crash journal written elsewhere (on {host}, for {at}), perhaps synced or copied here with the script; open it with recover \"apply\" to recover them, or \"discard\" to set them aside",
            path.display()
        ),
    )
    .with_details(details)
}

/// The style a file has once saved: a Windows-1252 file becomes UTF-8.
fn saved_style(style: SourceStyle) -> SourceStyle {
    match style.encoding {
        io::Encoding::Windows1252 => SourceStyle {
            encoding: io::Encoding::Utf8,
            ..style
        },
        _ => style,
    }
}

/// One save at a time: while a save prepared off the UI thread is being
/// written, every other save fails, so an older write can never land after
/// a newer one (docs/specs/journal.md, "Saving off the UI thread").
fn refuse_while_saving(session: &Session) -> Result<(), CommandError> {
    if session.saving.is_some() {
        return Err(CommandError::new(
            "save_in_progress",
            "the script is being saved; try again when that save has finished",
        ));
    }
    Ok(())
}

/// Whether a replayed result reproduces what later entries depend on
/// (docs/specs/journal.md, "What replay compares"): the ids a command
/// assigned, and which step an undo or redo reversed. Nothing else is
/// compared, because newer versions add result fields and labels are
/// reworded or translated; a field the recorded result lacks is never
/// asked for.
fn reproduces(recorded: &Value, replayed: &Value) -> bool {
    let same = |r: &Value, q: &Value, key: &str| r.get(key).is_none_or(|v| q.get(key) == Some(v));
    if !["id", "undone", "redone"]
        .iter()
        .all(|k| same(recorded, replayed, k))
    {
        return false;
    }
    match recorded.get("results").and_then(Value::as_array) {
        None => true,
        Some(rs) => replayed
            .get("results")
            .and_then(Value::as_array)
            .is_some_and(|qs| {
                rs.len() == qs.len() && rs.iter().zip(qs).all(|(r, q)| same(r, q, "id"))
            }),
    }
}

/// What recovery makes of a journal, for the journal fuzz target.
#[doc(hidden)]
#[derive(Debug, PartialEq)]
pub struct RecoveryCheck {
    /// The replayed document, or `None` when the header cannot be used.
    pub doc: Option<Document>,
    /// Where the trusted entries end.
    pub valid_len: u64,
    pub problem: Option<String>,
    pub recover: bool,
}

/// Runs recovery's own judgement (the one `doc.open` acts on) over journal
/// bytes, for a script whose file holds `disk`, or has none, without
/// touching the disk (review finding F17.2: the fuzz target used to run a
/// copy of replay).
#[doc(hidden)]
pub fn recovery_check(contents: &[u8], disk: Option<&[u8]>) -> RecoveryCheck {
    let bytes = disk.unwrap_or_default();
    let format = FormatId::Fountain;
    let style = format.get().style(bytes);
    let path = Path::new("fuzz.fountain");
    match Dispatcher::default().assess(path, format, style, contents, (bytes, disk.is_none())) {
        Assessment::Unreadable(problem) => RecoveryCheck {
            doc: None,
            valid_len: 0,
            problem: Some(problem),
            recover: false,
        },
        Assessment::Replayed(r) => RecoveryCheck {
            doc: Some(r.session.doc),
            valid_len: r.valid_len,
            problem: r.problem,
            recover: r.recover,
        },
    }
}

/// Whether opening `path` would recover work from its crash journal,
/// judged without locking or changing anything, for the app's start-up
/// offer of leftover work. A journal that cannot be read, for example one
/// another process holds on Windows, counts as holding nothing.
pub fn journal_holds_work(path: &Path) -> bool {
    let Ok(contents) = fs::read(journal::path_for(path)) else {
        return false;
    };
    if contents.is_empty() {
        return false;
    }
    let (disk, missing) = match fs::read(path) {
        Ok(b) => (b, false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), true),
        Err(_) => return false,
    };
    let format = FormatId::for_path(path).unwrap_or_default();
    let style = format.get().style(&disk);
    match Dispatcher::default().assess(path, format, style, &contents, (&disk, missing)) {
        Assessment::Unreadable(_) => false,
        Assessment::Replayed(r) => r.recover,
    }
}

/// Whether `by` is an agent and the session holds a writer's recovered,
/// unsaved work, which an agent may not set aside (docs/specs/mcp.md §7).
fn holds_writers_work(session: &Session, by: &Author) -> bool {
    !by.is_writer() && session.recovered.is_some_and(|r| r.writer)
}

/// A save or export to a path where some other file already is
/// (docs/specs/commands.md): it is replaced only with `overwrite`.
fn exists(target: &Path) -> CommandError {
    CommandError::new(
        "already_exists",
        format!(
            "{} already exists; choose another path, or pass overwrite to replace it",
            target.display()
        ),
    )
    .with_details(json!({ "path": target.display().to_string() }))
}

/// Some choices are the writer's to make, never an agent's: an agent would
/// simply pass the flag.
/// The warning a save that accepted pending changes gives.
fn accepted_warning(name: &str, count: usize, copy: Option<&Path>) -> Value {
    let changes = if count == 1 {
        "1 pending change".to_string()
    } else {
        format!("{count} pending changes")
    };
    match copy {
        Some(copy) => json!({
            "code": "changes_accepted",
            "message": format!(
                "{name} cannot hold tracked changes, so saving accepted {changes}; the version with them marked is kept in {}",
                copy.display()
            ),
            "details": { "copy": copy.display().to_string(), "count": count },
        }),
        None => json!({
            "code": "changes_accepted",
            "message": format!("{name} cannot hold tracked changes, so saving accepted {changes}"),
            "details": { "count": count },
        }),
    }
}

/// The warning a save that took out revision sets and marks gives
/// (docs/specs/revisions.md §9).
fn cleared_warning(name: &str, sets: usize, blocks: usize, copy: Option<&Path>) -> Value {
    let what = format!(
        "{sets} revision set{} and the marks on {blocks} block{}",
        if sets == 1 { "" } else { "s" },
        if blocks == 1 { "" } else { "s" },
    );
    match copy {
        Some(copy) => json!({
            "code": "revisions_cleared",
            "message": format!(
                "{name} cannot hold revision marks, so saving took out {what}; the version with them is kept in {}",
                copy.display()
            ),
            "details": { "copy": copy.display().to_string(), "sets": sets, "blocks": blocks },
        }),
        None => json!({
            "code": "revisions_cleared",
            "message": format!("{name} cannot hold revision marks, so saving took out {what}"),
            "details": { "sets": sets, "blocks": blocks },
        }),
    }
}

fn writer_only(what: &str) -> CommandError {
    CommandError::new(
        "writer_only",
        format!("only the writer can choose to {what}; an agent may not"),
    )
}

fn writers_work(what: &str) -> CommandError {
    CommandError::new(
        "not_your_step",
        format!(
            "the document holds the writer's unsaved work, recovered after a crash; an agent may not {what} it. Save it, or leave it for the writer"
        ),
    )
    .with_details(json!({ "recovered": true }))
}

/// Whether losses include one that refuses a save without `allow_lossy`:
/// any but a `normalized` one (docs/specs/commands.md, "Losses").
fn blocking(losses: &[Value]) -> bool {
    losses.iter().any(|l| l["class"] != "normalized")
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(d: &mut Dispatcher, command: &str, params: Value) -> Result<Value, CommandError> {
        d.execute(Command::from_parts(command, params).unwrap())
    }

    fn faults(d: &mut Dispatcher) -> &mut crate::journal::Faults {
        &mut d.session.as_mut().unwrap().journal.as_mut().unwrap().faults
    }

    /// Review findings F8 and C6: replay compared results byte for byte, so
    /// a field added later (phase 3's undo `label`) stopped every older
    /// journal at its first undo. Only ids and undo targets count now.
    #[test]
    fn replay_compares_only_ids_and_undo_targets() {
        let r = |v: Value, w: Value| reproduces(&v, &w);
        assert!(r(
            json!({"undone": "block.insert"}),
            json!({"undone": "block.insert", "label": "Insert block"})
        ));
        assert!(r(
            json!({"undone": "typing", "label": "Typing"}),
            json!({"undone": "typing", "label": "Saisie"})
        ));
        assert!(!r(json!({"undone": "typing"}), json!({"undone": "batch"})));
        assert!(r(
            json!({"id": "b_3"}),
            json!({"id": "b_3", "new_field": 1})
        ));
        assert!(!r(json!({"id": "b_3"}), json!({"id": "b_4"})));
        assert!(r(
            json!({"count": 2, "blocks": ["b_1"]}),
            json!({"count": 3, "blocks": []})
        ));
        assert!(r(
            json!({"results": [{"id": "b_5"}, {}]}),
            json!({"results": [{"id": "b_5"}, {"x": 1}]})
        ));
        assert!(!r(
            json!({"results": [{"id": "b_5"}]}),
            json!({"results": [{"id": "b_6"}]})
        ));
        assert!(!r(json!({"results": [{}]}), json!({"results": []})));
    }

    /// Review finding F10, through the dispatcher: a journal that a failed
    /// write left broken refuses every edit, and a save, in place or off
    /// the UI thread, starts it again.
    #[test]
    fn a_broken_journal_refuses_edits_until_a_save() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.fountain");
        fs::write(&p, "INT. HOUSE - DAY\n\nRain.\n").unwrap();
        for off_the_ui_thread in [false, true] {
            let mut d = Dispatcher::new(Options::default());
            run(&mut d, "doc.open", json!({"path": p})).unwrap();
            let f = faults(&mut d);
            f.short_write = Some(10);
            f.fail_truncate = true;
            let insert = |text: &str| json!({"type": "action", "text": text});
            let e = run(&mut d, "block.insert", insert("Lost.")).unwrap_err();
            assert_eq!(e.code, "journal_error", "{e}");
            let e = run(&mut d, "block.insert", insert("Refused.")).unwrap_err();
            assert_eq!(e.code, "journal_error", "{e}");
            assert!(e.message.contains("save"), "{e}");
            if off_the_ui_thread {
                let job = d.save_prepare(&DocSave::default()).unwrap();
                let prepared = d.save_mark(job.render()).unwrap();
                let written = io::atomic_write(&prepared.target, &prepared.bytes);
                d.save_commit(prepared, written).unwrap();
            } else {
                run(&mut d, "doc.save", json!({})).unwrap();
            }
            run(&mut d, "block.insert", insert("Protected.")).unwrap();
            drop(d); // a crash

            let mut d = Dispatcher::new(Options::default());
            let r = run(&mut d, "doc.open", json!({"path": p})).unwrap();
            assert_eq!(r["recovered"]["commands"], 1, "{r}");
            assert_eq!(r["warnings"], json!([]), "{r}");
            let text = run(&mut d, "doc.export", json!({})).unwrap()["text"].clone();
            assert!(text.as_str().unwrap().ends_with("Protected.\n"), "{text}");
            assert!(!text.as_str().unwrap().contains("Lost."), "{text}");
            d.discard();
        }
    }
}
