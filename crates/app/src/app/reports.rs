//! The report view (editor.md §20): the scenes, characters and locations
//! reports in three tabs, a row going to its scene.

use std::collections::BTreeMap;

use coldopen_core::report::{self, Kind, Table};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Background, Element, Length, Task};
use serde_json::json;

use super::report_lines::ReportLines;
use super::view::{ellipsize, hint, title};

/// The widest a column grows, in characters.
const WIDEST: usize = 36;
/// A row's height in pixels. Rows are all one height, so only those in
/// sight are built and drawn.
const ROW_H: f32 = 22.0;
/// The rows' area's height until it first reports its own.
const ROWS_H: f32 = 600.0;
/// The rows' scrolling area.
pub(super) const REPORT_ROWS: &str = "report-rows";
use super::{App, Message};
use crate::fonts;
use crate::page_view::KeyPress;
use crate::theme::Palette;

/// The open report view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportView {
    /// The tab shown: an index into [`Kind::ALL`].
    pub tab: usize,
    pub selected: usize,
    pub tables: Vec<Table>,
    /// Each tab's header and rows as lines of text, made once when the
    /// view opens (2026-10-30: drawn by one widget, `report_lines.rs`).
    pub lines: Vec<(String, Vec<String>)>,
    /// How far the rows are scrolled, and how tall their area is, in
    /// pixels.
    pub offset: u32,
    pub height: u32,
}

impl ReportView {
    pub fn table(&self) -> Option<&Table> {
        self.tables.get(self.tab)
    }
}

impl App {
    /// "Reports…": reads the three reports of the open script.
    pub(super) fn reports_open(&mut self) {
        if self.d.session().is_none() {
            return;
        }
        let index: BTreeMap<String, u64> = self
            .run("scene.list", json!({}))
            .and_then(|r| r["scenes"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|s| Some((s["id"].as_str()?.to_string(), s["index"].as_u64()?)))
            .collect();
        let tables: Vec<Table> = Kind::ALL
            .iter()
            .map(|&k| {
                self.run(k.command(), json!({}))
                    .map(|r| report::table(k, &r, &index))
                    .unwrap_or_default()
            })
            .collect();
        let lines = tables.iter().map(table_lines).collect();
        self.reports = Some(ReportView {
            tab: 0,
            selected: 0,
            tables,
            lines,
            offset: 0,
            height: ROWS_H as u32,
        });
    }

    /// Shows another tab, from its top.
    pub(super) fn reports_tab(&mut self, tab: usize) -> Task<Message> {
        if let Some(r) = &mut self.reports {
            r.tab = tab % Kind::ALL.len();
            r.selected = 0;
            r.offset = 0;
        }
        iced::widget::operation::snap_to(REPORT_ROWS, scrollable::RelativeOffset::START)
    }

    /// Where the rows are scrolled to, as the scrolling area reports it.
    pub(super) fn reports_scrolled(&mut self, v: scrollable::Viewport) {
        if let Some(r) = &mut self.reports {
            r.offset = v.absolute_offset().y.max(0.0) as u32;
            r.height = v.bounds().height.max(ROW_H) as u32;
        }
    }

    /// Scrolls the rows so the selected one is in sight.
    fn reports_follow(&mut self) -> Task<Message> {
        let Some(r) = &mut self.reports else {
            return Task::none();
        };
        let top = r.selected as f32 * ROW_H;
        let (offset, height) = (r.offset as f32, r.height as f32);
        let to = if top < offset {
            top
        } else if top + ROW_H > offset + height {
            top + ROW_H - height
        } else {
            return Task::none();
        };
        r.offset = to as u32;
        iced::widget::operation::scroll_to(
            REPORT_ROWS,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some(to),
            },
        )
    }

    /// Goes to a row's scene and closes the view.
    pub(super) fn reports_go(&mut self, row: usize) -> Task<Message> {
        let target = self
            .reports
            .as_ref()
            .and_then(|r| r.table()?.targets.get(row).cloned().flatten());
        self.reports = None;
        let Some(id) = target.and_then(|t| serde_json::from_value(json!(t)).ok()) else {
            return Task::none();
        };
        let c = crate::editor::Caret::at(id, 0);
        self.editor.caret = c;
        self.editor.anchor = c;
        self.after_change(true)
    }

