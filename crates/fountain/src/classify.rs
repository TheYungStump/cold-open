//! Classifies logical lines into script elements (spec §5–§7).

use coldopen_screenplay::{Element, ElementKind, RichText, Run};

use crate::inline;
use crate::line::{
    self, Atom, Line, drop_first_visible, drop_last_visible, is_escaped, trim, trim_end,
    trim_start, visible,
};

// ---- Shape tests on visible text (escapes still present) ----

/// `t` has its leading whitespace removed.
pub(crate) fn is_scene_heading_prefix(t: &str) -> bool {
    const PREFIXES: [&str; 6] = ["INT./EXT", "INT/EXT", "I/E", "INT", "EXT", "EST"];
    PREFIXES.iter().any(|p| {
        t.len() > p.len()
            && t.is_char_boundary(p.len())
            && t[..p.len()].eq_ignore_ascii_case(p)
            && matches!(t.as_bytes()[p.len()], b'.' | b' ')
    })
}

pub(crate) fn is_page_break(t: &str) -> bool {
    let t = t.trim();
    t.len() >= 3 && t.chars().all(|c| c == '=')
}

/// `t` is trimmed on both sides.
pub(crate) fn is_centered(t: &str) -> bool {
    let chars: Vec<char> = t.chars().collect();
    chars.len() >= 3
        && chars[0] == '>'
        && chars[chars.len() - 1] == '<'
        && !is_escaped(&chars, chars.len() - 1)
        && chars[1..chars.len() - 1].iter().any(|c| !c.is_whitespace())
}

pub(crate) fn is_forced_heading(t: &str) -> bool {
    let mut c = t.chars();
    c.next() == Some('.') && c.next().is_some_and(char::is_alphanumeric)
}

/// `t` has its leading whitespace removed but keeps trailing whitespace, so
/// a space after the colon stops the match.
pub(crate) fn is_transition_shape(t: &str) -> bool {
    t.ends_with("TO:") && !t.chars().any(char::is_lowercase)
}

/// `t` is trimmed on both sides.
pub(crate) fn is_character_shape(t: &str) -> bool {
    let name = strip_dual(t).0;
    let name = match name.find('(') {
        Some(i) => &name[..i],
        None => name,
    };
    name.chars().any(char::is_uppercase) && !name.chars().any(char::is_lowercase)
}

/// Splits off a final unescaped `^` (and the spaces before it).
pub(crate) fn strip_dual(t: &str) -> (&str, bool) {
    let chars: Vec<char> = t.chars().collect();
    if chars.last() == Some(&'^') && !is_escaped(&chars, chars.len() - 1) {
        (t[..t.len() - 1].trim_end(), true)
    } else {
        (t, false)
    }
}

/// Splits a trailing `#…#` scene number off a trimmed heading.
pub(crate) fn split_scene_number(t: &str) -> Option<(usize, String)> {
    let chars: Vec<char> = t.chars().collect();
    let n = chars.len();
    if n < 3 || chars[n - 1] != '#' || is_escaped(&chars, n - 1) {
        return None;
    }
    let mut p = n - 1;
    while p > 0 {
        p -= 1;
        let c = chars[p];
        if c == '#' {
            if p + 1 == n - 1 || is_escaped(&chars, p) {
                return None;
            }
            let number: String = chars[p + 1..n - 1].iter().collect();
            return Some((n - p, number));
        }
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '.') {
            return None;
        }
    }
    None
}

/// Spacing used when `space_before` is unset (spec §7).
pub(crate) fn default_gap(prev: Option<ElementKind>, kind: ElementKind) -> u32 {
    use ElementKind::*;
    if kind.is_dialogue_member()
        && matches!(prev, Some(Character | Parenthetical | Dialogue | Lyric))
    {
        0
    } else {
        1
    }
}

// ---- Entries: lines after hidden-only lines are merged ----

