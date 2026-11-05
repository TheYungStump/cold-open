//! The local usage summary (docs/specs/usage-summary.md): counts of what
//! was used, kept in `usage.json` in the data folder, never sent. Every key
//! comes from a fixed table, so nothing a writer types can get in (§2).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use coldopen_core::Command;
use serde::{Deserialize, Serialize};

use crate::actions::APP_ACTIONS;

/// The file's format.
pub const VERSION: u32 = 1;

/// The most app versions kept (§2).
const MAX_VERSIONS: usize = 20;

/// How long counts wait in memory before they're added to the file (§5).
pub const WRITE_EVERY: Duration = Duration::from_secs(5 * 60);

/// Requests named with an index (`recent:3`) count under their prefix.
const INDEXED: [&str; 3] = ["recent", "leftover", "forget"];

/// The formats a script is opened or saved in (§2).
pub const FORMATS: [&str; 3] = ["fountain", "fdx", "project"];

/// What drew the window in a session (§2.1).
pub const RENDERERS: [&str; 4] = ["cpu", "gpu", "cpu_fallback", "not_asked"];

/// A finished session's length, rounded into a bucket (§2).
pub const LENGTHS: [(&str, &str); 4] = [
    ("under_5_min", "under 5 minutes"),
    ("5_to_30_min", "5 to 30 minutes"),
    ("30_min_to_2_h", "30 minutes to 2 hours"),
    ("over_2_h", "over 2 hours"),
];

/// `usage.json` (§4).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Summary {
    pub version: u32,
    pub since: String,
    pub app_versions: Vec<String>,
    pub sessions: u64,
    pub session_lengths: BTreeMap<String, u64>,
    pub commands: BTreeMap<String, u64>,
    pub agent_commands: BTreeMap<String, u64>,
    pub opened: BTreeMap<String, u64>,
    pub saved: BTreeMap<String, u64>,
    pub renderer: BTreeMap<String, u64>,
}

/// Counts waiting in memory to be added to the file (§5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub sessions: u64,
    pub session_lengths: BTreeMap<&'static str, u64>,
    pub commands: BTreeMap<&'static str, u64>,
    pub agent_commands: BTreeMap<&'static str, u64>,
    pub opened: BTreeMap<&'static str, u64>,
    pub saved: BTreeMap<&'static str, u64>,
    pub renderer: BTreeMap<&'static str, u64>,
}

/// The table's own copy of a request's name, or `None` for a name that
/// isn't in it: a core command, an app action, or an indexed request's
/// prefix (§2).
pub fn known_name(name: &str) -> Option<&'static str> {
    if let Some(n) = Command::NAMES.iter().find(|n| **n == name) {
        return Some(n);
    }
    if let Some(a) = APP_ACTIONS.iter().find(|a| a.name == name) {
        return Some(a.name);
    }
    let prefix = name.split(':').next()?;
    if name.contains(':') {
        return INDEXED.iter().find(|p| **p == prefix).copied();
    }
    None
}

fn known_in(set: &'static [&'static str], name: &str) -> Option<&'static str> {
    set.iter().find(|n| **n == name).copied()
}

/// A session's length as its bucket's key.
pub fn length_bucket(d: Duration) -> &'static str {
    let m = d.as_secs() / 60;
    match m {
        0..5 => LENGTHS[0].0,
        5..30 => LENGTHS[1].0,
        30..120 => LENGTHS[2].0,
        _ => LENGTHS[3].0,
    }
}

impl Pending {
    pub fn is_empty(&self) -> bool {
        *self == Pending::default()
    }

    /// One request the writer made, by name. Unknown names are dropped.
    pub fn command(&mut self, name: &str) {
        if let Some(n) = known_name(name) {
            *self.commands.entry(n).or_default() += 1;
        }
    }

    /// One core command an agent ran. Unknown names are dropped.
    pub fn agent_command(&mut self, name: &str) {
        if let Some(n) = Command::NAMES.iter().find(|n| **n == name) {
            *self.agent_commands.entry(n).or_default() += 1;
        }
    }

    pub fn opened(&mut self, format: &str) {
        if let Some(f) = known_in(&FORMATS, format) {
            *self.opened.entry(f).or_default() += 1;
        }
    }

    pub fn saved(&mut self, format: &str) {
        if let Some(f) = known_in(&FORMATS, format) {
            *self.saved.entry(f).or_default() += 1;
        }
    }

