//! `diff` and `changes.import` (docs/specs/diff.md §5, §6): two drafts in
//! any format compared, B printed with the changes marked, and a draft
//! brought in as one writer's tracked edits that accept to B and reject to
//! A.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Author, Command, CommandError, Dispatcher, Options};
use proptest::prelude::*;
use serde_json::{Value, json};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../diff/tests/fixtures")
}

fn run(
    d: &mut Dispatcher,
    by: &Author,
    command: &str,
    params: Value,
) -> Result<Value, CommandError> {
    d.execute_as(Command::from_parts(command, params).unwrap(), by)
}

fn ok(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    run(d, &Author::writer(), command, params).unwrap_or_else(|e| panic!("{command}: {e}"))
}

/// Copies of the fixture drafts in a scratch folder.
fn drafts(dir: &Path) -> (PathBuf, PathBuf) {
    let (a, b) = (dir.join("a.fountain"), dir.join("b.fountain"));
    fs::copy(fixtures().join("a.fountain"), &a).unwrap();
    fs::copy(fixtures().join("b.fountain"), &b).unwrap();
    (a, b)
}

fn open(path: &Path) -> Dispatcher {
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": path}));
    d
}

fn ops(r: &Value) -> Vec<String> {
    r["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["op"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn two_files_compare_with_their_names_and_a_summary() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = drafts(dir.path());
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "diff", json!({"a": a, "b": b}));
    assert_eq!(r["v"], 1);
    assert_eq!(r["a"]["format"], "fountain");
    assert_eq!(r["b"]["path"], json!(b.display().to_string()));
    assert_eq!(ops(&r), ["changed", "changed", "deleted", "moved", "added"]);
    let text = r["text"].as_str().unwrap();
    assert!(
        text.starts_with("a.fountain \u{2192} b.fountain: 2 scenes changed, 1 moved"),
        "{text}"
    );
    // Without `a`, the open document as it stands.
    let mut d = open(&a);
    let r = ok(&mut d, "diff", json!({"b": b}));
    assert_eq!(ops(&r), ["changed", "changed", "deleted", "moved", "added"]);
    // Against itself, nothing.
    let r = ok(&mut d, "diff", json!({"b": a}));
    assert_eq!(r["scenes"], json!([]));
    assert_eq!(r["summary"]["scenes"]["changed"], 0);
}

/// A script against itself written as .fdx by Cold Open has no scene
/// differences: Shots, page format and the store aside, the .fdx holds
/// everything the Fountain draft prints.
#[test]
fn a_script_against_its_own_fdx_has_no_scene_differences() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = drafts(dir.path());
    let fdx = dir.path().join("a.fdx");
    let mut d = open(&a);
    ok(
        &mut d,
        "doc.save",
        json!({"path": fdx, "allow_lossy": true}),
    );
    let r = ok(&mut d, "diff", json!({"a": a, "b": fdx}));
    assert_eq!(r["compared"]["transparent"], false);
    assert_eq!(r["scenes"], json!([]), "{}", r["text"]);
}

#[test]
fn the_pdf_marks_b_and_never_writes_over_a_draft() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = drafts(dir.path());
    let mut d = open(&a);
    let pdf = dir.path().join("changes.pdf");
    let r = ok(&mut d, "diff", json!({"b": b, "pdf": pdf}));
    assert_eq!(r["pdf"], json!(pdf.display().to_string()));
    assert!(fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    // The open document is not changed.
    assert_eq!(ok(&mut d, "changes.list", json!({}))["changes"], json!([]));
    assert_eq!(ok(&mut d, "doc.get", json!({}))["dirty"], false);
    // Only a .pdf path, never over a file without `overwrite`, which an
    // agent may not pass, and never over a draft.
    let wrong = dir.path().join("x.txt");
    let e = run(
        &mut d,
        &Author::writer(),
        "diff",
        json!({"b": b, "pdf": wrong}),
    )
    .unwrap_err();
    assert_eq!(e.code, "unsupported_format");
    let e = run(
        &mut d,
        &Author::writer(),
        "diff",
        json!({"b": b, "pdf": pdf}),
    )
    .unwrap_err();
    assert_eq!(e.code, "already_exists");
    let agent = Author::agent("claude-code", Some("diff".into()));
    let e = run(
        &mut d,
        &agent,
        "diff",
        json!({"b": b, "pdf": pdf, "overwrite": true}),
    )
    .unwrap_err();
    assert_eq!(e.code, "writer_only");
    let e = run(
        &mut d,
        &Author::writer(),
        "diff",
        json!({"a": b, "b": a, "pdf": a.with_extension("PDF"), "overwrite": true}),
    );
    assert!(e.is_ok(), "a.PDF is not a.fountain");
    let open_pdf = a.with_extension("pdf");
    let e = run(
        &mut d,
        &Author::writer(),
        "diff",
        json!({"b": b, "pdf": open_pdf, "overwrite": true}),
    );
    assert!(e.is_ok());
}

