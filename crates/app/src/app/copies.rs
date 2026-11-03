//! Repaired files and set-aside copies in the app (docs/specs/editor.md
//! §13): the repair report, "Repair a file…", the Recovered versions
//! screen, and the start-up clean-up of old copies (docs/specs/journal.md,
//! "Recovered versions").

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use coldopen_core::recovered::{self, Copy, Kind};
use coldopen_screenplay::ElementKind;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Background, Border, Element, Fill, Length};
use serde_json::{Value, json};

use super::view::{body, dialog_button, file_name, hint, title};
use super::{After, App, Message, Origin, Request};
use crate::files::{Ask, with_extension};
use crate::fonts;
use crate::theme::Palette;

/// How many issues the repair dialog lists before Details.
const FIRST_ISSUES: usize = 3;
/// How many issues Details lists before the list scrolls.
const SCROLL_AFTER: usize = 8;
/// How much of a copy the preview shows.
const PREVIEW_BLOCKS: usize = 400;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// A repair report on screen: after opening a damaged .fdx file, or after
/// "Repair a file…" wrote a repaired copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairReport {
    /// The file's name.
    pub file: String,
    pub fixed: usize,
    pub dropped: usize,
    /// Each issue in plain words, with where it was; never script text.
    pub issues: Vec<String>,
    /// Every issue is listed, not just the first few.
    pub details: bool,
    /// The repaired copy's name, when "Repair a file…" wrote one.
    pub written: Option<String>,
}

impl RepairReport {
    /// From a `repaired` result (`{fixed, dropped, issues}`) or a
    /// `file.repair` result.
    fn from_result(file: &Path, repaired: &Value, issues: &Value, written: Option<&Path>) -> Self {
        let count = |k: &str| repaired[k].as_u64().unwrap_or(0) as usize;
        let issues = issues
            .as_array()
            .into_iter()
            .flatten()
            .filter(|i| matches!(i["class"].as_str(), Some("fixed" | "dropped")))
            .map(issue_line)
            .collect();
        RepairReport {
            file: file_name(file),
            fixed: count("fixed"),
            dropped: count("dropped"),
            issues,
            details: false,
            written: written.map(file_name),
        }
    }
}

/// One issue as the dialog lists it.
fn issue_line(i: &Value) -> String {
    let mut s = i["message"].as_str().unwrap_or("").to_string();
    if let Some(s0) = s.get_mut(0..1) {
        s0.make_ascii_uppercase();
    }
    match (
        i["line"].as_u64(),
        i["column"].as_u64(),
        i["paragraph"].as_u64(),
    ) {
        (Some(l), Some(c), _) => s.push_str(&format!(" (line {l}, column {c})")),
        (Some(l), None, _) => s.push_str(&format!(" (line {l})")),
        (None, _, Some(p)) => s.push_str(&format!(" (paragraph {})", p + 1)),
        _ => {}
    }
    s
}

/// One row of Recovered versions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyRow {
    pub copy: Copy,
    /// Its journal's header reads, or the original is there.
    pub readable: bool,
    /// Older than 30 days and now shown: the next start removes it.
    pub expires: bool,
}

/// The Recovered versions screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveredScreen {
    pub rows: Vec<CopyRow>,
    pub selected: usize,
    /// The selected copy's script, read-only, as (kind, text) lines.
    pub preview: Vec<(ElementKind, String)>,
    /// Why there is no preview.
    pub preview_note: Option<String>,
}

/// What a copy is, in plain words.
pub fn kind_words(k: Kind) -> &'static str {
    match k {
        Kind::Discarded => "Set aside when you chose Don't save",
        Kind::Diverged => "Changes that could not all be replayed",
        Kind::Corrupt => "An unreadable crash journal",
        Kind::Original => "The file before Cold Open repaired it",
        Kind::Marked => "With its tracked changes, before a save accepted them",
    }
}

impl App {
    /// After an open: the repair dialog, when the reader repaired anything.
    pub(super) fn repair_from(&mut self, repaired: &Value, path: &Path) {
        if repaired.is_null() {
            return;
        }
        self.repair = Some(RepairReport::from_result(
            path,
            repaired,
            &repaired["issues"],
            None,
        ));
    }

    /// "Repair a file…" wrote `out`: the report for it.
    pub(super) fn repair_file(&mut self, input: PathBuf, out: PathBuf, overwrite: bool) {
        let params = json!({ "path": input, "out": out, "overwrite": overwrite });
        let Some(r) = self.run("file.repair", params) else {
            return;
        };
        let counts = json!({
            "fixed": r["issues"].as_array().map_or(0, |a| a.iter().filter(|i| i["class"] == "fixed").count()),
            "dropped": r["issues"].as_array().map_or(0, |a| a.iter().filter(|i| i["class"] == "dropped").count()),
        });
        self.repair = Some(RepairReport::from_result(
            &input,
            &counts,
            &r["issues"],
            Some(&out),
        ));
    }