enum Entry {
    Blank,
    Hold,
    Content(Line),
    /// Hidden runs with no visible text anywhere in their run of lines.
    Standalone(Vec<Run>),
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Blank,
    Hold,
    Content,
    HiddenOnly,
}

fn class(l: &Line) -> Class {
    let vis = visible(l);
    if vis.chars().any(|c| !c.is_whitespace()) {
        Class::Content
    } else if line::has_hidden(l) {
        Class::HiddenOnly
    } else if vis.chars().count() >= 2 {
        Class::Hold
    } else {
        Class::Blank
    }
}

fn hidden_atoms(l: Line) -> Vec<Atom> {
    l.into_iter()
        .filter(|a| matches!(a, Atom::Hidden(_)))
        .collect()
}

fn entries(lines: Vec<Line>) -> Vec<Entry> {
    let classes: Vec<Class> = lines.iter().map(class).collect();
    let mut lines: Vec<Option<Line>> = lines.into_iter().map(Some).collect();
    let n = lines.len();
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if classes[i] == Class::Blank {
            out.push(Entry::Blank);
            i += 1;
            continue;
        }
        let j = (i..n).find(|&k| classes[k] == Class::Blank).unwrap_or(n);
        let content: Vec<usize> = (i..j).filter(|&k| classes[k] == Class::Content).collect();
        if content.is_empty() {
            for k in i..j {
                match classes[k] {
                    Class::Hold => out.push(Entry::Hold),
                    _ => {
                        let runs = hidden_atoms(lines[k].take().unwrap_or_default())
                            .into_iter()
                            .filter_map(|a| match a {
                                Atom::Hidden(r) => Some(r),
                                Atom::Ch(_) => None,
                            })
                            .collect();
                        out.push(Entry::Standalone(runs));
                    }
                }
            }
        } else {
            // Hidden-only lines join the previous content line, or the next
            // one if they come first (spec §5).
            let mut prefix: Vec<Atom> = Vec::new();
            for k in i..j {
                if classes[k] != Class::HiddenOnly {
                    continue;
                }
                let atoms = hidden_atoms(lines[k].take().unwrap_or_default());
                match content.iter().rev().find(|&&c| c < k) {
                    Some(&p) => {
                        if let Some(l) = lines[p].as_mut() {
                            l.extend(atoms);
                        }
                    }
                    None => prefix.extend(atoms),
                }
            }
            if !prefix.is_empty()
                && let Some(l) = lines[content[0]].as_mut()
            {
                prefix.append(l);
                *l = prefix;
            }
            for k in i..j {
                match classes[k] {
                    Class::Hold => out.push(Entry::Hold),
                    Class::Content => out.push(Entry::Content(lines[k].take().unwrap_or_default())),
                    _ => {}
                }
            }
        }
        i = j;
    }
    out
}

// ---- Building elements with spacing ----

struct Builder {
    out: Vec<Element>,
    run: u32,
    gap: u32,
    prev: Option<ElementKind>,
}

impl Builder {
    fn blank(&mut self) {
        self.run += 1;
        self.gap = self.gap.max(self.run);
    }

    fn transparent(&mut self, e: Element) {
        self.run = 0;
        self.out.push(e);
    }

    fn printing(&mut self, mut e: Element) {
        if self.prev.is_some() && self.gap != default_gap(self.prev, e.kind) {
            e.space_before = Some(self.gap.min(u32::from(u8::MAX)) as u8);
        }
        self.prev = Some(e.kind);
        self.run = 0;
        self.gap = 0;
        self.out.push(e);
    }
}

fn vis_trim_start(l: &Line) -> String {
    visible(l).trim_start().to_string()
}

fn content_at(entries: &[Entry], i: usize) -> Option<&Line> {
    match entries.get(i) {
        Some(Entry::Content(l)) => Some(l),
        _ => None,
    }
}

/// True for lines that interrupt an action paragraph (spec §6.1).
fn is_marker_line(t: &str) -> bool {
    is_page_break(t)
        || t.starts_with('#')
        || t.starts_with('=')
        || t.starts_with('~')
        || is_centered(t.trim_end())
}

