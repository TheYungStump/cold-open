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
        });
    }

    /// Shows another tab.
    pub(super) fn reports_tab(&mut self, tab: usize) {
        if let Some(r) = &mut self.reports {
            r.tab = tab % Kind::ALL.len();
            r.selected = 0;
        }
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
                self.reports_tab(tab);
            }
            Key::Named(Named::ArrowDown) => {
                r.selected = (r.selected + 1).min(rows.saturating_sub(1))
            }
            Key::Named(Named::ArrowUp) => r.selected = r.selected.saturating_sub(1),
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
        let table = r.table().cloned().unwrap_or_default();
        let widths: Vec<f32> = table
            .header
            .iter()
            .enumerate()
            .map(|(k, h)| {
                let longest = table
                    .rows
                    .iter()
                    .map(|row| row[k].chars().count())
                    .chain(std::iter::once(h.len()))
                    .max()
                    .unwrap_or(4)
                    .min(WIDEST);
                longest as f32 * 7.6 + 12.0
            })
            .collect();
        let cells = |cells: Vec<String>, header: bool, color: iced::Color| {
            cells
                .into_iter()
                .enumerate()
                .fold(row![].spacing(0), |acc, (k, c)| {
                    let numeric = report::numeric(table.header[k]);
                    // A long cell, such as a speaker's scenes, is cut to its
                    // column; the CSV copy keeps all of it.
                    let c = if header {
                        c.replace('_', " ")
                    } else if c.chars().count() > WIDEST {
                        ellipsize(&c, WIDEST)
                    } else {
                        c
                    };
                    let mut t = text(c)
                        .font(if header { fonts::UI_BOLD } else { fonts::UI })
                        .size(12)
                        .color(color)
                        .wrapping(text::Wrapping::None)
                        .width(Length::Fixed(widths[k]));
                    if numeric {
                        t = t.align_x(iced::alignment::Horizontal::Right);
                    }
                    acc.push(container(t).padding([2, 6]))
                })
        };
        let mut list = column![].spacing(1);
        for (i, cells_row) in table.rows.iter().enumerate() {
            let selected = i == r.selected;
            let line = button(cells(cells_row.clone(), false, pal.text))
                .on_press(Message::ReportRow(i))
                .padding(0)
                .style(move |_t: &Theme, _s| button::Style {
                    background: Some(Background::Color(if selected {
                        pal.accent_soft
                    } else {
                        pal.panel_alt
                    })),
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
        if table.rows.is_empty() {
            list = list.push(hint(pal, "Nothing to report yet."));
        }
        column![
            title(pal, "Reports"),
            tabs,
            cells(
                table.header.iter().map(|h| h.to_string()).collect(),
                true,
                pal.text_quiet
            ),
            scrollable(list).height(Length::Fixed(440.0)),
            hint(
                pal,
                format!(
                    "Up and Down: choose  ·  Enter: go to the scene  ·  Ctrl+Tab: next report  ·  {m}+C: copy as CSV  ·  Esc: close"
                ),
            ),
        ]
        .spacing(10)
        .into()
    }
}
