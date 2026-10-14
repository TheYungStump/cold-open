//! From script to flow (layout.md §4): blocks of members, each with its
//! cells, geometry, spacing, wrapped lines and split points.
#![deny(clippy::float_arithmetic)]

use std::collections::BTreeMap;

use coldopen_screenplay::{Align, Element, ElementFormat, ElementKind, PageFormat, Pt, Script};

use crate::page::Diagnostic;
use crate::sentence::sentence_starts;
use crate::text::{self, Look, Text};
use crate::wrap::{self, Line, Measure};

/// Where and how a member's lines are set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Geom {
    pub left: Pt,
    pub right: Pt,
    pub first_indent: Pt,
    pub align: Align,
    pub measure: Measure,
    pub pitch: Pt,
}

impl Geom {
    pub(crate) fn of(f: &ElementFormat, format: &PageFormat) -> Geom {
        Geom {
            left: f.left,
            right: f.right,
            first_indent: f.first_indent,
            align: f.align,
            measure: Measure {
                first: f.cells_per_line(format.pitch, true),
                rest: f.cells_per_line(format.pitch, false),
                hyphens: format.hyphen_breaks,
                overhang: format.hyphen_overhang,
            },
            pitch: format.pitch,
        }
    }

    /// Where a line of `width` cells starts.
    pub(crate) fn line_x(&self, line: &Line) -> Pt {
        let cap = if line.first {
            self.measure.first
        } else {
            self.measure.rest
        };
        let base = if line.first {
            self.left.0 + self.first_indent.0
        } else {
            self.left.0
        };
        let spare = i64::from(cap.saturating_sub(line.width));
        let x = match self.align {
            Align::Left => i64::from(base),
            Align::Center => i64::from(base) + (spare / 2) * i64::from(self.pitch.0),
            Align::Right => {
                i64::from(self.right.0) - i64::from(line.width) * i64::from(self.pitch.0)
            }
        };
        Pt(i32::try_from(x).unwrap_or(base))
    }
}

/// One paragraph of a block.
#[derive(Clone, Debug)]
pub(crate) struct Member {
    pub kind: ElementKind,
    /// Index of the source element in the script.
    pub source: Option<usize>,
    pub paginate_as: ElementKind,
    pub geom: Geom,
    pub text: Text,
    /// Space above this member when it is not the first content on a page.
    pub space: Pt,
    /// Lines wrapped from the start.
    pub lines: Vec<Line>,
    /// Cell indices where a new sentence starts (split points).
    pub sentences: Vec<u32>,
    /// A page break comes before this member.
    pub explicit_break: bool,
    /// The scene number to print, for a heading when numbers are on.
    pub scene_number: Option<String>,
    /// For a character: the dual flag from the script.
    pub dual: bool,
}

impl Member {
    pub(crate) fn is_dialogue_like(&self) -> bool {
        matches!(self.kind, ElementKind::Dialogue | ElementKind::Lyric)
    }

    /// The first `limit` lines from cell `offset`. A page only needs
    /// the lines it can hold; wrapping the rest of a long element on every
    /// page would be quadratic.
    pub(crate) fn lines_from_at_most(&self, offset: u32, limit: usize) -> Vec<Line> {
        let k = if offset == 0 {
            Ok(0)
        } else {
            self.lines.binary_search_by_key(&offset, |l| l.start)
        };
        match k {
            Ok(k) => self.lines[k..].iter().take(limit).copied().collect(),
            Err(_) => wrap::wrap_at_most(&self.text, offset, false, self.geom.measure, limit),
        }
    }

    /// Sentence starts strictly inside `(start, end)`.
    pub(crate) fn sentences_within(&self, start: u32, end: u32) -> &[u32] {
        let a = self.sentences.partition_point(|&s| s <= start);
        let b = self.sentences.partition_point(|&s| s < end);
        self.sentences.get(a..b.max(a)).unwrap_or(&[])
    }

