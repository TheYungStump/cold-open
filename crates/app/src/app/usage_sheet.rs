//! The local usage summary (docs/specs/usage-summary.md), Share feedback…
//! (editor.md §23) and the Settings sheet (editor.md §24). Nothing here
//! sends anything: feedback ends in the browser or the clipboard.

use std::time::Instant;

use iced::widget::{Space, button, column, container, row, scrollable, text, text_editor};
use iced::{Border, Element, Length, Task};

use super::view::{body, dialog_button, hint, title};
use super::{App, Message, Sheet};
use crate::feedback::{self, Address, Details};
use crate::files::{CrashReports, INTERFACE_SIZES, Renderer};
use crate::fonts;
use crate::page_view::KeyPress;
use crate::theme::{Choice, Palette};
use crate::usage::{self, Counter, Read};

/// The text box's id, so the sheet can give it the keyboard.
pub(super) const SHARE_TEXT: &str = "share-text";

/// The usage summary's state in the app (usage-summary.md §5, §6).
#[derive(Debug)]
pub struct UsageState {
    pub counter: Counter,
    /// Whether this run may count at all: the real app, not a test or a
    /// bench (`Boot::count_usage`).
    pub allowed: bool,
    pub started: Instant,
    pub last_write: Instant,
    /// The View sheet's text, made when it opens.
    pub view: String,
    /// The View sheet asked "Reset?".
    pub confirm_reset: bool,
}

impl Default for UsageState {
    fn default() -> UsageState {
        UsageState {
            counter: Counter::default(),
            allowed: false,
            started: Instant::now(),
            last_write: Instant::now(),
            view: String::new(),
            confirm_reset: false,
        }
    }
}

/// Share feedback's sheet, kept while the app runs, so closing it by
/// mistake loses nothing (editor.md §23).
#[derive(Debug)]
pub struct ShareState {
    pub content: text_editor::Content,
    /// "Include my usage summary", in Share feedback.
    pub include: bool,
    /// The same tick in Report a problem.
    pub problem_include: bool,
    /// The text box has the keyboard.
    pub typing: bool,
    /// The summary's text when the sheet opened, or why there is none.
    pub summary: Result<String, &'static str>,
}

impl Default for ShareState {
    fn default() -> ShareState {
        ShareState {
            content: text_editor::Content::new(),
            include: false,
            problem_include: false,
            typing: false,
            summary: Err("No usage summary yet"),
        }
    }
}

/// The Settings sheet's selection (editor.md §24).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsState {
    pub row: usize,
    /// On the Usage summary row: its switch (0), View… (1) or Reset… (2).
    pub part: usize,
}

#[derive(Clone, Debug)]
pub enum UsageInput {
    Copy,
    Reset,
    ResetYes,
    ResetNo,
    Toggle,
    Close,
}

#[derive(Clone, Debug)]
pub enum ShareInput {
    Edit(text_editor::Action),
    Include,
    Form,
    Issue,
    CopyAll,
    /// The text box let go of the keyboard (Esc or Tab).
    LeftText,
    /// T: the text box takes the keyboard again.
    Focus,
    Close,
    /// Report a problem's tick.
    ProblemInclude,
}

#[derive(Clone, Debug)]
pub enum SettingsInput {
    /// A row's value, chosen with the mouse: the row and the value's index.
    Choose(usize, usize),
    /// The usage summary row's View… and Reset….
    UsageView,
    UsageReset,
    Close,
}

/// The rows, in order.
const ROWS: [&str; 7] = [
    "Theme",
    "Interface size",
    "Renderer",
    "Your name on tracked changes",
    "Agents can attach",
    "Crash reports",
    "Usage summary",
];
const THEMES: [(&str, Choice); 4] = [
    ("Follow the system", Choice::System),
    ("Light", Choice::Light),
    ("Dark", Choice::Dark),
    ("High contrast", Choice::HighContrast),
];
const RENDERERS: [(&str, Renderer); 3] = [
    ("Automatic", Renderer::Auto),
    ("CPU", Renderer::Cpu),
    ("GPU", Renderer::Gpu),
];

