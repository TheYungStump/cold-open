//! Comparing drafts in the app (editor.md §21): "Compare with…" shows the
//! other draft's pages with the changes marked, read-only, beside a list of
//! the changed scenes; "Review a draft as changes…" brings a draft in as
//! one writer's tracked changes.

use std::collections::BTreeSet;
use std::path::PathBuf;

use coldopen_core::{BlockId, Document, View};
use iced::widget::{button, column, container, row, scrollable, space, text, text_input};
use iced::{Background, Border, Element, Fill, Length, Task};
use serde_json::{Value, json};

use super::view::{body, dialog_button, ellipsize, hint, title};
use super::{App, Message, Prompt};
use crate::editor::geometry::{Caret, Surface};
use crate::files::Ask;
use crate::fonts;
use crate::page_view::{GAP, KeyPress, MARGIN, PageView};
use crate::theme::Palette;

/// The compare view's pages, for scrolling.
const COMPARE_PAGES: &str = "compare-pages";
/// The import name's field.
pub const IMPORT_NAME: &str = "import-name";
/// The list's width, as the navigator's.
const LIST: f32 = 320.0;

/// One changed scene in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareRow {
    pub label: String,
    /// Where it starts in the marked pages.
    pub block: Option<BlockId>,
}

/// The open compare view.
pub struct CompareView {
    /// The other draft.
    pub b: PathBuf,
    /// The first line of the summary.
    pub summary: String,
    pub rows: Vec<CompareRow>,
    pub selected: usize,
    /// The other draft's text with the changes marked, and its pages.
    pub doc: Document,
    pub view: View,
    /// No agent marks here.
    marked: BTreeSet<usize>,
}

/// What the compare view and the import question send.
#[derive(Clone, Debug)]
pub enum CompareInput {
    Row(usize),
    SavePdf,
    Close,
    ImportName(String),
    ImportSubmit,
}

impl App {
    /// "Compare with…": the open script against the draft at `path`.
    pub(super) fn compare_open(&mut self, path: PathBuf) -> Task<Message> {
        let Some(s) = self.d.session() else {
            return Task::none();
        };
        let (b, fb) = match coldopen_core::diff::read_draft(&path) {
            Ok(x) => x,
            Err(e) => {
                self.banners.push(e.message.into());
                return Task::none();
            }
        };
        let named = |doc, format, path| coldopen_core::diff::Named { doc, format, path };
        let result = coldopen_core::diff::result(
            &named(&s.doc, s.format, s.path.as_deref()),
            &named(&b, fb, Some(path.as_path())),
        );
        let name = coldopen_core::diff::default_name(&path);
        let doc = match coldopen_core::diff::marked(&s.doc, s.format, &b, fb, &name) {
            Ok(d) => d,
            Err(e) => {
                self.banners.push(e.message.into());
                return Task::none();
            }
        };
        let view = View::with_format(&doc, 0, s.format);
        let rows = rows(&result, &doc);
        let summary = result["text"]
            .as_str()
            .and_then(|t| t.lines().next())
            .unwrap_or_default()
            .to_string();
        self.compare = Some(CompareView {
            b: path,
            summary,
            rows,
            selected: 0,
            doc,
            view,
            marked: BTreeSet::new(),
        });
        self.compare_go(0)
    }

