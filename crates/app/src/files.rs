//! Folders and settings (docs/specs/editor.md §11). Folders are found from
//! the usual environment variables, with no extra dependency.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::theme::Choice;

/// The app's folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dirs {
    /// `settings.json`, `keymap.json`, `flow.json`.
    pub config: PathBuf,
    /// `Unsaved/` for scripts never saved.
    pub data: PathBuf,
    /// Agent link session files (docs/specs/mcp.md §5.1).
    pub run: PathBuf,
}

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Dirs {
    /// The folders for this OS and user. `COLDOPEN_HOME`, when set, puts
    /// all three under one folder, for tests and portable use.
    pub fn find() -> Option<Dirs> {
        if let Some(home) = env("COLDOPEN_HOME") {
            return Some(Dirs::under(&home));
        }
        // The run folder is shared with `coldopen mcp`, so both use the
        // same rule.
        let run = coldopen_link::run_dir()?;
        if cfg!(windows) {
            let roaming = env("APPDATA")?;
            let local = env("LOCALAPPDATA").unwrap_or_else(|| roaming.clone());
            return Some(Dirs {
                config: roaming.join("Cold Open"),
                data: local.join("Cold Open"),
                run,
            });
        }
        let home = env("HOME")?;
        if cfg!(target_os = "macos") {
            let base = home.join("Library/Application Support/Cold Open");
            return Some(Dirs {
                config: base.clone(),
                data: base,
                run,
            });
        }
        let config = env("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let data = env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        Some(Dirs {
            config: config.join("coldopen"),
            data: data.join("coldopen"),
            run,
        })
    }

    /// All three folders under one root.
    pub fn under(root: &Path) -> Dirs {
        Dirs {
            config: root.join("config"),
            data: root.join("data"),
            run: root.join("run"),
        }
    }

    pub fn unsaved(&self) -> PathBuf {
        self.data.join("Unsaved")
    }
}

/// Which renderer draws the window (docs/progress/phase-3.md,
/// measurements): the CPU one uses the least memory and keeps up on
/// screens up to about 1080p; the GPU one keeps up on 4K and Retina
/// screens but its driver adds 100 MB or more.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Renderer {
    /// The GPU on macOS, whose screens are nearly all high-density; the
    /// CPU elsewhere.
    #[default]
    Auto,
    Cpu,
    Gpu,
}

impl Renderer {
    /// What iced's `ICED_BACKEND` should say: renderers to try, in order.
    pub fn backends(self) -> &'static str {
        match self {
            Renderer::Auto if cfg!(target_os = "macos") => "wgpu,tiny-skia",
            Renderer::Auto | Renderer::Cpu => "tiny-skia",
            Renderer::Gpu => "wgpu,tiny-skia",
        }
    }
}

/// What `settings.json` remembers (editor.md §11).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Choice,
    pub zoom: f32,
    pub draft: bool,
    pub navigator: bool,
    pub agent_access: bool,
    pub recent: Vec<PathBuf>,
    pub renderer: Renderer,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: Choice::System,
            zoom: 1.0,
            draft: false,
            navigator: true,
            agent_access: true,
            recent: Vec::new(),
            renderer: Renderer::Auto,
        }
    }
}

/// The zoom steps, in percent (editor.md §3).
pub const ZOOMS: [u32; 12] = [50, 67, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];

impl Settings {
    /// Reads `settings.json`; a missing or broken file gives the defaults
    /// and, when broken, a warning.
    pub fn load(dirs: &Dirs) -> (Settings, Option<String>) {
        let p = dirs.config.join("settings.json");
        match std::fs::read_to_string(&p) {
            Err(_) => (Settings::default(), None),
            Ok(t) => match serde_json::from_str::<Settings>(&t) {
                Ok(mut s) => {
                    s.zoom = s.zoom.clamp(0.5, 3.0);
                    s.recent.truncate(10);
                    (s, None)
                }
                Err(e) => (
                    Settings::default(),
                    Some(format!(
                        "settings.json could not be read ({e}); defaults are in use"
                    )),
                ),
            },
        }
    }

    pub fn save(&self, dirs: &Dirs) -> std::io::Result<()> {
        std::fs::create_dir_all(&dirs.config)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        coldopen_core::io::atomic_write(&dirs.config.join("settings.json"), text.as_bytes())
    }

    /// Puts a file at the top of the recent list.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(10);
    }

    pub fn zoom_step(&mut self, delta: i32) {
        let pct = (self.zoom * 100.0).round() as u32;
        let i = ZOOMS
            .iter()
            .position(|&z| z >= pct)
            .unwrap_or(ZOOMS.len() - 1) as i32;
        let j = (i + delta).clamp(0, ZOOMS.len() as i32 - 1) as usize;
        self.zoom = ZOOMS[j] as f32 / 100.0;
    }
}

/// A question for a file dialog (editor.md §11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// A script to open.
    Open,
    /// Where to save the script; the suggested file name.
    SaveAs(String),
    /// Where to write a PDF; the suggested file name.
    ExportPdf(String),
}

/// A dialog's answer, when it comes: a path, or `None` for Cancel.
pub type Answer = Pin<Box<dyn Future<Output = Option<PathBuf>> + Send>>;

