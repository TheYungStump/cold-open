//! Logical lines: visible characters with hidden runs (notes, boneyard) kept
//! in place as single atoms.

use coldopen_screenplay::Run;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Atom {
    Ch(char),
    /// A `Run::Note` or `Run::Boneyard`.
    Hidden(Run),
}

pub(crate) type Line = Vec<Atom>;

#[cfg(test)]
pub(crate) fn from_str(s: &str) -> Line {
    s.chars().map(Atom::Ch).collect()
}

/// The visible characters of a line, hidden atoms skipped.
pub(crate) fn visible(line: &[Atom]) -> String {
    line.iter()
        .filter_map(|a| match a {
            Atom::Ch(c) => Some(*c),
            Atom::Hidden(_) => None,
        })
        .collect()
}

pub(crate) fn has_hidden(line: &[Atom]) -> bool {
    line.iter().any(|a| matches!(a, Atom::Hidden(_)))
}

/// Removes leading whitespace characters, keeping any hidden atoms among them.
pub(crate) fn trim_start(line: &mut Line) {
    let first = line
        .iter()
        .position(|a| matches!(a, Atom::Ch(c) if !c.is_whitespace()))
        .unwrap_or(line.len());
    let mut i = 0;
    line.retain(|a| {
        let keep = i >= first || matches!(a, Atom::Hidden(_));
        i += 1;
        keep
    });
}

/// Removes trailing whitespace characters, keeping any hidden atoms among them.
pub(crate) fn trim_end(line: &mut Line) {
    let last = line
        .iter()
        .rposition(|a| matches!(a, Atom::Ch(c) if !c.is_whitespace()))
        .map_or(0, |p| p + 1);
    let mut i = 0;
    line.retain(|a| {
        let keep = i < last || matches!(a, Atom::Hidden(_));
        i += 1;
        keep
    });
}

pub(crate) fn trim(line: &mut Line) {
    trim_start(line);
    trim_end(line);
}

/// Removes the first `n` visible characters, keeping hidden atoms.
pub(crate) fn drop_first_visible(line: &mut Line, n: usize) {
    let mut seen = 0;
    line.retain(|a| match a {
        Atom::Ch(_) if seen < n => {
            seen += 1;
            false
        }
        _ => true,
    });
}

/// Removes the last `n` visible characters, keeping hidden atoms.
pub(crate) fn drop_last_visible(line: &mut Line, n: usize) {
    let total = line.iter().filter(|a| matches!(a, Atom::Ch(_))).count();
    let keep = total.saturating_sub(n);
    let mut seen = 0;
    line.retain(|a| match a {
        Atom::Ch(_) => {
            seen += 1;
            seen <= keep
        }
        Atom::Hidden(_) => true,
    });
}

/// Replaces each tab with four spaces.
pub(crate) fn expand_tabs(line: &mut Line) {
    if !line.contains(&Atom::Ch('\t')) {
        return;
    }
    let mut out = Vec::with_capacity(line.len() + 8);
    for a in line.drain(..) {
        if a == Atom::Ch('\t') {
            out.extend(std::iter::repeat_n(Atom::Ch(' '), 4));
        } else {
            out.push(a);
        }
    }
    *line = out;
}

/// True when the character at `i` is preceded by an odd number of
/// backslashes, so it is escaped.
pub(crate) fn is_escaped(chars: &[char], i: usize) -> bool {
    let mut n = 0;
    let mut j = i;
    while j > 0 && chars[j - 1] == '\\' {
        n += 1;
        j -= 1;
    }
    n % 2 == 1
}

/// Splits text into lines at CRLF, LF or a lone CR. A final terminator does
/// not start an extra empty line.
pub(crate) fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                lines.push(&text[start..i]);
                i += 1;
                start = i;
            }
            b'\r' => {
                lines.push(&text[start..i]);
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(s: &str) -> Atom {
        Atom::Hidden(Run::Note(s.into()))
    }

    #[test]
    fn trimming_keeps_hidden_atoms() {
        let mut l = vec![
            Atom::Ch(' '),
            note("a"),
            Atom::Ch(' '),
            Atom::Ch('x'),
            Atom::Ch(' '),
            note("b"),
            Atom::Ch(' '),
        ];
        trim(&mut l);
        assert_eq!(l, vec![note("a"), Atom::Ch('x'), note("b")]);
    }

    #[test]
    fn drop_visible() {
        let mut l = from_str("@BOB");
        drop_first_visible(&mut l, 1);
        assert_eq!(visible(&l), "BOB");
        let mut l = vec![Atom::Ch('a'), Atom::Ch('b'), note("n"), Atom::Ch('c')];
        drop_last_visible(&mut l, 2);
        assert_eq!(l, vec![Atom::Ch('a'), note("n")]);
    }

    #[test]
    fn splits_all_line_endings() {
        assert_eq!(split_lines("a\r\nb\rc\nd"), vec!["a", "b", "c", "d"]);
        assert_eq!(split_lines("a\n"), vec!["a"]);
        assert_eq!(split_lines("a\n\n"), vec!["a", ""]);
        assert_eq!(split_lines(""), Vec::<&str>::new());
        assert_eq!(split_lines("\r\r"), vec!["", ""]);
    }

    #[test]
    fn escapes() {
        let c: Vec<char> = r"a\^b\\^".chars().collect();
        assert!(is_escaped(&c, 2));
        assert!(!is_escaped(&c, 6));
    }
}
