//! The About sheet (editor.md §22): version, commit, source and every
//! license, one tab each. A license is drawn a line at a time, only the
//! lines in sight, as the report view's rows are, so the long third-party
//! list costs no more to scroll than a short one.

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length, Task};

use super::view::{body, hint, title};
use super::{App, Message, Sheet};
use crate::about::{self, AboutTab};
use crate::fonts;
use crate::page_view::KeyPress;
use crate::theme::Palette;

/// A license line's height in pixels.
const LINE_H: f32 = 15.0;
/// Lines drawn beyond each edge of the visible part.
const SPARE: usize = 10;
/// The license area's height.
const AREA_H: f32 = 320.0;
/// The widest a license line is drawn before it wraps, in characters.
const WRAP: usize = 80;
/// The license area's scrolling id.
pub(super) const ABOUT_TEXT: &str = "about-text";

/// What the About sheet keeps between frames.
#[derive(Clone, Debug, Default)]
pub struct AboutState {
    pub tab: AboutTab,
    /// The tab's text, split into lines.
    lines: Vec<String>,
    /// How far the text is scrolled, in pixels.
    offset: f32,
}

impl AboutState {
    fn new(tab: AboutTab) -> AboutState {
        AboutState {
            tab,
            lines: tab.text().lines().flat_map(|l| wrap(l, WRAP)).collect(),
            offset: 0.0,
        }
    }
}

impl App {
    pub(super) fn about_open(&mut self) {
        self.about = AboutState::new(AboutTab::ColdOpen);
        self.sheet = Some(Sheet::About);
    }

    pub(super) fn about_tab(&mut self, tab: AboutTab) -> Task<Message> {
        self.about = AboutState::new(tab);
        iced::widget::operation::snap_to(ABOUT_TEXT, scrollable::RelativeOffset::START)
    }

    /// The About sheet's tab, while the sheet is open.
    pub fn about_shown(&self) -> Option<AboutTab> {
        (self.sheet == Some(Sheet::About)).then_some(self.about.tab)
    }

    /// The About sheet's text, a line at a time.
    pub fn about_lines(&self) -> &[String] {
        &self.about.lines
    }

    pub(super) fn about_scrolled(&mut self, v: scrollable::Viewport) {
        self.about.offset = v.absolute_offset().y;
    }

    /// What draws the window, as the About sheet and the version info say
    /// it.
    pub fn renderer_label(&self) -> String {
        match &self.graphics {
            Some((_, backend)) if backend == "tiny-skia" => {
                if self.settings.renderer == crate::files::Renderer::Gpu {
                    "CPU (the GPU renderer could not start)".into()
                } else {
                    "CPU".into()
                }
            }
            Some((adapter, backend)) => format!("GPU: {adapter} ({backend})"),
            None => match self.settings.renderer.backends() {
                "tiny-skia" => "CPU".into(),
                _ => "GPU, or CPU if the GPU can't start".into(),
            },
        }
    }

    pub(super) fn copy_version_info(&mut self) -> Task<Message> {
        self.banners.push("Version info copied.".into());
        iced::clipboard::write(about::version_info(&self.renderer_label()))
    }

    /// Keys while the sheet is open: Tab and Shift+Tab (or Ctrl+Tab)
    /// change tabs, C copies the version info.
    pub(super) fn about_key(&mut self, kp: &KeyPress) -> Option<Task<Message>> {
        use iced::keyboard::{Key, key::Named};
        match &kp.key {
            Key::Named(Named::Tab) => {
                Some(self.about_tab(self.about.tab.next(kp.modifiers.shift())))
            }
            Key::Character(c)
                if c.as_str().eq_ignore_ascii_case("c") && !kp.modifiers.command() =>
            {
                Some(self.copy_version_info())
            }
            _ => None,
        }
    }

    pub(super) fn about_view(&self, pal: Palette) -> Element<'_, Message> {
        let a = &self.about;
        let link = |label: &str, url: String| {
            button(text(label.to_string()).font(fonts::UI).size(13))
                .on_press(Message::OpenLink(url))
                .padding([3, 10])
                .style(button::secondary)
        };
        let mut links = row![link("Source code", about::source_link())].spacing(8);
        if let Some(url) = about::DOCS_URL {
            links = links.push(link("User guide", url.to_string()));
        }
        if let Some(url) = about::FEEDBACK_URL {
            links = links.push(link("Send feedback", url.to_string()));
        }
        links = links.push(
            button(text("Copy version info").font(fonts::UI).size(13))
                .on_press(Message::CopyVersionInfo)
                .padding([3, 10])
                .style(button::secondary),
        );
        let tabs = AboutTab::ALL.iter().fold(row![].spacing(6), |acc, t| {
            acc.push(
                button(text(t.title()).font(fonts::UI).size(12))
                    .on_press(Message::AboutTab(*t))
                    .padding([3, 10])
                    .style(if *t == a.tab {
                        button::primary
                    } else {
                        button::secondary
                    }),
            )
        });
        let n = a.lines.len();
        let first = ((a.offset / LINE_H) as usize).saturating_sub(SPARE).min(n);
        let last = (((a.offset + AREA_H) / LINE_H).ceil() as usize + SPARE).min(n);
        let mut lines = column![iced::widget::space().height(Length::Fixed(first as f32 * LINE_H))];
        for line in &a.lines[first..last] {
            lines = lines.push(
                container(
                    text(line.as_str())
                        .font(fonts::PAGE)
                        .size(11)
                        .color(pal.text)
                        .wrapping(text::Wrapping::None),
                )
                .height(Length::Fixed(LINE_H)),
            );
        }
        lines = lines.push(iced::widget::space().height(Length::Fixed((n - last) as f32 * LINE_H)));
        column![
            title(
                pal,
                format!("Cold Open {}", coldopen_core::build_info::VERSION)
            ),
            body(
                pal,
                format!(
                    "Commit {} · {} · drawn with {}",
                    coldopen_core::build_info::COMMIT,
                    coldopen_core::build_info::TARGET,
                    self.renderer_label()
                )
            ),
            links,
            tabs,
            scrollable(lines)
                .id(ABOUT_TEXT)
                .on_scroll(Message::AboutScrolled)
                .width(Length::Fill)
                .height(Length::Fixed(AREA_H)),
            hint(
                pal,
                "Tab: next license  ·  C: copy version info  ·  Esc: close"
            ),
        ]
        .spacing(10)
        .into()
    }
}

/// A line cut at spaces into lines of at most `width` characters, keeping
/// its indent; a word longer than that stays whole.
fn wrap(line: &str, width: usize) -> Vec<String> {
    if line.chars().count() <= width {
        return vec![line.to_string()];
    }
    let indent: String = line.chars().take_while(|c| *c == ' ').collect();
    let mut out = Vec::new();
    let mut cur = indent.clone();
    for word in line.split_whitespace() {
        let fits = cur.chars().count() + 1 + word.chars().count() <= width;
        if cur.trim().is_empty() {
            cur.push_str(word);
        } else if fits {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(std::mem::replace(&mut cur, format!("{indent}{word}")));
        }
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn long_lines_wrap_at_spaces() {
        assert_eq!(wrap("short", 10), vec!["short"]);
        assert_eq!(
            wrap("  one two three four", 10),
            vec!["  one two", "  three", "  four"]
        );
        assert_eq!(wrap("abcdefghijklmno x", 10), vec!["abcdefghijklmno", "x"]);
    }
}
