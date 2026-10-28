//! How character names and scene headings are read (docs/specs/reports.md
//! §2, §3), shared by layout's continued names, the editor's completion and
//! the reports, so they never disagree.

/// A character block's speaker: its name, its extensions and the key that
/// compares speakers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Speaker {
    /// The text before the first `(`, trimmed.
    pub name: String,
    /// The text inside each pair of brackets after the name, trimmed and
    /// upper-cased, in order. An unclosed bracket runs to the end.
    pub extensions: Vec<String>,
    /// The name in upper case, with runs of whitespace as one space and
    /// `’` read as `'`.
    pub key: String,
}

/// Upper case, runs of whitespace as one space, `’` as `'`.
fn key_of(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
        .replace('\u{2019}', "'")
}

/// The speaker of a character block's text, or `None` when there is no
/// name before its first bracket.
pub fn speaker(text: &str) -> Option<Speaker> {
    let (name, rest) = match text.find('(') {
        Some(i) => (&text[..i], &text[i..]),
        None => (text, ""),
    };
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut extensions = Vec::new();
    let mut rest = rest;
    while let Some(open) = rest.find('(') {
        let after = &rest[open + 1..];
        let close = after.find(')').unwrap_or(after.len());
        let ext = after[..close].trim();
        if !ext.is_empty() {
            extensions.push(ext.to_uppercase());
        }
        rest = after.get(close + 1..).unwrap_or("");
    }
    Some(Speaker {
        name: name.to_string(),
        extensions,
        key: key_of(name),
    })
}

/// Whether an extension, or a whole name, holds the continued marker:
/// `CONT'D`, `CONTD` or the page format's own string, compared in capitals
/// with `’` as `'` (docs/specs/layout.md §7.5).
pub fn has_contd_marker(text: &str, contd: &str) -> bool {
    let norm = |s: &str| s.to_uppercase().replace('\u{2019}', "'");
    let t = norm(text);
    t.contains(&norm(contd)) || t.contains("CONT'D") || t.contains("CONTD")
}

/// Where a scene happens, as its heading's prefix says (§3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Setting {
    Int,
    Ext,
    IntExt,
    Est,
    None,
}

impl Setting {
    pub fn name(self) -> &'static str {
        match self {
            Setting::Int => "int",
            Setting::Ext => "ext",
            Setting::IntExt => "int_ext",
            Setting::Est => "est",
            Setting::None => "none",
        }
    }
}

/// A scene heading, read (§3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub setting: Setting,
    /// The prefix as written, with its trailing `.` when it has one.
    pub prefix: Option<String>,
    /// The location as written (case kept), trimmed.
    pub location: String,
    /// The location with runs of whitespace as one space, in upper case.
    pub location_key: String,
    /// The text after the last ` - `, trimmed, when there is one.
    pub time: Option<String>,
}

/// The scene prefixes, in the order they are tried (§3): the longer forms
/// first, so `INT./EXT` is never read as `INT`.
const PREFIXES: [(&str, Setting); 9] = [
    ("INT./EXT", Setting::IntExt),
    ("INT/EXT", Setting::IntExt),
    ("EXT./INT", Setting::IntExt),
    ("EXT/INT", Setting::IntExt),
    ("I/E", Setting::IntExt),
    ("E/I", Setting::IntExt),
    ("INT", Setting::Int),
    ("EXT", Setting::Ext),
    ("EST", Setting::Est),
];

