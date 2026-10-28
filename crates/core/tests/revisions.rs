//! Production revision sets (docs/specs/revisions.md): the commands, what
//! each edit marks, undo, settling tracked changes, crashes, and a property
//! test over random edits.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Author, Command, CommandError, Dispatcher, Options};
use proptest::prelude::*;
use serde_json::{Value, json};

const SCRIPT: &str = "\
INT. KITCHEN - DAY

Maya makes toast.

MAYA
Breakfast is ready.

EXT. GARDEN - DAY

Theo waters plants.

INT. SHED - NIGHT

A lamp flickers.
";

fn writer() -> Author {
    Author::named_writer("Maya")
}

fn agent() -> Author {
    Author::agent("claude-code", Some("block_insert".into()))
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
    run(d, &writer(), command, params).unwrap_or_else(|e| panic!("{command} failed: {e}"))
}

fn open(dir: &Path) -> (Dispatcher, PathBuf) {
    let p = dir.join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    (d, p)
}

fn blocks(d: &mut Dispatcher) -> Vec<Value> {
    ok(d, "doc.get", json!({}))["blocks"]
        .as_array()
        .unwrap()
        .clone()
}

fn block(d: &mut Dispatcher, id: &str) -> Value {
    blocks(d)
        .into_iter()
        .find(|b| b["id"] == id)
        .unwrap_or_else(|| panic!("no block {id}"))
}

/// A block's text with the characters set `rev` marks in capitals, and the
/// block's own mark: "maya MAKES toast." for a mark on "makes".
fn shown(d: &mut Dispatcher, id: &str, rev: &str) -> (String, Option<String>) {
    let b = block(d, id);
    let runs = match b["runs"].as_array() {
        Some(r) => r.clone(),
        None => vec![json!({"text": b["text"]})],
    };
    let mut s = String::new();
    for r in runs {
        let Some(t) = r["text"].as_str() else {
            continue;
        };
        if r["rev"] == rev {
            s.push_str(&t.to_uppercase());
        } else {
            s.push_str(&t.to_lowercase());
        }
    }
    (s, b["rev"].as_str().map(str::to_string))
}

fn any_marks(d: &mut Dispatcher) -> bool {
    blocks(d).iter().any(|b| {
        b.get("rev").is_some()
            || b["runs"]
                .as_array()
                .is_some_and(|r| r.iter().any(|r| r.get("rev").is_some()))
    })
}

#[test]
fn start_takes_the_next_color_and_today() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    let r = ok(&mut d, "revision.start", json!({}));
    assert_eq!(r["set"]["id"], "r_1");
    assert_eq!(r["set"]["color"], "Blue");
    assert_eq!(r["set"]["label"], "Blue Rev.");
    assert_eq!(r["set"]["date"].as_str().unwrap().len(), 10);
    let e = run(&mut d, &writer(), "revision.start", json!({})).unwrap_err();
    assert_eq!(e.code, "revision_active");
    ok(&mut d, "revision.end", json!({}));
    let e = run(&mut d, &writer(), "revision.end", json!({})).unwrap_err();
    assert_eq!(e.code, "revision_not_active");
    let r = ok(
        &mut d,
        "revision.start",
        json!({"label": "Pink Draft", "date": "2026-11-02"}),
    );
    assert_eq!(r["set"]["color"], "Pink");
    assert_eq!(r["set"]["label"], "Pink Draft");
    let list = ok(&mut d, "revision.list", json!({}));
    assert_eq!(list["active"], "r_2");
    assert_eq!(list["sets"].as_array().unwrap().len(), 2);
    assert!(list["sets"][0]["ended"].is_u64());
    let e = run(
        &mut d,
        &writer(),
        "revision.start",
        json!({"date": "2026-02-30"}),
    )
    .unwrap_err();
    assert_eq!(e.code, "revision_active");
}

#[test]
fn only_the_writer_runs_the_revision_commands() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    for (c, p) in [
        ("revision.start", json!({})),
        ("revision.end", json!({})),
        ("revision.colors", json!({"colors": ["White", "Blue"]})),
        ("revision.clear", json!({"all": true})),
    ] {
        let e = run(&mut d, &agent(), c, p).unwrap_err();
        assert_eq!(e.code, "writer_only", "{c}");
    }
    assert!(run(&mut d, &agent(), "revision.list", json!({})).is_ok());
}

