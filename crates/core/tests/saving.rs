//! Saving off the UI thread and deferred journal flushing
//! (docs/specs/journal.md, "Durability" and "Saving off the UI thread").

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use coldopen_core::command::DocSave;
use coldopen_core::{Command, Dispatcher, Durability, Options, io};
use serde_json::{Value, json};

fn app() -> Dispatcher {
    let mut d = Dispatcher::new(Options::default());
    d.set_durability(Durability::APP);
    d
}

fn ok(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    let cmd = Command::from_parts(command, params).unwrap();
    match d.execute(cmd) {
        Ok(v) => v,
        Err(e) => panic!("{command} failed: {e}"),
    }
}

fn script(dir: &Path) -> PathBuf {
    let p = dir.join("save.fountain");
    fs::write(&p, "INT. LAB - NIGHT\n\nRain.\n").unwrap();
    p
}

fn type_at_end(d: &mut Dispatcher, s: &str) {
    let len = ok(d, "doc.get", json!({}))["blocks"][1]["text"]
        .as_str()
        .unwrap()
        .chars()
        .count();
    for (k, c) in s.chars().enumerate() {
        ok(
            d,
            "text.insert",
            json!({"id": "b_2", "at": len + k, "text": c.to_string()}),
        );
    }
}

fn text(d: &mut Dispatcher) -> String {
    ok(d, "doc.export", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The `recovered` report without `last`, which comes from the wall clock;
/// `last` must be a recent time.
fn recovered(r: &Value) -> Value {
    let mut v = r["recovered"].clone();
    let last = v.as_object_mut().and_then(|o| o.remove("last"));
    assert!(
        last.and_then(|l| l.as_u64())
            .is_some_and(|t| t > 1_600_000_000_000),
        "{r}"
    );
    v
}

#[test]
fn deferred_flushing_catches_up() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut d = app();
    ok(&mut d, "doc.open", json!({"path": p}));
    type_at_end(&mut d, " Wind.");
    let t = Instant::now();
    loop {
        let st = d.journal_status().unwrap();
        if st.pending == 0 {
            assert_eq!(st.error, None);
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "{st:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
    // A plain dispatcher has no flusher to report on.
    let mut plain = Dispatcher::new(Options::default());
    ok(&mut plain, "doc.new", json!({}));
    assert_eq!(plain.journal_status(), None);
}

#[test]
fn a_save_with_no_edits_meanwhile_restarts_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut d = app();
    ok(&mut d, "doc.open", json!({"path": p}));
    type_at_end(&mut d, " Wind.");
    let prepared = d.save_prepare(&DocSave::default()).unwrap();
    let written = io::atomic_write(&prepared.target, &prepared.bytes);
    let r = d.save_commit(prepared, written).unwrap();
    assert_eq!(r["dirty"], false);
    assert_eq!(
        fs::read_to_string(&p).unwrap(),
        "INT. LAB - NIGHT\n\nRain. Wind.\n"
    );
    // The journal holds a fresh header and no entries.
    let j = d.session().unwrap().journal.as_ref().unwrap();
    assert_eq!(j.entries(), 0);
    assert_eq!(ok(&mut d, "doc.get", json!({}))["dirty"], false);
}

#[test]
fn edits_during_a_save_stay_in_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut d = app();
    ok(&mut d, "doc.open", json!({"path": p}));
    type_at_end(&mut d, " Wind.");
    let prepared = d.save_prepare(&DocSave::default()).unwrap();
    // The writer keeps typing while the worker writes the file.
    type_at_end(&mut d, " Hail.");
    let written = io::atomic_write(&prepared.target, &prepared.bytes);
    let r = d.save_commit(prepared, written).unwrap();
    assert_eq!(r["dirty"], true);
    assert_eq!(
        fs::read_to_string(&p).unwrap(),
        "INT. LAB - NIGHT\n\nRain. Wind.\n"
    );
    let live = text(&mut d);
    drop(d); // crash

    let mut d2 = app();
    let r = ok(&mut d2, "doc.open", json!({"path": p}));
    // The saved file is one the journal knows about, so it is not an
    // outside change, and the edits typed during the save come back.
    assert_eq!(r["recovered"]["external_change"], false);
    assert_eq!(text(&mut d2), live);
}

#[test]
fn a_crash_before_the_file_is_replaced_recovers_everything() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut d = app();
    ok(&mut d, "doc.open", json!({"path": p}));
    type_at_end(&mut d, " Wind.");
    let prepared = d.save_prepare(&DocSave::default()).unwrap();
    let live = text(&mut d);
    drop(prepared);
    drop(d); // crash before the worker wrote anything

    let mut d2 = app();
    let r = ok(&mut d2, "doc.open", json!({"path": p}));
    assert_eq!(
        recovered(&r),
        json!({"commands": 6, "external_change": false})
    );
    assert_eq!(text(&mut d2), live);
}

#[test]
fn a_crash_after_the_file_is_replaced_needs_no_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut d = app();
    ok(&mut d, "doc.open", json!({"path": p}));
    type_at_end(&mut d, " Wind.");
    let prepared = d.save_prepare(&DocSave::default()).unwrap();
    io::atomic_write(&prepared.target, &prepared.bytes).unwrap();
    drop(d); // crash before the commit

    let mut d2 = app();
    let r = ok(&mut d2, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"], Value::Null);
    assert_eq!(text(&mut d2), "INT. LAB - NIGHT\n\nRain. Wind.\n");
}

#[test]
fn a_failed_write_keeps_the_document_unsaved() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut d = app();
    ok(&mut d, "doc.open", json!({"path": p}));
    type_at_end(&mut d, "!");
    let prepared = d.save_prepare(&DocSave::default()).unwrap();
    let e = d
        .save_commit(prepared, Err(std::io::Error::other("disk full")))
        .unwrap_err();
    assert_eq!(e.code, "io_error");
    assert_eq!(ok(&mut d, "doc.get", json!({}))["dirty"], true);
}

#[test]
fn prepare_refuses_save_as_and_untitled() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut d = app();
    ok(&mut d, "doc.open", json!({"path": p}));
    let other = DocSave {
        path: Some(dir.path().join("other.fountain").display().to_string()),
        ..Default::default()
    };
    assert_eq!(d.save_prepare(&other).unwrap_err().code, "invalid_params");
    let mut u = app();
    ok(&mut u, "doc.new", json!({}));
    assert_eq!(
        u.save_prepare(&DocSave::default()).unwrap_err().code,
        "no_path"
    );
}
