//! Greedy wrapping on the cell grid (layout.md §5.2).
#![deny(clippy::float_arithmetic)]

use crate::text::{CellKind, Text};

/// One wrapped line: a range of cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    /// First cell of the line.
    pub start: u32,
    /// One past the last printed cell; trailing spaces are excluded.
    pub end: u32,
    /// Where the next line starts, after any spaces or line break consumed
    /// by this break.
    pub next: u32,
    /// Cells used.
    pub width: u32,
    /// The paragraph's first line, which takes the first-line indent.
    pub first: bool,
    /// Ended by a line break in the text.
    pub hard: bool,
}

/// How many cells a line holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Measure {
    pub first: u32,
    pub rest: u32,
    pub hyphens: bool,
    /// A hyphen ending a line may take one cell past `first`/`rest`.
    pub overhang: bool,
}

fn width_of(t: &Text, start: u32, end: u32) -> u32 {
    t.cells
        .get(start as usize..end as usize)
        .map_or(0, |cs| cs.iter().map(|c| u32::from(c.width)).sum())
}

/// Trims trailing spaces from `[start, end)`.
fn trim_end(t: &Text, start: u32, mut end: u32) -> u32 {
    while end > start
        && t.cells
            .get(end as usize - 1)
            .is_some_and(|c| c.kind == CellKind::Space)
    {
        end -= 1;
    }
    end
}

/// Wraps `t` from cell `from`. `first_line` says whether the line starting
/// at `from` is the paragraph's first line. An empty paragraph gives one
/// empty line; a start at or past the end gives no lines.
pub(crate) fn wrap(t: &Text, from: u32, first_line: bool, m: Measure) -> Vec<Line> {
    wrap_at_most(t, from, first_line, m, usize::MAX)
}

/// The first `limit` lines of [`wrap`], without wrapping the rest.
pub(crate) fn wrap_at_most(
    t: &Text,
    from: u32,
    first_line: bool,
    m: Measure,
    limit: usize,
) -> Vec<Line> {
    let n = t.len();
    let mut lines = Vec::new();
    if n == 0 {
        if from == 0 {
            lines.push(Line {
                start: 0,
                end: 0,
                next: 0,
                width: 0,
                first: true,
                hard: false,
            });
        }
        return lines;
    }
    let mut i = from;
    let mut first = first_line;
    while i < n && lines.len() < limit {
        let cap = if first { m.first } else { m.rest }.max(1);
        let line = next_line(t, i, cap, m.hyphens, m.overhang);
        let (end, next, hard) = line;
        let end = trim_end(t, i, end);
        lines.push(Line {
            start: i,
            end,
            next,
            width: width_of(t, i, end),
            first,
            hard,
        });
        // A line break at the very end leaves one empty line after it.
        if hard && next >= n && lines.len() < limit {
            lines.push(Line {
                start: n,
                end: n,
                next: n,
                width: 0,
                first: false,
                hard: false,
            });
        }
        first = false;
        i = next;
    }
    lines
}

/// Lines that start at the given cells instead of where wrapping would put
/// them; the first line always starts at cell 0. Only the reference
/// comparison asks for this (layout.md §13, off-grid lines).
pub(crate) fn lines_at(t: &Text, starts: &[u32]) -> Vec<Line> {
    let n = t.len();
    let mut s: Vec<u32> = std::iter::once(0)
        .chain(starts.iter().copied().filter(|&c| c > 0 && c < n))
        .collect();
    s.sort_unstable();
    s.dedup();
    if n == 0 {
        return vec![Line {
            start: 0,
            end: 0,
            next: 0,
            width: 0,
            first: true,
            hard: false,
        }];
    }
    s.iter()
        .enumerate()
        .map(|(k, &start)| {
            let next = s.get(k + 1).copied().unwrap_or(n);
            let hard = next > start
                && t.cells
                    .get(next as usize - 1)
                    .is_some_and(|c| c.kind == CellKind::Newline);
            let end = trim_end(t, start, if hard { next - 1 } else { next });
            Line {
                start,
                end,
                next,
                width: width_of(t, start, end),
                first: k == 0,
                hard,
            }
        })
        .collect()
}