pub(crate) fn classify(lines: Vec<Line>) -> Vec<Element> {
    let entries = entries(lines);
    let mut b = Builder {
        out: Vec::new(),
        run: 0,
        gap: 0,
        prev: None,
    };
    let mut para_start = true;
    let mut i = 0;
    while i < entries.len() {
        let l = match &entries[i] {
            Entry::Blank | Entry::Hold => {
                b.blank();
                para_start = true;
                i += 1;
                continue;
            }
            Entry::Standalone(runs) => {
                for r in runs {
                    let (kind, text) = match r {
                        Run::Boneyard(s) => (ElementKind::Boneyard, s),
                        Run::Note(s) => (ElementKind::Note, s),
                        Run::Text { text, .. } => (ElementKind::Note, text),
                    };
                    b.transparent(Element::new(kind, RichText::plain(text.clone())));
                }
                para_start = true;
                i += 1;
                continue;
            }
            Entry::Content(l) => l,
        };
        let t = vis_trim_start(l);

        if is_page_break(&t) {
            b.transparent(Element::new(ElementKind::PageBreak, RichText::empty()));
            para_start = true;
            i += 1;
            continue;
        }
        if t.starts_with('#') {
            b.transparent(section(l.clone()));
            para_start = true;
            i += 1;
            continue;
        }
        if t.starts_with('=') {
            b.transparent(synopsis(l.clone()));
            para_start = true;
            i += 1;
            continue;
        }
        if t.starts_with('~') {
            i = lyric_block(&entries, i, &mut b);
            para_start = false;
            continue;
        }
        if is_centered(t.trim_end()) {
            i = centered_block(&entries, i, &mut b);
            para_start = false;
            continue;
        }
        if !para_start {
            i = action_block(&entries, i, None, &mut b);
            continue;
        }

        let next = entries.get(i + 1);
        let next_is_content = matches!(next, Some(Entry::Content(_)));
        let next_holds = matches!(next, Some(Entry::Content(_) | Entry::Hold));
        para_start = false;

        if t.starts_with('!') {
            let mut first = l.clone();
            trim_start(&mut first);
            drop_first_visible(&mut first, 1);
            i = action_block(&entries, i, Some(first), &mut b);
        } else if t.starts_with('@') {
            let mut first = l.clone();
            trim_start(&mut first);
            drop_first_visible(&mut first, 1);
            i = character_group(&entries, i, first, &mut b);
        } else if is_forced_heading(&t) {
            let mut first = l.clone();
            trim_start(&mut first);
            drop_first_visible(&mut first, 1);
            b.printing(scene_heading(first));
            i += 1;
        } else if t.starts_with('>') {
            let mut first = l.clone();
            trim_start(&mut first);
            drop_first_visible(&mut first, 1);
            b.printing(simple(ElementKind::Transition, first));
            i += 1;
        } else if !next_is_content && is_scene_heading_prefix(&t) {
            b.printing(scene_heading(l.clone()));
            i += 1;
        } else if !next_is_content && is_transition_shape(&t) {
            b.printing(simple(ElementKind::Transition, l.clone()));
            i += 1;
        } else if next_holds && is_character_shape(t.trim_end()) {
            i = character_group(&entries, i, l.clone(), &mut b);
        } else {
            i = action_block(&entries, i, None, &mut b);
        }
    }
    // Emphasis can consume delimiters and expose edge whitespace, so the
    // text is brought to normal form only now (spec §10).
    let mut out = b.out;
    for e in &mut out {
        e.text = crate::norm::normalize_text(e);
    }
    out
}

fn simple(kind: ElementKind, mut l: Line) -> Element {
    trim(&mut l);
    Element::new(kind, inline::parse_lines(&[l]))
}

