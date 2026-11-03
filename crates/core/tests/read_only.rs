//! A script in a folder that can't be written (docs/specs/journal.md, "A
//! folder that can't be written", 2026-10-30): it is not opened, since
//! nothing typed would have a journal to go to.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Command, Dispatcher, Options, journal};
use serde_json::json;

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
            // The read-only attribute doesn't stop new files in a folder
            // on Windows; an access rule does. Everyone: no new files, no
            // writing data.
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

fn open(d: &mut Dispatcher, path: &Path) -> Result<serde_json::Value, coldopen_core::CommandError> {
    d.execute(Command::from_parts("doc.open", json!({ "path": path })).unwrap())
}

#[test]
fn a_script_in_a_read_only_folder_is_not_opened() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("pilot.fountain");
    fs::write(&script, "INT. HOUSE - DAY\n\nRain.\n").unwrap();
    let Some(_ro) = ReadOnly::new(dir.path()) else {
        return;
    };
    let mut d = Dispatcher::new(Options::default());
    let e = open(&mut d, &script).unwrap_err();
    assert_eq!(e.code, "folder_read_only", "{}", e.message);
    assert_eq!(
        e.details.unwrap()["path"],
        json!(script.display().to_string())
    );
    assert!(d.session().is_none());
    assert!(!journal::path_for(&script).exists());
    let e = d
        .execute(
            Command::from_parts(
                "doc.new",
                json!({ "path": dir.path().join("new.fountain") }),
            )
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(e.code, "folder_read_only", "{}", e.message);
}

#[test]
fn a_journal_already_there_is_still_recovered_read_only_or_not() {
    // A journal that exists keeps its own error: it may hold work. Here
    // it can be read but the folder takes no new files, so the open goes
    // as far as the journal lets it, and never reports folder_read_only.
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("pilot.fountain");
    fs::write(&script, "INT. HOUSE - DAY\n\nRain.\n").unwrap();
    {
        let mut d = Dispatcher::new(Options::default());
        open(&mut d, &script).unwrap();
        d.execute(
            Command::from_parts(
                "block.insert",
                json!({"type": "action", "text": "Unsaved."}),
            )
            .unwrap(),
        )
        .unwrap();
        // The session ends without a save: the journal keeps the line.
    }
    assert!(journal::path_for(&script).exists());
    let Some(_ro) = ReadOnly::new(dir.path()) else {
        return;
    };
    let mut d = Dispatcher::new(Options::default());
    if let Err(e) = open(&mut d, &script) {
        assert_ne!(e.code, "folder_read_only", "{}", e.message);
    }
}

#[test]
fn save_as_into_a_read_only_folder_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let locked = tempfile::tempdir().unwrap();
    let script = home.path().join("pilot.fountain");
    fs::write(&script, "INT. HOUSE - DAY\n\nRain.\n").unwrap();
    let mut d = Dispatcher::new(Options::default());
    open(&mut d, &script).unwrap();
    let Some(_ro) = ReadOnly::new(locked.path()) else {
        return;
    };
    let target = locked.path().join("pilot.fountain");
    let e = d
        .execute(Command::from_parts("doc.save", json!({ "path": target })).unwrap())
        .unwrap_err();
    assert_eq!(e.code, "folder_read_only", "{}", e.message);
    assert_eq!(fs::read_dir(locked.path()).unwrap().count(), 0);
    assert_eq!(
        d.session().unwrap().path.as_deref(),
        Some(script.as_path()),
        "the script keeps its own file"
    );
}
