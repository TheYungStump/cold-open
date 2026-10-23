//! Production revision sets in the app (editor.md §18): starting, ending,
//! recoloring and clearing sets, each through the dispatcher.

use iced::Element;
use iced::Task;
use iced::widget::{column, row, text_input};
use serde_json::json;

use super::view::{body, dialog_button, hint, title};
use super::{App, Message, Prompt};
use crate::fonts;
use crate::page_view::KeyPress;
use crate::theme::Palette;

/// The start form's color field, which has the keyboard when it opens.
pub const REVISION_COLOR: &str = "revision-color";

/// What the start form holds (editor.md §18).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartForm {
    pub color: String,
    pub label: String,
    /// The label was typed, so it no longer follows the color.
    pub label_edited: bool,
    pub date: String,
}

impl StartForm {
    fn default_label(color: &str) -> String {
        format!("{} Rev.", color.trim())
    }
}

/// The input of one of the revision questions.
#[derive(Clone, Debug)]
pub enum RevisionInput {
    Color(String),
    Label(String),
    Date(String),
    Colors(String),
    Submit,
}

impl App {
    /// "Start revision…": the form, or first the question about the set
    /// that is active.
    pub(super) fn revision_start_ask(&mut self) -> Task<Message> {
        let Some(s) = self.d.session() else {
            return Task::none();
        };
        if let Some(active) = s.doc.revisions.active_set() {
            self.prompt = Some(Prompt::RevisionEndFirst {
                active: coldopen_core::revisions::header_text(active),
            });
            return Task::none();
        }
        let color = s.doc.revisions.next_color();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        self.prompt = Some(Prompt::RevisionStart(StartForm {
            label: StartForm::default_label(&color),
            color,
            label_edited: false,
            date: coldopen_core::revisions::utc_date(now),
        }));
        Task::batch([
            iced::widget::operation::focus(REVISION_COLOR),
            iced::widget::operation::select_all(REVISION_COLOR),
        ])
    }

    /// "End revision".
    pub(super) fn revision_end(&mut self) {
        if let Some(r) = self.run("revision.end", json!({})) {
            let label = r["set"]["label"].as_str().unwrap_or("The set");
            self.banners
                .push(format!("{label} has ended. Its marks stay.").into());
        }
    }

    /// "Revision colors…": the list as one line, names between commas.
    pub(super) fn revision_colors_ask(&mut self) -> Task<Message> {
        let Some(s) = self.d.session() else {
            return Task::none();
        };
        self.prompt = Some(Prompt::RevisionColors(s.doc.revisions.colors.join(", ")));
        Task::batch([
            iced::widget::operation::focus(REVISION_COLOR),
            iced::widget::operation::move_cursor_to_end(REVISION_COLOR),
        ])
    }

    /// "Clear revision marks…": which set, or all of them.
    pub(super) fn revision_clear_ask(&mut self) {
        let sets = self.d.session().map_or(0, |s| s.doc.revisions.sets.len());
        if sets == 0 {
            self.banners
                .push("This script has no revision sets.".into());
            return;
        }
        self.prompt = Some(Prompt::RevisionClear { choice: 0 });
    }

    /// Typing and Enter in the revision questions.
    pub(super) fn revision_input(&mut self, input: RevisionInput) -> Task<Message> {
        match (&mut self.prompt, input) {
            (Some(Prompt::RevisionStart(f)), RevisionInput::Color(c)) => {
                if !f.label_edited {
                    f.label = StartForm::default_label(&c);
                }
                f.color = c;
            }
            (Some(Prompt::RevisionStart(f)), RevisionInput::Label(l)) => {
                f.label_edited = true;
                f.label = l;
            }
            (Some(Prompt::RevisionStart(f)), RevisionInput::Date(d)) => f.date = d,
            (Some(Prompt::RevisionColors(t)), RevisionInput::Colors(c)) => *t = c,
            (_, RevisionInput::Submit) => return self.revision_submit(),
            _ => {}
        }
        Task::none()
    }