#[test]
fn a_draft_comes_in_as_changes_that_accept_to_b_and_reject_to_a() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = drafts(dir.path());
    let original = fs::read_to_string(&a).unwrap();
    let mut d = open(&a);
    let r = ok(&mut d, "changes.import", json!({"path": b, "name": "Sam"}));
    assert!(r["edits"].as_u64().unwrap() > 0, "{r}");
    let skipped: Vec<&Value> = r["skipped"].as_array().unwrap().iter().collect();
    assert_eq!(skipped.len(), 2, "the restyle and the title page: {r}");
    let h = ok(&mut d, "history.list", json!({}));
    assert_eq!(h["undo"][0]["label"], "Changes from Sam");
    let list = ok(&mut d, "changes.list", json!({}));
    assert_eq!(
        list["changes"].as_array().unwrap().len(),
        1,
        "one change: {list}"
    );
    assert_eq!(list["changes"][0]["by"]["name"], "Sam");
    assert_eq!(r["change"], list["changes"][0]["id"]);
    // Accepting everything leaves only what was skipped.
    ok(&mut d, "changes.accept", json!({"all": true}));
    let left = ok(&mut d, "diff", json!({"b": b}));
    let blocks: Vec<&str> = left["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["blocks"].as_array().unwrap())
        .map(|b| b["op"].as_str().unwrap())
        .collect();
    assert_eq!(blocks, ["restyled"], "{}", left["text"]);
    // Undo the accept, reject everything: A as it was.
    ok(&mut d, "undo", json!({}));
    ok(&mut d, "changes.reject", json!({"all": true}));
    assert_eq!(ok(&mut d, "doc.export", json!({}))["text"], json!(original));
}

#[test]
fn import_is_refused_to_agents_and_over_pending_changes() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = drafts(dir.path());
    let mut d = open(&a);
    let agent = Author::agent("claude-code", Some("changes_import".into()));
    let e = run(&mut d, &agent, "changes.import", json!({"path": b})).unwrap_err();
    assert_eq!(e.code, "writer_only");
    ok(&mut d, "changes.import", json!({"path": b}));
    let e = run(
        &mut d,
        &Author::writer(),
        "changes.import",
        json!({"path": b}),
    )
    .unwrap_err();
    assert_eq!(e.code, "change_pending");
    // A batch may not hold one.
    let e = run(
        &mut d,
        &Author::writer(),
        "batch",
        json!({"commands": [{"command": "changes.import", "params": {"path": b}}]}),
    )
    .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}

#[test]
fn an_import_survives_a_crash_without_the_draft() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = drafts(dir.path());
    let mut d = open(&a);
    ok(&mut d, "changes.import", json!({"path": b}));
    let want = ok(&mut d, "doc.export", json!({"format": "pages"}));
    drop(d); // a crash
    fs::remove_file(&b).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": a}));
    assert_eq!(r["recovered"]["commands"], 1, "one batch: {r}");
    assert_eq!(ok(&mut d, "doc.export", json!({"format": "pages"})), want);
    let list = ok(&mut d, "changes.list", json!({}));
    assert_eq!(
        list["changes"][0]["by"]["name"], "b",
        "the draft's file name"
    );
}

const HEADINGS: [&str; 5] = [
    "INT. LAB - NIGHT",
    "EXT. ROOF - NIGHT",
    "INT. HALL - DAY",
    "INT. KITCHEN - MORNING",
    "EXT. STREET - DUSK",
];
const WORDS: [&str; 10] = [
    "rain", "the", "pumps", "stopped", "Maya", "listens", "wind", "rail", "kettle", "twice",
];

