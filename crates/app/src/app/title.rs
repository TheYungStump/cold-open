//! The title page editor (editor.md §19): a sheet beside the page with a
//! field per Fountain key, or a row per positioned line. A field the writer
//! leaves, changed, is one `title.set` or `title.set_lines`, so one undo
//! step.

use coldopen_core::Document;
use coldopen_screenplay::{Align, Pt, RichText, Run, TitleLine};
use iced::alignment::Horizontal;
use iced::widget::{
    button, column, container, row, scrollable, space, text, text_editor, text_input,
};
use iced::{Background, Border, Element, Fill, Length, Task};
use serde_json::{Value, json};

use super::view::{body, hint, title};
use super::{App, Message, PAGES};
use crate::fonts;
use crate::page_view::KeyPress;
use crate::theme::Palette;

/// The keys the sheet always shows, first and in this order (editor.md
/// §19).
pub const USUAL: [&str; 8] = [
    "Title",
    "Credit",
    "Author",
    "Source",
    "Draft date",
    "Contact",
    "Notes",
    "Copyright",
];

/// The sheet's width, in pixels.
pub const WIDTH: f32 = 440.0;

/// How a line break inside a title line shows in its one-line field, and
/// what typing it there makes (editor.md §19).
const BREAK: char = '¶';

/// One key's field.
pub struct KeyField {
    /// The key as the title page spells it, or as the sheet offers it.
    pub key: String,
    /// Its values in the document.
    pub values: Vec<RichText>,
    /// What the field shows: the values one per line.
    pub content: text_editor::Content,
}

impl KeyField {
    fn new(key: String, values: Vec<RichText>) -> KeyField {
        let shown = values
            .iter()
            .map(RichText::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        KeyField {
            key,
            values,
            content: text_editor::Content::with_text(&shown),
        }
    }

    /// The values as typed, one per non-empty line, trimmed.
    fn typed(&self) -> Vec<String> {
        self.content
            .text()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()
    }

    fn dirty(&self) -> bool {
        let had: Vec<String> = self.values.iter().map(RichText::plain_text).collect();
        self.typed() != had
    }
}

/// One positioned line's row.
#[derive(Clone, Debug, PartialEq)]
pub struct LineField {
    /// The line in the document, or as it will be added.
    pub line: TitleLine,
    pub text: String,
    pub align: Align,
    /// The space above, in lines, as typed.
    pub space: String,
}

impl LineField {
    fn new(line: TitleLine, line_height: Pt) -> LineField {
        LineField {
            text: line.text.plain_text().replace('\n', &BREAK.to_string()),
            align: line.align,
            space: lines_of(line.space_before, line_height),
            line,
        }
    }
}

/// What the sheet edits.
pub enum TitleForm {
    Keys {
        fields: Vec<KeyField>,
        /// The name typed into "Add a key…".
        new_key: String,
    },
    Lines(Vec<LineField>),
}

/// The open sheet.
pub struct TitleSheet {
    pub form: TitleForm,
    /// The field that has the keyboard, in Tab order: for lines, each
    /// line's text and then its space above.
    pub current: usize,
    /// The page format's line height, for the space above in lines.
    pub line_height: Pt,
}

impl TitleSheet {
    /// The sheet for a document, in its form; a script with no title page
    /// gets the form its file format uses.
    pub fn of(doc: &Document, fdx: bool) -> TitleSheet {
        let line_height = coldopen_core::view::page_format(doc).get().line_height;
        let lines = match coldopen_core::title::form(doc) {
            "lines" => true,
            "keys" => false,
            _ => fdx,
        };
        let form = if lines {
            TitleForm::Lines(
                doc.title_lines
                    .iter()
                    .map(|l| LineField::new(l.clone(), line_height))
                    .collect(),
            )
        } else {
            let mut fields: Vec<KeyField> = USUAL
                .iter()
                .map(|&k| match doc.title_page.get(k) {
                    Some(f) => KeyField::new(f.key.clone(), f.values.clone()),
                    None => KeyField::new(k.to_string(), Vec::new()),
                })
                .collect();
            for f in &doc.title_page.fields {
                if !fields.iter().any(|s| s.key.eq_ignore_ascii_case(&f.key)) {
                    fields.push(KeyField::new(f.key.clone(), f.values.clone()));
                }
            }
            TitleForm::Keys {
                fields,
                new_key: String::new(),
            }
        };
        TitleSheet {
            form,
            current: 0,
            line_height,
        }
    }

