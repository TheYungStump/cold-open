//! Nothing that holds a writer's unsaved work is deleted, truncated or
//! overwritten unless it is copied aside first, and nothing is written
//! before the checks that could refuse it (docs/specs/journal.md). Each
//! test reproduces a finding from the review of phases 1–3, named in its
//! title (docs/progress/review-fixes.md), or (2026-10-28, the section at
//! the end) from the reviews of phases 4–5b
//! (docs/progress/review-fixes-4-5b.md).

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
    ok(d, &Author::writer(), "doc.export", json!({}))["text"]
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
    let p = crashed_with_work(dir.path(), &Author::writer());
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
    let p = crashed_with_work(dir.path(), &Author::writer());
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
    let p = crashed_with_work(dir.path(), &Author::writer());
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &Author::writer(), "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &Author::writer(),
        "block.insert",
        json!({"type": "action", "text": "And this session's line."}),
    );
    let r = ok(
        &mut d,
        &Author::writer(),
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
    ok(&mut d, &Author::writer(), "doc.open", json!({"path": p}));
    let r = ok(
        &mut d,
        &Author::writer(),
        "doc.close",
        json!({"discard": true}),
    );
    assert_eq!(r["warnings"], json!([]), "{r}");
    assert_eq!(asides(dir.path(), "discarded").len(), 1);
}

#[test]
fn f4_dispatcher_discard_keeps_a_copy_too() {
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &Author::writer());
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &Author::writer(), "doc.open", json!({"path": p}));
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
    let w = Author::writer();
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
    let w = Author::writer();
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
    let w = Author::writer();
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
    let w = Author::writer();
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
    let p = crashed_with_work(dir.path(), &Author::writer());
    let j = journal::path_for(&p);
    let before = fs::read(&j).unwrap();
    let w = Author::writer();
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
    let w = Author::writer();
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
    let w = Author::writer();
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
    let w = Author::writer();
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
    let w = Author::writer();
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
    let w = Author::writer();
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
    let w = Author::writer();
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

/// Review finding C1 (and Antigravity's 1): a save restarts the journal,
/// but the undo history keeps the steps from before it. Undoing one of
/// them after the save was journaled as a bare `undo`, which replay could
/// not repeat (`nothing_to_undo`), so everything after it was lost.
#[test]
fn c1_undo_and_redo_across_a_save_survive_a_crash() {
    let w = Author::writer();
    // Codex's steps: set text, save, undo, type, crash.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("o.fountain");
    fs::write(&p, "Original.\n").unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &w,
        "block.set_text",
        json!({"id": "b_1", "text": "Saved."}),
    );
    ok(&mut d, &w, "doc.save", json!({}));
    ok(&mut d, &w, "undo", json!({}));
    ok(
        &mut d,
        &w,
        "text.insert",
        json!({"id": "b_1", "at": 9, "text": " New work."}),
    );
    let live = text(&mut d);
    assert_eq!(live, "Original. New work.\n");
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"], json!([]), "{r}");
    assert_eq!(r["recovered"]["commands"], 2, "{r}");
    assert_eq!(text(&mut d), live);
    // The step came back into the history with it: redo works.
    ok(&mut d, &w, "undo", json!({}));
    ok(&mut d, &w, "redo", json!({}));
    assert_eq!(text(&mut d), live);
    d.discard();

    // Redo across a save, and typing on both sides of it.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("r.fountain");
    fs::write(&p, "One.\n").unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "Two."}),
    );
    ok(&mut d, &w, "undo", json!({}));
    ok(
        &mut d,
        &w,
        "text.insert",
        json!({"id": "b_1", "at": 4, "text": " a"}),
    );
    ok(
        &mut d,
        &w,
        "text.insert",
        json!({"id": "b_1", "at": 6, "text": "b"}),
    );
    ok(&mut d, &w, "doc.save", json!({}));
    ok(
        &mut d,
        &w,
        "text.insert",
        json!({"id": "b_1", "at": 7, "text": "c"}),
    );
    ok(&mut d, &w, "undo", json!({})); // the "c" typed after the save
    ok(&mut d, &w, "undo", json!({})); // " ab", from before it
    ok(&mut d, &w, "redo", json!({}));
    // Undone again, so the document differs from the saved file and there
    // is something to recover.
    ok(&mut d, &w, "undo", json!({}));
    let live = text(&mut d);
    assert_eq!(live, "One.\n");
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"], json!([]), "{r}");
    assert_eq!(text(&mut d), live);
    // Both steps can be redone: the one from before the save, then the
    // one typed after it.
    ok(&mut d, &w, "redo", json!({}));
    assert_eq!(text(&mut d), "One. ab\n");
    ok(&mut d, &w, "redo", json!({}));
    assert_eq!(text(&mut d), "One. abc\n");
    d.discard();
}

