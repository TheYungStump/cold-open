//! The `.coldopen` bundle (docs/specs/bundle.md) through the dispatcher:
//! a lossless save and reopen, ids and changes that survive it, and the
//! crash journal that names the change table instead of copying it.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Author, Command, CommandError, Dispatcher, Options, journal};
use serde_json::{Value, json};

const SCRIPT: &str = "\
INT. KITCHEN - DAY

Maya makes toast.

MAYA
Breakfast is ready.

EXT. GARDEN - DAY

Theo waters plants.
";

fn writer() -> Author {
    Author::named_writer("Maya")
}

fn agent() -> Author {
    Author::agent("claude-code", Some("text_replace".into()))
}

fn run(
    d: &mut Dispatcher,
    by: &Author,
    command: &str,
    params: Value,
) -> Result<Value, CommandError> {
    d.execute_as(Command::from_parts(command, params).unwrap(), by)
}

fn ok(d: &mut Dispatcher, by: &Author, command: &str, params: Value) -> Value {
    run(d, by, command, params).unwrap_or_else(|e| panic!("{command} failed: {e}"))
}

fn get(d: &mut Dispatcher) -> Value {
    ok(d, &writer(), "doc.get", json!({}))
}

fn changes(d: &mut Dispatcher) -> Value {
    ok(d, &writer(), "changes.list", json!({}))["changes"].clone()
}

fn header(d: &mut Dispatcher) -> Value {
    let j = String::from_utf8(d.journal_bytes().unwrap()).unwrap();
    serde_json::from_str(j.lines().next().unwrap()).unwrap()
}

/// A Fountain script with an agent's tracked change, saved as a bundle.
fn project(dir: &Path) -> (Dispatcher, PathBuf) {
    let f = dir.join("s.fountain");
    fs::write(&f, SCRIPT).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &writer(), "doc.open", json!({"path": f}));
    ok(
        &mut d,
        &agent(),
        "text.replace",
        json!({"find": "toast", "with": "eggs"}),
    );
    let b = dir.join("s.coldopen");
    let r = ok(&mut d, &writer(), "doc.save", json!({"path": b}));
    assert_eq!(r["format"], "coldopen");
    assert_eq!(r["losses"], json!([]), "a bundle holds everything: {r}");
    (d, b)
}

#[test]
fn a_bundle_keeps_ids_changes_and_switches_across_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(&mut d, &writer(), "changes.track", json!({"writer": true}));
    ok(
        &mut d,
        &writer(),
        "block.insert",
        json!({"after": "b_2", "type": "action", "text": "The toaster pops."}),
    );
    ok(&mut d, &writer(), "doc.save", json!({}));
    let before = (get(&mut d)["blocks"].clone(), changes(&mut d));
    let id = header(&mut d)["doc_id"].clone();
    assert!(id.is_string(), "{}", header(&mut d));
    d.shutdown();

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert_eq!(r["format"], "coldopen");
    assert!(r["recovered"].is_null() && r["repaired"].is_null(), "{r}");
    assert_eq!((get(&mut d)["blocks"].clone(), changes(&mut d)), before);
    // Ids survive, so a new block continues from them.
    let r = ok(
        &mut d,
        &writer(),
        "block.insert",
        json!({"type": "action", "text": "Later."}),
    );
    assert_eq!(r["id"], "b_8", "{r}");
    // The switches came back: the writer's edit is tracked.
    assert!(r["change"].is_string(), "{r}");
    // A save in place keeps the identity.
    ok(&mut d, &writer(), "doc.save", json!({}));
    assert_eq!(header(&mut d)["doc_id"], id);
    d.shutdown();
}

#[test]
fn the_journal_names_the_change_table_instead_of_copying_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _b) = project(dir.path());
    let h = header(&mut d);
    assert_eq!(h["format"], "coldopen");
    assert_eq!(h["semantics"], journal::SEMANTICS);
    assert!(h.get("changes").is_none(), "{h}");
    assert_eq!(h["changes_ref"]["count"], 1, "{h}");
    // A Fountain journal, which has no file to hold the table, copies it.
    ok(
        &mut d,
        &writer(),
        "doc.save",
        json!({"path": dir.path().join("t.fountain"), "allow_lossy": true}),
    );
    let h = header(&mut d);
    assert_eq!(h["changes"]["changes"][0]["id"], "c_1", "{h}");
    assert!(
        h.get("doc_id").is_none(),
        "another file is another document"
    );
    d.discard();
}