    /// Each key and what its field shows, in the sheet's order; empty in
    /// the lines form.
    pub fn key_fields(&self) -> Vec<(String, String)> {
        match &self.form {
            TitleForm::Keys { fields, .. } => fields
                .iter()
                .map(|f| (f.key.clone(), f.content.text().trim_end().to_string()))
                .collect(),
            TitleForm::Lines(_) => Vec::new(),
        }
    }

    /// Each line's text, alignment and space above as its row shows them;
    /// empty in the keys form.
    pub fn line_rows(&self) -> Vec<(String, Align, String)> {
        match &self.form {
            TitleForm::Lines(lines) => lines
                .iter()
                .map(|l| (l.text.clone(), l.align, l.space.clone()))
                .collect(),
            TitleForm::Keys { .. } => Vec::new(),
        }
    }

    /// How many fields Tab steps through.
    fn stops(&self) -> usize {
        match &self.form {
            // The keys, then "Add a key…".
            TitleForm::Keys { fields, .. } => fields.len() + 1,
            TitleForm::Lines(lines) => lines.len() * 2,
        }
    }
}

/// What the sheet's widgets send.
#[derive(Clone, Debug)]
pub enum TitleInput {
    KeyEdit(usize, text_editor::Action),
    NewKey(String),
    AddKey,
    LineText(usize, String),
    LineSpace(usize, String),
    /// Enter in a line: a new line after it.
    LineEnter(usize),
    /// A line's alignment button: the next alignment.
    LineAlign(usize),
    Done,
}

/// The widget id of the field at Tab stop `i`.
pub fn field_id(i: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("title-field-{i}"))
}

/// Gives the field at Tab stop `i` the keyboard, its text selected, as a
/// form's fields take Tab.
fn focus_field(i: usize) -> Task<Message> {
    Task::batch([
        iced::widget::operation::focus(field_id(i)),
        iced::widget::operation::select_all(field_id(i)),
    ])
}

/// Points as lines of `line_height`: "2", or "1.5" for what is not whole.
fn lines_of(p: Pt, line_height: Pt) -> String {
    if line_height.0 <= 0 {
        return "0".into();
    }
    let tenths = (f64::from(p.0) * 10.0 / f64::from(line_height.0)).round() as i64;
    if tenths % 10 == 0 {
        (tenths / 10).to_string()
    } else {
        format!("{}.{}", tenths / 10, (tenths % 10).abs())
    }
}

