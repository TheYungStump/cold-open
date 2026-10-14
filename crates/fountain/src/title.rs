//! Title page detection (spec §4).

use coldopen_screenplay::{RichText, TitleField, TitlePage};

use crate::classify::is_scene_heading_prefix;
use crate::{Diagnostic, inline, line, scan};

/// Splits a key line into key and value. The key starts with a letter or
/// digit, holds only letters, digits, spaces and `-_.'/&`, and does not end
/// in a space.
pub(crate) fn key_line(l: &str) -> Option<(&str, &str)> {
    let (key, value) = l.split_once(':')?;
    let mut chars = key.chars();
    let first = chars.next()?;
    if !first.is_alphanumeric() || key.ends_with(' ') {
        return None;
    }
    if !key
        .chars()
        .all(|c| c.is_alphanumeric() || " -_.'/&".contains(c))
    {
        return None;
    }
    Some((key, value.trim()))
}

fn continuation(l: &str) -> Option<&str> {
    if (l.starts_with("   ") || l.starts_with('\t')) && !l.trim().is_empty() {
        Some(l.trim())
    } else {
        None
    }
}

/// Finds the title page at the start of `lines`. Returns it and the index
/// of the first line after it, or `None` when the file has no title page.
pub(crate) fn detect(lines: &[&str]) -> Option<(TitlePage, usize)> {
    let start = lines.iter().position(|l| !l.trim().is_empty())?;
    let end = lines[start..]
        .iter()
        .position(|l| l.trim().is_empty())
        .map_or(lines.len(), |p| p + start);
    let para = &lines[start..end];

    let first = para.first()?;
    key_line(first)?;
    // The reader's rule, reversed prefixes included (spec §4).
    if is_scene_heading_prefix(first) {
        return None;
    }

    let mut raw: Vec<(&str, Vec<&str>)> = Vec::new();
    for l in para {
        if let Some((k, v)) = key_line(l) {
            let values = if v.is_empty() { vec![] } else { vec![v] };
            raw.push((k, values));
        } else {
            raw.last_mut()?.1.push(continuation(l)?);
        }
    }
    if raw.iter().all(|(_, v)| v.is_empty()) {
        return None;
    }

    let fields = raw
        .into_iter()
        .map(|(k, vs)| TitleField::new(k, vs.into_iter().map(value).collect()))
        .collect();
    Some((TitlePage { fields }, end))
}

/// A value is one line of inline text; it may carry an inline note.
fn value(v: &str) -> RichText {
    let mut ignored: Vec<Diagnostic> = Vec::new();
    let scanned = scan::scan(&[v], 0, &mut ignored);
    let mut lines = scanned.lines;
    for l in &mut lines {
        line::trim(l);
    }
    inline::parse_lines(&lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_lines() {
        assert_eq!(key_line("Draft date: 1/2"), Some(("Draft date", "1/2")));
        assert_eq!(key_line("Title:"), Some(("Title", "")));
        assert_eq!(key_line(" Title: x"), None);
        assert_eq!(key_line("Title : x"), None);
        assert_eq!(key_line("#Title: x"), None);
        assert_eq!(key_line("no colon"), None);
    }

    #[test]
    fn needs_a_value() {
        assert!(detect(&["FADE IN:", "", "x"]).is_none());
        assert!(detect(&["Notes:", "Title: X"]).is_some());
    }

    #[test]
    fn rejects_mixed_paragraphs() {
        assert!(detect(&["Title: X", "some action"]).is_none());
        assert!(detect(&["INT. HOUSE: DAY"]).is_none());
        // The reader's reversed prefixes count too (spec §4).
        assert!(detect(&["EXT/INT. CAR: DAY"]).is_none());
        assert!(detect(&["E/I CAR: DAY"]).is_none());
    }
}
