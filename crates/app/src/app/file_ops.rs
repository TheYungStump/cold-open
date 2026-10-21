//! Files: new, open, save, save as, close and quit, PDF export, the
//! recovery dialog and leftover work (docs/specs/editor.md §11, §13).
//! Every change to a document is still a command; this module only decides
//! which command to send and when.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use coldopen_core::command::DocSave;
use coldopen_core::{BlockId, CommandError, Document, PreparedSave, RenderedSave, SaveJob};
use coldopen_screenplay::ElementKind;
use iced::Task;
use serde_json::{Value, json};

use super::{App, Message, Saving};
use crate::files::{Ask, with_extension};

/// What waits for the writer's answer to "save the changes?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum After {
    New,
    /// Show the open dialog.
    Open,
    OpenPath(PathBuf),
    Close,
    Quit,
    /// Restore a set-aside copy as a new unsaved script journaled at
    /// `path` (journal.md, "Recovered versions").
    Restore {
        copy: PathBuf,
        path: PathBuf,
    },
    /// Go back to the saved file; unsaved changes are set aside.
    Revert,
}

/// A question the app asks inside its window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prompt {
    /// The script has unsaved changes and the writer asked to leave it.
    Unsaved(After),
    /// Fountain can't hold everything in the script: save anyway? With
    /// the path for a "save as".
    Lossy {
        message: String,
        path: Option<PathBuf>,
        /// For a "save as": whether the file dialog asked about replacing
        /// that path.
        overwrite: bool,
        /// A copy written with Save a copy, not the script saved.
        export: bool,
        /// The pending tracked changes the save would accept, when tracked
        /// changes are among its losses (editor.md §17.5).
        tracked: usize,
    },
    /// Another program changed the file since Cold Open last read or saved
    /// it: overwrite, reload, or save as? With the save's `allow_lossy`.
    Changed { message: String, allow_lossy: bool },
    /// Opening found unsaved changes in a crash journal written on another
    /// computer or for another folder: recover them, or set them aside?
    Foreign {
        path: PathBuf,
        host: String,
        at: String,
    },
    /// Delete this set-aside copy? It cannot be undone.
    DeleteCopy(PathBuf),
    /// What name should the writer's changes carry? (editor.md §17.4)
    AuthorName { name: String, then: super::NameThen },
    /// Accept, or reject, every pending change? (editor.md §17.2)
    SettleAll { accept: bool, count: usize },
}

/// The writer's answer to a [`Prompt`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Save,
    DontSave,
    Cancel,
    /// Changed: replace the other program's version.
    Overwrite,
    /// Changed: read the file again; unsaved changes are kept aside.
    Reload,
    /// Changed: save under another name.
    SaveAs,
    /// Foreign: recover the journal's changes.
    Recover,
    /// Foreign: open the file, keeping the journal aside.
    SetAside,
    /// Lossy: save as a Cold Open project instead (editor.md §17.5).
    SaveAsProject,
}

/// What the recovery dialog shows (editor.md §13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recovery {
    pub commands: u64,
    /// When the newest recovered change was made, in ms since the epoch.
    pub last: Option<u64>,
    /// Headings of the scenes the changes touched, in script order.
    pub scenes: Vec<String>,
    pub external_change: bool,
    /// The script was never saved, so Discard leaves nothing.
    pub never_saved: bool,
}

/// The PDF export options (editor.md §11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportForm {
    pub scene_numbers: bool,
    pub title_page: bool,
    pub watermark: String,
    /// Print the script with every pending change accepted, instead of
    /// marked (editor.md §17).
    pub accepted: bool,
}

impl Default for ExportForm {
    fn default() -> ExportForm {
        ExportForm {
            scene_numbers: false,
            title_page: true,
            watermark: String::new(),
            accepted: false,
        }
    }
}

