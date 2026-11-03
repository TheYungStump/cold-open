//! Comparing drafts in the app (editor.md §21): "Compare with…" shows the
//! other draft's pages with the changes marked, read-only, beside a list of
//! the changed scenes; "Review a draft as changes…" brings a draft in as
//! one writer's tracked changes.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
    /// The open script's format, which the marked pages are laid out in.
    format: coldopen_core::FormatId,
    /// No agent marks here.
    marked: BTreeSet<usize>,
}

/// A comparison worked out off the UI thread (2026-10-28, review finding
/// CL7): what the compare view shows.
pub struct Compared {
    b: PathBuf,
    result: Value,
    doc: Document,
    view: View,
    format: coldopen_core::FormatId,
}

/// What a worker hands back, taken once. It may hold a layout, which is
/// neither cloned nor printed, so it travels in a shared slot.
pub struct Handoff<T>(Arc<Mutex<Option<T>>>);

impl<T> Clone for Handoff<T> {
    fn clone(&self) -> Self {
        Handoff(Arc::clone(&self.0))
    }
}

impl<T> std::fmt::Debug for Handoff<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Handoff")
    }
}

impl<T> Handoff<T> {
    fn new(t: T) -> Self {
        Handoff(Arc::new(Mutex::new(Some(t))))
    }

    fn take(&self) -> Option<T> {
        self.0.lock().ok().and_then(|mut slot| slot.take())
    }
}

/// A comparison or an import being worked out off the UI thread: the
/// status bar says so, and Esc stops waiting for it.
pub struct Working {
    pub token: u64,
    pub what: String,
}

/// A planned import, back from its worker.
#[derive(Clone, Debug)]
pub struct ImportPlanned {
    token: u64,
    planned: Handoff<coldopen_core::PlannedImport>,
    path: PathBuf,
    name: String,
    again: bool,
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
    /// A new token for work off the UI thread, and the status bar's words
    /// for it.
    fn start_work(&mut self, what: String) -> u64 {
        self.jobs += 1;
        self.working = Some(Working {
            token: self.jobs,
            what,
        });
        self.jobs
    }

    /// Whether the answer of the work numbered `token` is still awaited;
    /// if so, it is not any more.
    fn finish_work(&mut self, token: u64) -> bool {
        if self.working.as_ref().is_some_and(|w| w.token == token) {
            self.working = None;
            true
        } else {
            false
        }
    }

    /// "Compare with…": the open script against the draft at `path`. The
    /// draft is read, compared and laid out on a worker thread, as a save
    /// is written (2026-10-28, review finding CL7), so the editor never
    /// waits for it.
    pub(super) fn compare_open(&mut self, path: PathBuf) -> Task<Message> {
        let Some(s) = self.d.session() else {
            return Task::none();
        };
        let (doc, format, own) = (s.doc.clone(), s.format, s.path.clone());
        let token = self.start_work(format!("Comparing with {}…", super::view::file_name(&path)));
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("compare".into())
            .spawn(move || {
                let _ = tx.send(compare_draft(&doc, format, own, path));
            });
        if let Err(e) = spawned {
            self.working = None;
            self.banners.push(e.to_string().into());
            return Task::none();
        }
        Task::perform(rx, move |r| {
            let r = r.unwrap_or_else(|_| Err("the comparison stopped before it finished".into()));
            Message::Compared(token, Handoff::new(r))
        })
    }

