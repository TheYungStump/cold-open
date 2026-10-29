//! Script diff (docs/specs/diff.md): two drafts in their comparison form,
//! compared scene by scene with moves found, then block by block, then
//! word by word.
//!
//! The caller puts each draft in its comparison form (§2) and gives each
//! element the id of the block it came from. Everything here depends only
//! on the two drafts: no hash-map order, no clock, no locale.

mod summary;
mod units;

use std::collections::{BTreeMap, BTreeSet};

use coldopen_screenplay::words;
use coldopen_screenplay::{Element, ElementKind, RichText, Run, TitleLine, TitlePage};
use serde::Serialize;

pub use summary::{kind_noun, summary_text};
use units::{Unit, units};

/// The least similarity at which two scenes, or two blocks, are taken for
/// one another (§3.2, §3.3).
pub const SIMILAR: f64 = 0.5;

/// A draft's title page, in whichever form it has.
#[derive(Clone, Copy, Debug)]
pub enum Title<'a> {
    Keys(&'a TitlePage),
    Lines(&'a [TitleLine]),
}

/// One draft, in its comparison form (§2).
#[derive(Clone, Copy, Debug)]
pub struct Draft<'a> {
    pub elements: &'a [Element],
    /// The id of the block each element came from, as the result names it.
    pub ids: &'a [String],
    pub title: Title<'a>,
    /// The kinds this draft's page format prints in capitals: they are
    /// compared as they print (§2).
    pub caps: &'a [ElementKind],
}

/// The result (§4.1), without the drafts' names, which the caller adds.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Diff {
    pub compared: Compared,
    pub summary: Summary,
    pub title_page: Vec<TitleOp>,
    pub scenes: Vec<SceneOp>,
    /// The tracked edits that bring A to B (§6), in the order to apply
    /// them. Not part of the JSON.
    #[serde(skip)]
    pub edits: Vec<Edit>,
}

/// One edit that brings A towards B (§6), on element indices: A's for
/// what A has, B's for what is new. Restyled blocks, changed attributes
/// and the title page have no edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// A's scene that starts at `unit` goes before A's scene that starts
    /// at `before`, or to the end.
    Move {
        unit: usize,
        before: Option<usize>,
        /// The unit's last element, so the whole unit moves: sections and
        /// headings without a setting are part of it (§3.1), and what a
        /// comparison left out between it and the next unit goes with it.
        last: usize,
    },
    Delete {
        a: usize,
    },
    /// A's block takes B's block's kind.
    Retype {
        a: usize,
        b: usize,
    },
    /// A's block takes B's block's text.
    Retext {
        a: usize,
        b: usize,
    },
    /// B's block goes before A's block `before`, or at the end.
    Insert {
        b: usize,
        before: Option<usize>,
    },
}