#[test]
fn bad_dates_colors_and_labels_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    for p in [
        json!({"date": "2026-02-30"}),
        json!({"date": "26-10-22"}),
        json!({"color": ""}),
        json!({"label": "two\nlines"}),
    ] {
        let e = run(&mut d, &writer(), "revision.start", p.clone()).unwrap_err();
        assert_eq!(e.code, "invalid_params", "{p}");
    }
    let e = run(
        &mut d,
        &writer(),
        "revision.colors",
        json!({"colors": ["Blue", "BLUE"]}),
    )
    .unwrap_err();
    assert_eq!(e.code, "invalid_params");
}

#[test]
fn edits_mark_what_they_change() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    // Typing marks the typed word.
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 5, "text": "slowly "}),
    );
    assert_eq!(shown(&mut d, "b_2", "r_1").0, "maya SLOWLY makes toast.");
    // A deletion marks the character before the gap.
    ok(
        &mut d,
        "text.delete",
        json!({"id": "b_6", "from": 4, "to": 11}),
    );
    // Only the space before the gap is marked, and a space shows no case.
    assert_eq!(shown(&mut d, "b_6", "r_1").0, "theo plants.");
    let b6 = block(&mut d, "b_6");
    assert_eq!(b6["runs"][0]["text"], "Theo");
    assert_eq!(b6["runs"][1]["text"], " ");
    assert_eq!(b6["runs"][1]["rev"], "r_1");
    // A style marks the restyled text.
    ok(
        &mut d,
        "text.style",
        json!({"id": "b_8", "from": 2, "to": 6, "bold": true}),
    );
    assert_eq!(shown(&mut d, "b_8", "r_1").0, "a LAMP flickers.");
    // A retype marks the block.
    ok(
        &mut d,
        "block.set_type",
        json!({"id": "b_4", "type": "action"}),
    );
    assert_eq!(shown(&mut d, "b_4", "r_1").1.as_deref(), Some("r_1"));
    // A deleted block marks the end of the block before it.
    ok(&mut d, "block.delete", json!({"id": "b_6"}));
    let (b5, _) = shown(&mut d, "b_5", "r_1");
    assert!(b5.ends_with('Y'), "{b5}");
    // Notes never mark.
    let before = blocks(&mut d);
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_8", "runs": [{"text": "A "}, {"text": "lamp", "bold": true}, {"text": " flickers."}, {"note": "check"}]}),
    );
    assert_eq!(shown(&mut d, "b_8", "r_1").0, "a LAMP flickers.");
    assert_ne!(blocks(&mut d), before);
}

#[test]
fn a_moved_scene_is_marked_whole() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    ok(&mut d, "scene.move", json!({"id": "b_7", "before": "b_1"}));
    assert_eq!(shown(&mut d, "b_7", "r_1").1.as_deref(), Some("r_1"));
    assert_eq!(shown(&mut d, "b_8", "r_1").1.as_deref(), Some("r_1"));
    assert_eq!(shown(&mut d, "b_1", "r_1").1, None);
}

#[test]
fn marks_stay_after_the_set_ends_and_nothing_new_is_marked() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    );
    ok(&mut d, "revision.end", json!({}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_8", "at": 0, "text": "Then "}),
    );
    assert_eq!(shown(&mut d, "b_2", "r_1").0, "SLEEPY maya makes toast.");
    assert_eq!(shown(&mut d, "b_8", "r_1").0, "then a lamp flickers.");
    // An edit that rebuilds the text keeps the marks of what it did not
    // change.
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_2", "text": "Sleepy Maya makes eggs."}),
    );
    assert_eq!(shown(&mut d, "b_2", "r_1").0, "SLEEPY maya makes eggs.");
}

#[test]
fn a_later_set_replaces_the_mark_and_keeps_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_2", "text": "Maya burns toast."}),
    );
    ok(&mut d, "revision.end", json!({}));
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_2", "text": "Maya burns bread."}),
    );
    assert_eq!(shown(&mut d, "b_2", "r_1").0, "maya BURNS bread.");
    assert_eq!(shown(&mut d, "b_2", "r_2").0, "maya burns BREAD.");
}

