//! The dispatcher: the one place every front end sends commands. It owns
//! the open session and handles opening, saving, closing and recovery
//! (docs/specs/commands.md, docs/specs/journal.md).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::author::{Author, Clock};
use crate::command::{Class, Command, DocClose, DocNew, DocOpen, DocSave, ExportPdf, Recover};
use crate::document::Document;
use crate::error::CommandError;
use crate::flush::{Durability, FlushStatus};
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
}

impl Replayed {
    /// What a copy of a journal that replay stopped early is called.
    fn aside_tag(msg: &str) -> &'static str {
        if msg.contains("different result") {
            "diverged"
        } else {
            "corrupt"
        }
    }
}

/// A document written out for saving, before anything touches the disk.
struct Rendered {
    target: PathBuf,
    format: String,
    bytes: Vec<u8>,
    losses: Vec<Value>,
}

fn warning(code: &str, message: impl Into<String>) -> Value {
    json!({ "code": code, "message": message.into() })
}

fn is_fountain(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "fountain" | "spmd" | "txt"))
}

fn locked(path: &Path) -> CommandError {
    CommandError::new(
        "doc_locked",
        format!("{} is open in another Cold Open process", path.display()),
    )
    .with_details(json!({ "path": path.display().to_string() }))
}

fn open_journal(path: &Path, durability: Durability) -> Result<(Journal, Vec<u8>), CommandError> {
    let jpath = journal::path_for(path);
    Journal::open_locked(&jpath, durability).map_err(|e| match e {
        OpenError::Locked => locked(path),
        OpenError::Io(e) => CommandError::io(&jpath, &e),
    })
}