#[test]
fn tracked_changes_in_a_bundle_survive_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(&mut d, &agent(), "block.delete", json!({"id": "b_6"}));
    let before = (get(&mut d)["blocks"].clone(), changes(&mut d));
    drop(d); // a crash
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert_eq!(r["recovered"]["external_change"], false, "{r}");
    assert_eq!((get(&mut d)["blocks"].clone(), changes(&mut d)), before);
    d.discard();
}

#[test]
fn a_crash_between_a_save_and_the_journal_restart_loses_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(&mut d, &agent(), "block.delete", json!({"id": "b_6"}));
    let old_journal = d.journal_bytes().unwrap();
    // The save writes the bundle, with a longer table, then restarts the
    // journal; put the old journal back, as a crash between the two
    // would leave it.
    ok(&mut d, &writer(), "doc.save", json!({}));
    let saved = (get(&mut d)["blocks"].clone(), changes(&mut d));
    drop(d);
    let hidden_attr_safe = journal::path_for(&b);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&hidden_attr_safe)
        .unwrap();
    std::io::Write::write_all(&mut f, &old_journal).unwrap();
    drop(f);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert!(r["recovered"].is_null(), "the file holds it all: {r}");
    assert!(
        !r["warnings"].to_string().contains("change_data_missing"),
        "{r}"
    );
    assert_eq!((get(&mut d)["blocks"].clone(), changes(&mut d)), saved);
    d.shutdown();
}

#[test]
fn another_project_at_the_path_is_a_change_by_another_program() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(
        &mut d,
        &writer(),
        "block.insert",
        json!({"type": "action", "text": "UNSAVED."}),
    );
    drop(d); // a crash with unsaved work
    // Someone puts a different project with the same name there.
    let other = dir.path().join("other");
    fs::create_dir(&other).unwrap();
    let (mut d2, b2) = project(&other);
    d2.shutdown();
    fs::copy(&b2, &b).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert_eq!(r["recovered"]["external_change"], true, "{r}");
    assert!(
        get(&mut d)["blocks"].to_string().contains("UNSAVED."),
        "the work is recovered, not dropped"
    );
    d.discard();
}

#[test]
fn undo_and_redo_work_across_a_bundle_save_and_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(&mut d, &writer(), "changes.accept", json!({"all": true}));
    ok(&mut d, &writer(), "doc.save", json!({}));
    // Undo the accept after the save: the marks come back.
    ok(&mut d, &writer(), "undo", json!({}));
    assert_eq!(changes(&mut d).as_array().unwrap().len(), 1);
    let before = (get(&mut d)["blocks"].clone(), changes(&mut d));
    drop(d); // a crash
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert_eq!((get(&mut d)["blocks"].clone(), changes(&mut d)), before);
    d.discard();
}

#[test]
fn saving_a_bundle_as_fountain_asks_and_converting_back_makes_a_new_identity() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _b) = project(dir.path());
    let id = header(&mut d)["doc_id"].clone();
    let e = run(
        &mut d,
        &writer(),
        "doc.save",
        json!({"path": dir.path().join("c.fountain")}),
    )
    .unwrap_err();
    assert_eq!(e.code, "lossy_format");
    ok(
        &mut d,
        &writer(),
        "doc.save",
        json!({"path": dir.path().join("c.fountain"), "allow_lossy": true}),
    );
    let c = dir.path().join("c.coldopen");
    ok(&mut d, &writer(), "doc.save", json!({"path": c}));
    assert_ne!(header(&mut d)["doc_id"], id);
    d.shutdown();
}

#[test]
fn doc_new_at_a_bundle_path_starts_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("new.coldopen");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &writer(), "doc.new", json!({"path": p}));
    assert!(header(&mut d)["doc_id"].is_string());
    ok(
        &mut d,
        &writer(),
        "block.insert",
        json!({"type": "scene_heading", "text": "INT. LAB - NIGHT"}),
    );
    ok(&mut d, &writer(), "doc.save", json!({}));
    d.shutdown();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": p}));
    assert_eq!(r["blocks"], 1, "{r}");
    let e = run(
        &mut d,
        &writer(),
        "doc.export",
        json!({"format": "coldopen"}),
    )
    .unwrap_err();
    assert_eq!(e.code, "unsupported_format");
    d.shutdown();
}
