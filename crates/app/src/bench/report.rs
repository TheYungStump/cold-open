//! The bench's report: JSON and a readable summary, with nothing personal
//! in either (docs/specs/bench.md §4).

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::machine::Machine;

/// The report format's version.
pub const FORMAT: u32 = 1;

/// Why a step could not run. A fixed list, so no message from the system
/// (which may hold a path) ever reaches the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Skipped {
    NoWindow,
    NoGpu,
    Timeout,
    Failed,
}

/// One budget from `docs/spec.md`'s table, measured.
#[derive(Clone, Debug, Serialize)]
pub struct Budget {
    pub id: &'static str,
    #[serde(skip)]
    pub title: &'static str,
    /// What the value is: `median`, `p95` or `peak`.
    pub stat: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_mib: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_mib: Option<f64>,
    pub pass: Option<bool>,
    pub runs: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<Skipped>,
}

impl Budget {
    /// A time against its limit.
    pub fn ms(
        id: &'static str,
        title: &'static str,
        stat: &'static str,
        value: f64,
        limit: f64,
        runs: usize,
    ) -> Budget {
        Budget {
            id,
            title,
            stat,
            value_ms: Some(round(value)),
            limit_ms: Some(limit),
            value_mib: None,
            limit_mib: None,
            pass: Some(value < limit),
            runs,
            renderer: None,
            skipped: None,
        }
    }

    /// Memory against its limit.
    pub fn mib(id: &'static str, title: &'static str, value: f64, limit: f64) -> Budget {
        Budget {
            id,
            title,
            stat: "peak",
            value_ms: None,
            limit_ms: None,
            value_mib: Some(round(value)),
            limit_mib: Some(limit),
            pass: Some(value < limit),
            runs: 1,
            renderer: None,
            skipped: None,
        }
    }

    /// A budget that could not be measured.
    pub fn skipped(
        id: &'static str,
        title: &'static str,
        stat: &'static str,
        why: Skipped,
    ) -> Budget {
        Budget {
            id,
            title,
            stat,
            value_ms: None,
            limit_ms: None,
            value_mib: None,
            limit_mib: None,
            pass: None,
            runs: 0,
            renderer: None,
            skipped: Some(why),
        }
    }

    pub fn on(mut self, renderer: &str) -> Budget {
        self.renderer = Some(renderer.to_string());
        self
    }
}

/// A number kept for comparison, judged against nothing.
#[derive(Clone, Debug, Serialize)]
pub struct Info {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
    pub median_ms: f64,
    pub p95_ms: f64,
}

/// The app that ran the bench.
#[derive(Clone, Debug, Serialize)]
pub struct AppInfo {
    pub version: &'static str,
    pub commit: &'static str,
    pub target: &'static str,
}

/// The script measured.
#[derive(Clone, Debug, Serialize)]
pub struct ScriptInfo {
    pub name: &'static str,
    pub pages: usize,
    pub blocks: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    #[serde(skip)]
    started: Utc,
    pub bench: u32,
    pub app: AppInfo,
    pub when: String,
    pub quick: bool,
    pub machine: Machine,
    pub script: ScriptInfo,
    pub budgets: Vec<Budget>,
    pub info: Vec<Info>,
    pub passed: bool,
}

impl Report {
    pub fn new(machine: Machine, script: ScriptInfo, quick: bool) -> Report {
        let started = utc_now();
        Report {
            started,
            bench: FORMAT,
            app: AppInfo {
                version: coldopen_core::build_info::VERSION,
                commit: coldopen_core::build_info::COMMIT,
                target: coldopen_core::build_info::TARGET,
            },
            when: started.iso(),
            quick,
            machine,
            script,
            budgets: Vec::new(),
            info: Vec::new(),
            passed: false,
        }
    }

    /// Sets `passed`: every budget measured and met.
    pub fn finish(&mut self) {
        self.passed = !self.budgets.is_empty() && self.budgets.iter().all(|b| b.pass == Some(true));
    }

