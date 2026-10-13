//! Find and replace over rich text (the `text.replace` command).

use coldopen_screenplay::{RichText, Run, Style};

enum Cell {
    C(char, Style),
    H(Run),
}

fn chars_equal(a: char, b: char, match_case: bool) -> bool {
    a == b || (!match_case && a.to_lowercase().eq(b.to_lowercase()))
}

/// Replaces every non-overlapping match of `find` in the printed text.
/// The replacement takes the style of the first matched character; hidden
/// runs inside a match move to just after it. Returns `None` when nothing
/// matches. `find` must not be empty.
pub fn replace_in(
    rt: &RichText,
    find: &str,
    with: &str,
    match_case: bool,
) -> Option<(RichText, usize)> {
    let pat: Vec<char> = find.chars().collect();
    if pat.is_empty() {
        return None;
    }
    let mut cells = Vec::new();
    for r in rt.runs() {
        match r {
            Run::Text { text, style } => cells.extend(text.chars().map(|c| Cell::C(c, *style))),
            h => cells.push(Cell::H(h.clone())),
        }
    }
    let vis: Vec<(char, Style)> = cells
        .iter()
        .filter_map(|c| match c {
            Cell::C(ch, s) => Some((*ch, *s)),
            Cell::H(_) => None,
        })
        .collect();

    let mut starts = Vec::new();
    let mut i = 0;
    while i + pat.len() <= vis.len() {
        if vis[i..i + pat.len()]
            .iter()
            .zip(&pat)
            .all(|(&(c, _), &p)| chars_equal(c, p, match_case))
        {
            starts.push(i);
            i += pat.len();
        } else {
            i += 1;
        }
    }
    if starts.is_empty() {
        return None;
    }

    let mut out: Vec<Run> = Vec::new();
    let mut moved: Vec<Run> = Vec::new();
    let mut v = 0usize;
    let mut next = starts.iter().peekable();
    let mut in_match: Option<usize> = None;
    for cell in cells {
        match cell {
            Cell::H(h) => {
                if in_match.is_some() {
                    moved.push(h);
                } else {
                    out.push(h);
                }
            }
            Cell::C(ch, style) => {
                if in_match.is_none() && next.peek() == Some(&&v) {
                    next.next();
                    out.push(Run::styled(with, style));
                    in_match = Some(v + pat.len());
                }
                match in_match {
                    Some(end) => {
                        if v + 1 == end {
                            in_match = None;
                            out.append(&mut moved);
                        }
                    }
                    None => out.push(Run::styled(ch.to_string(), style)),
                }
                v += 1;
            }
        }
    }
    Some((RichText::new(out), starts.len()))
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
    fn replaces_plain_text() {
        let (t, n) = replace_in(&RichText::plain("MAYA and Maya"), "MAYA", "NINA", true).unwrap();
        assert_eq!((t.plain_text().as_str(), n), ("NINA and Maya", 1));
        let (t, n) = replace_in(&RichText::plain("MAYA and Maya"), "maya", "Nina", false).unwrap();
        assert_eq!((t.plain_text().as_str(), n), ("Nina and Nina", 2));
        assert!(replace_in(&RichText::plain("abc"), "x", "y", true).is_none());
    }

    #[test]
    fn keeps_style_and_moves_hidden_runs() {
        let rt = RichText::new(vec![
            Run::plain("a "),
            Run::styled("MA", B),
            Run::Note("n".into()),
            Run::plain("YA b"),
        ]);
        let (t, _) = replace_in(&rt, "MAYA", "NINA", true).unwrap();
        assert_eq!(
            t.runs(),
            &[
                Run::plain("a "),
                Run::styled("NINA", B),
                Run::Note("n".into()),
                Run::plain(" b")
            ]
        );
    }

    #[test]
    fn deletes_and_does_not_overlap() {
        let (t, n) = replace_in(&RichText::plain("aaaa"), "aa", "", true).unwrap();
        assert_eq!((t.plain_text().as_str(), n), ("", 2));
        let (t, n) = replace_in(&RichText::plain("aaa"), "aa", "b", true).unwrap();
        assert_eq!((t.plain_text().as_str(), n), ("ba", 1));
    }
}