#[test]
fn undo_and_redo_take_the_marks_with_the_edit() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    let plain = blocks(&mut d);
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    );
    let marked = blocks(&mut d);
    ok(&mut d, "undo", json!({}));
    assert_eq!(blocks(&mut d), plain);
    ok(&mut d, "redo", json!({}));
    assert_eq!(blocks(&mut d), marked);
    ok(&mut d, "undo", json!({}));
    ok(&mut d, "undo", json!({}));
    let list = ok(&mut d, "revision.list", json!({}));
    assert_eq!(list["sets"].as_array().unwrap().len(), 0);
}

#[test]
fn tracked_edits_are_marked_when_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    // An agent's edit is tracked; pending, it marks nothing.
    run(
        &mut d,
        &agent(),
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    )
    .unwrap();
    assert_eq!(shown(&mut d, "b_2", "r_1").0, "sleepy maya makes toast.");
    let change = ok(&mut d, "changes.list", json!({}))["changes"][0]["id"].clone();
    // Accepted after the set ended: marked with the set it was made under.
    ok(&mut d, "revision.end", json!({}));
    ok(&mut d, "changes.accept", json!({"id": change}));
    assert_eq!(shown(&mut d, "b_2", "r_1").0, "SLEEPY maya makes toast.");
    // Rejecting marks nothing.
    ok(&mut d, "revision.start", json!({}));
    run(&mut d, &agent(), "block.delete", json!({"id": "b_8"})).unwrap();
    let change = ok(&mut d, "changes.list", json!({}))["changes"][0]["id"].clone();
    ok(&mut d, "changes.reject", json!({"id": change}));
    assert_eq!(shown(&mut d, "b_8", "r_2").0, "a lamp flickers.");
    // Accepting a deletion marks its gap with the active set.
    run(&mut d, &agent(), "block.delete", json!({"id": "b_8"})).unwrap();
    ok(&mut d, "changes.accept", json!({"all": true}));
    let (heading, _) = shown(&mut d, "b_7", "r_2");
    assert!(heading.ends_with('T'), "{heading}");
}

#[test]
fn clearing_takes_the_marks_and_keeps_the_sets() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    );
    ok(
        &mut d,
        "block.set_type",
        json!({"id": "b_4", "type": "action"}),
    );
    ok(&mut d, "revision.end", json!({}));
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_8", "at": 0, "text": "Then "}),
    );
    let r = ok(&mut d, "revision.clear", json!({"set": "r_1"}));
    assert_eq!(r["count"], 2);
    assert_eq!(shown(&mut d, "b_2", "r_1").0, "sleepy maya makes toast.");
    assert_eq!(shown(&mut d, "b_8", "r_2").0, "THEN a lamp flickers.");
    ok(&mut d, "revision.clear", json!({"all": true}));
    assert!(!any_marks(&mut d));
    assert_eq!(
        ok(&mut d, "revision.list", json!({}))["sets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let e = run(&mut d, &writer(), "revision.clear", json!({"set": "r_9"})).unwrap_err();
    assert_eq!(e.code, "revision_not_found");
}

#[test]
fn sets_and_marks_survive_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, p) = open(dir.path());
    ok(&mut d, "revision.start", json!({"date": "2026-10-22"}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    );
    ok(&mut d, "block.delete", json!({"id": "b_6"}));
    let want = blocks(&mut d);
    let list = ok(&mut d, "revision.list", json!({}));
    drop(d); // a crash
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 3, "{r}");
    assert_eq!(blocks(&mut d), want);
    assert_eq!(ok(&mut d, "revision.list", json!({})), list);
    // Marking goes on after recovery, with the set still active.
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_8", "at": 0, "text": "Then "}),
    );
    assert_eq!(shown(&mut d, "b_8", "r_1").0, "THEN a lamp flickers.");
}

#[test]
fn a_batch_marks_each_edit() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "batch",
        json!({"commands": [
            {"command": "text.insert", "params": {"id": "b_2", "at": 0, "text": "Sleepy "}},
            {"command": "text.insert", "params": {"id": "b_8", "at": 0, "text": "Then "}},
        ]}),
    );
    assert_eq!(shown(&mut d, "b_2", "r_1").0, "SLEEPY maya makes toast.");
    assert_eq!(shown(&mut d, "b_8", "r_1").0, "THEN a lamp flickers.");
    ok(&mut d, "undo", json!({}));
    assert!(!any_marks(&mut d));
}

// ---- Property test ----

