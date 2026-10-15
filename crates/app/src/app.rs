//! The application: state, messages, update and view
//! (docs/specs/editor.md). The app owns the dispatcher, so the writer's
//! keystrokes and agent requests are applied one at a time in this update
//! loop.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use coldopen_core::{BlockId, Command, Dispatcher, Durability, Options};
use coldopen_screenplay::{ElementKind, Pt};
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{
    button, column, container, mouse_area, row, scrollable, space, stack, text, text_input,
};
use iced::{Background, Border, Center, Color, Element, Fill, Length, Subscription, Task, Theme};
use serde_json::{Value, json};

use crate::actions;
use crate::editor::flow::kind_label;
use crate::editor::{Editor, Motion, Spot};
use crate::files::{Dirs, Settings};
use crate::fonts;
use crate::keymap::{Chord, Keymap, MAC};
use crate::page_view::{self, KeyPress, NormRange, PageEvent, PageView};
use crate::theme::{Choice, Palette};

const PAGES: &str = "pages";
const FIND: &str = "find";
const PALETTE: &str = "palette";

/// Who has keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Page,
    Navigator,
    Find,
    Palette,
}

#[derive(Clone, Debug)]
pub enum Message {
    Page(PageEvent),
    /// A key no widget used: shortcuts while the page is not focused.
    GlobalKey(KeyPress),
    Tick,
    Scrolled(scrollable::Viewport),
    SystemTheme(bool),
    Action(String),
    Pasted(Option<String>),
    NavPress(usize),
    NavRelease(usize),
    NavEnter(usize),
    FindInput(String),
    ReplaceInput(String),
    FindCase,
    FindScope,
    ReplaceOne,
    ReplaceAll,
    PaletteInput(String),
    PaletteRun(usize),
    ParamsInput(String),
    ParamsRun,
    DismissBanner,
    Noop,
}

/// One row of the navigator (editor.md §7).
#[derive(Clone, Debug)]
pub struct NavRow {
    pub id: BlockId,
    pub section: bool,
    pub depth: u8,
    pub number: String,
    pub heading: String,
    pub page: Option<u64>,
    pub eighths: u64,
}

#[derive(Debug, Default)]
struct FindState {
    query: String,
    replace: String,
    replace_open: bool,
    match_case: bool,
    scene_only: bool,
    /// Matches as (block, from, to) in characters.
    matches: Vec<(BlockId, usize, usize)>,
    current: usize,
}

#[derive(Debug, Default)]
struct PaletteState {
    query: String,
    selected: usize,
    /// A command being given params as JSON, and the last response.
    params: Option<(String, String)>,
    response: Option<String>,
}

/// How long a keystroke took from arrival to the end of the update that
/// handled it, kept for the performance overlay and budgets.
#[derive(Debug, Default)]
pub struct Perf {
    pub keystrokes: Vec<f64>,
}

pub struct App {
    pub d: Dispatcher,
    pub editor: Editor,
    pub dirs: Option<Dirs>,
    pub settings: Settings,
    system_dark: bool,
    pub focus: Focus,
    caret_on: bool,
    last_input: Instant,
    pub banners: Vec<String>,
    viewport: Option<scrollable::Viewport>,
    fonts: Box<dyn coldopen_layout::Coverage>,
    keymap: Keymap,
    nav: Vec<NavRow>,
    nav_rev: Option<u64>,
    nav_drag: Option<usize>,
    nav_hover: Option<usize>,
    find: Option<FindState>,
    palette: Option<PaletteState>,
    pub agent_marks: BTreeSet<BlockId>,
    focus_mode: bool,
    // Cached for drawing, refreshed after every change.
    caret_spot: Option<Spot>,
    sel_range: Option<NormRange>,
    match_ranges: Vec<NormRange>,
    marked_elems: BTreeSet<usize>,
    pub perf: Perf,
}

/// How the app starts.
#[derive(Clone, Debug, Default)]
pub struct Boot {
    /// A script to open.
    pub file: Option<PathBuf>,
    /// Folders to use instead of the user's (tests, portable use).
    pub dirs: Option<Dirs>,
    /// The theme, overriding settings (screenshots).
    pub theme: Option<Choice>,
    /// Keep the journal in the CLI's every-entry mode (tests that crash
    /// the app on purpose use the app's own mode, the default).
    pub every_entry: bool,
}

fn coverage() -> Box<dyn coldopen_layout::Coverage> {
    match coldopen_layout::Fonts::new() {
        Ok(f) => Box::new(f),
        // Bundled fonts that do not parse are a build error caught by the
        // fonts crate's tests; drawing still works with every glyph assumed.
        Err(_) => Box::new(coldopen_layout::AllGlyphs),
    }
}

