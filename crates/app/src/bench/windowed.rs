//! The budgets that need a real window (docs/specs/bench.md §3): the app
//! starts itself as a child process, with its own empty folders, and either
//! exits after its first frame or runs the self-driving benchmark
//! (`app/bench.rs`), printing one line of JSON.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::report::Skipped;

/// How long one windowed run may take.
const TIMEOUT: Duration = Duration::from_secs(180);

/// What starts a windowed run.
pub struct Runner {
    /// The app's own executable.
    pub exe: PathBuf,
    /// The bench's temporary folder; each run gets a folder in it.
    pub work: PathBuf,
    /// The synthetic script, copied for each run.
    pub script: PathBuf,
    pub runs: usize,
}

/// One self-driving run's numbers, as `app/bench.rs` prints them.
#[derive(Clone, Debug, Default)]
pub struct Driven {
    /// The renderers asked for, as `ICED_BACKEND` names them.
    pub asked: String,
    pub median: f64,
    pub p95: f64,
    pub peak_mib: Option<f64>,
    pub adapter: Option<String>,
    pub backend: Option<String>,
    pub scale: Option<f32>,
    pub monitor: Option<(u32, u32)>,
}

impl Runner {
    /// A new folder for one run, with its own copy of the script.
    fn fresh(&self, name: &str) -> Result<(PathBuf, PathBuf), Skipped> {
        let home = self.work.join(name);
        std::fs::create_dir_all(&home).map_err(|_| Skipped::Failed)?;
        let copy = home.join("script.fountain");
        std::fs::copy(&self.script, &copy).map_err(|_| Skipped::Failed)?;
        Ok((home, copy))
    }

    fn command(&self, home: &Path, backend: &str) -> Command {
        let mut c = Command::new(&self.exe);
        c.env("COLDOPEN_HOME", home)
            .env("ICED_BACKEND", backend)
            // Frame to frame without waiting for the screen's refresh
            // (docs/specs/bench.md §3).
            .env("ICED_PRESENT_MODE", "immediate")
            .env("COLDOPEN_BENCH_SIZE", "1280x860")
            .env_remove("COLDOPEN_BENCH")
            .env_remove("COLDOPEN_EXIT_AFTER_FIRST_FRAME")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        c
    }

    /// Cold start: launch to the end of the first frame and exit, `runs`
    /// times. Returns the wall-clock times.
    pub fn cold_start(&self, backend: &str) -> Result<Vec<f64>, Skipped> {
        let mut walls = Vec::new();
        for i in 0..self.runs {
            let (home, _) = self.fresh(&format!("cold{i}"))?;
            let mut c = self.command(&home, backend);
            c.env("COLDOPEN_EXIT_AFTER_FIRST_FRAME", "1");
            let t = Instant::now();
            let out = finish(c)?;
            walls.push(t.elapsed().as_secs_f64() * 1000.0);
            if !out.contains("first frame") {
                return Err(Skipped::NoWindow);
            }
        }
        Ok(walls)
    }

    /// A self-driving run: `kind` is typing, scrolling, reports or memory.
    pub fn driven(&self, kind: &str, backend: &str, frames: usize) -> Result<Driven, Skipped> {
        let (home, copy) = self.fresh(&format!("{kind}-{}", backend.replace(',', "-")))?;
        let mut c = self.command(&home, backend);
        c.env("COLDOPEN_BENCH", kind)
            .env("COLDOPEN_BENCH_FRAMES", frames.to_string())
            .arg(&copy);
        let out = finish(c)?;
        let line = out
            .lines()
            .find(|l| l.starts_with('{'))
            .ok_or(Skipped::NoWindow)?;
        let v: Value = serde_json::from_str(line).map_err(|_| Skipped::Failed)?;
        let f = |k: &str| v[k].as_f64();
        let s = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
        Ok(Driven {
            asked: backend.to_string(),
            median: f("median_ms").ok_or(Skipped::Failed)?,
            p95: f("p95_ms").ok_or(Skipped::Failed)?,
            peak_mib: f("peak_mib"),
            adapter: s("adapter"),
            backend: s("backend"),
            scale: f("scale").map(|x| x as f32),
            monitor: match (f("monitor_width"), f("monitor_height")) {
                (Some(w), Some(h)) => Some((w as u32, h as u32)),
                _ => None,
            },
        })
    }
}

/// Runs a child to its end, within the time allowed, and returns what it
/// printed. A child that fails, or never opens a window, is skipped.
fn finish(mut c: Command) -> Result<String, Skipped> {
    let mut child = c.spawn().map_err(|_| Skipped::Failed)?;
    let mut stdout = child.stdout.take().ok_or(Skipped::Failed)?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut stdout, &mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Skipped::Timeout);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return Err(Skipped::Failed),
        }
    };
    let out = reader.join().unwrap_or_default();
    if !status.success() {
        // A renderer that cannot start ends the app before its first frame.
        return Err(Skipped::NoWindow);
    }
    Ok(out)
}