    /// A session started.
    pub fn session(&mut self) {
        self.sessions += 1;
    }

    /// A session ended, after `d`, drawn with `renderer`.
    pub fn session_end(&mut self, d: Duration, renderer: &str) {
        *self.session_lengths.entry(length_bucket(d)).or_default() += 1;
        if let Some(r) = known_in(&RENDERERS, renderer) {
            *self.renderer.entry(r).or_default() += 1;
        }
    }
}

impl Summary {
    /// `pending` added to the summary, which starts counting on `today`
    /// if it hasn't yet, for app version `version`.
    pub fn add(&mut self, pending: &Pending, today: &str, version: &str) {
        self.version = VERSION;
        if self.since.is_empty() {
            self.since = today.to_string();
        }
        if !self.app_versions.iter().any(|v| v == version) {
            self.app_versions.push(version.to_string());
            let extra = self.app_versions.len().saturating_sub(MAX_VERSIONS);
            self.app_versions.drain(..extra);
        }
        self.sessions += pending.sessions;
        fn merge(into: &mut BTreeMap<String, u64>, from: &BTreeMap<&'static str, u64>) {
            for (k, v) in from {
                *into.entry((*k).to_string()).or_default() += v;
            }
        }
        merge(&mut self.session_lengths, &pending.session_lengths);
        merge(&mut self.commands, &pending.commands);
        merge(&mut self.agent_commands, &pending.agent_commands);
        merge(&mut self.opened, &pending.opened);
        merge(&mut self.saved, &pending.saved);
        merge(&mut self.renderer, &pending.renderer);
    }

    /// Whether nothing has been counted.
    pub fn is_empty(&self) -> bool {
        self.sessions == 0
            && self.commands.is_empty()
            && self.agent_commands.is_empty()
            && self.opened.is_empty()
            && self.saved.is_empty()
    }

    /// Every key is from the tables of §2: a check on a file read back.
    pub fn keys_known(&self) -> bool {
        self.commands.keys().all(|k| known_name(k).is_some())
            && self
                .agent_commands
                .keys()
                .all(|k| Command::NAMES.contains(&k.as_str()))
            && self.opened.keys().all(|k| FORMATS.contains(&k.as_str()))
            && self.saved.keys().all(|k| FORMATS.contains(&k.as_str()))
            && self
                .renderer
                .keys()
                .all(|k| RENDERERS.contains(&k.as_str()))
            && self
                .session_lengths
                .keys()
                .all(|k| LENGTHS.iter().any(|(l, _)| l == k))
            && self.app_versions.iter().all(|v| plain_version(v))
            && (self.since.is_empty() || is_day(&self.since))
    }

    /// The summary in plain words (§9): what the View sheet shows and what
    /// feedback includes.
    pub fn text(&self) -> String {
        let mut out = String::new();
        let since = if self.since.is_empty() {
            String::new()
        } else {
            format!(" since {}", self.since)
        };
        out.push_str(&format!(
            "Usage summary from Cold Open, counted on this computer{since}.\n"
        ));
        let versions = if self.app_versions.is_empty() {
            "none".to_string()
        } else {
            self.app_versions.join(", ")
        };
        out.push_str(&format!("App versions: {versions}\n"));
        let finished: u64 = self.session_lengths.values().sum();
        let mut lengths: Vec<String> = LENGTHS
            .iter()
            .map(|(k, words)| {
                format!(
                    "{words}: {}",
                    self.session_lengths.get(*k).copied().unwrap_or(0)
                )
            })
            .collect();
        lengths.push(format!(
            "still open or ended by a crash: {}",
            self.sessions.saturating_sub(finished)
        ));
        out.push_str(&format!(
            "Sessions: {} ({})\n",
            self.sessions,
            lengths.join("; ")
        ));
        let drawn: Vec<String> = RENDERERS
            .iter()
            .filter_map(|k| {
                let n = self.renderer.get(*k).copied().unwrap_or(0);
                (n > 0).then(|| format!("{} in {n} session{}", renderer_words(k), plural(n)))
            })
            .collect();
        out.push_str(&format!("Drawn with: {}\n", list_or_none(&drawn, ", ")));
        out.push_str(&format!("Scripts opened: {}\n", formats_line(&self.opened)));
        out.push_str(&format!("Scripts saved: {}\n", formats_line(&self.saved)));
        out.push_str(&format!(
            "Commands you used: {}\n",
            commands_line(&self.commands)
        ));
        out.push_str(&format!(
            "Commands agents used: {}\n",
            commands_line(&self.agent_commands)
        ));
        out.push_str(
            "It holds no script text, no names, no file names or folders, and no times finer than a day.\n",
        );
        out
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn list_or_none(items: &[String], sep: &str) -> String {
    if items.is_empty() {
        "none".into()
    } else {
        items.join(sep)
    }
}

fn renderer_words(k: &str) -> &'static str {
    match k {
        "cpu" => "CPU",
        "gpu" => "GPU",
        "cpu_fallback" => "CPU (the GPU renderer couldn't start)",
        _ => "not asked",
    }
}

fn format_words(k: &str) -> &'static str {
    match k {
        "fountain" => "Fountain",
        "fdx" => ".fdx",
        _ => "Cold Open project",
    }
}