#[derive(Clone, Debug)]
enum Step {
    Start,
    End,
    Insert(usize, usize, &'static str),
    Delete(usize, usize, usize),
    SetType(usize),
    Remove(usize),
    Tracked(usize, &'static str),
    AcceptAll,
    Undo,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        1 => Just(Step::Start),
        1 => Just(Step::End),
        4 => (0..10usize, 0..30usize, prop::sample::select(&["x", "new words ", " "][..]))
            .prop_map(|(b, at, t)| Step::Insert(b, at, t)),
        3 => (0..10usize, 0..30usize, 1..8usize).prop_map(|(b, a, n)| Step::Delete(b, a, n)),
        1 => (0..10usize).prop_map(Step::SetType),
        1 => (0..10usize).prop_map(Step::Remove),
        2 => (0..10usize, prop::sample::select(&["agent ", "!"][..]))
            .prop_map(|(b, t)| Step::Tracked(b, t)),
        1 => Just(Step::AcceptAll),
        2 => Just(Step::Undo),
    ]
}

fn ids(d: &mut Dispatcher) -> Vec<(String, usize)> {
    blocks(d)
        .iter()
        .map(|b| {
            (
                b["id"].as_str().unwrap().to_string(),
                b["text"].as_str().map_or(0, |t| t.chars().count()),
            )
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// Every mark names a set that exists; nothing is marked while no set
    /// is active unless an accept marked it with a set it recorded; and
    /// undoing every step leaves the document with no marks.
    #[test]
    fn marks_follow_the_rules(steps in prop::collection::vec(step(), 1..30)) {
        let dir = tempfile::tempdir().unwrap();
        let (mut d, _) = open(dir.path());
        let mut applied = 0;
        for s in steps {
            let present = ids(&mut d);
            let pick = |i: usize| present.get(i % present.len().max(1)).cloned();
            let r = match s {
                Step::Start => run(&mut d, &writer(), "revision.start", json!({})),
                Step::End => run(&mut d, &writer(), "revision.end", json!({})),
                Step::Insert(b, at, t) => match pick(b) {
                    Some((id, n)) => run(&mut d, &writer(), "text.insert",
                        json!({"id": id, "at": at.min(n), "text": t})),
                    None => continue,
                },
                Step::Delete(b, a, k) => match pick(b) {
                    Some((id, n)) => {
                        let from = a.min(n);
                        run(&mut d, &writer(), "text.delete",
                            json!({"id": id, "from": from, "to": (from + k).min(n)}))
                    }
                    None => continue,
                },
                Step::SetType(b) => match pick(b) {
                    Some((id, _)) => run(&mut d, &writer(), "block.set_type",
                        json!({"id": id, "type": "action"})),
                    None => continue,
                },
                Step::Remove(b) => match pick(b) {
                    Some((id, _)) => run(&mut d, &writer(), "block.delete", json!({"id": id})),
                    None => continue,
                },
                Step::Tracked(b, t) => match pick(b) {
                    Some((id, _)) => run(&mut d, &agent(), "text.insert",
                        json!({"id": id, "at": 0, "text": t})),
                    None => continue,
                },
                Step::AcceptAll => run(&mut d, &writer(), "changes.accept", json!({"all": true})),
                Step::Undo => run(&mut d, &writer(), "undo", json!({})),
            };
            if r.is_ok() {
                applied += 1;
            }
            let sets = ok(&mut d, "revision.list", json!({}))["sets"].as_array().unwrap().len();
            for b in blocks(&mut d) {
                let mut named: Vec<u64> = Vec::new();
                if let Some(r) = b["rev"].as_str() {
                    named.push(r[2..].parse().unwrap());
                }
                for run in b["runs"].as_array().into_iter().flatten() {
                    if let Some(r) = run["rev"].as_str() {
                        named.push(r[2..].parse().unwrap());
                    }
                }
                for n in named {
                    prop_assert!(n >= 1 && n as usize <= sets, "{b} names r_{n} of {sets}");
                }
            }
        }
        for _ in 0..applied + 5 {
            if run(&mut d, &writer(), "undo", json!({})).is_err() {
                break;
            }
        }
        prop_assert!(!any_marks(&mut d));
    }
}

// ---- Files ----

#[test]
fn a_project_keeps_sets_and_marks_across_a_reopen_and_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({"date": "2026-10-22"}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    );
    ok(
        &mut d,
        "block.set_type",
        json!({"id": "b_4", "type": "action"}),
    );
    let b = dir.path().join("s.coldopen");
    let r = ok(&mut d, "doc.save", json!({"path": b}));
    assert_eq!(r["losses"], json!([]), "{r}");
    let want = blocks(&mut d);
    let list = ok(&mut d, "revision.list", json!({}));
    ok(&mut d, "doc.close", json!({}));
    ok(&mut d, "doc.open", json!({"path": b}));
    assert_eq!(blocks(&mut d), want);
    assert_eq!(ok(&mut d, "revision.list", json!({})), list);
    // Marking goes on, and a crash after a save recovers it all.
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_8", "at": 0, "text": "Then "}),
    );
    let want = blocks(&mut d);
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": b}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert_eq!(blocks(&mut d), want);
}

#[test]
fn fountain_and_fdx_report_the_revision_marks() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    );
    let e = run(&mut d, &writer(), "doc.save", json!({})).unwrap_err();
    assert_eq!(e.code, "lossy_format");
    let losses = e.details.as_ref().unwrap()["losses"].to_string();
    assert!(losses.contains("revision_marks"), "{losses}");
    let fdx = dir.path().join("s.fdx");
    let e = run(&mut d, &writer(), "export.fdx", json!({"path": fdx})).unwrap_err();
    assert!(
        e.details.as_ref().unwrap()["losses"]
            .to_string()
            .contains("revision_marks")
    );
    // An export leaves the script's marks as they are.
    ok(
        &mut d,
        "export.fdx",
        json!({"path": fdx, "allow_lossy": true}),
    );
    assert!(any_marks(&mut d));
    let text = fs::read_to_string(&fdx).unwrap();
    assert!(text.contains("Sleepy Maya makes toast."));
}