/// Reads a scene heading's text.
pub fn heading(text: &str) -> Heading {
    let t = text.trim();
    let mut setting = Setting::None;
    let mut prefix = None;
    let mut rest = t;
    for (p, s) in PREFIXES {
        // Compared on the text's own bytes, so a character whose capital
        // happens to start with the prefix never splits a slice.
        if !t.get(..p.len()).is_some_and(|h| h.eq_ignore_ascii_case(p)) {
            continue;
        }
        let after = &t[p.len()..];
        let (with_dot, after) = match after.strip_prefix('.') {
            Some(a) => (true, a),
            None => (false, after),
        };
        if !with_dot && !after.starts_with(char::is_whitespace) {
            continue;
        }
        setting = s;
        prefix = Some(format!(
            "{}{}",
            &t[..p.len()],
            if with_dot { "." } else { "" }
        ));
        rest = after.trim_start();
        break;
    }
    let (location, time) = match rest.rfind(" - ") {
        Some(d) => (&rest[..d], Some(rest[d + 3..].trim().to_string())),
        None => (rest, None),
    };
    let location = location
        .trim()
        .trim_end_matches(" -")
        .trim_end()
        .to_string();
    Heading {
        setting,
        prefix,
        location_key: key_of(&location),
        location,
        time: time.filter(|t| !t.is_empty()),
    }
}

/// A time of day as reports compare it: upper case, whitespace as one space.
pub fn time_key(time: &str) -> String {
    key_of(time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speakers_and_their_extensions() {
        let s = speaker("Maya (V.O.) (cont’d)").unwrap();
        assert_eq!(s.name, "Maya");
        assert_eq!(s.extensions, ["V.O.", "CONT’D"]);
        assert_eq!(s.key, "MAYA");
        assert_eq!(speaker("MAYA  ROSE").unwrap().key, "MAYA ROSE");
        assert_eq!(speaker("THEO (O.S").unwrap().extensions, ["O.S"]);
        assert_eq!(speaker("O’BRIEN").unwrap().key, "O'BRIEN");
        assert!(speaker("(V.O.)").is_none());
        assert!(speaker("  ").is_none());
        assert!(has_contd_marker("CONT’D", "(CONT'D)"));
        assert!(!has_contd_marker("CONTRERAS", "(CONT)"));
    }

    #[test]
    fn every_prefix_and_its_setting() {
        for (t, setting, prefix) in [
            ("INT. LAB - NIGHT", Setting::Int, "INT."),
            ("int lab - night", Setting::Int, "int"),
            ("EXT. ROOF - DAY", Setting::Ext, "EXT."),
            ("EST. CITY", Setting::Est, "EST."),
            ("INT./EXT. CAR - DAY", Setting::IntExt, "INT./EXT."),
            ("INT/EXT CAR - DAY", Setting::IntExt, "INT/EXT"),
            ("EXT./INT. CAR - DAY", Setting::IntExt, "EXT./INT."),
            ("EXT/INT. BUS", Setting::IntExt, "EXT/INT."),
            ("I/E BOOTH - NIGHT", Setting::IntExt, "I/E"),
            ("E/I. BOOTH", Setting::IntExt, "E/I."),
        ] {
            let h = heading(t);
            assert_eq!(h.setting, setting, "{t}");
            assert_eq!(h.prefix.as_deref(), Some(prefix), "{t}");
        }
    }

    #[test]
    fn locations_and_times() {
        let h = heading("INT. MAYA'S  KITCHEN - LATER - NIGHT");
        assert_eq!(h.location, "MAYA'S  KITCHEN - LATER");
        assert_eq!(h.location_key, "MAYA'S KITCHEN - LATER");
        assert_eq!(h.time.as_deref(), Some("NIGHT"));
        let h = heading("EXT. ROOF");
        assert_eq!((h.location.as_str(), h.time), ("ROOF", None));
        let h = heading("INTERIOR OF A CAR");
        assert_eq!(h.setting, Setting::None);
        assert_eq!(h.location, "INTERIOR OF A CAR");
        let h = heading("EXT. ROOF -- DAWN");
        assert_eq!(h.location, "ROOF -- DAWN");
        let h = heading("EXT. ROOF -");
        assert_eq!(h.location, "ROOF");
        // Never panics on text whose capitals differ in length.
        let h = heading("ﬁnt. lab");
        assert_eq!(h.setting, Setting::None);
    }
}
