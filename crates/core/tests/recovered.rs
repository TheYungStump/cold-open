//! Recovered versions (docs/specs/journal.md, "Recovered versions" and
//! "Hidden files"): set-aside copies are listed, restored as new unsaved
//! scripts that survive a crash, deleted one by one, and hidden on Windows.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Author, Command, Dispatcher, Options, journal};
use serde_json::{Value, json};

fn run(d: &mut Dispatcher, command: &str, params: Value) -> Result<Value, String> {
    d.execute(Command::from_parts(command, params).unwrap())
        .map_err(|e| e.code.to_string())
}

fn ok(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    run(d, command, params).unwrap_or_else(|e| panic!("{command}: {e}"))
}

/// A script with unsaved work that was set aside with Don't save.
fn discarded(dir: &Path) -> (PathBuf, PathBuf) {
    let p = dir.join("s.fountain");
    fs::write(&p, "INT. LAB - NIGHT\n\nRain.\n").unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "Thunder, set aside."}),
    );
    let r = ok(&mut d, "doc.close", json!({"discard": true}));
    let copy = PathBuf::from(r["warnings"][0]["details"]["copy"].as_str().unwrap());
    assert!(copy.is_file(), "{r}");
    (p, copy)
}

fn texts(d: &mut Dispatcher) -> Vec<String> {
    ok(d, "doc.get", json!({}))["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["text"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn copies_are_listed_restored_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let (script, copy) = discarded(dir.path());
    let script_bytes = fs::read(&script).unwrap();
    let copy_bytes = fs::read(&copy).unwrap();

    let mut d = Dispatcher::new(Options::default());
    let list = ok(&mut d, "recovered.list", json!({"path": script}));
    let copies = list["copies"].as_array().unwrap();
    assert_eq!(copies.len(), 1, "{list}");
    assert_eq!(copies[0]["kind"], "discarded");
    assert_eq!(copies[0]["readable"], true);
    assert_eq!(copies[0]["copy"], copy.display().to_string());

    // Restored as a new, unsaved script; nothing else is touched.
    let unsaved = dir.path().join("Unsaved");
    fs::create_dir(&unsaved).unwrap();
    let to = unsaved.join("s (restored).fountain");
    let r = ok(
        &mut d,
        "recovered.restore",
        json!({"copy": copy, "path": to}),
    );
    assert_eq!(r["format"], "fountain");
    assert_eq!(ok(&mut d, "doc.get", json!({}))["dirty"], true);
    assert!(texts(&mut d).iter().any(|t| t == "Thunder, set aside."));
    assert!(!to.exists(), "nothing is written until the writer saves");
    assert_eq!(fs::read(&script).unwrap(), script_bytes);
    assert_eq!(fs::read(&copy).unwrap(), copy_bytes);
    drop(d); // a crash
    let header = fs::read_to_string(journal::path_for(&to)).unwrap();
    assert!(header.lines().next().unwrap().contains("\"unsaved\":true"));

    // The restored script comes back after a crash.
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": to}));
    assert!(r["recovered"].is_object(), "{r}");
    assert!(texts(&mut d).iter().any(|t| t == "Thunder, set aside."));
    // Saving it writes a header for a file, which no longer says unsaved.
    ok(&mut d, "doc.save", json!({}));
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "After."}),
    );
    drop(d);
    let header = fs::read_to_string(journal::path_for(&to)).unwrap();
    assert!(!header.contains("\"unsaved\""), "{header}");

    // Deleting removes exactly that copy, and only copies can be deleted.
    let mut d = Dispatcher::new(Options::default());
    assert_eq!(
        run(&mut d, "recovered.delete", json!({"copy": script})).unwrap_err(),
        "invalid_params"
    );
    assert!(script.exists());
    ok(&mut d, "recovered.delete", json!({"copy": copy}));
    assert!(!copy.exists());
    let list = ok(&mut d, "recovered.list", json!({"path": script}));
    assert_eq!(list["copies"], json!([]));
}

#[test]
fn agents_may_list_but_not_restore_or_delete() {
    let dir = tempfile::tempdir().unwrap();
    let (script, copy) = discarded(dir.path());
    let mut d = Dispatcher::new(Options::default());
    let agent = Author::agent("claude-code", None);
    let run_as = |d: &mut Dispatcher, name: &str, params: Value| {
        d.execute_as(Command::from_parts(name, params).unwrap(), &agent)
            .map_err(|e| e.code)
    };
    assert!(run_as(&mut d, "recovered.list", json!({"path": script})).is_ok());
    let to = dir.path().join("r.fountain");
    assert_eq!(
        run_as(
            &mut d,
            "recovered.restore",
            json!({"copy": copy, "path": to})
        )
        .unwrap_err(),
        "writer_only"
    );
    assert_eq!(
        run_as(&mut d, "recovered.delete", json!({"copy": copy})).unwrap_err(),
        "writer_only"
    );
    assert!(copy.exists());
}

#[test]
fn a_copy_that_cannot_be_read_is_listed_but_not_restored() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, "Rain.\n").unwrap();
    // A journal whose header cannot be read is kept aside when opened.
    fs::write(journal::path_for(&p), "{not a header\n").unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"][0]["code"], "journal_unreadable", "{r}");
    let copy = r["warnings"][0]["details"]["copy"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&mut d, "doc.close", json!({}));
    let list = ok(&mut d, "recovered.list", json!({"path": p}));
    assert_eq!(list["copies"][0]["kind"], "corrupt");
    assert_eq!(list["copies"][0]["readable"], false);
    let to = dir.path().join("r.fountain");
    assert_eq!(
        run(
            &mut d,
            "recovered.restore",
            json!({"copy": copy, "path": to})
        )
        .unwrap_err(),
        "copy_unreadable"
    );
}

#[test]
fn a_damaged_original_is_listed_and_restored() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("b.fdx");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx/broken-markup.fdx"),
        &p,
    )
    .unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    ok(&mut d, "doc.save", json!({}));
    let list = ok(&mut d, "recovered.list", json!({}));
    assert_eq!(list["copies"][0]["kind"], "original", "{list}");
    let copy = list["copies"][0]["copy"].as_str().unwrap().to_string();
    ok(&mut d, "doc.close", json!({}));
    let to = dir.path().join("b (restored).fdx");
    let r = ok(
        &mut d,
        "recovered.restore",
        json!({"copy": copy, "path": to}),
    );
    assert_eq!(r["format"], "fdx");
    assert!(
        texts(&mut d)
            .iter()
            .any(|t| t == "Sparks fly from the grinder.")
    );
}

/// Windows ignores the leading dot, so journals and copies get the hidden
/// attribute there (docs/specs/journal.md, "Hidden files").
#[cfg(windows)]
#[test]
fn journals_and_copies_are_hidden_on_windows() {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    let hidden = |p: &Path| fs::metadata(p).unwrap().file_attributes() & HIDDEN != 0;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, "Rain.\n").unwrap();
    // A journal from before phase 4 is made hidden when next opened.
    fs::write(journal::path_for(&p), "").unwrap();
    assert!(!hidden(&journal::path_for(&p)));
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    assert!(hidden(&journal::path_for(&p)));
    assert!(!hidden(&p), "the script itself stays visible");
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "x"}),
    );
    ok(&mut d, "doc.save", json!({}));
    assert!(!hidden(&p), "saving keeps the script visible");
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "y"}),
    );
    let r = ok(&mut d, "doc.close", json!({"discard": true}));
    let copy = PathBuf::from(r["warnings"][0]["details"]["copy"].as_str().unwrap());
    assert!(hidden(&copy));
}
