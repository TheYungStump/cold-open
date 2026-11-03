//! Crash reports (docs/specs/crash-reports.md): what a report holds, and
//! the scrubbing that keeps script text, paths and names out of it. The
//! desktop app's panic hook fills a [`Report`]; everything here is plain
//! text work, with no I/O, so the fuzz target and the tests reach it.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::Command;

/// The report format's number (§3), on its first line.
pub const FORMAT: u32 = 1;
/// The longest message kept, in characters (§4.1).
pub const MESSAGE_MAX: usize = 500;
/// The most frames kept (§5).
pub const FRAMES_MAX: usize = 100;
/// What a quoted span becomes.
pub const QUOTED: &str = "[quoted]";
/// What an absolute path becomes.
pub const PATH: &str = "[path]";

/// The last request the program ran, for the report's `Last action` line
/// (§3): 0 before any, `1 + i` for `Command::NAMES[i]`, or a code at or
/// above [`APP_CODES`] that the app gives its own actions.
static LAST: AtomicU32 = AtomicU32::new(0);

/// Codes from here up are the app's (see [`note`]).
pub const APP_CODES: u32 = 0x1_0000;

/// Notes that a command is about to run. A relaxed store: the hook reads it
/// on the same thread, or late on another, and either is good enough.
pub fn note_command(name: &str) {
    if let Some(c) = command_code(name) {
        LAST.store(c, Ordering::Relaxed);
    }
}

/// Notes an app action by the app's own code (at or above [`APP_CODES`]).
pub fn note(code: u32) {
    LAST.store(code, Ordering::Relaxed);
}

/// The last request noted: `Ok(name)` for a command, `Err(code)` for an
/// app's code, or `Ok("none")`.
pub fn last() -> Result<&'static str, u32> {
    decode(LAST.load(Ordering::Relaxed))
}

fn command_code(name: &str) -> Option<u32> {
    Command::NAMES
        .iter()
        .position(|n| *n == name)
        .map(|i| 1 + i as u32)
}

fn decode(code: u32) -> Result<&'static str, u32> {
    match code {
        0 => Ok("none"),
        c if c >= APP_CODES => Err(c),
        c => Ok(Command::NAMES
            .get(c as usize - 1)
            .copied()
            .unwrap_or("none")),
    }
}

/// One crash report (§3). Every field is already scrubbed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub version: String,
    pub target: String,
    pub os: String,
    pub renderer: String,
    pub time: String,
    pub thread: String,
    pub last_action: String,
    pub location: String,
    pub message: String,
    /// `<module>+0x<offset>`, newest call first.
    pub frames: Vec<String>,
}

const FIELDS: [&str; 9] = [
    "Version",
    "Target",
    "OS",
    "Renderer",
    "Time",
    "Thread",
    "Last action",
    "Location",
    "Message",
];

impl Report {
    fn fields(&self) -> [&str; 9] {
        [
            &self.version,
            &self.target,
            &self.os,
            &self.renderer,
            &self.time,
            &self.thread,
            &self.last_action,
            &self.location,
            &self.message,
        ]
    }

    fn fields_mut(&mut self) -> [&mut String; 9] {
        [
            &mut self.version,
            &mut self.target,
            &mut self.os,
            &mut self.renderer,
            &mut self.time,
            &mut self.thread,
            &mut self.last_action,
            &mut self.location,
            &mut self.message,
        ]
    }

    /// The report's text, as it is written to disk.
    pub fn render(&self) -> String {
        let mut s = format!("Cold Open crash report {FORMAT}\n");
        for (k, v) in FIELDS.iter().zip(self.fields()) {
            s.push_str(&format!("{k}: {}\n", one_line(v)));
        }
        s.push_str("\nBacktrace:\n");
        for (i, f) in self.frames.iter().take(FRAMES_MAX).enumerate() {
            s.push_str(&format!("{i:4}: {}\n", one_line(f)));
        }
        s
    }