impl Dispatcher {
    pub fn new(opts: Options) -> Dispatcher {
        Dispatcher {
            session: None,
            opts,
            clock: Clock::default(),
            durability: None,
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

    /// Replaces the clock used to group typing into undo steps, for tests.
    pub fn set_clock(&mut self, clock: Clock) {
        if let Some(s) = &mut self.session {
            s.clock = clock.clone();
        }
        self.clock = clock;
    }

    fn new_session(&self, doc: Document, path: Option<PathBuf>, style: SourceStyle) -> Session {
        let mut s = Session::new(doc, path, style);
        s.clock = self.clock.clone();
        s
    }

    /// Runs one command as a writer.
    pub fn execute(&mut self, cmd: Command) -> Result<Value, CommandError> {
        self.execute_as(cmd, &Author::Writer)
    }

    /// Runs one command for an author.
    pub fn execute_as(&mut self, cmd: Command, by: &Author) -> Result<Value, CommandError> {
        match cmd {
            Command::DocNew(p) => self.doc_new(p),
            Command::DocOpen(p) => self.doc_open(p, by),
            Command::DocSave(p) => self.doc_save(p, by),
            Command::DocClose(p) => self.doc_close(p, by),
            Command::DocRevert(_) => self.doc_revert(by),
            Command::ExportPdf(p) => self.export_pdf(p),
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
        if let Some(path) = &path {
            if path.exists() {
                return Err(CommandError::new(
                    "already_exists",
                    format!("{} already exists; open it instead", path.display()),
                ));
            }
            if !is_fountain(path) {
                return Err(unsupported(path));
            }
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
                let (j, contents) = open_journal(path, self.durability())?;
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
            ..Default::default()
        };
        if let Some(kind) = p.start {
            let id = doc.allocate_id();
            doc.blocks.push(crate::document::Block {
                id,
                element: coldopen_screenplay::Element::new(kind, ""),
            });
        }
        let mut warnings = Vec::new();
        if path.is_none() {
            warnings.push(warning(
                "unjournaled",
                "an untitled document is not protected by a crash journal until it is saved",
            ));
        }
        let mut session = self.new_session(doc, path, SourceStyle::default());
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
        if !is_fountain(&path) {
            return Err(unsupported(&path));
        }
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
        let decoded = io::decode(&bytes);
        let parsed = coldopen_fountain::parse(&decoded.text);
        let mut warnings: Vec<Value> = Vec::new();
        if decoded.not_utf8 {
            warnings.push(warning(
                "not_utf8",
                "the file is not UTF-8; it was read as Windows-1252 and will be saved as UTF-8",
            ));
        }
        for d in &parsed.diagnostics {
            warnings.push(json!({ "code": d.code, "message": d.message, "line": d.line }));
        }

        let disk_doc = Document::from_script(parsed.script);
        let mut session = self.new_session(disk_doc.clone(), Some(path.clone()), decoded.style);
        let mut recovered = Value::Null;

        if self.opts.journal {
            let (mut j, contents) = open_journal(&path, self.durability())?;
            let mut resumed = false;
            if !contents.is_empty() {
                match p.recover {
                    Recover::Discard => {
                        if !by.is_writer()
                            && let Assessment::Replayed(r) =
                                self.assess(&path, decoded.style, &contents, (&bytes, missing))
                            && r.recover
                            && r.writer
                        {
                            return Err(writers_work("set aside"));
                        }
                        let aside = j
                            .copy_aside(&contents, "discarded")
                            .map_err(|e| CommandError::io(j.path(), &e))?;
                        warnings.push(warning(
                            "journal_discarded",
                            format!("unsaved changes were set aside in {}", aside.display()),
                        ));
                    }
                    Recover::Auto => {
                        let jio = |e: std::io::Error, j: &Journal| CommandError::io(j.path(), &e);
                        match self.assess(&path, decoded.style, &contents, (&bytes, missing)) {
                            Assessment::Unreadable(msg) => {
                                let aside =
                                    j.copy_aside(&contents, "corrupt").map_err(|e| jio(e, &j))?;
                                warnings.push(warning(
                                    "journal_unreadable",
                                    format!("{msg}; it was kept in {}", aside.display()),
                                ));
                            }
                            Assessment::Replayed(r) => {
                                if let Some(msg) = &r.problem {
                                    let aside = j
                                        .copy_aside(&contents, Replayed::aside_tag(msg))
                                        .map_err(|e| jio(e, &j))?;
                                    warnings.push(warning(
                                        "journal_partial",
                                        format!(
                                            "recovered {} of the journal's changes; {msg}; the journal was kept in {}",
                                            r.commands,
                                            aside.display()
                                        ),
                                    ));
                                }
                                if r.recover {
                                    j.resume(r.valid_len, r.applied).map_err(|e| jio(e, &j))?;
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
        let result = json!({
            "path": path.display().to_string(),
            "format": "fountain",
            "blocks": session.doc.blocks.len(),
            "scenes": session.doc.scenes().len(),
            "warnings": warnings,
            "recovered": recovered,
        });
        self.session = Some(session);
        Ok(result)
    }

    /// Replays a crash journal and judges whether it holds work the file
    /// does not, without touching the disk. `doc.open` acts on the answer;
    /// "save as" uses it to refuse a target with unsaved work.
    fn assess(
        &self,
        path: &Path,
        style: SourceStyle,
        contents: &[u8],
        (disk_bytes, missing): (&[u8], bool),
    ) -> Assessment {
        let read = match journal::read(contents) {
            Ok(r) => r,
            Err(msg) => return Assessment::Unreadable(msg),
        };

        let mut replay = self.new_session(read.header.doc.clone(), Some(path.to_path_buf()), style);
        // Fingerprints the file may legitimately have: the header's, and
        // those of the saves the journal records.
        let mut known_disk: Vec<Fingerprint> = read.header.disk.into_iter().collect();
        let mut applied = 0usize;
        let mut commands = 0usize;
        let mut last = 0u64;
        let mut writer = false;
        let mut valid_len = read.header_len();
        let mut problem = read.problem.clone();
        for (k, entry) in read.entries.iter().enumerate() {
            if let Some(fp) = entry.saved_disk() {
                known_disk.push(fp);
                applied = k + 1;
                valid_len = read.entry_end(k);
                continue;
            }
            let ex = Exec {
                by: entry.by.clone().unwrap_or_default(),
                merge: Merge::Recorded(entry.merge),
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
                    replay
                        .execute_with(&cmd, &ex)
                        .map_err(|e| format!("entry {} failed on replay: {e}", entry.seq))
                })
                .and_then(|result| {
                    if result == entry.result {
                        Ok(())
                    } else {
                        Err(format!(
                            "entry {} gave a different result on replay",
                            entry.seq
                        ))
                    }
                });
            if let Err(msg) = outcome {
                problem = Some(msg);
                break;
            }
            applied = k + 1;
            commands += 1;
            last = entry.t;
            writer |= ex.by.is_writer();
            valid_len = read.entry_end(k);
        }

        // The file already holds the replayed document only if writing it
        // is lossless and gives the same bytes. A lossy write can match the
        // file's text while the document differs, so it always recovers.
        let in_file = |doc: &Document| {
            let written = coldopen_fountain::write(&doc.to_script());
            written.losses.is_empty() && io::encode(&written.text, style) == disk_bytes
        };
        let external_change = if missing {
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
        // writer's, which is the cautious choice.
        writer |= commands == 0;
        replay.journal = None;
        Assessment::Replayed(Box::new(Replayed {
            session: replay,
            valid_len,
            applied: applied as u64,
            commands,
            last,
            problem,
            recover,
            external_change,
            writer,
        }))
    }

    /// Writes the document out for saving and checks it can be saved, without
    /// touching the disk.
    fn render(session: &Session, p: &DocSave) -> Result<Rendered, CommandError> {
        let target = match (&p.path, &session.path) {
            (Some(p), _) => PathBuf::from(p),
            (None, Some(p)) => p.clone(),
            (None, None) => {
                return Err(CommandError::new(
                    "no_path",
                    "this document has no file yet; give a path",
                ));
            }
        };
        let format = match &p.format {
            Some(f) => f.to_ascii_lowercase(),
            None if is_fountain(&target) => "fountain".into(),
            None => return Err(unsupported(&target)),
        };
        if format != "fountain" {
            return Err(CommandError::new(
                "unsupported_format",
                format!("\"{format}\" is not a supported format in this version; use \"fountain\""),
            ));
        }

        let written = coldopen_fountain::write(&session.doc.to_script());
        let losses = session.losses_json(&written.losses);
        if !losses.is_empty() && !p.allow_lossy {
            return Err(CommandError::new(
                "lossy_format",
                "Fountain cannot hold everything in this document; nothing was saved",
            )
            .with_details(json!({ "losses": losses })));
        }
        Ok(Rendered {
            target,
            format,
            bytes: io::encode(&written.text, session.style),
            losses,
        })
    }

    fn doc_save(&mut self, p: DocSave, by: &Author) -> Result<Value, CommandError> {
        if p.overwrite && !by.is_writer() {
            return Err(writer_only(
                "replace a file that is not the open document, or that another program changed",
            ));
        }
        let opts = self.opts;
        let durability = self.durability();
        let session = self
            .session
            .as_ref()
            .ok_or_else(CommandError::no_document)?;
        refuse_while_saving(session)?;
        let Rendered {
            target,
            format,
            bytes,
            losses,
        } = Self::render(session, &p)?;
        let same_file = session
            .path
            .as_ref()
            .is_some_and(|old| same_path(old, &target));
        if same_file {
            refuse_if_changed(session, &target, p.overwrite)?;
        }
        // Save as: the target's lock is taken, and its journal judged,
        // before anything is written (docs/specs/journal.md, "Saving").
        let claimed = if opts.journal && !same_file {
            Some(self.claim_target(&target, durability)?)
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
        if same_file {
            if let Some(j) = &mut session.journal {
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
        session.path = Some(target.clone());
        session.dirty = false;
        session.recovered = None;
        session.disk = Some(fingerprint);
        Ok(json!({
            "path": target.display().to_string(),
            "format": format,
            "bytes": bytes.len(),
            "losses": losses,
        }))
    }

    /// Locks the journal of a "save as" target and judges what it holds
    /// exactly as opening the target would. Where the open would recover
    /// work, the save is refused; where it would copy the journal aside,
    /// so does this. Returns the locked journal, and whether it was empty.
    fn claim_target(
        &self,
        target: &Path,
        durability: Durability,
    ) -> Result<(Journal, bool), CommandError> {
        let (j, contents) = open_journal(target, durability)?;
        if contents.is_empty() {
            return Ok((j, true));
        }
        let jio = |e: std::io::Error| CommandError::io(j.path(), &e);
        let (disk, missing) = match fs::read(target) {
            Ok(b) => (b, false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), true),
            Err(e) => return Err(CommandError::io(target, &e)),
        };
        let style = io::decode(&disk).style;
        match self.assess(target, style, &contents, (&disk, missing)) {
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

    /// Step 1 of a save in place off the UI thread: writes the document out
    /// and appends a `doc.saved` marker for those bytes. The caller then
    /// runs `io::atomic_write(&prepared.target, &prepared.bytes)` on any
    /// thread, and hands the outcome to [`Dispatcher::save_commit`].
    /// "Save as" (a `path` other than the document's) uses `doc.save`.
    pub fn save_prepare(&mut self, p: &DocSave) -> Result<PreparedSave, CommandError> {
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
        if session.path.is_none() {
            return Err(CommandError::new(
                "no_path",
                "this document has no file yet; give a path",
            ));
        }
        refuse_while_saving(session)?;
        let r = Self::render(session, p)?;
        refuse_if_changed(session, &r.target, p.overwrite)?;
        let fingerprint = Fingerprint::of(&r.bytes);
        // A broken journal protects nothing, so it gets no marker; the
        // commit starts it again, which is the only way out of that state.
        if let Some(j) = &mut session.journal
            && !j.is_broken()
        {
            j.append_saved(fingerprint).map_err(|e| {
                CommandError::new(
                    "journal_error",
                    format!("the crash journal could not be written, so nothing was saved: {e}"),
                )
            })?;
            j.flush_now();
        }
        session.saves += 1;
        session.saving = Some(session.saves);
        Ok(PreparedSave {
            target: r.target,
            bytes: r.bytes,
            format: r.format,
            losses: r.losses,
            rev: session.rev,
            fingerprint,
            session: session.id(),
            number: session.saves,
        })
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
        let current = session.rev == prepared.rev;
        if current {
            if let Some(j) = &mut session.journal {
                j.start(&session.doc, Some(prepared.fingerprint))
                    .map_err(|e| CommandError::io(j.path(), &e))?;
            }
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
        }))
    }

    /// Lays out the open document and writes it as a PDF. The document and
    /// its journal are untouched (docs/specs/commands.md, "Output").
    fn export_pdf(&mut self, p: ExportPdf) -> Result<Value, CommandError> {
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
        let options = coldopen_layout::Options {
            scene_numbers: p.scene_numbers,
            no_title_page: !p.title_page,
            ..Default::default()
        };
        let (layout, map) = session.layout(&options);
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
                warning(
                    "journal_discarded",
                    format!("unsaved changes were set aside in {}", k.display()),
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
        let decoded = io::decode(&bytes);
        let parsed = coldopen_fountain::parse(&decoded.text);
        let mut warnings: Vec<Value> = Vec::new();
        if decoded.not_utf8 {
            warnings.push(warning(
                "not_utf8",
                "the file is not UTF-8; it was read as Windows-1252 and will be saved as UTF-8",
            ));
        }
        for d in &parsed.diagnostics {
            warnings.push(json!({ "code": d.code, "message": d.message, "line": d.line }));
        }
        let doc = Document::from_script(parsed.script);
        // A recovered header alone holds unsaved work with no entries, so
        // being dirty is enough to keep a copy (docs/specs/journal.md).
        let dirty = session.dirty;
        if let Some(j) = &mut session.journal {
            let mut restart = || -> std::io::Result<()> {
                if dirty || j.entries() > 0 {
                    let contents = j.contents()?;
                    let aside = j.copy_aside(&contents, "discarded")?;
                    warnings.push(warning(
                        "journal_discarded",
                        format!("unsaved changes were set aside in {}", aside.display()),
                    ));
                }
                j.start(&doc, Some(Fingerprint::of(&bytes)))
            };
            restart().map_err(|e| CommandError::io(j.path(), &e))?;
        }
        let mut fresh = Session::new(doc, Some(path.clone()), decoded.style);
        fresh.clock = clock;
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
            .with_details(json!({ "commands": Command::NAMES })));
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

fn unsupported(path: &Path) -> CommandError {
    CommandError::new(
        "unsupported_format",
        format!(
            "{} is not a Fountain file (.fountain, .spmd or .txt); other formats come later",
            path.display()
        ),
    )
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
    if overwrite {
        return Ok(());
    }
    let now = match fs::read(target) {
        Ok(b) => Fingerprint::of(&b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(CommandError::io(target, &e)),
    };
    if session.disk == Some(now) {
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
    let style = io::decode(&disk).style;
    match Dispatcher::default().assess(path, style, &contents, (&disk, missing)) {
        Assessment::Unreadable(_) => false,
        Assessment::Replayed(r) => r.recover,
    }
}

/// Whether `by` is an agent and the session holds a writer's recovered,
/// unsaved work, which an agent may not set aside (docs/specs/mcp.md §7).
fn holds_writers_work(session: &Session, by: &Author) -> bool {
    !by.is_writer() && session.recovered.is_some_and(|r| r.writer)
}

/// Some choices are the writer's to make, never an agent's: an agent would
/// simply pass the flag.
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
                let prepared = d.save_prepare(&DocSave::default()).unwrap();
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