    /// How many lines the text from `offset` takes, counting at most
    /// `limit`: the rules only ask whether a remainder reaches a minimum,
    /// and wrapping a long tail for every candidate would be quadratic.
    pub(crate) fn count_lines_from(&self, offset: u32, limit: u32) -> u32 {
        let n = if offset == 0 {
            self.lines.len()
        } else if let Ok(k) = self.lines.binary_search_by_key(&offset, |l| l.start) {
            self.lines.len() - k
        } else {
            wrap::wrap_at_most(&self.text, offset, false, self.geom.measure, limit as usize).len()
        };
        u32::try_from(n).unwrap_or(u32::MAX).min(limit)
    }

    /// True when cell `i` starts a sentence.
    pub(crate) fn is_sentence_start(&self, i: u32) -> bool {
        self.sentences.binary_search(&i).is_ok()
    }

    /// True when cell `i` follows a line break in the text.
    pub(crate) fn follows_newline(&self, i: u32) -> bool {
        i > 0
            && self
                .text
                .cells
                .get(i as usize - 1)
                .is_some_and(|c| c.kind == text::CellKind::Newline)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockKind {
    /// One member.
    Single,
    /// A dialogue group; `named` when member 0 is the character.
    Group { named: bool },
    /// Two groups side by side; the right one starts at `right`.
    Dual { right: usize },
}

#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub kind: BlockKind,
    pub members: Vec<Member>,
    /// A page break comes before this block.
    pub explicit_break: bool,
    /// Dialogue and lyric lines in members `i..`, for each `i` up to the
    /// member count; filled once the flow is complete.
    pub dialogue_suffix: Vec<u32>,
}

impl Block {
    pub(crate) fn is_named_group(&self) -> bool {
        matches!(self.kind, BlockKind::Group { named: true })
    }

    /// Dialogue and lyric lines in members `from..`.
    pub(crate) fn dialogue_lines_from(&self, from: usize) -> u32 {
        if self.dialogue_suffix.len() == self.members.len() + 1 {
            return self.dialogue_suffix.get(from).copied().unwrap_or(0);
        }
        Self::count_dialogue(&self.members[from.min(self.members.len())..])
    }

    fn count_dialogue(ms: &[Member]) -> u32 {
        ms.iter()
            .filter(|m| m.is_dialogue_like())
            .map(|m| u32::try_from(m.lines.len()).unwrap_or(u32::MAX))
            .fold(0, u32::saturating_add)
    }

    /// Fills `dialogue_suffix` so the paginator's rule checks are O(1).
    fn index(&mut self) {
        let mut v = vec![0u32; self.members.len() + 1];
        for (i, m) in self.members.iter().enumerate().rev() {
            let own = if m.is_dialogue_like() {
                u32::try_from(m.lines.len()).unwrap_or(u32::MAX)
            } else {
                0
            };
            v[i] = v[i + 1].saturating_add(own);
        }
        self.dialogue_suffix = v;
    }