    /// Reads a report back from its text, for the sheet and the addresses
    /// it fills in. `None` for a file that isn't one.
    pub fn parse(text: &str) -> Option<Report> {
        let mut lines = text.lines();
        lines.next()?.strip_prefix("Cold Open crash report ")?;
        let mut r = Report::default();
        for line in lines.by_ref() {
            if line == "Backtrace:" {
                break;
            }
            if let Some((k, v)) = line.split_once(": ")
                && let Some(i) = FIELDS.iter().position(|f| *f == k)
            {
                *r.fields_mut()[i] = v.to_string();
            }
        }
        for line in lines {
            if let Some((_, f)) = line.split_once(": ") {
                r.frames.push(f.to_string());
            }
        }
        Some(r)
    }

    /// The location and the message, as one line: what a prefilled issue
    /// or form says before the whole report is pasted (§7).
    pub fn summary(&self) -> String {
        format!("{} at {}", self.message, self.location)
    }
}

/// A field on one line: line breaks become spaces, other control
/// characters (but the tab) go.
fn one_line(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c {
            '\n' | '\r' => Some(' '),
            '\t' => Some('\t'),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// What §4.1 step 3 takes out, from the environment.
#[derive(Clone, Debug, Default)]
pub struct Private {
    pub home: Option<String>,
    pub user: Option<String>,
}

impl Private {
    /// The home folder and user name of the process's environment.
    pub fn from_env() -> Private {
        let var = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| std::env::var(n).ok().filter(|v| !v.is_empty()))
        };
        Private {
            home: var(&["HOME", "USERPROFILE"]),
            user: var(&["USER", "USERNAME", "LOGNAME"]),
        }
    }
}

/// The panic's message, scrubbed (§4.1).
pub fn scrub_message(msg: &str, private: &Private) -> String {
    let s = quoted_out(msg);
    let s = paths_out(&s);
    let s = private_out(&s, private);
    let s = one_line(&s);
    cap(&s, MESSAGE_MAX)
}