/// A format's key in the summary.
pub fn format_key(f: coldopen_core::FormatId) -> &'static str {
    match f {
        coldopen_core::FormatId::Fountain => "fountain",
        coldopen_core::FormatId::Fdx => "fdx",
        coldopen_core::FormatId::Bundle => "project",
    }
}

fn formats_line(m: &BTreeMap<String, u64>) -> String {
    let items: Vec<String> = FORMATS
        .iter()
        .filter_map(|k| {
            m.get(*k)
                .filter(|n| **n > 0)
                .map(|n| format!("{} {n}", format_words(k)))
        })
        .collect();
    list_or_none(&items, ", ")
}

/// Commands by count, highest first, then by name (§9).
fn commands_line(m: &BTreeMap<String, u64>) -> String {
    let mut v: Vec<(&String, &u64)> = m.iter().filter(|(_, n)| **n > 0).collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let items: Vec<String> = v.iter().map(|(k, n)| format!("{k} {n}")).collect();
    list_or_none(&items, ", ")
}

/// `YYYY-MM-DD`.
fn is_day(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// A version such as `0.1.0-alpha.3`: digits, letters, dots and dashes.
fn plain_version(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+')
}

/// Today's date in UTC, as `YYYY-MM-DD`, from the system clock: no time
/// zone is read (§2).
pub fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    day_of(secs / 86_400)
}

/// The civil date of day `days` since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn day_of(days: u64) -> String {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The file's place in the data folder (§4).
pub fn path(data: &Path) -> PathBuf {
    data.join("usage.json")
}

/// What reading the file found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    /// No file yet.
    Missing,
    Ok(Box<Summary>),
    /// A file that can't be read, or from a newer version: left alone (§4).
    Unreadable,
}

/// Reads `usage.json`.
pub fn read(path: &Path) -> Read {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Missing,
        Err(_) => Read::Unreadable,
        Ok(bytes) => match serde_json::from_slice::<Summary>(&bytes) {
            Ok(s) if s.version <= VERSION && s.keys_known() => Read::Ok(Box::new(s)),
            _ => Read::Unreadable,
        },
    }
}

/// Adds `pending` to the file at `path`, atomically (§5). A file that
/// can't be read is left alone and nothing is added. Blocking: run it off
/// the UI thread.
pub fn add_to_file(path: &Path, pending: &Pending, version: &str) -> std::io::Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let mut s = match read(path) {
        Read::Missing => Summary::default(),
        Read::Ok(s) => *s,
        Read::Unreadable => return Ok(()),
    };
    s.add(pending, &today_utc(), version);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(&s).map_err(std::io::Error::other)?;
    text.push('\n');
    coldopen_core::io::atomic_write(path, text.as_bytes())
}

/// One write to `usage.json` at a time, from the timer's thread or the UI's.
static FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// [`add_to_file`], one at a time.
pub fn add_to_file_locked(path: &Path, pending: &Pending, version: &str) -> std::io::Result<()> {
    let _held = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    add_to_file(path, pending, version)
}

