//! The application: state, messages, update and view
//! (docs/specs/editor.md). The app owns the dispatcher, so the writer's
//! keystrokes and agent requests are applied one at a time in this update
//! loop.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use coldopen_core::{
    BlockId, Command, CommandError, Dispatcher, Durability, Options, PreparedSave,
};
use coldopen_screenplay::{ChangeId, ElementKind, Pt};
use iced::keyboard::{self, Key, key::Named};
use iced::widget::scrollable;
use iced::{Subscription, Task, Theme};
use serde_json::{Value, json};

mod agents;
mod bench;
mod changes;
mod copies;
mod file_ops;
mod router;
mod view;

pub use agents::Agent;
pub use bench::{Bench, BenchKind};
pub use changes::{ALL_AUTHORS, ChangeRow, ChangesPanel, NameThen};
pub use copies::{CopyRow, RecoveredScreen, RepairReport};
pub use file_ops::{After, ExportForm, Prompt, Recovery, Reply};
pub use router::{Origin, Outcome, Request};
pub use view::eighths;

use crate::actions;
use crate::editor::{Editor, Motion, Spot};
use crate::files::{Ask, Dialogs, Dirs, Settings};
use crate::fonts;
use crate::keymap::{Chord, Keymap, MAC};
use crate::page_view::{self, KeyPress, NormRange, PageEvent};
use crate::theme::{Choice, Palette};

const PAGES: &str = "pages";
const FIND: &str = "find";
const PALETTE: &str = "palette";
const EXPORT: &str = "export";

/// Who has keyboard focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Page,
    Navigator,
    /// The changes panel (editor.md §17.3).
    Changes,
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
    /// Closes the notice the bar shows; the next one takes its place.
    DismissBanner,
    /// Closes one notice of the full list, by its place in `banners`.
    DismissBannerAt(usize),
    DismissAllBanners,
    /// Opens or closes the full list of notices.
    BannersList(bool),
    /// A file dialog's answer: a path, or `None` for Cancel.
    Chosen(Ask, Option<PathBuf>),
    /// The save worker rendered the file (docs/specs/journal.md, "Saving
    /// off the UI thread").
    Rendered(coldopen_core::RenderedSave),
    /// The save worker finished writing it.
    Saved(Result<(), String>),
    Reply(Reply),
    RecoveryKeep,
    RecoveryDiscard,
    ExportNumbers,
    ExportTitle,
    ExportWatermark(String),
    ExportGo,
    ExportCancel,
    /// A recent file or leftover work to reopen.
    OpenPath(PathBuf),
    DismissLeftover,
    /// The window's close button.
    CloseRequested,
    /// Esc pressed in a text field.
    Escaped,
    /// A click outside the open popup (editor.md §2).
    ClickAway,
    /// The window is about to draw a frame.
    Frame,
    /// The agent link (docs/specs/mcp.md §5).
    Link(crate::link::LinkEvent),
    /// The repair dialog (editor.md §13).
    RepairOk,
    RepairDetails,
    RepairSaveCopy,
    /// Recovered versions (editor.md §13).
    RecoveredSelect(usize),
    RecoveredRestore,
    RecoveredDelete,
    RecoveredClose,
    /// The writer's name, as typed in its question (editor.md §17.4).
    NameInput(String),
    NameSubmit,
    /// The changes panel (editor.md §17.3): a row clicked, a row's Accept
    /// or Reject button, the author filter.
    ChangesPick(usize),
    ChangesSettle(usize, bool),
    ChangesFilter(String),
    /// The export form's "Print the current text".
    ExportChanges,
    Noop,
}

/// A message over the page (editor.md §13). Some offer the Recovered
/// versions screen; none shows the path of a set-aside copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Banner {
    pub text: String,
    /// Shows a Recovered versions button.
    pub recovered: bool,
}

impl Banner {
    /// "Your unsaved changes were set aside…", with the button.
    pub fn set_aside() -> Banner {
        Banner {
            text: "Your unsaved changes were set aside. Open Recovered versions to restore them."
                .into(),
            recovered: true,
        }
    }

    /// A save to a format that cannot hold tracked changes accepted them
    /// (editor.md §17.5); `kept` when the version with them marked was set
    /// aside.
    pub fn accepted(count: u64, kept: bool) -> Banner {
        let (changes, them) = if count == 1 {
            ("the pending change".to_string(), "it")
        } else {
            (format!("{count} pending changes"), "them")
        };
        Banner {
            text: if kept {
                format!(
                    "Saving accepted {changes}. The version with {them} marked is in Recovered versions."
                )
            } else {
                format!("Saving accepted {changes}.")
            },
            recovered: kept,
        }
    }

    /// The first save over a repaired file kept it as it was.
    pub fn original_kept() -> Banner {
        Banner {
            text:
                "The file as it was before the repair was kept. Open Recovered versions to see it."
                    .into(),
            recovered: true,
        }
    }
}

impl From<String> for Banner {
    fn from(text: String) -> Banner {
        Banner {
            text,
            recovered: false,
        }
    }
}

impl From<&str> for Banner {
    fn from(text: &str) -> Banner {
        Banner::from(text.to_string())
    }
}

impl std::fmt::Display for Banner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl std::ops::Deref for Banner {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
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
    /// The heading is a pending deletion.
    pub deleted: bool,
    /// The scene's first pending change, for the dot in its color.
    pub change: Option<ChangeId>,
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

/// A read-only sheet over the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    /// The undo history, with each step's label and author (editor.md §12).
    History,
    /// Version and licenses.
    About,
}

/// One line of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PaletteItem {
    /// What runs: an action or command name, `recent:<n>` or
    /// `leftover:<n>`.
    action: String,
    title: String,
    /// The command's name, or a file's folder.
    detail: String,
}