/// §4.1 step 1: quoted spans become `[quoted]`.
fn quoted_out(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            // Rust's own slicing messages quote the text in backticks
            // without escaping it, so a backtick inside the text would end
            // a short span early: a span runs to the last backtick.
            '`' => {
                out.push_str(QUOTED);
                match chars.iter().rposition(|&d| d == '`') {
                    Some(j) if j > i => i = j + 1,
                    _ => return out,
                }
            }
            '"' => {
                out.push_str(QUOTED);
                match close(&chars, i + 1, '"') {
                    Some(j) => i = j + 1,
                    None => return out,
                }
            }
            '\'' if i == 0 || !chars[i - 1].is_alphanumeric() => {
                out.push_str(QUOTED);
                match close(&chars, i + 1, '\'') {
                    Some(j) => i = j + 1,
                    None => return out,
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// The index of the quote that closes a span opened before `from`, with a
/// backslash escaping the character after it, as `Debug` writes them.
fn close(chars: &[char], from: usize, quote: char) -> Option<usize> {
    let mut j = from;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            c if c == quote => return Some(j),
            _ => j += 1,
        }
    }
    None
}

/// §4.1 step 2: absolute paths become `[path]`.
fn paths_out(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let starts_word = i == 0 || is_boundary(chars[i - 1]);
        if starts_word && path_at(&chars, i) {
            out.push_str(PATH);
            while i < chars.len() && !ends_path(chars[i]) {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn is_boundary(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | '[' | '{' | '=' | ':' | ',' | ';' | '<')
}

fn ends_path(c: char) -> bool {
    c.is_whitespace() || matches!(c, ')' | ']' | '}' | ',' | ';' | '>')
}

/// Whether an absolute path starts at `i`: `C:\`, `C:/`, `\\server`, `~/`,
/// or `/` and then a letter, a dot, `_` or `~`.
fn path_at(chars: &[char], i: usize) -> bool {
    let at = |k: usize| chars.get(i + k).copied();
    match at(0) {
        Some(d) if d.is_ascii_alphabetic() => {
            at(1) == Some(':') && matches!(at(2), Some('\\' | '/'))
        }
        Some('\\') => at(1) == Some('\\') && at(2).is_some_and(|c| c.is_alphanumeric()),
        Some('~') => at(1) == Some('/'),
        Some('/') => at(1).is_some_and(|c| c.is_alphabetic() || matches!(c, '.' | '_' | '~')),
        _ => false,
    }
}

/// §4.1 step 3: the home folder and the user name.
fn private_out(s: &str, p: &Private) -> String {
    let mut s = s.to_string();
    if let Some(home) = p.home.as_deref().filter(|h| h.len() >= 2) {
        for form in [home.to_string(), home.replace('\\', "/")] {
            s = replace_ci(&s, &form, "[home]", false);
        }
    }
    if let Some(user) = p.user.as_deref().filter(|u| u.chars().count() >= 3) {
        s = replace_ci(&s, user, "[user]", true);
    }
    s
}

/// Replaces every match of `what`, ignoring case; with `word`, only where
/// it is not part of a longer word.
fn replace_ci(s: &str, what: &str, with: &str, word: bool) -> String {
    let hay: Vec<char> = s.chars().collect();
    let needle: Vec<char> = what.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return s.to_string();
    }
    let lower: Vec<char> = hay
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < hay.len() {
        let fits = i + needle.len() <= hay.len() && lower[i..i + needle.len()] == needle[..];
        let whole = !word
            || ((i == 0 || !hay[i - 1].is_alphanumeric())
                && hay
                    .get(i + needle.len())
                    .is_none_or(|c| !c.is_alphanumeric()));
        if fits && whole {
            out.push_str(with);
            i += needle.len();
        } else {
            out.push(hay[i]);
            i += 1;
        }
    }
    out
}

/// At most `max` characters, with `…` when cut.
fn cap(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

/// The panic's file, cut to the part a developer needs (§4.2).
pub fn trim_location(file: &str) -> String {
    let f = file.replace('\\', "/");
    if let Some(i) = f.rfind("/registry/src/") {
        let rest = &f[i + "/registry/src/".len()..];
        if let Some((_, krate)) = rest.split_once('/') {
            return krate.to_string();
        }
    }
    for root in ["/library/", "crates/"] {
        if let Some(i) = f.find(root) {
            let from = if root.starts_with('/') { i + 1 } else { i };
            return f[from..].to_string();
        }
    }
    if f.starts_with("library/") {
        return f;
    }
    module_name(&f)
}

/// A module's file name, without its folder (§4.3).
pub fn module_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|p| !p.is_empty())
        .unwrap_or("?")
        .to_string()
}

/// One frame's line (§5).
pub fn frame(module: Option<&str>, offset: usize) -> String {
    match module {
        Some(m) => format!("{}+0x{offset:x}", module_name(m)),
        None => format!("?+0x{offset:x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrub(s: &str) -> String {
        scrub_message(s, &Private::default())
    }

    #[test]
    fn rusts_slicing_message_loses_the_text() {
        let text = "Mañana, MAYA said `hi` to the café";
        let msg = format!(
            "byte index 3 is not a char boundary; it is inside 'ñ' (bytes 2..4) of `{text}`"
        );
        let s = scrub(&msg);
        assert_eq!(
            s,
            "byte index 3 is not a char boundary; it is inside [quoted] (bytes 2..4) of [quoted]"
        );
        for word in ["Mañana", "MAYA", "said", "hi", "café", "ñ"] {
            assert!(!s.contains(word), "{word} in {s}");
        }
        let cut = "byte index 300 is out of bounds of `Lighthouse keeper's log`[...]";
        assert_eq!(
            scrub(cut),
            "byte index 300 is out of bounds of [quoted][...]"
        );
    }

    /// Rust's real messages, not our idea of them.
    #[test]
    fn real_panics_lose_the_text() {
        let text = String::from("MAYA: the café is closed");
        let n = text.find('é').unwrap() + 1;
        let payloads = [
            std::panic::catch_unwind(|| text[..n].len()).unwrap_err(),
            std::panic::catch_unwind(|| text[..999].len()).unwrap_err(),
            std::panic::catch_unwind(|| {
                let r: Result<u8, String> = if text.is_empty() {
                    Ok(0)
                } else {
                    Err(text.clone())
                };
                r.unwrap()
            })
            .unwrap_err(),
        ];
        for p in payloads {
            let msg = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap();
            let s = scrub(&msg);
            for word in ["MAYA", "café", "closed", "é"] {
                assert!(!s.contains(word), "{word} in {s} (from {msg})");
            }
        }
    }

    #[test]
    fn debug_strings_and_chars_with_escapes() {
        let s = scrub(
            r#"called `Result::unwrap()` on an `Err` value: Io("can't \"open\" it", 'x', '\'')"#,
        );
        assert!(!s.contains("open"), "{s}");
        assert!(!s.contains("can't"), "{s}");
        let s = scrub(r#"bad name: "MAYA \"THE\" KEEPER" and then text"#);
        assert_eq!(s, "bad name: [quoted] and then text");
        let s = scrub(r"chars 'a' and '\'' end");
        assert_eq!(s, "chars [quoted] and [quoted] end");
    }

    #[test]
    fn apostrophes_open_nothing() {
        assert_eq!(scrub("can't save: it's locked"), "can't save: it's locked");
    }

    #[test]
    fn an_unclosed_quote_takes_the_rest() {
        assert_eq!(scrub("found \"MAYA walks in"), "found [quoted]");
        assert_eq!(scrub("found `MAYA"), "found [quoted]");
        assert_eq!(scrub("found 'MAYA"), "found [quoted]");
    }

    #[test]
    fn paths_go() {
        let cases = [
            (
                r"cannot open C:\Users\kim\Scripts\a.fountain now",
                "cannot open [path] now",
            ),
            ("cannot open D:/drafts/b.fdx", "cannot open [path]"),
            (
                r"share \\server\share\x.fdx missing",
                "share [path] missing",
            ),
            (
                "in /home/kim/scripts/a.fountain: denied",
                "in [path] denied",
            ),
            ("at (/Users/kim/x)", "at ([path])"),
            ("see ~/scripts", "see [path]"),
            ("in=/tmp/x.json", "in=[path]"),
        ];
        for (msg, want) in cases {
            assert_eq!(scrub(msg), want, "{msg}");
        }
        // Fractions, "and/or", addresses and relative paths stay.
        for keep in [
            "3/8 of a page",
            "and/or",
            "see https://example.com/x",
            "crates/app/src/x.rs",
        ] {
            assert_eq!(scrub(keep), keep);
        }
    }

    #[test]
    fn the_home_folder_and_user_name_go() {
        let p = Private {
            home: Some(r"C:\Users\Kim".into()),
            user: Some("Kim".into()),
        };
        let s = scrub_message("kim's job failed for KIM in c:/users/kim", &p);
        assert!(!s.to_lowercase().contains("kim"), "{s}");
        // A whole word only, and only a name of three characters or more.
        let s = scrub_message("Kimberly met kim", &p);
        assert_eq!(s, "Kimberly met [user]");
        let short = Private {
            home: None,
            user: Some("al".into()),
        };
        assert_eq!(scrub_message("all along", &short), "all along");
    }

    #[test]
    fn control_characters_and_length() {
        assert_eq!(scrub("one\ntwo\r\x07three"), "one two three");
        let long = "x".repeat(10_000);
        let s = scrub(&long);
        assert_eq!(s.chars().count(), MESSAGE_MAX);
        assert!(s.ends_with('…'));
        let wide = "é".repeat(10_000);
        assert_eq!(scrub(&wide).chars().count(), MESSAGE_MAX);
    }

    #[test]
    fn locations_are_cut_down() {
        let cases = [
            (
                "/home/runner/work/cold-open/cold-open/crates/app/src/editor/mod.rs",
                "crates/app/src/editor/mod.rs",
            ),
            ("crates/core/src/crash.rs", "crates/core/src/crash.rs"),
            (
                r"C:\Users\kim\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\iced_widget-0.14.2\src\text_input.rs",
                "iced_widget-0.14.2/src/text_input.rs",
            ),
            (
                "/Users/kim/.cargo/registry/src/index.crates.io-6f17d22bba15001f/winit-0.30.0/src/x.rs",
                "winit-0.30.0/src/x.rs",
            ),
            (
                "/rustc/90b35a6239c3d8bdabc530a6a0816f7ff89a0aaf/library/core/src/str/mod.rs",
                "library/core/src/str/mod.rs",
            ),
            (
                "library/std/src/thread/mod.rs",
                "library/std/src/thread/mod.rs",
            ),
            ("/opt/kim/elsewhere/thing.rs", "thing.rs"),
        ];
        for (file, want) in cases {
            assert_eq!(trim_location(file), want, "{file}");
        }
    }

    #[test]
    fn modules_and_frames_name_no_folder() {
        assert_eq!(
            frame(Some(r"C:\Users\kim\Cold Open\Cold Open.exe"), 0x1a2b),
            "Cold Open.exe+0x1a2b"
        );
        assert_eq!(
            frame(Some("/tmp/.mount_abc/usr/bin/coldopen-app"), 16),
            "coldopen-app+0x10"
        );
        assert_eq!(frame(None, 255), "?+0xff");
    }

    #[test]
    fn a_report_renders_and_reads_back() {
        let r = Report {
            version: "0.1.0-alpha.2 (5d6e764a1b2c)".into(),
            target: "x86_64-pc-windows-msvc".into(),
            os: "Windows 11 (26100)".into(),
            renderer: "CPU".into(),
            time: "2026-11-03T12:34:56Z".into(),
            thread: "main".into(),
            last_action: "text.insert".into(),
            location: "crates/app/src/editor/mod.rs:812:22".into(),
            message: "boom\nagain".into(),
            frames: vec!["Cold Open.exe+0x1a2b3c".into(), "ntdll.dll+0x8d2a1".into()],
        };
        let text = r.render();
        assert!(text.starts_with("Cold Open crash report 1\nVersion: 0.1.0-alpha.2"));
        assert!(text.contains("\nMessage: boom again\n\nBacktrace:\n   0: Cold Open.exe+0x1a2b3c\n   1: ntdll.dll+0x8d2a1\n"));
        let back = Report::parse(&text).unwrap();
        assert_eq!(back.message, "boom again");
        assert_eq!(back.frames, r.frames);
        assert_eq!(back.location, r.location);
        assert_eq!(
            back.summary(),
            "boom again at crates/app/src/editor/mod.rs:812:22"
        );
        assert!(Report::parse("not a report").is_none());
    }

    // The store itself is a global, which tests running in parallel would
    // race on, so the codes are checked here and the store by the app's
    // crash report test.
    #[test]
    fn commands_and_app_codes_decode() {
        assert_eq!(decode(0), Ok("none"));
        let c = command_code("text.insert").unwrap();
        assert!(c < APP_CODES);
        assert_eq!(decode(c), Ok("text.insert"));
        assert_eq!(decode(APP_CODES + 3), Err(APP_CODES + 3));
        assert_eq!(command_code("not.a.command"), None);
        assert_eq!(decode(9_999), Ok("none"));
    }
}
