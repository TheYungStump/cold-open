//! Crash journals written by the builds of earlier phases still recover,
//! entry for entry (review finding F17.3). The fixtures and how they were
//! made are in `tests/fixtures/journal/README.md`.

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

/// Opens the fixture script with a phase's journal beside it, as after a
/// crash of that phase's build.
fn recover(phase: &str) -> (tempfile::TempDir, Dispatcher, Value) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("lighthouse.fountain");
    fs::copy(fixture("lighthouse.fountain"), &script).unwrap();
    fs::copy(
        fixture(&format!("{phase}.journal")),
        journal::path_for(&script),
    )
    .unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": script}));
    (dir, d, r)
}

fn entries(phase: &str) -> usize {
    let text = fs::read_to_string(fixture(&format!("{phase}.journal"))).unwrap();
    text.lines().count() - 1
}

#[test]
fn journals_from_every_earlier_phase_recover_in_full() {
    for phase in ["phase1", "phase2", "phase3"] {
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