impl App {
    pub fn new(boot: Boot) -> App {
        let dirs = boot.dirs.clone().or_else(Dirs::find);
        let (mut settings, warn) = match &dirs {
            Some(d) => Settings::load(d),
            None => (Settings::default(), None),
        };
        if let Some(t) = boot.theme {
            settings.theme = t;
        }
        let mut banners: Vec<String> = warn.into_iter().collect();
        let keymap_text = dirs
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.config.join("keymap.json")).ok());
        let (keymap, kw) = Keymap::load(keymap_text.as_deref(), &actions::known);
        banners.extend(kw);

        let mut d = Dispatcher::new(Options::default());
        if !boot.every_entry {
            d.set_durability(Durability::APP);
        }
        let mut app = App {
            d,
            editor: Editor::new(&coldopen_core::Document::default()),
            dirs,
            settings,
            system_dark: false,
            focus: Focus::Page,
            caret_on: true,
            last_input: Instant::now(),
            banners,
            viewport: None,
            fonts: coverage(),
            keymap,
            nav: Vec::new(),
            nav_rev: None,
            nav_drag: None,
            nav_hover: None,
            find: None,
            palette: None,
            agent_marks: BTreeSet::new(),
            focus_mode: false,
            caret_spot: None,
            sel_range: None,
            match_ranges: Vec::new(),
            marked_elems: BTreeSet::new(),
            perf: Perf::default(),
        };
        if let Some(text) = app
            .dirs
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.config.join("flow.json")).ok())
        {
            let w = app.editor.flow.apply_overrides(&text);
            app.banners.extend(w);
        }
        match boot.file {
            Some(f) => app.open(&f),
            None => app.new_script(),
        }
        app.editor.draft = app.settings.draft;
        let _ = app.after_change(false);
        app
    }

    /// For screenshots: a new speech on page 1 with the autocomplete list
    /// open.
    pub fn demo_autocomplete(&mut self) {
        let target = self.d.session().and_then(|s| {
            s.doc
                .blocks
                .iter()
                .filter(|b| b.element.kind == ElementKind::Dialogue)
                .nth(1)
                .map(|b| (b.id, b.element.text.char_len()))
        });
        if let Some((id, len)) = target {
            let c = crate::editor::Caret {
                block: id,
                offset: len,
            };
            self.editor.caret = c;
            self.editor.anchor = c;
            self.editor.enter(&mut self.d);
            self.editor.insert_text(&mut self.d, "N");
        }
        let _ = self.after_change(false);
    }

    fn palette_colors(&self) -> Palette {
        Palette::of(self.settings.theme, self.system_dark)
    }

    pub fn theme(&self) -> Theme {
        self.palette_colors().theme()
    }

    pub fn title(&self) -> String {
        let doc = self.d.session();
        let name = doc
            .and_then(|s| s.path.as_ref())
            .and_then(|p| p.file_name())
            .map_or_else(
                || "Untitled".to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
        let dirty = doc.is_some_and(|s| s.dirty);
        format!("{}{name} — Cold Open", if dirty { "• " } else { "" })
    }

    fn run(&mut self, name: &str, params: Value) -> Option<Value> {
        let cmd = match Command::from_parts(name, params) {
            Ok(c) => c,
            Err(e) => {
                self.banners.push(e);
                return None;
            }
        };
        match self.d.execute(cmd) {
            Ok(v) => Some(v),
            Err(e) => {
                self.banners.push(e.message);
                None
            }
        }
    }

    /// Starts an untitled script with a journal from its first keystroke
    /// (editor.md §11).
    pub fn new_script(&mut self) {
        let path = self.dirs.as_ref().map(|d| {
            let _ = std::fs::create_dir_all(d.unsaved());
            crate::files::next_untitled(d)
        });
        let params = match &path {
            Some(p) => json!({ "path": p }),
            None => json!({}),
        };
        if self.run("doc.new", params).is_none() {
            self.run("doc.new", json!({}));
        }
        self.run(
            "block.insert",
            json!({ "type": "scene_heading", "text": "" }),
        );
        if let Some(s) = self.d.session() {
            self.editor = Editor::new(&s.doc);
            self.editor.draft = self.settings.draft;
        }
    }

    pub fn open(&mut self, path: &Path) {
        match self.run("doc.open", json!({ "path": path })) {
            Some(r) => {
                for w in r["warnings"].as_array().into_iter().flatten() {
                    if let Some(m) = w["message"].as_str() {
                        self.banners.push(m.to_string());
                    }
                }
                if !r["recovered"].is_null() {
                    self.banners.push(format!(
                        "Recovered {} unsaved changes from the last session.",
                        r["recovered"]["commands"]
                    ));
                }
                self.settings.add_recent(path);
                self.save_settings();
                if let Some(s) = self.d.session() {
                    self.editor = Editor::new(&s.doc);
                    self.editor.draft = self.settings.draft;
                }
            }
            None => self.new_script(),
        }
    }

    fn save_settings(&mut self) {
        if let Some(d) = &self.dirs
            && let Err(e) = self.settings.save(d)
        {
            self.banners
                .push(format!("Settings could not be saved: {e}"));
        }
    }

    /// Everything that follows a change: refresh the layout, keep the
    /// caret inside the document, recompute what the view draws, and
    /// scroll the caret into sight.
    fn after_change(&mut self, scroll: bool) -> Task<Message> {
        self.d.view();
        if let Some(s) = self.d.session() {
            self.editor.fix_caret(&s.doc);
        }
        self.editor
            .problems
            .drain(..)
            .for_each(|p| self.banners.push(p.message));
        self.caret_spot = self.editor.caret_spot(&mut self.d);
        self.caret_on = true;
        self.last_input = Instant::now();
        self.refresh_ranges();
        self.refresh_nav();
        if scroll {
            self.scroll_to_caret()
        } else {
            Task::none()
        }
    }

    /// Selection, find matches and agent marks in normal-form terms.
    fn refresh_ranges(&mut self) {
        let (Some(view), Some(s)) = (self.d.cached_view(), self.d.session()) else {
            return;
        };
        let doc = &s.doc;
        let norm = |id: BlockId, off: usize| -> Option<(usize, u32)> {
            let bi = doc.index_of(id)?;
            let t = doc.blocks[bi].element.text.plain_text();
            let b = crate::editor::geometry::byte_of(&t, off);
            view.sources.to_norm(bi, b as u32)
        };
        self.sel_range = if self.editor.has_selection() {
            let (a, b) = self.editor.selection(doc);
            match (norm(a.block, a.offset), norm(b.block, b.offset)) {
                (Some(x), Some(y)) => Some((x, y)),
                _ => None,
            }
        } else {
            None
        };
        self.match_ranges = self
            .find
            .as_ref()
            .map(|f| {
                f.matches
                    .iter()
                    .filter_map(|&(id, from, to)| Some((norm(id, from)?, norm(id, to)?)))
                    .collect()
            })
            .unwrap_or_default();
        self.marked_elems = page_view::marked_elements(view, doc, &self.agent_marks);
    }

    fn refresh_nav(&mut self) {
        let rev = self.d.session().map(|s| s.rev);
        if rev == self.nav_rev {
            return;
        }
        self.nav_rev = rev;
        let Some(doc) = self.d.session().map(|s| s.doc.clone()) else {
            return;
        };
        let scenes = self
            .run("scene.list", json!({}))
            .and_then(|r| r["scenes"].as_array().cloned())
            .unwrap_or_default();
        let mut rows = Vec::new();
        let mut si = scenes.iter().peekable();
        for b in &doc.blocks {
            match b.element.kind {
                ElementKind::Section => rows.push(NavRow {
                    id: b.id,
                    section: true,
                    depth: b.element.depth,
                    number: String::new(),
                    heading: b.element.text.plain_text(),
                    page: None,
                    eighths: 0,
                }),
                ElementKind::SceneHeading => {
                    if let Some(s) = si.next() {
                        rows.push(NavRow {
                            id: b.id,
                            section: false,
                            depth: 0,
                            number: s["number"]
                                .as_str()
                                .map_or_else(|| s["index"].to_string(), str::to_string),
                            heading: s["heading"].as_str().unwrap_or("").to_string(),
                            page: s["page"].as_u64(),
                            eighths: s["eighths"].as_u64().unwrap_or(0),
                        });
                    }
                }
                _ => {}
            }
        }
        self.nav = rows;
    }

    /// The scroll offset that keeps the caret in sight, as a task.
    fn scroll_to_caret(&self) -> Task<Message> {
        let (Some(spot), Some(view), Some(vp)) = (
            self.caret_spot,
            self.d.cached_view(),
            self.viewport.as_ref(),
        ) else {
            return Task::none();
        };
        let f = view.layout().format();
        let k = self.settings.zoom * 96.0 / 72.0 / 100.0;
        let page_h = if self.editor.draft {
            0.0
        } else {
            f.page_height.0 as f32 * k
        };
        let y = 28.0 + spot.page as f32 * (page_h + 24.0) + spot.top.0 as f32 * k;
        let lh = f.line_height.0 as f32 * k;
        let top = vp.absolute_offset().y;
        let h = vp.bounds().height;
        let target = if y < top + lh {
            Some((y - 3.0 * lh).max(0.0))
        } else if y + lh > top + h - lh {
            Some(y + 4.0 * lh - h)
        } else {
            None
        };
        match target {
            Some(t) => iced::widget::operation::scroll_to(
                PAGES,
                scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(t),
                },
            ),
            None => Task::none(),
        }
    }

    fn screen_height(&self) -> Pt {
        let h = self.viewport.as_ref().map_or(700.0, |v| v.bounds().height);
        Pt((h / (self.settings.zoom * 96.0 / 72.0 / 100.0)) as i32)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Page(ev) => self.page_event(ev),
            Message::GlobalKey(kp) => self.global_key(kp),
            Message::Tick => {
                if self.last_input.elapsed() > Duration::from_millis(500) {
                    self.caret_on = !self.caret_on;
                }
                Task::none()
            }
            Message::Scrolled(vp) => {
                self.viewport = Some(vp);
                Task::none()
            }
            Message::SystemTheme(dark) => {
                self.system_dark = dark;
                Task::none()
            }
            Message::Action(a) => self.action(&a),
            Message::Pasted(t) => {
                if let Some(t) = t {
                    self.editor.paste(&mut self.d, &t);
                }
                self.after_change(true)
            }
            Message::NavPress(i) => {
                self.nav_drag = Some(i);
                self.focus = Focus::Navigator;
                Task::none()
            }
            Message::NavEnter(i) => {
                self.nav_hover = Some(i);
                Task::none()
            }
            Message::NavRelease(i) => self.nav_release(i),
            Message::FindInput(q) => {
                if let Some(f) = &mut self.find {
                    f.query = q;
                }
                self.refresh_find();
                self.after_change(false)
            }
            Message::ReplaceInput(r) => {
                if let Some(f) = &mut self.find {
                    f.replace = r;
                }
                Task::none()
            }
            Message::FindCase => {
                if let Some(f) = &mut self.find {
                    f.match_case = !f.match_case;
                }
                self.refresh_find();
                self.after_change(false)
            }
            Message::FindScope => {
                if let Some(f) = &mut self.find {
                    f.scene_only = !f.scene_only;
                }
                self.refresh_find();
                self.after_change(false)
            }
            Message::ReplaceOne => self.replace(false),
            Message::ReplaceAll => self.replace(true),
            Message::PaletteInput(q) => {
                if let Some(p) = &mut self.palette {
                    p.query = q;
                    p.selected = 0;
                }
                Task::none()
            }
            Message::PaletteRun(i) => self.palette_run(i),
            Message::ParamsInput(t) => {
                if let Some(p) = &mut self.palette
                    && let Some((_, json)) = &mut p.params
                {
                    *json = t;
                }
                Task::none()
            }
            Message::ParamsRun => self.params_run(),
            Message::DismissBanner => {
                self.banners.clear();
                Task::none()
            }
            Message::Noop => Task::none(),
        }
    }

    fn page_event(&mut self, ev: PageEvent) -> Task<Message> {
        match ev {
            PageEvent::Key(kp) => {
                let t0 = Instant::now();
                let task = self.key(kp);
                self.perf
                    .keystrokes
                    .push(t0.elapsed().as_secs_f64() * 1000.0);
                task
            }
            PageEvent::Text(t) => {
                self.editor.insert_text(&mut self.d, &t);
                self.after_change(true)
            }
            PageEvent::Preedit(p) => {
                self.editor.preedit = p.map(|text| crate::editor::Preedit { text });
                Task::none()
            }
            PageEvent::Press {
                page,
                x,
                y,
                shift,
                clicks,
            } => {
                self.focus = Focus::Page;
                self.editor.click(&mut self.d, page, x, y, shift);
                if clicks == 2 {
                    self.editor
                        .motion(&mut self.d, Motion::WordLeft, false, Pt::ZERO);
                    self.editor
                        .motion(&mut self.d, Motion::WordRight, true, Pt::ZERO);
                } else if clicks == 3 {
                    self.editor
                        .motion(&mut self.d, Motion::PrevBlock, false, Pt::ZERO);
                    self.editor
                        .motion(&mut self.d, Motion::LineEnd, true, Pt::ZERO);
                }
                self.after_change(false)
            }
            PageEvent::Drag { page, x, y } => {
                self.editor.click(&mut self.d, page, x, y, true);
                self.after_change(false)
            }
        }
    }

    /// A key on the page: shortcuts first, then editing (editor.md §5).
    fn key(&mut self, kp: KeyPress) -> Task<Message> {
        let m = kp.modifiers;
        let word = if MAC { m.alt() } else { m.control() };
        // Autocomplete keys.
        if self.editor.completion.is_some() {
            match kp.key.as_ref() {
                Key::Named(Named::ArrowDown) => {
                    self.editor.completion_move(true);
                    return Task::none();
                }
                Key::Named(Named::ArrowUp) => {
                    self.editor.completion_move(false);
                    return Task::none();
                }
                Key::Named(Named::Escape) => {
                    self.editor.dismiss_completion(&self.d);
                    return Task::none();
                }
                _ => {}
            }
        }
        if let Some(chord) = Chord::from_key(&kp.key, m)
            && let Some(action) = self.keymap.action(&chord).map(str::to_string)
        {
            return self.action(&action);
        }
        let screen = self.screen_height();
        let motion = |mo: Motion| Some((mo, m.shift()));
        let mv = match kp.key.as_ref() {
            Key::Named(Named::ArrowLeft) if MAC && m.logo() => motion(Motion::LineStart),
            Key::Named(Named::ArrowRight) if MAC && m.logo() => motion(Motion::LineEnd),
            Key::Named(Named::ArrowUp) if MAC && m.logo() => motion(Motion::DocStart),
            Key::Named(Named::ArrowDown) if MAC && m.logo() => motion(Motion::DocEnd),
            Key::Named(Named::ArrowUp) if m.control() && m.alt() => motion(Motion::PrevBlock),
            Key::Named(Named::ArrowDown) if m.control() && m.alt() => motion(Motion::NextBlock),
            Key::Named(Named::ArrowLeft) if word => motion(Motion::WordLeft),
            Key::Named(Named::ArrowRight) if word => motion(Motion::WordRight),
            Key::Named(Named::ArrowLeft) => motion(Motion::Left),
            Key::Named(Named::ArrowRight) => motion(Motion::Right),
            Key::Named(Named::ArrowUp) => motion(Motion::Up),
            Key::Named(Named::ArrowDown) => motion(Motion::Down),
            Key::Named(Named::Home) if m.control() => motion(Motion::DocStart),
            Key::Named(Named::End) if m.control() => motion(Motion::DocEnd),
            Key::Named(Named::Home) => motion(Motion::LineStart),
            Key::Named(Named::End) => motion(Motion::LineEnd),
            Key::Named(Named::PageUp) => motion(Motion::PageUp),
            Key::Named(Named::PageDown) => motion(Motion::PageDown),
            _ => None,
        };
        if let Some((mo, extend)) = mv {
            self.editor.motion(&mut self.d, mo, extend, screen);
            self.editor.pending_style = None;
            return self.after_change(true);
        }
        match kp.key.as_ref() {
            Key::Named(Named::Enter) => {
                if m.shift() {
                    self.editor.line_break(&mut self.d);
                } else {
                    self.editor.enter(&mut self.d);
                }
            }
            Key::Named(Named::Tab) => self.editor.tab(&mut self.d, m.shift()),
            Key::Named(Named::Backspace) => self.editor.delete(&mut self.d, false, word),
            Key::Named(Named::Delete) => self.editor.delete(&mut self.d, true, word),
            Key::Named(Named::Escape) => {
                self.find = None;
                self.palette = None;
                self.editor.anchor = self.editor.caret;
            }
            _ => {
                // Text: no Ctrl or Cmd, except AltGr, which Windows reports
                // as Ctrl+Alt.
                let altgr = m.control() && m.alt() && !MAC;
                let typing = (!m.control() || altgr) && !m.logo();
                match kp.text {
                    Some(t) if typing && !t.chars().any(char::is_control) => {
                        self.editor.insert_text(&mut self.d, &t);
                    }
                    _ => return Task::none(),
                }
            }
        }
        self.clear_marks_at_caret();
        self.after_change(true)
    }

    /// Writer edits clear the agent mark on that block (editor.md §12).
    fn clear_marks_at_caret(&mut self) {
        self.agent_marks.remove(&self.editor.caret.block);
    }

    /// Keys the page did not take: shortcuts, palette navigation, Esc.
    fn global_key(&mut self, kp: KeyPress) -> Task<Message> {
        if let Some(p) = &mut self.palette {
            match kp.key.as_ref() {
                Key::Named(Named::ArrowDown) => {
                    p.selected += 1;
                    return Task::none();
                }
                Key::Named(Named::ArrowUp) => {
                    p.selected = p.selected.saturating_sub(1);
                    return Task::none();
                }
                Key::Named(Named::Escape) => {
                    self.palette = None;
                    self.focus = Focus::Page;
                    return Task::none();
                }
                _ => {}
            }
        }
        if let Key::Named(Named::Escape) = kp.key.as_ref() {
            self.find = None;
            self.focus = Focus::Page;
            return self.after_change(false);
        }
        if self.focus == Focus::Navigator {
            let current = self.current_scene_row();
            match kp.key.as_ref() {
                Key::Named(Named::ArrowUp) if kp.modifiers.alt() => {
                    return self.action("nav.move_scene_up");
                }
                Key::Named(Named::ArrowDown) if kp.modifiers.alt() => {
                    return self.action("nav.move_scene_down");
                }
                Key::Named(Named::ArrowUp) => {
                    if let Some(i) = current.and_then(|i| i.checked_sub(1)) {
                        return self.goto_row(i);
                    }
                }
                Key::Named(Named::ArrowDown) => {
                    if let Some(i) = current.map(|i| i + 1).filter(|&i| i < self.nav.len()) {
                        return self.goto_row(i);
                    }
                }
                Key::Named(Named::Enter) => {
                    self.focus = Focus::Page;
                    return Task::none();
                }
                _ => {}
            }
        }
        match Chord::from_key(&kp.key, kp.modifiers)
            .and_then(|c| self.keymap.action(&c).map(str::to_string))
        {
            Some(a) => self.action(&a),
            None => Task::none(),
        }
    }

    fn current_scene_row(&self) -> Option<usize> {
        let doc = &self.d.session()?.doc;
        let ci = doc.index_of(self.editor.caret.block)?;
        self.nav
            .iter()
            .rposition(|r| doc.index_of(r.id).is_some_and(|i| i <= ci))
    }

    fn goto_row(&mut self, i: usize) -> Task<Message> {
        if let Some(r) = self.nav.get(i) {
            let c = crate::editor::Caret {
                block: r.id,
                offset: 0,
            };
            self.editor.caret = c;
            self.editor.anchor = c;
        }
        self.after_change(true)
    }

    fn nav_release(&mut self, i: usize) -> Task<Message> {
        let from = self.nav_drag.take();
        self.nav_hover = None;
        match from {
            Some(f) if f != i => {
                let (Some(src), Some(dst)) = (self.nav.get(f), self.nav.get(i)) else {
                    return Task::none();
                };
                if src.section {
                    return Task::none();
                }
                // Dropping on a row puts the scene before it; dropping on
                // the scene's own next row leaves it where it is.
                let params = if i > f && i + 1 >= self.nav.len() {
                    json!({ "id": src.id })
                } else {
                    let target = if i > f {
                        self.nav.get(i + 1).map(|r| r.id)
                    } else {
                        Some(dst.id)
                    };
                    match target {
                        Some(t) => json!({ "id": src.id, "before": t }),
                        None => json!({ "id": src.id }),
                    }
                };
                self.run("scene.move", params);
                self.after_change(false)
            }
            _ => self.goto_row(i),
        }
    }

    fn refresh_find(&mut self) {
        let Some(f) = &self.find else { return };
        if f.query.is_empty() {
            if let Some(f) = &mut self.find {
                f.matches.clear();
            }
            return;
        }
        let mut params = json!({ "find": f.query, "match_case": f.match_case });
        if f.scene_only
            && let Some(row) = self.current_scene_row().and_then(|i| self.nav.get(i))
            && !row.section
        {
            params["scope"] = json!({ "scene": row.id });
        }
        let matches: Vec<(BlockId, usize, usize)> = self
            .run("text.find", params)
            .and_then(|r| r["matches"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|m| {
                Some((
                    m["id"].as_str().and_then(BlockId::parse)?,
                    m["from"].as_u64()? as usize,
                    m["to"].as_u64()? as usize,
                ))
            })
            .collect();
        if let Some(f) = &mut self.find {
            f.matches = matches;
            f.current = f.current.min(f.matches.len().saturating_sub(1));
        }
    }

    fn find_step(&mut self, back: bool) -> Task<Message> {
        self.refresh_find();
        let Some(f) = &mut self.find else {
            return Task::none();
        };
        if f.matches.is_empty() {
            return Task::none();
        }
        let n = f.matches.len();
        f.current = if back {
            (f.current + n - 1) % n
        } else {
            (f.current + 1) % n
        };
        let (id, from, to) = f.matches[f.current];
        self.editor.anchor = crate::editor::Caret {
            block: id,
            offset: from,
        };
        self.editor.caret = crate::editor::Caret {
            block: id,
            offset: to,
        };
        self.after_change(true)
    }

    fn replace(&mut self, all: bool) -> Task<Message> {
        let Some(f) = &self.find else {
            return Task::none();
        };
        let mut params = json!({ "find": f.query, "with": f.replace, "match_case": f.match_case });
        if all {
            if f.scene_only
                && let Some(row) = self.current_scene_row().and_then(|i| self.nav.get(i))
            {
                params["scope"] = json!({ "scene": row.id });
            }
        } else if let Some(&(id, from, _)) = f.matches.get(f.current) {
            params["at"] = json!({ "id": id, "from": from });
        } else {
            return Task::none();
        }
        self.run("text.replace", params);
        self.refresh_find();
        self.after_change(true)
    }

    /// Runs an action by name (editor.md §10).
    pub fn action(&mut self, name: &str) -> Task<Message> {
        let mut scroll = true;
        match name {
            "app.palette" => {
                self.palette = Some(PaletteState::default());
                self.focus = Focus::Palette;
                return iced::widget::operation::focus(PALETTE);
            }
            "app.new" => {
                if self.d.session().is_some_and(|s| s.dirty) {
                    self.banners.push("Save or close this script first.".into());
                } else {
                    self.d.shutdown();
                    self.new_script();
                }
            }
            "doc.save" => self.save(),
            "undo" => self.editor.undo(&mut self.d, false),
            "redo" => self.editor.undo(&mut self.d, true),
            "app.undo_agent" => {
                while self
                    .d
                    .session()
                    .and_then(|s| s.undo_steps().next().map(|(_, _, by)| !by.is_writer()))
                    .unwrap_or(false)
                {
                    self.editor.undo(&mut self.d, false);
                }
            }
            "app.dismiss_marks" => self.agent_marks.clear(),
            "edit.copy" => {
                if let Some(t) = self.editor.copy(&self.d) {
                    return iced::clipboard::write(t);
                }
            }
            "edit.cut" => {
                if let Some(t) = self.editor.cut(&mut self.d) {
                    let refresh = self.after_change(true);
                    return Task::batch([iced::clipboard::write(t), refresh]);
                }
            }
            "edit.paste" => return iced::clipboard::read().map(Message::Pasted),
            "edit.select_all" => self.editor.select_all(&mut self.d),
            "edit.bold" => self.editor.toggle_style(&mut self.d, 'b'),
            "edit.italic" => self.editor.toggle_style(&mut self.d, 'i'),
            "edit.underline" => self.editor.toggle_style(&mut self.d, 'u'),
            "edit.find" | "edit.replace" => {
                let mut f = self.find.take().unwrap_or_default();
                f.replace_open = name == "edit.replace" || f.replace_open;
                if self.editor.has_selection()
                    && let Some(t) = self.editor.copy(&self.d)
                    && !t.contains('\n')
                {
                    f.query = t;
                }
                self.find = Some(f);
                self.focus = Focus::Find;
                self.refresh_find();
                let r = self.after_change(false);
                return Task::batch([r, iced::widget::operation::focus(FIND)]);
            }
            "edit.find_next" => return self.find_step(false),
            "edit.find_previous" => return self.find_step(true),
            "view.zoom_in" => {
                self.settings.zoom_step(1);
                self.save_settings();
            }
            "view.zoom_out" => {
                self.settings.zoom_step(-1);
                self.save_settings();
            }
            "view.zoom_reset" => {
                self.settings.zoom = 1.0;
                self.save_settings();
            }
            "view.draft" => {
                self.settings.draft = !self.settings.draft;
                self.editor.draft = self.settings.draft;
                self.save_settings();
            }
            "view.navigator" => {
                self.settings.navigator = !self.settings.navigator;
                self.save_settings();
                scroll = false;
            }
            "view.focus" => {
                self.focus_mode = !self.focus_mode;
                scroll = false;
            }
            "view.next_pane" => {
                self.focus = match self.focus {
                    Focus::Page if self.settings.navigator => Focus::Navigator,
                    Focus::Page | Focus::Navigator if self.find.is_some() => Focus::Find,
                    _ => Focus::Page,
                };
                if self.focus == Focus::Find {
                    return iced::widget::operation::focus(FIND);
                }
                scroll = false;
            }
            "view.theme_system"
            | "view.theme_light"
            | "view.theme_dark"
            | "view.theme_high_contrast" => {
                self.settings.theme = match name {
                    "view.theme_light" => Choice::Light,
                    "view.theme_dark" => Choice::Dark,
                    "view.theme_high_contrast" => Choice::HighContrast,
                    _ => Choice::System,
                };
                self.save_settings();
                scroll = false;
            }
            "nav.previous_scene" => {
                self.editor
                    .motion(&mut self.d, Motion::PrevScene, false, Pt::ZERO);
            }
            "nav.next_scene" => {
                self.editor
                    .motion(&mut self.d, Motion::NextScene, false, Pt::ZERO);
            }
            "nav.move_scene_up" | "nav.move_scene_down" => {
                if let Some(i) = self.current_scene_row() {
                    let scenes: Vec<usize> = (0..self.nav.len())
                        .filter(|&j| !self.nav[j].section)
                        .collect();
                    if let Some(k) = scenes.iter().position(|&j| j == i) {
                        let id = self.nav[i].id;
                        let params = if name.ends_with("up") {
                            scenes
                                .get(k.wrapping_sub(1))
                                .map(|&j| json!({ "id": id, "before": self.nav[j].id }))
                        } else {
                            match scenes.get(k + 2) {
                                Some(&j) => Some(json!({ "id": id, "before": self.nav[j].id })),
                                None if k + 1 < scenes.len() => Some(json!({ "id": id })),
                                None => None,
                            }
                        };
                        if let Some(p) = params {
                            self.run("scene.move", p);
                        }
                    }
                }
            }
            "element.dual" => {
                let id = self.editor.caret.block;
                let dual = self
                    .d
                    .session()
                    .and_then(|s| s.doc.get(id))
                    .is_some_and(|b| b.element.dual);
                self.run("block.set_attrs", json!({ "id": id, "dual": !dual }));
            }
            n if n.starts_with("element.") => {
                if let Some(kind) = ElementKind::from_name(&n["element.".len()..]) {
                    self.editor.set_type(&mut self.d, kind);
                }
            }
            "app.quit" => {
                if self.d.session().is_some_and(|s| s.dirty) {
                    self.banners.push(
                        "This script has unsaved changes. Save it first, or close it from the palette with discard.".into(),
                    );
                } else {
                    self.d.shutdown();
                    return iced::exit();
                }
            }
            n if Command::NAMES.contains(&n) => {
                self.palette = Some(PaletteState {
                    params: Some((n.to_string(), "{}".to_string())),
                    ..PaletteState::default()
                });
                self.focus = Focus::Palette;
                return iced::widget::operation::focus(PALETTE);
            }
            other => self
                .banners
                .push(format!("\"{other}\" is not available yet.")),
        }
        self.after_change(scroll)
    }

    /// Saves in place, off the UI thread when there is a file
    /// (docs/specs/journal.md, "Saving off the UI thread"). For now the
    /// write runs here; the worker thread arrives with the file dialogs.
    fn save(&mut self) {
        let untitled = self
            .d
            .session()
            .and_then(|s| s.path.as_ref())
            .is_none_or(|p| {
                self.dirs
                    .as_ref()
                    .is_some_and(|d| p.starts_with(d.unsaved()))
            });
        if untitled {
            self.banners
                .push("Use Save as to choose where to keep this script.".into());
            return;
        }
        match self.d.save_prepare(&Default::default()) {
            Ok(prepared) => {
                let written = coldopen_core::io::atomic_write(&prepared.target, &prepared.bytes);
                if let Err(e) = self.d.save_commit(prepared, written) {
                    self.banners.push(e.message);
                }
            }
            Err(e) if e.code == "lossy_format" => self.banners.push(format!(
                "{} Use the palette's doc.save with allow_lossy to save anyway.",
                e.message
            )),
            Err(e) => self.banners.push(e.message),
        }
    }

    fn palette_items(&self) -> Vec<(&'static str, &'static str)> {
        let q = self.palette.as_ref().map_or("", |p| p.query.as_str());
        let mut items: Vec<(i32, &'static str, &'static str)> = actions::all()
            .into_iter()
            .filter_map(|(n, t)| actions::score(q, n, t).map(|s| (s, n, t)))
            .collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.0));
        items.into_iter().take(12).map(|(_, n, t)| (n, t)).collect()
    }

    fn palette_run(&mut self, i: usize) -> Task<Message> {
        let items = self.palette_items();
        let Some(&(name, _)) = items.get(i.min(items.len().saturating_sub(1))) else {
            return Task::none();
        };
        self.palette = None;
        self.focus = Focus::Page;
        self.action(name)
    }

    fn params_run(&mut self) -> Task<Message> {
        let Some((name, json)) = self.palette.as_ref().and_then(|p| p.params.clone()) else {
            return Task::none();
        };
        let response = match serde_json::from_str::<Value>(&json) {
            Ok(params) => {
                let r = self.d.handle(&json!({ "command": name, "params": params }));
                serde_json::to_string_pretty(&r).unwrap_or_default()
            }
            Err(e) => format!("The params are not JSON: {e}"),
        };
        if let Some(p) = &mut self.palette {
            let mut short: String = response.chars().take(4000).collect();
            if response.len() > short.len() {
                short.push_str("\n…");
            }
            p.response = Some(short);
        }
        self.after_change(false)
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let keys = iced::event::listen_with(|event, status, _window| match (event, status) {
            (
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    text,
                    ..
                }),
                iced::event::Status::Ignored,
            ) => Some(Message::GlobalKey(KeyPress {
                key,
                modifiers,
                text: text.map(|t| t.to_string()),
            })),
            _ => None,
        });
        Subscription::batch([
            keys,
            iced::time::every(Duration::from_millis(530)).map(|_| Message::Tick),
            iced::system::theme_changes()
                .map(|m| Message::SystemTheme(m == iced::theme::Mode::Dark)),
        ])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let pal = self.palette_colors();
        let (Some(view), Some(session)) = (self.d.cached_view(), self.d.session()) else {
            return container(text("No script is open.").font(fonts::UI))
                .center(Fill)
                .into();
        };
        let doc = &session.doc;
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
            focused: self.focus == Focus::Page && self.palette.is_none(),
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
        let mut body = row![];
        if self.settings.navigator && !self.focus_mode {
            body = body.push(self.navigator(pal));
        }
        body = body.push(pages);
        let mut main = column![];
        if !self.focus_mode {
            main = main.push(self.hint_bar(pal));
        }
        if !self.banners.is_empty() {
            main = main.push(self.banner(pal));
        }
        if self.find.is_some() {
            main = main.push(self.find_bar(pal));
        }
        main = main.push(body.height(Fill));
        if !self.focus_mode {
            main = main.push(self.status_bar(pal));
        }
        let mut layers = stack![main];
        if self.palette.is_some() {
            layers = layers.push(self.palette_view(pal));
        }
        layers.into()
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
        let r = row![
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
            text(format!("{}%", (self.settings.zoom * 100.0).round() as u32))
                .font(fonts::UI)
                .size(12)
                .color(pal.text_quiet),
        ]
        .spacing(16)
        .align_y(Center);
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
            for (i, (name, title)) in items.iter().enumerate() {
                let keys = self.keymap.keys_for(name).join(", ");
                let selected = i == sel;
                let fg = if selected { pal.accent_text } else { pal.text };
                let item = row![
                    text(*title).font(fonts::UI).size(13).color(fg).width(Fill),
                    text(*name).font(fonts::UI).size(11).color(pal.text_quiet),
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
        container(column![space().height(Length::Fixed(80.0)), card].align_x(Center))
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

/// A length in eighths of a page, as scripts write it.
pub fn eighths(n: u64) -> String {
    match (n / 8, n % 8) {
        (0, r) => format!("{r}/8"),
        (w, 0) => format!("{w}"),
        (w, r) => format!("{w} {r}/8"),
    }
}

/// The settings iced needs to draw the app: our fonts, and the interface
/// face as the default.
pub fn iced_settings() -> iced::Settings {
    iced::Settings {
        fonts: fonts::files()
            .into_iter()
            .map(std::borrow::Cow::Borrowed)
            .collect(),
        default_font: fonts::UI,
        default_text_size: iced::Pixels(14.0),
        ..iced::Settings::default()
    }
}