/// A Fountain script of scenes with action and speeches.
fn fountain() -> impl Strategy<Value = String> {
    let block = (any::<bool>(), prop::collection::vec(0..WORDS.len(), 1..7));
    let scene = (0..HEADINGS.len(), prop::collection::vec(block, 0..4));
    prop::collection::vec(scene, 1..6).prop_map(|scenes| {
        let mut out = String::new();
        for (h, blocks) in scenes {
            out.push_str(HEADINGS[h]);
            out.push_str("\n\n");
            for (speech, w) in blocks {
                let mut text = w.iter().map(|&i| WORDS[i]).collect::<Vec<_>>().join(" ");
                text.push('.');
                if let Some(c) = text.get_mut(0..1) {
                    c.make_ascii_uppercase();
                }
                if speech {
                    out.push_str("MAYA\n");
                }
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
        out
    })
}

/// What a script says: each printing block's kind and text.
fn said(d: &mut Dispatcher) -> Vec<(String, String)> {
    let blocks = ok(d, "doc.get", json!({}));
    blocks["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["type"].as_str().unwrap().to_string(),
                b["text"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Bringing B into A, then accepting everything, gives B's text;
    /// rejecting everything gives A back (diff.md §7).
    #[test]
    fn importing_accepts_to_b_and_rejects_to_a(x in fountain(), y in fountain()) {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.fountain"), dir.path().join("b.fountain"));
        fs::write(&a, &x).unwrap();
        fs::write(&b, &y).unwrap();
        let mut want_b = open(&b);
        let want_b = said(&mut want_b);
        let mut d = open(&a);
        let want_a = said(&mut d);
        let r = ok(&mut d, "changes.import", json!({"path": b}));
        if r["change"].is_null() {
            // Nothing to bring in: the drafts say the same.
            prop_assert_eq!(&want_a, &want_b);
            return Ok(());
        }
        ok(&mut d, "changes.accept", json!({"all": true}));
        prop_assert_eq!(said(&mut d), want_b);
        ok(&mut d, "undo", json!({}));
        ok(&mut d, "changes.reject", json!({"all": true}));
        prop_assert_eq!(said(&mut d), want_a);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// The `diff` fuzz target's check, on drafts that share most of their
    /// scenes and on arbitrary bytes.
    #[test]
    fn the_fuzz_check_holds_on_drafts(x in fountain(), y in fountain()) {
        let data = [x.as_bytes(), b"\0", y.as_bytes()].concat();
        prop_assert_eq!(coldopen_core::fuzzing::diff(&data), Ok(()));
        let shared = [x.as_bytes(), b"\0", x.as_bytes(), y.as_bytes()].concat();
        prop_assert_eq!(coldopen_core::fuzzing::diff(&shared), Ok(()));
    }

    #[test]
    fn the_fuzz_check_holds_on_bytes(data in prop::collection::vec(any::<u8>(), 0..400)) {
        prop_assert_eq!(coldopen_core::fuzzing::diff(&data), Ok(()));
    }
}

/// The `diff` fuzz target's finding of 2026-10-26 (run 40294830086): a
/// draft of one stray line against a page break.
#[test]
fn fuzz_finding_a_page_break_against_one_line() {
    let data = [0xb1, 0x5b, 0xc2, 0xc2, 0x3d, 0x3d, 0x3d];
    assert_eq!(coldopen_core::fuzzing::diff(&data), Ok(()));
}

/// A tracked retype that would drop text is refused: its change could not
/// give the text back on reject (found with the finding above).
#[test]
fn a_tracked_retype_that_drops_text_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("p.fountain");
    fs::write(
        &p,
        "Hello there.
",
    )
    .unwrap();
    let mut d = open(&p);
    let e = run(
        &mut d,
        &Author::writer(),
        "block.set_type",
        json!({"id": "b_1", "type": "page_break", "tracked": true}),
    )
    .unwrap_err();
    assert_eq!(e.code, "invalid_attr");
    // Untracked, undo gives it back as ever.
    ok(
        &mut d,
        "block.set_type",
        json!({"id": "b_1", "type": "page_break"}),
    );
    ok(&mut d, "undo", json!({}));
    assert_eq!(
        said(&mut d),
        [("action".to_string(), "Hello there.".to_string())]
    );
}