    /// Keys while the view is open, or `None` when it is not.
    pub(super) fn reports_key(
        &mut self,
        kp: &KeyPress,
        enter: bool,
        esc: bool,
    ) -> Option<Task<Message>> {
        use iced::keyboard::{Key, key::Named};
        let r = self.reports.as_mut()?;
        let cmd = kp.modifiers.command();
        let rows = r.table().map_or(0, |t| t.rows.len());
        match kp.key.as_ref() {
            _ if esc => self.reports = None,
            _ if enter => {
                let row = r.selected;
                return Some(self.reports_go(row));
            }
            Key::Named(Named::Tab) if kp.modifiers.control() => {
                let n = Kind::ALL.len();
                let tab = if kp.modifiers.shift() {
                    r.tab + n - 1
                } else {
                    r.tab + 1
                };
                return Some(self.reports_tab(tab));
            }
            Key::Named(Named::ArrowDown) => {
                r.selected = (r.selected + 1).min(rows.saturating_sub(1));
                return Some(self.reports_follow());
            }
            Key::Named(Named::ArrowUp) => {
                r.selected = r.selected.saturating_sub(1);
                return Some(self.reports_follow());
            }
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("c") => {
                let csv = r.table().map(report::csv).unwrap_or_default();
                self.banners.push("Copied the report as CSV.".into());
                return Some(iced::clipboard::write(csv));
            }
            _ => {}
        }
        Some(Task::none())
    }

    /// The view, as drawn.
    pub(super) fn reports_view<'a>(
        &'a self,
        pal: Palette,
        r: &'a ReportView,
    ) -> Element<'a, Message> {
        let m = if crate::keymap::MAC { "Cmd" } else { "Ctrl" };
        let tabs = Kind::ALL
            .iter()
            .enumerate()
            .fold(row![].spacing(6), |acc, (k, kind)| {
                let on = k == r.tab;
                acc.push(
                    button(text(kind.title()).font(fonts::UI).size(13))
                        .on_press(Message::ReportTab(k))
                        .padding([4, 12])
                        .style(if on {
                            button::primary
                        } else {
                            button::secondary
                        }),
                )
            });
        let (header, lines): (&str, &'a [String]) = r
            .lines
            .get(r.tab)
            .map_or(("", &[]), |(h, l)| (h.as_str(), l.as_slice()));
        let list = ReportLines::new(
            lines,
            r.selected,
            ROW_H,
            fonts::PAGE,
            13.0,
            pal.text,
            (pal.accent_soft, pal.focus),
            Message::ReportRow,
        );
        let header = container(
            text(header)
                .font(fonts::PAGE)
                .size(13)
                .color(pal.text_quiet)
                .wrapping(text::Wrapping::None),
        )
        .padding([2, 6]);
        let rows: Element<'a, Message> = if lines.is_empty() {
            container(hint(pal, "Nothing to report yet."))
                .height(Length::Fill)
                .into()
        } else {
            scrollable(list)
                .id(REPORT_ROWS)
                .on_scroll(Message::ReportScrolled)
                .height(Length::Fill)
                .into()
        };
        let content = column![
            row![
                title(pal, "Reports"),
                iced::widget::space::horizontal(),
                button(text("Close").font(fonts::UI).size(13))
                    .on_press(Message::ReportClose)
                    .padding([4, 12]),
            ]
            .align_y(iced::Alignment::Center),
            tabs,
            header,
            rows,
            hint(
                pal,
                format!(
                    "Up and Down: choose  ·  Enter: go to the scene  ·  Ctrl+Tab: next report  ·  {m}+C: copy as CSV  ·  Esc: close"
                ),
            ),
        ]
        .spacing(10)
        .max_width(1100.0);
        container(content)
            .padding(24)
            .center_x(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(pal.panel_alt)),
                ..container::Style::default()
            })
            .into()
    }
}

/// A table as lines: the header, then each row, its columns padded in the
/// page's monospaced font and counts right-aligned. A long cell is cut to
/// its column with "…"; the CSV copy keeps all of it.
fn table_lines(table: &Table) -> (String, Vec<String>) {
    let cut = |c: &str| {
        if c.chars().count() > WIDEST {
            ellipsize(c, WIDEST)
        } else {
            c.to_string()
        }
    };
    let header: Vec<String> = table.header.iter().map(|h| h.replace('_', " ")).collect();
    let widths: Vec<usize> = header
        .iter()
        .enumerate()
        .map(|(k, h)| {
            table
                .rows
                .iter()
                .map(|row| cut(&row[k]).chars().count())
                .chain(std::iter::once(h.chars().count()))
                .max()
                .unwrap_or(4)
        })
        .collect();
    let line = |cells: &[String], is_header: bool| -> String {
        let mut out = String::new();
        for (k, c) in cells.iter().enumerate() {
            let c = if is_header { c.clone() } else { cut(c) };
            let w = widths[k];
            if k > 0 {
                out.push_str("  ");
            }
            if report::numeric(table.header[k]) {
                out.push_str(&format!("{c:>w$}"));
            } else {
                out.push_str(&format!("{c:<w$}"));
            }
        }
        out.trim_end().to_string()
    };
    let rows = table.rows.iter().map(|r| line(r, false)).collect();
    (line(&header, true), rows)
}