    /// Scrolls the pages to row `i`'s scene.
    fn compare_go(&mut self, i: usize) -> Task<Message> {
        let zoom = self.shown_zoom();
        let Some(c) = self.compare.as_mut() else {
            return Task::none();
        };
        if c.rows.is_empty() {
            return Task::none();
        }
        c.selected = i.min(c.rows.len() - 1);
        let Some(block) = c.rows[c.selected].block else {
            return Task::none();
        };
        let Some(spot) = Surface::Pages(&c.view).spot(&c.doc, Caret::at(block, 0)) else {
            return Task::none();
        };
        let f = c.view.layout().format();
        let k = zoom * 96.0 / 72.0 / 100.0;
        let page_h = f.page_height.0 as f32 * k;
        let y = MARGIN + spot.page as f32 * (page_h + GAP) + spot.top.0 as f32 * k;
        let lh = f.line_height.0 as f32 * k;
        iced::widget::operation::scroll_to(
            COMPARE_PAGES,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some((y - 3.0 * lh).max(0.0)),
            },
        )
    }

    /// What the view's buttons and the import question send.
    pub(super) fn compare_input(&mut self, input: CompareInput) -> Task<Message> {
        match input {
            CompareInput::Row(i) => self.compare_go(i),
            CompareInput::Close => {
                self.compare = None;
                Task::none()
            }
            CompareInput::SavePdf => {
                let Some(c) = &self.compare else {
                    return Task::none();
                };
                let name = format!("{} (changes).pdf", coldopen_core::diff::default_name(&c.b));
                self.ask(Ask::DiffPdf(name))
            }
            CompareInput::ImportName(n) => {
                if let Some(Prompt::ImportName { name, .. }) = &mut self.prompt {
                    *name = n;
                }
                Task::none()
            }
            CompareInput::ImportSubmit => self.import_submit(),
        }
    }

    /// "Save as PDF…" in the compare view: the other draft with the changes
    /// marked, written as `diff` writes it.
    pub(super) fn compare_pdf(&mut self, target: PathBuf, asked: bool) -> Task<Message> {
        let Some(c) = &self.compare else {
            return Task::none();
        };
        let params = json!({
            "b": c.b.display().to_string(),
            "pdf": target.display().to_string(),
            "overwrite": asked,
        });
        if self.run("diff", params).is_some() {
            self.banners
                .push(format!("Saved {}.", super::view::file_name(&target)).into());
        }
        Task::none()
    }

    /// "Review a draft as changes…": the draft chosen, the name its changes
    /// carry is asked, prefilled with its file name.
    pub(super) fn import_ask(&mut self, path: PathBuf) -> Task<Message> {
        let name = coldopen_core::diff::default_name(&path);
        self.prompt = Some(Prompt::ImportName { path, name });
        Task::batch([
            iced::widget::operation::focus(IMPORT_NAME),
            iced::widget::operation::select_all(IMPORT_NAME),
        ])
    }

    fn import_submit(&mut self) -> Task<Message> {
        let Some(Prompt::ImportName { path, name }) = self.prompt.take() else {
            return Task::none();
        };
        let params = json!({ "path": path.display().to_string(), "name": name.trim() });
        let Some(r) = self.run("changes.import", params) else {
            return Task::none();
        };
        let skipped = r["skipped"].as_array().map_or(0, Vec::len);
        let message = match (r["change"].is_null(), skipped) {
            (true, 0) => "The draft says what this script says: nothing to bring in.".to_string(),
            (true, n) => format!(
                "Nothing could be brought in as changes; {n} {} only in styles, attributes or the title page.",
                if n == 1 {
                    "difference is"
                } else {
                    "differences are"
                }
            ),
            (false, 0) => format!("Brought in {} as changes.", name.trim()),
            (false, n) => format!(
                "Brought in {} as changes. {n} {} in styles, attributes or the title page could not be tracked and were left out.",
                name.trim(),
                if n == 1 { "difference" } else { "differences" }
            ),
        };
        self.banners.push(message.into());
        if !r["change"].is_null() {
            self.settings.changes_panel = true;
            self.save_settings();
        }
        self.after_change(false)
    }

    /// Keys while the compare view is open, or `None` when it is not.
    pub(super) fn compare_key(
        &mut self,
        kp: &KeyPress,
        enter: bool,
        esc: bool,
    ) -> Option<Task<Message>> {
        use iced::keyboard::{Key, key::Named};
        let c = self.compare.as_ref()?;
        let at = c.selected;
        let ch = match kp.key.as_ref() {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        Some(match kp.key.as_ref() {
            _ if esc => {
                self.compare = None;
                Task::none()
            }
            _ if enter => self.compare_go(at),
            Key::Named(Named::ArrowDown) => {
                let last = c.rows.len().saturating_sub(1);
                if let Some(c) = self.compare.as_mut() {
                    c.selected = (at + 1).min(last);
                }
                Task::none()
            }
            Key::Named(Named::ArrowUp) => {
                if let Some(c) = self.compare.as_mut() {
                    c.selected = at.saturating_sub(1);
                }
                Task::none()
            }
            _ if kp.modifiers.command() && kp.modifiers.shift() && ch == "e" => {
                self.compare_input(CompareInput::SavePdf)
            }
            _ => Task::none(),
        })
    }

    /// The compare view, over the whole window (editor.md §21).
    pub(super) fn compare_view<'a>(
        &'a self,
        pal: Palette,
        c: &'a CompareView,
    ) -> Element<'a, Message> {
        let m = if crate::keymap::MAC { "Cmd" } else { "Ctrl" };
        let mut list = column![].spacing(2);
        if c.rows.is_empty() {
            list = list.push(container(body(pal, "No scene differs.")).padding([6, 12]));
        }
        for (i, r) in c.rows.iter().enumerate() {
            let on = i == c.selected;
            list = list.push(
                button(
                    text(ellipsize(&r.label, 44))
                        .font(fonts::UI)
                        .size(13)
                        .color(if on { pal.text } else { pal.text_quiet })
                        .wrapping(text::Wrapping::None),
                )
                .on_press(Message::Compare(CompareInput::Row(i)))
                .padding([4, 12])
                .width(Fill)
                .style(move |_, _| button::Style {
                    background: on.then_some(Background::Color(pal.accent_soft)),
                    text_color: pal.text,
                    ..button::Style::default()
                }),
            );
        }
        let side = container(
            column![
                text(format!("Changed scenes · {}", c.rows.len()))
                    .font(fonts::UI)
                    .size(12)
                    .color(pal.text_quiet),
                scrollable(list).height(Fill),
            ]
            .spacing(8)
            .padding([8, 0]),
        )
        .width(Length::Fixed(LIST))
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(pal.panel)),
            border: Border {
                color: pal.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });
        let page = PageView {
            view: &c.view,
            doc: &c.doc,
            draft: None,
            title: false,
            palette: pal,
            zoom: self.shown_zoom(),
            focused: false,
            caret: None,
            caret_on: false,
            preedit: None,
            completion: None,
            selection: None,
            matches: &[],
            marked: &c.marked,
            coverage: &*self.fonts,
            on_event: Box::new(|_| Message::Noop),
        };
        let pages = scrollable(Element::from(page))
            .id(COMPARE_PAGES)
            .width(Fill)
            .height(Fill);
        let top = container(
            row![
                column![
                    title(
                        pal,
                        format!("Compared with {}", super::view::file_name(&c.b))
                    ),
                    hint(pal, c.summary.clone()),
                ]
                .spacing(2),
                space::horizontal(),
                dialog_button(
                    "Save as PDF…",
                    Message::Compare(CompareInput::SavePdf),
                    false
                ),
                dialog_button("Close", Message::Compare(CompareInput::Close), true),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .padding([10, 14])
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(pal.panel_alt)),
            ..container::Style::default()
        });
        let bottom = container(hint(
            pal,
            format!(
                "Up and Down: choose a scene  ·  Enter: go to it  ·  {m}+Shift+E: save as PDF  ·  Esc: close. Inserted text is underlined in color; deleted text is struck through."
            ),
        ))
        .padding([6, 14])
        .width(Fill);
        container(column![top, row![side, pages].height(Fill), bottom])
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(pal.desk)),
                ..container::Style::default()
            })
            .into()
    }

    /// The question for the import's name.
    pub(super) fn import_view(&self, pal: Palette, name: &str) -> Element<'_, Message> {
        column![
            title(pal, "Review a draft as changes"),
            body(
                pal,
                "Each difference comes in as a tracked change you can accept or reject. Give the changes a name, such as the co-writer's.",
            ),
            text_input("Name", name)
                .id(IMPORT_NAME)
                .on_input(|n| Message::Compare(CompareInput::ImportName(n)))
                .on_submit(Message::Compare(CompareInput::ImportSubmit))
                .font(fonts::UI)
                .size(14),
            row![
                dialog_button("Bring it in", Message::Compare(CompareInput::ImportSubmit), true),
                dialog_button("Cancel", Message::Escaped, false),
            ]
            .spacing(8),
            hint(pal, "Enter: bring it in  ·  Esc: cancel"),
        ]
        .spacing(12)
        .into()
    }
}

