//! Files: new, open, save, save as, close and quit, PDF export, the
//! recovery dialog and leftover work (docs/specs/editor.md §11, §13).
//! Every change to a document is still a command; this module only decides
//! which command to send and when.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use coldopen_core::command::DocSave;
use coldopen_core::{BlockId, Document, PreparedSave};
use coldopen_screenplay::ElementKind;
use iced::Task;
use serde_json::{Value, json};

use super::{App, Message};
use crate::editor::Editor;
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
    },
}

/// The writer's answer to a [`Prompt`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Save,
    DontSave,
    Cancel,
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
}

impl Default for ExportForm {
    fn default() -> ExportForm {
        ExportForm {
            scene_numbers: false,
            title_page: true,
            watermark: String::new(),
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
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = coldopen_core::io::decode(&bytes).text;
            Document::from_script(coldopen_fountain::parse(&text).script)
        }
        Err(_) => Document::default(),
    }
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

    /// Does `after` now that the script is saved, clean or given up.
    fn proceed(&mut self, after: After) -> Task<Message> {
        match after {
            After::New => {
                self.d.shutdown();
                self.new_script();
                self.after_change(true)
            }
            After::Open => self.ask(Ask::Open),
            After::OpenPath(p) => {
                self.d.shutdown();
                self.open(&p);
                self.after_change(true)
            }
            After::Close => {
                self.d.shutdown();
                self.editor = Editor::new(&Document::default());
                self.after_change(false)
            }
            After::Quit => {
                self.d.shutdown();
                iced::exit()
            }
        }
    }

    pub(super) fn reply(&mut self, reply: Reply) -> Task<Message> {
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
                self.run("doc.close", json!({ "discard": true }));
                self.proceed(after)
            }
            (Prompt::Lossy { path: Some(p), .. }, Reply::Save) => self.save_as(p, true),
            (Prompt::Lossy { path: None, .. }, Reply::Save) => self.save(true),
            (Prompt::Lossy { .. }, Reply::DontSave) => {
                self.after_save = None;
                Task::none()
            }
        }
    }

    /// Mod+S: saves in place on a worker thread, or asks where for a
    /// script that has never been saved (docs/specs/journal.md, "Saving
    /// off the UI thread").
    pub(super) fn save(&mut self, allow_lossy: bool) -> Task<Message> {
        if self.d.session().is_none() {
            return Task::none();
        }
        if self.untitled() {
            return self.ask(Ask::SaveAs(format!("{}.fountain", self.stem())));
        }
        if self.saving.is_some() {
            self.save_again = true;
            return Task::none();
        }
        let p = DocSave {
            allow_lossy,
            ..DocSave::default()
        };
        match self.d.save_prepare(&p) {
            Ok(prepared) => {
                let task = write_on_worker(&prepared);
                self.saving = Some(prepared);
                task
            }
            Err(e) if e.code == "lossy_format" => {
                self.prompt = Some(Prompt::Lossy {
                    message: e.message,
                    path: None,
                });
                Task::none()
            }
            Err(e) => {
                self.after_save = None;
                self.banners
                    .push(format!("The script was not saved: {}", e.message));
                Task::none()
            }
        }
    }

    /// The worker finished writing the file.
    pub(super) fn saved(&mut self, written: Result<(), String>) -> Task<Message> {
        let Some(prepared) = self.saving.take() else {
            return Task::none();
        };
        if let Err(e) = self
            .d
            .save_commit(prepared, written.map_err(std::io::Error::other))
        {
            self.after_save = None;
            self.save_again = false;
            self.banners
                .push(format!("The script was not saved: {}", e.message));
            return Task::none();
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
    /// docs/progress/phase-3.md).
    pub(super) fn save_as(&mut self, path: PathBuf, allow_lossy: bool) -> Task<Message> {
        if self.saving.is_some() {
            self.banners
                .push("A save is still running. Try Save as again in a moment.".into());
            return Task::none();
        }
        let params = json!({ "path": path, "allow_lossy": allow_lossy });
        let cmd = match coldopen_core::Command::from_parts("doc.save", params) {
            Ok(c) => c,
            Err(e) => {
                self.banners.push(e);
                return Task::none();
            }
        };
        match self.d.execute(cmd) {
            Ok(_) => {
                self.settings.add_recent(&path);
                self.save_settings();
                match self.after_save.take() {
                    Some(after) => self.proceed(after),
                    None => self.after_change(false),
                }
            }
            Err(e) if e.code == "lossy_format" => {
                self.prompt = Some(Prompt::Lossy {
                    message: e.message,
                    path: Some(path),
                });
                Task::none()
            }
            Err(e) => {
                self.after_save = None;
                self.banners
                    .push(format!("The script was not saved: {}", e.message));
                Task::none()
            }
        }
    }

    /// A file dialog's answer.
    pub(super) fn chosen(&mut self, ask: Ask, path: Option<PathBuf>) -> Task<Message> {
        let Some(path) = path else {
            // Cancelled: nothing waits on a save that will not happen.
            if matches!(ask, Ask::SaveAs(_)) {
                self.after_save = None;
            }
            return Task::none();
        };
        match ask {
            Ask::Open => self.leave(After::OpenPath(path)),
            Ask::SaveAs(_) => self.save_as(with_extension(path, "fountain"), false),
            Ask::ExportPdf(_) => self.export_to(with_extension(path, "pdf")),
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

    fn export_to(&mut self, path: PathBuf) -> Task<Message> {
        let form = self.export.take().unwrap_or_default();
        let mut params = json!({
            "path": path,
            "scene_numbers": form.scene_numbers,
            "title_page": form.title_page,
        });
        if !form.watermark.trim().is_empty() {
            params["watermark"] = json!(form.watermark.trim());
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
            self.banners.push(msg);
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
            self.run("doc.close", json!({ "discard": true }));
            self.new_script();
        } else if self.run("doc.revert", json!({})).is_some()
            && let Some(s) = self.d.session()
        {
            self.editor = Editor::new(&s.doc);
            self.editor.draft = self.settings.draft;
        }
        self.after_change(true)
    }
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
