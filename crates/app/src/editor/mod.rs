//! The editor model: caret, selection, smart element flow and
//! autocomplete, turning keys into dispatcher commands
//! (docs/specs/editor.md §4–§6). It has no iced types, so tests drive it
//! directly.

pub mod flow;
pub mod geometry;

use coldopen_core::command::{
    BatchItem, BlockDelete, BlockInsert, BlockJoin, BlockSetType, BlockSplit, Empty, StyleSpec,
    TextComplete, TextDelete, TextInsert, TextStyle,
};
use coldopen_core::{BlockId, Command, CommandError, Dispatcher, Document};
use coldopen_screenplay::{ElementKind, Pt, Style};
use serde_json::Value;
use unicode_width::UnicodeWidthChar;

use flow::FlowTable;
pub use geometry::{Caret, DraftIndex, Spot, Surface};

/// Caret motions (editor.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    /// One screen up or down; the editor is told the screen's height.
    PageUp,
    PageDown,
    PrevScene,
    NextScene,
    PrevBlock,
    NextBlock,
}

/// Autocomplete suggestions on screen (editor.md §6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub block: BlockId,
    pub context: String,
    pub from: usize,
    pub to: usize,
    pub items: Vec<String>,
    pub selected: usize,
}

/// Text an input method is still composing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    pub text: String,
}

/// The kinds that print in capitals, so typing in them inserts capitals
/// (editor.md §5.1).
pub fn caps(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::SceneHeading
            | ElementKind::Character
            | ElementKind::Transition
            | ElementKind::Shot
    )
}

/// Kinds where Shift+Enter inserts a line break.
fn multiline(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Action | ElementKind::Dialogue | ElementKind::Lyric | ElementKind::General
    )
}

const SCENE_PREFIXES: [&str; 8] = [
    "INT", "EXT", "EST", "INT./EXT", "INT/EXT", "I/E", "EXT/INT", "E/I",
];

/// A failed command, for the banner.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub code: &'static str,
    pub message: String,
}

impl From<CommandError> for Problem {
    fn from(e: CommandError) -> Problem {
        Problem {
            code: e.code,
            message: e.message,
        }
    }
}

/// The editor's state for one document.
#[derive(Debug)]
pub struct Editor {
    /// Where typing goes.
    pub caret: Caret,
    /// The other end of the selection; equal to `caret` when nothing is
    /// selected.
    pub anchor: Caret,
    /// The column Up and Down keep, from the paper's left edge.
    pub goal_x: Option<Pt>,
    /// The style for the next text typed, set by Mod+B, I or U with no
    /// selection.
    pub pending_style: Option<Style>,
    pub preedit: Option<Preedit>,
    pub completion: Option<Completion>,
    pub flow: FlowTable,
    /// The continuous draft view instead of pages.
    pub draft: bool,
    pub draft_index: DraftIndex,
    draft_rev: Option<u64>,
    /// A block that was empty when typing in it began, for auto-convert.
    fresh: Option<BlockId>,
    /// Autocomplete closed by Esc for this block and text.
    dismissed: Option<(BlockId, String)>,
    /// Parentheticals whose brackets the editor added, not the writer, with
    /// the text it wrapped in them. Converting the block to another kind
    /// takes them off again while that text is unchanged, and empty ones go
    /// when the writer leaves them (editor.md §5.2).
    auto_parens: std::collections::BTreeMap<BlockId, String>,
    /// The last command failures, for the banner.
    pub problems: Vec<Problem>,
}

fn kind_of(doc: &Document, id: BlockId) -> Option<ElementKind> {
    doc.get(id).map(|b| b.element.kind)
}

fn text_of(doc: &Document, id: BlockId) -> String {
    doc.get(id)
        .map(|b| b.element.text.plain_text())
        .unwrap_or_default()
}

fn len_of(doc: &Document, id: BlockId) -> usize {
    text_of(doc, id).chars().count()
}

/// Offsets where clusters start: a character of zero width belongs to the
/// one before it (docs/specs/layout.md §2).
fn cluster_starts(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, c) in s.chars().enumerate() {
        if i == 0 || UnicodeWidthChar::width(c).unwrap_or(0) > 0 || c == '\n' {
            out.push(i);
        }
    }
    out
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '\u{2019}'
}