/// The list's rows: one per scene the summary names, each with where it
/// starts in the marked pages.
fn rows(result: &Value, doc: &Document) -> Vec<CompareRow> {
    let lines: Vec<&str> = result["text"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .skip(1)
        .filter(|l| !l.starts_with("Title page:"))
        .collect();
    let scenes = result["scenes"].as_array().cloned().unwrap_or_default();
    let mut from = 0;
    scenes
        .iter()
        .zip(lines)
        .map(|(s, label)| {
            // A scene A has keeps its block; an added one is found among
            // the inserted blocks, in order.
            let block = match s["a"]["id"]
                .as_str()
                .and_then(|id| serde_json::from_value::<BlockId>(json!(id)).ok())
            {
                Some(id) if doc.index_of(id).is_some() => Some(id),
                _ => {
                    let heading = s["b"]["heading"].as_str().unwrap_or_default();
                    doc.blocks
                        .iter()
                        .enumerate()
                        .skip(from)
                        .find(|(_, b)| {
                            b.marks.inserted.is_some()
                                && (heading.is_empty() || b.element.text.plain_text() == heading)
                        })
                        .map(|(i, b)| {
                            from = i + 1;
                            b.id
                        })
                }
            };
            CompareRow {
                label: label.trim_end_matches('.').to_string(),
                block,
            }
        })
        .collect()
}
