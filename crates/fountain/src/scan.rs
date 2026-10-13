//! Finds notes (`[[ ]]`) and boneyard (`/* */`) before any classification
//! (spec §5). Each becomes one hidden atom on the line where it starts.

use coldopen_screenplay::Run;

use crate::Diagnostic;
use crate::line::{Atom, Line};

pub(crate) struct Scanned {
    pub lines: Vec<Line>,
    /// 1-based source line number where each logical line starts.
    pub line_nos: Vec<usize>,
}

/// A line inside a note that ends the note search: empty, or a single
/// whitespace character. Two or more whitespace characters hold the note
/// together instead.
fn is_blank(chars: &[char]) -> bool {
    chars.iter().all(|c| c.is_whitespace()) && chars.len() < 2
}

pub(crate) fn scan(lines: &[&str], first_line_no: usize, diags: &mut Vec<Diagnostic>) -> Scanned {
    let mut chars: Vec<char> = Vec::new();
    for (k, l) in lines.iter().enumerate() {
        if k > 0 {
            chars.push('\n');
        }
        chars.extend(l.chars());
    }

    let mut out = Scanned {
        lines: vec![Vec::new()],
        line_nos: vec![first_line_no],
    };
    if lines.is_empty() {
        out.lines.clear();
        out.line_nos.clear();
        return out;
    }

    let mut line_no = first_line_no;
    // Searches that failed once fail again from any later start before
    // these points, so remember them to keep scanning linear.
    let mut boneyard_fails_from: Option<usize> = None;
    let mut note_fails_until: usize = 0;

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\n' => {
                out.lines.push(Vec::new());
                line_no += 1;
                out.line_nos.push(line_no);
                i += 1;
            }
            '\\' if next.is_some_and(|n| n.is_ascii_punctuation()) => {
                push(&mut out, Atom::Ch('\\'));
                push(&mut out, Atom::Ch(chars[i + 1]));
                i += 2;
            }
            '/' if next == Some('*') => {
                let found = if boneyard_fails_from.is_some_and(|p| i >= p) {
                    None
                } else {
                    find(&chars, i + 2, &['*', '/'])
                };
                match found {
                    Some(j) => {
                        let content: String = chars[i + 2..j].iter().collect();
                        line_no += content.matches('\n').count();
                        push(&mut out, Atom::Hidden(Run::Boneyard(content)));
                        i = j + 2;
                    }
                    None => {
                        boneyard_fails_from.get_or_insert(i);
                        diags.push(Diagnostic::new(
                            line_no,
                            "unclosed_boneyard",
                            "/* has no closing */, so it is kept as text",
                        ));
                        push(&mut out, Atom::Ch('/'));
                        i += 1;
                    }
                }
            }
            '[' if next == Some('[') => {
                let found = if i < note_fails_until {
                    None
                } else {
                    match find_note_end(&chars, i + 2) {
                        Ok(j) => Some(j),
                        Err(stop) => {
                            note_fails_until = stop;
                            None
                        }
                    }
                };
                match found {
                    Some(j) => {
                        let raw: String = chars[i + 2..j].iter().collect();
                        line_no += raw.matches('\n').count();
                        push(&mut out, Atom::Hidden(Run::Note(note_text(&raw))));
                        i = j + 2;
                    }
                    None => {
                        diags.push(Diagnostic::new(
                            line_no,
                            "unclosed_note",
                            "[[ has no closing ]] before an empty line, so it is kept as text",
                        ));
                        push(&mut out, Atom::Ch('['));
                        i += 1;
                    }
                }
            }
            _ => {
                push(&mut out, Atom::Ch(c));
                i += 1;
            }
        }
    }
    out
}

fn push(out: &mut Scanned, a: Atom) {
    if let Some(l) = out.lines.last_mut() {
        l.push(a);
    }
}

/// Index of the first occurrence of `pat` at or after `from`.
fn find(chars: &[char], from: usize, pat: &[char; 2]) -> Option<usize> {
    if from >= chars.len() {
        return None;
    }
    chars[from..]
        .windows(2)
        .position(|w| w == pat)
        .map(|p| p + from)
}

/// Finds the `]]` closing a note. A blank line ends the search; the error
/// carries the position where it stopped.
fn find_note_end(chars: &[char], from: usize) -> Result<usize, usize> {
    let mut k = from;
    while k < chars.len() {
        if chars[k] == ']' && chars.get(k + 1) == Some(&']') {
            return Ok(k);
        }
        if chars[k] == '\n' {
            let end = chars[k + 1..]
                .iter()
                .position(|&c| c == '\n')
                .map_or(chars.len(), |p| p + k + 1);
            if is_blank(&chars[k + 1..end]) {
                return Err(k);
            }
        }
        k += 1;
    }
    Err(chars.len())
}

/// Note content: hold lines (two or more whitespace characters) become
/// empty lines.
fn note_text(raw: &str) -> String {
    raw.split('\n')
        .map(|l| {
            if !l.is_empty() && l.chars().all(char::is_whitespace) {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line::visible;

    fn run(lines: &[&str]) -> (Scanned, Vec<Diagnostic>) {
        let mut d = Vec::new();
        let s = scan(lines, 1, &mut d);
        (s, d)
    }

    #[test]
    fn boneyard_spans_blank_lines() {
        let (s, d) = run(&["a /* x", "", "y */ b", "c"]);
        assert!(d.is_empty());
        assert_eq!(s.lines.len(), 2);
        assert_eq!(visible(&s.lines[0]), "a  b");
        assert_eq!(s.line_nos, vec![1, 4]);
        assert!(s.lines[0].contains(&Atom::Hidden(Run::Boneyard(" x\n\ny ".into()))));
    }

    #[test]
    fn note_stops_at_blank_line() {
        let (s, d) = run(&["a [[x", "", "y]]"]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, "unclosed_note");
        assert_eq!(visible(&s.lines[0]), "a [[x");
    }

    #[test]
    fn note_holds_across_two_spaces() {
        let (s, d) = run(&["a[[x", "  ", "y]]b"]);
        assert!(d.is_empty());
        assert_eq!(s.lines.len(), 1);
        assert!(s.lines[0].contains(&Atom::Hidden(Run::Note("x\n\ny".into()))));
    }

    #[test]
    fn escapes_block_openers() {
        let (s, d) = run(&[r"a \[[x]] /\* y"]);
        assert!(d.is_empty());
        assert!(!crate::line::has_hidden(&s.lines[0]));
    }

    #[test]
    fn many_unclosed_openers_stay_linear() {
        let line = "/*[[".repeat(20_000);
        let (s, d) = run(&[&line]);
        assert_eq!(visible(&s.lines[0]), line);
        assert_eq!(d.len(), 40_000);
    }
}