fn scene_heading(mut l: Line) -> Element {
    trim(&mut l);
    let mut number = None;
    if let Some((len, n)) = split_scene_number(&visible(&l)) {
        drop_last_visible(&mut l, len);
        trim_end(&mut l);
        number = Some(n);
    }
    let mut e = Element::new(ElementKind::SceneHeading, inline::parse_lines(&[l]));
    e.scene_number = number;
    e
}

fn section(mut l: Line) -> Element {
    trim_start(&mut l);
    let depth = visible(&l).chars().take_while(|&c| c == '#').count();
    drop_first_visible(&mut l, depth);
    trim(&mut l);
    let depth = u8::try_from(depth).unwrap_or(u8::MAX);
    Element::new(ElementKind::Section, inline::parse_lines(&[l])).depth(depth)
}

fn synopsis(mut l: Line) -> Element {
    trim_start(&mut l);
    drop_first_visible(&mut l, 1);
    trim(&mut l);
    Element::new(ElementKind::Synopsis, inline::parse_lines(&[l]))
}

fn lyric_line(mut l: Line) -> Line {
    trim_start(&mut l);
    drop_first_visible(&mut l, 1);
    trim(&mut l);
    l
}

fn lyric_block(entries: &[Entry], mut i: usize, b: &mut Builder) -> usize {
    let mut lines = Vec::new();
    while let Some(l) = content_at(entries, i) {
        if !vis_trim_start(l).starts_with('~') {
            break;
        }
        lines.push(lyric_line(l.clone()));
        i += 1;
    }
    b.printing(Element::new(
        ElementKind::Lyric,
        inline::parse_lines(&lines),
    ));
    i
}

fn centered_block(entries: &[Entry], mut i: usize, b: &mut Builder) -> usize {
    let mut lines = Vec::new();
    while let Some(l) = content_at(entries, i) {
        if !is_centered(visible(l).trim()) {
            break;
        }
        let mut l = l.clone();
        trim(&mut l);
        drop_first_visible(&mut l, 1);
        drop_last_visible(&mut l, 1);
        trim(&mut l);
        lines.push(l);
        i += 1;
    }
    b.printing(Element::new(ElementKind::Action, inline::parse_lines(&lines)).centered());
    i
}

fn action_line(mut l: Line) -> Line {
    line::expand_tabs(&mut l);
    trim_end(&mut l);
    l
}

/// An action element starting at `i` (spec §6.4). `first` replaces the
/// first line when a forcing marker was removed from it.
fn action_block(entries: &[Entry], mut i: usize, first: Option<Line>, b: &mut Builder) -> usize {
    let mut lines = Vec::new();
    match first {
        // A lone `!` forces the line after it and adds no empty line of its
        // own; any hidden runs on it move to that line.
        Some(f)
            if visible(&f).trim().is_empty()
                && content_at(entries, i + 1)
                    .is_some_and(|n| !is_marker_line(&vis_trim_start(n))) =>
        {
            let mut joined: Line = f
                .into_iter()
                .filter(|a| matches!(a, Atom::Hidden(_)))
                .collect();
            if let Some(n) = content_at(entries, i + 1) {
                joined.extend(n.iter().cloned());
            }
            lines.push(action_line(joined));
            i += 1;
        }
        Some(f) => lines.push(action_line(f)),
        None => {
            if let Some(l) = content_at(entries, i) {
                lines.push(action_line(l.clone()));
            }
        }
    }
    i += 1;
    while let Some(l) = content_at(entries, i) {
        if is_marker_line(&vis_trim_start(l)) {
            break;
        }
        lines.push(action_line(l.clone()));
        i += 1;
    }
    b.printing(Element::new(
        ElementKind::Action,
        inline::parse_lines(&lines),
    ));
    i
}

enum Pending {
    None,
    Dialogue(Vec<Line>),
    Lyric(Vec<Line>),
}

impl Pending {
    fn flush(&mut self, b: &mut Builder) {
        match std::mem::replace(self, Pending::None) {
            Pending::None => {}
            Pending::Dialogue(ls) => {
                b.printing(Element::new(
                    ElementKind::Dialogue,
                    inline::parse_lines(&ls),
                ));
            }
            Pending::Lyric(ls) => {
                b.printing(Element::new(ElementKind::Lyric, inline::parse_lines(&ls)))
            }
        }
    }
}