/// "5 minutes ago" for a time in ms since the epoch.
pub fn ago(ms: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let s = now.saturating_sub(ms) / 1000;
    let (n, unit) = match s {
        0..60 => return "less than a minute ago".into(),
        60..3600 => (s / 60, "minute"),
        3600..86_400 => (s / 3600, "hour"),
        _ => (s / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

/// The document as the file on disk holds it, with the ids a fresh open
/// gives, or an empty one when there is no file.
fn disk_document(path: &Path) -> Document {
    let format = coldopen_core::FormatId::for_path(path).unwrap_or_default();
    std::fs::read(path)
        .ok()
        .and_then(|bytes| format.get().read(&bytes).ok())
        .map(|read| read.doc)
        .unwrap_or_default()
}

/// The heading of the scene each block is in, or a name for the part
/// before the first scene.
fn scene_names(doc: &Document) -> Vec<String> {
    let mut current = "Before the first scene".to_string();
    doc.blocks
        .iter()
        .map(|b| {
            if b.element.kind == ElementKind::SceneHeading {
                let t = b.element.text.plain_text();
                current = if t.trim().is_empty() {
                    "An untitled scene".into()
                } else {
                    t
                };
            }
            current.clone()
        })
        .collect()
}

/// The scenes whose blocks differ between the file and the recovered
/// document, matched by block id: changed, added, moved or removed.
pub fn touched_scenes(now: &Document, disk: &Document) -> Vec<String> {
    let names_now = scene_names(now);
    let names_disk = scene_names(disk);
    let ids_now: BTreeSet<BlockId> = now.blocks.iter().map(|b| b.id).collect();
    let mut out: Vec<String> = Vec::new();
    let mut add = |name: &String| {
        if !out.contains(name) {
            out.push(name.clone());
        }
    };
    for (i, b) in now.blocks.iter().enumerate() {
        let before = i.checked_sub(1).map(|j| now.blocks[j].id);
        let changed = match disk.index_of(b.id) {
            Some(k) => {
                disk.blocks[k].element != b.element
                    || k.checked_sub(1).map(|j| disk.blocks[j].id) != before
            }
            None => true,
        };
        if changed {
            add(&names_now[i]);
        }
    }
    for (k, b) in disk.blocks.iter().enumerate() {
        if !ids_now.contains(&b.id) {
            add(&names_disk[k]);
        }
    }
    out
}

impl App {
    /// A script that has never been saved where the writer chose: it lives
    /// in `Unsaved/`, or has no path at all.
    pub(super) fn untitled(&self) -> bool {
        self.d
            .session()
            .and_then(|s| s.path.as_ref())
            .is_none_or(|p| {
                self.dirs
                    .as_ref()
                    .is_some_and(|d| p.starts_with(d.unsaved()))
            })
    }

    pub(super) fn dirty(&self) -> bool {
        self.d.session().is_some_and(|s| s.dirty)
    }

    /// The folder dialogs start in: the script's own, or the most recent
    /// file's.
    fn start_dir(&self) -> Option<PathBuf> {
        let own = (!self.untitled())
            .then(|| self.d.session().and_then(|s| s.path.clone()))
            .flatten();
        own.or_else(|| self.settings.recent.first().cloned())
            .and_then(|p| p.parent().map(Path::to_path_buf))
    }

    /// The script's name without its extension.
    pub(super) fn stem(&self) -> String {
        if self.untitled() {
            return "Untitled".into();
        }
        self.d
            .session()
            .and_then(|s| s.path.as_ref())
            .and_then(|p| p.file_stem())
            .map_or_else(|| "Untitled".into(), |s| s.to_string_lossy().into_owned())
    }

    /// Opens a file dialog; its answer comes back as [`Message::Chosen`].
    pub(super) fn ask(&self, ask: Ask) -> Task<Message> {
        let answer = (self.dialogs.0)(ask.clone(), self.start_dir());
        Task::perform(answer, move |p| Message::Chosen(ask.clone(), p))
    }

    /// Leaves the script for `after`, asking first when it has unsaved
    /// changes.
    pub(super) fn leave(&mut self, after: After) -> Task<Message> {
        if self.dirty() {
            self.prompt = Some(Prompt::Unsaved(after));
            return Task::none();
        }
        self.proceed(after)
    }

    /// Does `after` now that the script is saved, clean or given up. While
    /// a save is being written it waits for it (editor.md §16).
    pub(super) fn proceed(&mut self, after: After) -> Task<Message> {
        if self.saving.is_some() && after != After::Open {
            self.waiting = Some(after);
            return Task::none();
        }
        match after {
            After::New => {
                self.close_links();
                self.d.shutdown();
                self.new_script();
                self.after_change(true)
            }
            After::Open => self.ask(Ask::Open),
            After::OpenPath(p) => {
                self.close_links();
                self.d.shutdown();
                self.open(&p);
                self.after_change(true)
            }
            After::Close => {
                self.close_links();
                self.d.shutdown();
                self.editor = self.fresh_editor(&Document::default());
                self.after_change(false)
            }
            After::Quit => {
                self.close_links();
                self.d.shutdown();
                iced::exit()
            }
            After::Restore { copy, path } => {
                self.close_links();
                self.d.shutdown();
                self.restore(&copy, &path);
                self.after_change(true)
            }
            After::Revert => self.reload(),
        }
    }

    /// Opens a set-aside copy as a new unsaved script journaled at
    /// `path`; the copy and its script are never written (journal.md,
    /// "Recovered versions").
    pub(super) fn restore(&mut self, copy: &Path, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let script = coldopen_core::recovered::parse(copy).map(|c| c.script);
        if self
            .run("recovered.restore", json!({ "copy": copy, "path": path }))
            .is_some()
            && let Some(s) = self.d.session()
        {
            self.editor = self.fresh_editor(&s.doc);
            let name = script
                .as_deref()
                .and_then(Path::file_name)
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            self.banners.push(
                format!(
                    "Restored a recovered version of {name} as a new script. Save it to keep it."
                )
                .into(),
            );
        }
    }

    /// The format the open script is saved in, as an extension.
    pub(super) fn ext(&self) -> &'static str {
        self.d
            .session()
            .map_or(coldopen_core::FormatId::Fountain, |s| s.format)
            .name()
    }

    /// The name a dialog suggests for the script.
    pub(super) fn suggested(&self) -> String {
        format!("{}.{}", self.stem(), self.ext())
    }

    /// Writes a copy of the script in the format `path` names (Save a
    /// copy); the script itself, its path and its format do not change.
    pub(super) fn export_copy(
        &mut self,
        path: PathBuf,
        allow_lossy: bool,
        overwrite: bool,
    ) -> Task<Message> {
        let name = match coldopen_core::FormatId::for_path(&path) {
            Some(coldopen_core::FormatId::Fdx) => "export.fdx",
            _ => "export.fountain",
        };
        let params = json!({ "path": path, "allow_lossy": allow_lossy, "overwrite": overwrite });
        let cmd = match coldopen_core::Command::from_parts(name, params) {
            Ok(c) => c,
            Err(e) => {
                self.banners.push(e.into());
                return Task::none();
            }
        };
        match self.d.execute(cmd) {
            Ok(r) => {
                self.notices(&r);
                self.banners
                    .push(format!("Saved a copy as {}.", super::view::file_name(&path)).into());
            }
            Err(e) if e.code == "lossy_format" => {
                self.prompt = Some(self.lossy_prompt(e, Some(path), overwrite, true));
            }
            Err(e) => self
                .banners
                .push(format!("The copy was not written: {}", e.message).into()),
        }
        Task::none()
    }

    /// Shows a command result's warnings as banners. Copies set aside are
    /// announced without their paths, with a Recovered versions button;
    /// repairs are the repair dialog's to show. True when work was set
    /// aside.
    pub(super) fn notices(&mut self, r: &Value) -> bool {
        let mut set_aside = false;
        for w in r["warnings"].as_array().into_iter().flatten() {
            if matches!(w["class"].as_str(), Some("fixed" | "dropped")) {
                continue;
            }
            if w["code"] == "changes_accepted" {
                let n = w["details"]["count"].as_u64().unwrap_or(1);
                self.banners
                    .push(super::Banner::accepted(n, w["details"]["copy"].is_string()));
            } else if w["details"]["copy"].is_string() {
                let b = if w["code"] == "original_kept" {
                    super::Banner::original_kept()
                } else {
                    set_aside = true;
                    super::Banner::set_aside()
                };
                if !self.banners.contains(&b) {
                    self.banners.push(b);
                }
            } else if let Some(m) = w["message"].as_str() {
                self.banners.push(m.into());
            }
        }
        set_aside
    }

    /// The question for a save that would lose something. When tracked
    /// changes are among the losses it says so first (editor.md §17.5).
    pub(super) fn lossy_prompt(
        &self,
        e: CommandError,
        path: Option<PathBuf>,
        overwrite: bool,
        export: bool,
    ) -> Prompt {
        let losses: Vec<Value> = e
            .details
            .as_ref()
            .and_then(|d| d["losses"].as_array().cloned())
            .unwrap_or_default();
        let tracked = if losses.iter().any(|l| l["code"] == "tracked_changes") {
            self.d.session().map_or(0, |s| s.doc.pending().len().max(1))
        } else {
            0
        };
        // The heading names the format (view.rs).
        let message = if tracked > 0 {
            let others = losses
                .iter()
                .any(|l| l["code"] != "tracked_changes" && l["class"] == "lost");
            let (changes, them) = if tracked == 1 {
                ("the pending change".to_string(), "it")
            } else {
                (format!("all {tracked} pending changes"), "them")
            };
            let saving = if export {
                format!(
                    "Saving the copy writes it with {changes} accepted. The script keeps {them}."
                )
            } else {
                format!(
                    "Saving accepts {changes} in the script. The version with {them} marked is kept in Recovered versions."
                )
            };
            format!(
                "{saving}{}",
                if others {
                    " Some other things in this script can't be kept either."
                } else {
                    ""
                }
            )
        } else {
            e.message
        };
        Prompt::Lossy {
            message,
            path,
            overwrite,
            export,
            tracked,
        }
    }

    /// Asks before deleting a set-aside copy (editor.md §13).
    pub(super) fn ask_delete(&mut self, copy: PathBuf) -> Task<Message> {
        self.prompt = Some(Prompt::DeleteCopy(copy));
        Task::none()
    }

    pub(super) fn reply(&mut self, reply: Reply) -> Task<Message> {
        if matches!(self.prompt, Some(Prompt::AuthorName { .. })) {
            return self.name_done(reply == Reply::Save);
        }
        let Some(prompt) = self.prompt.take() else {
            return Task::none();
        };
        match (prompt, reply) {
            (_, Reply::Cancel) => {
                self.after_save = None;
                Task::none()
            }
            (Prompt::Unsaved(after), Reply::Save) => {
                self.after_save = Some(after);
                self.save(false)
            }
            (Prompt::Unsaved(after), Reply::DontSave) => {
                if let Some(r) = self.run("doc.close", json!({ "discard": true })) {
                    self.notices(&r);
                }
                self.proceed(after)
            }
            (
                Prompt::Lossy {
                    path: Some(p),
                    overwrite,
                    export: true,
                    ..
                },
                Reply::Save,
            ) => self.export_copy(p, true, overwrite),
            (
                Prompt::Lossy {
                    path: Some(p),
                    overwrite,
                    ..
                },
                Reply::Save,
            ) => self.save_as(p, true, overwrite),
            (Prompt::Lossy { path: None, .. }, Reply::Save) => self.save(true),
            (Prompt::Lossy { export: false, .. }, Reply::SaveAsProject) => {
                self.ask(Ask::SaveAsProject(format!("{}.coldopen", self.stem())))
            }
            (Prompt::SettleAll { accept, .. }, Reply::Save) => {
                self.settle_all(accept);
                self.after_change(false)
            }
            (Prompt::Lossy { .. }, Reply::DontSave) => {
                self.after_save = None;
                Task::none()
            }
            (Prompt::Changed { allow_lossy, .. }, Reply::Overwrite) => {
                self.save_with(allow_lossy, true)
            }
            (Prompt::Changed { .. }, Reply::Reload) => {
                self.after_save = None;
                self.reload()
            }
            (Prompt::Changed { .. }, Reply::SaveAs) => self.ask(Ask::SaveAs(self.suggested())),
            (Prompt::Foreign { path, .. }, Reply::Recover) => {
                self.open_with(&path, "apply");
                self.after_change(true)
            }
            (Prompt::Foreign { path, .. }, Reply::SetAside) => {
                self.open_with(&path, "discard");
                self.after_change(true)
            }
            (Prompt::DeleteCopy(copy), Reply::Save) => {
                self.run("recovered.delete", json!({ "copy": copy }));
                self.recovered_refresh();
                self.after_change(false)
            }
            // A key that answers some other question: this one stays open.
            (prompt, _) => {
                self.prompt = Some(prompt);
                Task::none()
            }
        }
    }

    /// Reads the file again after another program changed it. The unsaved
    /// changes are kept in a copy of the journal (doc.revert).
    pub(super) fn reload(&mut self) -> Task<Message> {
        if let Some(r) = self.run("doc.revert", json!({}))
            && let Some(s) = self.d.session()
        {
            self.editor = self.fresh_editor(&s.doc);
            if !self.notices(&r) {
                self.banners
                    .push(format!("Reloaded {}.", self.script_name()).into());
            }
        }
        self.after_change(true)
    }

    /// Mod+S: saves in place on a worker thread, or asks where for a
    /// script that has never been saved (docs/specs/journal.md, "Saving
    /// off the UI thread").
    pub(super) fn save(&mut self, allow_lossy: bool) -> Task<Message> {
        self.save_with(allow_lossy, false)
    }

    /// [`App::save`], with `overwrite` once the writer chose to replace
    /// another program's changes.
    pub(super) fn save_with(&mut self, allow_lossy: bool, overwrite: bool) -> Task<Message> {
        if self.d.session().is_none() {
            return Task::none();
        }
        if self.untitled() {
            return self.ask(Ask::SaveAs(self.suggested()));
        }
        if self.saving.is_some() {
            self.save_again = true;
            return Task::none();
        }
        let p = DocSave {
            allow_lossy,
            overwrite,
            ..DocSave::default()
        };
        // The UI thread only takes a copy of the document; the worker
        // writes the file out and checks it (journal.md, "Saving off the UI
        // thread").
        let rev = self.d.session().map(|s| s.rev);
        match self.d.save_prepare(&p) {
            Ok(job) => {
                self.saving = Some(Saving::Rendering { allow_lossy });
                let render = render_on_worker(job);
                // Saving anyway to Fountain or .fdx accepted the pending
                // changes first (commands.md, `doc.save`): the page, the
                // navigator and the panel show that now, not at the next key.
                if self.d.session().map(|s| s.rev) != rev {
                    return Task::batch([self.after_change(false), render]);
                }
                render
            }
            Err(e) => {
                self.after_save = None;
                self.banners
                    .push(format!("The script was not saved: {}", e.message).into());
                Task::none()
            }
        }
    }

    /// The worker rendered the file: the journal gets its marker and the
    /// worker writes the file, or the save ends with the question its
    /// rendering raised.
    pub(super) fn rendered(&mut self, r: RenderedSave) -> Task<Message> {
        let Some(Saving::Rendering { allow_lossy }) = self.saving else {
            return Task::none();
        };
        let e = match self.d.save_mark(r) {
            Ok(prepared) => {
                let task = write_on_worker(&prepared);
                self.saving = Some(Saving::Writing(prepared));
                return task;
            }
            Err(e) => e,
        };
        self.saving = None;
        self.save_again = false;
        // The writer has moved on (closed, opened another script): what
        // waited runs, and no question is asked about a script left behind.
        if let Some(after) = self.waiting.take() {
            self.after_save = None;
            if e.code != "save_superseded" {
                self.banners
                    .push(format!("The script was not saved: {}", e.message).into());
            }
            return self.proceed(after);
        }
        match e.code {
            "lossy_format" => self.prompt = Some(self.lossy_prompt(e, None, false, false)),
            "file_changed" => {
                self.prompt = Some(Prompt::Changed {
                    message: e.message,
                    allow_lossy,
                });
            }
            _ => {
                self.after_save = None;
                self.banners
                    .push(format!("The script was not saved: {}", e.message).into());
            }
        }
        Task::none()
    }

    /// The worker finished writing the file.
    pub(super) fn saved(&mut self, written: Result<(), String>) -> Task<Message> {
        let Some(Saving::Writing(prepared)) = self.saving.take() else {
            return Task::none();
        };
        let commit = self
            .d
            .save_commit(prepared, written.map_err(std::io::Error::other));
        // Whatever the save's outcome, what waited for it runs now.
        let waiting = self.waiting.take();
        if let Err(e) = commit {
            self.after_save = None;
            self.save_again = false;
            // A save that finished after its script was closed or reopened
            // did save; it just no longer says anything about this script.
            self.banners.push(
                if e.code == "save_superseded" {
                    let mut m = e.message;
                    if let Some(first) = m.get_mut(0..1) {
                        first.make_ascii_uppercase();
                    }
                    m
                } else {
                    format!("The script was not saved: {}", e.message)
                }
                .into(),
            );
            return waiting.map_or_else(Task::none, |after| self.proceed(after));
        }
        if let Ok(r) = &commit {
            self.notices(r);
        }
        if let Some(after) = waiting {
            self.after_save = None;
            self.save_again = false;
            return self.proceed(after);
        }
        // Edits made while the file was written are still unsaved.
        let again = std::mem::take(&mut self.save_again) || self.after_save.is_some();
        if again && self.dirty() {
            return self.save(false);
        }
        match self.after_save.take() {
            Some(after) => self.proceed(after),
            None => Task::none(),
        }
    }

    /// Saves to a path the writer chose, here on the UI thread: the dialog
    /// has just paused the writer anyway (decision 10 in
    /// docs/progress/phase-3.md). `overwrite` is true when that dialog
    /// asked about replacing this very path.
    pub(super) fn save_as(
        &mut self,
        path: PathBuf,
        allow_lossy: bool,
        overwrite: bool,
    ) -> Task<Message> {
        if self.saving.is_some() {
            self.banners
                .push("A save is still running. Try Save as again in a moment.".into());
            return Task::none();
        }
        let params = json!({ "path": path, "allow_lossy": allow_lossy, "overwrite": overwrite });
        let cmd = match coldopen_core::Command::from_parts("doc.save", params) {
            Ok(c) => c,
            Err(e) => {
                self.banners.push(e.into());
                return Task::none();
            }
        };
        match self.d.execute(cmd) {
            Ok(r) => {
                self.notices(&r);
                self.settings.add_recent(&path);
                self.save_settings();
                match self.after_save.take() {
                    Some(after) => self.proceed(after),
                    None => self.after_change(false),
                }
            }
            Err(e) if e.code == "lossy_format" => {
                self.prompt = Some(self.lossy_prompt(e, Some(path), overwrite, false));
                Task::none()
            }
            Err(e) => {
                self.after_save = None;
                self.banners
                    .push(format!("The script was not saved: {}", e.message).into());
                Task::none()
            }
        }
    }

    /// A file dialog's answer.
    pub(super) fn chosen(&mut self, ask: Ask, path: Option<PathBuf>) -> Task<Message> {
        let Some(path) = path else {
            // Cancelled: nothing waits on a save that will not happen.
            if matches!(ask, Ask::SaveAs(_) | Ask::SaveAsProject(_)) {
                self.after_save = None;
            }
            return Task::none();
        };
        match ask {
            Ask::Open => self.leave(After::OpenPath(path)),
            Ask::SaveAs(_) => {
                // The dialog asked before replacing the path it returned,
                // not the one an added extension makes. A name typed with
                // no extension keeps the script's format.
                let target = with_extension(path.clone(), self.ext());
                let asked = target == path;
                self.save_as(target, false, asked)
            }
            Ask::SaveAsProject(_) => {
                let target = with_extension(path.clone(), "coldopen");
                let asked = target == path;
                self.save_as(target, false, asked)
            }
            Ask::ExportFdx(_) | Ask::ExportFountain(_) => {
                let ext = if matches!(ask, Ask::ExportFdx(_)) {
                    "fdx"
                } else {
                    "fountain"
                };
                let target = with_extension(path.clone(), ext);
                let asked = target == path;
                self.export_copy(target, false, asked)
            }
            Ask::RepairIn => {
                let name = format!(
                    "{} (repaired).fdx",
                    path.file_stem()
                        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
                );
                self.ask(Ask::RepairOut(path, name))
            }
            Ask::RepairOut(input, _) => {
                self.repair_chosen(input, path);
                Task::none()
            }
            Ask::ExportPdf(_) => {
                let target = with_extension(path.clone(), "pdf");
                let asked = target == path;
                self.export_to(target, asked)
            }
        }
    }

    pub(super) fn export_start(&mut self) -> Task<Message> {
        if self.d.session().is_none() {
            return Task::none();
        }
        if self.export.is_none() {
            self.export = Some(ExportForm::default());
        }
        iced::widget::operation::focus(super::EXPORT)
    }

    pub(super) fn export_choose(&mut self) -> Task<Message> {
        self.ask(Ask::ExportPdf(format!("{}.pdf", self.stem())))
    }

    /// Exports to a path the writer chose; `overwrite` when the system's
    /// dialog asked about replacing this very path.
    fn export_to(&mut self, path: PathBuf, overwrite: bool) -> Task<Message> {
        let form = self.export.take().unwrap_or_default();
        let mut params = json!({
            "path": path,
            "scene_numbers": form.scene_numbers,
            "title_page": form.title_page,
            "overwrite": overwrite,
        });
        if !form.watermark.trim().is_empty() {
            params["watermark"] = json!(form.watermark.trim());
        }
        if form.accepted {
            params["changes"] = json!("accepted");
        }
        if let Some(r) = self.run("export.pdf", params) {
            let warnings = r["warnings"].as_array().map_or(0, Vec::len);
            let mut msg = format!(
                "Exported {} {} to {}.",
                r["pages"],
                if r["pages"] == 1 { "page" } else { "pages" },
                path.display()
            );
            if warnings > 0 {
                msg.push_str(&format!(
                    " {warnings} warning{}: {}",
                    if warnings == 1 { "" } else { "s" },
                    r["warnings"][0]["message"].as_str().unwrap_or("")
                ));
            }
            self.banners.push(msg.into());
        }
        Task::none()
    }

    /// Fills the recovery dialog after an open recovered unsaved work.
    pub(super) fn recovery_from(&mut self, recovered: &Value, path: &Path) {
        if recovered.is_null() {
            return;
        }
        let disk = disk_document(path);
        let scenes = self
            .d
            .session()
            .map(|s| touched_scenes(&s.doc, &disk))
            .unwrap_or_default();
        self.recovery = Some(Recovery {
            commands: recovered["commands"].as_u64().unwrap_or(0),
            last: recovered["last"].as_u64().filter(|&t| t > 0),
            scenes,
            external_change: recovered["external_change"].as_bool().unwrap_or(false),
            never_saved: !path.exists(),
        });
    }

    /// Keep: carry on with the recovered script, unsaved.
    pub(super) fn recovery_keep(&mut self) -> Task<Message> {
        self.recovery = None;
        self.after_change(true)
    }

    /// Discard: back to the saved file, with the journal kept aside; for a
    /// script never saved, a new one.
    pub(super) fn recovery_discard(&mut self) -> Task<Message> {
        let Some(r) = self.recovery.take() else {
            return Task::none();
        };
        if r.never_saved {
            self.close_links();
            if let Some(r) = self.run("doc.close", json!({ "discard": true })) {
                self.notices(&r);
            }
            self.new_script();
        } else if let Some(r) = self.run("doc.revert", json!({})) {
            self.notices(&r);
            if let Some(s) = self.d.session() {
                self.editor = self.fresh_editor(&s.doc);
            }
        }
        self.after_change(true)
    }
}

/// Renders a save on its own thread, so typing never waits on writing the
/// script out: Fountain's writer, or a project's JSON and deflate, take
/// tens of milliseconds on a feature script. The outcome comes back as
/// [`Message::Rendered`].
fn render_on_worker(job: SaveJob) -> Task<Message> {
    let ticket = job.ticket();
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("save".into())
        .spawn(move || {
            let _ = tx.send(job.render());
        });
    let failed = move |why: String| {
        Message::Rendered(RenderedSave::failed(
            ticket,
            CommandError::new("save_failed", why),
        ))
    };
    if let Err(e) = spawned {
        return Task::done(failed(e.to_string()));
    }
    Task::perform(rx, move |r| match r {
        Ok(r) => Message::Rendered(r),
        Err(_) => failed("the save stopped before the script was written out".into()),
    })
}

/// Writes a prepared save on its own thread, so typing never waits on the
/// disk; the result comes back as [`Message::Saved`].
fn write_on_worker(prepared: &PreparedSave) -> Task<Message> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    let target = prepared.target.clone();
    let bytes = prepared.bytes.clone();
    let spawned = std::thread::Builder::new()
        .name("save".into())
        .spawn(move || {
            let r = coldopen_core::io::atomic_write(&target, &bytes).map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
    if let Err(e) = spawned {
        return Task::done(Message::Saved(Err(e.to_string())));
    }
    Task::perform(
        async move {
            rx.await
                .unwrap_or_else(|_| Err("the save stopped before it finished".into()))
        },
        Message::Saved,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use coldopen_screenplay::{Element, Script};

    fn doc(items: &[(ElementKind, &str)]) -> Document {
        Document::from_script(Script {
            elements: items.iter().map(|(k, t)| Element::new(*k, *t)).collect(),
            ..Script::default()
        })
    }

    #[test]
    fn touched_scenes_by_id() {
        use ElementKind::*;
        let disk = doc(&[
            (SceneHeading, "INT. A"),
            (Action, "One."),
            (SceneHeading, "INT. B"),
            (Action, "Two."),
            (SceneHeading, "INT. C"),
            (Action, "Three."),
        ]);
        let mut now = disk.clone();
        assert!(touched_scenes(&now, &disk).is_empty());
        now.blocks[3].element = Element::new(Action, "Two, changed.");
        now.blocks.remove(5);
        assert_eq!(touched_scenes(&now, &disk), ["INT. B", "INT. C"]);
        assert_eq!(touched_scenes(&now, &Document::default()).len(), 3);
    }

    #[test]
    fn times_read_as_ago() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert_eq!(ago(now), "less than a minute ago");
        assert_eq!(ago(now - 61_000), "1 minute ago");
        assert_eq!(ago(now - 3 * 3_600_000), "3 hours ago");
    }
}
