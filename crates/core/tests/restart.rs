//! Starting the journal again off the UI thread (docs/specs/journal.md,
//! "Starting the journal again off the UI thread", 2026-11-02): a child
//! process saves, types during the restart, and is killed at each of its
//! steps. Reopening must give back every edit the child had made, with the
//! journal whole and nothing left beside it.

use std::fs;
use std::path::Path;
use std::process::Command as Process;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use coldopen_core::command::DocSave;
use coldopen_core::{Command, Dispatcher, Durability, Options, io, journal};
use serde_json::{Value, json};

const SCRIPT_TEXT: &str = "INT. LAB - NIGHT\n\nRain.\n";

fn ok(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    let cmd = Command::from_parts(command, params).unwrap();
    match d.execute(cmd) {
        Ok(v) => v,
        Err(e) => panic!("{command} failed: {e}"),
    }
}

/// Types at the end of the second block, one character at a time.
fn type_at_end(d: &mut Dispatcher, s: &str) {
    let block = ok(d, "doc.get", json!({}))["blocks"][1].clone();
    let len = block["text"].as_str().unwrap().chars().count();
    for (k, c) in s.chars().enumerate() {
        ok(
            d,
            "text.insert",
            json!({"id": block["id"], "at": len + k, "text": c.to_string()}),
        );
    }
}

fn text(d: &mut Dispatcher) -> String {
    ok(d, "doc.export", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

// ---- The child --------------------------------------------------------

/// Where the child dies, from `COLDOPEN_RESTART_KILL`.
static KILL: OnceLock<String> = OnceLock::new();
/// Set once the child has typed during the restart: until then the tidying
/// waits, so the edits land between the switch and the rename.
static TYPED: AtomicBool = AtomicBool::new(false);
static TIDIED: AtomicBool = AtomicBool::new(false);

fn die_if(point: &str) {
    if KILL.get().is_some_and(|k| k == point) {
        eprintln!("killed at {point}");
        std::process::abort();
    }
}

fn hook(step: &str) {
    die_if(step);
    match step {
        "tidy_started" => {
            let until = Instant::now() + Duration::from_secs(10);
            while !TYPED.load(Ordering::SeqCst) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        "tidied" => TIDIED.store(true, Ordering::SeqCst),
        _ => {}
    }
}

/// Saves as the app does, with the worker's steps on this thread.
fn save(d: &mut Dispatcher, during: Option<&str>) -> Value {
    let job = d.save_prepare(&DocSave::default()).unwrap();
    let prepared = d.save_mark(job.render()).unwrap();
    let written = io::atomic_write(&prepared.target, &prepared.bytes);
    prepared.finisher().run(written.is_ok());
    if let Some(s) = during {
        type_at_end(d, s);
    }
    d.save_commit(prepared, written).unwrap()
}

/// Run by the parent as its own process; does nothing in a normal run.
#[test]
fn child() {
    let (Ok(kill), Ok(dir)) = (
        std::env::var("COLDOPEN_RESTART_KILL"),
        std::env::var("COLDOPEN_RESTART_DIR"),
    ) else {
        return;
    };
    KILL.set(kill.clone()).unwrap();
    journal::set_restart_hook(hook);
    let p = Path::new(&dir).join("save.fountain");
    let mut d = Dispatcher::new(Options::default());
    d.set_durability(Durability::APP);
    ok(&mut d, "doc.open", json!({"path": p}));
    type_at_end(&mut d, " A.");
    if kill == "no_switch" {
        // An edit while the file is written: no switch, a stale `.next`.
        let r = save(&mut d, Some(" D."));
        assert_eq!(r["dirty"], true, "{r}");
        type_at_end(&mut d, " E.");
        die_if("no_switch");
    }
    let r = save(&mut d, None);
    assert_eq!(r["dirty"], false, "{r}");
    // The tidying is held: these go into the next journal before it is
    // renamed into place.
    type_at_end(&mut d, " B1. B2.");
    die_if("typed");
    TYPED.store(true, Ordering::SeqCst);
    let until = Instant::now() + Duration::from_secs(10);
    while !TIDIED.load(Ordering::SeqCst) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(TIDIED.load(Ordering::SeqCst), "the tidying never finished");
    type_at_end(&mut d, " C.");
    die_if("after");
    panic!("the child was never killed at {kill}");
}

// ---- The parent -------------------------------------------------------

/// Kills the child at `point`, then reopens the script as the app would.
fn kill_and_reopen(point: &str) -> (tempfile::TempDir, Dispatcher, Value) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("save.fountain");
    fs::write(&p, SCRIPT_TEXT).unwrap();
    let out = Process::new(std::env::current_exe().unwrap())
        .args(["--exact", "child", "--nocapture", "--test-threads=1"])
        .env("COLDOPEN_RESTART_KILL", point)
        .env("COLDOPEN_RESTART_DIR", dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&format!("killed at {point}")),
        "{point}: the child was not killed there:\n{stderr}"
    );
    assert!(!out.status.success());
    let mut d = Dispatcher::new(Options::default());
    d.set_durability(Durability::APP);
    let r = ok(&mut d, "doc.open", json!({"path": p}));
    (dir, d, r)
}

/// Every file beside the script but the script and its journal.
fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != "save.fountain" && n != ".save.fountain.coldopen-journal")
        .collect()
}

