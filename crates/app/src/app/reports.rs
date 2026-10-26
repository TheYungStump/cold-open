//! The report view (editor.md §20): the scenes, characters and locations
//! reports in three tabs, a row going to its scene.

use std::collections::BTreeMap;

use coldopen_core::report::{self, Kind, Table};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Background, Border, Element, Length, Task, Theme};
use serde_json::json;

use super::view::{ellipsize, hint, title};

/// The widest a column grows, in characters.
const WIDEST: usize = 36;
/// A row's height in pixels. Rows are all one height, so only those in
/// sight are built and drawn.
const ROW_H: f32 = 22.0;
/// The rows' area's height until it first reports its own.
const ROWS_H: f32 = 600.0;
/// Rows drawn beyond each edge of the visible part.
const SPARE: usize = 8;
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
        let tables = Kind::ALL
            .iter()
            .map(|&k| {
                self.run(k.command(), json!({}))
                    .map(|r| report::table(k, &r, &index))
                    .unwrap_or_default()
            })
            .collect();
        self.reports = Some(ReportView {
            tab: 0,
            selected: 0,
            tables,
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
    pub(super) fn reports_view(&self, pal: Palette, r: &ReportView) -> Element<'_, Message> {
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
        let empty = Table::default();
        let table = r.table().unwrap_or(&empty);
        // Each row is one line of text, its columns padded in the page's
        // monospaced font. Cells drawn one by one each made a region of
        // their own to repaint when the rows scrolled, and the CPU
        // renderer filled a window-sized clip mask for every region:
        // 244 ms a step on Big Fish (the manual test of 2026-10-26).
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
        let cells = |cells: &[String], is_header: bool, color: iced::Color| {
            container(
                text(line(cells, is_header))
                    .font(fonts::PAGE)
                    .size(13)
                    .color(color)
                    .wrapping(text::Wrapping::None),
            )
            .padding([2, 6])
        };
        // Only the rows in sight, and a few either side; spaces stand for
        // the rest, so the scrolling area keeps its whole height.
        let n = table.rows.len();
        let first = ((r.offset as f32 / ROW_H) as usize)
            .saturating_sub(SPARE)
            .min(n);
        let last = (((r.offset + r.height) as f32 / ROW_H).ceil() as usize + SPARE).min(n);
        let mut list = column![iced::widget::space().height(Length::Fixed(first as f32 * ROW_H))];
        for (i, cells_row) in table.rows.iter().enumerate().take(last).skip(first) {
            let selected = i == r.selected;
            let line = button(cells(cells_row, false, pal.text))
                .on_press(Message::ReportRow(i))
                .padding(0)
                .width(Length::Fill)
                .height(Length::Fixed(ROW_H))
                .style(move |_t: &Theme, _s| button::Style {
                    // Only the selected row paints: fewer regions to
                    // repaint while the rows scroll.
                    background: selected.then_some(Background::Color(pal.accent_soft)),
                    text_color: pal.text,
                    border: Border {
                        color: if selected {
                            pal.focus
                        } else {
                            iced::Color::TRANSPARENT
                        },
                        width: 1.0,
                        radius: 3.0.into(),
                    },
                    ..button::Style::default()
                });
            list = list.push(line);
        }
        list = list.push(iced::widget::space().height(Length::Fixed((n - last) as f32 * ROW_H)));
        if table.rows.is_empty() {
            list = list.push(hint(pal, "Nothing to report yet."));
        }
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
            cells(&header, true, pal.text_quiet),
            scrollable(list)
                .id(REPORT_ROWS)
                .on_scroll(Message::ReportScrolled)
                .height(Length::Fill),
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
