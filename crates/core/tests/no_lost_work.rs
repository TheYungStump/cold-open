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

/// Review findings F5 and C3: a document made with `doc.new` and never
/// saved exists only in its journal, and a second `doc.new` at the same
/// path truncated that journal.
#[test]
fn f5_doc_new_never_truncates_an_existing_journal() {
    const NEVER_SAVED: &str = "A WHOLE NEW SCENE NEVER SAVED";
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("Untitled-1.fountain");
    let w = Author::Writer;
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.new", json!({"path": p, "start": "action"}));
    ok(
        &mut d,
        &w,
        "text.insert",
        json!({"id": "b_1", "at": 0, "text": NEVER_SAVED}),
    );
    drop(d); // a crash
    let j = journal::path_for(&p);
    let before = fs::read(&j).unwrap();

    let mut d = Dispatcher::new(Options::default());
    let e = err(&mut d, &w, "doc.new", json!({"path": p}));
    assert_eq!(e.code, "already_exists", "{e}");
    assert!(d.session().is_none());
    assert_eq!(fs::read(&j).unwrap(), before);
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert!(text(&mut d).contains(NEVER_SAVED));
}

/// `doc.new` checks everything before it closes the open document, so a
/// refused `doc.new` leaves that document open.
#[test]
fn f5_a_refused_doc_new_leaves_the_open_document_alone() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.fountain");
    fs::write(&a, SCRIPT).unwrap();
    let w = Author::Writer;
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": a}));
    let e = err(&mut d, &w, "doc.new", json!({"path": a}));
    assert_eq!(e.code, "already_exists", "{e}");
    let e = err(
        &mut d,
        &w,
        "doc.new",
        json!({"path": dir.path().join("notes.pdf")}),
    );
    assert_eq!(e.code, "unsupported_format", "{e}");
    assert_eq!(
        ok(&mut d, &w, "doc.get", json!({}))["path"],
        a.display().to_string()
    );
    assert!(journal::path_for(&a).exists());
}

/// Opens `other.fountain` with a line of its own, ready to save as
/// another path.
fn other_document(dir: &Path) -> Dispatcher {
    let q = dir.join("other.fountain");
    fs::write(&q, "EXT. ELSEWHERE - NIGHT\n").unwrap();
    let w = Author::Writer;
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": q}));
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "Agent draft."}),
    );
    d
}

/// Review findings F2 and C2: "save as" onto a script another process has
/// open wrote the file, then failed to lock the journal and reported
/// `doc_locked`, so the caller believed nothing had happened. These saves
/// pass `overwrite`, as the app does after its dialog: the checks hold
/// even then.
#[test]
fn f2_save_as_onto_a_locked_script_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let w = Author::Writer;
    let mut a = Dispatcher::new(Options::default());
    ok(&mut a, &w, "doc.open", json!({"path": p}));

    let mut b = other_document(dir.path());
    let e = err(
        &mut b,
        &w,
        "doc.save",
        json!({"path": p, "overwrite": true}),
    );
    assert_eq!(e.code, "doc_locked", "{e}");
    assert_eq!(fs::read_to_string(&p).unwrap(), SCRIPT);
    // B still has its own document, unsaved; A is untouched.
    let doc = ok(&mut b, &w, "doc.get", json!({}));
    assert!(doc["path"].as_str().unwrap().ends_with("other.fountain"));
    assert_eq!(doc["dirty"], true);
    assert_eq!(ok(&mut a, &w, "doc.get", json!({}))["dirty"], false);
}

/// When nobody holds the target but its last session crashed, "save as"
/// replaced the file and truncated the crashed session's journal.
#[test]
fn f2_save_as_onto_a_script_with_unsaved_work_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &Author::Writer);
    let j = journal::path_for(&p);
    let before = fs::read(&j).unwrap();
    let w = Author::Writer;
    let mut b = other_document(dir.path());
    let e = err(
        &mut b,
        &w,
        "doc.save",
        json!({"path": p, "overwrite": true}),
    );
    assert_eq!(e.code, "unsaved_changes", "{e}");
    assert_eq!(e.details.as_ref().unwrap()["path"], p.display().to_string());
    assert_eq!(fs::read_to_string(&p).unwrap(), SCRIPT);
    assert_eq!(fs::read(&j).unwrap(), before);
    drop(b);
    // The crashed session's work is still there to recover.
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert!(text(&mut d).contains(UNSAVED));
}

