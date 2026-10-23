//! Find and replace over rich text (the `text.find` and `text.replace`
//! commands). Both work on the current text: pending deletions are skipped,
//! like hidden runs (docs/specs/changes.md §2.1).

use coldopen_screenplay::{ChangeId, Marks, RichText, Run, Style};

/// A tracked replacement: the change it records into, and which changes are
/// the author's own (docs/specs/changes.md §5.1).
pub struct Tracked<'a> {
    pub change: ChangeId,
    pub own: &'a dyn Fn(ChangeId) -> bool,
}

enum Cell {
    /// A character of the current text.
    C(char, Style, Marks),
    /// A hidden run or a pending deletion.
    H(Run),
}

fn chars_equal(a: char, b: char, match_case: bool) -> bool {
    a == b || (!match_case && a.to_lowercase().eq(b.to_lowercase()))
}

fn cells(rt: &RichText) -> Vec<Cell> {
    let mut cells = Vec::new();
    for r in rt.runs() {
        match r {
            Run::Text { text, style, marks } if !marks.is_deleted() => {
                cells.extend(text.chars().map(|c| Cell::C(c, *style, *marks)))
            }
            h => cells.push(Cell::H(h.clone())),
        }
    }
    cells
}

/// Character offsets where the non-overlapping matches of `find` start in
/// the printed text, from the left. Empty when `find` is empty.
pub fn find_in(rt: &RichText, find: &str, match_case: bool) -> Vec<usize> {
    let pat: Vec<char> = find.chars().collect();
    if pat.is_empty() {
        return Vec::new();
    }
    let vis: Vec<char> = rt
        .runs()
        .iter()
        .filter_map(|r| match r {
            Run::Text { text, marks, .. } if !marks.is_deleted() => Some(text.chars()),
            _ => None,
        })
        .flatten()
        .collect();
    let mut starts = Vec::new();
    let mut i = 0;
    while i + pat.len() <= vis.len() {
        if vis[i..i + pat.len()]
            .iter()
            .zip(&pat)
            .all(|(&c, &p)| chars_equal(c, p, match_case))
        {
            starts.push(i);
            i += pat.len();
        } else {
            i += 1;
        }
    }
    starts
}

/// Replaces the matches of length `len` starting at `starts` (sorted, not
/// overlapping). The replacement takes the style of the first matched
/// character; hidden runs inside a match move to just after it. A tracked
/// replacement keeps the matched text as a pending deletion (or drops it,
/// where it was the author's own pending insertion) and puts the
/// replacement, marked inserted, right after it.
fn replace_at(
    rt: &RichText,
    starts: &[usize],
    len: usize,
    with: &str,
    tracked: Option<&Tracked<'_>>,
) -> RichText {
    let mut out: Vec<Run> = Vec::new();
    let mut moved: Vec<Run> = Vec::new();
    let mut v = 0usize;
    let mut next = starts.iter().peekable();
    // The end of the match being replaced, and the replacement's style.
    let mut in_match: Option<(usize, Style)> = None;
    for cell in cells(rt) {
        match cell {
            Cell::H(h) => {
                // A tracked replacement keeps the old text where it was,
                // struck, so what sits inside it stays in place too.
                if in_match.is_some() && tracked.is_none() {
                    moved.push(h);
                } else {
                    out.push(h);
                }
            }
            Cell::C(ch, style, marks) => {
                if in_match.is_none() && next.peek() == Some(&&v) {
                    next.next();
                    if tracked.is_none() {
                        out.push(Run::styled(with, style));
                    }
                    in_match = Some((v + len, style));
                }
                match in_match {
                    Some((end, first)) => {
                        if let Some(t) = tracked
                            && !marks.ins.is_some_and(t.own)
                        {
                            out.push(Run::marked(
                                ch.to_string(),
                                style,
                                Marks {
                                    del: Some(t.change),
                                    ..marks
                                },
                            ));
                        }
                        if v + 1 == end {
                            in_match = None;
                            if let Some(t) = tracked {
                                out.push(Run::marked(
                                    with,
                                    first,
                                    Marks {
                                        ins: Some(t.change),
                                        ..Marks::NONE
                                    },
                                ));
                            }
                            out.append(&mut moved);
                        }
                    }
                    None => out.push(Run::marked(ch.to_string(), style, marks)),
                }
                v += 1;
            }
        }
    }
    RichText::new(out)
}

