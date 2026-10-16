//! Inline text: backslash escapes and emphasis (spec §8).
//!
//! Emphasis follows the CommonMark delimiter-run algorithm, limited to `*`
//! (italic, bold) and `_` (underline). It runs on one line at a time.

use coldopen_screenplay::{RichText, Run, Style};

use crate::line::{Atom, Line};

fn is_ws(c: Option<char>) -> bool {
    c.is_none_or(char::is_whitespace)
}

/// CommonMark counts Unicode punctuation and symbols; anything that is
/// neither alphanumeric nor whitespace is close enough for screenplays.
fn is_punct(c: Option<char>) -> bool {
    c.is_some_and(|c| !c.is_alphanumeric() && !c.is_whitespace())
}

enum Tok {
    Text(String),
    Delim {
        ch: char,
        count: usize,
        orig: usize,
        can_open: bool,
        can_close: bool,
    },
    Hidden(Run),
}

struct T {
    tok: Tok,
    style: Style,
}

fn prev_char(line: &[Atom], i: usize) -> Option<char> {
    line[..i].iter().rev().find_map(|a| match a {
        Atom::Ch(c) => Some(*c),
        Atom::Hidden(_) => None,
    })
}

fn next_char(line: &[Atom], j: usize) -> Option<char> {
    line[j..].iter().find_map(|a| match a {
        Atom::Ch(c) => Some(*c),
        Atom::Hidden(_) => None,
    })
}

fn tokenize(line: &[Atom]) -> Vec<T> {
    let mut toks: Vec<T> = Vec::new();
    let push_text = |toks: &mut Vec<T>, c: char| {
        if let Some(T {
            tok: Tok::Text(s), ..
        }) = toks.last_mut()
        {
            s.push(c);
        } else {
            toks.push(T {
                tok: Tok::Text(c.to_string()),
                style: Style::PLAIN,
            });
        }
    };
    let mut i = 0;
    while i < line.len() {
        match &line[i] {
            Atom::Hidden(r) => {
                toks.push(T {
                    tok: Tok::Hidden(r.clone()),
                    style: Style::PLAIN,
                });
                i += 1;
            }
            Atom::Ch('\\') => match line.get(i + 1) {
                Some(Atom::Ch(p)) if p.is_ascii_punctuation() => {
                    push_text(&mut toks, *p);
                    i += 2;
                }
                _ => {
                    push_text(&mut toks, '\\');
                    i += 1;
                }
            },
            Atom::Ch(c @ ('*' | '_')) => {
                let c = *c;
                let mut j = i;
                while j < line.len() && line[j] == Atom::Ch(c) {
                    j += 1;
                }
                let before = prev_char(line, i);
                let after = next_char(line, j);
                let left = !is_ws(after) && (!is_punct(after) || is_ws(before) || is_punct(before));
                let right =
                    !is_ws(before) && (!is_punct(before) || is_ws(after) || is_punct(after));
                let (can_open, can_close) = if c == '*' {
                    (left, right)
                } else {
                    (
                        left && (!right || is_punct(before)),
                        right && (!left || is_punct(after)),
                    )
                };
                toks.push(T {
                    tok: Tok::Delim {
                        ch: c,
                        count: j - i,
                        orig: j - i,
                        can_open,
                        can_close,
                    },
                    style: Style::PLAIN,
                });
                i = j;
            }
            Atom::Ch(c) => {
                push_text(&mut toks, *c);
                i += 1;
            }
        }
    }
    toks
}

fn delim(t: &T) -> Option<(char, usize, usize, bool, bool)> {
    match t.tok {
        Tok::Delim {
            ch,
            count,
            orig,
            can_open,
            can_close,
        } => Some((ch, count, orig, can_open, can_close)),
        _ => None,
    }
}

fn take(t: &mut T, n: usize) {
    if let Tok::Delim { count, .. } = &mut t.tok {
        *count -= n;
    }
}

fn apply(style: &mut Style, ch: char, n: usize) {
    match (ch, n) {
        ('_', _) => style.underline = true,
        (_, 1) => style.italic = true,
        _ => style.bold = true,
    }
}

