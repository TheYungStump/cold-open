//! Recent files that are gone (editor.md §11): found on a worker thread
//! whenever the list is shown, shown dimmed with "Missing", and never
//! dropped unless the writer says so, since an unplugged drive or a folder
//! that has not synced yet comes back.

use std::path::{Path, PathBuf};

use iced::Task;

use super::view::file_name;
use super::{App, Message, Prompt};

/// Whether a recent file is gone: no file, and no crash journal that
/// would open it.
pub fn gone(p: &Path) -> bool {
    !p.exists() && !coldopen_core::journal::path_for(p).exists()
}

impl App {
    /// Checks every recent file on a worker thread, so a slow network path
    /// never holds up a frame. The answer comes back as
    /// [`Message::RecentChecked`].
    pub(super) fn check_recent(&self) -> Task<Message> {
        let paths = self.settings.recent.clone();
        if paths.is_empty() {
            return Task::none();
        }
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let missing: Vec<PathBuf> = paths.iter().filter(|p| gone(p)).cloned().collect();
            let _ = tx.send((paths, missing));
        });
        Task::perform(rx, |r| match r {
            Ok((checked, missing)) => Message::RecentChecked { checked, missing },
            Err(_) => Message::Noop,
        })
    }

    /// The worker's answer: the files it checked, and which are gone.
    pub(super) fn recent_checked(&mut self, checked: Vec<PathBuf>, missing: Vec<PathBuf>) {
        for p in checked {
            if missing.contains(&p) && self.settings.recent.contains(&p) {
                self.missing.insert(p);
            } else {
                self.missing.remove(&p);
            }
        }
    }

    /// "Remove from recent": the file leaves the list and `settings.json`.
    pub(super) fn forget_recent(&mut self, p: &Path) {
        self.settings.recent.retain(|r| r != p);
        self.missing.remove(p);
        self.save_settings();
        self.banners
            .push(format!("Removed {} from recent files.", file_name(p)).into());
    }

    /// Choosing a recent file that is gone asks whether to remove it,
    /// before anything else happens to the open script. `None` when the
    /// file is there.
    pub(super) fn missing_recent(&mut self, p: &Path) -> Option<Task<Message>> {
        if !self.settings.recent.iter().any(|r| r == p) || !gone(p) {
            return None;
        }
        self.missing.insert(p.to_path_buf());
        self.prompt = Some(Prompt::MissingRecent(p.to_path_buf()));
        Some(Task::none())
    }
}
