//! The paginator (layout.md §7): lays out one page from a cursor.
#![deny(clippy::float_arithmetic)]

use std::collections::BTreeSet;

use coldopen_screenplay::{ElementKind, PageFormat, Pt, SplitMode};

use crate::flow::{Block, BlockKind, Flow, Member, has_contd_marker};
use crate::page::{BreakCause, Cluster, Diagnostic, LineKind, PlacedLine, Rule};
use crate::text::{self, Look, Text};
use crate::wrap::{self, Line};

/// A position in the flow: block, member, and cell offset in the member's
/// text (layout.md §7.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cursor {
    pub block: u32,
    pub member: u32,
    pub offset: u32,
}

impl Cursor {
    pub const START: Cursor = Cursor {
        block: 0,
        member: 0,
        offset: 0,
    };
}

/// Rule switches, for experiments such as the reference comparison's
/// cause analysis. Everything is on by default.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub heading: bool,
    pub name: bool,
    pub min_lines: u32,
    pub parenthetical: bool,
    pub transition: bool,
    pub sentences: bool,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            heading: true,
            name: true,
            min_lines: 2,
            parenthetical: true,
            transition: true,
            sentences: true,
        }
    }
}

/// One laid-out page body, before page numbers.
#[derive(Clone, Debug)]
pub(crate) struct PageBody {
    pub lines: Vec<PlacedLine>,
    pub cause: BreakCause,
    pub diagnostics: Vec<Diagnostic>,
    pub start: Cursor,
    pub next: Cursor,
    /// (element, top, bottom) of each content row on the page, from the
    /// body top.
    pub spans: Vec<(usize, Pt, Pt)>,
}

/// A placed content row.
#[derive(Clone, Copy, Debug)]
struct Row {
    block: u32,
    member: u32,
    line: Line,
    /// Top of the line, from the body top.
    y: Pt,
    /// Space placed above it (first row of a member only).
    space: Pt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandKind {
    Block,
    Member,
    Split { sentence: bool },
}

#[derive(Clone, Copy, Debug)]
struct Cand {
    next: Cursor,
    /// Rows kept on this page.
    rows: usize,
    /// For a mid-line split: the last kept row ends before this cell.
    cut: Option<u32>,
    foot: bool,
    level: u8,
    violated: u8,
}

fn rule_bit(r: Rule) -> u8 {
    1 << (r as u8)
}

fn rules_of(bits: u8) -> Vec<Rule> {
    [
        Rule::Heading,
        Rule::Name,
        Rule::DialogueSplit,
        Rule::Parenthetical,
        Rule::ActionSplit,
        Rule::Transition,
        Rule::Sentence,
        Rule::NoSplit,
    ]
    .into_iter()
    .filter(|&r| bits & rule_bit(r) != 0)
    .collect()
}

/// Running counts while a page fills.
#[derive(Clone, Copy, Debug, Default)]
struct Counts {
    /// Content rows after the last heading-like row; `None` with no heading
    /// on the page.
    since_heading: Option<u32>,
    /// The open group: its block, whether its name is on this page, and its
    /// dialogue rows on this page.
    group: Option<(u32, bool, u32)>,
    /// Rows of the current member on this page.
    member_rows: u32,
    content_rows: u32,
}

pub(crate) struct Paginator<'a> {
    pub flow: &'a Flow,
    pub format: &'a PageFormat,
    pub rules: Rules,
}