/// A character line at `i` (marker already removed) and the dialogue group
/// after it (spec §6.3).
fn character_group(entries: &[Entry], mut i: usize, mut first: Line, b: &mut Builder) -> usize {
    trim(&mut first);
    let (_, dual) = strip_dual(&visible(&first));
    if dual {
        drop_last_visible(&mut first, 1);
        trim_end(&mut first);
    }
    let mut c = Element::new(ElementKind::Character, inline::parse_lines(&[first]));
    c.dual = dual;
    b.printing(c);
    i += 1;

    let mut cur = Pending::None;
    loop {
        match entries.get(i) {
            None | Some(Entry::Blank) | Some(Entry::Standalone(_)) => break,
            Some(Entry::Hold) => {
                match &mut cur {
                    Pending::Dialogue(ls) => ls.push(Vec::new()),
                    _ => {
                        cur.flush(b);
                        cur = Pending::Dialogue(vec![Vec::new()]);
                    }
                }
                i += 1;
            }
            Some(Entry::Content(l)) => {
                let t = vis_trim_start(l);
                if t.starts_with('(') {
                    let close = (i..entries.len())
                        .take_while(|&k| matches!(entries[k], Entry::Content(_)))
                        .find(|&k| {
                            content_at(entries, k)
                                .is_some_and(|l| visible(l).trim_end().ends_with(')'))
                        });
                    if let Some(k) = close {
                        cur.flush(b);
                        let lines: Vec<Line> = (i..=k)
                            .filter_map(|j| content_at(entries, j))
                            .map(|l| {
                                let mut l = l.clone();
                                trim(&mut l);
                                l
                            })
                            .collect();
                        b.printing(Element::new(
                            ElementKind::Parenthetical,
                            inline::parse_lines(&lines),
                        ));
                        i = k + 1;
                        continue;
                    }
                }
                if t.starts_with('~') {
                    let ll = lyric_line(l.clone());
                    match &mut cur {
                        Pending::Lyric(ls) => ls.push(ll),
                        _ => {
                            cur.flush(b);
                            cur = Pending::Lyric(vec![ll]);
                        }
                    }
                } else {
                    let mut dl = l.clone();
                    trim(&mut dl);
                    match &mut cur {
                        Pending::Dialogue(ls) => ls.push(dl),
                        _ => {
                            cur.flush(b);
                            cur = Pending::Dialogue(vec![dl]);
                        }
                    }
                }
                i += 1;
            }
        }
    }
    cur.flush(b);
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert!(is_scene_heading_prefix("INT. X"));
        assert!(is_scene_heading_prefix("i/e x"));
        assert!(!is_scene_heading_prefix("INTERIOR"));
        assert!(!is_scene_heading_prefix("INT"));
        assert!(is_page_break("==="));
        assert!(!is_page_break("=="));
        assert!(is_centered(">X<"));
        assert!(!is_centered(r">X\<"));
        assert!(!is_centered("> <"));
        assert!(is_forced_heading(".X"));
        assert!(!is_forced_heading("..X"));
        assert!(is_transition_shape("CUT TO:"));
        assert!(!is_transition_shape("CUT TO: "));
        assert!(is_character_shape("HANS (on the radio)"));
        assert!(is_character_shape("R2 ^"));
        assert!(!is_character_shape("23"));
        assert!(!is_character_shape("Bob"));
    }

    #[test]
    fn scene_numbers() {
        assert_eq!(split_scene_number("X #1A#"), Some((4, "1A".into())));
        assert_eq!(split_scene_number("X #1 A#"), None);
        assert_eq!(split_scene_number(r"X \#1#"), None);
        assert_eq!(split_scene_number("X ##"), None);
        assert_eq!(split_scene_number("#"), None);
    }
}