#[test]
fn save_anyway_takes_the_marks_out_and_keeps_the_marked_version() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, p) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    );
    let marked = blocks(&mut d);
    let r = ok(&mut d, "doc.save", json!({"allow_lossy": true}));
    let codes: Vec<&str> = r["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w["code"].as_str())
        .collect();
    assert_eq!(codes, ["revisions_cleared"], "{r}");
    assert!(
        r["losses"].to_string().find("revision_marks").is_none(),
        "{r}"
    );
    assert!(!any_marks(&mut d));
    assert_eq!(ok(&mut d, "revision.list", json!({}))["sets"], json!([]));
    assert!(
        fs::read_to_string(&p)
            .unwrap()
            .contains("Sleepy Maya makes toast.")
    );
    // The next save asks nothing, and undo brings the marks back.
    ok(&mut d, "doc.save", json!({}));
    ok(&mut d, "undo", json!({}));
    assert_eq!(blocks(&mut d), marked);
    let copies = ok(&mut d, "recovered.list", json!({}))["copies"].clone();
    assert!(
        copies
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["kind"] == "marked"),
        "{copies}"
    );
}

#[test]
fn save_anyway_with_tracked_changes_and_revisions_does_both() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    run(
        &mut d,
        &agent(),
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Sleepy "}),
    )
    .unwrap();
    let r = ok(&mut d, "doc.save", json!({"allow_lossy": true}));
    let codes: Vec<&str> = r["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|w| w["code"].as_str())
        .collect();
    assert_eq!(codes, ["changes_accepted", "revisions_cleared"], "{r}");
    assert!(!any_marks(&mut d));
    assert_eq!(ok(&mut d, "changes.list", json!({}))["changes"], json!([]));
    assert_eq!(block(&mut d, "b_2")["text"], "Sleepy Maya makes toast.");
}

// ---- Printing ----

