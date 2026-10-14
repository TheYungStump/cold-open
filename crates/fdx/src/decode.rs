//! Turning the input bytes into text (docs/specs/fdx.md §2.1).

use crate::report::{Issue, codes};

/// Decodes .fdx bytes into text, with a note for anything unusual.
///
/// A UTF-8 byte-order mark is removed; UTF-16 is recognized by its mark or
/// by a leading `<` paired with a zero byte; other input is UTF-8 when
/// valid, windows-1252 when the XML declaration names it (or ISO-8859-1),
/// and lossy UTF-8 otherwise.
pub fn decode_input(bytes: &[u8]) -> (String, Vec<Issue>) {
    let mut notes = Vec::new();
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (utf8(rest, &mut notes), notes);
    }
    let utf16 = if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        Some((rest, false))
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        Some((rest, true))
    } else if bytes.starts_with(&[b'<', 0]) {
        Some((bytes, false))
    } else if bytes.starts_with(&[0, b'<']) {
        Some((bytes, true))
    } else {
        None
    };
    if let Some((rest, big_endian)) = utf16 {
        let order = if big_endian { "big" } else { "little" };
        notes.push(note(
            codes::ENCODING_UTF16,
            format!("the file is UTF-16 ({order}-endian)"),
        ));
        let (text, lossy) = decode_utf16(rest, big_endian);
        if lossy {
            notes.push(note(
                codes::ENCODING_LOSSY,
                "the UTF-16 text holds invalid sequences; they were replaced with U+FFFD",
            ));
        }
        return (text, notes);
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_string(), notes);
    }
    if declares_windows_1252(bytes) {
        notes.push(note(
            codes::ENCODING_WINDOWS_1252,
            "the file is not UTF-8; it was decoded as windows-1252, as its declaration says",
        ));
        return (bytes.iter().map(|&b| windows_1252(b)).collect(), notes);
    }
    (utf8(bytes, &mut notes), notes)
}

fn note(code: &'static str, message: impl Into<String>) -> Issue {
    Issue {
        code,
        message: message.into(),
        paragraph: None,
    }
}

fn utf8(bytes: &[u8], notes: &mut Vec<Issue>) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            notes.push(note(
                codes::ENCODING_LOSSY,
                "the file is not valid UTF-8; invalid bytes were replaced with U+FFFD",
            ));
            String::from_utf8_lossy(bytes).into_owned()
        }
    }
}

/// UTF-16 text and whether anything had to be replaced.
fn decode_utf16(bytes: &[u8], big_endian: bool) -> (String, bool) {
    let units = bytes.as_chunks::<2>().0.iter().map(|&pair| {
        if big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    });
    let mut lossy = !bytes.len().is_multiple_of(2);
    let mut text = String::with_capacity(bytes.len() / 2);
    for c in char::decode_utf16(units) {
        match c {
            Ok(c) => text.push(c),
            Err(_) => {
                lossy = true;
                text.push(char::REPLACEMENT_CHARACTER);
            }
        }
    }
    if !bytes.len().is_multiple_of(2) {
        text.push(char::REPLACEMENT_CHARACTER);
    }
    (text, lossy)
}

/// Whether the XML declaration in the first 256 bytes names windows-1252
/// or ISO-8859-1 as the encoding.
fn declares_windows_1252(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(256)];
    if !head.starts_with(b"<?xml") {
        return false;
    }
    let end = head
        .windows(2)
        .position(|w| w == b"?>")
        .unwrap_or(head.len());
    let decl = String::from_utf8_lossy(&head[..end]).to_ascii_lowercase();
    let Some(i) = decl.find("encoding") else {
        return false;
    };
    let rest = decl[i + "encoding".len()..].trim_start();
    let Some(rest) = rest.strip_prefix('=') else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
        return false;
    };
    let value = rest[1..].split(quote).next().unwrap_or("");
    matches!(
        value.trim(),
        "windows-1252" | "cp1252" | "iso-8859-1" | "latin1"
    )
}

/// One windows-1252 byte as a character (WHATWG index-windows-1252). The
/// five unassigned bytes map to the C1 controls of the same value.
fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}',
        '\u{017D}', '\u{008F}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes_of(notes: &[Issue]) -> Vec<&'static str> {
        notes.iter().map(|n| n.code).collect()
    }

    #[test]
    fn utf8_with_and_without_bom() {
        let (t, n) = decode_input("<a>é</a>".as_bytes());
        assert_eq!((t.as_str(), n.len()), ("<a>é</a>", 0));
        let (t, n) = decode_input(b"\xEF\xBB\xBF<a/>");
        assert_eq!((t.as_str(), n.len()), ("<a/>", 0));
    }

    #[test]
    fn utf16_by_bom_and_by_shape() {
        let le: Vec<u8> = "<a>’</a>"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let be: Vec<u8> = "<a>’</a>"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        for (bytes, bom) in [(&le, &[0xFF, 0xFE]), (&be, &[0xFE, 0xFF])] {
            let mut with_bom = bom.to_vec();
            with_bom.extend_from_slice(bytes);
            for input in [&with_bom, bytes] {
                let (t, n) = decode_input(input);
                assert_eq!(t, "<a>’</a>");
                assert_eq!(codes_of(&n), [codes::ENCODING_UTF16]);
            }
        }
        let (t, n) = decode_input(&[0xFF, 0xFE, b'<', 0, 0x00, 0xD8, b'x']);
        assert_eq!(t, "<\u{FFFD}\u{FFFD}");
        assert_eq!(codes_of(&n), [codes::ENCODING_UTF16, codes::ENCODING_LOSSY]);
    }

    #[test]
    fn windows_1252_when_declared() {
        let bytes = b"<?xml version=\"1.0\" encoding=\"Windows-1252\"?><a>\x93caf\xE9\x94 \x81</a>";
        let (t, n) = decode_input(bytes);
        assert!(t.ends_with("<a>“café” \u{81}</a>"), "{t}");
        assert_eq!(codes_of(&n), [codes::ENCODING_WINDOWS_1252]);
        let bytes = b"<?xml version='1.0' encoding = 'ISO-8859-1' ?><a>\xE9</a>";
        assert_eq!(
            decode_input(bytes).0,
            "<?xml version='1.0' encoding = 'ISO-8859-1' ?><a>é</a>"
        );
    }

    #[test]
    fn lossy_otherwise() {
        let (t, n) = decode_input(b"<a>\xE9</a>");
        assert_eq!(t, "<a>\u{FFFD}</a>");
        assert_eq!(codes_of(&n), [codes::ENCODING_LOSSY]);
        let (t, n) = decode_input(b"\xEF\xBB\xBF<a>\xFF</a>");
        assert_eq!(t, "<a>\u{FFFD}</a>");
        assert_eq!(codes_of(&n), [codes::ENCODING_LOSSY]);
    }
}
