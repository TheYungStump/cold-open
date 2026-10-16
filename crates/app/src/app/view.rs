//! The app's view: the window's panels and overlays (docs/specs/editor.md
//! §2). It reads the app's state and never changes it.

use iced::widget::{
    button, column, container, mouse_area, opaque, row, scrollable, space, stack, text, text_input,
};
use iced::{Background, Border, Center, Color, Element, Fill, Length, Theme};

use super::Sheet;
use super::file_ops::ago;
use super::{
    App, EXPORT, ExportForm, FIND, Focus, Message, PAGES, PALETTE, Prompt, Recovery, Reply,
};
use crate::editor::flow::kind_label;
use crate::fonts;
use crate::page_view::PageView;
use crate::shield::shield;
use crate::theme::Palette;
use std::time::Duration;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let pal = self.palette_colors();
        let main = match (self.d.cached_view(), self.d.session()) {
            (Some(view), Some(session)) => self.editor_view(pal, view, &session.doc),
            _ => self.start_screen(pal),
        };
        // Behind the palette or a dialog, a click only closes it, and the
        // wheel scrolls nothing (editor.md §2).
        let covered = self.palette.is_some() || self.modal();
        let main = shield(main, covered.then_some(Message::ClickAway), true);
        let mut layers = stack![main];
        if let Some(m) = self.dialog(pal) {
            layers = layers.push(m);
        }
        if self.palette.is_some() {
            layers = layers.push(self.palette_view(pal));
        }
        layers.into()
    }

    fn editor_view<'a>(
        &'a self,
        pal: Palette,
        view: &'a coldopen_core::View,
        doc: &'a coldopen_core::Document,
    ) -> Element<'a, Message> {
        let page = PageView {
            view,
            doc,
            draft: if self.editor.draft {
                self.editor.draft_index.draft.as_ref()
            } else {
                None
            },
            palette: pal,
            zoom: self.settings.zoom,
            focused: self.focus == Focus::Page && self.palette.is_none() && !self.modal(),
            caret: self.caret_spot,
            caret_on: self.caret_on,
            preedit: self.editor.preedit.as_ref().map(|p| p.text.as_str()),
            completion: self.editor.completion.as_ref(),
            selection: self.sel_range,
            matches: &self.match_ranges,
            marked: &self.marked_elems,
            coverage: &*self.fonts,
            on_event: Box::new(Message::Page),
        };
        let pages = scrollable(Element::from(page))
            .id(PAGES)
            .on_scroll(Message::Scrolled)
            .width(Fill)
            .height(Fill);
        // While the find bar or the autocomplete list is open, a click
        // anywhere else only closes it (editor.md §2). The page itself
        // handles clicks around the autocomplete list.
        let away =
            (self.find.is_some() || self.editor.completion.is_some()).then_some(Message::ClickAway);
        let away_pages = self.find.is_some().then_some(Message::ClickAway);
        let mut body = row![];
        if self.settings.navigator && !self.focus_mode {
            body = body.push(shield(self.navigator(pal), away.clone(), false));
        }
        body = body.push(shield(pages, away_pages, false));
        let mut main = column![];
        if !self.focus_mode {
            main = main.push(shield(self.hint_bar(pal), away.clone(), false));
        }
        if !self.banners.is_empty() {
            main = main.push(shield(self.banner(pal), away.clone(), false));
        }
        if !self.leftover.is_empty() {
            main = main.push(shield(self.leftover_bar(pal), away.clone(), false));
        }
        if self.find.is_some() {
            main = main.push(self.find_bar(pal));
        }
        main = main.push(body.height(Fill));
        if !self.focus_mode {
            main = main.push(shield(self.status_bar(pal), away, false));
        }
        main.into()
    }

    /// No script is open: start one, open one, or pick a recent file.
    fn start_screen(&self, pal: Palette) -> Element<'_, Message> {
        let keys = |a: &str| {
            self.keymap
                .keys_for(a)
                .into_iter()
                .next()
                .unwrap_or_default()
        };
        let mut col = column![
            text("No script is open")
                .font(fonts::UI_BOLD)
                .size(20)
                .color(pal.text),
            row![
                button(text(format!("New script  {}", keys("app.new"))).font(fonts::UI))
                    .on_press(Message::Action("app.new".into()))
                    .padding([6, 14]),
                button(text(format!("Open…  {}", keys("app.open"))).font(fonts::UI))
                    .on_press(Message::Action("app.open".into()))
                    .padding([6, 14]),
            ]
            .spacing(10),
        ]
        .spacing(14)
        .align_x(Center);
        if !self.settings.recent.is_empty() {
            col = col.push(
                text(format!(
                    "Recent files (also in the palette, {})",
                    keys("app.palette")
                ))
                .font(fonts::UI)
                .size(12)
                .color(pal.text_quiet),
            );
            for p in &self.settings.recent {
                col = col.push(
                    button(text(file_name(p)).font(fonts::UI).size(13))
                        .on_press(Message::OpenPath(p.clone()))
                        .style(button::text)
                        .padding([2, 8]),
                );
            }
        }
        let mut page = column![];
        if !self.banners.is_empty() {
            page = page.push(self.banner(pal));
        }
        if !self.leftover.is_empty() {
            page = page.push(self.leftover_bar(pal));
        }
        page = page.push(container(col).center(Fill));
        container(page)
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(pal.desk)),
                ..container::Style::default()
            })
            .into()
    }

    /// Unsaved work from an earlier session, offered at start-up
    /// (editor.md §11).
    fn leftover_bar(&self, pal: Palette) -> Element<'_, Message> {
        let mut r = row![
            text("Unsaved work from an earlier session:")
                .font(fonts::UI)
                .size(13)
                .color(pal.text),
        ]
        .spacing(8)
        .align_y(Center);
        for p in self.leftover.iter().take(4) {
            r = r.push(
                button(
                    text(format!("Reopen {}", file_name(p)))
                        .font(fonts::UI)
                        .size(12),
                )
                .on_press(Message::OpenPath(p.clone()))
                .padding([3, 10]),
            );
        }
        if self.leftover.len() > 4 {
            r = r.push(
                text(format!(
                    "and {} more in the palette",
                    self.leftover.len() - 4
                ))
                .font(fonts::UI)
                .size(12)
                .color(pal.text_quiet),
            );
        }
        r = r.push(space::horizontal());
        r = r.push(
            button(text("Dismiss").font(fonts::UI).size(12))
                .on_press(Message::DismissLeftover)
                .padding([3, 10]),
        );
        container(r)
            .padding([6, 14])
            .width(Fill)
            .style(move |_| bar(pal.accent_soft, pal.border))
            .into()
    }

    /// The open dialog, if any: unsaved changes, a lossy save, recovered
    /// work, or PDF export (editor.md §11, §13).
    fn dialog(&self, pal: Palette) -> Option<Element<'_, Message>> {
        let content = if let Some(p) = &self.prompt {
            self.prompt_view(pal, p)
        } else if let Some(r) = &self.recovery {
            self.recovery_view(pal, r)
        } else if let Some(f) = &self.export {
            export_view(pal, f)
        } else {
            self.sheet_view(pal, self.sheet?)
        };
        let card = container(content)
            .padding(18)
            .width(Length::Fixed(520.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(pal.panel_alt)),
                border: Border {
                    color: if pal.high_contrast {
                        pal.focus
                    } else {
                        pal.border
                    },
                    width: if pal.high_contrast { 2.0 } else { 1.0 },
                    radius: 10.0.into(),
                },
                ..container::Style::default()
            });
        Some(
            container(opaque(card))
                .center(Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.3))),
                    ..container::Style::default()
                })
                .into(),
        )
    }

    /// The script's file name, or "this script" when it has none yet.
    fn script_name(&self) -> String {
        if self.untitled() {
            return "this script".into();
        }
        self.d
            .session()
            .and_then(|s| s.path.as_deref())
            .map_or_else(|| "this script".into(), file_name)
    }

    fn prompt_view(&self, pal: Palette, p: &Prompt) -> Element<'_, Message> {
        match p {
            Prompt::Unsaved(_) => {
                let (t, b) = if self.untitled() {
                    (
                        "Save this script?".to_string(),
                        "It has never been saved. If you don't save it, it is gone.",
                    )
                } else {
                    (
                        format!("Save the changes to {}?", self.script_name()),
                        "If you don't save, the changes since the last save are lost.",
                    )
                };
                column![
                    title(pal, t),
                    body(pal, b),
                    row![
                        dialog_button("Save", Message::Reply(Reply::Save), true),
                        dialog_button("Don't save", Message::Reply(Reply::DontSave), false),
                        dialog_button("Cancel", Message::Reply(Reply::Cancel), false),
                    ]
                    .spacing(8),
                    hint(pal, "Enter: Save  ·  D: Don't save  ·  Esc: Cancel"),
                ]
                .spacing(12)
                .into()
            }
            Prompt::Lossy { message, .. } => column![
                title(pal, "Fountain can't hold everything in this script"),
                body(pal, message.clone()),
                row![
                    dialog_button("Save anyway", Message::Reply(Reply::Save), false),
                    dialog_button("Cancel", Message::Reply(Reply::Cancel), true),
                ]
                .spacing(8),
                hint(pal, "Enter: Save anyway  ·  Esc: Cancel"),
            ]
            .spacing(12)
            .into(),
        }
    }

    fn sheet_view(&self, pal: Palette, sheet: Sheet) -> Element<'_, Message> {
        let m = if crate::keymap::MAC { "Cmd" } else { "Ctrl" };
        match sheet {
            Sheet::History => {
                let mut list = column![].spacing(3);
                let step = |label: &str, by: &coldopen_core::Author, quiet: bool| {
                    let who = if by.is_writer() {
                        String::new()
                    } else {
                        "  (agent)".to_string()
                    };
                    let color = if quiet {
                        pal.text_quiet
                    } else if by.is_writer() {
                        pal.text
                    } else {
                        pal.agent
                    };
                    text(format!("{label}{who}"))
                        .font(fonts::UI)
                        .size(13)
                        .color(color)
                };
                let mut n = 0;
                if let Some(s) = self.d.session() {
                    let redo: Vec<_> = s.redo_steps().collect();
                    for (_, label, by) in redo.into_iter().rev() {
                        list = list.push(step(&format!("Redo: {label}"), by, true));
                        n += 1;
                    }
                    for (_, label, by) in s.undo_steps() {
                        list = list.push(step(label, by, false));
                        n += 1;
                    }
                }
                if n == 0 {
                    list = list.push(hint(pal, "Nothing to undo yet."));
                }
                column![
                    title(pal, "Undo history"),
                    hint(
                        pal,
                        "Newest first. Steps that redo would bring back are greyed."
                    ),
                    scrollable(list).height(Length::Fixed(320.0)),
                    hint(
                        pal,
                        format!("{m}+Z: undo  ·  {m}+Shift+Z: redo  ·  Esc: close")
                    ),
                ]
                .spacing(10)
                .into()
            }
            Sheet::About => column![
                title(pal, format!("Cold Open {}", env!("CARGO_PKG_VERSION"))),
                body(
                    pal,
                    "Free software under the GNU General Public License, version 3 or \
                     later. The format and layout libraries are also available under the \
                     MIT or Apache-2.0 licenses."
                ),
                hint(
                    pal,
                    "Bundled fonts: Courier Prime for the page, and Atkinson Hyperlegible \
                     Next (subset to Latin) for the interface, both under the SIL Open Font \
                     License 1.1:"
                ),
                scrollable(
                    text(format!(
                        "Courier Prime\n\n{}\n\nAtkinson Hyperlegible Next\n\n{}",
                        coldopen_fonts::LICENSE,
                        coldopen_fonts::UI_LICENSE
                    ))
                    .font(fonts::PAGE)
                    .size(11)
                    .color(pal.text)
                )
                .height(Length::Fixed(300.0)),
                hint(pal, "Esc: close"),
            ]
            .spacing(10)
            .into(),
        }
    }

    fn recovery_view(&self, pal: Palette, r: &Recovery) -> Element<'_, Message> {
        let when = r
            .last
            .map_or_else(String::new, |t| format!(", the last one {}", ago(t)));
        let changes = if r.commands == 1 { "change" } else { "changes" };
        let mut col = column![
            title(pal, "Unsaved work was recovered"),
            body(
                pal,
                format!(
                    "Cold Open found {} {changes} to {} that were not saved{when}. \
                     They are back in the script.",
                    r.commands,
                    self.script_name()
                )
            ),
        ]
        .spacing(10);
        if !r.scenes.is_empty() {
            let mut list = r
                .scenes
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            if r.scenes.len() > 6 {
                list.push_str(&format!("\nand {} more", r.scenes.len() - 6));
            }
            col = col.push(hint(pal, "Scenes with recovered changes:"));
            col = col.push(text(list).font(fonts::PAGE).size(12).color(pal.text));
        }
        if r.external_change {
            col = col.push(
                text(
                    "The file was changed by another program since. Saving will replace \
                     that version.",
                )
                .font(fonts::UI_BOLD)
                .size(13)
                .color(pal.danger),
            );
        }
        col = col.push(hint(
            pal,
            if r.never_saved {
                "Discard starts a new script, because this one was never saved. A copy of                  the recovered work is kept."
            } else {
                "Discard goes back to the saved file. A copy of the recovered work is kept \
                 next to the script."
            },
        ));
        col = col.push(
            row![
                dialog_button("Keep", Message::RecoveryKeep, true),
                dialog_button("Discard", Message::RecoveryDiscard, false),
            ]
            .spacing(8),
        );
        col.push(hint(pal, "Enter: Keep  ·  D: Discard")).into()
    }

    fn hint_bar(&self, pal: Palette) -> Element<'_, Message> {
        let session = self.d.session();
        let name = session
            .and_then(|s| s.path.as_ref())
            .filter(|p| {
                !self
                    .dirs
                    .as_ref()
                    .is_some_and(|d| p.starts_with(d.unsaved()))
            })
            .and_then(|p| p.file_name())
            .map_or_else(
                || "Untitled".to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
        let dirty = session.is_some_and(|s| s.dirty);
        let kind = session
            .and_then(|s| s.doc.get(self.editor.caret.block))
            .map(|b| b.element.kind);
        let mut r = row![
            text(name).font(fonts::UI_BOLD).size(14).color(pal.text),
            text(if dirty { "edited" } else { "" })
                .font(fonts::UI)
                .size(13)
                .color(pal.text_quiet),
            space::horizontal(),
        ]
        .spacing(10)
        .align_y(Center);
        if let Some(k) = kind {
            let row_flow = self.editor.flow.row(k);
            r = r.push(
                container(
                    text(kind_label(k))
                        .font(fonts::UI_BOLD)
                        .size(13)
                        .color(pal.accent_text),
                )
                .padding([3, 12])
                .style(move |_| pill(pal.accent_soft)),
            );
            let mut hint = format!("Enter: {}", kind_label(row_flow.enter));
            if let Some(t) = row_flow.tab {
                hint.push_str(&format!("  ·  Tab: {}", kind_label(t)));
            }
            r = r.push(text(hint).font(fonts::UI).size(13).color(pal.text_quiet));
        }
        let palette_keys = self
            .keymap
            .keys_for("app.palette")
            .into_iter()
            .next()
            .unwrap_or_default();
        r = r.push(space::horizontal());
        r = r.push(
            container(
                text(palette_keys)
                    .font(fonts::UI)
                    .size(12)
                    .color(pal.text_quiet),
            )
            .padding([2, 8])
            .style(move |_| outline(pal.border)),
        );
        container(r)
            .padding([7, 14])
            .width(Fill)
            .style(move |_| bar(pal.panel_alt, pal.border))
            .into()
    }

    fn navigator(&self, pal: Palette) -> Element<'_, Message> {
        let current = self.current_scene_row();
        let mut list = column![
            container(
                text(format!(
                    "Scenes · {}",
                    self.nav.iter().filter(|r| !r.section).count()
                ))
                .font(fonts::UI)
                .size(12)
                .color(pal.text_quiet)
            )
            .padding([4, 12])
        ];
        for (i, r) in self.nav.iter().enumerate() {
            let selected = current == Some(i);
            let hovering = self.nav_drag.is_some() && self.nav_hover == Some(i);
            let label = if r.section {
                row![
                    text(r.heading.clone())
                        .font(fonts::UI_BOLD)
                        .size(12)
                        .color(pal.text_quiet)
                ]
                .padding([0, 4 * u16::from(r.depth.saturating_sub(1))])
            } else {
                let page = r.page.map_or_else(String::new, |p| p.to_string());
                let fg = if selected { pal.accent_text } else { pal.text };
                row![
                    text(r.number.clone())
                        .font(fonts::UI)
                        .size(12)
                        .color(pal.text_quiet)
                        .width(Length::Fixed(22.0)),
                    text(ellipsize(&r.heading, 24))
                        .font(fonts::UI)
                        .size(13)
                        .color(fg)
                        .wrapping(text::Wrapping::None)
                        .width(Fill),
                    text(page).font(fonts::UI).size(12).color(pal.text_quiet),
                ]
                .spacing(6)
            };
            let bg = if selected { pal.accent_soft } else { pal.panel };
            let rule = if hovering {
                pal.accent
            } else {
                Color::TRANSPARENT
            };
            let cell = container(label)
                .padding([6, 12])
                .width(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(bg)),
                    border: Border {
                        color: rule,
                        width: if hovering { 2.0 } else { 0.0 },
                        radius: 0.0.into(),
                    },
                    ..container::Style::default()
                });
            list = list.push(
                mouse_area(cell)
                    .on_press(Message::NavPress(i))
                    .on_release(Message::NavRelease(i))
                    .on_enter(Message::NavEnter(i)),
            );
        }
        list = list.push(
            container(
                text("Drag a scene, or Alt+Up/Down, to move it")
                    .font(fonts::UI)
                    .size(11)
                    .color(pal.text_quiet),
            )
            .padding([6, 12]),
        );
        let focused = self.focus == Focus::Navigator;
        container(scrollable(list).height(Fill))
            .width(Length::Fixed(250.0))
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(pal.panel)),
                border: Border {
                    color: if focused { pal.focus } else { pal.border },
                    width: if focused { 2.0 } else { 1.0 },
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            })
            .into()
    }

    fn status_bar(&self, pal: Palette) -> Element<'_, Message> {
        let total = self.d.cached_view().map_or(0, |v| v.layout().pages.len());
        let page = self.caret_spot.map_or(1, |s| s.page + 1);
        let scene = self.current_scene_row().and_then(|i| self.nav.get(i));
        let scene_txt = scene.map_or_else(String::new, |r| {
            format!("Scene {} · {}", r.number, eighths(r.eighths))
        });
        let (journal, jcolor) = match self.d.journal_status() {
            _ if self.saving.is_some() => ("Saving…".to_string(), pal.text_quiet),
            Some(st) if st.error.is_some() => ("Not protected".to_string(), pal.danger),
            Some(st) if st.oldest.is_some_and(|a| a > Duration::from_millis(250)) => {
                ("Protecting…".to_string(), pal.text_quiet)
            }
            Some(_) => ("Protected".to_string(), pal.success),
            None => (String::new(), pal.text_quiet),
        };
        let page_txt = if self.editor.draft {
            "Draft view".to_string()
        } else {
            format!("Page {page} of {total}")
        };
        let mut r = row![
            text(page_txt)
                .font(fonts::UI)
                .size(12)
                .color(pal.text_quiet),
            text(scene_txt)
                .font(fonts::UI)
                .size(12)
                .color(pal.text_quiet),
            text(journal).font(fonts::UI).size(12).color(jcolor),
            space::horizontal(),
        ]
        .spacing(16)
        .align_y(Center);
        // Visible whenever an agent is attached, with one-click disconnect
        // and the off switch (editor.md §12).
        if let Some(agents) = self.agent_status() {
            let small = |label: &'static str, action: &'static str| {
                button(text(label).font(fonts::UI).size(11))
                    .on_press(Message::Action(action.into()))
                    .padding([1, 8])
                    .style(button::secondary)
            };
            r = r.push(
                container(
                    row![
                        text(agents).font(fonts::UI_BOLD).size(12).color(pal.agent),
                        small("Disconnect", "app.agent_disconnect"),
                        small("Turn off agent access", "app.agent_access"),
                    ]
                    .spacing(8)
                    .align_y(Center),
                )
                .padding([1, 8])
                .style(move |_| outline(pal.agent)),
            );
        }
        r = r.push(
            text(format!("{}%", (self.settings.zoom * 100.0).round() as u32))
                .font(fonts::UI)
                .size(12)
                .color(pal.text_quiet),
        );
        container(r)
            .padding([5, 14])
            .width(Fill)
            .style(move |_| bar(pal.panel_alt, pal.border))
            .into()
    }

    fn banner(&self, pal: Palette) -> Element<'_, Message> {
        let msg = self.banners.last().cloned().unwrap_or_default();
        let more = if self.banners.len() > 1 {
            format!("  (+{} more)", self.banners.len() - 1)
        } else {
            String::new()
        };
        container(
            row![
                text(format!("{msg}{more}"))
                    .font(fonts::UI)
                    .size(13)
                    .color(pal.text)
                    .width(Fill),
                button(text("Dismiss").font(fonts::UI).size(12))
                    .on_press(Message::DismissBanner)
                    .padding([3, 10]),
            ]
            .spacing(10)
            .align_y(Center),
        )
        .padding([6, 14])
        .width(Fill)
        .style(move |_| bar(pal.accent_soft, pal.border))
        .into()
    }

    fn find_bar(&self, pal: Palette) -> Element<'_, Message> {
        let Some(f) = &self.find else {
            return space().into();
        };
        let count = if f.query.is_empty() {
            String::new()
        } else if f.matches.is_empty() {
            "No matches".to_string()
        } else {
            format!("{} of {}", f.current + 1, f.matches.len())
        };
        let toggle = |label: &'static str, on: bool, msg: Message| {
            button(text(label).font(fonts::UI).size(12))
                .on_press(msg)
                .padding([3, 8])
                .style(move |theme: &Theme, status| {
                    let mut s = button::secondary(theme, status);
                    if on {
                        s.background = Some(Background::Color(pal.accent_soft));
                        s.text_color = pal.accent_text;
                    }
                    s
                })
        };
        let mut r = row![
            text_input("Find", &f.query)
                .id(FIND)
                .on_input(Message::FindInput)
                .on_submit(Message::Action("edit.find_next".into()))
                .font(fonts::UI)
                .size(13)
                .width(Length::Fixed(260.0)),
            text(count).font(fonts::UI).size(12).color(pal.text_quiet),
            toggle("Match case", f.match_case, Message::FindCase),
            toggle("This scene", f.scene_only, Message::FindScope),
        ]
        .spacing(8)
        .align_y(Center);
        if f.replace_open {
            r = r.push(
                text_input("Replace with", &f.replace)
                    .on_input(Message::ReplaceInput)
                    .font(fonts::UI)
                    .size(13)
                    .width(Length::Fixed(200.0)),
            );
            r = r.push(
                button(text("Replace").font(fonts::UI).size(12))
                    .on_press(Message::ReplaceOne)
                    .padding([3, 8]),
            );
            r = r.push(
                button(text("Replace all").font(fonts::UI).size(12))
                    .on_press(Message::ReplaceAll)
                    .padding([3, 8]),
            );
        }
        container(r)
            .padding([6, 14])
            .width(Fill)
            .style(move |_| bar(pal.panel, pal.border))
            .into()
    }

    fn palette_view(&self, pal: Palette) -> Element<'_, Message> {
        let Some(p) = &self.palette else {
            return space().into();
        };
        let mut col = column![].spacing(6);
        if let Some((name, json)) = &p.params {
            col = col.push(
                text(format!("{name} — params as JSON"))
                    .font(fonts::UI_BOLD)
                    .size(14)
                    .color(pal.text),
            );
            col = col.push(
                text_input("{}", json)
                    .id(PALETTE)
                    .on_input(Message::ParamsInput)
                    .on_submit(Message::ParamsRun)
                    .font(fonts::PAGE)
                    .size(13),
            );
            if let Some(r) = &p.response {
                col = col.push(
                    scrollable(text(r.clone()).font(fonts::PAGE).size(12).color(pal.text))
                        .height(Length::Fixed(260.0)),
                );
            }
        } else {
            col = col.push(
                text_input("Type a command", &p.query)
                    .id(PALETTE)
                    .on_input(Message::PaletteInput)
                    .on_submit(Message::PaletteRun(p.selected))
                    .font(fonts::UI)
                    .size(14),
            );
            let items = self.palette_items();
            let sel = p.selected.min(items.len().saturating_sub(1));
            for (i, it) in items.into_iter().enumerate() {
                let keys = self.keymap.keys_for(&it.action).join(", ");
                let selected = i == sel;
                let fg = if selected { pal.accent_text } else { pal.text };
                let item = row![
                    text(it.title)
                        .font(fonts::UI)
                        .size(13)
                        .color(fg)
                        .width(Fill),
                    text(it.detail)
                        .font(fonts::UI)
                        .size(11)
                        .color(pal.text_quiet)
                        .wrapping(text::Wrapping::None),
                    text(keys).font(fonts::UI).size(11).color(pal.text_quiet),
                ]
                .spacing(10);
                col = col.push(
                    button(item)
                        .on_press(Message::PaletteRun(i))
                        .padding([5, 8])
                        .width(Fill)
                        .style(move |_t: &Theme, _s| button::Style {
                            background: Some(Background::Color(if selected {
                                pal.accent_soft
                            } else {
                                Color::TRANSPARENT
                            })),
                            text_color: fg,
                            border: Border {
                                radius: 4.0.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }),
                );
            }
        }
        let card = container(col)
            .padding(14)
            .width(Length::Fixed(560.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(pal.panel_alt)),
                border: Border {
                    color: pal.border,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..container::Style::default()
            });
        container(column![space().height(Length::Fixed(80.0)), opaque(card)].align_x(Center))
            .width(Fill)
            .height(Fill)
            .align_x(Center)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.25))),
                ..container::Style::default()
            })
            .into()
    }
}

