//! Tracked changes in the app (docs/specs/editor.md §17): settling them
//! from the caret, the scene or the whole script, moving between them, the
//! two switches, the writer's name, and the changes panel. Every change to
//! the script is still a command; this module decides which one to send.

use coldopen_core::changes::ByKind;
use coldopen_core::{Block, BlockId, Command, Document};
use coldopen_screenplay::{ChangeId, ElementKind};
use iced::widget::{
    button, column, container, mouse_area, pick_list, rich_text, row, scrollable, space, span,
    text, text_input,
};
use iced::{Background, Border, Center, Color, Element, Fill, Font, Length, Task};
use serde_json::{Value, json};

use super::file_ops::ago;
use super::view::{body, dialog_button, ellipsize, hint, title};
use super::{App, Focus, Message, Prompt};
use crate::editor::Caret;
use crate::fonts;
use crate::page_view::change_color;
use crate::theme::Palette;

/// The id of the writer's-name field.
pub(super) const NAME: &str = "author-name";

/// The author filter's first choice.
pub const ALL_AUTHORS: &str = "All authors";

/// The name the writer's changes carry until they give one.
pub const DEFAULT_NAME: &str = "Writer";

/// What waits for the writer's name (editor.md §17.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameThen {
    /// Turn the writer's tracking on.
    TrackMine,
    /// Nothing more: asked on opening a script, or from the palette.
    Nothing,
}

/// One row of the changes panel (editor.md §17.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeRow {
    pub id: ChangeId,
    pub author: String,
    /// When it was made, in ms since the epoch; 0 when unknown.
    pub t: u64,
    pub what: &'static str,
    /// The heading of the scene it starts in.
    pub scene: Option<BlockId>,
    pub inserted: String,
    pub deleted: String,
}

/// The changes panel's rows, its chosen row and its filter.
#[derive(Debug, Default)]
pub struct ChangesPanel {
    pub rows: Vec<ChangeRow>,
    /// The chosen row, among those the filter shows.
    pub selected: usize,
    /// Only this author's changes.
    pub filter: Option<String>,
    /// The revision the rows were read at.
    rev: Option<u64>,
}

impl ChangesPanel {
    /// The rows the filter shows.
    pub fn visible(&self) -> Vec<&ChangeRow> {
        self.rows
            .iter()
            .filter(|r| self.filter.as_ref().is_none_or(|f| *f == r.author))
            .collect()
    }

    /// The authors with changes, in order of their first row.
    pub fn authors(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for r in &self.rows {
            if !out.contains(&r.author) {
                out.push(r.author.clone());
            }
        }
        out
    }

    /// The chosen row's change.
    pub fn chosen(&self) -> Option<ChangeId> {
        self.visible().get(self.selected).map(|r| r.id)
    }
}

/// A block's own mark: the one its margin label shows.
pub fn block_change(b: &Block) -> Option<ChangeId> {
    let m = &b.marks;
    m.deleted
        .or(m.inserted)
        .or_else(|| m.retyped.first().map(|r| r.change))
        .or(m.split)
        .or(m.joined)
        .or_else(|| m.moved.first().map(|r| r.change))
}

/// Where each change's marks start, in document order, a run of marks of
/// one change listed once: (block index, offset, change).
fn mark_spots(doc: &Document) -> Vec<(usize, usize, ChangeId)> {
    let mut out: Vec<(usize, usize, ChangeId)> = Vec::new();
    for (i, b) in doc.blocks.iter().enumerate() {
        let own = block_change(b).map(|c| (0, c));
        for (off, c) in own.into_iter().chain(b.element.text.mark_offsets()) {
            if out.last().is_none_or(|l| l.2 != c) {
                out.push((i, off, c));
            }
        }
    }
    out
}

/// The caret at the start of a change's first mark.
pub fn change_caret(doc: &Document, id: ChangeId) -> Option<Caret> {
    mark_spots(doc)
        .into_iter()
        .find(|s| s.2 == id)
        .map(|(i, offset, _)| Caret::at(doc.blocks[i].id, offset))
}

/// An author as the panel names them.
fn author_label(by: &Value) -> String {
    match (by["kind"].as_str(), by["name"].as_str()) {
        (_, Some(n)) => n.to_string(),
        (Some("writer"), None) => DEFAULT_NAME.into(),
        (Some(_), None) => "An agent".into(),
        _ => "Unknown".into(),
    }
}

