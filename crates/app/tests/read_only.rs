//! A script in a folder that can't be written opens as a copy in
//! `Unsaved/` (docs/specs/editor.md §22, decided 2026-10-30).

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot};

/// A folder made read-only for this test, and writable again after it.
/// `None` where the OS lets this user write anyway (root, for one).
struct ReadOnly(PathBuf);

impl ReadOnly {
    fn new(dir: &Path) -> Option<ReadOnly> {
        let r = ReadOnly(dir.to_path_buf());
        r.set(true);
        if fs::write(dir.join("probe"), b"x").is_ok() {
            let _ = fs::remove_file(dir.join("probe"));
            eprintln!("skipped: this user can still write to a read-only folder");
            return None;
        }
        Some(r)
    }

    fn set(&self, read_only: bool) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if read_only { 0o555 } else { 0o755 };
            fs::set_permissions(&self.0, fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(windows)]
        {
            let dir = self.0.display().to_string();
            let args: Vec<&str> = if read_only {
                vec![&dir, "/deny", "*S-1-1-0:(AD,WD)"]
            } else {
                vec![&dir, "/remove:d", "*S-1-1-0"]
            };
            let out = std::process::Command::new("icacls")
                .args(&args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

impl Drop for ReadOnly {
    fn drop(&mut self) {
        self.set(false);
    }
}

#[test]
fn a_script_in_a_read_only_folder_opens_as_a_copy() {
    let home = tempfile::tempdir().unwrap();
    let shared = tempfile::tempdir().unwrap();
    let script = shared.path().join("pilot.fountain");
    let text = "INT. HOUSE - DAY\n\nRain.\n";
    fs::write(&script, text).unwrap();
    let Some(_ro) = ReadOnly::new(shared.path()) else {
        return;
    };
    let dirs = Dirs::under(home.path());
    let mut app = App::new(Boot {
        file: Some(script.clone()),
        dirs: Some(dirs.clone()),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    let mut h = Headless::new(app, iced::Size::new(1000.0, 700.0)).unwrap();
    let copy = dirs.unsaved().join("pilot (copy).fountain");
    let s = h.app.d.session().expect("the copy is open");
    assert_eq!(s.path.as_deref(), Some(copy.as_path()));
    assert!(
        h.app
            .banners
            .iter()
            .any(|b| b.contains("can't be written, so this is a copy")),
        "{:?}",
        h.app.banners
    );
    // Typing goes to the copy's journal; the original is untouched.
    h.doc_end();
    h.type_text("x");
    assert!(coldopen_core::journal::path_for(&copy).exists());
    assert_eq!(fs::read_to_string(&script).unwrap(), text);
    assert_eq!(fs::read_dir(shared.path()).unwrap().count(), 1);
    h.app.d.discard();
}