fn pill(bg: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: 999.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

fn outline(c: Color) -> container::Style {
    container::Style {
        border: Border {
            color: c,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..container::Style::default()
    }
}

fn bar(bg: Color, edge: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: edge,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

fn ellipsize(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}

fn export_view(pal: Palette, f: &ExportForm) -> Element<'_, Message> {
    let toggle = |label: &'static str, on: bool, m: Message| {
        button(
            text(format!("{label}: {}", if on { "on" } else { "off" }))
                .font(fonts::UI)
                .size(13),
        )
        .on_press(m)
        .padding([4, 10])
        .style(if on {
            button::primary
        } else {
            button::secondary
        })
    };
    let m = if crate::keymap::MAC { "Cmd" } else { "Ctrl" };
    column![
        title(pal, "Export a PDF"),
        row![
            toggle("Scene numbers", f.scene_numbers, Message::ExportNumbers),
            toggle("Title page", f.title_page, Message::ExportTitle),
        ]
        .spacing(8),
        text_input("Watermark (optional)", &f.watermark)
            .id(EXPORT)
            .on_input(Message::ExportWatermark)
            .on_submit(Message::ExportGo)
            .font(fonts::UI)
            .size(13),
        row![
            dialog_button("Choose where to save…", Message::ExportGo, true),
            dialog_button("Cancel", Message::ExportCancel, false),
        ]
        .spacing(8),
        hint(
            pal,
            format!(
                "{m}+1: scene numbers  ·  {m}+2: title page  ·  Enter: choose where to save  ·  Esc: cancel"
            )
        ),
    ]
    .spacing(12)
    .into()
}

fn title<'a>(pal: Palette, t: impl text::IntoFragment<'a>) -> text::Text<'a, Theme> {
    text(t).font(fonts::UI_BOLD).size(16).color(pal.text)
}

fn body<'a>(pal: Palette, t: impl text::IntoFragment<'a>) -> text::Text<'a, Theme> {
    text(t).font(fonts::UI).size(13).color(pal.text)
}

fn hint<'a>(pal: Palette, t: impl text::IntoFragment<'a>) -> text::Text<'a, Theme> {
    text(t).font(fonts::UI).size(12).color(pal.text_quiet)
}

fn dialog_button(
    label: &'static str,
    m: Message,
    primary: bool,
) -> button::Button<'static, Message> {
    button(text(label).font(fonts::UI).size(13))
        .on_press(m)
        .padding([5, 14])
        .style(if primary {
            button::primary
        } else {
            button::secondary
        })
}

fn file_name(p: &std::path::Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// A length in eighths of a page, as scripts write it.
pub fn eighths(n: u64) -> String {
    match (n / 8, n % 8) {
        (0, r) => format!("{r}/8"),
        (w, 0) => format!("{w}"),
        (w, r) => format!("{w} {r}/8"),
    }
}
