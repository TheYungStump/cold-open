//! The dispatcher: the one place every front end sends commands. It owns
//! the open session and handles opening, saving, closing and recovery
//! (docs/specs/commands.md, docs/specs/journal.md).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::command::{Command, DocClose, DocNew, DocOpen, DocSave, Recover};
use crate::document::Document;
use crate::error::CommandError;
use crate::io::{self, Fingerprint, SourceStyle};
use crate::journal::{self, Journal, OpenError};
use crate::session::Session;

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

fn open_journal(path: &Path, fsync: bool) -> Result<(Journal, Vec<u8>), CommandError> {
    let jpath = journal::path_for(path);
    Journal::open_locked(&jpath, fsync).map_err(|e| match e {
        OpenError::Locked => locked(path),
        OpenError::Io(e) => CommandError::io(&jpath, &e),
    })
}

impl Dispatcher {
    pub fn new(opts: Options) -> Dispatcher {
        Dispatcher {
            session: None,
            opts,
        }
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Runs one command.
    pub fn execute(&mut self, cmd: Command) -> Result<Value, CommandError> {
        match cmd {
            Command::DocNew(p) => self.doc_new(p),
            Command::DocOpen(p) => self.doc_open(p),
            Command::DocSave(p) => self.doc_save(p),
            Command::DocClose(p) => self.doc_close(p),
            other => self
                .session
                .as_mut()
                .ok_or_else(CommandError::no_document)?
                .execute(&other),
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

    /// Ends the session, throwing away unsaved changes and the journal.
    pub fn discard(&mut self) {
        if let Some(s) = self.session.take()
            && let Some(j) = s.journal
        {
            let _ = j.delete();
        }
    }

    /// Closes a clean session before opening another one.
    fn close_for_switch(&mut self) -> Result<(), CommandError> {
        if self.session.as_ref().is_some_and(|s| s.dirty) {
            return Err(CommandError::new(
                "unsaved_changes",
                "the open document has unsaved changes; save it or close it with discard",
            ));
        }
        self.shutdown();
        Ok(())
    }

    fn doc_new(&mut self, p: DocNew) -> Result<Value, CommandError> {
        self.close_for_switch()?;
        let doc = Document {
            next_id: 1,
            ..Default::default()
        };
        let mut warnings = Vec::new();
        let mut session = match &p.path {
            Some(path) => {
                let path = PathBuf::from(path);
                if path.exists() {
                    return Err(CommandError::new(
                        "already_exists",
                        format!("{} already exists; open it instead", path.display()),
                    ));
                }
                if !is_fountain(&path) {
                    return Err(unsupported(&path));
                }
                Session::new(doc, Some(path), SourceStyle::default())
            }
            None => {
                warnings.push(warning(
                    "unjournaled",
                    "an untitled document is not protected by a crash journal until it is saved",
                ));
                Session::new(doc, None, SourceStyle::default())
            }
        };
        if self.opts.journal
            && let Some(path) = session.path.clone()
        {
            let (mut j, _) = open_journal(&path, self.opts.fsync)?;
            j.start(&session.doc, None)
                .map_err(|e| CommandError::io(j.path(), &e))?;
            session.journal = Some(j);
        }
        let path = session.path.as_ref().map(|p| p.display().to_string());
        self.session = Some(session);
        Ok(json!({ "path": path, "warnings": warnings }))
    }

    fn doc_open(&mut self, p: DocOpen) -> Result<Value, CommandError> {
        self.close_for_switch()?;
        let path = PathBuf::from(&p.path);
        if !is_fountain(&path) {
            return Err(unsupported(&path));
        }
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

        let disk_doc = Document::from_script(parsed.script);
        let mut session = Session::new(disk_doc.clone(), Some(path.clone()), decoded.style);
        let mut recovered = Value::Null;

        if self.opts.journal {
            let (mut j, contents) = open_journal(&path, self.opts.fsync)?;
            let mut resumed = false;
            if !contents.is_empty() {
                match p.recover {
                    Recover::Discard => {
                        let aside = j
                            .copy_aside(&contents, "discarded")
                            .map_err(|e| CommandError::io(j.path(), &e))?;
                        warnings.push(warning(
                            "journal_discarded",
                            format!("unsaved changes were set aside in {}", aside.display()),
                        ));
                    }
                    Recover::Auto => {
                        resumed = self.recover(
                            &mut session,
                            &mut j,
                            &contents,
                            &bytes,
                            &mut warnings,
                            &mut recovered,
                        )?;
                    }
                }
            }
            if !resumed {
                j.start(&disk_doc, Some(Fingerprint::of(&bytes)))
                    .map_err(|e| CommandError::io(j.path(), &e))?;
            }
            session.journal = Some(j);
        }

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

    /// Replays a crash journal into `session`. Returns true when the session
    /// now holds recovered, unsaved work and the journal continues.
    #[allow(clippy::too_many_arguments)]
    fn recover(
        &self,
        session: &mut Session,
        j: &mut Journal,
        contents: &[u8],
        disk_bytes: &[u8],
        warnings: &mut Vec<Value>,
        recovered: &mut Value,
    ) -> Result<bool, CommandError> {
        let jio = |e: std::io::Error, j: &Journal| CommandError::io(j.path(), &e);
        let read = match journal::read(contents) {
            Ok(r) => r,
            Err(msg) => {
                let aside = j.copy_aside(contents, "corrupt").map_err(|e| jio(e, j))?;
                warnings.push(warning(
                    "journal_unreadable",
                    format!("{msg}; it was kept in {}", aside.display()),
                ));
                return Ok(false);
            }
        };

        let mut replay = Session::new(read.header.doc.clone(), session.path.clone(), session.style);
        let mut applied = 0usize;
        let mut valid_len = read.header_len();
        let mut problem = read.problem.clone();
        for (k, entry) in read.entries.iter().enumerate() {
            let outcome = Command::from_parts(&entry.command, entry.params.clone())
                .map_err(|e| format!("entry {} cannot be read as a command: {e}", entry.seq))
                .and_then(|cmd| {
                    replay
                        .execute(&cmd)
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
            valid_len = read.entry_end(k);
        }
        if let Some(msg) = &problem {
            let tag = if msg.contains("different result") {
                "diverged"
            } else {
                "corrupt"
            };
            let aside = j.copy_aside(contents, tag).map_err(|e| jio(e, j))?;
            warnings.push(warning(
                "journal_partial",
                format!(
                    "recovered {applied} of the journal's changes; {msg}; the journal was kept in {}",
                    aside.display()
                ),
            ));
        }
        if applied == 0 {
            return Ok(false);
        }

        // The file already holds the replayed document only if writing it
        // is lossless and gives the same bytes. A lossy write can match the
        // file's text while the document differs, so it always recovers.
        let written = coldopen_fountain::write(&replay.doc.to_script());
        if written.losses.is_empty() && io::encode(&written.text, session.style) == disk_bytes {
            return Ok(false);
        }

        let external_change = read.header.disk != Some(Fingerprint::of(disk_bytes));
        j.resume(valid_len, applied as u64).map_err(|e| jio(e, j))?;
        replay.dirty = true;
        replay.journal = None;
        *session = replay;
        *recovered = json!({ "commands": applied, "external_change": external_change });
        Ok(true)
    }

    fn doc_save(&mut self, p: DocSave) -> Result<Value, CommandError> {
        let opts = self.opts;
        let session = self
            .session
            .as_mut()
            .ok_or_else(CommandError::no_document)?;
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

        let bytes = io::encode(&written.text, session.style);
        io::atomic_write(&target, &bytes).map_err(|e| CommandError::io(&target, &e))?;
        let fingerprint = Fingerprint::of(&bytes);

        let same_file = session
            .path
            .as_ref()
            .is_some_and(|old| same_path(old, &target));
        if opts.journal {
            if same_file {
                if let Some(j) = &mut session.journal {
                    j.start(&session.doc, Some(fingerprint))
                        .map_err(|e| CommandError::io(j.path(), &e))?;
                }
            } else {
                let (mut j, _) = open_journal(&target, opts.fsync)?;
                j.start(&session.doc, Some(fingerprint))
                    .map_err(|e| CommandError::io(j.path(), &e))?;
                if let Some(old) = session.journal.replace(j) {
                    let _ = old.delete();
                }
            }
        }
        session.path = Some(target.clone());
        session.dirty = false;
        Ok(json!({
            "path": target.display().to_string(),
            "format": format,
            "bytes": bytes.len(),
            "losses": losses,
        }))
    }

    fn doc_close(&mut self, p: DocClose) -> Result<Value, CommandError> {
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
        self.discard();
        Ok(json!({}))
    }

    /// Handles one request envelope and returns the response envelope.
    pub fn handle(&mut self, request: &Value) -> Value {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let id = if matches!(id, Value::Null | Value::Number(_) | Value::String(_)) {
            id
        } else {
            return failure(
                Value::Null,
                CommandError::new("invalid_request", "\"id\" must be a number, string or null"),
            );
        };
        match self.parse_and_run(request) {
            Ok(result) => json!({ "v": 1, "id": id, "ok": true, "result": result }),
            Err(e) => failure(id, e),
        }
    }

    fn parse_and_run(&mut self, request: &Value) -> Result<Value, CommandError> {
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
        let cmd = Command::from_parts(name, params).map_err(CommandError::invalid_params)?;
        self.execute(cmd)
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

fn same_path(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}
