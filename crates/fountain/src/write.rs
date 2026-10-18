//! The Fountain writer (spec §9).
//!
//! The script is first brought into Fountain normal form (spec §10), then
//! written element by element. Forcing markers and escapes are added only
//! where the reader would otherwise misread a line. As a final check the
//! output is parsed again; any difference not already reported becomes a
//! loss, so nothing is ever dropped silently.

use coldopen_screenplay::{Element, ElementKind, Run, Script, Style, TitlePage};

use crate::classify::{
    default_gap, is_centered, is_character_shape, is_forced_heading, is_page_break,
    is_scene_heading_prefix, is_strict_scene_heading_prefix, is_transition_shape,
    split_scene_number, strip_dual,
};
use crate::norm::{RLine, normalize_with_map, split, visible};
use crate::{inline, line, scan, title};

/// Something in the script that Fountain cannot express exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loss {
    /// Index of the element in the input script, or `None` for the title page.
    pub element: Option<usize>,
    /// Stable machine-readable code, such as `shot_element`.
    pub code: &'static str,
    pub message: String,
}

/// The result of [`write`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    pub text: String,
    /// Empty when the text reads back as the script's normal form.
    pub losses: Vec<Loss>,
}

/// Writes a script as canonical Fountain.
pub fn write(script: &Script) -> Written {
    let (norm, map) = normalize_with_map(script);
    let mut w = Writer {
        out: Vec::new(),
        losses: Vec::new(),
        map: &map,
    };
    let has_title = !norm.title_page.is_empty();
    if has_title {
        w.title_page(&norm.title_page);
    }
    w.body(&norm.elements, has_title);

    let mut text = w.out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    let mut losses = w.losses;
    if losses.is_empty() {
        let back = crate::parse(&text).script;
        if back != norm {
            let at = back
                .elements
                .iter()
                .zip(&norm.elements)
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| back.elements.len().min(norm.elements.len()));
            let element = if back.title_page != norm.title_page {
                None
            } else {
                map.get(at).copied()
            };
            losses.push(Loss {
                element,
                code: "unexpressible",
                message: "this content does not read back the same way from Fountain".into(),
            });
        }
    }
    Written { text, losses }
}

// ---- Inline text ----

#[derive(Clone, Copy, PartialEq)]
enum Flag {
    Underline,
    Bold,
    Italic,
}

impl Flag {
    fn marker(self) -> &'static str {
        match self {
            Flag::Underline => "_",
            Flag::Bold => "**",
            Flag::Italic => "*",
        }
    }

    fn of(s: Style) -> Vec<Flag> {
        let mut v = Vec::new();
        if s.underline {
            v.push(Flag::Underline);
        }
        if s.bold {
            v.push(Flag::Bold);
        }
        if s.italic {
            v.push(Flag::Italic);
        }
        v
    }
}

