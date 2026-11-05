//! Report a problem, the crash notice and the crash report sheet
//! (docs/specs/editor.md §23; docs/specs/crash-reports.md §6 to §9).
//! Every path ends in the browser or the clipboard: nothing is sent.

use std::path::PathBuf;

use coldopen_core::crash::Report;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length, Task};

use super::view::{body, dialog_button, hint, title};
use super::{App, Banner, Message, Prompt, Sheet};
use crate::crash;
use crate::feedback::{self, Details};
use crate::files::CrashReports;
use crate::fonts;
use crate::page_view::KeyPress;
use crate::theme::Palette;

/// The kept reports, newest first, and the one the sheet shows.
#[derive(Clone, Debug, Default)]
pub struct CrashSheet {
    pub reports: Vec<PathBuf>,
    pub index: usize,
    /// The shown report's text, exactly as it is on disk.
    pub text: String,
}

/// The buttons of both sheets and the crash notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedbackInput {
    /// Report a problem: a GitHub issue, the form, or the details copied.
    Issue,
    Form,
    CopyDetails,
    /// The crash report sheet.
    SendGithub,
    SendForm,
    CopyReport,
    Delete,
    Older,
    Newer,
    /// The crash notice's "Never ask again".
    Never,
    /// Either sheet's Close.
    Close,
}

impl App {
    /// At start: when reports are on and one hasn't been offered, the
    /// crash notice, once; every unseen report is then marked seen (§6).
    pub(super) fn offer_crash_report(&mut self) {
        if self.settings.crash_reports == CrashReports::Off {
            return;
        }
        let Some(dirs) = &self.dirs else {
            return;
        };
        let unseen: Vec<PathBuf> = crash::kept(&crash::folder(dirs))
            .into_iter()
            .filter(|p| !crash::seen(p))
            .collect();
        if unseen.is_empty() {
            return;
        }
        for p in &unseen {
            crash::mark_seen(p);
        }
        self.banners.push(Banner::crash());
    }

    /// "Look at crash reports": the sheet on the newest kept report.
    pub(super) fn crash_reports_open(&mut self) {
        let reports = self
            .dirs
            .as_ref()
            .map(|d| crash::kept(&crash::folder(d)))
            .unwrap_or_default();
        if reports.is_empty() {
            self.banners.push("No crash reports are kept.".into());
            return;
        }
        self.crash_sheet = CrashSheet {
            reports,
            index: 0,
            text: String::new(),
        };
        self.crash_load();
        self.banners.retain(|b| !b.crash);
        self.sheet = Some(Sheet::CrashReport);
    }

    fn crash_load(&mut self) {
        let s = &mut self.crash_sheet;
        s.index = s.index.min(s.reports.len().saturating_sub(1));
        s.text = s
            .reports
            .get(s.index)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_else(|| "This report can't be read.".into());
    }

    fn crash_shown(&self) -> Option<Report> {
        Report::parse(&self.crash_sheet.text)
    }

    /// "Delete this report?" answered yes.
    pub(super) fn crash_delete(&mut self, path: &std::path::Path) {
        if let Err(e) = std::fs::remove_file(path) {
            self.banners
                .push(format!("The report couldn't be deleted: {e}").into());
        }
        self.crash_sheet.reports.retain(|p| p != path);
        if self.crash_sheet.reports.is_empty() {
            self.sheet = None;
            self.banners.push("The crash report was deleted.".into());
        } else {
            self.crash_load();
        }
    }

    /// Opens an address, or, in tests, notes it.
    pub(super) fn open_address(&mut self, url: String) {
        if let Some(seen) = &mut self.opened {
            seen.push(url);
        } else if let Err(e) = crate::files::open_link(&url) {
            self.banners
                .push(format!("Your browser couldn't be opened: {e}").into());
        }
    }