/// The CommonMark "process emphasis" procedure over one line's tokens.
///
/// Linear in the line's length (review finding F15): the delimiter stack
/// is a linked list, so removing a delimiter is constant time (a `Vec`
/// shifted its tail on every removal, which made a long line of unmatched
/// emphasis quadratic), and the styles of matched pairs are applied at the
/// end in one pass, with counts that rise at each opener and fall at each
/// closer, rather than token by token for every pair.
fn process_emphasis(toks: &mut [T]) {
    const NONE: usize = usize::MAX;
    let delims: Vec<usize> = (0..toks.len())
        .filter(|&i| delim(&toks[i]).is_some())
        .collect();
    let n = delims.len();
    let mut prev: Vec<usize> = (0..n).map(|i| i.checked_sub(1).unwrap_or(NONE)).collect();
    let mut next: Vec<usize> = (0..n)
        .map(|i| if i + 1 < n { i + 1 } else { NONE })
        .collect();
    let unlink = |i: usize, prev: &mut [usize], next: &mut [usize]| {
        let (p, q) = (prev[i], next[i]);
        if p != NONE {
            next[p] = q;
        }
        if q != NONE {
            prev[q] = p;
        }
    };
    // Lowest token index an opener may have, per (char, closer can open, orig % 3).
    let mut bottom = [[[0usize; 3]; 2]; 2];
    // Per style (underline, italic, bold): +1 after each opener, -1 at each
    // closer; a token is styled where the running sum is above zero.
    let mut rise = vec![[0i32; 3]; toks.len() + 1];

    let mut ci = if n > 0 { 0 } else { NONE };
    while ci != NONE {
        let c_tok = delims[ci];
        let Some((ch, _, c_orig, c_open, c_close)) = delim(&toks[c_tok]) else {
            ci = next[ci];
            continue;
        };
        if !c_close {
            ci = next[ci];
            continue;
        }
        let key = (usize::from(ch == '_'), usize::from(c_open), c_orig % 3);
        let floor = bottom[key.0][key.1][key.2];

        let mut found = None;
        let mut oi = prev[ci];
        while oi != NONE {
            let o_tok = delims[oi];
            if o_tok < floor {
                break;
            }
            if let Some((och, ocount, oorig, oopen, oclose)) = delim(&toks[o_tok])
                && och == ch
                && oopen
                && ocount > 0
            {
                let odd = (oclose || c_open)
                    && (oorig + c_orig) % 3 == 0
                    && !(oorig % 3 == 0 && c_orig % 3 == 0);
                if !odd {
                    found = Some(oi);
                    break;
                }
            }
            oi = prev[oi];
        }

        match found {
            Some(oi) => {
                let o_tok = delims[oi];
                let ocount = delim(&toks[o_tok]).map_or(0, |d| d.1);
                let ccount = delim(&toks[c_tok]).map_or(0, |d| d.1);
                let used = if ocount >= 2 && ccount >= 2 { 2 } else { 1 };
                let flag = match (ch, used) {
                    ('_', _) => 0,
                    (_, 1) => 1,
                    _ => 2,
                };
                rise[o_tok + 1][flag] += 1;
                rise[c_tok][flag] -= 1;
                take(&mut toks[o_tok], used);
                take(&mut toks[c_tok], used);
                // Delimiters between the pair can no longer match.
                let mut k = next[oi];
                while k != ci && k != NONE {
                    let after = next[k];
                    unlink(k, &mut prev, &mut next);
                    k = after;
                }
                if delim(&toks[o_tok]).map_or(0, |d| d.1) == 0 {
                    unlink(oi, &mut prev, &mut next);
                }
                if delim(&toks[c_tok]).map_or(0, |d| d.1) == 0 {
                    let after = next[ci];
                    unlink(ci, &mut prev, &mut next);
                    ci = after;
                }
            }
            None => {
                bottom[key.0][key.1][key.2] = c_tok;
                let after = next[ci];
                if !c_open {
                    unlink(ci, &mut prev, &mut next);
                }
                ci = after;
            }
        }
    }

    let mut level = [0i32; 3];
    for (t, r) in toks.iter_mut().zip(&rise) {
        for (l, d) in level.iter_mut().zip(r) {
            *l += d;
        }
        for (ch, used, on) in [('_', 1, level[0]), ('*', 1, level[1]), ('*', 2, level[2])] {
            if on > 0 {
                apply(&mut t.style, ch, used);
            }
        }
    }
}