impl App {
    /// At start: counting on when this run may count and the setting
    /// allows, and the session counted (usage-summary.md §2.2).
    pub(super) fn usage_start(&mut self, allowed: bool) {
        self.usage.allowed = allowed;
        let on = allowed && self.settings.count_usage;
        self.usage.counter.set_on(on);
        self.d.set_command_hook(Some(self.usage.counter.hook()));
        self.usage.counter.count(|p| p.session());
        self.usage.started = Instant::now();
        self.usage.last_write = Instant::now();
    }

    fn usage_path(&self) -> Option<std::path::PathBuf> {
        self.dirs.as_ref().map(|d| usage::path(&d.data))
    }

    /// The timer: counts that have waited long enough go to the file, on
    /// a thread of their own (§5).
    pub(super) fn usage_tick(&mut self) {
        if !self.usage.counter.is_on()
            || self.usage.last_write.elapsed() < usage::WRITE_EVERY
            || !self.usage.counter.waiting()
        {
            return;
        }
        self.usage.last_write = Instant::now();
        let Some(path) = self.usage_path() else {
            return;
        };
        let pending = self.usage.counter.take();
        let _ = std::thread::Builder::new()
            .name("usage".into())
            .spawn(move || {
                let _ = usage::add_to_file_locked(&path, &pending, usage::app_version());
            });
    }

    /// Writes what's waiting now, on this thread: before the summary is
    /// shown, and at quit (§5).
    pub(super) fn usage_write_now(&mut self) {
        if !self.usage.counter.is_on() || !self.usage.counter.waiting() {
            return;
        }
        let Some(path) = self.usage_path() else {
            return;
        };
        let pending = self.usage.counter.take();
        self.usage.last_write = Instant::now();
        let _ = usage::add_to_file_locked(&path, &pending, usage::app_version());
    }

    /// At quit: the session's length and renderer, then the file (§5).
    pub(super) fn usage_quit(&mut self) {
        let kind = self.usage_renderer();
        let length = self.usage.started.elapsed();
        self.usage.counter.count(|p| p.session_end(length, kind));
        self.usage_write_now();
    }