impl Paginator<'_> {
    fn is_end(&self, c: Cursor) -> bool {
        c.block as usize >= self.flow.blocks.len()
    }

    pub(crate) fn end(&self) -> Cursor {
        Cursor {
            block: self.flow.blocks.len() as u32,
            member: 0,
            offset: 0,
        }
    }

    /// The canonical form of a cursor (layout.md §7.1).
    pub(crate) fn canonical(&self, mut c: Cursor) -> Cursor {
        loop {
            let Some(b) = self.flow.blocks.get(c.block as usize) else {
                return self.end();
            };
            if matches!(b.kind, BlockKind::Dual { .. }) && (c.member > 0 || c.offset > 0) {
                // A dual block is never entered part way.
                c.member = 0;
                c.offset = 0;
                return c;
            }
            let Some(m) = b.members.get(c.member as usize) else {
                c = Cursor {
                    block: c.block + 1,
                    member: 0,
                    offset: 0,
                };
                continue;
            };
            if c.offset > 0 {
                let n = m.text.len();
                let mut o = c.offset.min(n);
                while o < n && m.text.cells[o as usize].kind != text::CellKind::Glyph {
                    o += 1;
                }
                if o >= n {
                    c = Cursor {
                        block: c.block,
                        member: c.member + 1,
                        offset: 0,
                    };
                    continue;
                }
                c.offset = o;
            }
            return c;
        }
    }

    fn block(&self, c: Cursor) -> &Block {
        &self.flow.blocks[c.block as usize]
    }

    fn line_h(&self) -> Pt {
        self.format.line_height
    }

    fn more_rows(&self, b: &Block) -> u32 {
        match (&self.format.more, b.members.first()) {
            (Some(s), Some(name)) if b.is_named_group() => {
                let t = text::build_plain(s, false, Look::default());
                wrap::wrap(&t, 0, true, name.geom.measure).len() as u32
            }
            _ => 0,
        }
    }

    /// The continued name for a group: its printed name plus the marker.
    fn contd_text(&self, b: &Block) -> Option<Text> {
        let contd = self.format.contd.as_ref()?;
        let name = b.members.first()?;
        let printed = &name.text.printed;
        let s = if has_contd_marker(printed, contd) {
            printed.clone()
        } else {
            format!("{printed} {contd}")
        };
        let look = name.text.cells.first().map(|c| c.look).unwrap_or_default();
        Some(text::build_plain(&s, false, look))
    }

    fn is_continuation(&self, c: Cursor) -> bool {
        !self.is_end(c) && self.block(c).is_named_group() && (c.member > 0 || c.offset > 0)
    }

    /// Lays out one page starting at `start`.
    pub(crate) fn page_from(&self, start: Cursor) -> PageBody {
        let start = self.canonical(start);
        let lh = self.line_h();
        let body = self.format.body_height();
        let in_margin = self.format.more_contd_in_margin;
        let mut diags = Vec::new();
        let mut lines: Vec<PlacedLine> = Vec::new();
        if self.is_end(start) {
            return PageBody {
                lines,
                cause: BreakCause::End,
                diagnostics: diags,
                start,
                next: start,
                spans: Vec::new(),
            };
        }

        // Head insert: the continued name.
        let mut y = Pt::ZERO;
        let mut head: Vec<PlacedLine> = Vec::new();
        if self.is_continuation(start) {
            let b = self.block(start);
            if let (Some(t), Some(name)) = (self.contd_text(b), b.members.first()) {
                let ls = wrap::wrap(&t, 0, true, name.geom.measure);
                let h = Pt(lh.0.saturating_mul(ls.len() as i32));
                let first_rows = self.first_rows_needed(start);
                if in_margin {
                    let top = Pt(-h.0);
                    for (k, l) in ls.iter().enumerate() {
                        head.push(self.place(
                            &t,
                            l,
                            &name.geom,
                            Pt(top.0.saturating_add(lh.0.saturating_mul(k as i32))),
                            LineKind::ContdName,
                            None,
                        ));
                    }
                } else if h.0.saturating_add(first_rows.0) <= body.0 {
                    for (k, l) in ls.iter().enumerate() {
                        head.push(self.place(
                            &t,
                            l,
                            &name.geom,
                            Pt(lh.0.saturating_mul(k as i32)),
                            LineKind::ContdName,
                            None,
                        ));
                    }
                    y = h;
                } else {
                    diags.push(Diagnostic::new(
                        "inserts_dropped",
                        name.source,
                        "the continued name did not fit and was left out",
                    ));
                }
            }
        }

        let mut rows: Vec<Row> = Vec::new();
        let mut dual_lines: Vec<(usize, PlacedLine)> = Vec::new();
        let mut cnt = Counts::default();
        let mut best: [Option<Cand>; 3] = [None, None, None];
        let mut stop: Option<(Cand, BreakCause)> = None;
        let mut pos = start;

        'fill: loop {
            if self.is_end(pos) {
                stop = Some((
                    Cand {
                        next: pos,
                        rows: rows.len(),
                        cut: None,
                        foot: false,
                        level: 0,
                        violated: 0,
                    },
                    BreakCause::End,
                ));
                break;
            }
            let b = self.block(pos);
            let first_of_block = pos.member == 0 && pos.offset == 0;
            let m = &b.members[pos.member as usize];

            // Explicit breaks end a page that already has content.
            let explicit =
                (first_of_block && b.explicit_break) || (pos.offset == 0 && m.explicit_break);
            if explicit && cnt.content_rows > 0 {
                let mut foot = b.is_named_group() && pos.member > 0 && self.format.more.is_some();
                if foot && !in_margin && y.0 + lh.0 * self.more_rows(b) as i32 > body.0 {
                    // The break is forced here and (MORE) has no room.
                    foot = false;
                    diags.push(Diagnostic::new(
                        "inserts_dropped",
                        m.source,
                        "a (MORE) line did not fit before a page break and was left out",
                    ));
                }
                stop = Some((
                    Cand {
                        next: pos,
                        rows: rows.len(),
                        cut: None,
                        foot,
                        level: 0,
                        violated: 0,
                    },
                    BreakCause::Explicit,
                ));
                break;
            }

            if let BlockKind::Dual { right } = b.kind {
                let space = if cnt.content_rows > 0 {
                    m.space
                } else {
                    Pt::ZERO
                };
                let drows = b.dual_rows(self.format);
                let h = Pt(lh.0.saturating_mul(drows as i32));
                if y.0.saturating_add(space.0).saturating_add(h.0) > body.0 && cnt.content_rows > 0
                {
                    break 'fill;
                }
                let top = Pt(y.0 + space.0);
                for range in [0..right, right..b.members.len()] {
                    let mut cy = top;
                    for (k, mi) in range.enumerate() {
                        let mm = &b.members[mi];
                        if k > 0 {
                            cy =
                                Pt(cy.0
                                    + crate::flow::rows_of(mm.space, self.format) as i32 * lh.0);
                        }
                        for l in &mm.lines {
                            let pl = self.place(
                                &mm.text,
                                l,
                                &mm.geom,
                                cy,
                                LineKind::Content(mm.kind),
                                mm.source,
                            );
                            dual_lines.push((rows.len(), pl));
                            cy = Pt(cy.0 + lh.0);
                        }
                    }
                }
                // A dual block counts as one atomic row group.
                rows.push(Row {
                    block: pos.block,
                    member: 0,
                    line: Line {
                        start: 0,
                        end: 0,
                        next: 0,
                        width: 0,
                        first: true,
                        hard: false,
                    },
                    y: top,
                    space,
                });
                y = Pt(top.0 + h.0);
                cnt.content_rows += drows;
                cnt.since_heading = cnt.since_heading.map(|n| n + drows);
                cnt.group = None;
                cnt.member_rows = 0;
                let next = Cursor {
                    block: pos.block + 1,
                    member: 0,
                    offset: 0,
                };
                self.offer(
                    &mut best,
                    &cnt,
                    pos,
                    CandKind::Block,
                    next,
                    rows.len(),
                    None,
                    y,
                    0,
                );
                pos = next;
                continue;
            }

            // Entering a group or a new member.
            if b.is_named_group() || matches!(b.kind, BlockKind::Group { .. }) {
                if cnt.group.map(|g| g.0) != Some(pos.block) {
                    let name_here = b.is_named_group() && pos.member == 0 && pos.offset == 0;
                    cnt.group = Some((pos.block, name_here, 0));
                }
            } else {
                cnt.group = None;
            }
            cnt.member_rows = 0;

            // No page holds more than `page_rows` lines, and the rules look
            // `min_lines` past the last one.
            let page_rows = usize::try_from(body.0 / lh.0.max(1)).unwrap_or(0);
            let limit = page_rows + self.rules.min_lines as usize + 2;
            let ls = m.lines_from_at_most(pos.offset, limit);
            let first_space = if cnt.content_rows == 0 || pos.offset > 0 {
                Pt::ZERO
            } else {
                m.space
            };
            for (li, l) in ls.iter().enumerate() {
                let space = if li == 0 { first_space } else { Pt::ZERO };
                // The first content row is always placed, so every page makes
                // progress (§7.6).
                if y.0.saturating_add(space.0).saturating_add(lh.0) > body.0 && cnt.content_rows > 0
                {
                    break 'fill;
                }
                let top = Pt(y.0 + space.0);
                rows.push(Row {
                    block: pos.block,
                    member: pos.member,
                    line: *l,
                    y: top,
                    space,
                });
                y = Pt(top.0 + lh.0);
                cnt.content_rows += 1;
                cnt.member_rows += 1;
                if m.paginate_as == ElementKind::SceneHeading {
                    cnt.since_heading = Some(0);
                } else if let Some(n) = cnt.since_heading.as_mut() {
                    *n += 1;
                }
                if m.is_dialogue_like()
                    && let Some(g) = cnt.group.as_mut()
                {
                    g.2 += 1;
                }

                // Candidates after this row.
                if self.format.split_mode == SplitMode::MidLine && self.format.break_at_sentences {
                    for &s in m.sentences_within(l.start, l.end) {
                        let rest = m.count_lines_from(s, self.rules.min_lines);
                        let next = Cursor {
                            block: pos.block,
                            member: pos.member,
                            offset: s,
                        };
                        self.offer(
                            &mut best,
                            &cnt,
                            pos,
                            CandKind::Split { sentence: true },
                            next,
                            rows.len(),
                            Some(s),
                            y,
                            rest,
                        );
                    }
                }
                if li + 1 < ls.len() {
                    let s = ls[li + 1].start;
                    let sentence = m.is_sentence_start(self.canonical_offset(m, s));
                    let rest = (ls.len() - li - 1) as u32;
                    let next = Cursor {
                        block: pos.block,
                        member: pos.member,
                        offset: s,
                    };
                    self.offer(
                        &mut best,
                        &cnt,
                        pos,
                        CandKind::Split { sentence },
                        next,
                        rows.len(),
                        None,
                        y,
                        rest,
                    );
                } else if (pos.member as usize) + 1 < b.members.len() {
                    let next = Cursor {
                        block: pos.block,
                        member: pos.member + 1,
                        offset: 0,
                    };
                    self.offer(
                        &mut best,
                        &cnt,
                        pos,
                        CandKind::Member,
                        next,
                        rows.len(),
                        None,
                        y,
                        0,
                    );
                } else {
                    let next = Cursor {
                        block: pos.block + 1,
                        member: 0,
                        offset: 0,
                    };
                    self.offer(
                        &mut best,
                        &cnt,
                        pos,
                        CandKind::Block,
                        next,
                        rows.len(),
                        None,
                        y,
                        0,
                    );
                }
            }
            if ls.len() == limit {
                // Only reachable if a page could hold `limit` lines; the
                // member is not finished, so neither is the page.
                break 'fill;
            }
            pos = if (pos.member as usize) + 1 < b.members.len() {
                Cursor {
                    block: pos.block,
                    member: pos.member + 1,
                    offset: 0,
                }
            } else {
                Cursor {
                    block: pos.block + 1,
                    member: 0,
                    offset: 0,
                }
            };
        }

        let (cand, cause) = match stop {
            Some(s) => s,
            None => match best.iter().flatten().next() {
                Some(c) => {
                    let c = *c;
                    let cause = if c.level == 0 {
                        BreakCause::Natural
                    } else {
                        BreakCause::Fallback {
                            level: c.level,
                            rules: rules_of(c.violated),
                        }
                    };
                    (c, cause)
                }
                None => {
                    // Nothing fits with its (MORE) line: keep the first row and
                    // drop the inserts.
                    diags.push(Diagnostic::new(
                        "inserts_dropped",
                        None,
                        "a (MORE) line did not fit and was left out",
                    ));
                    let r = rows.first().copied();
                    let next = r.map_or(self.end(), |r| self.after_row(&r));
                    (
                        Cand {
                            next,
                            rows: rows.len().min(1),
                            cut: None,
                            foot: false,
                            level: 2,
                            violated: 0,
                        },
                        BreakCause::Fallback {
                            level: 2,
                            rules: Vec::new(),
                        },
                    )
                }
            },
        };

        // Build the page.
        lines.extend(head);
        let mut spans = Vec::new();
        let mut kept_y = Pt::ZERO;
        let mut seen_heading_numbers = BTreeSet::new();
        for (i, r) in rows.iter().take(cand.rows).enumerate() {
            let b = &self.flow.blocks[r.block as usize];
            if let BlockKind::Dual { .. } = b.kind {
                for (k, (_, pl)) in dual_lines.iter().filter(|(ri, _)| *ri == i).enumerate() {
                    lines.push(pl.clone());
                    if let Some((e, _, _)) = pl.source {
                        let top = Pt(pl.y.0 - self.format.margin_top.0 - self.format.baseline.0);
                        let space = if k == 0 { r.space } else { Pt::ZERO };
                        spans.push((e, Pt(top.0 - space.0), Pt(top.0 + lh.0)));
                    }
                }
                kept_y = Pt(r.y.0 + lh.0 * b.dual_rows(self.format) as i32);
                continue;
            }
            let m = &b.members[r.member as usize];
            let mut l = r.line;
            if i + 1 == cand.rows
                && let Some(cut) = cand.cut
            {
                let mut e = cut;
                while e > l.start && m.text.cells[e as usize - 1].kind != text::CellKind::Glyph {
                    e -= 1;
                }
                l.end = e;
                l.width = m.text.cells[l.start as usize..l.end as usize]
                    .iter()
                    .map(|c| u32::from(c.width))
                    .sum();
            }
            lines.push(self.place(
                &m.text,
                &l,
                &m.geom,
                r.y,
                LineKind::Content(m.kind),
                m.source,
            ));
            if l.first
                && let Some(n) = &m.scene_number
                && seen_heading_numbers.insert((r.block, r.member))
            {
                let baseline = self.baseline_y(r.y);
                for x in [
                    self.format.scene_number_left,
                    self.format.scene_number_right,
                ] {
                    lines.push(self.plain_line(n, x, baseline, LineKind::SceneNumber));
                }
            }
            if let Some(e) = m.source {
                spans.push((e, Pt(r.y.0 - r.space.0), Pt(r.y.0 + lh.0)));
            }
            kept_y = Pt(r.y.0 + lh.0);
        }
        if cand.foot
            && let Some(more) = &self.format.more
        {
            let b = &self.flow.blocks[cand_block(&cand, &rows) as usize];
            if let Some(name) = b.members.first() {
                let t = text::build_plain(more, false, Look::default());
                let ls = wrap::wrap(&t, 0, true, name.geom.measure);
                let top = if in_margin { body } else { kept_y };
                for (k, l) in ls.iter().enumerate() {
                    let mut g = name.geom;
                    g.first_indent = Pt::ZERO;
                    g.align = coldopen_screenplay::Align::Left;
                    lines.push(self.place(
                        &t,
                        l,
                        &g,
                        Pt(top.0 + lh.0 * k as i32),
                        LineKind::More,
                        None,
                    ));
                }
            }
        }
        let next = self.canonical(cand.next);
        PageBody {
            lines,
            cause,
            diagnostics: diags,
            start,
            next,
            spans,
        }
    }

    /// Height the first content line needs: one line, plus a (MORE) line if
    /// the page starts inside a group.
    fn first_rows_needed(&self, c: Cursor) -> Pt {
        let lh = self.line_h();
        let b = self.block(c);
        let extra = if self.format.more_contd_in_margin {
            0
        } else {
            self.more_rows(b)
        };
        Pt(lh.0.saturating_mul(1 + extra as i32))
    }

    fn canonical_offset(&self, m: &Member, mut s: u32) -> u32 {
        let n = m.text.len();
        while s < n && m.text.cells[s as usize].kind != text::CellKind::Glyph {
            s += 1;
        }
        s
    }

    fn after_row(&self, r: &Row) -> Cursor {
        let b = &self.flow.blocks[r.block as usize];
        if matches!(b.kind, BlockKind::Dual { .. }) {
            return Cursor {
                block: r.block + 1,
                member: 0,
                offset: 0,
            };
        }
        let m = &b.members[r.member as usize];
        if r.line.next < m.text.len() {
            return Cursor {
                block: r.block,
                member: r.member,
                offset: r.line.next,
            };
        }
        self.canonical(Cursor {
            block: r.block,
            member: r.member + 1,
            offset: 0,
        })
    }

    /// Evaluates a candidate and records it as the latest at its level.
    #[allow(clippy::too_many_arguments)]
    fn offer(
        &self,
        best: &mut [Option<Cand>; 3],
        cnt: &Counts,
        pos: Cursor,
        kind: CandKind,
        next: Cursor,
        rows: usize,
        cut: Option<u32>,
        y: Pt,
        rest_lines: u32,
    ) {
        let b = self.block(pos);
        let m = &b.members[pos.member as usize];
        let r = self.rules;
        let min = r.min_lines;
        let mut v: u8 = 0;
        let in_group = matches!(b.kind, BlockKind::Group { .. });
        let named = b.is_named_group();

        if r.heading && cnt.since_heading.is_some_and(|n| n < min) {
            v |= rule_bit(Rule::Heading);
        }
        let group_rows = cnt.group.filter(|g| g.0 == pos.block).map_or(0, |g| g.2);
        let name_on_page = cnt.group.filter(|g| g.0 == pos.block).is_some_and(|g| g.1);
        match kind {
            CandKind::Block => {
                if r.name
                    && named
                    && b.members.len() == 1
                    && !matches!(b.kind, BlockKind::Dual { .. })
                {
                    v |= rule_bit(Rule::Name);
                }
                // A page never ends right after a parenthetical; in a dual
                // block, after either column's last member.
                let ends_in_paren = |ms: &[Member]| {
                    ms.last()
                        .is_some_and(|m| m.kind == ElementKind::Parenthetical)
                };
                let paren_last = match b.kind {
                    BlockKind::Dual { right } => {
                        ends_in_paren(&b.members[..right]) || ends_in_paren(&b.members[right..])
                    }
                    _ => ends_in_paren(&b.members),
                };
                if r.parenthetical && paren_last {
                    v |= rule_bit(Rule::Parenthetical);
                }
                if r.transition
                    && let Some(nb) = self.flow.blocks.get(next.block as usize)
                    && nb
                        .members
                        .first()
                        .is_some_and(|m| m.paginate_as == ElementKind::Transition)
                {
                    v |= rule_bit(Rule::Transition);
                }
            }
            CandKind::Member => {
                let total = b.dialogue_lines_from(0);
                if r.name && named && name_on_page && group_rows < min.min(total) {
                    v |= rule_bit(Rule::Name);
                }
                if r.name && named && pos.member == 0 {
                    v |= rule_bit(Rule::Name);
                }
                let after = b.dialogue_lines_from(pos.member as usize + 1);
                if group_rows < min || after < min {
                    v |= rule_bit(Rule::DialogueSplit);
                }
                if r.parenthetical && (m.kind == ElementKind::Parenthetical || after == 0) {
                    v |= rule_bit(Rule::Parenthetical);
                }
            }
            CandKind::Split { sentence } => {
                let never = matches!(
                    m.kind,
                    ElementKind::SceneHeading
                        | ElementKind::Shot
                        | ElementKind::Character
                        | ElementKind::Transition
                        | ElementKind::Parenthetical
                ) || m.paginate_as == ElementKind::SceneHeading;
                if never {
                    v |= rule_bit(Rule::NoSplit);
                }
                let offset = next.offset;
                if m.kind == ElementKind::Lyric {
                    if !m.follows_newline(offset) {
                        v |= rule_bit(Rule::NoSplit);
                    }
                } else if self.format.break_at_sentences && r.sentences && !sentence {
                    v |= rule_bit(Rule::Sentence);
                }
                let split_rule = if in_group {
                    Rule::DialogueSplit
                } else {
                    Rule::ActionSplit
                };
                if cnt.member_rows < min || rest_lines < min {
                    v |= rule_bit(split_rule);
                }
                if in_group {
                    let total = b.dialogue_lines_from(0);
                    if r.name && named && name_on_page && group_rows < min.min(total) {
                        v |= rule_bit(Rule::Name);
                    }
                }
            }
        }

        let level = if v == 0 {
            0
        } else if v == rule_bit(Rule::Sentence) {
            1
        } else {
            2
        };
        let foot = named && !matches!(kind, CandKind::Block) && self.format.more.is_some();
        let foot_h = if foot && !self.format.more_contd_in_margin {
            Pt(self.line_h().0.saturating_mul(self.more_rows(b) as i32))
        } else {
            Pt::ZERO
        };
        if y.0 + foot_h.0 > self.format.body_height().0 {
            return;
        }
        let cand = Cand {
            next,
            rows,
            cut,
            foot,
            level,
            violated: v,
        };
        for slot in best.iter_mut().skip(level as usize) {
            *slot = Some(cand);
        }
    }

    fn baseline_y(&self, top: Pt) -> Pt {
        Pt(self.format.margin_top.0 + top.0 + self.format.baseline.0)
    }

    /// Places a wrapped line of `t` whose top is `top` (from the body top).
    fn place(
        &self,
        t: &Text,
        l: &Line,
        g: &crate::flow::Geom,
        top: Pt,
        kind: LineKind,
        source: Option<usize>,
    ) -> PlacedLine {
        let cells = t.cells.get(l.start as usize..l.end as usize).unwrap_or(&[]);
        let mut text = String::new();
        let mut clusters = Vec::with_capacity(cells.len());
        for c in cells {
            text.push_str(t.cell_text(c));
            clusters.push(Cluster {
                end: text.len() as u32,
                cells: c.width,
                look: c.look,
            });
        }
        let src = match (source, cells.first(), cells.last()) {
            (Some(e), Some(a), Some(z)) => Some((e, a.src_start, z.src_end)),
            (Some(e), _, _) => Some((e, 0, 0)),
            _ => None,
        };
        PlacedLine {
            x: g.line_x(l),
            y: self.baseline_y(top),
            kind,
            source: src,
            text,
            clusters,
        }
    }

    /// A one-line plain string at `x` with baseline `y`.
    pub(crate) fn plain_line(&self, s: &str, x: Pt, y: Pt, kind: LineKind) -> PlacedLine {
        plain_line(s, x, y, kind)
    }

    /// The continuous draft view (layout.md §15.3): every element's lines
    /// one after another, with the space above each, and no pages. `starts`
    /// are the cursors where body pages begin; their tops are returned.
    pub(crate) fn draft(&self, starts: &[Cursor]) -> crate::Draft {
        let lh = self.line_h();
        let mut lines = Vec::new();
        // (cursor of a line's first cell, its top), in flow order.
        let mut marks: Vec<(Cursor, Pt)> = Vec::new();
        let mut y = Pt::ZERO;
        let mut first = true;
        for (bi, b) in self.flow.blocks.iter().enumerate() {
            let bi = bi as u32;
            if let BlockKind::Dual { right } = b.kind {
                if !first {
                    y = Pt(y.0 + b.members.first().map_or(0, |m| m.space.0));
                }
                let top = y;
                marks.push((
                    Cursor {
                        block: bi,
                        member: 0,
                        offset: 0,
                    },
                    top,
                ));
                for range in [0..right, right..b.members.len()] {
                    let mut cy = top;
                    for (k, m) in b.members[range].iter().enumerate() {
                        if k > 0 {
                            cy = Pt(cy.0 + m.space.0);
                        }
                        for l in &m.lines {
                            lines.push(self.place(
                                &m.text,
                                l,
                                &m.geom,
                                cy,
                                LineKind::Content(m.kind),
                                m.source,
                            ));
                            cy = Pt(cy.0 + lh.0);
                        }
                    }
                }
                y = Pt(top.0 + lh.0 * b.dual_rows(self.format) as i32);
                first = false;
                continue;
            }
            for (mi, m) in b.members.iter().enumerate() {
                if !first {
                    y = Pt(y.0 + m.space.0);
                }
                first = false;
                for l in &m.lines {
                    marks.push((
                        Cursor {
                            block: bi,
                            member: mi as u32,
                            offset: l.start,
                        },
                        y,
                    ));
                    lines.push(self.place(
                        &m.text,
                        l,
                        &m.geom,
                        y,
                        LineKind::Content(m.kind),
                        m.source,
                    ));
                    if l.first
                        && let Some(n) = &m.scene_number
                    {
                        let baseline = self.baseline_y(y);
                        for x in [
                            self.format.scene_number_left,
                            self.format.scene_number_right,
                        ] {
                            lines.push(self.plain_line(n, x, baseline, LineKind::SceneNumber));
                        }
                    }
                    y = Pt(y.0 + lh.0);
                }
            }
        }
        // A page starts in the line holding its cursor: the last line that
        // starts at or before it.
        let page_tops = starts
            .iter()
            .map(|c| {
                let k = marks.partition_point(|(m, _)| m <= c);
                marks.get(k.saturating_sub(1)).map_or(Pt::ZERO, |(_, t)| *t)
            })
            .collect();
        crate::Draft {
            lines,
            page_tops,
            height: y,
        }
    }
}

/// The block a candidate's foot insert belongs to: the block of its last
/// kept row.
fn cand_block(c: &Cand, rows: &[Row]) -> u32 {
    rows.get(c.rows.saturating_sub(1))
        .map_or(c.next.block, |r| r.block)
}

/// A line of plain text, such as a page number.
pub(crate) fn plain_line(s: &str, x: Pt, y: Pt, kind: LineKind) -> PlacedLine {
    let t = text::build_plain(s, false, Look::default());
    let mut text = String::new();
    let mut clusters = Vec::new();
    for c in &t.cells {
        if c.kind == text::CellKind::Newline {
            continue;
        }
        text.push_str(t.cell_text(c));
        clusters.push(Cluster {
            end: text.len() as u32,
            cells: c.width,
            look: c.look,
        });
    }
    PlacedLine {
        x,
        y,
        kind,
        source: None,
        text,
        clusters,
    }
}
