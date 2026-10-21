//! Fountain normal form (spec §10): the layout-neutral rewrites that the
//! reader applies, so `parse(write(s)) == normalize(s)` for any script the
//! writer can express.

use coldopen_screenplay::{Element, ElementKind, Marks, RichText, Run, Script, Style, TitleField};

use crate::classify::default_gap;

/// One line of rich text: runs with no line breaks inside them.
pub(crate) type RLine = Vec<Run>;

/// Splits rich text at line breaks. Empty text gives one empty line.
pub(crate) fn split(rt: &RichText) -> Vec<RLine> {
    let mut lines: Vec<RLine> = vec![Vec::new()];
    for run in rt.runs() {
        match run {
            Run::Text { text, style, marks } => {
                for (k, part) in text.split('\n').enumerate() {
                    if k > 0 {
                        lines.push(Vec::new());
                    }
                    if !part.is_empty()
                        && let Some(l) = lines.last_mut()
                    {
                        l.push(Run::marked(part, *style, *marks));
                    }
                }
            }
            hidden => {
                if let Some(l) = lines.last_mut() {
                    l.push(hidden.clone());
                }
            }
        }
    }
    lines
}

pub(crate) fn join(lines: Vec<RLine>) -> RichText {
    let mut runs = Vec::new();
    for (k, l) in lines.into_iter().enumerate() {
        if k > 0 {
            runs.push(Run::plain("\n"));
        }
        runs.extend(l);
    }
    RichText::new(runs)
}

pub(crate) fn visible(line: &[Run]) -> String {
    let mut s = String::new();
    for r in line {
        if let Run::Text { text, .. } = r {
            s.push_str(text);
        }
    }
    s
}

pub(crate) fn has_hidden(line: &[Run]) -> bool {
    line.iter().any(Run::is_hidden)
}

/// A line with no visible text and no hidden runs.
pub(crate) fn is_empty_line(line: &[Run]) -> bool {
    visible(line).is_empty() && !has_hidden(line)
}

/// A line with hidden runs but nothing visible.
pub(crate) fn is_hidden_only(line: &[Run]) -> bool {
    has_hidden(line) && visible(line).trim().is_empty()
}

/// A character with its style and tracked-change marks (which only a
/// print form of a document carries; docs/specs/layout.md §17), or a hidden
/// run.
enum Cell {
    C(char, Style, Marks),
    H(Run),
}

fn cells(line: &[Run]) -> Vec<Cell> {
    let mut out = Vec::new();
    for r in line {
        match r {
            Run::Text { text, style, marks } => {
                out.extend(text.chars().map(|c| Cell::C(c, *style, *marks)))
            }
            h => out.push(Cell::H(h.clone())),
        }
    }
    out
}

fn uncells(cells: Vec<Cell>) -> RLine {
    // Runs of one style are gathered here rather than one run per
    // character, which made normalizing a feature script take tens of
    // milliseconds.
    let mut runs = Vec::new();
    let mut text = String::new();
    let mut style = Style::default();
    let mut marks = Marks::NONE;
    for c in cells {
        match c {
            Cell::C(ch, s, m) => {
                if (s != style || m != marks) && !text.is_empty() {
                    runs.push(Run::marked(std::mem::take(&mut text), style, marks));
                }
                style = s;
                marks = m;
                text.push(ch);
            }
            Cell::H(r) => {
                if !text.is_empty() {
                    runs.push(Run::marked(std::mem::take(&mut text), style, marks));
                }
                runs.push(r);
            }
        }
    }
    if !text.is_empty() {
        runs.push(Run::marked(text, style, marks));
    }
    RichText::new(runs).into_runs()
}

fn with_cells(line: &[Run], f: impl FnOnce(&mut Vec<Cell>)) -> RLine {
    let mut c = cells(line);
    f(&mut c);
    uncells(c)
}

/// The line as it is, in normal form: what the cell rewrites return when
/// they find nothing to change.
fn unchanged(line: &[Run]) -> RLine {
    RichText::new(line.to_vec()).into_runs()
}

/// The first and last printed characters of a line.
fn edge_chars(line: &[Run]) -> (Option<char>, Option<char>) {
    let mut first = None;
    let mut last = None;
    for r in line {
        if let Run::Text { text, .. } = r {
            if first.is_none() {
                first = text.chars().next();
            }
            if let Some(c) = text.chars().next_back() {
                last = Some(c);
            }
        }
    }
    (first, last)
}