/// Review finding F16, first part: an undo entry used to say only "undo",
/// so if the rebuilt history ever differed (a change in how steps group),
/// replay would silently reverse some other step. Each entry now names
/// the step, and a mismatch stops replay instead.
#[test]
fn f16_an_undo_of_a_different_step_is_caught() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let w = Author::writer();
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
    ok(&mut d, &w, "undo", json!({}));
    drop(d);
    let j = journal::path_for(&p);
    let text_j = fs::read_to_string(&j).unwrap();
    assert_eq!(text_j.matches(r#","step":2}"#).count(), 1, "{text_j}");
    fs::write(&j, text_j.replace(r#","step":2}"#, r#","step":1}"#)).unwrap();

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 2, "{r}");
    let warning = r["warnings"][0]["message"].as_str().unwrap();
    assert!(
        warning.contains("different step") && warning.contains(".diverged-"),
        "{warning}"
    );
    let t = text(&mut d);
    assert!(t.contains("Line A.") && t.contains("Line B."), "{t}");
    d.discard();
}

/// Review finding F16, second part: params were journaled as sent, with
/// defaults left out. `text.insert` without `style` takes the style around
/// it, and `block.split` without `type` keeps the block's type; if either
/// default ever changed, old entries would replay differently with the
/// same `{}` result, and nothing would notice. Entries now carry the
/// values that were used.
#[test]
fn f16_journaled_params_carry_their_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let w = Author::writer();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &w,
        "text.style",
        json!({"id": "b_2", "from": 0, "to": 5, "italic": true}),
    );
    ok(
        &mut d,
        &w,
        "text.insert",
        json!({"id": "b_2", "at": 2, "text": "ai"}),
    );
    ok(&mut d, &w, "block.split", json!({"id": "b_2", "at": 3}));
    ok(
        &mut d,
        &w,
        "batch",
        json!({"commands": [{"command": "text.insert", "params": {"id": "b_1", "at": 0, "text": "X"}}]}),
    );
    let live = text(&mut d);
    drop(d);
    let journal_text = fs::read_to_string(journal::path_for(&p)).unwrap();
    let entries: Vec<Value> = journal_text
        .lines()
        .skip(1)
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        entries[1]["params"]["style"],
        json!({"italic": true}),
        "{}",
        entries[1]
    );
    assert_eq!(entries[2]["params"]["type"], "action", "{}", entries[2]);
    assert_eq!(
        entries[3]["params"]["commands"][0]["params"]["style"],
        json!({}),
        "{}",
        entries[3]
    );
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"], json!([]), "{r}");
    assert_eq!(text(&mut d), live);
    d.discard();
}

/// Rewrites the header of `p`'s journal with `f`.
fn edit_header(p: &Path, f: impl FnOnce(&mut Value)) {
    let j = journal::path_for(p);
    let text_j = fs::read_to_string(&j).unwrap();
    let (head, rest) = text_j.split_once('\n').unwrap();
    let mut header: Value = serde_json::from_str(head).unwrap();
    f(&mut header);
    fs::write(&j, format!("{header}\n{rest}")).unwrap();
}