/// A journal left by a crash with nothing unsaved holds only what the file
/// already holds, so it does not stand in the way of "save as".
#[test]
fn f2_save_as_onto_a_script_with_nothing_unsaved_goes_ahead() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let w = Author::Writer;
    let mut a = Dispatcher::new(Options::default());
    ok(&mut a, &w, "doc.open", json!({"path": p}));
    drop(a); // a crash with nothing unsaved
    assert!(journal::path_for(&p).exists());

    let mut b = other_document(dir.path());
    ok(
        &mut b,
        &w,
        "doc.save",
        json!({"path": p, "overwrite": true}),
    );
    assert!(fs::read_to_string(&p).unwrap().contains("Agent draft."));
    assert!(asides(dir.path(), "corrupt").is_empty());

    // An unreadable journal is copied aside, as opening would.
    let r = dir.path().join("r.fountain");
    fs::write(&r, SCRIPT).unwrap();
    fs::write(journal::path_for(&r), "not a journal\n").unwrap();
    ok(
        &mut b,
        &w,
        "doc.save",
        json!({"path": r, "overwrite": true}),
    );
    assert_eq!(asides(dir.path(), "corrupt").len(), 1);
}

/// Review finding F14: after a save, the journal starts again with the
/// saved document in its header. If the file then is not that document
/// (a rename that never reached the disk before a power cut), the header
/// is the only copy of the save, and opening threw it away silently.
#[test]
fn f14_a_header_that_disagrees_with_the_file_is_recovered() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let w = Author::Writer;
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "SAVED WORK."}),
    );
    ok(&mut d, &w, "doc.save", json!({}));
    drop(d); // a crash with nothing unsaved
    fs::write(&p, SCRIPT).unwrap(); // the save never reached the disk

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    let mut rec = r["recovered"].clone();
    rec.as_object_mut().map(|o| o.remove("last"));
    assert_eq!(rec, json!({"commands": 0, "external_change": true}), "{r}");
    assert!(text(&mut d).contains("SAVED WORK."));
    assert_eq!(ok(&mut d, &w, "doc.get", json!({}))["dirty"], true);
    // Nothing was lost on the way: the file is still the old one, and the
    // journal still holds the save.
    assert_eq!(fs::read_to_string(&p).unwrap(), SCRIPT);
    d.shutdown();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 0, "{r}");
    assert!(text(&mut d).contains("SAVED WORK."));
}

/// A header that the file does match is nothing to recover, even when the
/// file is not in Cold Open's normal form.
#[test]
fn f14_a_header_that_matches_the_file_is_not_recovered() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    // Not the normal form: trailing spaces and extra blank lines.
    fs::write(&p, "INT. HOUSE - DAY   \n\n\n\nRain.\n\n").unwrap();
    let w = Author::Writer;
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"], Value::Null, "{r}");
    // Nor is a header whose document the file holds in the normal form,
    // whatever its fingerprint.
    ok(&mut d, &w, "doc.close", json!({}));
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    drop(d);
    let normal = coldopen_fountain::write(
        &coldopen_fountain::parse("INT. HOUSE - DAY   \n\n\n\nRain.\n\n").script,
    )
    .text;
    fs::write(&p, normal).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"], Value::Null, "{r}");
}