enum Piece {
    /// A literal character and its index in the line's printed text.
    Lit(char, usize),
    Mark(&'static str),
    Raw(String),
}

fn first_char(p: &Piece) -> Option<char> {
    match p {
        Piece::Lit(c, _) => Some(*c),
        Piece::Mark(m) => m.chars().next(),
        Piece::Raw(r) => r.chars().next(),
    }
}

/// Note content as written inside `[[ ]]`: empty lines become two spaces so
/// they hold the note together.
fn note_body(n: &str) -> String {
    n.split('\n')
        .map(|l| if l.trim().is_empty() { "  " } else { l })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Moves whitespace at the edges of underlined spans out of the underline.
/// Underlined edge spaces cannot be written, so this is used only after the
/// exact attempts fail.
fn hoist_underline(line: &[Run]) -> RLine {
    let mut out: Vec<Run> = Vec::new();
    for r in line {
        match r {
            Run::Text { text, style } if style.underline => {
                let core = text.trim();
                if core.is_empty() {
                    out.push(Run::styled(
                        text.clone(),
                        Style {
                            underline: false,
                            ..*style
                        },
                    ));
                    continue;
                }
                let lead = &text[..text.len() - text.trim_start().len()];
                let trail = &text[text.trim_end().len()..];
                let plain_edge = Style {
                    underline: false,
                    ..*style
                };
                out.push(Run::styled(lead, plain_edge));
                out.push(Run::styled(core, *style));
                out.push(Run::styled(trail, plain_edge));
            }
            other => out.push(other.clone()),
        }
    }
    coldopen_screenplay::RichText::new(out).into_runs()
}

/// How emphasis markers are placed. Delimiter runs that touch can pair up
/// differently than intended, so the writer tries these in turn and keeps
/// the first that reads back exactly.
#[derive(Clone, Copy)]
enum Nesting {
    /// Keep styles open across runs; longest-lasting style outermost.
    Nested,
    /// Close everything at each style change; open underline, bold, italic.
    Flat,
    /// Close everything at each style change; open italic, bold, underline.
    FlatReversed,
    /// Like `Nested`, but the styles stay in longest-lasting order at every
    /// run: an open style that ends before one about to open is closed and
    /// reopened inside it (`_a_*_b_c*`). Which of these reads back depends
    /// on the characters next to the markers (spec §9).
    Ordered,
}

/// One way to place markers: the nesting, whether literal `*` and `_` are
/// escaped, and whether styles the next text drops are closed before a note
/// or boneyard instead of after it.
#[derive(Clone, Copy)]
struct Plan {
    nesting: Nesting,
    escape_delims: bool,
    close_before_hidden: bool,
}

/// The plans the writer tries, in order. Closing before a hidden span comes
/// last so lines that already read back keep their written form; it is
/// needed when the style change after a note would otherwise put two
/// delimiter runs together (`**bold[[n]]***it*`).
const PLANS: [Plan; 16] = {
    const fn p(nesting: Nesting, escape_delims: bool, close_before_hidden: bool) -> Plan {
        Plan {
            nesting,
            escape_delims,
            close_before_hidden,
        }
    }
    [
        p(Nesting::Nested, false, false),
        p(Nesting::Nested, true, false),
        p(Nesting::Flat, false, false),
        p(Nesting::Flat, true, false),
        p(Nesting::FlatReversed, false, false),
        p(Nesting::FlatReversed, true, false),
        p(Nesting::Nested, false, true),
        p(Nesting::Nested, true, true),
        p(Nesting::Flat, false, true),
        p(Nesting::Flat, true, true),
        p(Nesting::FlatReversed, false, true),
        p(Nesting::FlatReversed, true, true),
        p(Nesting::Ordered, false, false),
        p(Nesting::Ordered, true, false),
        p(Nesting::Ordered, false, true),
        p(Nesting::Ordered, true, true),
    ]
};

fn render(line: &[Run], plan: Plan, after: Option<char>, escape_at: &[usize]) -> String {
    let Plan {
        nesting,
        escape_delims,
        close_before_hidden,
    } = plan;
    let mut pieces: Vec<Piece> = Vec::new();
    let mut stack: Vec<Flag> = Vec::new();
    let mut idx = 0usize;
    // How many text runs from `r` on keep `f`, so longer-lived styles open
    // first (outermost) and inner ones can close without reopening.
    let lasts = |r: usize, f: Flag| -> usize {
        line[r..]
            .iter()
            .filter_map(|x| match x {
                Run::Text { style, .. } => Some(Flag::of(*style).contains(&f)),
                _ => None,
            })
            .take_while(|&has| has)
            .count()
    };
    // The order `want` should stand in from outermost, for the run at `r`:
    // longest-lasting first, open styles before new ones among equals.
    let ordered = |stack: &[Flag], want: &[Flag], r: usize| -> Vec<Flag> {
        let mut w = want.to_vec();
        w.sort_by_key(|f| {
            let open = stack.iter().position(|s| s == f).unwrap_or(usize::MAX);
            (std::cmp::Reverse(lasts(r, *f)), open)
        });
        w
    };
    // Which open styles must close before `want` applies at run `r`.
    let closing_from = |stack: &[Flag], want: &[Flag], r: usize| match nesting {
        Nesting::Nested => stack.iter().position(|f| !want.contains(f)),
        Nesting::Ordered => {
            let order = ordered(stack, want, r);
            stack
                .iter()
                .zip(&order)
                .position(|(s, o)| s != o)
                .or_else(|| (stack.len() > order.len()).then_some(order.len()))
        }
        Nesting::Flat | Nesting::FlatReversed => {
            let same = stack.len() == want.len() && stack.iter().all(|f| want.contains(f));
            if same { None } else { Some(0) }
        }
    };
    for (r, run) in line.iter().enumerate() {
        if close_before_hidden && matches!(run, Run::Note(_) | Run::Boneyard(_)) {
            let (next_r, next): (usize, Vec<Flag>) = line[r + 1..]
                .iter()
                .enumerate()
                .find_map(|(k, x)| match x {
                    Run::Text { style, .. } => Some((r + 1 + k, Flag::of(*style))),
                    _ => None,
                })
                .unwrap_or((line.len(), Vec::new()));
            if let Some(pos) = closing_from(&stack, &next, next_r) {
                for f in stack.drain(pos..).rev() {
                    pieces.push(Piece::Mark(f.marker()));
                }
            }
        }
        match run {
            Run::Note(n) => pieces.push(Piece::Raw(format!("[[{}]]", note_body(n)))),
            Run::Boneyard(b) => pieces.push(Piece::Raw(format!("/*{b}*/"))),
            Run::Text { text, style } => {
                let want = Flag::of(*style);
                if let Some(pos) = closing_from(&stack, &want, r) {
                    for f in stack.drain(pos..).rev() {
                        pieces.push(Piece::Mark(f.marker()));
                    }
                }
                let mut opening: Vec<Flag> =
                    want.into_iter().filter(|f| !stack.contains(f)).collect();
                match nesting {
                    // Stable sort keeps underline, bold, italic order among equals.
                    Nesting::Nested | Nesting::Ordered => {
                        opening.sort_by_key(|&f| std::cmp::Reverse(lasts(r, f)));
                    }
                    Nesting::Flat => {}
                    Nesting::FlatReversed => opening.reverse(),
                }
                for f in opening {
                    stack.push(f);
                    pieces.push(Piece::Mark(f.marker()));
                }
                for c in text.chars() {
                    pieces.push(Piece::Lit(c, idx));
                    idx += 1;
                }
            }
        }
    }
    for f in stack.into_iter().rev() {
        pieces.push(Piece::Mark(f.marker()));
    }

    let mut s = String::new();
    for k in 0..pieces.len() {
        match &pieces[k] {
            Piece::Lit(c, i) => {
                let next = pieces.get(k + 1).map_or(after, first_char);
                let c = *c;
                if escape_at.contains(i) && c.is_ascii_punctuation() {
                    s.push('\\');
                    s.push(c);
                    continue;
                }
                match c {
                    '*' | '_' if escape_delims => s.push('\\'),
                    '\\' if next.is_some_and(|n| n.is_ascii_punctuation()) => s.push('\\'),
                    '[' if next == Some('[') => s.push('\\'),
                    '/' if next == Some('*') => s.push('\\'),
                    _ => {}
                }
                s.push(c);
            }
            Piece::Mark(m) => s.push_str(m),
            Piece::Raw(r) => s.push_str(r),
        }
    }
    s
}

/// Reads one written line back into runs, as the parser would.
fn reparse_line(s: &str) -> Option<RLine> {
    let mut ignored = Vec::new();
    let scanned = scan::scan(&[s], 0, &mut ignored);
    if scanned.lines.len() != 1 {
        return None;
    }
    let runs = inline::parse_line(&scanned.lines[0]);
    Some(coldopen_screenplay::RichText::new(runs).into_runs())
}

/// Writes one line of runs. Returns the text and whether it reads back
/// exactly. `after` is the character that will follow the line in the
/// output; `escape_at` lists printed-text indices to escape.
fn write_inline(line: &[Run], after: Option<char>, escape_at: &[usize]) -> (String, bool) {
    let target = coldopen_screenplay::RichText::new(line.to_vec()).into_runs();
    // Fast path: most lines are plain text with nothing that needs escaping.
    if escape_at.is_empty()
        && let [Run::Text { text, style }] = &target[..]
        && style.is_plain()
        && !text.contains(['*', '_', '\\', '[', '/'])
    {
        return (text.clone(), true);
    }
    if target.is_empty() {
        return (String::new(), true);
    }
    for plan in PLANS {
        let s = render(&target, plan, after, escape_at);
        if reparse_line(&s).as_deref() == Some(&target[..]) {
            return (s, true);
        }
    }
    let hoisted = hoist_underline(&target);
    for plan in PLANS.iter().filter(|p| p.escape_delims) {
        let s = render(&hoisted, *plan, after, escape_at);
        if reparse_line(&s).as_deref() == Some(&hoisted[..]) {
            return (s, false);
        }
    }
    let plain: RLine = target
        .iter()
        .map(|r| match r {
            Run::Text { text, .. } => Run::plain(text.clone()),
            other => other.clone(),
        })
        .collect();
    let flat = Plan {
        nesting: Nesting::Flat,
        escape_delims: true,
        close_before_hidden: false,
    };
    (render(&plain, flat, after, escape_at), false)
}

/// The text the reader sees in a written line: hidden spans removed,
/// markers and escapes still present.
fn seen(s: &str) -> String {
    let mut ignored = Vec::new();
    let scanned = scan::scan(&[s], 0, &mut ignored);
    scanned
        .lines
        .iter()
        .map(|l| line::visible(l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_non_ws(line: &[Run]) -> Vec<usize> {
    visible(line)
        .chars()
        .position(|c| !c.is_whitespace())
        .into_iter()
        .collect()
}

/// True for lines the reader takes as a §6.1 marker line.
fn is_marker(t: &str) -> bool {
    is_page_break(t)
        || t.starts_with('#')
        || t.starts_with('=')
        || t.starts_with('~')
        || is_centered(t.trim_end())
}

/// True for lines that start with a forcing marker (§6.2 rules 1–4).
fn is_forced(t: &str) -> bool {
    t.starts_with('!') || t.starts_with('@') || is_forced_heading(t) || t.starts_with('>')
}

// ---- Elements ----

struct Writer<'a> {
    out: Vec<String>,
    losses: Vec<Loss>,
    map: &'a [usize],
}

/// Where an element sits, as far as the reader is concerned.
#[derive(Clone, Copy)]
struct Ctx {
    /// The reader starts a paragraph at this element.
    para_start: bool,
    /// A blank line (or the end) follows the element.
    blank_after: bool,
    /// The next line belongs to this element's dialogue group.
    group_follows: bool,
    /// That next line is a hold line (dialogue starting with an empty line),
    /// which the reader does not count as content.
    hold_follows: bool,
}

impl Writer<'_> {
    fn loss(&mut self, idx: Option<usize>, code: &'static str, message: impl Into<String>) {
        self.losses.push(Loss {
            element: idx.and_then(|i| self.map.get(i).copied()),
            code,
            message: message.into(),
        });
    }

    fn blank(&mut self, n: u32) {
        for _ in 0..n {
            self.out.push(String::new());
        }
    }

    fn inline(
        &mut self,
        idx: Option<usize>,
        l: &[Run],
        after: Option<char>,
        esc: &[usize],
    ) -> String {
        let (s, exact) = write_inline(l, after, esc);
        if !exact {
            self.loss(
                idx,
                "style",
                "some bold, italic or underline cannot be written in Fountain here",
            );
        }
        s
    }

    /// One line for elements that hold a single line in Fountain.
    fn single_line(&mut self, idx: usize, e: &Element) -> RLine {
        let lines = split(&e.text);
        if lines.len() == 1 {
            return lines.into_iter().next().unwrap_or_default();
        }
        self.loss(
            Some(idx),
            "line_break",
            format!(
                "a {} holds one line in Fountain; line breaks become spaces",
                e.kind
            ),
        );
        let mut joined = Vec::new();
        for (k, l) in lines.into_iter().enumerate() {
            if k > 0 {
                joined.push(Run::plain(" "));
            }
            joined.extend(l);
        }
        coldopen_screenplay::RichText::new(joined).into_runs()
    }

    fn title_page(&mut self, tp: &TitlePage) {
        let mut any_value = false;
        for (k, f) in tp.fields.iter().enumerate() {
            let key_ok = !f.key.contains(':') && title::key_line(&format!("{}:", f.key)).is_some();
            if !key_ok || (k == 0 && is_scene_heading_prefix(&f.key)) {
                self.loss(
                    None,
                    "title_key",
                    format!("\"{}\" cannot be a title page key", f.key),
                );
            }
            let mut values = Vec::new();
            for v in &f.values {
                let lines = split(v);
                if lines.len() > 1 {
                    self.loss(
                        None,
                        "line_break",
                        "a title page value holds one line; use more values",
                    );
                }
                let mut joined = Vec::new();
                for (k, l) in lines.into_iter().enumerate() {
                    if k > 0 {
                        joined.push(Run::plain(" "));
                    }
                    joined.extend(l);
                }
                values.push(self.inline(None, &joined, None, &[]));
            }
            any_value |= !values.is_empty();
            match values.len() {
                0 => self.out.push(format!("{}:", f.key)),
                1 => self.out.push(format!("{}: {}", f.key, values[0])),
                _ => {
                    self.out.push(format!("{}:", f.key));
                    for v in values {
                        self.out.push(format!("    {v}"));
                    }
                }
            }
        }
        if !any_value {
            self.loss(
                None,
                "title_empty",
                "a title page needs at least one value to be recognized",
            );
        }
    }

    fn body(&mut self, els: &[Element], has_title: bool) {
        let gap_of = |prev: Option<usize>, i: usize| -> Option<u32> {
            prev.map(|p| {
                els[i]
                    .space_before
                    .map_or_else(|| default_gap(Some(els[p].kind), els[i].kind), u32::from)
            })
        };
        let printing: Vec<usize> = (0..els.len())
            .filter(|&i| els[i].kind.is_printing())
            .collect();

        let mut pending: Vec<usize> = Vec::new();
        let mut prev: Option<usize> = None;
        let mut in_group = false;
        let mut first_line_of_body: Option<(usize, usize)> = None;

        for (pos, &i) in printing.iter().enumerate() {
            pending
                .extend((prev.map_or(0, |p| p + 1)..i).filter(|&t| els[t].kind.is_transparent()));
            let e = &els[i];
            let gap = gap_of(prev, i);

            // Dialogue groups continue only directly, with no gap.
            let joins_group =
                in_group && gap == Some(0) && pending.is_empty() && e.kind.is_dialogue_member();

            let para_start = match gap {
                None => {
                    if !self.out.is_empty() {
                        self.blank(1);
                    }
                    for &t in &pending {
                        self.transparent(t, &els[t]);
                        self.blank(1);
                    }
                    true
                }
                Some(0) if joins_group => false,
                Some(0) if pending.is_empty() => {
                    // Only action and lyrics can continue straight on from
                    // the line above, and never inside a dialogue group.
                    let continues =
                        matches!(e.kind, ElementKind::Action | ElementKind::Lyric) && !in_group;
                    if continues {
                        false
                    } else {
                        self.loss(
                            Some(i),
                            "spacing",
                            format!(
                                "a {} cannot sit directly below the line above it in Fountain",
                                e.kind
                            ),
                        );
                        self.blank(1);
                        true
                    }
                }
                Some(g) => {
                    let line_markers = pending.iter().all(|&t| {
                        matches!(
                            els[t].kind,
                            ElementKind::Section | ElementKind::Synopsis | ElementKind::PageBreak
                        )
                    });
                    if g == 0 && line_markers && !in_group {
                        for &t in &pending {
                            self.transparent(t, &els[t]);
                        }
                    } else {
                        let g = if g == 0 {
                            self.loss(
                                Some(i),
                                "spacing",
                                "notes and boneyard need blank lines around them here",
                            );
                            1
                        } else {
                            g
                        };
                        self.blank(g);
                        for &t in &pending {
                            self.transparent(t, &els[t]);
                            self.blank(1);
                        }
                    }
                    true
                }
            };
            pending.clear();

            let next = printing.get(pos + 1).copied();
            let between_has_transparent =
                next.is_some_and(|n| (i + 1..n).any(|t| els[t].kind.is_transparent()));
            let next_gap = next.and_then(|n| gap_of(Some(i), n));
            let blank_after = next_gap.is_none_or(|g| g >= 1);
            let group_follows = next.is_some_and(|n| {
                next_gap == Some(0) && !between_has_transparent && els[n].kind.is_dialogue_member()
            });
            let hold_follows = group_follows
                && next.is_some_and(|n| {
                    els[n].kind == ElementKind::Dialogue
                        && split(&els[n].text)
                            .first()
                            .is_some_and(|l| crate::norm::is_empty_line(l))
                });

            if self.out.is_empty() && !has_title {
                first_line_of_body = Some((0, i));
            }
            let ctx = Ctx {
                para_start,
                blank_after,
                group_follows,
                hold_follows,
            };
            let starts_group = self.element(i, e, ctx, joins_group);
            in_group = starts_group || (joins_group && e.kind.is_dialogue_member());
            prev = Some(i);
        }

        // Transparent elements after the last printing element.
        let tail_from = prev.map_or(0, |p| p + 1);
        for (t, e) in els.iter().enumerate().skip(tail_from) {
            if !self.out.is_empty() {
                self.blank(1);
            }
            self.transparent(t, e);
        }

        if let Some((line_idx, i)) = first_line_of_body {
            self.guard_opening(line_idx, i, &els[i]);
        }
    }

    /// Without a title page, the first paragraph must not read as one, nor
    /// start with U+FEFF, which a reader removes as a byte-order mark
    /// (spec §3, §9).
    fn guard_opening(&mut self, line_idx: usize, i: usize, e: &Element) {
        let bom = self
            .out
            .get(line_idx)
            .is_some_and(|l| l.starts_with('\u{feff}'));
        if !bom {
            // Physical lines: inline notes and boneyard can hold line breaks.
            let para: Vec<&str> = self
                .out
                .iter()
                .skip(line_idx)
                .flat_map(|l| l.split('\n'))
                .take_while(|l| !l.trim().is_empty())
                .collect();
            if title::detect(&para).is_none() {
                return;
            }
        }
        let Some(first) = self.out.get_mut(line_idx) else {
            return;
        };
        match e.kind {
            ElementKind::Action | ElementKind::General => first.insert(0, '!'),
            ElementKind::Character => first.insert(0, '@'),
            ElementKind::Transition => first.insert(0, '>'),
            ElementKind::SceneHeading | ElementKind::Shot
                if first.chars().next().is_some_and(char::is_alphanumeric) =>
            {
                first.insert(0, '.')
            }
            // The read-back check reports what can't be forced.
            _ if bom => {}
            _ => self.loss(
                Some(i),
                "title_lookalike",
                "the opening lines would read as a title page",
            ),
        }
    }

    fn transparent(&mut self, i: usize, e: &Element) {
        match e.kind {
            ElementKind::PageBreak => self.out.push("===".into()),
            ElementKind::Section | ElementKind::Synopsis => {
                let l = self.single_line(i, e);
                let s = self.inline(Some(i), &l, None, &[]);
                let marker = if e.kind == ElementKind::Section {
                    "#".repeat(usize::from(e.depth.max(1)))
                } else {
                    "=".to_string()
                };
                if s.is_empty() {
                    self.out.push(marker);
                } else {
                    self.out.push(format!("{marker} {s}"));
                }
            }
            ElementKind::Note => {
                let t = e.text.plain_text();
                if t.contains("]]") || t.ends_with(']') {
                    self.loss(
                        Some(i),
                        "note_text",
                        "a note cannot contain ]] or end with ]",
                    );
                }
                self.push_multiline(format!("[[{}]]", note_body(&t)));
            }
            ElementKind::Boneyard => {
                let t = e.text.plain_text();
                if t.contains("*/") {
                    self.loss(Some(i), "boneyard_text", "boneyard cannot contain */");
                }
                self.push_multiline(format!("/*{t}*/"));
            }
            _ => {}
        }
    }

    fn push_multiline(&mut self, s: String) {
        for l in s.split('\n') {
            self.out.push(l.to_string());
        }
    }

    /// Writes one printing element. Returns true when it opens a dialogue
    /// group (a character).
    fn element(&mut self, i: usize, e: &Element, ctx: Ctx, in_group: bool) -> bool {
        use ElementKind::*;
        match e.kind {
            Action if e.centered => self.centered(i, e),
            Action => self.action(i, e, ctx),
            General => {
                self.loss(
                    Some(i),
                    "general_element",
                    "Fountain has no General element; it is written as action",
                );
                self.action(i, e, ctx);
            }
            SceneHeading => self.scene_heading(i, e, ctx),
            Shot => {
                self.loss(
                    Some(i),
                    "shot_element",
                    "Fountain has no Shot element; it is written as a scene heading",
                );
                self.scene_heading(
                    i,
                    e,
                    Ctx {
                        blank_after: false,
                        ..ctx
                    },
                );
            }
            Transition => self.transition(i, e, ctx),
            Character => {
                self.character(i, e, ctx);
                return true;
            }
            Lyric => self.lyric(i, e),
            Parenthetical => {
                if !in_group {
                    self.loss(
                        Some(i),
                        "orphan_parenthetical",
                        "a parenthetical must follow a character or dialogue",
                    );
                }
                self.parenthetical(i, e);
            }
            Dialogue => {
                if !in_group {
                    self.loss(
                        Some(i),
                        "orphan_dialogue",
                        "dialogue must follow a character",
                    );
                }
                self.dialogue(i, e);
            }
            PageBreak | Section | Synopsis | Note | Boneyard => self.transparent(i, e),
        }
        false
    }

    fn action(&mut self, i: usize, e: &Element, ctx: Ctx) {
        let lines = split(&e.text);
        let n = lines.len();
        let mut out: Vec<String> = Vec::with_capacity(n);
        for (k, l) in lines.iter().enumerate() {
            let mut s = self.inline(Some(i), l, None, &[]);
            if is_marker(seen(&s).trim_start()) {
                s = self.inline(Some(i), l, None, &first_non_ws(l));
            }
            // A paragraph-start line gets `!` below; any other line holding
            // only notes or boneyard needs an anchor to stay a line.
            if !(k == 0 && ctx.para_start) {
                s = anchor_hidden_only(l, s);
            }
            out.push(s);
        }
        if ctx.para_start
            && let Some(first) = out.first_mut()
        {
            let v = seen(first);
            let t = v.trim_start();
            let more = n > 1 || !ctx.blank_after;
            let misread = t.is_empty()
                || is_forced(t)
                || (!more && is_scene_heading_prefix(t))
                || (!more && is_transition_shape(t))
                || (more && is_character_shape(t.trim_end()));
            if misread {
                first.insert(0, '!');
            }
        }
        self.out.extend(out);
    }

    fn centered(&mut self, i: usize, e: &Element) {
        for l in split(&e.text) {
            if visible(&l).trim().is_empty() && !norm_has_hidden(&l) {
                self.loss(
                    Some(i),
                    "empty_centered_line",
                    "a centered line needs some text",
                );
            }
            let s = self.inline(Some(i), &l, Some('<'), &[]);
            let s = anchor_hidden_only(&l, s);
            self.out.push(format!(">{s}<"));
        }
    }

    fn lyric(&mut self, i: usize, e: &Element) {
        for l in split(&e.text) {
            let s = self.inline(Some(i), &l, None, &[]);
            self.out.push(format!("~{s}"));
        }
    }

    fn scene_heading(&mut self, i: usize, e: &Element, ctx: Ctx) {
        let l = self.single_line(i, e);
        let plain = visible(&l);
        let number = e.scene_number.as_deref();
        let mut esc = Vec::new();
        if number.is_none()
            && let Some((len, _)) = split_scene_number(&plain)
        {
            esc.push(plain.chars().count() - len);
        }
        let after = number.map(|_| ' ');
        let mut s = self.inline(Some(i), &l, after, &esc);
        let v = seen(&s);
        let t = v.trim_start();
        // Only the syntax page's prefixes go unforced; a reversed one such
        // as `E/I` is written `.E/I` so every Fountain reader sees a heading.
        let auto = ctx.para_start
            && ctx.blank_after
            && is_strict_scene_heading_prefix(t)
            && !is_forced(t)
            && !is_marker(t);
        if !auto {
            if !ctx.para_start || !t.chars().next().is_some_and(char::is_alphanumeric) {
                self.loss(
                    Some(i),
                    "scene_heading",
                    "this scene heading cannot be marked as one in Fountain here",
                );
            }
            s.insert(0, '.');
        }
        if let Some(n) = number {
            if n.is_empty()
                || !n
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            {
                self.loss(
                    Some(i),
                    "scene_number",
                    "Fountain scene numbers use only letters, digits, - and .",
                );
            }
            s.push_str(&format!(" #{n}#"));
        }
        self.out.push(s);
    }

    fn transition(&mut self, i: usize, e: &Element, ctx: Ctx) {
        let l = self.single_line(i, e);
        let plain = visible(&l);
        let esc: Vec<usize> = if plain.ends_with('<') {
            vec![plain.chars().count() - 1]
        } else {
            Vec::new()
        };
        let mut s = self.inline(Some(i), &l, None, &esc);
        let v = seen(&s);
        let t = v.trim_start();
        let auto = ctx.para_start
            && ctx.blank_after
            && is_transition_shape(t)
            && !is_forced(t)
            && !is_marker(t)
            && !is_scene_heading_prefix(t);
        if !auto {
            if !ctx.para_start {
                self.loss(
                    Some(i),
                    "transition",
                    "a transition cannot sit directly below the line above it",
                );
            }
            s.insert(0, '>');
        }
        self.out.push(s);
    }

    fn character(&mut self, i: usize, e: &Element, ctx: Ctx) {
        let l = self.single_line(i, e);
        let plain = visible(&l);
        let mut esc = Vec::new();
        if !e.dual && plain.ends_with('^') {
            esc.push(plain.chars().count() - 1);
        }
        let after = e.dual.then_some(' ');
        let mut s = self.inline(Some(i), &l, after, &esc);
        if !e.dual && strip_dual(&seen(&s)).1 {
            s = self.inline(
                Some(i),
                &l,
                after,
                &[plain.chars().count().saturating_sub(1)],
            );
        }
        let v = seen(&s);
        let t = v.trim();
        // A hold line after the name is not content, so the scene heading
        // and transition rules, which come first, could claim the line, as
        // it is written: the dual marker can make a heading of a name
        // ("EST ^").
        let line = if e.dual {
            format!("{t} ^")
        } else {
            t.to_string()
        };
        let claimed_earlier =
            ctx.hold_follows && (is_scene_heading_prefix(&line) || is_transition_shape(&line));
        let auto = ctx.para_start
            && ctx.group_follows
            && is_character_shape(t)
            && !is_forced(t)
            && !is_marker(t)
            && !claimed_earlier;
        if !auto {
            if !ctx.para_start {
                self.loss(
                    Some(i),
                    "character",
                    "a character cannot sit directly below the line above it",
                );
            }
            s.insert(0, '@');
        }
        if e.dual {
            s.push_str(" ^");
        }
        self.out.push(s);
    }

    fn parenthetical(&mut self, i: usize, e: &Element) {
        let lines = split(&e.text);
        let texts: Vec<String> = lines.iter().map(|l| visible(l)).collect();
        let n = texts.len();
        let ok = texts.first().is_some_and(|f| f.starts_with('('))
            && texts.last().is_some_and(|l| l.ends_with(')'))
            && texts[..n.saturating_sub(1)]
                .iter()
                .all(|t| !t.ends_with(')'));
        if !ok {
            self.loss(
                Some(i),
                "parenthetical",
                "a parenthetical must start with ( and end with )",
            );
        }
        for l in &lines {
            let s = self.inline(Some(i), l, None, &[]);
            self.out.push(s);
        }
    }

    fn dialogue(&mut self, i: usize, e: &Element) {
        for l in split(&e.text) {
            if visible(&l).trim().is_empty() {
                if norm_has_hidden(&l) {
                    let s = self.inline(Some(i), &l, Some('*'), &[]);
                    self.out.push(anchor_hidden_only(&l, s));
                } else {
                    self.out.push("  ".into());
                }
                continue;
            }
            let mut s = self.inline(Some(i), &l, None, &[]);
            let v = seen(&s);
            let t = v.trim_start();
            if t.starts_with('(') || t.starts_with('~') {
                s = self.inline(Some(i), &l, None, &first_non_ws(&l));
            }
            self.out.push(s);
        }
    }
}

/// A line holding only notes or boneyard would join the line next to it
/// when read back. An empty emphasis pair around it adds nothing visible but
/// keeps it a line of its own.
fn anchor_hidden_only(l: &[Run], s: String) -> String {
    if visible(l).trim().is_empty() && norm_has_hidden(l) {
        format!("*{s}*")
    } else {
        s
    }
}

fn norm_has_hidden(l: &[Run]) -> bool {
    crate::norm::has_hidden(l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use coldopen_screenplay::{ElementKind, RichText};

    const B: Style = Style {
        bold: true,
        italic: false,
        underline: false,
    };
    const I: Style = Style {
        bold: false,
        italic: true,
        underline: false,
    };
    const U: Style = Style {
        bold: false,
        italic: false,
        underline: true,
    };

    fn w(runs: Vec<Run>) -> (String, bool) {
        write_inline(&RichText::new(runs).into_runs(), None, &[])
    }

    #[test]
    fn inline_styles() {
        assert_eq!(
            w(vec![Run::styled("a", B), Run::styled("b", I)]),
            ("**a***b*".into(), true)
        );
        assert_eq!(w(vec![Run::styled("a b", U)]), ("_a b_".into(), true));
        assert_eq!(
            w(vec![Run::plain("snake_case")]),
            ("snake_case".into(), true)
        );
        assert_eq!(w(vec![Run::plain("*a*")]), (r"\*a\*".into(), true));
        assert_eq!(w(vec![Run::plain("a /* b")]), (r"a \/* b".into(), true));
        assert_eq!(w(vec![Run::plain("[[x")]), (r"\[[x".into(), true));
    }

    #[test]
    fn a_style_change_after_a_note_keeps_both_styles() {
        // Closing the bold after the note would put `***` in front of the
        // italic, which reads back differently; it closes before instead.
        let note = |t: &str| Run::Note(t.into());
        assert_eq!(
            w(vec![
                Run::styled("bold", B),
                note("open\nclose"),
                Run::styled("//", I),
                Run::plain(" bone"),
            ]),
            ("**bold**[[open\nclose]]*/\\/* bone".into(), true)
        );
        // Lines that already read back keep their written form.
        assert_eq!(
            w(vec![Run::styled("a", B), note("n"), Run::styled("b", I)]),
            ("**a[[n]]***b*".into(), true)
        );
        // A style that carries on across a note still spans it.
        assert_eq!(
            w(vec![Run::styled("a ", B), note("n"), Run::styled(" b", B)]),
            ("**a [[n]] b**".into(), true)
        );
    }

    #[test]
    fn underline_mid_word_is_a_loss() {
        let (_, exact) = w(vec![
            Run::plain("un"),
            Run::styled("der", U),
            Run::plain("line"),
        ]);
        assert!(!exact);
    }

    #[test]
    fn shot_and_general_are_losses() {
        let s = Script {
            elements: vec![
                Element::new(ElementKind::Shot, "CLOSE ON MAYA"),
                Element::new(ElementKind::General, "Hello"),
            ],
            ..Default::default()
        };
        let out = write(&s);
        let codes: Vec<&str> = out.losses.iter().map(|l| l.code).collect();
        assert!(codes.contains(&"shot_element"));
        assert!(codes.contains(&"general_element"));
    }

    /// Headings use the syntax page's prefixes to decide on forcing; every
    /// other check asks what our reader would do, reversed prefixes included.
    #[test]
    fn reversed_prefixes() {
        let el = Element::new;
        let cases = [
            (vec![el(ElementKind::SceneHeading, "E/I CAR")], ".E/I CAR"),
            (
                vec![el(ElementKind::Action, "E/I is a label.")],
                "!E/I is a label.",
            ),
            (
                vec![el(ElementKind::Transition, "EXT/INT TO:")],
                ">EXT/INT TO:",
            ),
            (
                vec![
                    el(ElementKind::Character, "E/I CAR"),
                    el(ElementKind::Dialogue, "\nHonk."),
                ],
                "@E/I CAR\n  \nHonk.",
            ),
        ];
        for (elements, text) in cases {
            let s = Script {
                elements,
                ..Default::default()
            };
            let out = write(&s);
            assert_eq!(out.text, format!("{text}\n"));
            assert!(out.losses.is_empty(), "{:?}", out.losses);
            assert_eq!(crate::parse(&out.text).script, s);
        }

        let s = Script {
            title_page: TitlePage {
                fields: vec![coldopen_screenplay::TitleField::new(
                    "EXT/INT. CAR",
                    vec![RichText::plain("DAY")],
                )],
            },
            ..Default::default()
        };
        let codes: Vec<&str> = write(&s).losses.iter().map(|l| l.code).collect();
        assert_eq!(codes, ["title_key"]);
    }
}