/// Parses one visible line into runs.
pub(crate) fn parse_line(line: &[Atom]) -> Vec<Run> {
    let mut toks = tokenize(line);
    process_emphasis(&mut toks);
    let mut runs = Vec::with_capacity(toks.len());
    for t in toks {
        match t.tok {
            Tok::Text(s) => runs.push(Run::styled(s, t.style)),
            Tok::Delim { ch, count, .. } if count > 0 => {
                runs.push(Run::styled(ch.to_string().repeat(count), t.style));
            }
            Tok::Delim { .. } => {}
            Tok::Hidden(r) => runs.push(r),
        }
    }
    runs
}

/// Parses lines into one rich text, joining them with line breaks.
pub(crate) fn parse_lines(lines: &[Line]) -> RichText {
    let mut runs = Vec::new();
    for (k, l) in lines.iter().enumerate() {
        if k > 0 {
            runs.push(Run::plain("\n"));
        }
        runs.extend(parse_line(l));
    }
    RichText::new(runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line::from_str;

    const I: Style = Style {
        bold: false,
        italic: true,
        underline: false,
    };
    const B: Style = Style {
        bold: true,
        italic: false,
        underline: false,
    };
    const BI: Style = Style {
        bold: true,
        italic: true,
        underline: false,
    };
    const U: Style = Style {
        bold: false,
        italic: false,
        underline: true,
    };

    fn p(s: &str) -> Vec<Run> {
        RichText::new(parse_line(&from_str(s))).into_runs()
    }

    #[test]
    fn basic_styles() {
        assert_eq!(p("*a*"), vec![Run::styled("a", I)]);
        assert_eq!(p("**a**"), vec![Run::styled("a", B)]);
        assert_eq!(p("***a***"), vec![Run::styled("a", BI)]);
        assert_eq!(p("_a_"), vec![Run::styled("a", U)]);
    }

    #[test]
    fn mixed_nesting() {
        assert_eq!(
            p("**a*b***"),
            vec![Run::styled("a", B), Run::styled("b", BI)]
        );
        assert_eq!(
            p("*a **b** c*"),
            vec![
                Run::styled("a ", I),
                Run::styled("b", BI),
                Run::styled(" c", I)
            ]
        );
    }

    #[test]
    fn flanking() {
        assert_eq!(p("a * b *"), vec![Run::plain("a * b *")]);
        assert_eq!(p("snake_case_name"), vec![Run::plain("snake_case_name")]);
        assert_eq!(
            p("in*side*word"),
            vec![Run::plain("in"), Run::styled("side", I), Run::plain("word")]
        );
    }

    #[test]
    fn escapes() {
        assert_eq!(p(r"\*a\*"), vec![Run::plain("*a*")]);
        assert_eq!(p(r"a\b"), vec![Run::plain(r"a\b")]);
        assert_eq!(p(r"\\"), vec![Run::plain(r"\")]);
        assert_eq!(p(r"\\*a*"), vec![Run::plain(r"\"), Run::styled("a", I)]);
    }

    #[test]
    fn unmatched_is_literal() {
        assert_eq!(p("**a"), vec![Run::plain("**a")]);
        assert_eq!(p("a**"), vec![Run::plain("a**")]);
        assert_eq!(p("****"), vec![Run::plain("****")]);
    }

    #[test]
    fn emphasis_around_hidden_atom() {
        let mut line = from_str("*a");
        line.push(Atom::Hidden(Run::Note("n".into())));
        line.extend(from_str("b*"));
        assert_eq!(
            RichText::new(parse_line(&line)).into_runs(),
            vec![
                Run::styled("a", I),
                Run::Note("n".into()),
                Run::styled("b", I)
            ]
        );
    }
}