    /// A comparison back from its worker: the compare view opens, unless
    /// the writer stopped waiting for it.
    pub(super) fn compared(
        &mut self,
        token: u64,
        out: Handoff<Result<Compared, String>>,
    ) -> Task<Message> {
        if !self.finish_work(token) {
            return Task::none();
        }
        let Compared {
            b: path,
            result,
            doc,
            view,
            format,
        } = match out.take() {
            Some(Ok(c)) => c,
            Some(Err(why)) => {
                self.banners.push(why.into());
                return Task::none();
            }
            None => return Task::none(),
        };
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
            format,
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

    /// "Save as PDF…" in the compare view: the pages it shows, the other
    /// draft with the changes marked, laid out and written on a worker
    /// thread as `diff` writes them (2026-10-30, editor.md §21). The view
    /// stays open meanwhile, and the banner comes when it is written.
    pub(super) fn compare_pdf(&mut self, target: PathBuf, asked: bool) -> Task<Message> {
        let Some(c) = &self.compare else {
            return Task::none();
        };
        if self.pdf_writing.is_some() {
            self.banners
                .push("A PDF is still being written; try again when it's done.".into());
            return Task::none();
        }
        // What the `diff` command would refuse: the drafts, the open
        // script or its journal, or a file the dialog didn't ask about.
        let own = self.d.session().and_then(|s| s.path.clone());
        let taken = [
            Some(c.b.clone()),
            own.clone(),
            own.as_deref().map(coldopen_core::journal::path_for),
        ];
        if taken.iter().flatten().any(|t| same_file(t, &target)) {
            self.banners.push(
                format!(
                    "{} is one of the drafts or the open script; choose another name.",
                    super::view::file_name(&target)
                )
                .into(),
            );
            return Task::none();
        }
        if !asked && target.exists() {
            self.banners
                .push(format!("{} already exists.", super::view::file_name(&target)).into());
            return Task::none();
        }
        let (doc, format) = (c.doc.clone(), c.format);
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        let to = target.clone();
        let spawned = std::thread::Builder::new()
            .name("diff-pdf".into())
            .spawn(move || {
                let r =
                    coldopen_core::diff::write_marked_pdf(doc, format, &to).map_err(|e| e.message);
                let _ = tx.send(r);
            });
        if let Err(e) = spawned {
            self.banners.push(e.to_string().into());
            return Task::none();
        }
        self.pdf_writing = Some(super::view::file_name(&target));
        Task::perform(rx, move |r| {
            let r = r.unwrap_or_else(|_| Err("the PDF stopped before it was written".into()));
            Message::ComparePdfWritten(target.clone(), r)
        })
    }

    /// A diff PDF back from its worker: a banner either way.
    pub(super) fn compare_pdf_written(&mut self, target: PathBuf, r: Result<(), String>) {
        self.pdf_writing = None;
        let name = super::view::file_name(&target);
        self.banners.push(match r {
            Ok(()) => format!("Saved {name}.").into(),
            Err(e) => format!("Couldn't save {name}: {e}").into(),
        });
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
        self.import_start(path, name.trim().to_string(), false)
    }

    /// Plans bringing the draft in on a worker thread (2026-10-28, review
    /// finding CL7); the edits are applied when they come back.
    fn import_start(&mut self, path: PathBuf, name: String, again: bool) -> Task<Message> {
        let p = coldopen_core::command::ChangesImport {
            path: path.display().to_string(),
            name: Some(name.clone()),
        };
        let job = match self.d.import_prepare(&p) {
            Ok(j) => j,
            Err(e) => {
                self.banners.push(e.message.into());
                return Task::none();
            }
        };
        let token = self.start_work(format!("Comparing with {}…", super::view::file_name(&path)));
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("import".into())
            .spawn(move || {
                let _ = tx.send(job.plan());
            });
        if let Err(e) = spawned {
            self.working = None;
            self.banners.push(e.to_string().into());
            return Task::none();
        }
        Task::perform(rx, move |planned| match planned {
            Ok(planned) => Message::ImportPlanned(ImportPlanned {
                token,
                planned: Handoff::new(planned),
                path: path.clone(),
                name: name.clone(),
                again,
            }),
            Err(_) => Message::Compared(
                token,
                Handoff::new(Err("the comparison stopped before it finished".into())),
            ),
        })
    }

    /// The planned edits, back from the worker: applied as one tracked
    /// change, unless the writer stopped waiting. A script edited
    /// meanwhile is planned again, once.
    pub(super) fn import_planned(&mut self, p: ImportPlanned) -> Task<Message> {
        if !self.finish_work(p.token) {
            return Task::none();
        }
        let Some(planned) = p.planned.take() else {
            return Task::none();
        };
        let r = match self.d.import_apply(planned) {
            Ok(r) => r,
            Err(e) if e.code == "import_stale" && !p.again => {
                return self.import_start(p.path, p.name, true);
            }
            Err(e) => {
                self.banners.push(e.message.into());
                return Task::none();
            }
        };
        let name = p.name;
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
            (false, 0) => format!("Brought in {name} as changes."),
            (false, n) => format!(
                "Brought in {name} as changes. {n} {} in styles, attributes or the title page could not be tracked and were left out.",
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
                container(
                    text(format!("Changed scenes · {}", c.rows.len()))
                        .font(fonts::UI)
                        .size(12)
                        .color(pal.text_quiet),
                )
                .padding([0, 12]),
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
                "Up and Down: choose a scene  ·  Enter: go to it  ·  {m}+Shift+E: save as PDF  ·  Esc: close. New text is in color, deleted text struck through; restyles show in the list only."
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

/// The draft at `path` compared with the open script as it was when asked,
/// marked and laid out: the compare view's content, worked out on a worker
/// thread.
fn compare_draft(
    doc: &Document,
    format: coldopen_core::FormatId,
    own: Option<PathBuf>,
    path: PathBuf,
) -> Result<Compared, String> {
    let (b, fb) = coldopen_core::diff::read_draft(&path).map_err(|e| e.message)?;
    let named = |doc, format, path| coldopen_core::diff::Named { doc, format, path };
    let result = coldopen_core::diff::result(
        &named(doc, format, own.as_deref()),
        &named(&b, fb, Some(path.as_path())),
    );
    let name = coldopen_core::diff::default_name(&path);
    let marked = coldopen_core::diff::marked(doc, format, &b, fb, &name).map_err(|e| e.message)?;
    let view = View::with_format(&marked, 0, format);
    Ok(Compared {
        b: path,
        result,
        doc: marked,
        view,
        format,
    })
}

/// Whether two paths name one file, as the `diff` command judges it.
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}