    pub(super) fn feedback(&mut self, input: FeedbackInput) -> Task<Message> {
        match input {
            FeedbackInput::Issue => self.problem_send(false),
            FeedbackInput::Form => self.problem_send(true),
            FeedbackInput::CopyDetails => {
                self.banners.push("The details were copied.".into());
                iced::clipboard::write(self.problem_copy_text())
            }
            FeedbackInput::SendGithub | FeedbackInput::SendForm => {
                let Some(r) = self.crash_shown() else {
                    return Task::none();
                };
                let url = if input == FeedbackInput::SendGithub {
                    Some(feedback::crash_issue(&r))
                } else {
                    feedback::crash_form(&r)
                };
                if let Some(url) = url {
                    self.open_address(url);
                }
                self.banners.push(
                    "The whole report is on your clipboard: paste it into the page, then send it from there."
                        .into(),
                );
                iced::clipboard::write(self.crash_sheet.text.clone())
            }
            FeedbackInput::CopyReport => {
                self.banners.push("The crash report was copied.".into());
                iced::clipboard::write(self.crash_sheet.text.clone())
            }
            FeedbackInput::Delete => {
                if let Some(p) = self.crash_sheet.reports.get(self.crash_sheet.index) {
                    self.prompt = Some(Prompt::DeleteCrashReport(p.clone()));
                }
                Task::none()
            }
            FeedbackInput::Older | FeedbackInput::Newer => {
                let s = &mut self.crash_sheet;
                s.index = if input == FeedbackInput::Older {
                    (s.index + 1).min(s.reports.len().saturating_sub(1))
                } else {
                    s.index.saturating_sub(1)
                };
                self.crash_load();
                Task::none()
            }
            FeedbackInput::Close => {
                self.sheet = None;
                Task::none()
            }
            FeedbackInput::Never => {
                self.set_crash_reports(CrashReports::Off);
                self.banners.retain(|b| !b.crash);
                self.banners.push(
                    "Crash reports are off. \"Turn crash reports on or off\" in the palette turns them back on."
                        .into(),
                );
                Task::none()
            }
        }
    }

    pub(super) fn set_crash_reports(&mut self, to: CrashReports) {
        self.settings.crash_reports = to;
        crash::set_on(to == CrashReports::Ask);
        self.save_settings();
    }

    /// "Turn crash reports on or off".
    pub(super) fn crash_reports_toggle(&mut self) {
        let to = if self.settings.crash_reports == CrashReports::Off {
            CrashReports::Ask
        } else {
            CrashReports::Off
        };
        self.set_crash_reports(to);
        self.banners.push(if to == CrashReports::Ask {
            "Crash reports are on: after a crash, Cold Open saves a report with no script text and asks at the next start.".into()
        } else {
            "Crash reports are off: a crash writes no report.".into()
        });
    }

    /// "Crash on purpose", answered Crash now (§9).
    pub(super) fn crash_now(&mut self) -> Task<Message> {
        if cfg!(debug_assertions)
            && std::env::var("COLDOPEN_TEST_CRASH").is_ok_and(|v| v == "worker")
        {
            crash::crash_a_worker();
            return Task::none();
        }
        let text = self.d.session().and_then(|s| {
            s.doc
                .get(self.editor.caret.block)
                .map(|b| b.element.text.plain_text())
        });
        crash::crash_on_purpose(text.as_deref())
    }

    /// Keys while one of the two sheets is open (§23).
    pub(super) fn feedback_key(&mut self, kp: &KeyPress, esc: bool) -> Option<Task<Message>> {
        use iced::keyboard::{Key, key::Named};
        let sheet = self.sheet?;
        if !matches!(sheet, Sheet::ReportProblem | Sheet::CrashReport) {
            return None;
        }
        if esc {
            self.sheet = None;
            return Some(Task::none());
        }
        let ch = match kp.key.as_ref() {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        let form = feedback::FORM_URL.is_some();
        let input = match (sheet, ch.as_str(), kp.key.as_ref()) {
            (Sheet::ReportProblem, "g", _) => FeedbackInput::Issue,
            (Sheet::ReportProblem, "f", _) if form => FeedbackInput::Form,
            (Sheet::ReportProblem, "c", _) => FeedbackInput::CopyDetails,
            (Sheet::ReportProblem, "u", _) => {
                return Some(self.share_input(super::ShareInput::ProblemInclude));
            }
            (Sheet::CrashReport, "g", _) => FeedbackInput::SendGithub,
            (Sheet::CrashReport, "f", _) if form => FeedbackInput::SendForm,
            (Sheet::CrashReport, "c", _) => FeedbackInput::CopyReport,
            (Sheet::CrashReport, _, Key::Named(Named::Delete)) => FeedbackInput::Delete,
            (Sheet::CrashReport, _, Key::Named(Named::ArrowLeft)) => FeedbackInput::Older,
            (Sheet::CrashReport, _, Key::Named(Named::ArrowRight)) => FeedbackInput::Newer,
            _ => return Some(Task::none()),
        };
        Some(self.feedback(input))
    }

    pub(super) fn report_problem_view(&self, pal: Palette) -> Element<'_, Message> {
        let d = Details::now(self.renderer_label());
        let mut buttons = row![dialog_button(
            "Open a GitHub issue",
            Message::Feedback(FeedbackInput::Issue),
            true
        )]
        .spacing(8);
        let mut keys = String::from("G: GitHub issue (needs an account)");
        if feedback::FORM_URL.is_some() {
            buttons = buttons.push(dialog_button(
                "Use the form (no account)",
                Message::Feedback(FeedbackInput::Form),
                false,
            ));
            keys.push_str("  ·  F: the form (no account needed)");
        }
        buttons = buttons
            .push(dialog_button(
                "Copy the details",
                Message::Feedback(FeedbackInput::CopyDetails),
                false,
            ))
            .push(dialog_button(
                "Close",
                Message::Feedback(FeedbackInput::Close),
                false,
            ));
        column![
            title(pal, "Report a problem"),
            body(
                pal,
                "Cold Open opens a page in your browser with these details filled in. \
                 Nothing is sent until you submit it there."
            ),
            container(text(d.lines()).font(fonts::PAGE).size(12).color(pal.text)).padding(8),
            hint(
                pal,
                "No script text, no file names or paths, and no user or computer names."
            ),
            self.summary_tick(
                pal,
                self.share.problem_include,
                Message::Share(super::ShareInput::ProblemInclude)
            ),
            // On a second line rather than past the card, if it ever
            // runs out of room.
            buttons.wrap().vertical_spacing(8),
            hint(
                pal,
                format!("{keys}  ·  C: copy  ·  U: include the usage summary  ·  Esc: close")
            ),
        ]
        .spacing(10)
        .into()
    }