#[derive(Debug, Default)]
struct PaletteState {
    query: String,
    selected: usize,
    /// A command being given params as JSON, and the last response.
    params: Option<(String, String)>,
    response: Option<String>,
}

/// A save in place on its way through the worker (docs/specs/journal.md,
/// "Saving off the UI thread").
#[derive(Debug)]
pub(super) enum Saving {
    /// The worker is rendering the file; with the save's `allow_lossy`, for
    /// the question a changed file asks.
    Rendering { allow_lossy: bool },
    /// The worker is writing these bytes.
    Writing(PreparedSave),
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
    pub banners: Vec<Banner>,
    /// The full list of notices is open (editor.md §13).
    pub banners_open: bool,
    viewport: Option<scrollable::Viewport>,
    fonts: Box<dyn coldopen_layout::Coverage>,
    keymap: Keymap,
    nav: Vec<NavRow>,
    nav_rev: Option<u64>,
    /// The navigator waits for the next tick (see `after_change`).
    nav_stale: bool,
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
    /// The element flow: the template with the writer's `flow.json`.
    flow: crate::editor::flow::FlowTable,
    dialogs: Dialogs,
    /// A save in place on the worker thread.
    saving: Option<Saving>,
    /// Save again when the running save finishes.
    save_again: bool,
    /// What to do once the script is saved.
    after_save: Option<After>,
    /// What waits for the save in flight to finish before it runs: opening,
    /// closing or leaving a script never overlaps a save (editor.md §16).
    waiting: Option<After>,
    pub prompt: Option<Prompt>,
    pub recovery: Option<Recovery>,
    /// The repair report, after opening a damaged .fdx file or repairing
    /// one.
    pub repair: Option<RepairReport>,
    /// The Recovered versions screen.
    pub recovered: Option<RecoveredScreen>,
    pub export: Option<ExportForm>,
    pub sheet: Option<Sheet>,
    /// Scripts with unsaved work from an earlier session (editor.md §11).
    pub leftover: Vec<PathBuf>,
    /// The agent link server, while agent access is on and a script is
    /// open (editor.md §12).
    pub link: Option<crate::link::LinkServer>,
    /// The path the link last published, to notice a change.
    link_path: Option<PathBuf>,
    /// Attached agents, by link.
    pub agents: std::collections::BTreeMap<u64, Agent>,
    /// The last revision announced to agents.
    sent_rev: Option<u64>,
    /// See [`Boot::exit_after_first_frame`].
    first_frame: Option<Instant>,
    bench: Option<bench::BenchState>,
    /// See [`App::theme`].
    scroll_flip: bool,
    /// The link could not start; said once.
    link_failed: bool,
    /// The changes panel (editor.md §17.3).
    pub changes: ChangesPanel,
    /// Pending changes, and how many of them are agents'.
    pub review: (usize, usize),
    /// The writer's name was asked for in this session (editor.md §17.4).
    name_asked: bool,
    /// The name question opened while a script opened: its field takes
    /// the keyboard after the next change.
    focus_name: bool,
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
    /// Exit after the first frame, printing how long it took since this
    /// instant (cold-start measurement and the windowed smoke test).
    pub exit_after_first_frame: Option<Instant>,
    /// Run a self-driving benchmark, report it and exit.
    pub bench: Option<Bench>,
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
        let mut banners: Vec<Banner> = warn.into_iter().map(Banner::from).collect();
        let keymap_text = dirs
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.config.join("keymap.json")).ok());
        let (keymap, kw) = Keymap::load(keymap_text.as_deref(), &actions::known);
        banners.extend(kw.into_iter().map(Banner::from));

        let mut d = Dispatcher::new(Options::default());
        if !boot.every_entry {
            d.set_durability(Durability::APP);
        }
        d.set_writer_name(settings.author_name.clone());
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
            banners_open: false,
            viewport: None,
            fonts: coverage(),
            keymap,
            nav: Vec::new(),
            nav_rev: None,
            nav_stale: false,
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
            flow: crate::editor::flow::FlowTable::default(),
            dialogs: Dialogs::native(),
            saving: None,
            save_again: false,
            after_save: None,
            waiting: None,
            prompt: None,
            recovery: None,
            repair: None,
            recovered: None,
            export: None,
            sheet: None,
            leftover: Vec::new(),
            link: None,
            link_path: None,
            agents: std::collections::BTreeMap::new(),
            sent_rev: None,
            link_failed: false,
            first_frame: boot.exit_after_first_frame,
            bench: boot.bench.clone().map(bench::BenchState::new),
            scroll_flip: false,
            changes: ChangesPanel::default(),
            review: (0, 0),
            name_asked: false,
            focus_name: false,
        };
        if let Some(text) = app
            .dirs
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.config.join("flow.json")).ok())
        {
            let w = app.flow.apply_overrides(&text);
            app.banners.extend(w.into_iter().map(Banner::from));
        }
        app.clean_up_copies();
        match boot.file {
            Some(f) => app.open(&f),
            None => {
                if let Some(d) = &app.dirs {
                    app.leftover = crate::files::leftover_work(d, &app.settings.recent);
                }
                app.new_script();
            }
        }
        app.editor.draft = app.settings.draft;
        let _ = app.after_change(false);
        app
    }

    /// [`App::new`] with the task the window's first frame runs: when the
    /// script it opened asks for the writer's name, the name's field takes
    /// the keyboard (editor.md §17.4).
    pub fn boot(boot: Boot) -> (App, Task<Message>) {
        let app = App::new(boot);
        let task = if matches!(app.prompt, Some(Prompt::AuthorName { .. })) {
            Task::batch([
                iced::widget::operation::focus(changes::NAME),
                iced::widget::operation::select_all(changes::NAME),
            ])
        } else {
            Task::none()
        };
        (app, task)
    }

    /// An editor for a document just opened or created, with the writer's
    /// flow table and view.
    pub(super) fn fresh_editor(&self, doc: &coldopen_core::Document) -> Editor {
        let mut e = Editor::new(doc);
        e.flow = self.flow.clone();
        e.draft = self.settings.draft;
        e
    }

    /// Answers file dialogs some other way than the system's dialogs
    /// (tests and the gate driver).
    pub fn set_dialogs(&mut self, dialogs: Dialogs) {
        self.dialogs = dialogs;
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
            let c = crate::editor::Caret::at(id, len);
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
        let mut pal = self.palette_colors();
        // While the pages scroll, the window background alternates between
        // two colours one step apart, which nobody can see. A change of
        // background makes iced's CPU renderer redraw the window in one
        // pass; otherwise it splits a scroll's damage into dozens of
        // regions and redraws each, about five times slower
        // (docs/progress/phase-3.md, measurements).
        if self.scroll_flip {
            pal.panel.b = if pal.panel.b > 0.5 {
                pal.panel.b - 1.0 / 255.0
            } else {
                pal.panel.b + 1.0 / 255.0
            };
        }
        pal.theme()
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
                self.banners.push(e.into());
                return None;
            }
        };
        match self.d.execute(cmd) {
            Ok(v) => Some(v),
            Err(e) => {
                self.banners.push(e.message.into());
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
            Some(p) => json!({ "path": p, "start": "scene_heading" }),
            None => json!({ "start": "scene_heading" }),
        };
        if self.run("doc.new", params).is_none() {
            self.run("doc.new", json!({ "start": "scene_heading" }));
        }
        if let Some(s) = self.d.session() {
            self.editor = self.fresh_editor(&s.doc);
        }
    }

    pub fn open(&mut self, path: &Path) {
        self.open_with(path, "auto");
    }

    /// Opens a script with `recover` (`auto`, `apply` or `discard`). A crash
    /// journal written on another computer or for another folder is offered
    /// first (docs/specs/journal.md, "Header").
    pub(super) fn open_with(&mut self, path: &Path, recover: &str) {
        let opened = Command::from_parts("doc.open", json!({ "path": path, "recover": recover }))
            .map_err(CommandError::invalid_params)
            .and_then(|cmd| self.d.execute(cmd));
        let opened = match opened {
            Err(e) if e.code == "journal_foreign" => {
                let detail = |k: &str| {
                    e.details
                        .as_ref()
                        .and_then(|d| d[k].as_str())
                        .unwrap_or("unknown")
                        .to_string()
                };
                self.prompt = Some(Prompt::Foreign {
                    path: path.to_path_buf(),
                    host: detail("host"),
                    at: detail("path"),
                });
                return;
            }
            Err(e) => {
                self.banners.push(e.message.into());
                None
            }
            Ok(r) => Some(r),
        };
        match opened {
            Some(r) => {
                self.notices(&r);
                if let Some(s) = self.d.session() {
                    self.editor = self.fresh_editor(&s.doc);
                }
                self.recovery_from(&r["recovered"], path);
                self.repair_from(&r["repaired"], path);
                self.ask_name_on_open();
                self.leftover.retain(|p| p != path);
                let unsaved = self
                    .dirs
                    .as_ref()
                    .is_some_and(|d| path.starts_with(d.unsaved()));
                if !unsaved {
                    self.settings.add_recent(path);
                    self.save_settings();
                }
            }
            // The banner says why; the script that was open stays open.
            None if self.d.session().is_none() => self.new_script(),
            None => {}
        }
    }

    fn save_settings(&mut self) {
        if let Some(d) = &self.dirs
            && let Err(e) = self.settings.save(d)
        {
            self.banners
                .push(format!("Settings could not be saved: {e}").into());
        }
    }

    /// Everything that follows a change: refresh the layout, keep the
    /// caret inside the document, recompute what the view draws, and
    /// scroll the caret into sight.
    fn after_change(&mut self, scroll: bool) -> Task<Message> {
        self.d.view();
        self.sync_link();
        if let Some(s) = self.d.session() {
            self.editor.fix_caret(&s.doc);
        }
        self.editor
            .problems
            .drain(..)
            .for_each(|p| self.banners.push(p.message.into()));
        self.caret_spot = self.editor.caret_spot(&mut self.d);
        self.caret_on = true;
        self.last_input = Instant::now();
        self.refresh_ranges();
        // Typing in body text changes only the navigator's page numbers and
        // lengths, so that waits for the next tick (half a second); anything
        // else, such as a heading, a new block or an agent's edit, shows at
        // once. It saves about 1.5 ms a key on a feature script.
        if self.typing_in_body() && self.nav_rev.is_some() {
            self.nav_stale = true;
        } else {
            self.refresh_nav();
        }
        let task = if scroll {
            self.scroll_to_caret()
        } else {
            Task::none()
        };
        if std::mem::take(&mut self.focus_name) {
            return Task::batch([
                task,
                iced::widget::operation::focus(changes::NAME),
                iced::widget::operation::select_all(changes::NAME),
            ]);
        }
        task
    }

    /// Selection, find matches and agent marks in normal-form terms.
    fn refresh_ranges(&mut self) {
        let (Some(view), Some(s)) = (self.d.cached_view(), self.d.session()) else {
            return;
        };
        let doc = &s.doc;
        let norm = |c: crate::editor::Caret| -> Option<(usize, u32)> {
            let bi = doc.index_of(c.block)?;
            // The pages show pending deletions (editor.md §4).
            let b = c.shown_byte(&doc.blocks[bi].element.text);
            view.sources.to_norm(bi, b as u32)
        };
        let at = crate::editor::Caret::at;
        self.sel_range = if self.editor.has_selection() {
            let (a, b) = self.editor.selection(doc);
            match (norm(a), norm(b)) {
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
                    .filter_map(|&(id, from, to)| Some((norm(at(id, from))?, norm(at(id, to))?)))
                    .collect()
            })
            .unwrap_or_default();
        self.marked_elems = page_view::marked_elements(view, doc, &self.agent_marks);
    }

    /// The newest change was the writer typing or deleting in a block that
    /// is not a heading or section.
    fn typing_in_body(&self) -> bool {
        self.d.session().is_some_and(|s| {
            let top = s.undo_steps().next().map(|(c, _, by)| (c, by.is_writer()));
            matches!(top, Some(("typing" | "text.insert" | "text.delete", true)))
                && s.doc.get(self.editor.caret.block).is_some_and(|b| {
                    !matches!(
                        b.element.kind,
                        ElementKind::SceneHeading | ElementKind::Section
                    )
                })
        })
    }

    fn refresh_nav(&mut self) {
        self.nav_stale = false;
        let rev = self.d.session().map(|s| s.rev);
        if rev == self.nav_rev {
            return;
        }
        self.nav_rev = rev;
        self.refresh_changes();
        // With no script open there is nothing to list, and asking would
        // say "no document is open" over the start screen.
        if rev.is_none() {
            self.nav.clear();
            return;
        }
        let scenes = self
            .run("scene.list", json!({}))
            .and_then(|r| r["scenes"].as_array().cloned())
            .unwrap_or_default();
        let Some(doc) = self.d.session().map(|s| &s.doc) else {
            return;
        };
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
                    deleted: b.marks.deleted.is_some(),
                    change: None,
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
                            deleted: b.marks.deleted.is_some(),
                            change: None,
                        });
                    }
                }
                _ => {}
            }
            // A scene holding pending changes gets a dot (editor.md §17.1).
            if let Some(last) = rows.last_mut()
                && !last.section
                && last.change.is_none()
            {
                last.change = b.change_ids().next();
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

    /// Pixels per Pt unit at `zoom`, the height of a page (or of the draft
    /// view's one long column) in pixels, and how many there are.
    fn page_geometry(&self, zoom: f32) -> Option<(f32, f32, usize)> {
        let view = self.d.cached_view()?;
        let f = view.layout().format();
        let k = zoom * 96.0 / 72.0 / 100.0;
        if self.editor.draft {
            let h = self
                .editor
                .draft_index
                .draft
                .as_ref()
                .map_or(0, |d| d.height.0)
                + f.margin_top.0
                + f.margin_bottom.0;
            Some((k, h as f32 * k, 1))
        } else {
            Some((
                k,
                f.page_height.0 as f32 * k,
                view.layout().pages.len().max(1),
            ))
        }
    }

    /// The page and the Pt-unit height on it at a y in the page area's
    /// content, at the current zoom.
    fn page_at(&self, y: f32) -> Option<(usize, f32)> {
        let (k, page_h, pages) = self.page_geometry(self.settings.zoom)?;
        let step = page_h + page_view::GAP;
        let p = (((y - page_view::MARGIN) / step).floor().max(0.0) as usize).min(pages - 1);
        Some((p, (y - page_view::MARGIN - p as f32 * step) / k))
    }

    /// The page in the middle of the view, from 0; the draft view is one
    /// page.
    pub fn visible_page(&self) -> Option<usize> {
        let vp = self.viewport.as_ref()?;
        let mid = vp.absolute_offset().y + vp.bounds().height / 2.0;
        self.page_at(mid).map(|(p, _)| p)
    }

    /// What stays put on screen when zooming: the caret's line when it is
    /// in view, otherwise the middle of the view. As (page, height on that
    /// page in Pt units, distance from the top of the page area).
    fn zoom_anchor(&self) -> Option<(usize, f32, f32)> {
        if let (Some(y), Some(spot)) = (self.caret_screen_y(), self.caret_spot) {
            return Some((spot.page, spot.top.0 as f32, y));
        }
        let vp = self.viewport.as_ref()?;
        let h = vp.bounds().height;
        let (p, y_pt) = self.page_at(vp.absolute_offset().y + h / 2.0)?;
        Some((p, y_pt, h / 2.0))
    }

    /// The palette's selected row, while it is open.
    pub fn palette_selection(&self) -> Option<usize> {
        self.palette.as_ref().map(|p| p.selected)
    }

    pub fn find_open(&self) -> bool {
        self.find.is_some()
    }

    /// The caret's top-left corner in the window, when it is in view.
    pub fn caret_window_point(&self) -> Option<iced::Point> {
        let y = self.caret_screen_y()?;
        let vp = self.viewport.as_ref()?;
        let spot = self.caret_spot?;
        let (k, _, _) = self.page_geometry(self.settings.zoom)?;
        let page_w = self.d.cached_view()?.layout().format().page_width.0 as f32 * k;
        let left = ((vp.content_bounds().width - page_w) / 2.0).max(page_view::MARGIN);
        let b = vp.bounds();
        Some(iced::Point::new(
            b.x + left - vp.absolute_offset().x + spot.x.0 as f32 * k,
            b.y + y,
        ))
    }

    /// How far the caret's line is below the top of the page area, when it
    /// is in view.
    pub fn caret_screen_y(&self) -> Option<f32> {
        let vp = self.viewport.as_ref()?;
        let spot = self.caret_spot?;
        let (k, page_h, _) = self.page_geometry(self.settings.zoom)?;
        let y = page_view::MARGIN
            + spot.page as f32 * (page_h + page_view::GAP)
            + spot.top.0 as f32 * k;
        let top = vp.absolute_offset().y;
        (y >= top && y < top + vp.bounds().height).then_some(y - top)
    }

    /// Zooms, keeping the caret's line, or else the middle of the view,
    /// where it was on screen (editor.md §3).
    fn zoom_to(&mut self, zoom: f32) -> Task<Message> {
        let anchor = self.zoom_anchor();
        self.settings.zoom = zoom;
        self.save_settings();
        let refresh = self.after_change(false);
        let (Some((page, y_pt, on_screen)), Some((k, page_h, _))) =
            (anchor, self.page_geometry(zoom))
        else {
            return refresh;
        };
        let y = page_view::MARGIN + page as f32 * (page_h + page_view::GAP) + y_pt * k;
        let keep = iced::widget::operation::scroll_to(
            PAGES,
            scrollable::AbsoluteOffset {
                x: None,
                y: Some((y - on_screen).max(0.0)),
            },
        );
        Task::batch([refresh, keep])
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
                if self.nav_stale {
                    self.refresh_nav();
                }
                Task::none()
            }
            Message::Scrolled(vp) => {
                let moved = self
                    .viewport
                    .as_ref()
                    .is_none_or(|old| old.absolute_offset() != vp.absolute_offset());
                if moved {
                    self.scroll_flip = !self.scroll_flip;
                }
                self.viewport = Some(vp);
                Task::none()
            }
            Message::SystemTheme(dark) => {
                self.system_dark = dark;
                Task::none()
            }
            Message::Action(a) => self.dispatch(Request::Action(a), Origin::Button).task,
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
                self.banners.pop();
                self.banners_open &= self.banners.len() > 1;
                Task::none()
            }
            Message::DismissBannerAt(i) => {
                if i < self.banners.len() {
                    self.banners.remove(i);
                }
                self.banners_open &= self.banners.len() > 1;
                Task::none()
            }
            Message::DismissAllBanners => {
                self.banners.clear();
                self.banners_open = false;
                Task::none()
            }
            Message::BannersList(open) => {
                self.banners_open = open && self.banners.len() > 1;
                Task::none()
            }
            Message::Chosen(ask, path) => self.chosen(ask, path),
            Message::Rendered(r) => self.rendered(r),
            Message::Saved(r) => self.saved(r),
            Message::Reply(r) => self.reply(r),
            Message::RecoveryKeep => self.recovery_keep(),
            Message::RepairOk | Message::RepairDetails | Message::RepairSaveCopy => {
                self.repair_reply(&message)
            }
            Message::RecoveredSelect(i) => {
                self.recovered_select(i);
                Task::none()
            }
            Message::RecoveredRestore => self.recovered_restore(),
            Message::RecoveredDelete => self.recovered_delete(),
            Message::RecoveredClose => {
                self.recovered = None;
                Task::none()
            }
            Message::NameInput(t) => {
                if let Some(Prompt::AuthorName { name, .. }) = &mut self.prompt {
                    *name = t;
                }
                Task::none()
            }
            Message::NameSubmit => self.name_done(true),
            Message::ChangesPick(i) => {
                self.focus = Focus::Changes;
                self.goto_change_row(i)
            }
            Message::ChangesSettle(i, accept) => self.settle_row(i, accept),
            Message::ChangesFilter(f) => {
                self.changes.filter = (f != ALL_AUTHORS).then_some(f);
                self.changes.selected = 0;
                Task::none()
            }
            Message::ExportChanges => {
                if let Some(f) = &mut self.export {
                    f.accepted = !f.accepted;
                }
                Task::none()
            }
            Message::RecoveryDiscard => self.recovery_discard(),
            Message::ExportNumbers => {
                if let Some(f) = &mut self.export {
                    f.scene_numbers = !f.scene_numbers;
                }
                Task::none()
            }
            Message::ExportTitle => {
                if let Some(f) = &mut self.export {
                    f.title_page = !f.title_page;
                }
                Task::none()
            }
            Message::ExportWatermark(w) => {
                if let Some(f) = &mut self.export {
                    f.watermark = w;
                }
                Task::none()
            }
            Message::ExportGo => self.export_choose(),
            Message::ExportCancel => {
                self.export = None;
                Task::none()
            }
            Message::OpenPath(p) => self.leave(After::OpenPath(p)),
            Message::DismissLeftover => {
                self.leftover.clear();
                Task::none()
            }
            Message::CloseRequested => self.leave(After::Quit),
            Message::Link(ev) => self.link_event(ev),
            Message::Frame if self.bench.is_some() => self.bench_frame(),
            Message::Frame => match self.first_frame.take() {
                Some(t0) => {
                    println!(
                        "first frame {:.1} ms after launch",
                        t0.elapsed().as_secs_f64() * 1000.0
                    );
                    self.d.shutdown();
                    iced::exit()
                }
                None => Task::none(),
            },
            Message::Escaped => {
                if matches!(self.prompt, Some(Prompt::AuthorName { .. })) {
                    return self.name_done(false);
                }
                if self.palette.is_some() {
                    self.palette = None;
                    self.focus = Focus::Page;
                } else if self.export.is_some() {
                    self.export = None;
                } else if self.focus == Focus::Find {
                    self.find = None;
                    self.focus = Focus::Page;
                    return self.after_change(false);
                }
                Task::none()
            }
            Message::ClickAway => self.click_away(),
            Message::Noop => Task::none(),
        }
    }

    /// A click outside the open popup closes it, as Esc does, and does
    /// nothing else (editor.md §2). The save and recovery dialogs decide
    /// what happens to the writer's work, so a stray click leaves them open.
    fn click_away(&mut self) -> Task<Message> {
        if self.palette.is_some() {
            self.palette = None;
            self.focus = Focus::Page;
            return Task::none();
        }
        if self.modal() {
            if self.prompt.is_none() && self.recovery.is_none() {
                self.export = None;
                self.sheet = None;
            }
            return Task::none();
        }
        self.editor.dismiss_completion(&self.d);
        if self.find.is_some() {
            self.find = None;
            self.focus = Focus::Page;
            return self.after_change(false);
        }
        Task::none()
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
            PageEvent::Pick(i) => {
                if let Some(c) = &mut self.editor.completion {
                    c.selected = i;
                }
                self.editor.accept_completion(&mut self.d);
                self.clear_marks_at_caret();
                self.after_change(true)
            }
            PageEvent::Dismiss => self.click_away(),
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
                // Esc closes the nearest thing: the find bar, a selection,
                // then messages and the leftover-work offer.
                if self.find.is_some() || self.palette.is_some() {
                    self.find = None;
                    self.palette = None;
                } else if self.editor.has_selection() {
                    self.editor.anchor = self.editor.caret;
                } else {
                    self.dismiss_messages();
                }
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

    /// Closes the full list of notices, then clears the notices, then
    /// (once they are gone) the leftover-work offer: Esc and the palette
    /// reach all three.
    fn dismiss_messages(&mut self) {
        if self.banners_open {
            self.banners_open = false;
        } else if self.banners.is_empty() {
            self.leftover.clear();
        } else {
            self.banners.clear();
        }
    }

    /// A dialog is open over the window, so the page takes no keys.
    pub(super) fn modal(&self) -> bool {
        self.prompt.is_some()
            || self.recovery.is_some()
            || self.repair.is_some()
            || self.recovered.is_some()
            || self.export.is_some()
            || self.sheet.is_some()
    }

    /// Keys for an open dialog (editor.md §11, §13): each button has a key,
    /// and nothing else runs while it is open.
    fn modal_key(&mut self, kp: &KeyPress) -> Task<Message> {
        let ch = match kp.key.as_ref() {
            Key::Character(c) => c.to_lowercase(),
            _ => String::new(),
        };
        let cmd = kp.modifiers.command();
        let enter = matches!(kp.key.as_ref(), Key::Named(Named::Enter));
        let esc = matches!(kp.key.as_ref(), Key::Named(Named::Escape));
        if matches!(self.prompt, Some(Prompt::Foreign { .. })) {
            return if enter || ch == "r" {
                self.reply(Reply::Recover)
            } else if ch == "s" {
                self.reply(Reply::SetAside)
            } else if esc || ch == "c" {
                self.reply(Reply::Cancel)
            } else {
                Task::none()
            };
        }
        if matches!(self.prompt, Some(Prompt::Changed { .. })) {
            // Enter takes the choice that keeps both versions.
            return if enter || ch == "a" {
                self.reply(Reply::SaveAs)
            } else if ch == "o" {
                self.reply(Reply::Overwrite)
            } else if ch == "r" {
                self.reply(Reply::Reload)
            } else if esc || ch == "c" {
                self.reply(Reply::Cancel)
            } else {
                Task::none()
            };
        }
        if matches!(self.prompt, Some(Prompt::AuthorName { .. })) {
            return if enter {
                self.name_done(true)
            } else if esc {
                self.name_done(false)
            } else {
                Task::none()
            };
        }
        // Saving the script itself offers a project bundle, which Enter
        // chooses when tracked changes would be lost (editor.md §17.5).
        if let Some(Prompt::Lossy {
            export: false,
            tracked,
            ..
        }) = &self.prompt
        {
            let tracked = *tracked > 0;
            return if ch == "p" || (enter && tracked) {
                self.reply(Reply::SaveAsProject)
            } else if enter || ch == "s" {
                self.reply(Reply::Save)
            } else if ch == "d" || ch == "n" {
                self.reply(Reply::DontSave)
            } else if esc || ch == "c" {
                self.reply(Reply::Cancel)
            } else {
                Task::none()
            };
        }
        if matches!(self.prompt, Some(Prompt::DeleteCopy(_))) {
            return if enter || ch == "d" {
                self.reply(Reply::Save)
            } else if esc || ch == "c" {
                self.reply(Reply::Cancel)
            } else {
                Task::none()
            };
        }
        if self.prompt.is_some() {
            return if enter || ch == "s" {
                self.reply(Reply::Save)
            } else if ch == "d" || ch == "n" {
                self.reply(Reply::DontSave)
            } else if esc || ch == "c" {
                self.reply(Reply::Cancel)
            } else {
                Task::none()
            };
        }
        if self.recovery.is_some() {
            return if enter || esc || ch == "k" {
                self.recovery_keep()
            } else if ch == "d" {
                self.recovery_discard()
            } else {
                Task::none()
            };
        }
        if let Some(r) = &self.repair {
            return if enter || esc || ch == "o" {
                self.repair_reply(&Message::RepairOk)
            } else if ch == "d" && !r.issues.is_empty() {
                self.repair_reply(&Message::RepairDetails)
            } else if ch == "s" && r.written.is_none() {
                self.repair_reply(&Message::RepairSaveCopy)
            } else {
                Task::none()
            };
        }
        if let Some(s) = &self.recovered {
            let selected = s.selected;
            match kp.key.as_ref() {
                Key::Named(Named::ArrowDown) => self.recovered_select(selected + 1),
                Key::Named(Named::ArrowUp) => self.recovered_select(selected.saturating_sub(1)),
                Key::Named(Named::Delete) => return self.recovered_delete(),
                _ if enter || ch == "r" => return self.recovered_restore(),
                _ if esc => self.recovered = None,
                _ => {}
            }
            return Task::none();
        }
        if let Some(sheet) = self.sheet {
            if esc || enter {
                self.sheet = None;
            } else if sheet == Sheet::History && cmd && ch == "z" {
                // Step through the history while watching it.
                let redo = kp.modifiers.shift();
                self.editor.undo(&mut self.d, redo);
                return self.after_change(false);
            }
            return Task::none();
        }
        if self.export.is_some() {
            if esc {
                self.export = None;
            } else if enter {
                return self.export_choose();
            } else if cmd && ch == "1" {
                return self.update(Message::ExportNumbers);
            } else if cmd && ch == "2" {
                return self.update(Message::ExportTitle);
            } else if cmd && ch == "3" {
                return self.update(Message::ExportChanges);
            }
        }
        Task::none()
    }

    /// Keys the page did not take: shortcuts, palette navigation, Esc.
    fn global_key(&mut self, kp: KeyPress) -> Task<Message> {
        if self.modal() {
            return self.modal_key(&kp);
        }
        let last = self.palette_items().len().saturating_sub(1);
        if let Some(p) = &mut self.palette {
            match kp.key.as_ref() {
                // The selection stops at either end of the list.
                Key::Named(Named::ArrowDown) => {
                    p.selected = (p.selected + 1).min(last);
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
        if self.focus == Focus::Changes
            && self.palette.is_none()
            && let Some(task) = self.changes_key(&kp)
        {
            return task;
        }
        if let Key::Named(Named::Escape) = kp.key.as_ref() {
            if self.find.is_none() {
                self.dismiss_messages();
            }
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
            let c = crate::editor::Caret::at(r.id, 0);
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
        self.editor.anchor = crate::editor::Caret::at(id, from);
        self.editor.caret = crate::editor::Caret::at(id, to);
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

    /// Runs an action by name (editor.md §10), as a shortcut would.
    pub fn action(&mut self, name: &str) -> Task<Message> {
        self.dispatch(Request::Action(name.to_string()), Origin::Key)
            .task
    }

    /// The actions themselves, for [`App::dispatch`].
    fn action_by(&mut self, name: &str, _origin: &Origin) -> Task<Message> {
        let mut scroll = true;
        match name {
            "app.palette" => {
                self.palette = Some(PaletteState::default());
                self.focus = Focus::Palette;
                return iced::widget::operation::focus(PALETTE);
            }
            "app.new" => return self.leave(After::New),
            "app.open" => return self.leave(After::Open),
            "app.close" => return self.leave(After::Close),
            "app.quit" => return self.leave(After::Quit),
            "doc.save" => return self.save(false),
            "app.save_as" => {
                if self.d.session().is_some() {
                    return self.ask(Ask::SaveAs(self.suggested()));
                }
            }
            "app.export_pdf" => return self.export_start(),
            "app.export_fdx" | "app.export_fountain" => {
                if self.d.session().is_some() {
                    let stem = self.stem();
                    return self.ask(if name == "app.export_fdx" {
                        Ask::ExportFdx(format!("{stem}.fdx"))
                    } else {
                        Ask::ExportFountain(format!("{stem}.fountain"))
                    });
                }
            }
            "app.repair" => return self.ask(Ask::RepairIn),
            "app.recovered" => {
                self.palette = None;
                self.recovered_open();
                scroll = false;
            }
            n if n.starts_with("leftover:") => {
                let i: usize = n["leftover:".len()..].parse().unwrap_or(usize::MAX);
                if let Some(p) = self.leftover.get(i).cloned() {
                    return self.leave(After::OpenPath(p));
                }
            }
            n if n.starts_with("recent:") => {
                let i: usize = n["recent:".len()..].parse().unwrap_or(usize::MAX);
                if let Some(p) = self.settings.recent.get(i).cloned() {
                    return self.leave(After::OpenPath(p));
                }
            }
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
            "app.dismiss_messages" => {
                self.dismiss_messages();
                scroll = false;
            }
            "app.history" => {
                self.sheet = Some(Sheet::History);
                scroll = false;
            }
            "app.about" => {
                self.sheet = Some(Sheet::About);
                scroll = false;
            }
            "app.agent_access" => {
                self.settings.agent_access = !self.settings.agent_access;
                self.save_settings();
                self.sync_link();
                self.banners.push(if self.settings.agent_access {
                    "Agent access is on: an agent you start with `coldopen mcp` can attach to this script.".into()
                } else {
                    "Agent access is off: no agent can attach to scripts open here.".into()
                });
                scroll = false;
            }
            "app.save_as_project" => {
                if self.d.session().is_some() {
                    return self.ask(Ask::SaveAsProject(format!("{}.coldopen", self.stem())));
                }
            }
            "app.author_name" => return self.ask_name(NameThen::Nothing),
            "changes.accept_here" => self.settle_here(true),
            "changes.reject_here" => self.settle_here(false),
            "changes.accept_scene" => self.settle_scene(true),
            "changes.reject_scene" => self.settle_scene(false),
            "changes.accept_all" | "changes.reject_all" => {
                self.ask_settle_all(name == "changes.accept_all");
                scroll = false;
            }
            "changes.next" => self.change_step(false),
            "changes.previous" => self.change_step(true),
            "changes.track_mine" => {
                let ask = self.track_mine();
                let refresh = self.after_change(false);
                return Task::batch([ask, refresh]);
            }
            "changes.track_agents" => {
                self.track_agents();
                scroll = false;
            }
            "view.changes" => {
                self.settings.changes_panel = !self.settings.changes_panel;
                self.save_settings();
                if !self.settings.changes_panel && self.focus == Focus::Changes {
                    self.focus = Focus::Page;
                }
                self.refresh_changes();
                scroll = false;
            }
            "app.renderer_cpu" | "app.renderer_gpu" => {
                self.settings.renderer = if name == "app.renderer_gpu" {
                    crate::files::Renderer::Gpu
                } else {
                    crate::files::Renderer::Cpu
                };
                self.save_settings();
                self.banners
                    .push("The new renderer is used from the next start of Cold Open.".into());
                scroll = false;
            }
            "app.agent_disconnect" => {
                if self.agents.is_empty() {
                    self.banners.push("No agent is connected.".into());
                }
                self.close_links();
                scroll = false;
            }
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
            "view.zoom_in" | "view.zoom_out" => {
                let mut s = self.settings.clone();
                s.zoom_step(if name == "view.zoom_in" { 1 } else { -1 });
                return self.zoom_to(s.zoom);
            }
            "view.zoom_reset" => return self.zoom_to(1.0),
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
                    Focus::Page | Focus::Navigator if self.settings.changes_panel => Focus::Changes,
                    Focus::Page | Focus::Navigator | Focus::Changes if self.find.is_some() => {
                        Focus::Find
                    }
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
                .push(format!("\"{other}\" is not available yet.").into()),
        }
        self.after_change(scroll)
    }

    /// What the palette lists for its query: actions and commands, recent
    /// files and leftover work (editor.md §9, §11), best match first.
    fn palette_items(&self) -> Vec<PaletteItem> {
        let q = self.palette.as_ref().map_or("", |p| p.query.as_str());
        let file = |action: String, verb: &str, p: &Path| {
            let name = p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            PaletteItem {
                action,
                title: format!("{verb}: {name}"),
                detail: p
                    .parent()
                    .map_or_else(String::new, |d| d.display().to_string()),
            }
        };
        let mut all: Vec<PaletteItem> = Vec::new();
        for (i, p) in self.leftover.iter().enumerate() {
            all.push(file(format!("leftover:{i}"), "Reopen unsaved work", p));
        }
        for (n, t) in actions::all() {
            all.push(PaletteItem {
                action: n.to_string(),
                title: t.to_string(),
                detail: n.to_string(),
            });
        }
        for (i, p) in self.settings.recent.iter().enumerate() {
            all.push(file(format!("recent:{i}"), "Open recent", p));
        }
        let mut scored: Vec<(i32, usize, PaletteItem)> = all
            .into_iter()
            .enumerate()
            .filter_map(|(k, it)| actions::score(q, &it.detail, &it.title).map(|s| (s, k, it)))
            .collect();
        scored.sort_by_key(|(s, k, _)| (std::cmp::Reverse(*s), *k));
        scored.into_iter().take(12).map(|(_, _, it)| it).collect()
    }

    fn palette_run(&mut self, i: usize) -> Task<Message> {
        let items = self.palette_items();
        let Some(item) = items.get(i.min(items.len().saturating_sub(1))) else {
            return Task::none();
        };
        let action = item.action.clone();
        self.palette = None;
        self.focus = Focus::Page;
        self.dispatch(Request::Action(action), Origin::Palette).task
    }

    /// Runs the palette's params form through the router (editor.md §9,
    /// §16): session commands run the app's flows, the rest answer in the
    /// form.
    fn params_run(&mut self) -> Task<Message> {
        let Some((name, json)) = self.palette.as_ref().and_then(|p| p.params.clone()) else {
            return Task::none();
        };
        let response = match serde_json::from_str::<Value>(&json) {
            Ok(params) => {
                let request = json!({ "command": name, "params": params });
                match Dispatcher::read_request(&request).1 {
                    Ok(cmd) => {
                        let out = self.dispatch(Request::Command(cmd), Origin::Form);
                        match out.response {
                            None => {
                                self.palette = None;
                                self.focus = Focus::Page;
                                return out.task;
                            }
                            Some(r) => {
                                let r = Dispatcher::response(Value::Null, r);
                                serde_json::to_string_pretty(&r).unwrap_or_default()
                            }
                        }
                    }
                    Err(e) => {
                        let r = Dispatcher::response(Value::Null, Err(e));
                        serde_json::to_string_pretty(&r).unwrap_or_default()
                    }
                }
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
        let keys = iced::event::listen_with(unclaimed_key);
        let link = match &self.link {
            Some(l) => Subscription::run_with(l.key.clone(), crate::link::stream),
            None => Subscription::none(),
        };
        let frames = if self.first_frame.is_some() || self.bench.is_some() {
            iced::window::frames().map(|_| Message::Frame)
        } else {
            Subscription::none()
        };
        Subscription::batch([
            frames,
            link,
            keys,
            iced::window::close_requests().map(|_| Message::CloseRequested),
            iced::time::every(Duration::from_millis(530)).map(|_| Message::Tick),
            iced::system::theme_changes()
                .map(|m| Message::SystemTheme(m == iced::theme::Mode::Dark)),
        ])
    }
}

/// A key press no widget captured becomes a [`Message::GlobalKey`], and Esc
/// that a text field captured becomes [`Message::Escaped`]: the app's
/// keyboard subscription, and the headless runner, use it.
pub fn unclaimed_key(
    event: iced::Event,
    status: iced::event::Status,
    _window: iced::window::Id,
) -> Option<Message> {
    match (event, status) {
        // A text field takes Esc to let go of the keyboard; the overlay it
        // belongs to closes on the same press.
        (
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                ..
            }),
            iced::event::Status::Captured,
        ) => Some(Message::Escaped),
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