    /// Height in lines of a dual block: the taller column, counting space
    /// between members.
    pub(crate) fn dual_rows(&self, format: &PageFormat) -> u32 {
        let BlockKind::Dual { right } = self.kind else {
            return 0;
        };
        let col = |ms: &[Member]| -> u32 {
            ms.iter()
                .enumerate()
                .map(|(i, m)| {
                    let space = if i == 0 { 0 } else { rows_of(m.space, format) };
                    space + m.lines.len() as u32
                })
                .sum()
        };
        col(&self.members[..right]).max(col(&self.members[right..]))
    }
}

/// Whole lines a space takes, rounded up.
pub(crate) fn rows_of(space: Pt, format: &PageFormat) -> u32 {
    let lh = i64::from(format.line_height.0.max(1));
    let s = i64::from(space.0.max(0));
    u32::try_from((s + lh - 1) / lh).unwrap_or(0)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Flow {
    pub blocks: Vec<Block>,
    pub diagnostics: Vec<Diagnostic>,
}

/// What the flow builder needs besides the script.
pub(crate) struct FlowInput<'a> {
    pub format: &'a PageFormat,
    pub overrides: &'a BTreeMap<usize, Pt>,
    pub scene_numbers: bool,
    /// Forced line starts, by element (see `Options::line_breaks`).
    pub line_breaks: &'a BTreeMap<usize, Vec<u32>>,
}

/// The name as compared for same-speaker continueds: capitals, without an
/// extension in brackets.
fn speaker_key(e: &Element) -> String {
    e.character_name().unwrap_or_default().to_uppercase()
}

/// True when `name` already carries the continued marker (layout.md §7.5).
pub(crate) fn has_contd_marker(name: &str, contd: &str) -> bool {
    let norm = |s: &str| s.to_uppercase().replace('\u{2019}', "'");
    let n = norm(name);
    let marker = norm(contd);
    n.contains(&marker) || n.contains("CONT'D") || n.contains("CONTD")
}

fn make_member(
    e: &Element,
    source: usize,
    fmt: &ElementFormat,
    geom: Geom,
    format: &PageFormat,
    diags: &mut Vec<Diagnostic>,
) -> Member {
    let extra = Look {
        bold: fmt.bold,
        italic: fmt.italic,
        underline: fmt.underline,
    };
    let (text, notes) = text::build(&e.text, fmt.caps, extra);
    if notes.control_chars {
        diags.push(Diagnostic::new(
            "control_char",
            Some(source),
            "control characters were removed",
        ));
    }
    let sentences = sentence_starts(&text.source, format.ellipsis_ends_sentence)
        .into_iter()
        .map(|o| text.cell_at_source(o))
        .filter(|&i| i > 0 && i < text.len())
        .collect::<Vec<_>>();
    let lines = wrap::wrap(&text, 0, true, geom.measure);
    Member {
        kind: e.kind,
        source: Some(source),
        paginate_as: fmt.paginate_as,
        geom,
        text,
        space: Pt::ZERO,
        lines,
        sentences,
        explicit_break: false,
        scene_number: None,
        dual: e.dual,
    }
}

/// Re-wraps a member after its text changed (a continued name).
fn rewrap(m: &mut Member) {
    m.lines = wrap::wrap(&m.text, 0, true, m.geom.measure);
}

/// Geometry for a member of a dual-dialogue column.
fn dual_geom(kind: ElementKind, col: coldopen_screenplay::DualColumn, format: &PageFormat) -> Geom {
    let d = &format.dual;
    let base = format
        .elements
        .get(kind)
        .cloned()
        .unwrap_or_else(|| format.elements.dialogue.clone());
    let mut f = base;
    f.cells = None;
    match kind {
        ElementKind::Character => {
            f.left = Pt(col.left.0 + d.character_indent.0);
            f.right = col.right;
            f.first_indent = Pt::ZERO;
        }
        ElementKind::Parenthetical => {
            f.left = Pt(col.left.0 + d.parenthetical_inset.0);
            f.right = Pt(col.right.0 - d.parenthetical_inset.0);
        }
        _ => {
            f.left = col.left;
            f.right = col.right;
            f.first_indent = Pt::ZERO;
        }
    }
    if f.right.0 <= f.left.0 {
        f.right = Pt(f.left.0 + format.pitch.0.max(1));
    }
    if f.left.0 + f.first_indent.0 < 0 || f.left.0 + f.first_indent.0 >= f.right.0 {
        f.first_indent = Pt::ZERO;
    }
    Geom::of(&f, format)
}

/// Builds the flow for a script.
pub(crate) fn build(script: &Script, input: &FlowInput<'_>) -> Flow {
    let format = input.format;
    let mut diags = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut prev_kind: Option<ElementKind> = None;
    let mut pending_break = false;
    let mut printed_any = false;
    let mut scene_index = 0usize;
    // Same-speaker continueds: the last speaker in this scene, and whether
    // only action came after them.
    let mut last_speaker: Option<String> = None;
    let mut only_action_since = false;

    for (idx, e) in script.elements.iter().enumerate() {
        let kind = e.kind;
        if kind == ElementKind::PageBreak {
            if printed_any {
                pending_break = true;
            }
            continue;
        }
        if kind.is_transparent() {
            if kind == ElementKind::Section {
                last_speaker = None;
            }
            continue;
        }
        let Some(fmt) = format.elements.get(kind) else {
            continue;
        };
        let explicit = printed_any && (pending_break || fmt.starts_new_page);
        pending_break = false;

        let mut geom = Geom::of(fmt, format);
        if kind == ElementKind::Action && e.centered {
            geom.align = Align::Center;
        }
        let mut m = make_member(e, idx, fmt, geom, format, &mut diags);
        if let Some(starts) = input.line_breaks.get(&idx) {
            let cells: Vec<u32> = starts.iter().map(|&b| m.text.cell_at_source(b)).collect();
            m.lines = wrap::lines_at(&m.text, &cells);
        }

        // Spacing (layout.md §6).
        let usual = kind.default_gap(prev_kind);
        let space = if let Some(&o) = input.overrides.get(&idx) {
            o
        } else if let Some(n) = e.space_before.filter(|&n| n != usual) {
            Pt(i32::from(n).saturating_mul(format.line_height.0))
        } else if usual == 0 {
            Pt::ZERO
        } else {
            fmt.space_before
        };
        // Overrides come from files and are not validated with the format;
        // a space taller than the body behaves the same as the body.
        m.space = space.clamp(Pt::ZERO, format.body_height());

        // Scene numbers and same-speaker continueds.
        match kind {
            ElementKind::SceneHeading => {
                scene_index += 1;
                last_speaker = None;
                if input.scene_numbers {
                    m.scene_number = Some(
                        e.scene_number
                            .clone()
                            .unwrap_or_else(|| scene_index.to_string()),
                    );
                }
            }
            ElementKind::Character => {
                let key = speaker_key(e);
                if format.contd_after_action
                    && only_action_since
                    && last_speaker.as_deref() == Some(key.as_str())
                    && let Some(contd) = &format.contd
                    && !has_contd_marker(&m.text.printed, contd)
                {
                    let mut name = e.text.clone();
                    let mut runs = name.into_runs();
                    runs.push(coldopen_screenplay::Run::plain(format!(" {contd}")));
                    name = coldopen_screenplay::RichText::new(runs);
                    let extra = Look {
                        bold: fmt.bold,
                        italic: fmt.italic,
                        underline: fmt.underline,
                    };
                    m.text = text::build(&name, fmt.caps, extra).0;
                    m.sentences.clear();
                    rewrap(&mut m);
                }
                last_speaker = Some(key);
                only_action_since = false;
            }
            ElementKind::Action | ElementKind::General => {
                if last_speaker.is_some() {
                    only_action_since = true;
                }
            }
            ElementKind::Parenthetical | ElementKind::Dialogue | ElementKind::Lyric => {}
            _ => {
                last_speaker = None;
                only_action_since = false;
            }
        }

        // Grouping.
        // A page break inside a group keeps the group together; the break
        // becomes a member break (§7.3).
        let continues_group = kind.is_dialogue_member()
            && prev_kind.is_some_and(|p| p == ElementKind::Character || p.is_dialogue_member());
        if continues_group && let Some(b) = blocks.last_mut() {
            m.explicit_break = explicit;
            b.members.push(m);
        } else {
            let block_kind = match kind {
                ElementKind::Character => BlockKind::Group { named: true },
                k if k.is_dialogue_member() => BlockKind::Group { named: false },
                _ => BlockKind::Single,
            };
            blocks.push(Block {
                kind: block_kind,
                members: vec![m],
                explicit_break: explicit,
                dialogue_suffix: Vec::new(),
            });
        }
        prev_kind = Some(kind);
        printed_any = true;
    }

    // Empty elements at the end are dropped.
    while let Some(b) = blocks.last_mut() {
        while b.members.len() > 1 && b.members.last().is_some_and(|m| m.text.cells.is_empty()) {
            b.members.pop();
        }
        if b.members.len() == 1 && b.members[0].text.cells.is_empty() {
            blocks.pop();
        } else {
            break;
        }
    }

    let mut blocks = pair_duals(blocks, format, &mut diags);
    for b in &mut blocks {
        b.index();
    }
    Flow {
        blocks,
        diagnostics: diags,
    }
}

/// Pairs dual characters with the group before them (layout.md §4.4).
fn pair_duals(blocks: Vec<Block>, format: &PageFormat, diags: &mut Vec<Diagnostic>) -> Vec<Block> {
    let lh = i64::from(format.line_height.0.max(1));
    let body_rows = u32::try_from(i64::from(format.body_height().0) / lh).unwrap_or(0);
    let insert_rows = 2;
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    let mut paired_last = false;
    for b in blocks {
        let is_dual = b.is_named_group() && b.members.first().is_some_and(|m| m.dual);
        if !is_dual {
            out.push(b);
            paired_last = false;
            continue;
        }
        let source = b.members[0].source;
        // Both groups need something to say; a name alone keeps its own keep
        // rules.
        let partner = if !b.explicit_break
            && !paired_last
            && b.members.len() > 1
            && out
                .last()
                .is_some_and(|p| p.is_named_group() && p.members.len() > 1)
        {
            out.pop()
        } else {
            None
        };
        let Some(left) = partner else {
            diags.push(Diagnostic::new(
                "dual_without_partner",
                source,
                "a dual character has no dialogue group before it to pair with",
            ));
            out.push(b);
            paired_last = false;
            continue;
        };
        let right_start = left.members.len();
        let mut members = Vec::with_capacity(left.members.len() + b.members.len());
        let explicit = left.explicit_break;
        let first_space = left.members.first().map_or(Pt::ZERO, |m| m.space);
        for (side, ms) in [(0, left.members), (1, b.members)] {
            let col = if side == 0 {
                format.dual.left
            } else {
                format.dual.right
            };
            for mut m in ms {
                m.geom = dual_geom(m.kind, col, format);
                m.explicit_break = false;
                rewrap(&mut m);
                members.push(m);
            }
        }
        if let Some(m) = members.first_mut() {
            m.space = first_space;
        }
        // The right column's first space is ignored while the pair is a
        // dual block, and kept for when a pair too tall for a page is set as
        // two groups (§4.4).
        let dual = Block {
            kind: BlockKind::Dual { right: right_start },
            members,
            explicit_break: explicit,
            dialogue_suffix: Vec::new(),
        };
        if dual.dual_rows(format) + insert_rows > body_rows {
            diags.push(Diagnostic::new(
                "dual_too_tall",
                source,
                "a dual-dialogue pair is taller than a page and is set one group after the other",
            ));
            let (l, r) = unpair(dual, format);
            out.push(l);
            out.push(r);
            paired_last = false;
            continue;
        }
        out.push(dual);
        paired_last = true;
    }
    out
}

/// Turns a dual block back into two sequential groups with normal geometry.
fn unpair(b: Block, format: &PageFormat) -> (Block, Block) {
    let BlockKind::Dual { right } = b.kind else {
        return (
            b,
            Block {
                kind: BlockKind::Single,
                members: Vec::new(),
                explicit_break: false,
                dialogue_suffix: Vec::new(),
            },
        );
    };
    let mut members = b.members;
    let right_members = members.split_off(right);
    let regeom = |ms: Vec<Member>| -> Vec<Member> {
        ms.into_iter()
            .map(|mut m| {
                if let Some(f) = format.elements.get(m.kind) {
                    m.geom = Geom::of(f, format);
                }
                rewrap(&mut m);
                m
            })
            .collect()
    };
    let r = regeom(right_members);
    (
        Block {
            kind: BlockKind::Group { named: true },
            members: regeom(members),
            explicit_break: b.explicit_break,
            dialogue_suffix: Vec::new(),
        },
        Block {
            kind: BlockKind::Group { named: true },
            members: r,
            explicit_break: false,
            dialogue_suffix: Vec::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contd_markers_match_only_as_specified() {
        assert!(has_contd_marker("MAYA (CONT'D)", "(CONT'D)"));
        assert!(has_contd_marker("Maya (cont’d)", "(CONT'D)"));
        assert!(has_contd_marker("MAYA (CONTD)", "(CONT)"));
        assert!(has_contd_marker("MAYA (CONT)", "(CONT)"));
        // A marker's letters inside a name are not the marker.
        assert!(!has_contd_marker("CONTRERAS", "(CONT)"));
        assert!(!has_contd_marker("MAYA", "()"));
    }
}