/// Deletes `usage.json` (Reset, §7).
pub fn reset(path: &Path) -> std::io::Result<()> {
    let _held = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// The counts waiting in memory and whether counting is on, shared by the
/// app and the dispatcher's hook (§2.2, §6).
#[derive(Clone, Debug, Default)]
pub struct Counter {
    on: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pending: std::sync::Arc<std::sync::Mutex<Pending>>,
}

impl Counter {
    pub fn is_on(&self) -> bool {
        self.on.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Turns counting on or off. Off drops what was waiting (§6).
    pub fn set_on(&self, on: bool) {
        self.on.store(on, std::sync::atomic::Ordering::Relaxed);
        if !on {
            *self.lock() = Pending::default();
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Counts with `f`, only while counting is on.
    pub fn count(&self, f: impl FnOnce(&mut Pending)) {
        if self.is_on() {
            f(&mut self.lock());
        }
    }

    /// What's waiting, taken out to be written.
    pub fn take(&self) -> Pending {
        std::mem::take(&mut *self.lock())
    }

    /// Whether anything is waiting.
    pub fn waiting(&self) -> bool {
        !self.lock().is_empty()
    }

    /// The dispatcher's hook: each command, by its author (§2.2).
    pub fn hook(&self) -> std::sync::Arc<coldopen_core::CommandHookFn> {
        let c = self.clone();
        std::sync::Arc::new(move |name: &'static str, by: &coldopen_core::Author| {
            c.count(|p| {
                if by.is_writer() {
                    p.command(name);
                } else {
                    p.agent_command(name);
                }
            });
        })
    }
}

/// The version without its commit (`0.1.0-alpha.3`).
pub fn app_version() -> &'static str {
    coldopen_core::build_info::VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_civil_dates() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(20_397), "2025-11-05");
        assert_eq!(day_of(20_762), "2026-11-05");
        assert_eq!(day_of(11_016), "2000-02-29");
    }

    #[test]
    fn only_known_names_count() {
        let mut p = Pending::default();
        p.command("text.insert");
        p.command("app.palette");
        p.command("recent:3");
        p.command("MAYA");
        p.command("C:\\Users\\dana\\script.fountain");
        p.command("recent");
        p.agent_command("block.insert");
        p.agent_command("app.palette");
        p.opened("fdx");
        p.opened("docx");
        assert_eq!(
            p.commands.keys().copied().collect::<Vec<_>>(),
            ["app.palette", "recent", "text.insert"]
        );
        assert_eq!(p.agent_commands.len(), 1);
        assert_eq!(p.opened.len(), 1);
    }

    #[test]
    fn plain_words() {
        let mut p = Pending::default();
        p.session();
        p.session();
        p.session_end(Duration::from_secs(40 * 60), "cpu");
        for _ in 0..3 {
            p.command("text.insert");
        }
        p.command("doc.save");
        p.saved("fountain");
        let mut s = Summary::default();
        s.add(&p, "2026-11-05", "0.1.0-alpha.3");
        assert!(s.keys_known());
        let t = s.text();
        assert!(t.starts_with(
            "Usage summary from Cold Open, counted on this computer since 2026-11-05.\n"
        ));
        assert!(t.contains("Sessions: 2 (under 5 minutes: 0; 5 to 30 minutes: 0; 30 minutes to 2 hours: 1; over 2 hours: 0; still open or ended by a crash: 1)"));
        assert!(t.contains("Drawn with: CPU in 1 session\n"));
        assert!(t.contains("Scripts opened: none\n"));
        assert!(t.contains("Scripts saved: Fountain 1\n"));
        assert!(t.contains("Commands you used: text.insert 3, doc.save 1\n"));
        assert!(t.contains("Commands agents used: none\n"));
    }

    #[test]
    fn the_file_adds_up_and_keeps_since() {
        let dir = tempfile::tempdir().unwrap();
        let f = path(dir.path());
        let mut p = Pending::default();
        p.command("undo");
        add_to_file(&f, &p, "0.1.0-alpha.3").unwrap();
        add_to_file(&f, &p, "0.1.0-alpha.4").unwrap();
        let Read::Ok(s) = read(&f) else {
            panic!("unreadable")
        };
        assert_eq!(s.commands["undo"], 2);
        assert_eq!(s.app_versions, ["0.1.0-alpha.3", "0.1.0-alpha.4"]);
        assert_eq!(s.since, today_utc());
    }

    #[test]
    fn a_broken_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let f = path(dir.path());
        std::fs::write(&f, "{ nope").unwrap();
        let mut p = Pending::default();
        p.command("undo");
        add_to_file(&f, &p, "0.1.0-alpha.3").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{ nope");
        assert_eq!(read(&f), Read::Unreadable);
        // A key from outside the tables makes the file unreadable too.
        std::fs::write(&f, r#"{"version":1,"commands":{"MAYA":1}}"#).unwrap();
        assert_eq!(read(&f), Read::Unreadable);
    }
}