/// Finds the end of one line starting at `start`: (end, next, hard).
fn next_line(t: &Text, start: u32, cap: u32, hyphens: bool, overhang: bool) -> (u32, u32, bool) {
    let n = t.len();
    let mut w: u32 = 0;
    // The last place the line may end: (end, next).
    let mut brk: Option<(u32, u32)> = None;
    let mut j = start;
    // Spaces at the start of the line are indentation and count as cells.
    let mut indent = true;
    while j < n {
        let c = t.cells[j as usize];
        match c.kind {
            CellKind::Newline => return (j, j + 1, true),
            CellKind::Space if !indent => {
                let mut run_end = j;
                let mut run_w = 0;
                while run_end < n && t.cells[run_end as usize].kind == CellKind::Space {
                    run_w += u32::from(t.cells[run_end as usize].width);
                    run_end += 1;
                }
                // A break here drops the spaces; if the next word does not fit,
                // this is where the line ends.
                brk = Some((j, run_end));
                if w + run_w > cap {
                    return (j, run_end, false);
                }
                w += run_w;
                j = run_end;
            }
            _ => {
                if c.kind == CellKind::Glyph {
                    indent = false;
                }
                let cw = u32::from(c.width);
                if overhang && w + cw == cap + 1 && t.is_hyphen(j as usize) {
                    // A hyphen may hang one cell past the measure when the
                    // line ends right after it: at the end, before a space,
                    // or at a hyphen break.
                    let next = t.cells.get(j as usize + 1);
                    let ends = next.is_none_or(|n| n.kind != CellKind::Glyph)
                        || (hyphens
                            && j > start
                            && t.is_alnum(j as usize - 1)
                            && t.is_alpha(j as usize + 1));
                    if ends {
                        let mut nx = j + 1;
                        while nx < n && t.cells[nx as usize].kind == CellKind::Space {
                            nx += 1;
                        }
                        let hard = nx < n && t.cells[nx as usize].kind == CellKind::Newline;
                        return (j + 1, if hard { nx + 1 } else { nx }, hard);
                    }
                }
                if w + cw > cap {
                    if let Some((e, nx)) = brk {
                        return (e, nx, false);
                    }
                    if j > start {
                        // A word longer than the line breaks at the last whole
                        // cluster that fits.
                        return (j, j, false);
                    }
                    // A single cluster wider than the line takes the line.
                    return (j + 1, j + 1, false);
                }
                w += cw;
                if hyphens
                    && t.is_hyphen(j as usize)
                    && j > start
                    && t.is_alnum(j as usize - 1)
                    && t.is_alpha(j as usize + 1)
                {
                    brk = Some((j + 1, j + 1));
                }
                j += 1;
            }
        }
    }
    (n, n, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Look, build_plain};

    fn lines(s: &str, first: u32, rest: u32) -> Vec<String> {
        let t = build_plain(s, false, Look::default());
        wrap(
            &t,
            0,
            true,
            Measure {
                first,
                rest,
                hyphens: true,
                overhang: false,
            },
        )
        .iter()
        .map(|l| {
            t.cells[l.start as usize..l.end as usize]
                .iter()
                .map(|c| t.cell_text(c))
                .collect::<String>()
        })
        .collect()
    }

    #[test]
    fn greedy_words() {
        assert_eq!(lines("aaa bbb ccc", 7, 7), vec!["aaa bbb", "ccc"]);
        assert_eq!(lines("aaa bbb ccc", 6, 6), vec!["aaa", "bbb", "ccc"]);
        assert_eq!(lines("aaa  bbb", 8, 8), vec!["aaa  bbb"]);
        assert_eq!(lines("aaa   ", 3, 3), vec!["aaa"]);
    }

    #[test]
    fn first_line_measure() {
        assert_eq!(lines("aa bb cc dd", 5, 8), vec!["aa bb", "cc dd"]);
        assert_eq!(lines("aa bb cc dd", 2, 8), vec!["aa", "bb cc dd"]);
    }

    #[test]
    fn hyphens_and_long_words() {
        assert_eq!(
            lines("well-known fact", 6, 6),
            vec!["well-", "known", "fact"]
        );
        assert_eq!(lines("abcdefghij", 4, 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(lines("a -b", 3, 3), vec!["a", "-b"]);
        assert_eq!(lines("2-3 x", 2, 2), vec!["2-", "3", "x"]);
    }

    #[test]
    fn line_breaks_and_indentation() {
        assert_eq!(lines("a\nb", 9, 9), vec!["a", "b"]);
        assert_eq!(lines("a\n\nb", 9, 9), vec!["a", "", "b"]);
        assert_eq!(lines("a\n", 9, 9), vec!["a", ""]);
        assert_eq!(lines("    indented", 20, 20), vec!["    indented"]);
        assert_eq!(lines("x\n  y z", 4, 4), vec!["x", "  y", "z"]);
        assert_eq!(lines("", 9, 9), vec![""]);
    }

    #[test]
    fn hyphen_overhang() {
        let t = build_plain("ab cd-- ef well-known", false, Look::default());
        let m = Measure {
            first: 6,
            rest: 6,
            hyphens: true,
            overhang: true,
        };
        let got: Vec<String> = wrap(&t, 0, true, m)
            .iter()
            .map(|l| {
                t.cells[l.start as usize..l.end as usize]
                    .iter()
                    .map(|c| t.cell_text(c))
                    .collect()
            })
            .collect();
        // "ab cd--" is 7 cells: the last hyphen hangs. "well-" breaks at the
        // hyphen without hanging; "known" is 5 cells.
        assert_eq!(got, vec!["ab cd--", "ef", "well-", "known"]);
    }

    #[test]
    fn wide_clusters() {
        assert_eq!(lines("日本語", 4, 4), vec!["日本", "語"]);
        assert_eq!(lines("日", 1, 1), vec!["日"]);
    }

    #[test]
    fn rewrap_from_offset() {
        let t = build_plain("aaa bbb ccc", false, Look::default());
        let m = Measure {
            first: 5,
            rest: 7,
            hyphens: false,
            overhang: false,
        };
        let from4 = wrap(&t, 4, false, m);
        assert_eq!(from4[0].start, 4);
        assert_eq!(from4.len(), 1);
        assert!(!from4[0].first);
    }
}
