//! Crash journals written by the builds of earlier phases still recover,
//! entry for entry (review finding F17.3). The fixtures and how they were
//! made are in `tests/fixtures/journal/README.md`. Since phase 5a they are
//! journals of semantics 1, replayed with no edit tracked
//! (docs/specs/journal.md), and a recovered one goes on with entries of
//! semantics 2. Since phase 5b a journal of semantics 2, written by the
//! phase 5a build with tracked changes in it, replays with no revision
//! marking and goes on with entries of semantics 3. The phase 5b build's
//! journal (semantics 3) holds every settle the fixes of the reviews of
//! phases 4–5b changed, so it shows those journals replay as they were made.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Command, Dispatcher, Options, journal};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/journal")
        .join(name)
}

fn ok(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    d.execute(Command::from_parts(command, params).unwrap())
        .unwrap_or_else(|e| panic!("{command} failed: {e}"))
}

/// The phases whose builds wrote a fixture journal, each with its script.
const PHASES: [(&str, &str); 7] = [
    ("phase1", "lighthouse.fountain"),
    ("phase2", "lighthouse.fountain"),
    ("phase3", "lighthouse.fountain"),
    ("phase4", "lighthouse.fountain"),
    ("phase4-fdx", "lighthouse.fdx"),
    ("phase5a", "lighthouse.fountain"),
    ("phase5b", "lighthouse.fountain"),
];

fn script_of(phase: &str) -> &'static str {
    PHASES.iter().find(|(p, _)| *p == phase).unwrap().1
}

/// Opens the fixture script with a phase's journal beside it, as after a
/// crash of that phase's build.
fn recover(phase: &str) -> (tempfile::TempDir, Dispatcher, Value) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join(script_of(phase));
    fs::copy(fixture(script_of(phase)), &script).unwrap();
    fs::copy(
        fixture(&format!("{phase}.journal")),
        journal::path_for(&script),
    )
    .unwrap();
    let mut d = Dispatcher::new(Options::default());
    // Since phase 4 a header names the computer and the path it was
    // written for, so on any other computer it is a journal from somewhere
    // else, which recovers when asked to (docs/specs/journal.md).
    let r = ok(
        &mut d,
        "doc.open",
        json!({"path": script, "recover": "apply"}),
    );
    (dir, d, r)
}

fn entries(phase: &str) -> usize {
    let text = fs::read_to_string(fixture(&format!("{phase}.journal"))).unwrap();
    text.lines().count() - 1
}