/// What a change does, from `changes.list`'s kinds.
fn what(kinds: &[&str], blocks: usize) -> &'static str {
    let has = |k: &str| kinds.contains(&k);
    if has("move") {
        "Moved scene"
    } else if has("block_insert") {
        if blocks > 1 {
            "New blocks"
        } else {
            "New block"
        }
    } else if has("block_delete") {
        if blocks > 1 {
            "Deleted blocks"
        } else {
            "Deleted block"
        }
    } else if has("split") {
        "Split"
    } else if has("join") {
        "Joined"
    } else if has("retype") {
        "Changed element"
    } else if has("insert") && has("delete") {
        "Replaced"
    } else if has("insert") {
        "Inserted"
    } else {
        "Deleted"
    }
}

/// Reads `changes.list`'s rows.
pub fn rows(list: &Value) -> Vec<ChangeRow> {
    list["changes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let kinds: Vec<&str> = c["kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            Some(ChangeRow {
                id: serde_json::from_value(c["id"].clone()).ok()?,
                author: author_label(&c["by"]),
                t: c["t"].as_u64().unwrap_or(0),
                what: what(&kinds, c["blocks"].as_array().map_or(1, Vec::len)),
                scene: c["scene"].as_str().and_then(BlockId::parse),
                inserted: c["inserted"].as_str().unwrap_or("").to_string(),
                deleted: c["deleted"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Text struck through, for deleted words and headings.
pub(super) fn struck<'a>(t: String, size: f32, color: Color) -> Element<'a, Message> {
    let s: text::Span<'a, (), Font> = span(t)
        .font(fonts::UI)
        .size(size)
        .color(color)
        .strikethrough(true);
    rich_text([s]).wrapping(text::Wrapping::None).into()
}

impl App {
    /// The change at the caret: the mark just before it, else the one just
    /// after it, else the block's own mark; with a selection, the change of
    /// its first mark (editor.md §17.2).
    pub fn change_at_caret(&self) -> Option<ChangeId> {
        let doc = &self.d.session()?.doc;
        if self.editor.has_selection() {
            let (a, b) = self.editor.selection(doc);
            let (i, j) = (doc.index_of(a.block)?, doc.index_of(b.block)?);
            return (i..=j).find_map(|k| {
                let blk = &doc.blocks[k];
                let from = if k == i { a.offset } else { 0 };
                let to = if k == j { b.offset } else { usize::MAX };
                blk.element
                    .text
                    .first_change_in(from, to)
                    .or_else(|| block_change(blk))
            });
        }
        let b = doc.get(self.editor.caret.block)?;
        b.element
            .text
            .change_at(self.editor.caret.offset)
            .or_else(|| block_change(b))
    }

    /// Sends `changes.accept` or `changes.reject` and says what could not
    /// be carried out. The count settled, when it ran.
    fn settle(&mut self, accept: bool, params: Value) -> Option<u64> {
        let name = if accept {
            "changes.accept"
        } else {
            "changes.reject"
        };
        let cmd = match Command::from_parts(name, params) {
            Ok(c) => c,
            Err(e) => {
                self.banners.push(e.into());
                return None;
            }
        };
        match self.d.execute(cmd) {
            Ok(r) => {
                let skipped = r["skipped"].as_array().map_or(0, Vec::len);
                if skipped > 0 {
                    self.banners.push(
                        format!(
                            "{skipped} {} can't be carried out and {} pending: a scene's old place is gone, or blocks can no longer be joined.",
                            if skipped == 1 { "change" } else { "changes" },
                            if skipped == 1 { "stays" } else { "stay" },
                        )
                        .into(),
                    );
                }
                r["count"].as_u64()
            }
            Err(e) if e.code == "change_conflict" => {
                self.banners.push(
                    "This change can't be carried out: the scene's old place is gone, or the blocks it would join no longer can be. It stays pending."
                        .into(),
                );
                None
            }
            Err(e) => {
                self.banners.push(e.message.into());
                None
            }
        }
    }

    /// Accepts or rejects the change at the caret.
    pub(super) fn settle_here(&mut self, accept: bool) {
        match self.change_at_caret() {
            Some(id) => {
                self.settle(accept, json!({ "id": id }));
            }
            None => self
                .banners
                .push("There is no tracked change at the caret.".into()),
        }
    }

    /// Accepts or rejects the changes in the caret's scene.
    pub(super) fn settle_scene(&mut self, accept: bool) {
        let heading = self.d.session().and_then(|s| {
            let ci = s.doc.index_of(self.editor.caret.block)?;
            s.doc.blocks[..=ci]
                .iter()
                .rev()
                .find(|b| b.element.kind == ElementKind::SceneHeading)
                .map(|b| b.id)
        });
        let Some(id) = heading else {
            self.banners.push("The caret is not in a scene.".into());
            return;
        };
        match self.settle(accept, json!({ "scene": id })) {
            Some(0) => self
                .banners
                .push("This scene has no changes to review.".into()),
            Some(n) => self.banners.push(
                format!(
                    "{} {n} {} in this scene.",
                    if accept { "Accepted" } else { "Rejected" },
                    if n == 1 { "change" } else { "changes" }
                )
                .into(),
            ),
            None => {}
        }
    }

    /// Asks before accepting or rejecting every change (editor.md §17.2).
    pub(super) fn ask_settle_all(&mut self, accept: bool) {
        let count = self.d.session().map_or(0, |s| s.doc.pending().len());
        if count == 0 {
            self.banners.push("No changes to review.".into());
            return;
        }
        self.prompt = Some(Prompt::SettleAll { accept, count });
    }

    /// The writer said yes to accepting or rejecting every change.
    pub(super) fn settle_all(&mut self, accept: bool) {
        if let Some(n) = self.settle(accept, json!({ "all": true })) {
            self.banners.push(
                format!(
                    "{} {n} {}.",
                    if accept { "Accepted" } else { "Rejected" },
                    if n == 1 { "change" } else { "changes" }
                )
                .into(),
            );
        }
    }

    /// Moves the caret to the next or previous change in document order,
    /// round to the other end as find does.
    pub(super) fn change_step(&mut self, back: bool) {
        let Some(doc) = self.d.session().map(|s| &s.doc) else {
            return;
        };
        let spots = mark_spots(doc);
        let here = (
            doc.index_of(self.editor.caret.block).unwrap_or(0),
            self.editor.caret.offset,
        );
        let target = if back {
            spots
                .iter()
                .rev()
                .find(|s| (s.0, s.1) < here)
                .or(spots.last())
        } else {
            spots.iter().find(|s| (s.0, s.1) > here).or(spots.first())
        };
        let Some(&(i, offset, id)) = target else {
            self.banners.push("No changes to review.".into());
            return;
        };
        let c = Caret::at(doc.blocks[i].id, offset);
        self.editor.caret = c;
        self.editor.anchor = c;
        self.choose_row(id);
    }

    /// Chooses a change's row in the panel, when it shows one.
    fn choose_row(&mut self, id: ChangeId) {
        if let Some(i) = self.changes.visible().iter().position(|r| r.id == id) {
            self.changes.selected = i;
        }
    }

    /// Turns the writer's switch over, asking for their name first when
    /// turning it on with none set (editor.md §17.4).
    pub(super) fn track_mine(&mut self) -> Task<Message> {
        let Some(on) = self.d.session().map(|s| s.doc.tracking.writer) else {
            return Task::none();
        };
        if !on && self.settings.author_name.is_none() && !self.name_asked {
            return self.ask_name(NameThen::TrackMine);
        }
        self.set_track(Some(!on), None);
        Task::none()
    }

    /// Turns this script's agent switch over.
    pub(super) fn track_agents(&mut self) {
        if let Some(on) = self.d.session().map(|s| s.doc.tracking.agents) {
            self.set_track(None, Some(!on));
        }
    }

    fn set_track(&mut self, writer: Option<bool>, agents: Option<bool>) {
        let mut params = json!({});
        if let Some(w) = writer {
            params["writer"] = json!(w);
        }
        if let Some(a) = agents {
            params["agents"] = json!(a);
        }
        if self.run("changes.track", params).is_none() {
            return;
        }
        let said = match (writer, agents) {
            (Some(true), _) => "Your changes are tracked from now on.",
            (Some(false), _) => "Your changes are no longer tracked.",
            (_, Some(true)) => "Agents' edits to this script are tracked from now on.",
            (_, Some(false)) => {
                "Agents' edits to this script are no longer tracked: they apply at once."
            }
            (None, None) => return,
        };
        self.banners.push(said.into());
    }

    /// Opens the question for the writer's name.
    pub(super) fn ask_name(&mut self, then: NameThen) -> Task<Message> {
        self.name_asked = true;
        self.prompt = Some(Prompt::AuthorName {
            name: self
                .settings
                .author_name
                .clone()
                .unwrap_or_else(|| DEFAULT_NAME.into()),
            then,
        });
        Task::batch([
            iced::widget::operation::focus(NAME),
            iced::widget::operation::select_all(NAME),
        ])
    }

    /// Asks for the writer's name on opening a script that tracks their
    /// changes, once a session, when no name is set.
    pub(super) fn ask_name_on_open(&mut self) {
        let tracking = self.d.session().is_some_and(|s| s.doc.tracking.writer);
        if tracking
            && self.settings.author_name.is_none()
            && !self.name_asked
            && self.prompt.is_none()
        {
            self.name_asked = true;
            self.prompt = Some(Prompt::AuthorName {
                name: DEFAULT_NAME.into(),
                then: NameThen::Nothing,
            });
            self.focus_name = true;
        }
    }

    /// The navigator's rows as they are drawn.
    pub fn nav_rows(&self) -> &[super::NavRow] {
        &self.nav
    }

    /// The name the writer's tracked changes carry now.
    pub fn editing_as(&self) -> String {
        match self.d.writer() {
            coldopen_core::Author::Writer { name: Some(n) } => n.clone(),
            _ => DEFAULT_NAME.into(),
        }
    }

    /// The name question's answer: Enter keeps the name, or with the name
    /// cleared goes back to no name, which shows as "Writer"; Esc goes on
    /// as before.
    pub(super) fn name_done(&mut self, keep: bool) -> Task<Message> {
        let Some(Prompt::AuthorName { name, then }) = self.prompt.take() else {
            return Task::none();
        };
        let name = name.trim();
        if keep {
            let name = (!name.is_empty()).then(|| name.to_string());
            self.settings.author_name = name.clone();
            self.save_settings();
            self.d.set_writer_name(name);
        }
        if then == NameThen::TrackMine {
            self.set_track(Some(true), None);
        }
        self.after_change(false)
    }

    /// Counts what waits for review and, while the panel shows, reads its
    /// rows again when the script changed. Runs when the navigator does.
    pub(super) fn refresh_changes(&mut self) {
        self.review = self.d.session().map_or((0, 0), |s| {
            let pending = s.doc.pending();
            let agents = pending
                .iter()
                .filter(|&&c| {
                    s.doc
                        .changes
                        .get(c)
                        .is_some_and(|e| e.by.kind == ByKind::Agent)
                })
                .count();
            (pending.len(), agents)
        });
        if !self.settings.changes_panel {
            // Read again when the panel opens.
            self.changes.rev = None;
            return;
        }
        let rev = self.d.session().map(|s| s.rev);
        if rev == self.changes.rev {
            return;
        }
        self.changes.rev = rev;
        let chosen = self.changes.chosen();
        self.changes.rows = match self.d.session() {
            Some(_) => self
                .run("changes.list", json!({}))
                .map(|r| rows(&r))
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let authors = self.changes.authors();
        if self
            .changes
            .filter
            .as_ref()
            .is_some_and(|f| !authors.contains(f))
        {
            self.changes.filter = None;
        }
        // The chosen change keeps its row; one just settled leaves the
        // next one chosen.
        let n = self.changes.visible().len();
        match chosen.and_then(|c| self.changes.visible().iter().position(|r| r.id == c)) {
            Some(i) => self.changes.selected = i,
            None => self.changes.selected = self.changes.selected.min(n.saturating_sub(1)),
        }
    }

    /// Moves the caret to a panel row's change and scrolls to it.
    pub(super) fn goto_change_row(&mut self, i: usize) -> Task<Message> {
        self.changes.selected = i;
        let target = self.changes.visible().get(i).map(|r| r.id).and_then(|id| {
            let doc = &self.d.session()?.doc;
            change_caret(doc, id)
        });
        if let Some(c) = target {
            self.editor.caret = c;
            self.editor.anchor = c;
        }
        self.after_change(true)
    }

    /// Accepts or rejects a panel row's change; the next row is chosen.
    pub(super) fn settle_row(&mut self, i: usize, accept: bool) -> Task<Message> {
        self.changes.selected = i;
        if let Some(id) = self.changes.visible().get(i).map(|r| r.id) {
            self.settle(accept, json!({ "id": id }));
        }
        self.after_change(false)
    }

    /// Keys while the panel has the keyboard (editor.md §17.3).
    pub(super) fn changes_key(&mut self, kp: &crate::page_view::KeyPress) -> Option<Task<Message>> {
        use iced::keyboard::Key;
        use iced::keyboard::key::Named;
        if kp.modifiers.command() || kp.modifiers.alt() {
            return None;
        }
        let n = self.changes.visible().len();
        let i = self.changes.selected;
        let ch = match kp.key.as_ref() {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        Some(match kp.key.as_ref() {
            Key::Named(Named::ArrowDown) => {
                self.changes.selected = (i + 1).min(n.saturating_sub(1));
                Task::none()
            }
            Key::Named(Named::ArrowUp) => {
                self.changes.selected = i.saturating_sub(1);
                Task::none()
            }
            Key::Named(Named::Enter) if n > 0 => self.goto_change_row(i),
            Key::Named(Named::Escape) => {
                self.focus = Focus::Page;
                Task::none()
            }
            _ if ch == "a" && n > 0 => self.settle_row(i, true),
            _ if ch == "r" && n > 0 => self.settle_row(i, false),
            _ if ch == "n" => self.ask_name(NameThen::Nothing),
            _ => return None,
        })
    }

    /// The changes panel, on the right of the pages (editor.md §17.3).
    pub(super) fn changes_panel(&self, pal: Palette) -> Element<'_, Message> {
        let slots = self
            .d
            .session()
            .map(|s| s.doc.change_slots())
            .unwrap_or_default();
        let visible = self.changes.visible();
        let small = |label: &'static str, m: Message| {
            button(text(label).font(fonts::UI).size(11))
                .on_press(m)
                .padding([2, 8])
                .style(button::secondary)
        };
        let mut authors = vec![ALL_AUTHORS.to_string()];
        authors.extend(self.changes.authors());
        let filter = self
            .changes
            .filter
            .clone()
            .unwrap_or_else(|| ALL_AUTHORS.to_string());
        let mut col = column![
            row![
                text(format!("Changes · {}", self.changes.rows.len()))
                    .font(fonts::UI)
                    .size(12)
                    .color(pal.text_quiet),
                space::horizontal(),
                small("Accept all", Message::Action("changes.accept_all".into())),
                small("Reject all", Message::Action("changes.reject_all".into())),
            ]
            .spacing(6)
            .align_y(Center),
            row![
                text(format!("Editing as: {}", ellipsize(&self.editing_as(), 20)))
                    .font(fonts::UI)
                    .size(12)
                    .color(pal.text)
                    .wrapping(text::Wrapping::None),
                space::horizontal(),
                small("Change my name…", Message::Action("app.author_name".into())),
            ]
            .spacing(6)
            .align_y(Center),
            pick_list(authors, Some(filter), Message::ChangesFilter)
                .font(fonts::UI)
                .text_size(12)
                .padding([3, 8])
                .width(Fill),
        ]
        .spacing(8)
        .padding([8, 12]);
        let mut list = column![].spacing(2);
        if visible.is_empty() {
            list = list.push(
                container(
                    text("No changes to review.")
                        .font(fonts::UI)
                        .size(13)
                        .color(pal.text_quiet),
                )
                .padding([6, 12]),
            );
        }
        for (i, r) in visible.iter().enumerate() {
            let selected = i == self.changes.selected;
            let color = change_color(&pal, &slots, r.id.0);
            let scene = r
                .scene
                .and_then(|id| self.nav.iter().find(|n| n.id == id))
                .map_or_else(
                    || "Before the first scene".to_string(),
                    |n| format!("{} {}", n.number, n.heading),
                );
            let when = if r.t == 0 { String::new() } else { ago(r.t) };
            let mut cell = column![
                row![
                    text("●").font(fonts::UI).size(11).color(color),
                    text(ellipsize(&r.author, 18))
                        .font(fonts::UI_BOLD)
                        .size(12)
                        .color(pal.text),
                    space::horizontal(),
                    text(when).font(fonts::UI).size(11).color(pal.text_quiet),
                ]
                .spacing(6)
                .align_y(Center),
                text(format!("{} · {}", r.what, ellipsize(&scene, 26)))
                    .font(fonts::UI)
                    .size(11)
                    .color(pal.text_quiet)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(2);
            if !r.deleted.is_empty() {
                cell = cell.push(struck(ellipsize(&r.deleted, 36), 12.0, color));
            }
            if !r.inserted.is_empty() {
                cell = cell.push(
                    text(ellipsize(&r.inserted, 36))
                        .font(fonts::UI)
                        .size(12)
                        .color(color)
                        .wrapping(text::Wrapping::None),
                );
            }
            if selected {
                cell = cell.push(
                    row![
                        small("Accept", Message::ChangesSettle(i, true)),
                        small("Reject", Message::ChangesSettle(i, false)),
                    ]
                    .spacing(6),
                );
            }
            // The chosen row is outlined, not filled, so the authors'
            // colors keep their contrast against the panel in every theme.
            let (bg, edge) = if selected {
                (pal.panel_alt, pal.focus)
            } else {
                (pal.panel, Color::TRANSPARENT)
            };
            list = list.push(
                mouse_area(
                    container(cell)
                        .padding([6, 12])
                        .width(Fill)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(bg)),
                            border: Border {
                                color: edge,
                                width: 2.0,
                                radius: 4.0.into(),
                            },
                            ..container::Style::default()
                        }),
                )
                .on_press(Message::ChangesPick(i)),
            );
        }
        col = col.push(scrollable(list).height(Fill));
        col = col.push(
            hint(
                pal,
                "A: accept  ·  R: reject  ·  Enter: go to it  ·  N: my name",
            )
            .size(11),
        );
        let focused = self.focus == Focus::Changes;
        container(col)
            .width(Length::Fixed(300.0))
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

    /// The question for the writer's name (editor.md §17.4).
    pub(super) fn name_view(&self, pal: Palette, name: &str) -> Element<'_, Message> {
        column![
            title(pal, "What name should your changes carry?"),
            body(
                pal,
                "Tracked changes show who made them. You can change it later with \"Change my name…\" in the changes panel, or leave it empty for \"Writer\".",
            ),
            text_input(DEFAULT_NAME, name)
                .id(NAME)
                .on_input(Message::NameInput)
                .on_submit(Message::NameSubmit)
                .font(fonts::UI)
                .size(14),
            row![
                dialog_button("Save the name", Message::NameSubmit, true),
                dialog_button("Not now", Message::Escaped, false),
            ]
            .spacing(8),
            hint(pal, "Enter: save the name  ·  Esc: not now (changes say \"Writer\")"),
        ]
        .spacing(12)
        .into()
    }

    /// The question before settling every change.
    pub(super) fn settle_all_view(
        &self,
        pal: Palette,
        accept: bool,
        count: usize,
    ) -> Element<'_, Message> {
        let verb = if accept { "Accept" } else { "Reject" };
        let what = if count == 1 {
            "the 1 change".to_string()
        } else {
            format!("all {count} changes")
        };
        column![
            title(pal, format!("{verb} {what}?")),
            body(
                pal,
                if accept {
                    "Each change becomes part of the script. Mod+Z gives them back."
                } else {
                    "Each change is taken out of the script. Mod+Z gives them back."
                }
                .replace("Mod", if crate::keymap::MAC { "Cmd" } else { "Ctrl" }),
            ),
            row![
                dialog_button(
                    if accept { "Accept all" } else { "Reject all" },
                    Message::Reply(super::Reply::Save),
                    true
                ),
                dialog_button("Cancel", Message::Reply(super::Reply::Cancel), false),
            ]
            .spacing(8),
            hint(
                pal,
                format!("Enter: {} all  ·  Esc: Cancel", verb.to_lowercase())
            ),
        ]
        .spacing(12)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_read_the_list() {
        let list = json!({ "changes": [
            { "id": "c_2", "by": { "kind": "agent", "name": "Claude" }, "t": 5,
              "kinds": ["insert", "delete"], "blocks": ["b_1"], "scene": "b_1",
              "inserted": "new", "deleted": "old" },
            { "id": "c_3", "by": { "kind": "writer" }, "t": 0,
              "kinds": ["block_insert", "insert"], "blocks": ["b_4", "b_5"], "scene": null,
              "inserted": "", "deleted": "" },
            { "id": "c_4", "by": null, "kinds": ["move"], "blocks": ["b_6"] },
        ]});
        let r = rows(&list);
        assert_eq!(r.len(), 3);
        assert_eq!(
            (r[0].id, r[0].author.as_str(), r[0].what),
            (ChangeId(2), "Claude", "Replaced")
        );
        assert_eq!(r[0].scene, BlockId::parse("b_1"));
        assert_eq!((r[1].author.as_str(), r[1].what), ("Writer", "New blocks"));
        assert_eq!(
            (r[2].author.as_str(), r[2].what),
            ("Unknown", "Moved scene")
        );
        let panel = ChangesPanel {
            rows: r,
            filter: Some("Writer".into()),
            ..ChangesPanel::default()
        };
        assert_eq!(panel.visible().len(), 1);
        assert_eq!(panel.authors(), ["Claude", "Writer", "Unknown"]);
        assert_eq!(panel.chosen(), Some(ChangeId(3)));
    }
}