    pub(super) fn crash_view(&self, pal: Palette) -> Element<'_, Message> {
        let s = &self.crash_sheet;
        let when = self
            .crash_shown()
            .map(|r| r.time)
            .filter(|t| !t.is_empty())
            .map_or_else(|| "an earlier day".into(), |t| date_of(&t));
        let mut buttons = row![dialog_button(
            "Send on GitHub",
            Message::Feedback(FeedbackInput::SendGithub),
            true
        )]
        .spacing(8);
        let mut keys = String::from("G: send on GitHub");
        if feedback::FORM_URL.is_some() {
            buttons = buttons.push(dialog_button(
                "Send with the form",
                Message::Feedback(FeedbackInput::SendForm),
                false,
            ));
            keys.push_str("  ·  F: the form (no account needed)");
        }
        buttons = buttons
            .push(dialog_button(
                "Copy",
                Message::Feedback(FeedbackInput::CopyReport),
                false,
            ))
            .push(dialog_button(
                "Delete",
                Message::Feedback(FeedbackInput::Delete),
                false,
            ))
            .push(dialog_button(
                "Close",
                Message::Feedback(FeedbackInput::Close),
                false,
            ));
        let mut nav = row![].spacing(8);
        if s.reports.len() > 1 {
            nav = nav
                .push(dialog_button(
                    "Older",
                    Message::Feedback(FeedbackInput::Older),
                    false,
                ))
                .push(dialog_button(
                    "Newer",
                    Message::Feedback(FeedbackInput::Newer),
                    false,
                ))
                .push(hint(
                    pal,
                    format!(
                        "Report {} of {}, newest first",
                        s.index + 1,
                        s.reports.len()
                    ),
                ));
            keys.push_str("  ·  Left, Right: older, newer");
        }
        column![
            title(pal, "Crash report"),
            body(
                pal,
                format!(
                    "Cold Open crashed on {when}. This report holds no script text, file \
                     names or user names. Read it before you send it."
                )
            ),
            scrollable(
                container(
                    text(s.text.as_str())
                        .font(fonts::PAGE)
                        .size(11)
                        .color(pal.text)
                )
                .padding(6)
            )
            .width(Length::Fill)
            .height(Length::Fixed(300.0)),
            nav,
            buttons.wrap().vertical_spacing(8),
            hint(
                pal,
                "Sending copies the whole report and opens a page to paste it into. Nothing is \
                 sent until you submit that page."
            ),
            hint(
                pal,
                format!("{keys}  ·  C: copy  ·  Delete: delete  ·  Esc: close")
            ),
        ]
        .spacing(10)
        .into()
    }
}

/// A report's UTC time (`2026-11-03T12:34:56Z`) as its date, in UTC
/// (crash-reports.md §7).
fn date_of(utc: &str) -> String {
    utc.split('T').next().unwrap_or(utc).to_string()
}