/// Replaces every non-overlapping match of `find` in the printed text.
/// Returns `None` when nothing matches. `find` must not be empty.
pub fn replace_in(
    rt: &RichText,
    find: &str,
    with: &str,
    match_case: bool,
    tracked: Option<&Tracked<'_>>,
) -> Option<(RichText, usize)> {
    let starts = find_in(rt, find, match_case);
    if starts.is_empty() {
        return None;
    }
    let len = find.chars().count();
    Some((replace_at(rt, &starts, len, with, tracked), starts.len()))
}

/// Replaces only the match that starts at character offset `from`, if the
/// matches of `find` include one starting there.
pub fn replace_one(
    rt: &RichText,
    find: &str,
    with: &str,
    match_case: bool,
    from: usize,
    tracked: Option<&Tracked<'_>>,
) -> Option<RichText> {
    if !find_in(rt, find, match_case).contains(&from) {
        return None;
    }
    Some(replace_at(rt, &[from], find.chars().count(), with, tracked))
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Style = Style {
        bold: true,
        italic: false,
        underline: false,
    };

    fn marks(ins: Option<u64>, del: Option<u64>) -> Marks {
        Marks {
            ins: ins.map(ChangeId),
            del: del.map(ChangeId),
            ..Marks::NONE
        }
    }

    #[test]
    fn replaces_plain_text() {
        let (t, n) = replace_in(
            &RichText::plain("MAYA and Maya"),
            "MAYA",
            "NINA",
            true,
            None,
        )
        .unwrap();
        assert_eq!((t.plain_text().as_str(), n), ("NINA and Maya", 1));
        let (t, n) = replace_in(
            &RichText::plain("MAYA and Maya"),
            "maya",
            "Nina",
            false,
            None,
        )
        .unwrap();
        assert_eq!((t.plain_text().as_str(), n), ("Nina and Nina", 2));
        assert!(replace_in(&RichText::plain("abc"), "x", "y", true, None).is_none());
    }

    #[test]
    fn keeps_style_and_moves_hidden_runs() {
        let rt = RichText::new(vec![
            Run::plain("a "),
            Run::styled("MA", B),
            Run::Note("n".into()),
            Run::plain("YA b"),
        ]);
        let (t, _) = replace_in(&rt, "MAYA", "NINA", true, None).unwrap();
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
    fn finds_and_replaces_one() {
        let rt = RichText::plain("aXa xa XA");
        assert_eq!(find_in(&rt, "xa", false), vec![1, 4, 7]);
        assert_eq!(find_in(&rt, "xa", true), vec![4]);
        assert!(find_in(&rt, "", true).is_empty());
        let t = replace_one(&rt, "xa", "yy", false, 4, None).unwrap();
        assert_eq!(t.plain_text(), "aXa yy XA");
        assert!(replace_one(&rt, "xa", "yy", false, 5, None).is_none());
    }

    #[test]
    fn a_tracked_replacement_strikes_the_old_text_and_inserts_after_it() {
        let rt = RichText::plain("she walks home");
        let own = |_: ChangeId| false;
        let t = Tracked {
            change: ChangeId(3),
            own: &own,
        };
        let (out, n) = replace_in(&rt, "walks", "runs", true, Some(&t)).unwrap();
        assert_eq!(n, 1);
        assert_eq!(out.plain_text(), "she runs home");
        assert_eq!(
            out.runs(),
            &[
                Run::plain("she "),
                Run::marked("walks", Style::PLAIN, marks(None, Some(3))),
                Run::marked("runs", Style::PLAIN, marks(Some(3), None)),
                Run::plain(" home"),
            ]
        );
        // Matching skips struck text: the old words are no longer found.
        assert!(find_in(&out, "walks", true).is_empty());
        assert_eq!(find_in(&out, "runs", true), vec![4]);
        // Replacing the author's own pending insertion drops it outright.
        let mine = |c: ChangeId| c == ChangeId(3);
        let again = Tracked {
            change: ChangeId(3),
            own: &mine,
        };
        let (out, _) = replace_in(&out, "runs", "jogs", true, Some(&again)).unwrap();
        assert_eq!(out.plain_text(), "she jogs home");
        assert!(!out.plain_text().contains("runs"));
        assert_eq!(
            out.runs()[2],
            Run::marked("jogs", Style::PLAIN, marks(Some(3), None))
        );
    }

    #[test]
    fn deletes_and_does_not_overlap() {
        let (t, n) = replace_in(&RichText::plain("aaaa"), "aa", "", true, None).unwrap();
        assert_eq!((t.plain_text().as_str(), n), ("", 2));
        let (t, n) = replace_in(&RichText::plain("aaa"), "aa", "b", true, None).unwrap();
        assert_eq!((t.plain_text().as_str(), n), ("ba", 1));
    }
}