/// `old` with its text made `new`: the common start and end keep their
/// styles, and the text between takes the style before it, as typing does.
pub fn retext(old: &RichText, new: &str) -> RichText {
    let o: Vec<char> = old.plain_text().chars().collect();
    let n: Vec<char> = new.chars().collect();
    let p = o.iter().zip(&n).take_while(|(a, b)| a == b).count();
    let s = o[p..]
        .iter()
        .rev()
        .zip(n[p..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let middle: String = n[p..n.len() - s].iter().collect();
    let style = old.style_at(p);
    old.delete(p, o.len() - s)
        .and_then(|t| t.insert(p, &middle, Some(style)))
        .unwrap_or_else(|_| RichText::plain(new))
}

/// A value or a line's text as the title commands take it: a string when
/// it is plain, its runs otherwise.
fn text_param(t: &RichText) -> Value {
    let plain = t.runs().iter().all(
        |r| matches!(r, Run::Text { style, marks, .. } if style.is_plain() && marks.is_none()),
    );
    if plain {
        json!(t.plain_text())
    } else {
        json!(t.runs())
    }
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Left => "left",
        Align::Center => "center",
        Align::Right => "right",
    }
}

fn points(p: Pt) -> f64 {
    f64::from(p.0) / 100.0
}

impl App {
    /// "Edit title page…": the sheet, and the page scrolled to the top,
    /// where the title page prints.
    pub(super) fn title_open(&mut self) -> Task<Message> {
        let Some(s) = self.d.session() else {
            return Task::none();
        };
        let fdx = s.format == coldopen_core::FormatId::Fdx;
        self.title_sheet = Some(TitleSheet::of(&s.doc, fdx));
        Task::batch([
            iced::widget::operation::snap_to(
                PAGES,
                iced::widget::scrollable::RelativeOffset::START,
            ),
            focus_field(0),
        ])
    }

    /// Sends what changed, then closes the sheet.
    pub(super) fn title_close(&mut self) -> Task<Message> {
        self.title_commit();
        self.title_sheet = None;
        self.after_change(false)
    }

    /// Sends every field that changed: a key each, or the lines at once.
    pub(super) fn title_commit(&mut self) {
        self.title_commit_except(None);
    }

    fn title_commit_except(&mut self, keep: Option<usize>) {
        let Some(sheet) = self.title_sheet.as_ref() else {
            return;
        };
        let mut sends: Vec<(usize, Value)> = Vec::new();
        let mut lines: Option<Value> = None;
        match &sheet.form {
            TitleForm::Keys { fields, .. } => {
                for (i, f) in fields.iter().enumerate() {
                    if Some(i) == keep || !f.dirty() {
                        continue;
                    }
                    let values: Vec<Value> = f
                        .typed()
                        .iter()
                        .enumerate()
                        .map(|(j, t)| {
                            text_param(&match f.values.get(j) {
                                Some(v) => retext(v, t),
                                None => RichText::plain(t.clone()),
                            })
                        })
                        .collect();
                    let mut params = json!({ "key": f.key, "values": values });
                    if f.values.is_empty() {
                        // After the nearest key before it that the title
                        // page has.
                        let doc_keys: Vec<String> = self
                            .d
                            .session()
                            .map(|s| {
                                s.doc
                                    .title_page
                                    .fields
                                    .iter()
                                    .map(|f| f.key.clone())
                                    .collect()
                            })
                            .unwrap_or_default();
                        let at = fields[..i]
                            .iter()
                            .rev()
                            .find_map(|b| doc_keys.iter().position(|k| *k == b.key))
                            .map_or(0, |p| p + 1);
                        params["at"] = json!(at);
                    }
                    sends.push((i, params));
                }
            }
            TitleForm::Lines(fields) => {
                let want = self.title_lines_wanted(sheet, fields);
                let had = self.d.session().map(|s| s.doc.title_lines.clone());
                if had.as_ref() != Some(&want) {
                    let list: Vec<Value> = want
                        .iter()
                        .map(|l| {
                            let mut v = json!({
                                "align": align_name(l.align),
                                "left": points(l.left),
                                "right": points(l.right),
                                "space_before": points(l.space_before),
                            });
                            match text_param(&l.text) {
                                Value::String(s) => v["text"] = json!(s),
                                runs => v["runs"] = runs,
                            }
                            v
                        })
                        .collect();
                    lines = Some(json!({ "lines": list }));
                }
            }
        }
        for (i, params) in sends {
            if self.run("title.set", params).is_some() {
                self.title_refresh_key(i);
            }
        }
        if let Some(params) = lines
            && self.run("title.set_lines", params).is_some()
        {
            self.title_refresh_lines();
        }
    }

    /// The lines as the rows say.
    fn title_lines_wanted(&self, sheet: &TitleSheet, fields: &[LineField]) -> Vec<TitleLine> {
        fields
            .iter()
            .map(|f| {
                let shown = lines_of(f.line.space_before, sheet.line_height);
                let space_before = if f.space.trim() == shown {
                    f.line.space_before
                } else {
                    match f.space.trim().parse::<f64>() {
                        Ok(n) if n.is_finite() && n >= 0.0 => {
                            Pt((n * f64::from(sheet.line_height.0)).round() as i32)
                        }
                        _ => f.line.space_before,
                    }
                };
                TitleLine {
                    text: retext(&f.line.text, &f.text.replace(BREAK, "\n")),
                    align: f.align,
                    left: f.line.left,
                    right: f.line.right,
                    space_before,
                }
            })
            .collect()
    }

    /// A key's field after its command: the values the document now has.
    fn title_refresh_key(&mut self, i: usize) {
        let Some(s) = self.d.session() else {
            return;
        };
        let Some(TitleSheet {
            form: TitleForm::Keys { fields, .. },
            ..
        }) = self.title_sheet.as_mut()
        else {
            return;
        };
        let Some(f) = fields.get_mut(i) else {
            return;
        };
        match s.doc.title_page.get(&f.key) {
            Some(d) => {
                f.key = d.key.clone();
                f.values = d.values.clone();
            }
            None => f.values = Vec::new(),
        }
    }

    /// The rows after `title.set_lines`: each line as the document has it.
    fn title_refresh_lines(&mut self) {
        let Some(s) = self.d.session() else {
            return;
        };
        let Some(sheet) = self.title_sheet.as_mut() else {
            return;
        };
        let line_height = sheet.line_height;
        if let TitleForm::Lines(fields) = &mut sheet.form {
            for (f, l) in fields.iter_mut().zip(&s.doc.title_lines) {
                *f = LineField::new(l.clone(), line_height);
            }
        }
    }

    /// The sheet again from the document, after an undo or redo.
    fn title_reload(&mut self) {
        let Some(s) = self.d.session() else {
            return;
        };
        let fdx = s.format == coldopen_core::FormatId::Fdx;
        let current = self.title_sheet.as_ref().map_or(0, |t| t.current);
        let mut sheet = TitleSheet::of(&s.doc, fdx);
        sheet.current = current.min(sheet.stops().saturating_sub(1));
        self.title_sheet = Some(sheet);
    }

    /// What the sheet's widgets send.
    pub(super) fn title_input(&mut self, input: TitleInput) -> Task<Message> {
        match input {
            TitleInput::KeyEdit(i, action) => {
                if action.is_edit() {
                    // Leaving a field that changed sends it (editor.md §19).
                    self.title_commit_except(Some(i));
                }
                if let Some(TitleSheet {
                    form: TitleForm::Keys { fields, .. },
                    current,
                    ..
                }) = self.title_sheet.as_mut()
                    && let Some(f) = fields.get_mut(i)
                {
                    *current = i;
                    f.content.perform(action);
                }
            }
            TitleInput::NewKey(k) => {
                if let Some(TitleSheet {
                    form: TitleForm::Keys { new_key, fields },
                    current,
                    ..
                }) = self.title_sheet.as_mut()
                {
                    *current = fields.len();
                    *new_key = k;
                }
            }
            TitleInput::AddKey => return self.title_add_key(),
            TitleInput::LineText(i, t) => {
                self.title_line_edit(2 * i, |f| f.text = t);
            }
            TitleInput::LineSpace(i, t) => {
                self.title_line_edit(2 * i + 1, |f| f.space = t);
            }
            TitleInput::LineEnter(i) => return self.title_line_insert(i),
            TitleInput::LineAlign(i) => {
                self.title_line_edit(2 * i, |f| {
                    f.align = match f.align {
                        Align::Left => Align::Center,
                        Align::Center => Align::Right,
                        Align::Right => Align::Left,
                    }
                });
                self.title_commit();
            }
            TitleInput::Done => return self.title_close(),
        }
        self.after_change(false)
    }

    /// An edit to line `stop / 2`: the other lines' changes are sent first.
    fn title_line_edit(&mut self, stop: usize, edit: impl FnOnce(&mut LineField)) {
        let line = stop / 2;
        let others_changed = {
            let Some(sheet) = self.title_sheet.as_ref() else {
                return;
            };
            let TitleForm::Lines(fields) = &sheet.form else {
                return;
            };
            let want = self.title_lines_wanted(sheet, fields);
            let had = self
                .d
                .session()
                .map(|s| s.doc.title_lines.clone())
                .unwrap_or_default();
            want.iter()
                .zip(&had)
                .enumerate()
                .any(|(k, (w, h))| k != line && w != h)
        };
        if others_changed {
            self.title_commit();
        }
        if let Some(sheet) = self.title_sheet.as_mut()
            && let TitleForm::Lines(fields) = &mut sheet.form
            && let Some(f) = fields.get_mut(line)
        {
            sheet.current = stop;
            edit(f);
        }
    }

    /// Enter in a line: a new empty line after it, in its column and
    /// alignment, sent at once. With no lines, the first, centered in the
    /// action column.
    fn title_line_insert(&mut self, i: usize) -> Task<Message> {
        self.title_commit();
        let column = self.d.session().map(|s| {
            let f = coldopen_core::view::page_format(&s.doc);
            (f.get().elements.action.left, f.get().elements.action.right)
        });
        let Some(sheet) = self.title_sheet.as_mut() else {
            return Task::none();
        };
        let line_height = sheet.line_height;
        let TitleForm::Lines(fields) = &mut sheet.form else {
            return Task::none();
        };
        let (at, line) = match fields.get(i) {
            Some(f) => (
                i + 1,
                TitleLine {
                    text: RichText::default(),
                    align: f.align,
                    left: f.line.left,
                    right: f.line.right,
                    space_before: Pt::ZERO,
                },
            ),
            None if fields.is_empty() => {
                let Some((left, right)) = column else {
                    return Task::none();
                };
                (
                    0,
                    TitleLine {
                        text: RichText::default(),
                        align: Align::Center,
                        left,
                        right,
                        space_before: Pt::ZERO,
                    },
                )
            }
            None => return Task::none(),
        };
        fields.insert(at, LineField::new(line, line_height));
        sheet.current = 2 * at;
        self.title_commit();
        Task::batch([focus_field(2 * at), self.after_change(false)])
    }

    /// Mod+Shift+K: the current line goes, sent at once.
    fn title_line_remove(&mut self) -> Task<Message> {
        self.title_commit();
        let Some(sheet) = self.title_sheet.as_mut() else {
            return Task::none();
        };
        let line = sheet.current / 2;
        let TitleForm::Lines(fields) = &mut sheet.form else {
            return Task::none();
        };
        if line >= fields.len() {
            return Task::none();
        }
        fields.remove(line);
        let next = line.min(fields.len().saturating_sub(1));
        sheet.current = 2 * next;
        self.title_commit();
        Task::batch([focus_field(2 * next), self.after_change(false)])
    }

    /// "Add a key…": a field for the key typed, if it can be one.
    fn title_add_key(&mut self) -> Task<Message> {
        let Some(TitleSheet {
            form: TitleForm::Keys { fields, new_key },
            current,
            ..
        }) = self.title_sheet.as_mut()
        else {
            return Task::none();
        };
        let key = new_key.trim().to_string();
        if let Some(at) = fields.iter().position(|f| f.key.eq_ignore_ascii_case(&key)) {
            new_key.clear();
            *current = at;
            return focus_field(at);
        }
        if let Err(e) = coldopen_core::title::check_key(&key) {
            self.banners.push(e.message.into());
            return Task::none();
        }
        fields.push(KeyField::new(key, Vec::new()));
        new_key.clear();
        let at = fields.len() - 1;
        *current = at;
        focus_field(at)
    }

    /// Keys while the sheet is open, or `None` when it is not: Tab and
    /// Shift+Tab move between fields, Esc closes, Mod+Z and Mod+Shift+Z
    /// undo and redo, and in the lines form Mod+L, Mod+E and Mod+R align
    /// the line and Mod+Shift+K removes it.
    pub(super) fn title_key(&mut self, kp: &KeyPress, esc: bool) -> Option<Task<Message>> {
        use iced::keyboard::{Key, key::Named};
        let sheet = self.title_sheet.as_ref()?;
        if esc {
            return Some(self.title_close());
        }
        let cmd = kp.modifiers.command();
        let shift = kp.modifiers.shift();
        let ch = match kp.key.as_ref() {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        let lines = matches!(sheet.form, TitleForm::Lines(_));
        if matches!(kp.key.as_ref(), Key::Named(Named::Tab)) && !cmd {
            self.title_commit();
            let sheet = self.title_sheet.as_mut()?;
            let n = sheet.stops().max(1);
            sheet.current = if shift {
                (sheet.current + n - 1) % n
            } else {
                (sheet.current + 1) % n
            };
            return Some(focus_field(sheet.current));
        }
        if cmd && (ch == "z" || (ch == "y" && !crate::keymap::MAC)) {
            // Typing not sent yet is undone first, as the field shows it.
            let typed = match &sheet.form {
                TitleForm::Keys { fields, .. } => fields.iter().any(KeyField::dirty),
                TitleForm::Lines(fields) => {
                    self.d.session().map(|s| &s.doc.title_lines)
                        != Some(&self.title_lines_wanted(sheet, fields))
                }
            };
            if !typed {
                let redo = ch == "y" || shift;
                self.editor.undo(&mut self.d, redo);
            }
            self.title_reload();
            return Some(self.after_change(false));
        }
        if lines && cmd && !shift && matches!(ch.as_str(), "l" | "e" | "r") {
            let align = match ch.as_str() {
                "l" => Align::Left,
                "e" => Align::Center,
                _ => Align::Right,
            };
            let stop = sheet.current;
            self.title_line_edit(stop, |f| f.align = align);
            self.title_commit();
            return Some(self.after_change(false));
        }
        if lines && cmd && shift && ch == "k" {
            return Some(self.title_line_remove());
        }
        Some(Task::none())
    }

    /// The sheet, on the right of the page area (editor.md §19).
    pub(super) fn title_view<'a>(
        &'a self,
        pal: Palette,
        sheet: &'a TitleSheet,
    ) -> Element<'a, Message> {
        let m = if crate::keymap::MAC { "Cmd" } else { "Ctrl" };
        let mut col = column![
            row![
                title(pal, "Title page"),
                space::horizontal(),
                button(text("Done").font(fonts::UI).size(13))
                    .on_press(Message::Title(TitleInput::Done))
                    .padding([4, 12]),
            ]
            .align_y(iced::Alignment::Center),
        ]
        .spacing(12);
        let mut fields_col = column![].spacing(10);
        match &sheet.form {
            TitleForm::Keys { fields, new_key } => {
                col = col.push(body(
                    pal,
                    "Each field holds its key's values, one per line. A field you leave with nothing in it removes its key.",
                ));
                for (i, f) in fields.iter().enumerate() {
                    fields_col = fields_col.push(
                        column![
                            hint(pal, f.key.clone()),
                            text_editor(&f.content)
                                .id(field_id(i))
                                .on_action(move |a| Message::Title(TitleInput::KeyEdit(i, a)))
                                .font(fonts::UI)
                                .size(14)
                                .padding(6)
                                .height(Length::Shrink),
                        ]
                        .spacing(4),
                    );
                }
                fields_col = fields_col.push(
                    text_input("Add a key…", new_key)
                        .id(field_id(fields.len()))
                        .on_input(|k| Message::Title(TitleInput::NewKey(k)))
                        .on_submit(Message::Title(TitleInput::AddKey))
                        .font(fonts::UI)
                        .size(14),
                );
                col = col.push(scrollable(fields_col).height(Fill));
                col = col.push(hint(
                    pal,
                    "Tab: next field  ·  Enter: another value  ·  Esc: done".to_string(),
                ));
            }
            TitleForm::Lines(lines) => {
                col = col.push(body(
                    pal,
                    "Each row is a line of the title page: its text, its alignment, and the space above it in lines.",
                ));
                for (i, l) in lines.iter().enumerate() {
                    let on = sheet.current / 2 == i;
                    let align = match l.align {
                        Align::Left => "Left",
                        Align::Center => "Center",
                        Align::Right => "Right",
                    };
                    fields_col = fields_col.push(
                        row![
                            text_input("(empty line)", &l.text)
                                .id(field_id(2 * i))
                                .on_input(move |t| Message::Title(TitleInput::LineText(i, t)))
                                .on_submit(Message::Title(TitleInput::LineEnter(i)))
                                .font(fonts::UI)
                                .size(14)
                                .width(Fill),
                            button(text(align).font(fonts::UI).size(12))
                                .on_press(Message::Title(TitleInput::LineAlign(i)))
                                .padding([4, 8])
                                .width(Length::Fixed(64.0))
                                .style(if on {
                                    button::primary
                                } else {
                                    button::secondary
                                }),
                            text_input("0", &l.space)
                                .id(field_id(2 * i + 1))
                                .on_input(move |t| Message::Title(TitleInput::LineSpace(i, t)))
                                .font(fonts::UI)
                                .size(14)
                                .width(Length::Fixed(48.0)),
                        ]
                        .spacing(6)
                        .align_y(iced::Alignment::Center),
                    );
                }
                if lines.is_empty() {
                    fields_col = fields_col.push(
                        button(text("Add a line").font(fonts::UI).size(13))
                            .on_press(Message::Title(TitleInput::LineEnter(usize::MAX)))
                            .padding([4, 12]),
                    );
                }
                col = col.push(scrollable(fields_col).height(Fill));
                col = col.push(hint(
                    pal,
                    format!(
                        "Tab: next field  ·  Enter: a line after this one  ·  {m}+L, {m}+E, {m}+R: left, center, right  ·  {m}+Shift+K: remove the line  ·  Esc: done"
                    ),
                ));
            }
        }
        let card = container(col)
            .padding(16)
            .width(Length::Fixed(WIDTH))
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(pal.panel_alt)),
                border: Border {
                    color: if pal.high_contrast {
                        pal.focus
                    } else {
                        pal.border
                    },
                    width: if pal.high_contrast { 2.0 } else { 1.0 },
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            });
        container(iced::widget::opaque(card))
            .width(Fill)
            .height(Fill)
            .align_x(Horizontal::Right)
            .into()
    }
}