fn is_ws_cell(c: &Cell) -> bool {
    matches!(c, Cell::C(ch, _, _) if ch.is_whitespace())
}

pub(crate) fn trim_start(line: &[Run]) -> RLine {
    if !edge_chars(line).0.is_some_and(char::is_whitespace) {
        return unchanged(line);
    }
    with_cells(line, |c| {
        let first = c
            .iter()
            .position(|x| matches!(x, Cell::C(ch, _, _) if !ch.is_whitespace()))
            .unwrap_or(c.len());
        let mut i = 0;
        c.retain(|x| {
            let keep = i >= first || !is_ws_cell(x);
            i += 1;
            keep
        });
    })
}

pub(crate) fn trim_end(line: &[Run]) -> RLine {
    if !edge_chars(line).1.is_some_and(char::is_whitespace) {
        return unchanged(line);
    }
    with_cells(line, |c| {
        let last = c
            .iter()
            .rposition(|x| matches!(x, Cell::C(ch, _, _) if !ch.is_whitespace()))
            .map_or(0, |p| p + 1);
        let mut i = 0;
        c.retain(|x| {
            let keep = i < last || !is_ws_cell(x);
            i += 1;
            keep
        });
    })
}

pub(crate) fn trim(line: &[Run]) -> RLine {
    trim_end(&trim_start(line))
}

fn expand_tabs(line: &[Run]) -> RLine {
    line.iter()
        .map(|r| match r {
            Run::Text { text, style, marks } if text.contains('\t') => {
                Run::marked(text.replace('\t', "    "), *style, *marks)
            }
            other => other.clone(),
        })
        .collect()
}

/// Bold and italic on whitespace at the edges of a styled span cannot be
/// written (emphasis needs a non-space next to each marker) and cannot be
/// seen, so they are dropped. Underline is visible and left alone.
fn drop_edge_ws_styles(line: &[Run]) -> RLine {
    let styled = line.iter().any(|r| {
        matches!(r, Run::Text { text, style, .. } if (style.bold || style.italic) && text.contains(char::is_whitespace))
    });
    if !styled {
        return unchanged(line);
    }
    with_cells(line, |c| {
        for flag in [0u8, 1] {
            let get = |s: &Style| if flag == 0 { s.bold } else { s.italic };
            let chars: Vec<usize> = (0..c.len())
                .filter(|&i| matches!(c[i], Cell::C(..)))
                .collect();
            let mut k = 0;
            while k < chars.len() {
                let on = matches!(&c[chars[k]], Cell::C(_, s, _) if get(s));
                if !on {
                    k += 1;
                    continue;
                }
                let start = k;
                while k < chars.len() && matches!(&c[chars[k]], Cell::C(_, s, _) if get(s)) {
                    k += 1;
                }
                let span = &chars[start..k];
                let mut clear: Vec<usize> = Vec::new();
                clear.extend(span.iter().copied().take_while(|&i| is_ws_cell(&c[i])));
                clear.extend(
                    span.iter()
                        .rev()
                        .copied()
                        .take_while(|&i| is_ws_cell(&c[i])),
                );
                for i in clear {
                    if let Cell::C(_, s, _) = &mut c[i] {
                        if flag == 0 {
                            s.bold = false;
                        } else {
                            s.italic = false;
                        }
                    }
                }
            }
        }
    })
}

/// Bold and italic, and with `underline` underline too, cleared from every
/// whitespace character of a line. The writer falls back on this for a line
/// it cannot write as it is (spec §9): it extends `drop_edge_ws_styles` from
/// the edges of a span to the whole line.
pub(crate) fn clear_ws_styles(line: &[Run], underline: bool) -> RLine {
    let styled = line.iter().any(|r| {
        matches!(r, Run::Text { text, style, .. }
            if (style.bold || style.italic || (underline && style.underline))
                && text.contains(char::is_whitespace))
    });
    if !styled {
        return unchanged(line);
    }
    with_cells(line, |c| {
        for x in c.iter_mut() {
            if let Cell::C(ch, s, _) = x
                && ch.is_whitespace()
            {
                s.bold = false;
                s.italic = false;
                if underline {
                    s.underline = false;
                }
            }
        }
    })
}