fn word_left(s: &str, at: usize) -> usize {
    let cs: Vec<char> = s.chars().collect();
    let mut i = at.min(cs.len());
    while i > 0 && !is_word(cs[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word(cs[i - 1]) {
        i -= 1;
    }
    i
}

fn word_right(s: &str, at: usize) -> usize {
    let cs: Vec<char> = s.chars().collect();
    let mut i = at.min(cs.len());
    while i < cs.len() && !is_word(cs[i]) {
        i += 1;
    }
    while i < cs.len() && is_word(cs[i]) {
        i += 1;
    }
    i
}

impl Editor {
    pub fn new(doc: &Document) -> Editor {
        let first = doc
            .blocks
            .iter()
            .find(|b| b.element.kind.is_printing())
            .or(doc.blocks.first())
            .map(|b| b.id)
            .unwrap_or(BlockId(0));
        let caret = Caret {
            block: first,
            offset: 0,
        };
        Editor {
            caret,
            anchor: caret,
            goal_x: None,
            pending_style: None,
            preedit: None,
            completion: None,
            flow: FlowTable::default(),
            draft: false,
            draft_index: DraftIndex::default(),
            draft_rev: None,
            fresh: None,
            dismissed: None,
            auto_parens: std::collections::BTreeMap::new(),
            problems: Vec::new(),
        }
    }

    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    /// The selection, start first.
    pub fn selection(&self, doc: &Document) -> (Caret, Caret) {
        let pos = |c: &Caret| (doc.index_of(c.block).unwrap_or(usize::MAX), c.offset);
        if pos(&self.anchor) <= pos(&self.caret) {
            (self.anchor, self.caret)
        } else {
            (self.caret, self.anchor)
        }
    }

    fn set_caret(&mut self, c: Caret, extend: bool) {
        self.caret = c;
        if !extend {
            self.anchor = c;
        }
        if self.fresh != Some(c.block) {
            self.fresh = None;
        }
    }

    /// Runs commands as one change when there are several.
    fn run(&mut self, d: &mut Dispatcher, cmds: Vec<Command>) -> Option<Value> {
        let cmd = match cmds.len() {
            0 => return None,
            1 => cmds.into_iter().next()?,
            _ => Command::Batch(coldopen_core::command::Batch {
                commands: cmds.iter().map(BatchItem::of).collect(),
                label: None,
            }),
        };
        match d.execute(cmd) {
            Ok(v) => Some(v),
            Err(e) => {
                self.problems.push(e.into());
                None
            }
        }
    }

    /// Brings the caret back inside the document after any change.
    pub fn fix_caret(&mut self, doc: &Document) {
        for c in [&mut self.caret, &mut self.anchor] {
            match doc.get(c.block) {
                Some(b) => c.offset = c.offset.min(b.element.text.char_len()),
                None => {
                    let b = doc
                        .blocks
                        .iter()
                        .rev()
                        .find(|b| b.id.0 < c.block.0)
                        .or(doc.blocks.first());
                    *c = Caret {
                        block: b.map_or(BlockId(0), |b| b.id),
                        offset: b.map_or(0, |b| b.element.text.char_len()),
                    };
                }
            }
        }
    }

    /// Keeps the caret on the same text after a command applied by someone
    /// else, such as an agent (editor.md §12).
    pub fn follow(&mut self, cmd: &Command, result: &Value) {
        for c in [&mut self.caret, &mut self.anchor] {
            match cmd {
                Command::TextInsert(p) if p.id == c.block && p.at < c.offset => {
                    c.offset += p.text.chars().count();
                }
                Command::TextDelete(p) if p.id == c.block && p.from < c.offset => {
                    c.offset -= c.offset.min(p.to) - p.from;
                }
                Command::BlockSplit(p) if p.id == c.block && p.at <= c.offset => {
                    if let Some(id) = result["id"].as_str().and_then(BlockId::parse) {
                        *c = Caret {
                            block: id,
                            offset: c.offset - p.at,
                        };
                    }
                }
                Command::BlockJoin(p) if p.id == c.block => {
                    if let (Some(id), Some(at)) = (
                        result["id"].as_str().and_then(BlockId::parse),
                        result["at"].as_u64(),
                    ) {
                        *c = Caret {
                            block: id,
                            offset: c.offset + at as usize,
                        };
                    }
                }
                _ => {}
            }
        }
    }

    /// The draft view's index, rebuilt when the document has changed.
    fn refresh_draft(&mut self, d: &mut Dispatcher) {
        if !self.draft {
            return;
        }
        let Some(view) = d.view() else { return };
        if self.draft_rev == Some(view.rev) && self.draft_index.draft.is_some() {
            return;
        }
        self.draft_rev = Some(view.rev);
        self.draft_index = DraftIndex::new(view.draft(), view.norm.elements.len());
    }

    /// Runs `f` with the document and the surface the caret moves on.
    pub fn with_surface<R>(
        &mut self,
        d: &mut Dispatcher,
        f: impl FnOnce(&Document, &Surface<'_>) -> R,
    ) -> Option<R> {
        d.view()?;
        self.refresh_draft(d);
        let view = d.cached_view()?;
        let doc = &d.session()?.doc;
        let surface = if self.draft {
            Surface::Draft(view, &self.draft_index)
        } else {
            Surface::Pages(view)
        };
        Some(f(doc, &surface))
    }

    /// Where the caret shows.
    pub fn caret_spot(&mut self, d: &mut Dispatcher) -> Option<Spot> {
        let caret = self.caret;
        self.with_surface(d, |doc, s| s.spot(doc, caret)).flatten()
    }

    fn editable(&self, doc: &Document, i: usize) -> bool {
        geometry::editable(doc.blocks[i].element.kind, self.draft)
    }

    /// Moves the caret. `screen` is the visible height, for Page Up/Down.
    pub fn motion(&mut self, d: &mut Dispatcher, m: Motion, extend: bool, screen: Pt) {
        let from = self.caret.block;
        self.move_caret(d, m, extend, screen);
        self.left(d, from);
    }

    /// Whether a parenthetical's text is still exactly the text the editor
    /// wrapped in brackets, brackets included.
    fn own_parens(&self, d: &Dispatcher, id: BlockId) -> bool {
        self.auto_parens.get(&id).is_some_and(|inner| {
            d.session().and_then(|s| s.doc.get(id)).is_some_and(|b| {
                b.element.kind == ElementKind::Parenthetical
                    && b.element.text.plain_text() == format!("({inner})")
            })
        })
    }

    /// Whether a block is a parenthetical holding only the `()` the
    /// editor added.
    fn bare_parens(&self, d: &Dispatcher, id: BlockId) -> bool {
        self.auto_parens.get(&id).is_some_and(String::is_empty) && self.own_parens(d, id)
    }

    /// After the caret left block `from`: brackets the editor added there,
    /// with nothing typed inside, go (editor.md §5.2).
    fn left(&mut self, d: &mut Dispatcher, from: BlockId) {
        if self.caret.block != from && self.bare_parens(d, from) {
            let cmd = Command::TextDelete(TextDelete {
                id: from,
                from: 0,
                to: 2,
            });
            self.run(d, vec![cmd]);
            if let Some(s) = d.session() {
                self.fix_caret(&s.doc);
            }
        }
    }

    fn move_caret(&mut self, d: &mut Dispatcher, m: Motion, extend: bool, screen: Pt) {
        let Some(doc) = d.session().map(|s| s.doc.clone()) else {
            return;
        };
        if !extend && self.has_selection() && matches!(m, Motion::Left | Motion::Right) {
            let (a, b) = self.selection(&doc);
            let c = if m == Motion::Left { a } else { b };
            self.set_caret(c, false);
            self.goal_x = None;
            return;
        }
        let Some(i) = doc.index_of(self.caret.block) else {
            return;
        };
        let text = text_of(&doc, self.caret.block);
        let len = text.chars().count();
        let off = self.caret.offset.min(len);
        let prev_editable = |from: usize| (0..from).rev().find(|&j| self.editable(&doc, j));
        let next_editable =
            |from: usize| (from + 1..doc.blocks.len()).find(|&j| self.editable(&doc, j));
        let at_end = |j: usize| Caret {
            block: doc.blocks[j].id,
            offset: doc.blocks[j].element.text.char_len(),
        };
        let at_start = |j: usize| Caret {
            block: doc.blocks[j].id,
            offset: 0,
        };
        let vertical = matches!(
            m,
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
        );
        let target: Option<Caret> = match m {
            Motion::Left => {
                if off > 0 {
                    let starts = cluster_starts(&text);
                    let p = starts
                        .iter()
                        .rev()
                        .find(|&&s| s < off)
                        .copied()
                        .unwrap_or(0);
                    Some(Caret {
                        offset: p,
                        ..self.caret
                    })
                } else {
                    prev_editable(i).map(at_end)
                }
            }
            Motion::Right => {
                if off < len {
                    let starts = cluster_starts(&text);
                    let p = starts.iter().find(|&&s| s > off).copied().unwrap_or(len);
                    Some(Caret {
                        offset: p,
                        ..self.caret
                    })
                } else {
                    next_editable(i).map(at_start)
                }
            }
            Motion::WordLeft => {
                if off > 0 {
                    Some(Caret {
                        offset: word_left(&text, off),
                        ..self.caret
                    })
                } else {
                    prev_editable(i).map(at_end)
                }
            }
            Motion::WordRight => {
                if off < len {
                    Some(Caret {
                        offset: word_right(&text, off),
                        ..self.caret
                    })
                } else {
                    next_editable(i).map(at_start)
                }
            }
            Motion::DocStart => (0..doc.blocks.len())
                .find(|&j| self.editable(&doc, j))
                .map(at_start),
            Motion::DocEnd => (0..doc.blocks.len())
                .rev()
                .find(|&j| self.editable(&doc, j))
                .map(at_end),
            Motion::PrevBlock => {
                if off > 0 {
                    Some(at_start(i))
                } else {
                    prev_editable(i).map(at_start)
                }
            }
            Motion::NextBlock => next_editable(i).map(at_start),
            Motion::PrevScene => (0..i)
                .rev()
                .find(|&j| doc.blocks[j].element.kind == ElementKind::SceneHeading)
                .map(at_start),
            Motion::NextScene => (i + 1..doc.blocks.len())
                .find(|&j| doc.blocks[j].element.kind == ElementKind::SceneHeading)
                .map(at_start),
            Motion::LineStart | Motion::LineEnd => {
                let caret = self.caret;
                let end = m == Motion::LineEnd;
                self.with_surface(d, |doc, s| {
                    s.spot(doc, caret)
                        .and_then(|spot| s.line_edge(doc, spot, end))
                })
                .flatten()
            }
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown => {
                let caret = self.caret;
                let goal = self.goal_x;
                let down = matches!(m, Motion::Down | Motion::PageDown);
                let steps = match m {
                    Motion::PageUp | Motion::PageDown => self
                        .with_surface(d, |_, s| (screen.0 / s.line_height().0.max(1)).max(1))
                        .unwrap_or(1),
                    _ => 1,
                };
                let r = self.with_surface(d, |doc, s| {
                    let spot = s.spot(doc, caret)?;
                    let goal = goal.unwrap_or(spot.x);
                    let mut c = caret;
                    let mut sp = spot;
                    for _ in 0..steps {
                        match s.vertical(doc, sp, down, goal) {
                            Some(n) => {
                                c = n;
                                match s.spot(doc, c) {
                                    Some(x) => sp = x,
                                    None => break,
                                }
                            }
                            None => break,
                        }
                    }
                    Some((c, goal))
                });
                match r.flatten() {
                    Some((c, goal)) => {
                        self.goal_x = Some(goal);
                        Some(c)
                    }
                    None => None,
                }
            }
        };
        if let Some(c) = target {
            self.set_caret(c, extend);
        }
        if !vertical {
            self.goal_x = None;
        }
        self.completion = None;
    }

    /// Places the caret at a point on a page (a click).
    pub fn click(&mut self, d: &mut Dispatcher, page: usize, x: Pt, y: Pt, extend: bool) {
        let from = self.caret.block;
        if let Some(Some(c)) = self.with_surface(d, |doc, s| s.hit(doc, page, x, y)) {
            self.set_caret(c, extend);
            self.goal_x = None;
            self.completion = None;
        }
        self.left(d, from);
    }

    pub fn select_all(&mut self, d: &mut Dispatcher) {
        let Some(doc) = d.session().map(|s| &s.doc) else {
            return;
        };
        if let (Some(f), Some(l)) = (doc.blocks.first(), doc.blocks.last()) {
            self.anchor = Caret {
                block: f.id,
                offset: 0,
            };
            self.caret = Caret {
                block: l.id,
                offset: l.element.text.char_len(),
            };
        }
    }

    /// Commands that delete the selection, leaving the caret at its start.
    fn delete_selection(&self, doc: &Document) -> Vec<Command> {
        let (a, b) = self.selection(doc);
        if a == b {
            return Vec::new();
        }
        if a.block == b.block {
            return vec![Command::TextDelete(TextDelete {
                id: a.block,
                from: a.offset,
                to: b.offset,
            })];
        }
        let (Some(ia), Some(ib)) = (doc.index_of(a.block), doc.index_of(b.block)) else {
            return Vec::new();
        };
        let mut cmds = vec![Command::TextDelete(TextDelete {
            id: a.block,
            from: a.offset,
            to: len_of(doc, a.block),
        })];
        for blk in &doc.blocks[ia + 1..ib] {
            cmds.push(Command::BlockDelete(BlockDelete { id: blk.id }));
        }
        cmds.push(Command::TextDelete(TextDelete {
            id: b.block,
            from: 0,
            to: b.offset,
        }));
        let both_print =
            doc.blocks[ia].element.kind.is_printing() && doc.blocks[ib].element.kind.is_printing();
        if both_print {
            cmds.push(Command::BlockJoin(BlockJoin { id: b.block }));
        }
        cmds
    }

    /// Types text at the caret (editor.md §5.1).
    pub fn insert_text(&mut self, d: &mut Dispatcher, text: &str) {
        if text.is_empty() {
            return;
        }
        let Some(doc) = d.session().map(|s| s.doc.clone()) else {
            return;
        };
        let Some(kind) = kind_of(&doc, self.caret.block) else {
            return;
        };
        if kind == ElementKind::PageBreak {
            return;
        }
        let block_text = text_of(&doc, self.caret.block);
        // Typing `)` over the closing bracket of a parenthetical moves past it.
        if text == ")"
            && !self.has_selection()
            && kind == ElementKind::Parenthetical
            && block_text.chars().nth(self.caret.offset) == Some(')')
        {
            self.caret.offset += 1;
            self.anchor = self.caret;
            return;
        }
        if block_text.is_empty() && !self.has_selection() {
            self.fresh = Some(self.caret.block);
        }
        let text = if caps(kind) {
            text.to_uppercase()
        } else {
            text.to_string()
        };
        let mut cmds = self.delete_selection(&doc);
        let (start, _) = self.selection(&doc);
        // An empty dialogue becomes a parenthetical when `(` is typed in it.
        if text == "(" && kind == ElementKind::Dialogue && block_text.is_empty() && cmds.is_empty()
        {
            cmds.push(Command::BlockSetType(BlockSetType {
                id: start.block,
                kind: ElementKind::Parenthetical,
            }));
            cmds.push(Command::TextInsert(TextInsert {
                id: start.block,
                at: 0,
                text: "()".into(),
                style: None,
            }));
            if self.run(d, cmds).is_some() {
                // The writer typed these brackets: they are never removed.
                self.auto_parens.remove(&start.block);
                self.set_caret(
                    Caret {
                        block: start.block,
                        offset: 1,
                    },
                    false,
                );
            }
            return;
        }
        cmds.push(Command::TextInsert(TextInsert {
            id: start.block,
            at: start.offset,
            text: text.clone(),
            style: self.pending_style.map(StyleSpec::from),
        }));
        if self.run(d, cmds).is_some() {
            self.set_caret(
                Caret {
                    block: start.block,
                    offset: start.offset + text.chars().count(),
                },
                false,
            );
            self.goal_x = None;
            self.auto_scene_heading(d);
        }
        self.refresh_completion(d);
    }

    /// An action block that began empty and now holds a scene prefix and
    /// `.` or a space becomes a scene heading (editor.md §5.2).
    fn auto_scene_heading(&mut self, d: &mut Dispatcher) {
        let Some(doc) = d.session().map(|s| &s.doc) else {
            return;
        };
        if self.fresh != Some(self.caret.block)
            || kind_of(doc, self.caret.block) != Some(ElementKind::Action)
        {
            return;
        }
        let t = text_of(doc, self.caret.block).to_uppercase();
        let matches = SCENE_PREFIXES.iter().any(|p| {
            t.strip_prefix(p)
                .is_some_and(|rest| rest == "." || rest == " ")
        });
        if matches {
            let id = self.caret.block;
            self.run(
                d,
                vec![Command::BlockSetType(BlockSetType {
                    id,
                    kind: ElementKind::SceneHeading,
                })],
            );
        }
    }

    /// Backspace or Delete (editor.md §5.1).
    pub fn delete(&mut self, d: &mut Dispatcher, forward: bool, word: bool) {
        let Some(doc) = d.session().map(|s| s.doc.clone()) else {
            return;
        };
        if self.has_selection() {
            let (start, _) = self.selection(&doc);
            let cmds = self.delete_selection(&doc);
            if self.run(d, cmds).is_some() {
                self.set_caret(start, false);
            }
            self.refresh_completion(d);
            return;
        }
        let id = self.caret.block;
        let Some(i) = doc.index_of(id) else { return };
        let text = text_of(&doc, id);
        let len = text.chars().count();
        let off = self.caret.offset.min(len);
        if !forward && off > 0 {
            let from = if word {
                word_left(&text, off)
            } else {
                cluster_starts(&text)
                    .iter()
                    .rev()
                    .find(|&&s| s < off)
                    .copied()
                    .unwrap_or(0)
            };
            let cmd = Command::TextDelete(TextDelete { id, from, to: off });
            if self.run(d, vec![cmd]).is_some() {
                self.set_caret(
                    Caret {
                        block: id,
                        offset: from,
                    },
                    false,
                );
            }
        } else if forward && off < len {
            let to = if word {
                word_right(&text, off)
            } else {
                cluster_starts(&text)
                    .iter()
                    .find(|&&s| s > off)
                    .copied()
                    .unwrap_or(len)
            };
            let cmd = Command::TextDelete(TextDelete { id, from: off, to });
            self.run(d, vec![cmd]);
        } else if !forward {
            self.join_back(d, &doc, i);
        } else if let Some(next) = doc.blocks.get(i + 1) {
            // Delete at the end: the same as Backspace at the start of the
            // next block.
            let next_id = next.id;
            let here = self.caret;
            self.set_caret(
                Caret {
                    block: next_id,
                    offset: 0,
                },
                false,
            );
            self.join_back(d, &doc, i + 1);
            if self.caret.block == next_id {
                self.set_caret(here, false);
            }
        }
        self.refresh_completion(d);
    }

    /// Backspace at the start of block `i`.
    fn join_back(&mut self, d: &mut Dispatcher, doc: &Document, i: usize) {
        let Some(b) = doc.blocks.get(i) else { return };
        let Some(prev) = i.checked_sub(1).and_then(|j| doc.blocks.get(j)) else {
            return;
        };
        let prev_end = Caret {
            block: prev.id,
            offset: prev.element.text.char_len(),
        };
        if prev.element.kind == ElementKind::PageBreak {
            self.run(d, vec![Command::BlockDelete(BlockDelete { id: prev.id })]);
            return;
        }
        if b.element.text.is_empty() {
            if self
                .run(d, vec![Command::BlockDelete(BlockDelete { id: b.id })])
                .is_some()
            {
                let target = (0..i)
                    .rev()
                    .find(|&j| self.editable(doc, j))
                    .map(|j| Caret {
                        block: doc.blocks[j].id,
                        offset: doc.blocks[j].element.text.char_len(),
                    })
                    .unwrap_or(prev_end);
                self.set_caret(target, false);
            }
            return;
        }
        if b.element.kind.is_printing() && prev.element.kind.is_printing() {
            if let Some(r) = self.run(d, vec![Command::BlockJoin(BlockJoin { id: b.id })])
                && let (Some(id), Some(at)) =
                    (r["id"].as_str().and_then(BlockId::parse), r["at"].as_u64())
            {
                self.set_caret(
                    Caret {
                        block: id,
                        offset: at as usize,
                    },
                    false,
                );
            }
            return;
        }
        self.set_caret(prev_end, false);
    }

    /// Enter (editor.md §5.2).
    pub fn enter(&mut self, d: &mut Dispatcher) {
        if self.completion.is_some() {
            self.accept_completion(d);
            return;
        }
        let Some(doc) = d.session().map(|s| s.doc.clone()) else {
            return;
        };
        if self.has_selection() {
            let (start, _) = self.selection(&doc);
            let cmds = self.delete_selection(&doc);
            if self.run(d, cmds).is_some() {
                self.set_caret(start, false);
            }
        }
        let Some(doc) = d.session().map(|s| s.doc.clone()) else {
            return;
        };
        let id = self.caret.block;
        let Some(mut kind) = kind_of(&doc, id) else {
            return;
        };
        if self.bare_parens(d, id) {
            let to = self.flow.row(kind).empty_enter;
            if to != kind {
                self.set_kind(d, id, to);
            }
            return;
        }
        let text = text_of(&doc, id);
        let len = text.chars().count();
        let mut off = self.caret.offset.min(len);
        if kind == ElementKind::Parenthetical && text.ends_with(')') && off + 1 == len {
            off = len;
        }
        if len == 0 {
            let to = self.flow.row(kind).empty_enter;
            if to != kind {
                self.set_kind(d, id, to);
            }
            return;
        }
        if off < len {
            let cmd = Command::BlockSplit(BlockSplit {
                id,
                at: off,
                kind: None,
            });
            if let Some(r) = self.run(d, vec![cmd])
                && let Some(new) = r["id"].as_str().and_then(BlockId::parse)
            {
                self.set_caret(
                    Caret {
                        block: new,
                        offset: 0,
                    },
                    false,
                );
            }
            return;
        }
        // At the end: an all-capitals action ending in TO: is a transition.
        if kind == ElementKind::Action
            && text.trim_end().ends_with("TO:")
            && !text.chars().any(char::is_lowercase)
        {
            self.run(
                d,
                vec![Command::BlockSetType(BlockSetType {
                    id,
                    kind: ElementKind::Transition,
                })],
            );
            kind = ElementKind::Transition;
        }
        let next = self.flow.row(kind).enter;
        let mut cmds = vec![Command::BlockSplit(BlockSplit {
            id,
            at: len,
            kind: Some(next),
        })];
        let paren = next == ElementKind::Parenthetical;
        if let Some(r) = self.run(d, std::mem::take(&mut cmds)) {
            let new = if r.get("results").is_some() {
                r["results"][0]["id"].as_str().and_then(BlockId::parse)
            } else {
                r["id"].as_str().and_then(BlockId::parse)
            };
            if let Some(new) = new {
                self.set_caret(
                    Caret {
                        block: new,
                        offset: 0,
                    },
                    false,
                );
                self.fresh = Some(new);
                if paren {
                    self.insert_parens(d, new);
                }
            }
        }
        self.completion = None;
        self.refresh_completion(d);
    }

    fn insert_parens(&mut self, d: &mut Dispatcher, id: BlockId) {
        let cmd = Command::TextInsert(TextInsert {
            id,
            at: 0,
            text: "()".into(),
            style: None,
        });
        if self.run(d, vec![cmd]).is_some() {
            self.auto_parens.insert(id, String::new());
            self.set_caret(
                Caret {
                    block: id,
                    offset: 1,
                },
                false,
            );
        }
    }

    /// Changes a block's kind, as one undo step (editor.md §5.2). A block
    /// that becomes a parenthetical has its text wrapped in brackets unless
    /// it already is; a parenthetical that becomes anything else loses the
    /// brackets the editor added, while the text between them is unchanged.
    /// So cycling with Tab leaves nothing behind, and the caret stays on
    /// the same character.
    fn set_kind(&mut self, d: &mut Dispatcher, id: BlockId, kind: ElementKind) {
        let paren = ElementKind::Parenthetical;
        let Some((from, text)) = d
            .session()
            .and_then(|s| s.doc.get(id))
            .map(|b| (b.element.kind, b.element.text.plain_text()))
        else {
            return;
        };
        let n = text.chars().count();
        let plain = Some(StyleSpec::default());
        let mut cmds = vec![Command::BlockSetType(BlockSetType { id, kind })];
        let mut wrap = false;
        let mut unwrap = false;
        if kind == paren && from != paren {
            let t = text.trim();
            if t.starts_with('(') && t.ends_with(')') && t.len() >= 2 {
                // The writer's own brackets.
                self.auto_parens.remove(&id);
            } else {
                wrap = true;
                cmds.push(Command::TextInsert(TextInsert {
                    id,
                    at: n,
                    text: ")".into(),
                    style: plain,
                }));
                cmds.push(Command::TextInsert(TextInsert {
                    id,
                    at: 0,
                    text: "(".into(),
                    style: plain,
                }));
            }
        } else if from == paren && kind != paren && self.own_parens(d, id) {
            // Changed inside, or the writer's, they stay. The record stays
            // too, for an undo back to the unchanged text.
            unwrap = true;
            cmds.push(Command::TextDelete(TextDelete {
                id,
                from: n - 1,
                to: n,
            }));
            cmds.push(Command::TextDelete(TextDelete { id, from: 0, to: 1 }));
        }
        if self.run(d, cmds).is_some() {
            if wrap {
                self.auto_parens.insert(id, text);
            }
            for c in [&mut self.caret, &mut self.anchor] {
                if c.block == id && wrap {
                    c.offset = c.offset.min(n) + 1;
                } else if c.block == id && unwrap {
                    c.offset = c.offset.saturating_sub(1).min(n - 2);
                }
            }
        }
        self.refresh_completion(d);
    }

    /// Shift+Enter: a line break in kinds that hold several lines.
    pub fn line_break(&mut self, d: &mut Dispatcher) {
        let kind = d.session().and_then(|s| kind_of(&s.doc, self.caret.block));
        match kind {
            Some(k) if multiline(k) => self.insert_text(d, "\n"),
            _ => self.enter(d),
        }
    }

    /// Tab or Shift+Tab (editor.md §5.2).
    pub fn tab(&mut self, d: &mut Dispatcher, back: bool) {
        if !back && self.completion.is_some() {
            self.accept_completion(d);
            return;
        }
        let id = self.caret.block;
        let Some(kind) = d.session().and_then(|s| kind_of(&s.doc, id)) else {
            return;
        };
        let row = self.flow.row(kind);
        if let Some(to) = if back { row.shift_tab } else { row.tab } {
            self.set_kind(d, id, to);
        }
    }

    /// Mod+1…8 and the palette's element commands.
    pub fn set_type(&mut self, d: &mut Dispatcher, kind: ElementKind) {
        let id = self.caret.block;
        self.set_kind(d, id, kind);
    }

    /// Mod+B, I or U: styles the selection, or the next text typed.
    pub fn toggle_style(&mut self, d: &mut Dispatcher, which: char) {
        let Some(doc) = d.session().map(|s| s.doc.clone()) else {
            return;
        };
        if !self.has_selection() {
            let mut s = self.pending_style.unwrap_or_else(|| {
                doc.get(self.caret.block)
                    .map(|b| b.element.text.style_at(self.caret.offset))
                    .unwrap_or_default()
            });
            match which {
                'b' => s.bold = !s.bold,
                'i' => s.italic = !s.italic,
                _ => s.underline = !s.underline,
            }
            self.pending_style = Some(s);
            return;
        }
        let (a, b) = self.selection(&doc);
        let first_style = doc
            .get(a.block)
            .map(|blk| blk.element.text.style_at(a.offset + 1))
            .unwrap_or_default();
        let on = match which {
            'b' => !first_style.bold,
            'i' => !first_style.italic,
            _ => !first_style.underline,
        };
        let (Some(ia), Some(ib)) = (doc.index_of(a.block), doc.index_of(b.block)) else {
            return;
        };
        let mut cmds = Vec::new();
        for (k, blk) in doc.blocks[ia..=ib].iter().enumerate() {
            if blk.element.kind.is_raw_text() || blk.element.kind == ElementKind::PageBreak {
                continue;
            }
            let from = if k == 0 { a.offset } else { 0 };
            let to = if ia + k == ib {
                b.offset
            } else {
                blk.element.text.char_len()
            };
            if from >= to {
                continue;
            }
            cmds.push(Command::TextStyle(TextStyle {
                id: blk.id,
                from,
                to,
                bold: (which == 'b').then_some(on),
                italic: (which == 'i').then_some(on),
                underline: (which == 'u').then_some(on),
            }));
        }
        self.run(d, cmds);
    }

    pub fn undo(&mut self, d: &mut Dispatcher, redo: bool) {
        let cmd = if redo {
            Command::Redo(Empty {})
        } else {
            Command::Undo(Empty {})
        };
        let id = self.caret.block;
        let before = d.session().map(|s| text_of(&s.doc, id));
        self.run(d, vec![cmd]);
        // Undoing or redoing a conversion that wrapped the caret's block in
        // brackets, or took them off, keeps the caret on the same character.
        let after = d.session().map(|s| text_of(&s.doc, id));
        if let (Some(b), Some(a)) = (before, after) {
            let carets = [&mut self.caret, &mut self.anchor];
            for c in carets.into_iter().filter(|c| c.block == id) {
                if a == format!("({b})") {
                    c.offset += 1;
                } else if b == format!("({a})") {
                    c.offset = c.offset.saturating_sub(1);
                }
            }
        }
        if let Some(s) = d.session() {
            self.fix_caret(&s.doc);
        }
        self.completion = None;
    }

    /// The selection as text for the clipboard: plain text within one
    /// block, Fountain across blocks (editor.md §5.4).
    pub fn copy(&self, d: &Dispatcher) -> Option<String> {
        let doc = &d.session()?.doc;
        if !self.has_selection() {
            return None;
        }
        let (a, b) = self.selection(doc);
        if a.block == b.block {
            let t = text_of(doc, a.block);
            return Some(t.chars().skip(a.offset).take(b.offset - a.offset).collect());
        }
        let (ia, ib) = (doc.index_of(a.block)?, doc.index_of(b.block)?);
        let mut elements = Vec::new();
        for (k, blk) in doc.blocks[ia..=ib].iter().enumerate() {
            let mut e = blk.element.clone();
            let len = e.text.char_len();
            let from = if k == 0 { a.offset } else { 0 };
            let to = if ia + k == ib { b.offset } else { len };
            if let Ok(t) = e.text.delete(to, len).and_then(|t| t.delete(0, from)) {
                e.text = t;
            }
            elements.push(e);
        }
        let script = coldopen_screenplay::Script {
            elements,
            ..Default::default()
        };
        Some(coldopen_fountain::write(&script).text)
    }

    pub fn cut(&mut self, d: &mut Dispatcher) -> Option<String> {
        let text = self.copy(d)?;
        self.delete(d, false, false);
        Some(text)
    }

    /// Pastes clipboard text: within the block when it has no line breaks,
    /// otherwise read as Fountain and inserted as blocks (editor.md §5.4).
    pub fn paste(&mut self, d: &mut Dispatcher, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let Some(kind) = d.session().and_then(|s| kind_of(&s.doc, self.caret.block)) else {
            return;
        };
        if !text.contains('\n') || kind.is_raw_text() {
            self.insert_text(d, &text);
            return;
        }
        let Some(doc) = d.session().map(|s| s.doc.clone()) else {
            return;
        };
        let mut cmds = self.delete_selection(&doc);
        let (start, _) = self.selection(&doc);
        let parsed = coldopen_fountain::parse(&text).script;
        if parsed.elements.is_empty() {
            return;
        }
        let len = len_of(&doc, start.block);
        let id = start.block;
        let mut inserts: Vec<Command> = parsed
            .elements
            .iter()
            .map(|e| {
                let r = coldopen_screenplay::ElementRepr::from_element(e);
                let mut p = BlockInsert::new(e.kind, "");
                p.text = r.text;
                p.runs = r.runs;
                p.scene_number = r.scene_number;
                p.dual = r.dual;
                p.centered = r.centered;
                p.depth = r.depth;
                p.space_before = r.space_before;
                p
            })
            .map(|mut p| {
                if start.offset == 0 && len > 0 {
                    p.before = Some(id);
                } else {
                    p.after = Some(id);
                }
                Command::BlockInsert(p)
            })
            .collect();
        if start.offset > 0 && start.offset < len {
            cmds.push(Command::BlockSplit(BlockSplit {
                id,
                at: start.offset,
                kind: None,
            }));
        }
        if !(start.offset == 0 && len > 0) {
            // Inserting each after the same block reverses them.
            inserts.reverse();
        }
        let n = inserts.len();
        cmds.extend(inserts);
        let first_insert = cmds.len() - n;
        let reversed = !(start.offset == 0 && len > 0);
        if let Some(r) = self.run(d, cmds) {
            let results = r["results"]
                .as_array()
                .cloned()
                .unwrap_or_else(|| vec![r.clone()]);
            let last = if reversed {
                results.get(first_insert)
            } else {
                results.last()
            };
            if let Some(id) = last.and_then(|v| v["id"].as_str()).and_then(BlockId::parse) {
                let len = d.session().map_or(0, |s| len_of(&s.doc, id));
                self.set_caret(
                    Caret {
                        block: id,
                        offset: len,
                    },
                    false,
                );
            }
        }
    }

    /// Asks the core for suggestions at the caret (editor.md §6).
    pub fn refresh_completion(&mut self, d: &mut Dispatcher) {
        let id = self.caret.block;
        let Some(doc) = d.session().map(|s| &s.doc) else {
            self.completion = None;
            return;
        };
        let text = text_of(doc, id);
        if self
            .dismissed
            .as_ref()
            .is_some_and(|(b, t)| *b == id && *t == text)
        {
            self.completion = None;
            return;
        }
        // Suggestions are for the end of what is typed.
        if self.caret.offset != text.chars().count() || self.has_selection() {
            self.completion = None;
            return;
        }
        let cmd = Command::TextComplete(TextComplete {
            id,
            at: self.caret.offset,
        });
        let r = d.execute(cmd).ok();
        let items: Vec<String> = r
            .as_ref()
            .and_then(|r| r["items"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|i| i["text"].as_str().map(str::to_string))
                    .take(8)
                    .collect()
            })
            .unwrap_or_default();
        self.completion = match (r, items.is_empty()) {
            (Some(r), false) => Some(Completion {
                block: id,
                context: r["context"].as_str().unwrap_or("").to_string(),
                from: r["from"].as_u64().unwrap_or(0) as usize,
                to: r["to"].as_u64().unwrap_or(0) as usize,
                items,
                selected: 0,
            }),
            _ => None,
        };
    }

    /// Up or Down in the list: the selection stops at either end, like
    /// every list in the app (editor.md §6).
    pub fn completion_move(&mut self, down: bool) {
        if let Some(c) = &mut self.completion {
            c.selected = if down {
                (c.selected + 1).min(c.items.len().saturating_sub(1))
            } else {
                c.selected.saturating_sub(1)
            };
        }
    }

    pub fn dismiss_completion(&mut self, d: &Dispatcher) {
        if let Some(c) = self.completion.take() {
            let text = d
                .session()
                .map(|s| text_of(&s.doc, c.block))
                .unwrap_or_default();
            self.dismissed = Some((c.block, text));
        }
    }

    /// Replaces the typed fragment with the chosen suggestion, as one step.
    pub fn accept_completion(&mut self, d: &mut Dispatcher) {
        let Some(c) = self.completion.take() else {
            return;
        };
        let Some(item) = c.items.get(c.selected).cloned() else {
            return;
        };
        let mut cmds = Vec::new();
        if c.to > c.from {
            cmds.push(Command::TextDelete(TextDelete {
                id: c.block,
                from: c.from,
                to: c.to,
            }));
        }
        cmds.push(Command::TextInsert(TextInsert {
            id: c.block,
            at: c.from,
            text: item.clone(),
            style: None,
        }));
        if self.run(d, cmds).is_some() {
            self.set_caret(
                Caret {
                    block: c.block,
                    offset: c.from + item.chars().count(),
                },
                false,
            );
        }
        self.refresh_completion(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clusters_and_words() {
        assert_eq!(cluster_starts("e\u{301}a"), vec![0, 2]);
        assert_eq!(word_left("say it's fine", 13), 9);
        assert_eq!(word_left("say it's fine", 9), 4);
        assert_eq!(word_right("say it's fine", 3), 8);
    }
}