    /// Enter in a revision question.
    fn revision_submit(&mut self) -> Task<Message> {
        match self.prompt.take() {
            Some(Prompt::RevisionStart(f)) => {
                let params = json!({
                    "color": f.color.trim(),
                    "label": f.label.trim(),
                    "date": f.date.trim(),
                });
                if self.run("revision.start", params).is_none() {
                    // The banner says what was wrong; the form stays.
                    self.prompt = Some(Prompt::RevisionStart(f));
                    return Task::none();
                }
                self.after_change(false)
            }
            Some(Prompt::RevisionEndFirst { .. }) => {
                self.revision_end();
                self.revision_start_ask()
            }
            Some(Prompt::RevisionColors(t)) => {
                let colors: Vec<String> = t
                    .split(',')
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty())
                    .collect();
                if self
                    .run("revision.colors", json!({ "colors": colors }))
                    .is_none()
                {
                    self.prompt = Some(Prompt::RevisionColors(t));
                }
                Task::none()
            }
            Some(Prompt::RevisionClear { choice }) => {
                let params = if choice == 0 {
                    json!({ "all": true })
                } else {
                    json!({ "set": format!("r_{choice}") })
                };
                if let Some(r) = self.run("revision.clear", params) {
                    let n = r["count"].as_u64().unwrap_or(0);
                    self.banners.push(
                        format!(
                            "Cleared the revision marks from {n} {}.",
                            if n == 1 { "block" } else { "blocks" }
                        )
                        .into(),
                    );
                }
                self.after_change(false)
            }
            other => {
                self.prompt = other;
                Task::none()
            }
        }
    }

    /// Keys while a revision question is open: Enter goes on, Esc closes,
    /// and Up and Down step through colors or sets. `None` when no
    /// revision question is open.
    pub(super) fn revision_key(
        &mut self,
        kp: &KeyPress,
        enter: bool,
        esc: bool,
    ) -> Option<Task<Message>> {
        use iced::keyboard::{Key, key::Named};
        let up = matches!(kp.key.as_ref(), Key::Named(Named::ArrowUp));
        let down = matches!(kp.key.as_ref(), Key::Named(Named::ArrowDown));
        let colors = self
            .d
            .session()
            .map(|s| s.doc.revisions.colors.clone())
            .unwrap_or_default();
        let sets = self.d.session().map_or(0, |s| s.doc.revisions.sets.len());
        match &mut self.prompt {
            Some(Prompt::RevisionStart(f)) => {
                if esc {
                    self.prompt = None;
                } else if enter {
                    return Some(self.revision_submit());
                } else if (up || down) && !colors.is_empty() {
                    let at = colors
                        .iter()
                        .position(|c| c.eq_ignore_ascii_case(f.color.trim()))
                        .unwrap_or(0);
                    let n = colors.len();
                    let next = if down { (at + 1) % n } else { (at + n - 1) % n };
                    if !f.label_edited {
                        f.label = StartForm::default_label(&colors[next]);
                    }
                    f.color = colors[next].clone();
                }
                Some(Task::none())
            }
            Some(Prompt::RevisionEndFirst { .. } | Prompt::RevisionColors(_)) => {
                if esc {
                    self.prompt = None;
                    Some(Task::none())
                } else if enter {
                    Some(self.revision_submit())
                } else {
                    Some(Task::none())
                }
            }
            Some(Prompt::RevisionClear { choice }) => {
                if esc {
                    self.prompt = None;
                } else if enter {
                    return Some(self.revision_submit());
                } else if up {
                    *choice = choice.saturating_sub(1);
                } else if down {
                    *choice = (*choice + 1).min(sets);
                }
                Some(Task::none())
            }
            _ => None,
        }
    }

    /// The revision questions, as drawn.
    pub(super) fn revision_prompt_view(
        &self,
        pal: Palette,
        p: &Prompt,
    ) -> Option<Element<'_, Message>> {
        Some(match p {
            Prompt::RevisionStart(f) => column![
                title(pal, "Start a revision set"),
                body(
                    pal,
                    "Every change to what prints is marked with this set until you end it. Revised pages print its label and date at the top.",
                ),
                column![
                    hint(pal, "Color"),
                    text_input("Color", &f.color)
                        .id(REVISION_COLOR)
                        .on_input(|c| Message::Revision(RevisionInput::Color(c)))
                        .on_submit(Message::Revision(RevisionInput::Submit))
                        .font(fonts::UI)
                        .size(14),
                ]
                .spacing(4),
                column![
                    hint(pal, "Label, printed before the date on revised pages"),
                    text_input("Label", &f.label)
                        .on_input(|l| Message::Revision(RevisionInput::Label(l)))
                        .on_submit(Message::Revision(RevisionInput::Submit))
                        .font(fonts::UI)
                        .size(14),
                ]
                .spacing(4),
                column![
                    hint(pal, "Date (year-month-day)"),
                    text_input("YYYY-MM-DD", &f.date)
                        .on_input(|d| Message::Revision(RevisionInput::Date(d)))
                        .on_submit(Message::Revision(RevisionInput::Submit))
                        .font(fonts::UI)
                        .size(14),
                ]
                .spacing(4),
                row![
                    dialog_button("Start", Message::Revision(RevisionInput::Submit), true),
                    dialog_button("Cancel", Message::Escaped, false),
                ]
                .spacing(8),
                hint(pal, "Up and Down: the next or previous color  ·  Enter: start  ·  Esc: cancel"),
            ]
            .spacing(12)
            .into(),
            Prompt::RevisionEndFirst { active } => column![
                title(pal, format!("End {active} first?")),
                body(pal, "One revision set is active at a time. Its marks stay when it ends."),
                row![
                    dialog_button("End it and go on", Message::Revision(RevisionInput::Submit), true),
                    dialog_button("Cancel", Message::Escaped, false),
                ]
                .spacing(8),
                hint(pal, "Enter: end it and go on  ·  Esc: cancel"),
            ]
            .spacing(12)
            .into(),
            Prompt::RevisionColors(t) => column![
                title(pal, "Revision colors"),
                body(
                    pal,
                    "The colors new sets take, in order, between commas. The first names the draft before any revision. Sets already made keep their colors.",
                ),
                text_input("White, Blue, Pink, …", t)
                    .id(REVISION_COLOR)
                    .on_input(|c| Message::Revision(RevisionInput::Colors(c)))
                    .on_submit(Message::Revision(RevisionInput::Submit))
                    .font(fonts::UI)
                    .size(14),
                row![
                    dialog_button("Save the colors", Message::Revision(RevisionInput::Submit), true),
                    dialog_button("Cancel", Message::Escaped, false),
                ]
                .spacing(8),
                hint(pal, "Enter: save the colors  ·  Esc: cancel"),
            ]
            .spacing(12)
            .into(),
            Prompt::RevisionClear { choice } => {
                let sets: Vec<String> = self.d.session().map_or_else(Vec::new, |s| {
                    s.doc
                        .revisions
                        .sets
                        .iter()
                        .map(coldopen_core::revisions::header_text)
                        .collect()
                });
                let what = match *choice {
                    0 => "every revision set".to_string(),
                    n => sets.get(n - 1).cloned().unwrap_or_default(),
                };
                column![
                    title(pal, format!("Clear the marks of {what}?")),
                    body(
                        pal,
                        "The asterisks and colors go; the sets stay in the list, and Mod+Z gives the marks back."
                            .replace("Mod", if crate::keymap::MAC { "Cmd" } else { "Ctrl" }),
                    ),
                    row![
                        dialog_button("Clear the marks", Message::Revision(RevisionInput::Submit), true),
                        dialog_button("Cancel", Message::Escaped, false),
                    ]
                    .spacing(8),
                    hint(pal, "Up and Down: choose the set, or all  ·  Enter: clear  ·  Esc: cancel"),
                ]
                .spacing(12)
                .into()
            }
            _ => return None,
        })
    }
}