/// Found by the Antigravity review (its finding 2): once only the header
/// is recovered (F14), the journal has no entries, and `doc.revert`, which
/// the app's Discard sends, copied the journal aside only when it had
/// entries. It overwrote the only copy of the save.
#[test]
fn f14_discarding_a_recovered_header_keeps_a_copy() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let w = Author::Writer;
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "SAVED WORK."}),
    );
    ok(&mut d, &w, "doc.save", json!({}));
    drop(d);
    fs::write(&p, SCRIPT).unwrap(); // the save never reached the disk

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 0, "{r}");
    let r = ok(&mut d, &w, "doc.revert", json!({}));
    assert_eq!(r["warnings"][0]["code"], "journal_discarded", "{r}");
    let kept = asides(dir.path(), "discarded");
    assert_eq!(kept.len(), 1);
    assert!(
        fs::read_to_string(&kept[0])
            .unwrap()
            .contains("SAVED WORK.")
    );
    assert!(!text(&mut d).contains("SAVED WORK."));
}

/// Review finding F13 (and Antigravity's 8): "save as" and PDF export
/// replaced whatever file was at the path, from any client, an agent
/// included. Every `.txt` counts as a script.
#[test]
fn f13_saving_or_exporting_over_another_file_needs_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.txt");
    let contract = dir.path().join("contract.pdf");
    fs::write(&notes, "My notes, not a script.\n").unwrap();
    fs::write(&contract, b"%PDF-1.4 not ours").unwrap();
    let w = Author::Writer;
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.new", json!({}));
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "x"}),
    );

    let e = err(&mut d, &w, "doc.save", json!({"path": notes}));
    assert_eq!(e.code, "already_exists", "{e}");
    let e = err(&mut d, &w, "export.pdf", json!({"path": contract}));
    assert_eq!(e.code, "already_exists", "{e}");
    assert_eq!(
        fs::read_to_string(&notes).unwrap(),
        "My notes, not a script.\n"
    );
    assert_eq!(fs::read(&contract).unwrap(), b"%PDF-1.4 not ours");

    // An agent may not choose to replace them.
    let a = agent();
    for (command, path) in [("doc.save", &notes), ("export.pdf", &contract)] {
        let e = err(
            &mut d,
            &a,
            command,
            json!({"path": path, "overwrite": true}),
        );
        assert_eq!(e.code, "writer_only", "{e}");
    }
    assert_eq!(fs::read(&contract).unwrap(), b"%PDF-1.4 not ours");

    // The writer may, and a new path needs nothing.
    ok(
        &mut d,
        &w,
        "export.pdf",
        json!({"path": contract, "overwrite": true}),
    );
    assert!(fs::read(&contract).unwrap().len() > 100);
    ok(
        &mut d,
        &w,
        "export.pdf",
        json!({"path": dir.path().join("new.pdf")}),
    );
    ok(
        &mut d,
        &w,
        "doc.save",
        json!({"path": notes, "overwrite": true}),
    );
    assert_eq!(fs::read_to_string(&notes).unwrap(), "x\n");
    d.shutdown();
}

/// Review finding F9: replay applied an entry before comparing its result.
/// When the result differed, the entry's change stayed in the recovered
/// document, but the journal was cut before it, so the writer kept working
/// on a document the journal could not rebuild, and lost it at the next
/// crash.
#[test]
fn f9_a_diverged_entry_leaves_no_trace_in_the_recovered_document() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let w = Author::Writer;
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "Line A."}),
    );
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "Line B."}),
    );
    drop(d);
    // Entry 2 recorded an id replay will not give: a diverged entry.
    let j = journal::path_for(&p);
    let text = fs::read_to_string(&j).unwrap();
    assert_eq!(text.matches(r#""result":{"id":"b_4"}"#).count(), 1);
    fs::write(
        &j,
        text.replace(r#""result":{"id":"b_4"}"#, r#""result":{"id":"b_99"}"#),
    )
    .unwrap();

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert_eq!(r["warnings"][0]["code"], "journal_partial", "{r}");
    let t = text_of(&mut d);
    assert!(t.contains("Line A.") && !t.contains("Line B."), "{t}");
    // What the writer sees is what the journal holds: an edit now, a
    // crash, and the same document comes back.
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "Line C."}),
    );
    let before = text_of(&mut d);
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"], json!([]), "{r}");
    assert_eq!(text_of(&mut d), before);
}

fn text_of(d: &mut Dispatcher) -> String {
    text(d)
}
