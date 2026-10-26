//! `coldopen diff` end to end in under 500 ms (docs/specs/diff.md §1,
//! decided 2026-10-22): the program started, a Fountain draft and an .fdx
//! draft of 120 pages read, compared, and the summary printed.
//!
//! The timing is always printed. It is asserted only in an optimized build
//! with `COLDOPEN_PERF=1`, as the other budgets are.
#![allow(clippy::disallowed_methods)]

use std::fs;
use std::process::Command;
use std::time::Instant;

use coldopen_core::{Dispatcher, Options};
use coldopen_screenplay::{Element, ElementKind, RichText, Script};
use serde_json::json;

fn assert_budget() -> bool {
    !cfg!(debug_assertions) && std::env::var("COLDOPEN_PERF").as_deref() == Ok("1")
}

/// The synthetic script with a draft's worth of edits, as the diff crate's
/// budget test makes it.
fn edited(s: &Script) -> Script {
    let mut scenes: Vec<Vec<Element>> = Vec::new();
    for e in &s.elements {
        if e.kind == ElementKind::SceneHeading || scenes.is_empty() {
            scenes.push(Vec::new());
        }
        scenes.last_mut().unwrap().push(e.clone());
    }
    let moved = scenes.remove(10);
    scenes.insert(50, moved);
    scenes.remove(30);
    scenes.insert(
        80,
        vec![
            Element::new(ElementKind::SceneHeading, "INT. NEW ROOM - NIGHT"),
            Element::new(ElementKind::Action, "Nobody has been here before."),
        ],
    );
    let mut speeches = 0;
    let mut elements: Vec<Element> = scenes.into_iter().flatten().collect();
    for e in &mut elements {
        if e.kind == ElementKind::Dialogue {
            speeches += 1;
            if speeches % 25 == 0 {
                e.text = RichText::plain(format!("{} Again.", e.text.plain_text()));
            }
        }
    }
    Script {
        title_page: s.title_page.clone(),
        elements,
    }
}

#[test]
fn coldopen_diff_runs_within_budget() {
    let dir = tempfile::tempdir().unwrap();
    let a = coldopen_layout::synthetic::feature_script(1);
    let (pa, pb) = (dir.path().join("a.fountain"), dir.path().join("b.fountain"));
    fs::write(&pa, coldopen_fountain::write(&a).text).unwrap();
    fs::write(&pb, coldopen_fountain::write(&edited(&a)).text).unwrap();
    // The second draft as .fdx, so both readers are timed.
    let fdx = dir.path().join("b.fdx");
    let mut d = Dispatcher::new(Options::default());
    let open = coldopen_core::Command::from_parts("doc.open", json!({"path": pb})).unwrap();
    d.execute(open).unwrap();
    let save =
        coldopen_core::Command::from_parts("doc.save", json!({"path": fdx, "allow_lossy": true}))
            .unwrap();
    d.execute(save).unwrap();
    drop(d);
    let mut best = f64::MAX;
    let mut text = String::new();
    for _ in 0..3 {
        let t = Instant::now();
        let out = Command::new(env!("CARGO_BIN_EXE_coldopen"))
            .args(["diff"])
            .arg(&pa)
            .arg(&fdx)
            .output()
            .unwrap();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        text = String::from_utf8(out.stdout).unwrap();
    }
    assert!(text.contains("1 moved, 1 added, 1 deleted"), "{text}");
    eprintln!("coldopen diff of two 120-page drafts: {best:.0} ms (best of 3)");
    if assert_budget() {
        assert!(
            best < 500.0,
            "coldopen diff took {best:.0} ms, over the 500 ms budget"
        );
    }
}