    /// The repair dialog's buttons and keys.
    pub(super) fn repair_reply(&mut self, m: &Message) -> iced::Task<Message> {
        match m {
            Message::RepairDetails => {
                if let Some(r) = &mut self.repair {
                    r.details = !r.details;
                }
                iced::Task::none()
            }
            Message::RepairSaveCopy => {
                let opened = self.repair.take().is_some_and(|r| r.written.is_none());
                if opened && self.d.session().is_some() {
                    return self.ask(Ask::SaveAs(self.suggested()));
                }
                iced::Task::none()
            }
            _ => {
                self.repair = None;
                iced::Task::none()
            }
        }
    }

    /// The repair dialog's answer to a chosen output path.
    pub(super) fn repair_chosen(&mut self, input: PathBuf, path: PathBuf) {
        let out = with_extension(path.clone(), "fdx");
        let asked = out == path;
        self.repair_file(input, out, asked);
    }

    /// Opens Recovered versions, and marks every copy it lists as shown.
    pub(super) fn recovered_open(&mut self) {
        let mut scripts: Vec<PathBuf> = self.settings.recent.clone();
        if let Some(p) = self.d.session().and_then(|s| s.path.clone()) {
            scripts.insert(0, p);
        }
        let copies = match &self.dirs {
            Some(d) => crate::files::known_copies(d, &scripts),
            None => scripts
                .iter()
                .flat_map(|p| recovered::of_script(p))
                .collect(),
        };
        let now = now_ms();
        if let Some(d) = &self.dirs {
            let mut offered = crate::files::load_offered(d);
            let before = offered.len();
            for c in &copies {
                offered.entry(c.copy.display().to_string()).or_insert(now);
            }
            if offered.len() != before
                && let Err(e) = crate::files::save_offered(d, &offered)
            {
                self.banners
                    .push(format!("Recovered versions could not note what it showed: {e}").into());
            }
        }
        let rows = copies
            .into_iter()
            .map(|copy| CopyRow {
                readable: recovered::readable(&copy),
                expires: now.saturating_sub(copy.at) > recovered::KEEP_MS,
                copy,
            })
            .collect();
        let selected = self.recovered.as_ref().map_or(0, |s| s.selected);
        self.recovered = Some(RecoveredScreen {
            rows,
            ..RecoveredScreen::default()
        });
        self.recovered_select(selected);
    }

    /// Lists the copies again, keeping the selection where it can.
    pub(super) fn recovered_refresh(&mut self) {
        if self.recovered.is_some() {
            self.recovered_open();
        }
    }

    /// Chooses a row and previews its copy.
    pub(super) fn recovered_select(&mut self, i: usize) {
        let Some(s) = &mut self.recovered else {
            return;
        };
        s.selected = i.min(s.rows.len().saturating_sub(1));
        s.preview.clear();
        s.preview_note = None;
        let Some(row) = s.rows.get(s.selected) else {
            return;
        };
        if !row.readable {
            s.preview_note = Some("This copy can't be read, so it can't be restored.".into());
            return;
        }
        match recovered::read_counted(&row.copy) {
            Ok(r) => {
                let doc = r.doc;
                s.preview = doc
                    .blocks
                    .iter()
                    .take(PREVIEW_BLOCKS)
                    .map(|b| (b.element.kind, b.element.text.plain_text()))
                    .collect();
                let mut notes = Vec::new();
                // A copy this version can replay only in part restores with
                // less than it holds (docs/specs/journal.md, "Recovered
                // versions", 2026-10-28).
                if r.problem.is_some() {
                    notes.push(format!(
                        "This copy restores {} of its {} changes; the rest can't be brought back.",
                        r.replayed, r.entries
                    ));
                }
                if doc.blocks.len() > PREVIEW_BLOCKS {
                    notes.push(format!(
                        "The first {PREVIEW_BLOCKS} of {} paragraphs.",
                        doc.blocks.len()
                    ));
                }
                if !notes.is_empty() {
                    s.preview_note = Some(notes.join(" "));
                }
            }
            Err(_) => s.preview_note = Some("This copy can't be read.".into()),
        }
    }

