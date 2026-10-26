//! The diff budget (docs/specs/diff.md §1, decided 2026-10-22): two
//! 120-page scripts already in memory compared in under 100 ms.
//!
//! The timing is always printed. It is asserted only in an optimized build
//! with `COLDOPEN_PERF=1`, because shared CI runners are noisy; CI sets it
//! on one Linux job, as for the layout budget.
#![allow(clippy::disallowed_methods)]

use std::time::Instant;

use coldopen_diff::{Draft, Title, compare};
use coldopen_screenplay::{Element, ElementKind, RichText, Script};

fn assert_budget() -> bool {
    !cfg!(debug_assertions) && std::env::var("COLDOPEN_PERF").as_deref() == Ok("1")
}

/// The synthetic script with a draft's worth of edits: scene 10 moved after
/// scene 50, scene 30 cut, a new scene before scene 80, and a word added to
/// every 25th speech.
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
fn two_feature_scripts_compare_within_budget() {
    let a = coldopen_layout::synthetic::feature_script(1);
    let b = edited(&a);
    let ids =
        |s: &Script| -> Vec<String> { (1..=s.elements.len()).map(|i| format!("b_{i}")).collect() };
    let (ia, ib) = (ids(&a), ids(&b));
    let run = || {
        compare(
            &Draft {
                elements: &a.elements,
                ids: &ia,
                title: Title::Keys(&a.title_page),
                caps: &[],
            },
            &Draft {
                elements: &b.elements,
                ids: &ib,
                title: Title::Keys(&b.title_page),
                caps: &[],
            },
            true,
        )
    };
    let d = run();
    assert_eq!(d.summary.scenes.moved, 1, "{:?}", d.summary);
    assert_eq!(d.summary.scenes.deleted, 1);
    assert_eq!(d.summary.scenes.added, 1);
    assert!(d.summary.blocks.changed >= 10, "{:?}", d.summary.blocks);
    let mut best = f64::MAX;
    for _ in 0..5 {
        let t = Instant::now();
        let again = run();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(again, d, "the same answer every time");
    }
    eprintln!(
        "diff of two {}-element scripts: {best:.2} ms (best of 5)",
        a.elements.len()
    );
    if assert_budget() {
        assert!(
            best < 100.0,
            "the diff took {best:.2} ms, over the 100 ms budget"
        );
    }
}