/// `script` with bold and italic cleared from whitespace, which does not
/// print: what the writer's read-back check compares (spec §9).
pub(crate) fn without_ws_emphasis(script: &Script) -> Script {
    let clear = |t: &RichText| RichText::new(clear_ws_styles(t.runs(), false));
    let mut s = script.clone();
    for v in s
        .title_page
        .fields
        .iter_mut()
        .flat_map(|f| f.values.iter_mut())
    {
        *v = clear(v);
    }
    for e in &mut s.elements {
        e.text = clear(&e.text);
    }
    s
}

/// The writer's last resort before plain text (spec §9): where two styles
/// cross inside a word, so that neither holds the other, the one that
/// starts later is dropped from the whole word.
pub(crate) fn untangle_words(line: &[Run]) -> RLine {
    fn on(s: &Style, f: usize) -> bool {
        [s.bold, s.italic, s.underline][f]
    }
    // The style to drop from `word` (character positions in `c`), if two
    // of its styles cross.
    fn crossing(c: &[Cell], word: &[usize]) -> Option<usize> {
        let spans = |f: usize| {
            let mut out = Vec::new();
            let mut k = 0;
            while k < word.len() {
                if matches!(&c[word[k]], Cell::C(_, s, _) if on(s, f)) {
                    let start = k;
                    while k < word.len() && matches!(&c[word[k]], Cell::C(_, s, _) if on(s, f)) {
                        k += 1;
                    }
                    out.push((start, k));
                } else {
                    k += 1;
                }
            }
            out
        };
        let all = [spans(0), spans(1), spans(2)];
        for f in 0..3 {
            for g in (0..3).filter(|&g| g != f) {
                for &(a0, a1) in &all[f] {
                    if all[g].iter().any(|&(b0, b1)| a0 < b0 && b0 < a1 && a1 < b1) {
                        return Some(g);
                    }
                }
            }
        }
        None
    }
    with_cells(line, |c| {
        let chars: Vec<usize> = (0..c.len())
            .filter(|&i| matches!(c[i], Cell::C(..)))
            .collect();
        let words: Vec<Vec<usize>> = chars
            .split(|&i| is_ws_cell(&c[i]))
            .map(<[usize]>::to_vec)
            .collect();
        for word in &words {
            while let Some(g) = crossing(c, word) {
                for &i in word {
                    if let Cell::C(_, s, _) = &mut c[i] {
                        match g {
                            0 => s.bold = false,
                            1 => s.italic = false,
                            _ => s.underline = false,
                        }
                    }
                }
            }
        }
    })
}

/// Joins hidden-only lines to the previous line (or the next, if first), as
/// the reader does (spec §5).
pub(crate) fn merge_hidden_only(lines: Vec<RLine>) -> Vec<RLine> {
    if lines.iter().all(|l| !is_hidden_only(l)) || lines.iter().all(|l| is_hidden_only(l)) {
        return lines;
    }
    let mut out: Vec<RLine> = Vec::new();
    let mut prefix: RLine = Vec::new();
    for l in lines {
        if is_hidden_only(&l) {
            let hidden: Vec<Run> = l.into_iter().filter(Run::is_hidden).collect();
            match out.last_mut() {
                Some(prev) => prev.extend(hidden),
                None => prefix.extend(hidden),
            }
        } else {
            let mut l = l;
            if !prefix.is_empty() {
                let mut p = std::mem::take(&mut prefix);
                p.append(&mut l);
                l = p;
            }
            out.push(l);
        }
    }
    out
}

pub(crate) fn normalize_text(e: &Element) -> RichText {
    use ElementKind::*;
    if e.kind.is_raw_text() || e.kind == PageBreak {
        return e.text.clone();
    }
    let lines = split(&e.text);
    let lines: Vec<RLine> = lines
        .iter()
        .map(|l| {
            let l = match (e.kind, e.centered) {
                (Action, false) => trim_end(&expand_tabs(l)),
                _ => trim(l),
            };
            drop_edge_ws_styles(&l)
        })
        .collect();
    join(merge_hidden_only(lines))
}

pub(crate) fn normalize_title(fields: &[TitleField]) -> Vec<TitleField> {
    fields
        .iter()
        .map(|f| TitleField {
            key: f.key.clone(),
            values: f
                .values
                .iter()
                .map(|v| {
                    join(
                        split(v)
                            .iter()
                            .map(|l| drop_edge_ws_styles(&trim(l)))
                            .collect(),
                    )
                })
                .filter(|v| !v.is_empty())
                .collect(),
        })
        .collect()
}

