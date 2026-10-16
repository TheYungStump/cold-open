//! Nothing that holds a writer's unsaved work is deleted, truncated or
//! overwritten unless it is copied aside first, and nothing is written
//! before the checks that could refuse it (docs/specs/journal.md). Each
//! test reproduces a finding from the review of phases 1–3, named in its
//! title (docs/progress/review-fixes.md).

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Author, Command, CommandError, Dispatcher, Options, journal};
use serde_json::{Value, json};

const SCRIPT: &str = "INT. HOUSE - DAY\n\nRain.\n";
const UNSAVED: &str = "THE WRITER'S UNSAVED PAGE.";

fn agent() -> Author {
    Author::agent("claude-code", Some("doc_close".into()))
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

fn err(d: &mut Dispatcher, by: &Author, command: &str, params: Value) -> CommandError {
    match run(d, by, command, params) {
        Ok(r) => panic!("{command} should fail: {r}"),
        Err(e) => e,
    }
}

fn text(d: &mut Dispatcher) -> String {
    ok(d, &Author::Writer, "doc.export", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Leaves `s.fountain` with a crash journal holding one unsaved line, made
/// by `by`, as the app, `serve --stdio` or `coldopen mcp` would after a
/// crash.
fn crashed_with_work(dir: &Path, by: &Author) -> PathBuf {
    let p = dir.join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, by, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        by,
        "block.insert",
        json!({"type": "action", "text": UNSAVED}),
    );
    drop(d); // a crash: the journal stays
    p
}

/// Files in `dir` whose names hold `.<tag>-`, as journal copies do.
fn asides(dir: &Path, tag: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(&format!(".{tag}-"))
        })
        .collect();
    v.sort();
    v
}

#[test]
fn f4_an_agent_cannot_discard_the_writers_recovered_work() {
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &Author::Writer);
    let j = journal::path_for(&p);
    let before = fs::read(&j).unwrap();

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &agent(), "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1);
    let e = err(&mut d, &agent(), "doc.close", json!({"discard": true}));
    assert_eq!(e.code, "not_your_step", "{e}");
    // The session and the journal still hold the writer's work.
    assert!(text(&mut d).contains(UNSAVED));
    d.shutdown();
    assert_eq!(fs::read(&j).unwrap(), before);
    assert!(asides(dir.path(), "discarded").is_empty());
}

#[test]
fn f4_an_agent_cannot_set_the_writers_work_aside_another_way() {
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &Author::Writer);
    let j = journal::path_for(&p);
    let before = fs::read(&j).unwrap();

    let mut d = Dispatcher::new(Options::default());
    let e = err(
        &mut d,
        &agent(),
        "doc.open",
        json!({"path": p, "recover": "discard"}),
    );
    assert_eq!(e.code, "not_your_step", "{e}");
    assert!(d.session().is_none());
    ok(&mut d, &agent(), "doc.open", json!({"path": p}));
    let e = err(&mut d, &agent(), "doc.revert", json!({}));
    assert_eq!(e.code, "not_your_step", "{e}");
    assert!(text(&mut d).contains(UNSAVED));
    d.shutdown();
    assert_eq!(fs::read(&j).unwrap(), before);
    assert!(asides(dir.path(), "discarded").is_empty());
}

#[test]
fn f4_an_agent_may_discard_its_own_recovered_work_and_a_copy_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &agent());
    let before = fs::read(journal::path_for(&p)).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &agent(), "doc.open", json!({"path": p}));
    let r = ok(&mut d, &agent(), "doc.close", json!({"discard": true}));
    assert_eq!(r["warnings"][0]["code"], "journal_discarded", "{r}");
    assert!(!journal::path_for(&p).exists());
    let kept = asides(dir.path(), "discarded");
    assert_eq!(kept.len(), 1);
    assert_eq!(fs::read(&kept[0]).unwrap(), before);
}

#[test]
fn f4_a_writers_discard_keeps_a_copy_of_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &Author::Writer);
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &Author::Writer, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &Author::Writer,
        "block.insert",
        json!({"type": "action", "text": "And this session's line."}),
    );
    let r = ok(
        &mut d,
        &Author::Writer,
        "doc.close",
        json!({"discard": true}),
    );
    assert_eq!(r["warnings"][0]["code"], "journal_discarded", "{r}");
    assert!(!journal::path_for(&p).exists());
    // The copy holds both the recovered line and this session's.
    let kept = asides(dir.path(), "discarded");
    assert_eq!(kept.len(), 1);
    let copy = fs::read_to_string(&kept[0]).unwrap();
    assert!(copy.contains(UNSAVED) && copy.contains("And this session's line."));

    // A clean close keeps nothing: there is nothing unsaved to keep.
    ok(&mut d, &Author::Writer, "doc.open", json!({"path": p}));
    let r = ok(
        &mut d,
        &Author::Writer,
        "doc.close",
        json!({"discard": true}),
    );
    assert_eq!(r["warnings"], json!([]), "{r}");
    assert_eq!(asides(dir.path(), "discarded").len(), 1);
}

#[test]
fn f4_dispatcher_discard_keeps_a_copy_too() {
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &Author::Writer);
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &Author::Writer, "doc.open", json!({"path": p}));
    let kept = d.discard();
    assert!(kept.is_some());
    assert_eq!(kept, asides(dir.path(), "discarded").pop());
    assert!(!journal::path_for(&p).exists());
}