    /// What drew the window, as far as the app knows without asking the
    /// system (§2.1).
    fn usage_renderer(&self) -> &'static str {
        match &self.graphics {
            Some((_, backend)) if backend == "tiny-skia" => {
                if self.settings.renderer == Renderer::Gpu {
                    "cpu_fallback"
                } else {
                    "cpu"
                }
            }
            Some(_) => "gpu",
            None => "not_asked",
        }
    }

    /// The summary in plain words, written up to now, or why there is
    /// none (§8).
    pub(super) fn usage_text(&mut self) -> Result<String, &'static str> {
        self.usage_write_now();
        let read = self.usage_path().map_or(Read::Missing, |p| usage::read(&p));
        match read {
            Read::Ok(s) if !s.is_empty() => Ok(s.text()),
            Read::Unreadable => Err("The usage summary file can't be read. Reset replaces it."),
            _ if !self.settings.count_usage => Err("No usage summary (counting is off)"),
            _ => Err("No usage summary yet"),
        }
    }

    /// A script opened, by its format (§2).
    pub(super) fn usage_opened(&mut self) {
        if let Some(f) = self.d.session().map(|s| usage::format_key(s.format)) {
            self.usage.counter.count(|p| p.opened(f));
        }
    }

    /// A save of the script finished, by its format (§2).
    pub(super) fn usage_saved(&mut self) {
        if let Some(f) = self.d.session().map(|s| usage::format_key(s.format)) {
            self.usage.counter.count(|p| p.saved(f));
        }
    }

    /// "View usage summary".
    pub(super) fn usage_open(&mut self) {
        self.palette = None;
        self.usage.view = match self.usage_text() {
            Ok(t) => t,
            Err(why) => format!("{why}.\n"),
        };
        self.usage.confirm_reset = false;
        self.sheet = Some(Sheet::Usage);
    }

    /// "Turn usage counting on or off" (§6).
    pub(super) fn usage_toggle(&mut self) {
        let to = !self.settings.count_usage;
        if !to {
            // What waited is dropped, not written: off means no writes.
            self.usage.counter.set_on(false);
        }
        self.settings.count_usage = to;
        self.save_settings();
        self.usage.counter.set_on(to && self.usage.allowed);
        self.banners.push(if to {
            "Usage counting is on: Cold Open counts which features you use, on this computer only, and never sends it.".into()
        } else {
            "Usage counting is off: nothing is counted or written.".into()
        });
        if self.sheet == Some(Sheet::Usage) {
            self.usage.view = match self.usage_text() {
                Ok(t) => t,
                Err(why) => format!("{why}.\n"),
            };
        }
    }

    fn usage_reset(&mut self) {
        let _ = self.usage.counter.take();
        let result = self.usage_path().map(|p| usage::reset(&p));
        self.usage.confirm_reset = false;
        if let Some(Err(e)) = result {
            self.banners
                .push(format!("The usage summary couldn't be reset: {e}").into());
            return;
        }
        self.banners.push("The usage summary was reset.".into());
        self.usage.view = match self.usage_text() {
            Ok(t) => t,
            Err(why) => format!("{why}.\n"),
        };
    }

    pub(super) fn usage_input(&mut self, input: UsageInput) -> Task<Message> {
        match input {
            UsageInput::Copy => {
                self.banners.push("The usage summary was copied.".into());
                return iced::clipboard::write(self.usage.view.clone());
            }
            UsageInput::Reset => self.usage.confirm_reset = true,
            UsageInput::ResetYes => self.usage_reset(),
            UsageInput::ResetNo => self.usage.confirm_reset = false,
            UsageInput::Toggle => self.usage_toggle(),
            UsageInput::Close => self.sheet = None,
        }
        Task::none()
    }

    // Share feedback… (editor.md §23).

    /// Opens the sheet with the text box taking the keyboard.
    pub(super) fn share_open(&mut self) -> Task<Message> {
        self.palette = None;
        self.share.include = false;
        self.share.summary = self.usage_text();
        self.share.typing = true;
        self.sheet = Some(Sheet::ShareFeedback);
        Task::batch([
            self.ask_graphics(),
            iced::widget::operation::focus(SHARE_TEXT),
        ])
    }

    /// The summary as it goes out, when ticked and there is one.
    fn ticked_summary(&self, ticked: bool) -> Option<String> {
        ticked.then(|| self.share.summary.clone().ok()).flatten()
    }

    /// Opens `a`, copying `all` first when it didn't fit.
    fn send(&mut self, a: Address, all: String) -> Task<Message> {
        self.open_address(a.url);
        if a.whole {
            return Task::none();
        }
        self.banners.push(
            "It didn't all fit in the page's address, so it's on your clipboard: paste it into the page, then send it from there."
                .into(),
        );
        iced::clipboard::write(all)
    }

    pub(super) fn share_input(&mut self, input: ShareInput) -> Task<Message> {
        match input {
            ShareInput::Edit(a) => {
                self.share.typing = true;
                self.share.content.perform(a);
                Task::none()
            }
            ShareInput::Include => {
                if self.share.summary.is_ok() {
                    self.share.include = !self.share.include;
                }
                Task::none()
            }
            ShareInput::ProblemInclude => {
                if self.share.summary.is_ok() {
                    self.share.problem_include = !self.share.problem_include;
                }
                Task::none()
            }
            ShareInput::LeftText => {
                self.share.typing = false;
                Task::none()
            }
            ShareInput::Focus => {
                self.share.typing = true;
                iced::widget::operation::focus(SHARE_TEXT)
            }
            ShareInput::Close => {
                self.sheet = None;
                Task::none()
            }
            ShareInput::Form | ShareInput::Issue | ShareInput::CopyAll => {
                let d = Details::now(self.renderer_label());
                let text = self.share.content.text();
                let summary = self.ticked_summary(self.share.include);
                let all = feedback::all_text(&text, &d, summary.as_deref());
                let task = match input {
                    ShareInput::CopyAll => {
                        self.banners.push("Your feedback was copied.".into());
                        iced::clipboard::write(all)
                    }
                    ShareInput::Form => match feedback::share_form(&text, &d, summary.as_deref()) {
                        Some(a) => self.send(a, all),
                        None => return Task::none(),
                    },
                    _ => {
                        let a = feedback::share_issue(&text, &d, summary.as_deref());
                        self.send(a, all)
                    }
                };
                // It went to the browser or the clipboard: the box starts
                // empty next time.
                self.share.content = text_editor::Content::new();
                self.share.include = false;
                self.sheet = None;
                task
            }
        }
    }

    /// Report a problem's buttons with the tick (editor.md §23).
    pub(super) fn problem_send(&mut self, form: bool) -> Task<Message> {
        let d = Details::now(self.renderer_label());
        let summary = self.ticked_summary(self.share.problem_include);
        let all = feedback::all_text("", &d, summary.as_deref());
        self.share.problem_include = false;
        self.sheet = None;
        if form {
            match feedback::bug_form_with(&d, summary.as_deref()) {
                Some(a) => self.send(a, all),
                None => Task::none(),
            }
        } else {
            let a = feedback::bug_issue_with(&d, summary.as_deref());
            self.send(a, all)
        }
    }

    /// The details Report a problem copies, with the summary if ticked.
    pub(super) fn problem_copy_text(&self) -> String {
        let d = Details::now(self.renderer_label());
        feedback::all_text(
            "",
            &d,
            self.ticked_summary(self.share.problem_include).as_deref(),
        )
    }

    /// Keys while Share feedback, the usage summary or Settings is open.
    pub(super) fn usage_sheet_key(&mut self, kp: &KeyPress, esc: bool) -> Option<Task<Message>> {
        use iced::keyboard::{Key, key::Named};
        let sheet = self.sheet?;
        let ch = match kp.key.as_ref() {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        let named = match kp.key.as_ref() {
            Key::Named(n) => Some(n),
            _ => None,
        };
        let enter = named == Some(Named::Enter);
        match sheet {
            Sheet::ShareFeedback => {
                if esc {
                    self.sheet = None;
                    return Some(Task::none());
                }
                let form = feedback::FORM_URL.is_some();
                let input = match (ch.as_str(), named) {
                    (_, Some(Named::Tab)) if self.share.typing => ShareInput::LeftText,
                    ("u", _) => ShareInput::Include,
                    ("f", _) if form => ShareInput::Form,
                    ("g", _) => ShareInput::Issue,
                    ("c", _) => ShareInput::CopyAll,
                    ("t", _) => ShareInput::Focus,
                    _ => return Some(Task::none()),
                };
                Some(self.share_input(input))
            }
            Sheet::Usage => {
                if self.usage.confirm_reset {
                    let input = if enter {
                        UsageInput::ResetYes
                    } else if esc {
                        UsageInput::ResetNo
                    } else {
                        return Some(Task::none());
                    };
                    return Some(self.usage_input(input));
                }
                let input = match ch.as_str() {
                    _ if esc => UsageInput::Close,
                    "c" => UsageInput::Copy,
                    "r" => UsageInput::Reset,
                    "o" => UsageInput::Toggle,
                    _ => return Some(Task::none()),
                };
                Some(self.usage_input(input))
            }
            Sheet::Settings => {
                if esc {
                    self.sheet = None;
                    return Some(Task::none());
                }
                let last = ROWS.len() - 1;
                let s = &mut self.settings_sheet;
                match named {
                    Some(Named::ArrowUp) => {
                        s.row = s.row.saturating_sub(1);
                        s.part = 0;
                    }
                    Some(Named::ArrowDown) => {
                        s.row = (s.row + 1).min(last);
                        s.part = 0;
                    }
                    Some(Named::Tab) if s.row == last => {
                        s.part = if kp.modifiers.shift() {
                            (s.part + 2) % 3
                        } else {
                            (s.part + 1) % 3
                        };
                    }
                    Some(Named::ArrowLeft) => return Some(self.settings_step(-1)),
                    Some(Named::ArrowRight) => return Some(self.settings_step(1)),
                    Some(Named::Space) | Some(Named::Enter) => {
                        return Some(self.settings_activate());
                    }
                    _ => {}
                }
                Some(Task::none())
            }
            _ => None,
        }
    }

    // Settings (editor.md §24).

    pub(super) fn settings_open(&mut self) {
        self.palette = None;
        self.settings_sheet = SettingsState::default();
        self.sheet = Some(Sheet::Settings);
    }

    /// Each row's values, and which is current.
    fn settings_values(&self, row: usize) -> (Vec<String>, usize) {
        match row {
            0 => (
                THEMES.iter().map(|(l, _)| (*l).to_string()).collect(),
                THEMES
                    .iter()
                    .position(|(_, c)| *c == self.settings.theme)
                    .unwrap_or(0),
            ),
            1 => (
                INTERFACE_SIZES.iter().map(|s| format!("{s}%")).collect(),
                INTERFACE_SIZES
                    .iter()
                    .position(|s| *s == self.settings.interface_size)
                    .unwrap_or(0),
            ),
            2 => (
                RENDERERS.iter().map(|(l, _)| (*l).to_string()).collect(),
                RENDERERS
                    .iter()
                    .position(|(_, r)| *r == self.settings.renderer)
                    .unwrap_or(0),
            ),
            3 => (
                vec![
                    self.settings
                        .author_name
                        .clone()
                        .unwrap_or_else(|| "Writer".into()),
                ],
                0,
            ),
            4 => (
                vec!["On".into(), "Off".into()],
                usize::from(!self.settings.agent_access),
            ),
            5 => (
                vec!["Ask after a crash".into(), "Off".into()],
                usize::from(self.settings.crash_reports == CrashReports::Off),
            ),
            _ => (
                vec!["Counting on".into(), "Counting off".into()],
                usize::from(!self.settings.count_usage),
            ),
        }
    }

    /// Sets row `row` to its value `i`, through the setting's own command.
    fn settings_choose(&mut self, row: usize, i: usize) -> Task<Message> {
        let (values, current) = self.settings_values(row);
        if i >= values.len() || (i == current && row != 3) {
            return Task::none();
        }
        match row {
            0 => {
                let name = match THEMES[i].1 {
                    Choice::System => "view.theme_system",
                    Choice::Light => "view.theme_light",
                    Choice::Dark => "view.theme_dark",
                    Choice::HighContrast => "view.theme_high_contrast",
                };
                self.action(name)
            }
            1 => {
                let step = if i > current {
                    "view.interface_larger"
                } else {
                    "view.interface_smaller"
                };
                let mut t = Task::none();
                for _ in 0..current.abs_diff(i) {
                    t = self.action(step);
                }
                t
            }
            2 => match RENDERERS[i].1 {
                Renderer::Cpu => self.action("app.renderer_cpu"),
                Renderer::Gpu => self.action("app.renderer_gpu"),
                Renderer::Auto => {
                    self.settings.renderer = Renderer::Auto;
                    self.save_settings();
                    self.banners
                        .push("The new renderer is used from the next start of Cold Open.".into());
                    Task::none()
                }
            },
            3 => {
                self.sheet = None;
                self.action("app.author_name")
            }
            4 => self.action("app.agent_access"),
            5 => {
                self.crash_reports_toggle();
                Task::none()
            }
            _ => {
                self.usage_toggle();
                Task::none()
            }
        }
    }

    /// Left and Right: the row's previous or next value.
    fn settings_step(&mut self, by: i32) -> Task<Message> {
        let row = self.settings_sheet.row;
        if row == 3 || row == ROWS.len() - 1 && self.settings_sheet.part != 0 {
            return Task::none();
        }
        let (values, current) = self.settings_values(row);
        let n = values.len();
        let i = if n == 2 {
            1 - current
        } else if by < 0 {
            current.saturating_sub(1)
        } else {
            (current + 1).min(n - 1)
        };
        self.settings_choose(row, i)
    }

    /// Space or Enter on the selected row.
    fn settings_activate(&mut self) -> Task<Message> {
        let s = self.settings_sheet;
        match s.row {
            3 => self.settings_choose(3, 0),
            6 if s.part == 1 => self.settings_input(SettingsInput::UsageView),
            6 if s.part == 2 => self.settings_input(SettingsInput::UsageReset),
            4..=6 => self.settings_step(1),
            _ => Task::none(),
        }
    }

    pub(super) fn settings_input(&mut self, input: SettingsInput) -> Task<Message> {
        match input {
            SettingsInput::Choose(row, i) => {
                self.settings_sheet.row = row;
                self.settings_choose(row, i)
            }
            SettingsInput::UsageView => {
                self.usage_open();
                Task::none()
            }
            SettingsInput::UsageReset => {
                self.usage_open();
                self.usage.confirm_reset = true;
                Task::none()
            }
            SettingsInput::Close => {
                self.sheet = None;
                Task::none()
            }
        }
    }

    // The views.

    pub(super) fn settings_view(&self, pal: Palette) -> Element<'_, Message> {
        let mut rows = column![].spacing(6);
        for (r, label) in ROWS.iter().enumerate() {
            let (values, current) = self.settings_values(r);
            let mut choices = row![].spacing(6);
            if r == 3 {
                choices = choices.push(body(pal, values[0].clone())).push(
                    button(text("Change…").font(fonts::UI).size(13))
                        .on_press(Message::SettingsSheet(SettingsInput::Choose(3, 0)))
                        .padding([4, 12])
                        .style(button::secondary),
                );
            } else {
                for (i, v) in values.iter().enumerate() {
                    choices = choices.push(
                        button(text(v.clone()).font(fonts::UI).size(13))
                            .on_press(Message::SettingsSheet(SettingsInput::Choose(r, i)))
                            .padding([4, 12])
                            .style(if i == current {
                                button::primary
                            } else {
                                button::secondary
                            }),
                    );
                }
                if r == ROWS.len() - 1 {
                    let part = self.settings_sheet.part;
                    let marked = |p: usize, label: &'static str| {
                        if self.settings_sheet.row == r && part == p {
                            format!("[{label}]")
                        } else {
                            label.to_string()
                        }
                    };
                    choices = choices
                        .push(
                            button(text(marked(1, "View…")).font(fonts::UI).size(13))
                                .on_press(Message::SettingsSheet(SettingsInput::UsageView))
                                .padding([4, 12])
                                .style(button::secondary),
                        )
                        .push(
                            button(text(marked(2, "Reset…")).font(fonts::UI).size(13))
                                .on_press(Message::SettingsSheet(SettingsInput::UsageReset))
                                .padding([4, 12])
                                .style(button::secondary),
                        );
                }
            }
            let selected = self.settings_sheet.row == r;
            let line = row![
                text(*label)
                    .font(fonts::UI)
                    .size(13)
                    .color(pal.text)
                    .width(Length::Fixed(210.0)),
                choices.wrap().vertical_spacing(6),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center);
            rows = rows.push(container(line).padding([6, 8]).width(Length::Fill).style(
                move |_| container::Style {
                    border: Border {
                        color: if selected {
                            pal.focus
                        } else {
                            iced::Color::TRANSPARENT
                        },
                        width: 2.0,
                        radius: 6.0.into(),
                    },
                    ..container::Style::default()
                },
            ));
        }
        column![
            title(pal, "Settings"),
            hint(
                pal,
                "Each change applies at once and is kept. The renderer changes at the next start."
            ),
            rows,
            hint(
                pal,
                "Usage summary: counts of which features you use, on this computer only. Cold Open never sends it."
            ),
            row![
                Space::new().width(Length::Fill),
                dialog_button("Close", Message::SettingsSheet(SettingsInput::Close), false),
            ],
            hint(
                pal,
                "Up, Down: row  ·  Left, Right: change  ·  Space, Enter: switch or open  ·  Tab: on the last row, its buttons  ·  Esc: close"
            ),
        ]
        .spacing(10)
        .into()
    }

    pub(super) fn usage_view(&self, pal: Palette) -> Element<'_, Message> {
        let on = self.settings.count_usage;
        let mut buttons = row![
            dialog_button("Copy", Message::Usage(UsageInput::Copy), false),
            dialog_button("Reset…", Message::Usage(UsageInput::Reset), false),
            dialog_button(
                if on {
                    "Turn counting off"
                } else {
                    "Turn counting on"
                },
                Message::Usage(UsageInput::Toggle),
                false,
            ),
            dialog_button("Close", Message::Usage(UsageInput::Close), true),
        ]
        .spacing(8);
        let mut col = column![
            title(pal, "Usage summary"),
            body(
                pal,
                "Counted on this computer only. Cold Open never sends it. It goes out only if you tick \
                 \"Include my usage summary\" when you share feedback."
            ),
            hint(
                pal,
                if on {
                    "Counting is on."
                } else {
                    "Counting is off: nothing is counted or written."
                }
            ),
            scrollable(
                container(
                    text(self.usage.view.as_str())
                        .font(fonts::PAGE)
                        .size(11)
                        .color(pal.text)
                )
                .padding(6)
            )
            .width(Length::Fill)
            .height(Length::Fixed(260.0)),
        ]
        .spacing(10);
        if self.usage.confirm_reset {
            col = col.push(body(
                pal,
                "Reset the usage summary? Its file is deleted, and counting starts again from today. Enter: reset  ·  Esc: keep it",
            ));
            buttons = row![
                dialog_button("Reset", Message::Usage(UsageInput::ResetYes), true),
                dialog_button("Keep it", Message::Usage(UsageInput::ResetNo), false),
            ]
            .spacing(8);
        }
        col.push(buttons.wrap().vertical_spacing(8))
            .push(hint(
                pal,
                "C: copy  ·  R: reset  ·  O: counting on or off  ·  Esc: close",
            ))
            .into()
    }

    /// The tick and, when ticked, the summary in full (usage-summary.md §8).
    pub(super) fn summary_tick(
        &self,
        pal: Palette,
        ticked: bool,
        m: Message,
    ) -> Element<'_, Message> {
        let label = match &self.share.summary {
            Ok(_) => "Include my usage summary",
            Err(why) => why,
        };
        let mark = if ticked { "[x]" } else { "[ ]" };
        let mut tick = button(text(format!("{mark} {label}")).font(fonts::UI).size(13))
            .padding([4, 10])
            .style(button::text);
        if self.share.summary.is_ok() {
            tick = tick.on_press(m);
        }
        let mut col = column![tick].spacing(6);
        if ticked && let Ok(s) = &self.share.summary {
            col = col.push(
                scrollable(
                    container(text(s.as_str()).font(fonts::PAGE).size(11).color(pal.text))
                        .padding(6),
                )
                .width(Length::Fill)
                .height(Length::Fixed(150.0)),
            );
        }
        col.into()
    }

    pub(super) fn share_view(&self, pal: Palette) -> Element<'_, Message> {
        let d = Details::now(self.renderer_label());
        let form = feedback::FORM_URL.is_some();
        let mod_key = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        let editor = text_editor(&self.share.content)
            .id(SHARE_TEXT)
            .placeholder("What works, what doesn't, and what you'd like…")
            .on_action(|a| Message::Share(ShareInput::Edit(a)))
            .key_binding(move |kp| {
                use iced::keyboard::{Key, key::Named};
                // Only while the box has the keyboard: otherwise the keys
                // are the sheet's.
                if !matches!(kp.status, text_editor::Status::Focused { .. }) {
                    return None;
                }
                match kp.key.as_ref() {
                    Key::Named(Named::Enter) if kp.modifiers.command() => {
                        Some(text_editor::Binding::Custom(Message::Share(if form {
                            ShareInput::Form
                        } else {
                            ShareInput::Issue
                        })))
                    }
                    Key::Named(Named::Escape) | Key::Named(Named::Tab) => {
                        Some(text_editor::Binding::Sequence(vec![
                            text_editor::Binding::Unfocus,
                            text_editor::Binding::Custom(Message::Share(ShareInput::LeftText)),
                        ]))
                    }
                    _ => text_editor::Binding::from_key_press(kp),
                }
            })
            .font(fonts::UI)
            .size(14)
            .padding(8)
            .height(Length::Fixed(140.0));
        let mut buttons = row![].spacing(8);
        if form {
            buttons = buttons.push(dialog_button(
                "Open the form (no account)",
                Message::Share(ShareInput::Form),
                true,
            ));
        }
        buttons = buttons
            .push(dialog_button(
                "Open a GitHub issue",
                Message::Share(ShareInput::Issue),
                !form,
            ))
            .push(dialog_button(
                "Copy all",
                Message::Share(ShareInput::CopyAll),
                false,
            ))
            .push(dialog_button(
                "Close",
                Message::Share(ShareInput::Close),
                false,
            ));
        let keys = if self.share.typing {
            format!(
                "{mod_key}+Enter: open the {}  ·  Esc or Tab: leave the text box",
                if form { "form" } else { "issue" }
            )
        } else {
            format!(
                "T: type  ·  U: include the summary  ·  {}G: GitHub issue  ·  C: copy all  ·  Esc: close",
                if form { "F: the form  ·  " } else { "" }
            )
        };
        column![
            title(pal, "Share feedback"),
            body(
                pal,
                "Tell us what works, what doesn't, and what you'd like. Cold Open opens a page in your \
                 browser with your text filled in. Nothing is sent until you submit it there."
            ),
            editor,
            self.summary_tick(pal, self.share.include, Message::Share(ShareInput::Include)),
            hint(pal, format!("Also sent: {}", d.lines().trim_end().replace('\n', " · "))),
            buttons.wrap().vertical_spacing(8),
            hint(pal, keys),
        ]
        .spacing(10)
        .into()
    }
}
