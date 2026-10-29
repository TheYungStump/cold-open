//! The diff budget (docs/specs/diff.md §1, decided 2026-10-22): two
//! 120-page scripts already in memory compared in under 100 ms.
//!
//! The timing is always printed. It is asserted only in an optimized build
//! with `COLDOPEN_PERF=1`, because shared CI runners are noisy; CI sets it
//! on one Linux job, as for the layout budget.
#![allow(clippy::disallowed_methods)]

use std::time::{Duration, Instant};

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

/// One unit of `n` action lines, each different, and the same with the
/// first and last line changed (Claude's CL7 shape).
fn long_unit(n: usize) -> (Script, Script) {
    let line =
        |k: usize| format!("Maya walks to the door and looks at the old clock number {k} again");
    let mut a = String::from("INT. HOUSE - DAY\n\n");
    let mut b = a.clone();
    for k in 0..n {
        a.push_str(&line(k));
        a.push_str("\n\n");
        let mut l = line(k);
        if k == 0 || k == n - 1 {
            l.push_str(" twice");
        }
        b.push_str(&l);
        b.push_str("\n\n");
    }
    (
        coldopen_fountain::parse(&a).script,
        coldopen_fountain::parse(&b).script,
    )
}

/// Two scripts compared, as the diff of two Fountain drafts.
fn diff_of(a: &Script, b: &Script) -> coldopen_diff::Diff {
    let ids =
        |s: &Script| -> Vec<String> { (1..=s.elements.len()).map(|i| format!("b_{i}")).collect() };
    let (ia, ib) = (ids(a), ids(b));
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
}

/// GR11 = CL7 (the reviews of phases 4–5b, 2026-10-28): past the
/// 1,000,000-pair bound, a unit's blocks are anchored on the ones that
/// occur once in each draft, so a long unit is compared in about linear
/// time and still finds its two changed lines; and a gap with no anchors
/// is bounded too. Before, 4,800 blocks took 14 s (release) and 740 MB.
#[test]
fn a_long_unit_is_compared_in_bounded_time() {
    let time = |n: usize| {
        let (a, b) = long_unit(n);
        let start = Instant::now();
        let d = diff_of(&a, &b);
        let t = start.elapsed();
        assert_eq!(d.summary.blocks.changed, 2, "{n}");
        assert_eq!(d.summary.blocks.added + d.summary.blocks.deleted, 0, "{n}");
        t
    };
    let small = (0..3).map(|_| time(1200)).min().unwrap();
    let large = (0..3).map(|_| time(4800)).min().unwrap();
    // A quadratic step shows as about 16 times for 4 times the blocks.
    assert!(
        large < small * 9 + Duration::from_millis(50),
        "1,200 blocks: {small:?}; 4,800 blocks: {large:?}"
    );

    // Grok's GR11 shape: the second draft rotated, so half the lines align
    // and the rest is one gap with no anchor. It finishes, bounded.
    let n = 1600;
    let mut a = String::new();
    let mut b = String::new();
    for k in 0..n {
        a.push_str(&format!("Line {k}.\n\n"));
        b.push_str(&format!("Line {}.\n\n", (k + n / 2) % n));
    }
    let (a, b) = (
        coldopen_fountain::parse(&a).script,
        coldopen_fountain::parse(&b).script,
    );
    let start = Instant::now();
    let _ = diff_of(&a, &b);
    assert!(
        start.elapsed() < Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
}