#[test]
fn journals_from_every_earlier_phase_recover_in_full() {
    for (phase, _) in PHASES {
        let (_dir, mut d, r) = recover(phase);
        assert_eq!(r["warnings"], json!([]), "{phase}: {r}");
        assert_eq!(
            r["recovered"]["commands"].as_u64(),
            Some(entries(phase) as u64),
            "{phase}: {r}"
        );
        let want: Value = serde_json::from_str(
            &fs::read_to_string(fixture(&format!("{phase}.expected-blocks.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(ok(&mut d, "doc.get", json!({}))["blocks"], want, "{phase}");
        // The rebuilt undo history goes on working.
        ok(&mut d, "undo", json!({}));
        d.discard();
    }
}

/// The fixture's text as the old build exported it: today's build gives the
/// same Fountain for the recovered document.
#[test]
fn recovered_journals_export_as_the_old_build_did() {
    for (phase, _) in PHASES {
        let path = fixture(&format!("{phase}.expected.fountain"));
        let Ok(want) = fs::read_to_string(&path) else {
            continue;
        };
        let (_dir, mut d, _) = recover(phase);
        // git may check the file out with CRLF line ends.
        let want = want.replace("\r\n", "\n");
        assert_eq!(ok(&mut d, "doc.export", json!({}))["text"], want, "{phase}");
        d.discard();
    }
}

/// Gives the journal of `script` the header this computer writes for it,
/// as if this computer's old build had written the journal: the upgrade a
/// writer makes. The identity is learned from a real journal, of a
/// throwaway script in the same folder left as a crash leaves it.
fn restamp(script: &Path) {
    let dir = script.parent().unwrap();
    let probe = dir.join("probe.fountain");
    fs::write(&probe, "Probe.\n").unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": probe}));
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "x"}),
    );
    drop(d);
    let probe_journal = journal::path_for(&probe);
    let mine: Value = serde_json::from_str(
        fs::read_to_string(&probe_journal)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    fs::remove_file(&probe_journal).unwrap();
    fs::remove_file(&probe).unwrap();
    let probe_path = mine["path"].as_str().unwrap();
    let path = format!(
        "{}{}",
        &probe_path[..probe_path.len() - "probe.fountain".len()],
        script.file_name().unwrap().to_string_lossy()
    );
    let jpath = journal::path_for(script);
    let text = fs::read_to_string(&jpath).unwrap();
    let (first, rest) = text.split_once('\n').unwrap();
    let mut header: Value = serde_json::from_str(first).unwrap();
    header["host"] = mine["host"].clone();
    header["path"] = json!(path);
    fs::write(&jpath, format!("{header}\n{rest}")).unwrap();
}

/// A journal of semantics 1, written on this computer by the phase 4
/// build, recovers after the upgrade, takes tracked edits of semantics 2 on
/// top, and the mix recovers after a second crash, marks and all.
#[test]
fn an_old_journal_goes_on_with_tracked_edits_and_recovers_again() {
    for phase in ["phase4", "phase4-fdx"] {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join(script_of(phase));
        fs::copy(fixture(script_of(phase)), &script).unwrap();
        fs::copy(
            fixture(&format!("{phase}.journal")),
            journal::path_for(&script),
        )
        .unwrap();
        restamp(&script);
        let mut d = Dispatcher::new(Options::default());
        let r = ok(&mut d, "doc.open", json!({"path": script}));
        assert_eq!(
            r["recovered"]["commands"].as_u64(),
            Some(entries(phase) as u64),
            "{phase}: {r}"
        );
        ok(&mut d, "changes.track", json!({"writer": true}));
        ok(
            &mut d,
            "text.insert",
            json!({"id": "b_2", "at": 0, "text": "Slowly, "}),
        );
        ok(
            &mut d,
            "text.delete",
            json!({"id": "b_7", "from": 0, "to": 6}),
        );
        let before = ok(&mut d, "doc.get", json!({}))["blocks"].clone();
        let pending = ok(&mut d, "changes.list", json!({}))["changes"].clone();
        assert_eq!(pending.as_array().unwrap().len(), 2, "{phase}: {pending}");
        drop(d); // a second crash

        let mut d = Dispatcher::new(Options::default());
        let r = ok(
            &mut d,
            "doc.open",
            json!({"path": dir.path().join(script_of(phase))}),
        );
        assert_eq!(r["warnings"], json!([]), "{phase}: {r}");
        assert_eq!(
            ok(&mut d, "doc.get", json!({}))["blocks"],
            before,
            "{phase}"
        );
        assert_eq!(
            ok(&mut d, "changes.list", json!({}))["changes"],
            pending,
            "{phase}"
        );
        // The oldest entries still undo, under the new ones.
        for _ in 0..4 {
            ok(&mut d, "undo", json!({}));
        }
        d.discard();
    }
}

/// The phase 5a journal, with tracked changes pending in it, recovers on
/// this computer after the upgrade; a revision set then marks new edits
/// (semantics 3) on top, and after a second crash everything comes back:
/// the old pending changes, the new marks and the set.
#[test]
fn a_phase_5a_journal_goes_on_with_revision_marks_and_recovers_again() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("lighthouse.fountain");
    fs::copy(fixture("lighthouse.fountain"), &script).unwrap();
    fs::copy(fixture("phase5a.journal"), journal::path_for(&script)).unwrap();
    restamp(&script);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": script}));
    assert_eq!(
        r["recovered"]["commands"].as_u64(),
        Some(entries("phase5a") as u64),
        "{r}"
    );
    let pending = ok(&mut d, "changes.list", json!({}))["changes"].clone();
    assert_eq!(
        pending.as_array().unwrap().len(),
        2,
        "the 5a build left two pending: {pending}"
    );
    // Nothing the 5a entries did is marked.
    assert_eq!(ok(&mut d, "revision.list", json!({}))["sets"], json!([]));
    ok(&mut d, "revision.start", json!({"date": "2026-10-26"}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Far off, "}),
    );
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_5", "text": "Four ships tonight."}),
    );
    let before = ok(&mut d, "doc.get", json!({}))["blocks"].clone();
    let sets = ok(&mut d, "revision.list", json!({}));
    assert!(
        before.to_string().contains("\"rev\""),
        "the new edits are marked"
    );
    drop(d); // a second crash

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": script}));
    assert_eq!(r["warnings"], json!([]), "{r}");
    assert_eq!(ok(&mut d, "doc.get", json!({}))["blocks"], before);
    assert_eq!(ok(&mut d, "changes.list", json!({}))["changes"], pending);
    assert_eq!(ok(&mut d, "revision.list", json!({})), sets);
    d.discard();
}