    /// Restore: the selected copy opens as a new untitled script, after
    /// the question about the open script's changes.
    pub(super) fn recovered_restore(&mut self) -> iced::Task<Message> {
        let Some(row) = self
            .recovered
            .as_ref()
            .and_then(|s| s.rows.get(s.selected))
            .filter(|r| r.readable)
            .cloned()
        else {
            return iced::Task::none();
        };
        let Some(path) = self.restore_path(&row.copy) else {
            self.banners
                .push("Restoring needs the app's data folder, which is not available.".into());
            return iced::Task::none();
        };
        let cmd = coldopen_core::Command::from_parts(
            "recovered.restore",
            json!({ "copy": row.copy.copy, "path": path }),
        );
        match cmd {
            Ok(cmd) => {
                self.recovered = None;
                self.dispatch(Request::Command(cmd), Origin::Button).task
            }
            Err(e) => {
                self.banners.push(e.into());
                iced::Task::none()
            }
        }
    }

    /// Delete: asks first.
    pub(super) fn recovered_delete(&mut self) -> iced::Task<Message> {
        let Some(copy) = self
            .recovered
            .as_ref()
            .and_then(|s| s.rows.get(s.selected))
            .map(|r| r.copy.copy.clone())
        else {
            return iced::Task::none();
        };
        match coldopen_core::Command::from_parts("recovered.delete", json!({ "copy": copy })) {
            Ok(cmd) => self.dispatch(Request::Command(cmd), Origin::Button).task,
            Err(e) => {
                self.banners.push(e.into());
                iced::Task::none()
            }
        }
    }

    /// Where a restored copy goes: `Unsaved/<script> (restored).<ext>`,
    /// numbered when that is taken.
    pub(super) fn restore_path(&self, copy: &Copy) -> Option<PathBuf> {
        crate::files::copy_path(self.dirs.as_ref()?, &copy.script, "restored")
    }

    /// The start-up clean-up (journal.md, "Recovered versions"): copies
    /// made more than 30 days ago that Recovered versions has shown are
    /// deleted; a copy never shown is never deleted. Marks for copies that
    /// are gone are dropped.
    pub(super) fn clean_up_copies(&mut self) {
        let Some(dirs) = self.dirs.clone() else {
            return;
        };
        let mut offered = crate::files::load_offered(&dirs);
        if offered.is_empty() {
            return;
        }
        let copies = crate::files::known_copies(&dirs, &self.settings.recent);
        for c in recovered::expired(&copies, &offered, now_ms()) {
            let cmd =
                coldopen_core::Command::from_parts("recovered.delete", json!({ "copy": c.copy }));
            if let Ok(cmd) = cmd {
                let _ = self.d.execute(cmd);
            }
        }
        let before = offered.len();
        offered.retain(|p, _| Path::new(p).exists());
        if offered.len() != before {
            let _ = crate::files::save_offered(&dirs, &offered);
        }
    }