/// Review finding F16, third part: a journal says nothing about where it
/// was written. Scripts kept in git or a synced folder take their journal
/// with them, and a co-writer who opened the script on another computer
/// "recovered" someone else's unsaved edits without being asked. Such a
/// journal is now offered, never applied on its own.
#[test]
fn f16_a_journal_from_another_computer_or_folder_is_offered_not_applied() {
    let w = Author::writer();
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &w);
    let header = fs::read_to_string(journal::path_for(&p)).unwrap();
    let header: Value = serde_json::from_str(header.lines().next().unwrap()).unwrap();
    assert!(
        header["host"].is_string() && header["path"].is_string(),
        "{header}"
    );
    assert_eq!(header["semantics"], journal::SEMANTICS, "{header}");

    // Synced from another computer.
    edit_header(&p, |h| h["host"] = json!("someone-elses-laptop"));
    let before = fs::read(journal::path_for(&p)).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let e = err(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(e.code, "journal_foreign", "{e}");
    assert_eq!(e.details.as_ref().unwrap()["host"], "someone-elses-laptop");
    assert!(d.session().is_none());
    assert_eq!(fs::read(journal::path_for(&p)).unwrap(), before);
    // The writer chose to apply it.
    let r = ok(
        &mut d,
        &w,
        "doc.open",
        json!({"path": p, "recover": "apply"}),
    );
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert!(text(&mut d).contains(UNSAVED));
    d.shutdown();

    // Committed with the script, and checked out in another folder.
    let other = tempfile::tempdir().unwrap();
    let q = other.path().join("s.fountain");
    fs::copy(&p, &q).unwrap();
    fs::copy(journal::path_for(&p), journal::path_for(&q)).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let e = err(&mut d, &w, "doc.open", json!({"path": q}));
    assert_eq!(e.code, "journal_foreign", "{e}");
    let r = ok(
        &mut d,
        &w,
        "doc.open",
        json!({"path": q, "recover": "discard"}),
    );
    assert_eq!(r["warnings"][0]["code"], "journal_discarded", "{r}");
    d.discard();
}

/// Phase 5a decision 43: after the writer recovered a journal from
/// another computer, the journal went on under the other computer's
/// header, so a crash here asked the same question again. Recovering it
/// now appends a marker that makes it this computer's: a crash after that
/// is recovered here without the question, and the journal still counts
/// as someone else's in any other folder (journal.md, 2026-10-21).
#[test]
fn a_recovered_foreign_journal_is_not_offered_again_here() {
    let w = Author::writer();
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &w);
    edit_header(&p, |h| h["host"] = json!("someone-elses-laptop"));
    let mut d = Dispatcher::new(Options::default());
    let e = err(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(e.code, "journal_foreign", "{e}");
    ok(
        &mut d,
        &w,
        "doc.open",
        json!({"path": p, "recover": "apply"}),
    );
    // More work, then a crash.
    ok(
        &mut d,
        &w,
        "block.insert",
        json!({"type": "action", "text": "After the recovery."}),
    );
    let want = text(&mut d);
    assert!(want.contains(UNSAVED));
    drop(d);
    // The header is as it was; one marker was appended.
    let journal = fs::read_to_string(journal::path_for(&p)).unwrap();
    assert!(
        journal
            .lines()
            .next()
            .unwrap()
            .contains("someone-elses-laptop")
    );
    assert_eq!(journal.matches(journal::ADOPTED).count(), 1, "{journal}");
    // Reopened here: recovered without the question, the work all there.
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 2, "{r}");
    assert_eq!(text(&mut d), want);
    drop(d); // a crash again

    // Copied to another folder, it is someone else's again.
    let other = tempfile::tempdir().unwrap();
    let q = other.path().join("s.fountain");
    fs::copy(&p, &q).unwrap();
    fs::copy(journal::path_for(&p), journal::path_for(&q)).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let e = err(&mut d, &w, "doc.open", json!({"path": q}));
    assert_eq!(e.code, "journal_foreign", "{e}");
    // Recovered there too, it gains a marker of its own.
    ok(
        &mut d,
        &w,
        "doc.open",
        json!({"path": q, "recover": "apply"}),
    );
    assert_eq!(text(&mut d), want);
    drop(d);
    let journal = fs::read_to_string(journal::path_for(&q)).unwrap();
    assert_eq!(journal.matches(journal::ADOPTED).count(), 2, "{journal}");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &w, "doc.open", json!({"path": q}));
    assert_eq!(text(&mut d), want);
    d.discard();
}

/// A journal written by a newer Cold Open, whose entries may mean things
/// this version does not know, is kept aside rather than replayed.
#[test]
fn f16_a_journal_with_newer_semantics_is_set_aside() {
    let w = Author::writer();
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &w);
    edit_header(&p, |h| h["semantics"] = json!(journal::SEMANTICS + 1));
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &w, "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"][0]["code"], "journal_unreadable", "{r}");
    assert!(
        r["warnings"][0]["message"]
            .as_str()
            .unwrap()
            .contains("newer"),
        "{r}"
    );
    assert_eq!(asides(dir.path(), "corrupt").len(), 1);
    d.discard();
}

// ---- The reviews of phases 4–5b (2026-10-28) ------------------------------