#[test]
fn a_kill_at_each_step_of_the_restart_loses_no_edit() {
    let saved = "INT. LAB - NIGHT\n\nRain. A.\n";
    let cases = [
        // Before the switch: the old journal, or the `.next` holding only
        // a header, which hold the same saved state.
        ("next_written", saved, false),
        ("switched", saved, false),
        // After the switch, edits typed into the `.next`, with the tidying
        // held until they are in.
        ("typed", "INT. LAB - NIGHT\n\nRain. A. B1. B2.\n", true),
        ("old_closed", "INT. LAB - NIGHT\n\nRain. A. B1. B2.\n", true),
        ("renamed", "INT. LAB - NIGHT\n\nRain. A. B1. B2.\n", true),
        ("tidied", "INT. LAB - NIGHT\n\nRain. A. B1. B2.\n", true),
        ("after", "INT. LAB - NIGHT\n\nRain. A. B1. B2. C.\n", true),
        // An edit during the save: no switch, and the stale `.next` goes.
        ("no_switch", "INT. LAB - NIGHT\n\nRain. A. D. E.\n", true),
        // The tidying starts as the switch is made, while the child types:
        // a kill there may come after any number of the typed keys, and
        // every one typed must come back. Checked below.
        (
            "tidy_started",
            "INT. LAB - NIGHT\n\nRain. A. B1. B2.\n",
            true,
        ),
    ];
    for (point, want, recovers) in cases {
        let (dir, mut d, r) = kill_and_reopen(point);
        let p = dir.path().join("save.fountain");
        let got = text(&mut d);
        let want = if point == "tidy_started" {
            // Any state from the save to the last key typed before it.
            let typed = got
                .strip_prefix("INT. LAB - NIGHT\n\nRain. A.")
                .and_then(|t| t.strip_suffix('\n'))
                .unwrap_or_else(|| panic!("{point}: {got:?}"));
            assert!(" B1. B2.".starts_with(typed), "{point}: {got:?}");
            assert_eq!(r["recovered"].is_object(), !typed.is_empty(), "{r}");
            got.clone()
        } else {
            assert_eq!(r["recovered"].is_object(), recovers, "{point}: {r}");
            want.to_string()
        };
        assert_eq!(got, want, "{point}: {r}");
        assert_eq!(fs::read_to_string(&p).unwrap(), saved, "{point}");
        assert_eq!(leftovers(dir.path()), Vec::<String>::new(), "{point}");
        // The journal recovery left goes on: an edit, a close that keeps
        // it, and the same again.
        type_at_end(&mut d, " F.");
        let want = format!("{} F.\n", want.trim_end_matches('\n'));
        d.shutdown();
        let mut d = Dispatcher::new(Options::default());
        let r = ok(&mut d, "doc.open", json!({"path": p}));
        assert_eq!(text(&mut d), want, "{point}, reopened: {r}");
        d.shutdown();
    }
}

/// A `.next` that holds entries but doesn't continue the journal (its
/// header names another place) is never lost: it is copied aside.
#[test]
fn a_stale_next_journal_with_entries_is_copied_aside() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("save.fountain");
    fs::write(&p, SCRIPT_TEXT).unwrap();
    let mut d = Dispatcher::new(Options::default());
    d.set_durability(Durability::APP);
    ok(&mut d, "doc.open", json!({"path": p}));
    type_at_end(&mut d, " A.");
    d.shutdown();
    let j = journal::path_for(&p);
    let bytes = fs::read(&j).unwrap();
    let next = journal::next_path(&j);
    fs::write(&next, &bytes).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": p}));
    assert_eq!(text(&mut d), "INT. LAB - NIGHT\n\nRain. A.\n", "{r}");
    assert!(!next.exists());
    let copies: Vec<String> = leftovers(dir.path())
        .into_iter()
        .filter(|n| n.contains(".coldopen-journal.next-"))
        .collect();
    assert_eq!(copies.len(), 1, "{:?}", leftovers(dir.path()));
    d.shutdown();
}