    /// The repair dialog.
    pub(super) fn repair_view(&self, pal: Palette, r: &RepairReport) -> Element<'_, Message> {
        let counts = format!(
            "{} {} fixed, {} {} that could not be read left out.",
            r.fixed,
            if r.fixed == 1 { "problem" } else { "problems" },
            r.dropped,
            if r.dropped == 1 { "part" } else { "parts" },
        );
        let (heading, said) = match &r.written {
            None => (
                "Cold Open repaired this file".to_string(),
                format!(
                    "{} was damaged. {counts} Nothing has been written yet; the first save \
                     keeps a copy of the file as it was.",
                    r.file
                ),
            ),
            Some(out) if r.fixed + r.dropped == 0 => (
                "Nothing needed repairing".to_string(),
                format!(
                    "{} reads cleanly. The copy {out} was written anyway.",
                    r.file
                ),
            ),
            Some(out) => (
                "Repaired copy written".to_string(),
                format!(
                    "{counts} The repaired copy is {out}; {} is unchanged.",
                    r.file
                ),
            ),
        };
        let mut col = column![title(pal, heading), body(pal, said)].spacing(10);
        let shown = if r.details {
            r.issues.len()
        } else {
            r.issues.len().min(FIRST_ISSUES)
        };
        if shown > 0 {
            let list = r.issues[..shown].join("\n");
            let list = text(list).font(fonts::UI).size(12).color(pal.text);
            // A long list scrolls; a short one takes only its own room.
            col = col.push(if shown > SCROLL_AFTER {
                Element::from(
                    scrollable(container(list).padding(iced::Padding::ZERO.right(14)))
                        .height(Length::Fixed(260.0)),
                )
            } else {
                Element::from(list)
            });
        }
        if !r.details && r.issues.len() > FIRST_ISSUES {
            col = col.push(hint(
                pal,
                format!("and {} more", r.issues.len() - FIRST_ISSUES),
            ));
        }
        let details = if r.details {
            "Fewer details"
        } else {
            "Details"
        };
        let mut buttons = row![dialog_button("OK", Message::RepairOk, true)].spacing(8);
        if !r.issues.is_empty() {
            buttons = buttons.push(dialog_button(details, Message::RepairDetails, false));
        }
        let mut keys = String::from("Enter: OK");
        if !r.issues.is_empty() {
            keys.push_str(if r.details {
                "  ·  D: Fewer details"
            } else {
                "  ·  D: Details"
            });
        }
        if r.written.is_none() {
            buttons = buttons.push(dialog_button(
                "Save repaired copy…",
                Message::RepairSaveCopy,
                false,
            ));
            keys.push_str("  ·  S: Save repaired copy");
        }
        col.push(buttons).push(hint(pal, keys)).into()
    }

    /// The Recovered versions screen.
    pub(super) fn recovered_view(&self, pal: Palette, s: &RecoveredScreen) -> Element<'_, Message> {
        let mut col = column![title(pal, "Recovered versions")].spacing(10);
        if s.rows.is_empty() {
            col = col.push(body(
                pal,
                "Nothing has been set aside. When Cold Open keeps a copy of unsaved \
                 changes, or of a file it repaired, it is listed here.",
            ));
            return col
                .push(dialog_button("Close", Message::RecoveredClose, true))
                .push(hint(pal, "Esc: Close"))
                .into();
        }
        let mut list = column![].spacing(2);
        for (i, r) in s.rows.iter().enumerate() {
            let chosen = i == s.selected;
            let mut lines = column![
                text(file_name(&r.copy.script))
                    .font(fonts::UI_BOLD)
                    .size(13)
                    .color(pal.text),
                text(format!(
                    "{} · {}",
                    kind_words(r.copy.kind),
                    super::file_ops::ago(r.copy.at)
                ))
                .font(fonts::UI)
                .size(12)
                .color(pal.text_quiet),
            ];
            if r.expires {
                lines = lines.push(
                    text("Removed at next start: older than 30 days")
                        .font(fonts::UI)
                        .size(12)
                        .color(pal.text_quiet),
                );
            }
            list = list.push(
                button(lines)
                    .on_press(Message::RecoveredSelect(i))
                    .width(Fill)
                    .padding([4, 8])
                    .style(move |theme, status| {
                        let mut st = button::text(theme, status);
                        if chosen {
                            st.background = Some(Background::Color(pal.accent_soft));
                            st.border = Border {
                                color: pal.focus,
                                width: 1.0,
                                radius: 4.0.into(),
                            };
                        }
                        st
                    }),
            );
        }
        let mut preview = column![].spacing(4);
        for (kind, line) in &s.preview {
            let indent = match kind {
                ElementKind::Character => 120.0,
                ElementKind::Dialogue | ElementKind::Lyric => 60.0,
                ElementKind::Parenthetical => 90.0,
                ElementKind::Transition => 180.0,
                _ => 0.0,
            };
            let line = if matches!(kind, ElementKind::SceneHeading | ElementKind::Character) {
                line.to_uppercase()
            } else {
                line.clone()
            };
            preview = preview.push(
                container(text(line).font(fonts::PAGE).size(12).color(pal.text))
                    .padding(iced::Padding::ZERO.left(indent)),
            );
        }
        if let Some(note) = &s.preview_note {
            preview = preview.push(hint(pal, note.clone()));
        }
        let panes = row![
            scrollable(list)
                .width(Length::Fixed(280.0))
                .height(Length::Fixed(380.0)),
            container(
                scrollable(container(preview).padding(iced::Padding::ZERO.right(16)))
                    .height(Length::Fixed(380.0))
            )
            .padding(10)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(pal.page)),
                border: Border {
                    color: pal.border,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..container::Style::default()
            }),
        ]
        .spacing(12);
        let readable = s.rows.get(s.selected).is_some_and(|r| r.readable);
        let mut restore = dialog_button("Restore", Message::RecoveredRestore, true);
        if !readable {
            restore = restore.on_press_maybe(None);
        }
        col.push(body(
            pal,
            "Copies Cold Open set aside instead of deleting. Restore opens one as a new \
             untitled script; nothing is overwritten.",
        ))
        .push(panes)
        .push(
            row![
                restore,
                dialog_button("Delete", Message::RecoveredDelete, false),
                dialog_button("Close", Message::RecoveredClose, false),
            ]
            .spacing(8),
        )
        .push(hint(
            pal,
            "Up/Down: Choose  ·  Enter or R: Restore  ·  Delete: Delete  ·  Esc: Close",
        ))
        .into()
    }

    /// Restore from the router: the copy opens at `path` once the open
    /// script is left.
    pub(super) fn leave_for_restore(
        &mut self,
        copy: PathBuf,
        path: PathBuf,
    ) -> iced::Task<Message> {
        self.leave(After::Restore { copy, path })
    }
}