/// Splits action at empty lines: each empty run becomes spacing.
fn split_action(e: Element, idx: usize, out: &mut Vec<(Element, usize)>, carry: &mut u32) {
    let lines = split(&e.text);
    if !lines.iter().any(|l| is_empty_line(l)) || lines.iter().all(|l| is_empty_line(l)) {
        out.push((e, idx));
        return;
    }
    let mut parts: Vec<(u32, Vec<RLine>)> = Vec::new();
    let mut empties = 0u32;
    for l in lines {
        if is_empty_line(&l) {
            empties += 1;
            continue;
        }
        match parts.last_mut() {
            Some((_, ls)) if empties == 0 => ls.push(l),
            _ => parts.push((empties, vec![l])),
        }
        empties = 0;
    }
    *carry += empties;
    for (k, (lead, ls)) in parts.into_iter().enumerate() {
        let mut part = e.clone();
        part.text = join(ls);
        if k == 0 {
            if lead > 0 {
                let base = e.space_before.map_or(1, u32::from);
                part.space_before = Some(clamp(base + lead));
            }
        } else {
            // Each empty line is one blank line between the parts.
            part.space_before = Some(clamp(lead));
        }
        out.push((part, idx));
    }
}

fn clamp(n: u32) -> u8 {
    u8::try_from(n).unwrap_or(u8::MAX)
}

fn mergeable(a: &Element, b: &Element) -> bool {
    use ElementKind::*;
    let gap = b
        .space_before
        .map_or_else(|| default_gap(Some(a.kind), b.kind), u32::from);
    gap == 0
        && match (a.kind, b.kind) {
            (Action, Action) => a.centered == b.centered,
            (Dialogue, Dialogue) | (Lyric, Lyric) => true,
            _ => false,
        }
}

/// The Fountain normal form of a script, with the index of the source
/// element for each element of the result. Indices never decrease.
pub fn normalize_with_map(script: &Script) -> (Script, Vec<usize>) {
    // 1. Text of each element; action split at empty lines.
    let mut split_out: Vec<(Element, usize)> = Vec::new();
    let mut carry = 0u32;
    for (idx, e) in script.elements.iter().enumerate() {
        let mut e = e.clone();
        e.text = normalize_text(&e);
        if carry > 0 && e.kind.is_printing() {
            let base = e.space_before.map_or(1, u32::from);
            e.space_before = Some(clamp(base + carry));
            carry = 0;
        }
        if e.kind == ElementKind::Action {
            split_action(e, idx, &mut split_out, &mut carry);
        } else {
            split_out.push((e, idx));
        }
    }

    // 2. Merge neighbours the reader would read as one element.
    let mut merged: Vec<(Element, usize)> = Vec::new();
    for (e, idx) in split_out {
        if let Some((last, _)) = merged.last_mut()
            && last.kind.is_printing()
            && e.kind.is_printing()
            && mergeable(last, &e)
        {
            let mut lines = split(&last.text);
            lines.extend(split(&e.text));
            last.text = join(lines);
            continue;
        }
        merged.push((e, idx));
    }

    // 3. Spacing that equals the default is left unset.
    let mut prev: Option<ElementKind> = None;
    for (e, _) in &mut merged {
        if e.kind.is_transparent() {
            e.space_before = None;
            continue;
        }
        match prev {
            None => e.space_before = None,
            Some(_) => {
                if e.space_before.map(u32::from) == Some(default_gap(prev, e.kind)) {
                    e.space_before = None;
                }
            }
        }
        prev = Some(e.kind);
    }

    let (elements, map) = merged.into_iter().unzip();
    (
        Script {
            title_page: coldopen_screenplay::TitlePage {
                fields: normalize_title(&script.title_page.fields),
            },
            elements,
        },
        map,
    )
}

/// The Fountain normal form of a script (spec §10). For any script the
/// writer reports no losses for, `parse(write(s)).script == normalize(s)`.
pub fn normalize(script: &Script) -> Script {
    normalize_with_map(script).0
}