/// Answers file dialogs: the system's own dialogs in the app, a script in
/// tests and the gate driver. Given the question and the folder to start
/// in.
#[derive(Clone)]
pub struct Dialogs(pub Arc<dyn Fn(Ask, Option<PathBuf>) -> Answer + Send + Sync>);

impl std::fmt::Debug for Dialogs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Dialogs")
    }
}

impl Dialogs {
    /// The operating system's dialogs.
    pub fn native() -> Dialogs {
        Dialogs(Arc::new(|ask, dir| {
            Box::pin(async move {
                let mut d = rfd::AsyncFileDialog::new();
                if let Some(dir) = dir {
                    d = d.set_directory(dir);
                }
                let picked = match ask {
                    Ask::Open => {
                        d.set_title("Open a script")
                            .add_filter("Screenplays", &["fountain", "spmd", "txt"])
                            .pick_file()
                            .await
                    }
                    Ask::SaveAs(name) => {
                        d.set_title("Save the script")
                            .set_file_name(name)
                            .add_filter("Fountain", &["fountain"])
                            .save_file()
                            .await
                    }
                    Ask::ExportPdf(name) => {
                        d.set_title("Export a PDF")
                            .set_file_name(name)
                            .add_filter("PDF", &["pdf"])
                            .save_file()
                            .await
                    }
                };
                picked.map(|f| f.path().to_path_buf())
            })
        }))
    }

    /// Answers every dialog at once with `f`, for tests and the driver.
    pub fn scripted(f: impl Fn(&Ask) -> Option<PathBuf> + Send + Sync + 'static) -> Dialogs {
        Dialogs(Arc::new(move |ask, _| {
            let answer = f(&ask);
            Box::pin(async move { answer })
        }))
    }
}

/// A chosen path, with `ext` added when the writer typed a name without
/// one.
pub fn with_extension(path: PathBuf, ext: &str) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension(ext)
    }
}

/// The next free `Untitled-<n>.fountain` in `Unsaved/` (editor.md §11).
pub fn next_untitled(dirs: &Dirs) -> PathBuf {
    let dir = dirs.unsaved();
    for n in 1.. {
        let p = dir.join(format!("Untitled-{n}.fountain"));
        if !p.exists() && !coldopen_core::journal::path_for(&p).exists() {
            return p;
        }
    }
    dir.join("Untitled.fountain")
}

/// Scripts whose journal holds unsaved work from an earlier session: in
/// `Unsaved/`, and among the recent files (editor.md §11).
pub fn leftover_work(dirs: &Dirs, recent: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let has_entries =
        |j: &Path| std::fs::read(j).is_ok_and(|b| b.iter().filter(|&&c| c == b'\n').count() > 1);
    if let Ok(rd) = std::fs::read_dir(dirs.unsaved()) {
        let mut names: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        names.sort();
        for j in names {
            let name = j.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if let Some(stem) = name
                .strip_prefix('.')
                .and_then(|n| n.strip_suffix(".coldopen-journal"))
                && has_entries(&j)
            {
                out.push(j.with_file_name(stem));
            }
        }
    }
    // A recent script's journal can hold work with no entries: the header
    // of a save that never reached the disk (journal.md, recovery step 4).
    // So it is judged as opening the script would judge it.
    for p in recent {
        if coldopen_core::dispatcher::journal_holds_work(p) && !out.contains(p) {
            out.push(p.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_tolerate_damage() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs::under(dir.path());
        let mut s = Settings {
            theme: Choice::HighContrast,
            ..Settings::default()
        };
        s.add_recent(Path::new("a.fountain"));
        s.save(&dirs).unwrap();
        assert_eq!(Settings::load(&dirs).0, s);
        std::fs::write(dirs.config.join("settings.json"), "{ nope").unwrap();
        let (d, w) = Settings::load(&dirs);
        assert_eq!(d, Settings::default());
        assert!(w.is_some());
    }

    /// Found by the Antigravity review (its finding 2): the start-up scan
    /// counted lines, so a recent script whose journal holds only the
    /// header of a save lost in a power cut was not offered.
    #[test]
    fn a_recent_script_with_a_lost_save_is_leftover_work() {
        use coldopen_core::{Command, Dispatcher, Options};
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs::under(&dir.path().join("app"));
        let p = dir.path().join("s.fountain");
        let old = "INT. HOUSE - DAY\n\nRain.\n";
        std::fs::write(&p, old).unwrap();
        let mut d = Dispatcher::new(Options::default());
        for (c, params) in [
            ("doc.open", serde_json::json!({"path": p})),
            (
                "block.insert",
                serde_json::json!({"type": "action", "text": "Saved."}),
            ),
            ("doc.save", serde_json::json!({})),
        ] {
            d.execute(Command::from_parts(c, params).unwrap()).unwrap();
        }
        drop(d); // a crash with nothing unsaved: nothing to offer
        let recent = vec![p.clone()];
        assert!(leftover_work(&dirs, &recent).is_empty());
        std::fs::write(&p, old).unwrap(); // the save never reached the disk
        assert_eq!(leftover_work(&dirs, &recent), recent);
    }

    #[test]
    fn zoom_steps() {
        let mut s = Settings::default();
        s.zoom_step(1);
        assert_eq!(s.zoom, 1.1);
        s.zoom_step(-2);
        assert_eq!(s.zoom, 0.9);
        for _ in 0..20 {
            s.zoom_step(1);
        }
        assert_eq!(s.zoom, 3.0);
    }
}