    pub fn missed(&self) -> usize {
        self.budgets
            .iter()
            .filter(|b| b.pass == Some(false))
            .count()
    }

    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).unwrap_or_default();
        s.push('\n');
        s
    }

    /// The readable summary.
    pub fn to_text(&self) -> String {
        let m = &self.machine;
        let mut out = format!(
            "Cold Open bench: {} ({}), {}\n",
            self.app.version, self.app.commit, self.app.target
        );
        out.push_str(&format!("When: {} (UTC)\n", self.when));
        out.push_str(&format!(
            "Machine: {} ({}), {}, {} cores, {} GiB\n",
            m.os, m.os_version, m.cpu, m.cores, m.memory_gib
        ));
        if !m.gpus.is_empty() {
            out.push_str(&format!("GPU: {}\n", m.gpus.join("; ")));
        }
        if let Some(s) = m.screen {
            out.push_str(&format!(
                "Screen: {}x{} at {}x\n",
                s.width, s.height, s.scale
            ));
        }
        out.push_str(&format!(
            "Renderer by default: {}\nScript: {}, {} pages\n",
            m.renderer_default, self.script.name, self.script.pages
        ));
        if self.quick {
            out.push_str("Quick run: fewer repetitions, not for judging budgets.\n");
        }
        out.push('\n');
        let rows: Vec<[String; 4]> = self
            .budgets
            .iter()
            .map(|b| {
                let result = match (b.value_ms, b.value_mib) {
                    (Some(v), _) if v < 1.0 => format!("{} {v:.3} ms", b.stat),
                    (Some(v), _) => format!("{} {v:.1} ms", b.stat),
                    (_, Some(v)) => format!("{v:.0} MiB"),
                    _ => "not measured".into(),
                };
                let limit = match (b.limit_ms, b.limit_mib) {
                    (Some(l), _) => format!("under {l:.0} ms"),
                    (_, Some(l)) => format!("under {l:.0} MB"),
                    _ => String::new(),
                };
                let verdict = match (b.pass, b.skipped) {
                    (Some(true), _) => "PASS".to_string(),
                    (Some(false), _) => "MISS".to_string(),
                    (None, Some(why)) => format!("SKIPPED ({})", skipped_words(why)),
                    (None, None) => "SKIPPED".to_string(),
                };
                let title = match &b.renderer {
                    Some(r) => format!("{} ({r})", b.title),
                    None => b.title.to_string(),
                };
                [title, result, limit, verdict]
            })
            .collect();
        let w: Vec<usize> = (0..3)
            .map(|k| rows.iter().map(|r| r[k].chars().count()).max().unwrap_or(0))
            .collect();
        for r in &rows {
            out.push_str(&format!(
                "{:<w0$}  {:<w1$}  {:<w2$}  {}\n",
                r[0],
                r[1],
                r[2],
                r[3],
                w0 = w[0],
                w1 = w[1],
                w2 = w[2]
            ));
        }
        if !self.info.is_empty() {
            out.push_str("\nFor comparison:\n");
            for i in &self.info {
                let on = i
                    .renderer
                    .as_deref()
                    .map(|r| format!(" ({r})"))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {}{on}: median {:.1} ms, p95 {:.1} ms\n",
                    i.id, i.median_ms, i.p95_ms
                ));
            }
        }
        out.push('\n');
        out.push_str(&if self.passed {
            "Every budget passed.\n".to_string()
        } else {
            let missed = self.missed();
            let skipped = self.budgets.iter().filter(|b| b.pass.is_none()).count();
            let n = |k: usize| {
                if k == 1 {
                    "1 budget".to_string()
                } else {
                    format!("{k} budgets")
                }
            };
            format!("{} missed, {} not measured.\n", n(missed), n(skipped))
        });
        out
    }

    /// `coldopen-bench-<os>-<arch>-<yyyymmdd-hhmmss>`, in UTC.
    pub fn file_stem(&self) -> String {
        format!(
            "coldopen-bench-{}-{}-{}",
            self.machine.os,
            self.machine.arch,
            self.started.compact()
        )
    }

    /// Writes the two files into `dir`, returning their paths.
    pub fn write(&self, dir: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
        let stem = self.file_stem();
        let json = dir.join(format!("{stem}.json"));
        let txt = dir.join(format!("{stem}.txt"));
        std::fs::write(&json, self.to_json())?;
        std::fs::write(&txt, self.to_text())?;
        Ok((json, txt))
    }
}

fn skipped_words(why: Skipped) -> &'static str {
    match why {
        Skipped::NoWindow => "no window",
        Skipped::NoGpu => "no GPU",
        Skipped::Timeout => "timed out",
        Skipped::Failed => "failed",
    }
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Where the report goes: Downloads, else the Desktop, else the current
/// folder (docs/specs/bench.md §2).
pub fn default_folder() -> PathBuf {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    let mut candidates = Vec::new();
    if cfg!(target_os = "linux")
        && let Some(d) = xdg_user_dir(home.as_deref(), "XDG_DOWNLOAD_DIR")
    {
        candidates.push(d);
    }
    if let Some(h) = &home {
        candidates.push(h.join("Downloads"));
        candidates.push(h.join("Desktop"));
    }
    candidates
        .into_iter()
        .find(|d| d.is_dir())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// A folder from `~/.config/user-dirs.dirs`, as the desktop names it.
fn xdg_user_dir(home: Option<&Path>, key: &str) -> Option<PathBuf> {
    let home = home?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|c| !c.is_empty())
        .map_or_else(|| home.join(".config"), PathBuf::from);
    let text = std::fs::read_to_string(config.join("user-dirs.dirs")).ok()?;
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{key}=")))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(value),
    };
    Some(path)
}

/// A UTC time, for the report and its file name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Utc {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Utc {
    /// From seconds since 1970 (the civil-from-days algorithm).
    pub fn from_unix(secs: u64) -> Utc {
        let days = (secs / 86_400) as i64;
        let rem = secs % 86_400;
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        Utc {
            year,
            month,
            day,
            hour: (rem / 3600) as u32,
            minute: (rem % 3600 / 60) as u32,
            second: (rem % 60) as u32,
        }
    }

    pub fn iso(self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    pub fn compact(self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

pub fn utc_now() -> Utc {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Utc::from_unix(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_dates_are_right() {
        assert_eq!(Utc::from_unix(0).iso(), "1970-01-01T00:00:00Z");
        assert_eq!(Utc::from_unix(951_782_400).iso(), "2000-02-29T00:00:00Z");
        assert_eq!(Utc::from_unix(1_793_368_800).iso(), "2026-10-30T14:00:00Z");
        assert_eq!(Utc::from_unix(1_793_368_800).compact(), "20261030-140000");
    }

    #[test]
    fn the_xdg_download_folder_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(".config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("user-dirs.dirs"),
            "# comment\nXDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\n",
        )
        .unwrap();
        // Only HOME is passed, so XDG_CONFIG_HOME must not point elsewhere.
        if std::env::var_os("XDG_CONFIG_HOME").is_none() {
            assert_eq!(
                xdg_user_dir(Some(dir.path()), "XDG_DOWNLOAD_DIR"),
                Some(dir.path().join("Téléchargements"))
            );
        }
    }
}