/// Why the normal form changes an element, for the writer's `normalized`
/// losses (spec §10, review finding F21): one per element it changes, in
/// element order. `map` is the map `normalize_with_map` gave for `norm`.
pub(crate) fn normalizations(
    script: &Script,
    norm: &Script,
    map: &[usize],
) -> Vec<(usize, &'static str)> {
    use ElementKind::*;
    let last_printing = script.elements.iter().rposition(|e| e.kind.is_printing());
    let mut out = Vec::new();
    let mut k = 0;
    for (idx, e) in script.elements.iter().enumerate() {
        let first = k;
        while k < map.len() && map[k] == idx {
            k += 1;
        }
        let outs = &norm.elements[first..k];
        if outs.len() == 1 && outs[0] == *e {
            continue;
        }
        if outs.is_empty() {
            out.push((
                idx,
                "this block is written joined to the block before it, as Fountain reads two such blocks",
            ));
            continue;
        }
        let text = normalize_text(e);
        let reason = if text != e.text {
            let raw = split(&e.text);
            let lines: Vec<String> = raw.iter().map(|l| visible(l)).collect();
            if e.kind == Action && !e.text.is_empty() && e.text.plain_text().trim().is_empty() {
                "an action holding only spaces is written empty"
            } else if lines.iter().any(|l| {
                l.trim() != l.as_str()
                    && !(e.kind == Action && !e.centered && l.trim_end() == l.as_str())
            }) {
                "spaces at the start or end of a line are not kept"
            } else if e.kind == Action && lines.iter().any(|l| l.contains('\t')) {
                "tabs in an action are written as four spaces"
            } else if merge_hidden_only(raw.clone()).len() != raw.len() {
                "a line holding only a note or boneyard joins the line next to it"
            } else {
                "bold or italic on spaces at the edge of a styled stretch is not kept"
            }
        } else if e.kind == Action && split(&e.text).iter().any(|l| is_empty_line(l)) {
            let lines = split(&e.text);
            let trailing = lines.iter().rev().take_while(|l| is_empty_line(l)).count();
            let rest = lines.len() - trailing;
            let inner = lines[..rest].iter().any(|l| is_empty_line(l));
            if trailing > 0 && !inner && Some(idx) == last_printing {
                "blank lines at the end of the script's last action are dropped"
            } else {
                "blank lines inside an action are written as spacing between blocks"
            }
        } else if outs.len() == 1 && outs[0].space_before != e.space_before {
            "spacing equal to the usual is written as the usual"
        } else {
            // Only the blocks after it were joined to it; each of those is
            // reported itself.
            continue;
        };
        out.push((idx, reason));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Style = Style {
        bold: true,
        italic: false,
        underline: false,
    };

    #[test]
    fn edge_whitespace_loses_bold() {
        let l = vec![Run::plain("a"), Run::styled(" b ", B), Run::plain("c")];
        assert_eq!(
            drop_edge_ws_styles(&l),
            vec![Run::plain("a "), Run::styled("b", B), Run::plain(" c")]
        );
    }

    #[test]
    fn action_splits_at_empty_lines() {
        let s = Script {
            elements: vec![
                Element::new(ElementKind::Action, "a\n\nb\n\n\nc"),
                Element::new(ElementKind::Action, "d"),
            ],
            ..Default::default()
        };
        let n = normalize(&s);
        let texts: Vec<(String, Option<u8>)> = n
            .elements
            .iter()
            .map(|e| (e.text.plain_text(), e.space_before))
            .collect();
        assert_eq!(
            texts,
            vec![
                ("a".into(), None),
                ("b".into(), None),
                ("c".into(), Some(2)),
                ("d".into(), None)
            ]
        );
    }

    #[test]
    fn merges_and_drops_default_spacing() {
        let s = Script {
            elements: vec![
                Element::new(ElementKind::Character, "BOB"),
                Element::new(ElementKind::Dialogue, "a"),
                Element::new(ElementKind::Dialogue, "b"),
                Element::new(ElementKind::Action, "x").space_before(1),
                Element::new(ElementKind::Action, "y").space_before(0),
            ],
            ..Default::default()
        };
        let n = normalize(&s);
        assert_eq!(n.elements.len(), 3);
        assert_eq!(n.elements[1].text.plain_text(), "a\nb");
        assert_eq!(n.elements[2].text.plain_text(), "x\ny");
        assert_eq!(n.elements[2].space_before, None);
    }
}