fn pages(d: &mut Dispatcher) -> String {
    ok(d, "doc.export", json!({"format": "pages"}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn marked_lines_print_an_asterisk_and_the_page_its_header() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    let plain = pages(&mut d);
    assert!(!plain.contains('*'));
    ok(&mut d, "revision.start", json!({"date": "2026-10-22"}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_8", "at": 0, "text": "Then "}),
    );
    let marked = pages(&mut d);
    let rows: Vec<&str> = marked.lines().collect();
    let line = rows
        .iter()
        .find(|r| r.contains("Then A lamp flickers."))
        .unwrap_or_else(|| panic!("{marked}"));
    // The asterisk sits 0.4 in from the right edge: cell 81 on Letter.
    assert_eq!(line.chars().count(), 82, "{line:?}");
    assert!(line.ends_with('*'), "{line:?}");
    assert_eq!(rows.iter().filter(|r| r.ends_with('*')).count(), 1);
    assert!(marked.contains("Blue Rev. 10/22/26"), "{marked}");
    // Nothing else moved.
    let unmarked: Vec<String> = marked
        .lines()
        .map(|r| {
            r.trim_end_matches('*')
                .trim_end()
                .replace("Blue Rev. 10/22/26", "")
                .trim_end()
                .to_string()
        })
        .collect();
    let want: Vec<String> = plain
        .lines()
        .map(|r| {
            r.replace("A lamp flickers.", "Then A lamp flickers.")
                .trim_end()
                .to_string()
        })
        .collect();
    assert_eq!(unmarked, want);
    let list = ok(&mut d, "revision.list", json!({}));
    assert_eq!(list["sets"][0]["pages"], 1, "{list}");
    assert_eq!(list["sets"][0]["blocks"], 1, "{list}");
}

#[test]
fn colored_pages_are_an_option() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({"date": "2026-10-22"}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_8", "at": 0, "text": "Then "}),
    );
    let white = dir.path().join("white.pdf");
    let colored = dir.path().join("colored.pdf");
    let again = dir.path().join("again.pdf");
    ok(&mut d, "export.pdf", json!({"path": white}));
    ok(
        &mut d,
        "export.pdf",
        json!({"path": colored, "revision_pages": "colored"}),
    );
    ok(
        &mut d,
        "export.pdf",
        json!({"path": again, "revision_pages": "colored"}),
    );
    let (w, c) = (fs::read(&white).unwrap(), fs::read(&colored).unwrap());
    assert_ne!(w, c);
    assert_eq!(
        c,
        fs::read(&again).unwrap(),
        "the same options, the same bytes"
    );
}

/// Text put back as the set found it loses the set's mark (revisions.md
/// §6.5, the 2026-10-26 manual test): a space deleted and typed again, a
/// word changed and changed back, a block retyped and retyped back.
#[test]
fn text_put_back_as_the_set_found_it_is_not_marked() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    // "Maya makes toast.": the space after "Maya" goes, and comes back.
    ok(
        &mut d,
        "text.delete",
        json!({"id": "b_2", "from": 4, "to": 5}),
    );
    assert_eq!(
        shown(&mut d, "b_2", "r_1").0,
        "MAYAMAKES toast.",
        "the joined word is marked"
    );
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 4, "text": " "}),
    );
    assert!(!any_marks(&mut d), "{:?}", blocks(&mut d));
    // Two words changed, one changed back: only the other stays marked.
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_6", "text": "Ana waters roses."}),
    );
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_6", "text": "Theo waters roses."}),
    );
    assert_eq!(shown(&mut d, "b_6", "r_1").0, "theo waters ROSES.");
    // A retype, and back.
    ok(
        &mut d,
        "block.set_type",
        json!({"id": "b_4", "type": "action"}),
    );
    ok(
        &mut d,
        "block.set_type",
        json!({"id": "b_4", "type": "dialogue"}),
    );
    assert_eq!(shown(&mut d, "b_4", "r_1").1, None);
    // Undo takes the starting points back with the edits.
    for _ in 0..6 {
        ok(&mut d, "undo", json!({}));
    }
    assert!(!any_marks(&mut d));
    let base = ok(&mut d, "doc.export", json!({"format": "json"}));
    assert!(!base.to_string().contains("\"base\""), "{base}");
}

/// Under a later set, text put back to how that set found it keeps the
/// earlier set's marks; ending a set clears its starting points.
#[test]
fn a_later_set_puts_back_the_earlier_sets_marks() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_2", "text": "Maya burns toast."}),
    );
    ok(&mut d, "revision.end", json!({}));
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_2", "text": "Maya burns bread."}),
    );
    assert_eq!(shown(&mut d, "b_2", "r_2").0, "maya burns BREAD.");
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_2", "text": "Maya burns toast."}),
    );
    assert_eq!(
        shown(&mut d, "b_2", "r_1").0,
        "maya BURNS toast.",
        "r_1's mark is back"
    );
    assert_eq!(shown(&mut d, "b_2", "r_2").0, "maya burns toast.");
    ok(&mut d, "revision.end", json!({}));
    let table = ok(&mut d, "doc.export", json!({"format": "json"}));
    assert!(!table.to_string().contains("\"base\""), "ended: {table}");
}