fn untracked_agent() -> Author {
    Author::agent("claude", None)
}

/// CL1, GP6: `coldopen mcp --untracked` records an agent's edits untracked.
/// That switch is the session's, not the script's, so the app that
/// recovers the journal does not have it; recovery takes each entry as it
/// was recorded, and every edit comes back.
#[test]
fn cl1_untracked_agent_edits_recover_without_the_switch() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, "INT. LAB - NIGHT\n\nRain falls.\n").unwrap();
    let a = untracked_agent();
    let mut d = Dispatcher::new(Options::default());
    d.set_agent_tracking(Some(false));
    ok(&mut d, &a, "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &a,
        "block.insert",
        json!({"type": "action", "text": "Thunder."}),
    );
    ok(
        &mut d,
        &a,
        "text.insert",
        json!({"id": "b_2", "at": 11, "text": " Hard."}),
    );
    ok(
        &mut d,
        &a,
        "title.set",
        json!({"key": "Title", "values": ["Storm"]}),
    );
    let before = ok(&mut d, &a, "doc.get", json!({}));
    drop(d); // a crash

    let mut app = Dispatcher::new(Options::default());
    let r = ok(&mut app, &Author::writer(), "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"], json!([]), "{r}");
    assert_eq!(r["recovered"]["commands"], 3, "{r}");
    let after = ok(&mut app, &Author::writer(), "doc.get", json!({}));
    assert_eq!(after["blocks"], before["blocks"]);
    assert_eq!(after["title_page"], before["title_page"]);
    assert!(asides(dir.path(), "corrupt").is_empty());
    app.discard();

    // A live agent still cannot opt out where agents are tracked.
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &a, "doc.open", json!({"path": p}));
    let e = err(
        &mut d,
        &a,
        "block.insert",
        json!({"type": "action", "text": "x", "tracked": false}),
    );
    assert_eq!(e.code, "writer_only");
    d.discard();
}

/// AR12 (part 1): reverting keeps `--untracked`, which belongs to the
/// session, not to the file read again.
#[test]
fn ar12_revert_keeps_the_untracked_switch() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let a = untracked_agent();
    let mut d = Dispatcher::new(Options::default());
    d.set_agent_tracking(Some(false));
    ok(&mut d, &a, "doc.open", json!({"path": p}));
    ok(&mut d, &Author::writer(), "doc.revert", json!({}));
    let r = ok(
        &mut d,
        &a,
        "block.insert",
        json!({"type": "action", "text": "Thunder."}),
    );
    assert!(r.get("change").is_none(), "tracked after a revert: {r}");
    d.discard();
}

/// CL2: a set-aside copy that replays only in part (here because an entry
/// was damaged) restores with less work than it holds. The restore says
/// so, and `recovered.list` shows how much of the copy replays.
#[test]
fn cl2_a_copy_that_replays_in_part_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let p = crashed_with_work(dir.path(), &Author::writer());
    let jpath = journal::path_for(&p);
    let mut lines = fs::read_to_string(&jpath).unwrap();
    lines.push_str("{\"seq\":2,\"t\":0,\"command\":\"block.insert\",\"params\":{\"type\":\"action\",\"text\":\"Lost.\",\"tracked\":false},\"result\":{\"id\":\"b_99\"}}\n");
    fs::write(&jpath, lines).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &Author::writer(), "doc.open", json!({"path": p}));
    let copy = r["warnings"][0]["details"]["copy"].clone();
    assert!(copy.is_string(), "{r}");
    d.discard();

    let list = ok(
        &mut d,
        &Author::writer(),
        "recovered.list",
        json!({"path": p}),
    );
    let row = list["copies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["copy"] == copy)
        .unwrap()
        .clone();
    assert_eq!(
        (row["replayed"].clone(), row["entries"].clone()),
        (json!(1), json!(2)),
        "{row}"
    );
    assert!(row["problem"].is_string(), "{row}");

    let target = dir.path().join("restored.fountain");
    let r = ok(
        &mut d,
        &Author::writer(),
        "recovered.restore",
        json!({"copy": copy, "path": target}),
    );
    assert_eq!(r["warnings"][0]["code"], "journal_partial", "{r}");
    assert_eq!(r["warnings"][0]["details"]["replayed"], 1, "{r}");
    assert!(text(&mut d).contains(UNSAVED));
    d.discard();
}