/// A unit's blocks lined up, in B's order (§3.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    /// Aligned or paired, with what differs.
    Both(usize, usize, Option<BlockOpKind>),
    Deleted(usize),
    Added(usize),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Compared {
    /// Sections, synopses, notes and boneyard were compared (§2).
    pub transparent: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub scenes: SceneCounts,
    pub blocks: BlockCounts,
    pub title_page: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SceneCounts {
    pub same: usize,
    pub changed: usize,
    pub moved: usize,
    pub added: usize,
    pub deleted: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BlockCounts {
    pub changed: usize,
    pub added: usize,
    pub deleted: usize,
    pub retyped: usize,
    pub restyled: usize,
    pub attributes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneOpKind {
    Changed,
    Moved,
    Added,
    Deleted,
}

/// A scene, or the opening before the first scene, as one draft has it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UnitRef {
    /// Counted from 1; the opening is 0.
    pub index: usize,
    /// The id of its first block: the heading, for a scene.
    pub id: String,
    /// The heading's text; empty for the opening.
    pub heading: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SceneOp {
    pub op: SceneOpKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<UnitRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<UnitRef>,
    /// For a move: B's index of the scene it now follows, or null at the
    /// start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<Option<usize>>,
    pub blocks: Vec<BlockOp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockOpKind {
    Changed,
    Added,
    Deleted,
    Retyped,
    Restyled,
    Attributes,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BlockRef {
    pub id: String,
    pub kind: ElementKind,
    /// Its index among the draft's elements, in comparison form.
    #[serde(skip)]
    pub at: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BlockOp {
    pub op: BlockOpKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<BlockRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<BlockRef>,
    /// For a changed block: its words, as runs of one op.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WordRun>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WordOp {
    Same,
    Del,
    Ins,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WordRun {
    pub op: WordOp,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleOpKind {
    Changed,
    Added,
    Deleted,
}

/// A difference in the title page (§3.5): a key, or a positioned line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TitleOp {
    pub op: TitleOpKind,
    /// The key; absent for a positioned line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// For a positioned line: its index in B, or in A when deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Vec<String>>,
}

/// Whether `kind` is compared only when both drafts can hold it (§2).
pub fn left_out(kind: ElementKind, transparent: bool) -> bool {
    !transparent
        && matches!(
            kind,
            ElementKind::Section
                | ElementKind::Synopsis
                | ElementKind::Note
                | ElementKind::Boneyard
        )
}

/// An element as compared: hidden runs left out unless transparent
/// elements are compared.
fn shown(e: &Element, transparent: bool) -> Element {
    if transparent || !e.text.runs().iter().any(Run::is_hidden) {
        return e.clone();
    }
    let mut e = e.clone();
    e.text = RichText::new(
        e.text
            .runs()
            .iter()
            .filter(|r| !r.is_hidden())
            .cloned()
            .collect(),
    );
    e
}

/// A character as compared: curly quotes and apostrophes as straight ones
/// (§2). Converters change them, and they never change what a script says.
fn fold_char(c: char) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' => '"',
        c => c,
    }
}

/// Text as compared: quotes folded, runs of spaces as one, and in
/// capitals when it prints so.
fn fold(text: &str, caps: bool) -> String {
    fold_after(text, caps, &mut false)
}

/// [`fold`] for one run of a line, where `space` says whether what printed
/// before it ended in a space, and is left saying so after it. A run of
/// spaces is one however runs split it: a style or a note inside it does
/// not print a second space (found by a local run of the `diff` check,
/// 2026-10-26).
fn fold_after(text: &str, caps: bool, space: &mut bool) -> String {
    let mut t = String::with_capacity(text.len());
    for c in text.chars().map(fold_char) {
        let is_space = matches!(c, ' ' | '\t' | '\u{A0}');
        if is_space && *space {
            continue;
        }
        *space = is_space;
        t.push(if is_space { ' ' } else { c });
    }
    if caps { t.to_uppercase() } else { t }
}

/// A character name without its CONT'D marker, when it has one (§2).
/// Layout prints the marker where a speaker continues, and drafts differ in
/// whether they type it.
fn without_contd(text: &str) -> Option<String> {
    let s = coldopen_screenplay::names::speaker(text)?;
    let contd = |e: &str| {
        let e = e.replace('\u{2019}', "'");
        e.contains("CONT'D") || e.contains("CONTD")
    };
    if !s.extensions.iter().any(|e| contd(e)) {
        return None;
    }
    let mut out = s.name;
    for e in s.extensions.iter().filter(|e| !contd(e)) {
        out.push_str(" (");
        out.push_str(e);
        out.push(')');
    }
    Some(out)
}

/// An element as it prints, for comparing (§2): each run's text folded.
fn printed(e: &Element, caps: &[ElementKind]) -> Element {
    let up = caps.contains(&e.kind);
    let mut e = e.clone();
    if e.kind == ElementKind::Character
        && let Some(name) = without_contd(&e.text.plain_text())
    {
        let style = e.text.style_at(0);
        e.text = RichText::new(vec![Run::styled(fold(&name, up), style)]);
        return e;
    }
    let mut space = false;
    e.text = RichText::new(
        e.text
            .runs()
            .iter()
            .map(|r| match r {
                Run::Text { text, style, marks } => Run::Text {
                    text: fold_after(text, up, &mut space),
                    style: *style,
                    marks: *marks,
                },
                r => r.clone(),
            })
            .collect(),
    );
    e
}

/// Equal as the diff sees blocks: kind, text with its styles, and the
/// printing attributes. Spacing is layout, not script (§2).
fn same_block(a: &Element, b: &Element) -> bool {
    a.kind == b.kind
        && a.text == b.text
        && a.scene_number == b.scene_number
        && a.dual == b.dual
        && a.centered == b.centered
        && a.depth == b.depth
}

/// The words of a text, lowercased, without spaces and punctuation.
fn word_list(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    words::split(&chars)
        .into_iter()
        .filter(|r| {
            chars[r.start..r.end]
                .iter()
                .all(|&c| words::is_word_char(c))
        })
        .map(|r| chars[r].iter().collect::<String>().to_lowercase())
        .collect()
}

/// What a similarity is measured on: three-word sequences, or words when
/// there are fewer than three (§3.2).
fn shingles(text: &str) -> BTreeSet<String> {
    let w = word_list(text);
    if w.len() < 3 {
        return w.into_iter().collect();
    }
    w.windows(3).map(|s| s.join(" ")).collect()
}

/// The Jaccard similarity of two shingle sets; two empty sets are equal.
fn similarity(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let both = a.intersection(b).count();
    let either = a.len() + b.len() - both;
    both as f64 / either as f64
}

/// The similarity of two blocks' words (§3.3): twice the longest common
/// run of words over both lengths, or, past the pair bound, the words they
/// share however ordered. Three-word sequences are too coarse for a short
/// speech, where one changed word leaves a third of them in common.
fn block_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let common = match words::common(a, b) {
        Some(pairs) => pairs.len(),
        None => {
            let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
            for w in a {
                counts.entry(w).or_default().0 += 1;
            }
            for w in b {
                counts.entry(w).or_default().1 += 1;
            }
            counts.values().map(|&(x, y)| x.min(y)).sum()
        }
    };
    2.0 * common as f64 / (a.len() + b.len()) as f64
}

/// Pairs items of two lists greedily by score (§3.2): highest first, then
/// the nearer positions, then A's position, then B's. `score` gives `None`
/// for a pair that may not be made. With `ordered`, a pair that would cross
/// one already made is not made: blocks in a gap pair in order, since a
/// block cannot move within a scene (§3.3; found by a local run of the
/// `diff` check, 2026-10-26).
fn greedy(
    n: usize,
    m: usize,
    ordered: bool,
    score: impl Fn(usize, usize) -> Option<f64>,
) -> Vec<(usize, usize)> {
    let mut candidates: Vec<(f64, usize, usize, usize)> = Vec::new();
    for i in 0..n {
        for j in 0..m {
            if let Some(s) = score(i, j) {
                candidates.push((s, i.abs_diff(j), i, j));
            }
        }
    }
    candidates.sort_by(|x, y| {
        y.0.total_cmp(&x.0)
            .then(x.1.cmp(&y.1))
            .then(x.2.cmp(&y.2))
            .then(x.3.cmp(&y.3))
    });
    let (mut used_a, mut used_b) = (vec![false; n], vec![false; m]);
    let mut out = Vec::new();
    for (_, _, i, j) in candidates {
        let crosses = |&(x, y): &(usize, usize)| (x < i) != (y < j);
        if !used_a[i] && !used_b[j] && !(ordered && out.iter().any(crosses)) {
            used_a[i] = true;
            used_b[j] = true;
            out.push((i, j));
        }
    }
    out.sort_unstable();
    out
}

/// The pairs, by A's order, that lie in the longest run increasing in B
/// too (§3.2 step 4). Ties go to the pairs that come first in A.
fn in_order(pairs: &[(usize, usize)]) -> Vec<bool> {
    let n = pairs.len();
    let mut len = vec![1usize; n];
    let mut prev = vec![usize::MAX; n];
    for i in 0..n {
        for j in 0..i {
            if pairs[j].1 < pairs[i].1 && len[j] + 1 > len[i] {
                len[i] = len[j] + 1;
                prev[i] = j;
            }
        }
    }
    let mut keep = vec![false; n];
    let Some(best) = len.iter().copied().max() else {
        return keep;
    };
    let mut at = len.iter().position(|&l| l == best).unwrap_or(usize::MAX);
    while at != usize::MAX {
        keep[at] = true;
        at = prev[at];
    }
    keep
}

/// Compares two drafts (§3). `transparent` says whether sections,
/// synopses, notes and boneyard are compared: only when neither draft is
/// .fdx (§2).
pub fn compare(a: &Draft<'_>, b: &Draft<'_>, transparent: bool) -> Diff {
    let (oa, ob): (Vec<Element>, Vec<Element>) = (
        a.elements.iter().map(|e| shown(e, transparent)).collect(),
        b.elements.iter().map(|e| shown(e, transparent)).collect(),
    );
    let ea: Vec<Element> = oa.iter().map(|e| printed(e, a.caps)).collect();
    let eb: Vec<Element> = ob.iter().map(|e| printed(e, b.caps)).collect();
    let ua = units(&ea, transparent);
    let ub = units(&eb, transparent);
    let pairs = pair_units(&ea, &ua, &eb, &ub);
    let keep = in_order(&pairs);

    let mut diff = Diff {
        compared: Compared { transparent },
        ..Diff::default()
    };
    let unit_ref = |d: &Draft<'_>, els: &[Element], u: &Unit| UnitRef {
        index: u.index,
        id: u
            .blocks
            .first()
            .map(|&i| d.ids.get(i).cloned().unwrap_or_default())
            .unwrap_or_default(),
        heading: if u.index == 0 {
            String::new()
        } else {
            u.blocks
                .first()
                .map(|&i| els[i].text.plain_text())
                .unwrap_or_default()
        },
    };

    // Each B unit's partner and whether it is in order.
    let mut partner_of_b: BTreeMap<usize, (usize, bool)> = BTreeMap::new();
    for (k, &(i, j)) in pairs.iter().enumerate() {
        partner_of_b.insert(j, (i, keep[k]));
    }
    let paired_a: BTreeSet<usize> = pairs.iter().map(|p| p.0).collect();
    // Deleted A units go after the B partner of the nearest in-order unit
    // before them in A, or first (§4.1).
    let mut deleted_after: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
    for i in 0..ua.len() {
        if paired_a.contains(&i) {
            continue;
        }
        let anchor = pairs
            .iter()
            .zip(&keep)
            .filter(|((pa, _), k)| **k && *pa < i)
            .map(|((_, pb), _)| *pb)
            .next_back();
        deleted_after.entry(anchor).or_default().push(i);
    }
    let emit_deleted = |anchor: Option<usize>, diff: &mut Diff| {
        for &i in deleted_after.get(&anchor).map_or(&[][..], Vec::as_slice) {
            let blocks: Vec<BlockOp> = ua[i]
                .blocks
                .iter()
                .map(|&k| BlockOp {
                    op: BlockOpKind::Deleted,
                    a: Some(block_ref(a, &ea, k)),
                    b: None,
                    words: Vec::new(),
                })
                .collect();
            diff.summary.scenes.deleted += 1;
            diff.summary.blocks.deleted += blocks.len();
            diff.scenes.push(SceneOp {
                op: SceneOpKind::Deleted,
                a: Some(unit_ref(a, &oa, &ua[i])),
                b: None,
                after: None,
                blocks,
            });
        }
    };
    emit_deleted(None, &mut diff);
    let mut rows: BTreeMap<usize, Vec<Row>> = BTreeMap::new();
    for (j, u) in ub.iter().enumerate() {
        match partner_of_b.get(&j) {
            None => {
                let blocks: Vec<BlockOp> = u
                    .blocks
                    .iter()
                    .map(|&k| BlockOp {
                        op: BlockOpKind::Added,
                        a: None,
                        b: Some(block_ref(b, &eb, k)),
                        words: Vec::new(),
                    })
                    .collect();
                diff.summary.scenes.added += 1;
                diff.summary.blocks.added += blocks.len();
                diff.scenes.push(SceneOp {
                    op: SceneOpKind::Added,
                    a: None,
                    b: Some(unit_ref(b, &ob, u)),
                    after: None,
                    blocks,
                });
            }
            Some(&(i, ordered)) => {
                let (blocks, unit_rows) =
                    compare_blocks((a, &ea, &oa), &ua[i].blocks, (b, &eb, &ob), &u.blocks);
                rows.insert(j, unit_rows);
                for op in &blocks {
                    count_block(&mut diff.summary.blocks, op);
                }
                let op = if !ordered {
                    diff.summary.scenes.moved += 1;
                    Some(SceneOpKind::Moved)
                } else if blocks.is_empty() {
                    diff.summary.scenes.same += 1;
                    None
                } else {
                    diff.summary.scenes.changed += 1;
                    Some(SceneOpKind::Changed)
                };
                if let Some(op) = op {
                    diff.scenes.push(SceneOp {
                        op,
                        a: Some(unit_ref(a, &oa, &ua[i])),
                        b: Some(unit_ref(b, &ob, u)),
                        after: (op == SceneOpKind::Moved)
                            .then(|| j.checked_sub(1).map(|p| ub[p].index).filter(|&p| p > 0)),
                        blocks,
                    });
                }
                if ordered {
                    emit_deleted(Some(j), &mut diff);
                }
            }
        }
    }
    diff.title_page = compare_title(a.title, b.title);
    diff.summary.title_page = diff.title_page.len();
    let retyped = |i: usize, kind: ElementKind| {
        let mut e = oa[i].clone();
        e.kind = kind;
        printed(&e, a.caps)
    };
    diff.edits = plan(&ea, &eb, &retyped, &ua, &ub, &partner_of_b, &rows);
    diff
}

/// The tracked edits that bring A to B (§6). Moves come first, from B's
/// last scene back, so each scene goes before one already in its place.
/// Then each scene in B's order: deletions, new kinds and text, and new
/// blocks before the next block A keeps, or before the next scene.
/// `retyped` is how an A block prints once it has a new kind.
fn plan(
    ea: &[Element],
    eb: &[Element],
    retyped: &dyn Fn(usize, ElementKind) -> Element,
    ua: &[Unit],
    ub: &[Unit],
    partner_of_b: &BTreeMap<usize, (usize, bool)>,
    rows: &BTreeMap<usize, Vec<Row>>,
) -> Vec<Edit> {
    let first = |i: usize| ua[i].blocks[0];
    let last = |i: usize| ua.get(i + 1).map_or(ea.len(), |u| u.blocks[0]) - 1;
    let next_scene =
        |j: usize| (j + 1..ub.len()).find_map(|k| partner_of_b.get(&k).map(|&(i, _)| first(i)));
    let mut edits = Vec::new();
    let mut next = None;
    for j in (0..ub.len()).rev() {
        if let Some(&(i, ordered)) = partner_of_b.get(&j) {
            if !ordered {
                edits.push(Edit::Move {
                    unit: first(i),
                    before: next,
                    last: last(i),
                });
            }
            next = Some(first(i));
        }
    }
    let paired_a: BTreeSet<usize> = partner_of_b.values().map(|&(i, _)| i).collect();
    for (i, u) in ua.iter().enumerate() {
        if !paired_a.contains(&i) {
            edits.extend(u.blocks.iter().map(|&a| Edit::Delete { a }));
        }
    }
    for (j, u) in ub.iter().enumerate() {
        let Some(unit_rows) = rows.get(&j) else {
            let before = next_scene(j);
            edits.extend(u.blocks.iter().map(|&b| Edit::Insert { b, before }));
            continue;
        };
        for (r, row) in unit_rows.iter().enumerate() {
            match *row {
                Row::Deleted(a) => edits.push(Edit::Delete { a }),
                Row::Added(b) => {
                    let before = unit_rows[r + 1..]
                        .iter()
                        .find_map(|row| match *row {
                            Row::Both(a, _, _) | Row::Deleted(a) => Some(a),
                            Row::Added(_) => None,
                        })
                        .or_else(|| next_scene(j));
                    edits.push(Edit::Insert { b, before });
                }
                Row::Both(a, b, Some(BlockOpKind::Retyped)) => {
                    edits.push(Edit::Retype { a, b });
                    // Text that prints the same in the old kind may not in
                    // the new one: a lowercase name prints in capitals as a
                    // character, and as typed in action (found by a local
                    // run of the `diff` check, 2026-10-26).
                    if retyped(a, eb[b].kind).text != eb[b].text {
                        edits.push(Edit::Retext { a, b });
                    }
                }
                Row::Both(a, b, Some(BlockOpKind::Changed)) => {
                    if ea[a].kind != eb[b].kind {
                        edits.push(Edit::Retype { a, b });
                    }
                    edits.push(Edit::Retext { a, b });
                }
                Row::Both(..) => {}
            }
        }
    }
    edits
}

fn block_ref(d: &Draft<'_>, els: &[Element], k: usize) -> BlockRef {
    BlockRef {
        id: d.ids.get(k).cloned().unwrap_or_default(),
        kind: els[k].kind,
        at: k,
    }
}

fn count_block(c: &mut BlockCounts, op: &BlockOp) {
    match op.op {
        BlockOpKind::Changed => {
            c.changed += 1;
            if op.a.as_ref().map(|r| r.kind) != op.b.as_ref().map(|r| r.kind) {
                c.retyped += 1;
            }
        }
        BlockOpKind::Added => c.added += 1,
        BlockOpKind::Deleted => c.deleted += 1,
        BlockOpKind::Retyped => c.retyped += 1,
        BlockOpKind::Restyled => c.restyled += 1,
        BlockOpKind::Attributes => c.attributes += 1,
    }
}

/// A key for a unit's whole content, for pairing equal units.
fn content_key(els: &[Element], u: &Unit) -> Vec<String> {
    u.blocks
        .iter()
        .map(|&i| {
            let e = &els[i];
            format!(
                "{}|{}|{}|{}|{}|{}|{:?}",
                e.kind.name(),
                e.scene_number.as_deref().unwrap_or(""),
                e.dual,
                e.centered,
                e.depth,
                e.text.plain_text(),
                e.text.runs()
            )
        })
        .collect()
}

/// Pairs A's units with B's (§3.2), sorted by A's index.
fn pair_units(ea: &[Element], ua: &[Unit], eb: &[Element], ub: &[Unit]) -> Vec<(usize, usize)> {
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let (mut used_a, mut used_b) = (vec![false; ua.len()], vec![false; ub.len()]);
    // 1. Equal content: the k-th of each content in A with the k-th in B.
    let mut by_content: BTreeMap<Vec<String>, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for (i, u) in ua.iter().enumerate() {
        by_content.entry(content_key(ea, u)).or_default().0.push(i);
    }
    for (j, u) in ub.iter().enumerate() {
        by_content.entry(content_key(eb, u)).or_default().1.push(j);
    }
    for (xs, ys) in by_content.values() {
        for (&i, &j) in xs.iter().zip(ys) {
            // The opening pairs only with the opening.
            if (ua[i].index == 0) != (ub[j].index == 0) {
                continue;
            }
            pairs.push((i, j));
            used_a[i] = true;
            used_b[j] = true;
        }
    }
    // 2. Similar content, among the rest.
    let rest_a: Vec<usize> = (0..ua.len()).filter(|&i| !used_a[i]).collect();
    let rest_b: Vec<usize> = (0..ub.len()).filter(|&j| !used_b[j]).collect();
    let text = |els: &[Element], u: &Unit| -> String {
        u.blocks
            .iter()
            .map(|&i| els[i].text.plain_text())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let sa: Vec<BTreeSet<String>> = rest_a
        .iter()
        .map(|&i| shingles(&text(ea, &ua[i])))
        .collect();
    let sb: Vec<BTreeSet<String>> = rest_b
        .iter()
        .map(|&j| shingles(&text(eb, &ub[j])))
        .collect();
    for (x, y) in greedy(rest_a.len(), rest_b.len(), false, |x, y| {
        let (i, j) = (rest_a[x], rest_b[y]);
        if (ua[i].index == 0) != (ub[j].index == 0) {
            return None;
        }
        let s = similarity(&sa[x], &sb[y]);
        (s >= SIMILAR).then_some(s)
    }) {
        pairs.push((rest_a[x], rest_b[y]));
    }
    pairs.sort_unstable();
    pairs
}

/// The block ops of a pair of units (§3.3), and their rows, in B's order.
fn compare_blocks(
    sa: Side<'_, '_>,
    xa: &[usize],
    sb: Side<'_, '_>,
    xb: &[usize],
) -> (Vec<BlockOp>, Vec<Row>) {
    let (a, ea, _) = sa;
    let (b, eb, _) = sb;
    let key = |els: &[Element], i: usize| (els[i].kind, els[i].text.plain_text());
    let ka: Vec<(ElementKind, String)> = xa.iter().map(|&i| key(ea, i)).collect();
    let kb: Vec<(ElementKind, String)> = xb.iter().map(|&i| key(eb, i)).collect();
    let aligned = align(&ka, &kb);
    let mut out = Vec::new();
    let mut rows = Vec::new();
    let (mut pa, mut pb) = (0, 0);
    let end = std::iter::once((xa.len(), xb.len()));
    for (i, j) in aligned.into_iter().chain(end) {
        gap(sa, &xa[pa..i], sb, &xb[pb..j], &mut out, &mut rows);
        if i < xa.len() {
            let (ka, kb) = (xa[i], xb[j]);
            let (x, y) = (&ea[ka], &eb[kb]);
            let op = if x.text != y.text {
                Some(BlockOpKind::Restyled)
            } else if !same_block(x, y) {
                Some(BlockOpKind::Attributes)
            } else {
                None
            };
            if let Some(op) = op {
                out.push(BlockOp {
                    op,
                    a: Some(block_ref(a, ea, ka)),
                    b: Some(block_ref(b, eb, kb)),
                    words: Vec::new(),
                });
            }
            rows.push(Row::Both(ka, kb, op));
            (pa, pb) = (i + 1, j + 1);
        }
    }
    (out, rows)
}

/// The longest common subsequence of two lists, or, past the pair bound,
/// only their common start and end (§3.3 step 3).
fn align<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let start = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let end = a[start..]
        .iter()
        .rev()
        .zip(b[start..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (ma, mb) = (&a[start..a.len() - end], &b[start..b.len() - end]);
    let middle = words::common(ma, mb).unwrap_or_default();
    (0..start)
        .map(|k| (k, k))
        .chain(middle.into_iter().map(|(i, j)| (i + start, j + start)))
        .chain((0..end).map(|k| (a.len() - end + k, b.len() - end + k)))
        .collect()
}

/// The unaligned blocks between two aligned ones (§3.3 step 2): pairs by
/// similarity, the same kind first, and the rest added or deleted. Deleted
/// blocks come before added ones, each in its draft's order.
fn gap(
    sa: Side<'_, '_>,
    ga: &[usize],
    sb: Side<'_, '_>,
    gb: &[usize],
    out: &mut Vec<BlockOp>,
    rows: &mut Vec<Row>,
) {
    let (a, ea, oa) = sa;
    let (b, eb, ob) = sb;
    if ga.is_empty() && gb.is_empty() {
        return;
    }
    let wa: Vec<Vec<String>> = ga
        .iter()
        .map(|&i| word_list(&ea[i].text.plain_text()))
        .collect();
    let wb: Vec<Vec<String>> = gb
        .iter()
        .map(|&j| word_list(&eb[j].text.plain_text()))
        .collect();
    let pairs = greedy(ga.len(), gb.len(), true, |x, y| {
        let (e, f) = (&ea[ga[x]], &eb[gb[y]]);
        // A page break pairs only with a page break, and a note or
        // boneyard block only with one of those: nothing else can become
        // one without losing its text or styles (2026-10-26).
        if (e.kind == ElementKind::PageBreak) != (f.kind == ElementKind::PageBreak)
            || e.kind.is_raw_text() != f.kind.is_raw_text()
        {
            return None;
        }
        let s = if e.text.plain_text() == f.text.plain_text() {
            1.0
        } else if wa[x].is_empty() && wb[y].is_empty() {
            // No words on either side, and different text: nothing to
            // say they are the same block.
            0.0
        } else {
            block_similarity(&wa[x], &wb[y])
        };
        // The same kind wins a tie.
        (s >= SIMILAR).then_some(s + if e.kind == f.kind { 1e-9 } else { 0.0 })
    });
    let paired_a: BTreeSet<usize> = pairs.iter().map(|p| p.0).collect();
    let paired_b: BTreeMap<usize, usize> = pairs.iter().map(|&(x, y)| (y, x)).collect();
    for (x, &i) in ga.iter().enumerate() {
        if !paired_a.contains(&x) {
            out.push(BlockOp {
                op: BlockOpKind::Deleted,
                a: Some(block_ref(a, ea, i)),
                b: None,
                words: Vec::new(),
            });
            rows.push(Row::Deleted(i));
        }
    }
    for (y, &j) in gb.iter().enumerate() {
        match paired_b.get(&y) {
            None => {
                out.push(BlockOp {
                    op: BlockOpKind::Added,
                    a: None,
                    b: Some(block_ref(b, eb, j)),
                    words: Vec::new(),
                });
                rows.push(Row::Added(j));
            }
            Some(&x) => {
                let i = ga[x];
                let (e, f) = (&ea[i], &eb[j]);
                let (pe, pf) = (e.text.plain_text(), f.text.plain_text());
                let (op, words) = if pe == pf {
                    (BlockOpKind::Retyped, Vec::new())
                } else {
                    let caps = |d: &Draft<'_>, e: &Element| d.caps.contains(&e.kind);
                    (
                        BlockOpKind::Changed,
                        word_runs(
                            (&oa[i].text.plain_text(), caps(a, &oa[i])),
                            (&ob[j].text.plain_text(), caps(b, &ob[j])),
                        ),
                    )
                };
                out.push(BlockOp {
                    op,
                    a: Some(block_ref(a, ea, i)),
                    b: Some(block_ref(b, eb, j)),
                    words,
                });
                rows.push(Row::Both(i, j, Some(op)));
            }
        }
    }
}

/// A draft's side of a comparison: the draft, its elements as compared,
/// and as they are.
type Side<'d, 'e> = (&'d Draft<'d>, &'e [Element], &'e [Element]);

/// Two texts compared word by word (§3.4), as runs of one op. Each text
/// comes with whether it prints in capitals; words are compared as they
/// print, and given as they are.
fn word_runs(a: (&str, bool), b: (&str, bool)) -> Vec<WordRun> {
    let words_of = |t: &str| -> Vec<String> {
        let c: Vec<char> = t.chars().collect();
        words::split(&c)
            .into_iter()
            .map(|r| c[r].iter().collect())
            .collect()
    };
    let (wa, wb) = (words_of(a.0), words_of(b.0));
    let ka: Vec<String> = wa.iter().map(|w| fold(w, a.1)).collect();
    let kb: Vec<String> = wb.iter().map(|w| fold(w, b.1)).collect();
    let aligned = align(&ka, &kb);
    let mut runs: Vec<WordRun> = Vec::new();
    let mut push = |op: WordOp, text: &str| match runs.last_mut() {
        Some(r) if r.op == op => r.text.push_str(text),
        _ => runs.push(WordRun {
            op,
            text: text.to_string(),
        }),
    };
    let (mut pa, mut pb) = (0, 0);
    let end = std::iter::once((wa.len(), wb.len()));
    for (i, j) in aligned.into_iter().chain(end) {
        for w in &wa[pa..i] {
            push(WordOp::Del, w);
        }
        for w in &wb[pb..j] {
            push(WordOp::Ins, w);
        }
        if i < wa.len() {
            push(WordOp::Same, &wb[j]);
            (pa, pb) = (i + 1, j + 1);
        }
    }
    runs
}

/// The title pages compared (§3.5). Keys against lines never reach here:
/// the caller turns lines into keys first.
fn compare_title(a: Title<'_>, b: Title<'_>) -> Vec<TitleOp> {
    match (a, b) {
        (Title::Lines(la), Title::Lines(lb)) => compare_lines(la, lb),
        (Title::Keys(ka), Title::Keys(kb)) => compare_keys(ka, kb),
        // Mixed forms never reach here from core, which turns lines into
        // keys first; the lines count as no keys.
        (Title::Keys(ka), Title::Lines(_)) => compare_keys(ka, &TitlePage::default()),
        (Title::Lines(_), Title::Keys(kb)) => compare_keys(&TitlePage::default(), kb),
    }
}

fn compare_keys(a: &TitlePage, b: &TitlePage) -> Vec<TitleOp> {
    let values = |f: &coldopen_screenplay::TitleField| -> Vec<String> {
        f.values.iter().map(RichText::plain_text).collect()
    };
    let folded = |f: &coldopen_screenplay::TitleField| -> Vec<String> {
        f.values
            .iter()
            .map(|v| fold(&v.plain_text(), false))
            .collect()
    };
    // A key may be there more than once: its k-th time in A pairs with its
    // k-th in B (found by the `diff` fuzz check, 2026-10-26).
    fn nth<'f>(
        fields: &'f [coldopen_screenplay::TitleField],
        key: &str,
        n: usize,
    ) -> Option<&'f coldopen_screenplay::TitleField> {
        fields
            .iter()
            .filter(|g| g.key.eq_ignore_ascii_case(key))
            .nth(n)
    }
    let occurrence = |fields: &[coldopen_screenplay::TitleField], i: usize| {
        fields[..i]
            .iter()
            .filter(|g| g.key.eq_ignore_ascii_case(&fields[i].key))
            .count()
    };
    let mut out = Vec::new();
    for (i, f) in b.fields.iter().enumerate() {
        match nth(&a.fields, &f.key, occurrence(&b.fields, i)) {
            None => out.push(TitleOp {
                op: TitleOpKind::Added,
                key: Some(f.key.clone()),
                line: None,
                a: None,
                b: Some(values(f)),
            }),
            Some(g) if folded(g) != folded(f) => out.push(TitleOp {
                op: TitleOpKind::Changed,
                key: Some(f.key.clone()),
                line: None,
                a: Some(values(g)),
                b: Some(values(f)),
            }),
            Some(_) => {}
        }
    }
    for (i, g) in a.fields.iter().enumerate() {
        if nth(&b.fields, &g.key, occurrence(&a.fields, i)).is_none() {
            out.push(TitleOp {
                op: TitleOpKind::Deleted,
                key: Some(g.key.clone()),
                line: None,
                a: Some(values(g)),
                b: None,
            });
        }
    }
    out
}

fn compare_lines(a: &[TitleLine], b: &[TitleLine]) -> Vec<TitleOp> {
    let key = |l: &TitleLine| (fold(&l.text.plain_text(), false), l.align, l.space_before);
    let ka: Vec<_> = a.iter().map(key).collect();
    let kb: Vec<_> = b.iter().map(key).collect();
    let aligned = align(&ka, &kb);
    let mut out = Vec::new();
    let (mut pa, mut pb) = (0, 0);
    let end = std::iter::once((a.len(), b.len()));
    for (i, j) in aligned.into_iter().chain(end) {
        let (ga, gb) = (pa..i, pb..j);
        let n = ga.len().min(gb.len());
        for k in 0..n {
            out.push(TitleOp {
                op: TitleOpKind::Changed,
                key: None,
                line: Some(pb + k),
                a: Some(vec![a[pa + k].text.plain_text()]),
                b: Some(vec![b[pb + k].text.plain_text()]),
            });
        }
        for k in ga.skip(n) {
            out.push(TitleOp {
                op: TitleOpKind::Deleted,
                key: None,
                line: Some(k),
                a: Some(vec![a[k].text.plain_text()]),
                b: None,
            });
        }
        for k in gb.skip(n) {
            out.push(TitleOp {
                op: TitleOpKind::Added,
                key: None,
                line: Some(k),
                a: None,
                b: Some(vec![b[k].text.plain_text()]),
            });
        }
        if i < a.len() {
            (pa, pb) = (i + 1, j + 1);
        }
    }
    out
}
