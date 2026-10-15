//! Repagination speed (docs/spec.md: a full repagination of a 120-page
//! script in under 50 ms, and typical edits repaginate incrementally). The
//! timed span is flow + wrap + paginate, from a script and format to pages;
//! parsing and PDF writing are not included.
//!
//! The timing is always printed. It is asserted only in an optimized build
//! with `COLDOPEN_PERF=1`, because shared CI runners are noisy; CI sets it on
//! one Linux job.
#![allow(clippy::disallowed_methods)]

use std::time::Instant;

use coldopen_layout::{Engine, Incremental, Options, ValidFormat};
use coldopen_screenplay::{Element, ElementKind, Script};

fn assert_budget() -> bool {
    !cfg!(debug_assertions) && std::env::var("COLDOPEN_PERF").as_deref() == Ok("1")
}

#[test]
fn full_repagination_is_fast() {
    let script = coldopen_layout::synthetic::feature_script(1);
    let format = ValidFormat::default();
    let options = Options::default();
    let pages = Engine::new(&script, &format, &options).run().pages.len();
    let mut best = f64::MAX;
    for _ in 0..9 {
        let t = Instant::now();
        let layout = Engine::new(&script, &format, &options).run();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(layout.pages.len(), pages);
        best = best.min(ms);
    }
    eprintln!("full repagination: {pages} pages in {best:.2} ms (best of 9)");
    assert!(
        pages >= 118,
        "the synthetic script should be about 120 pages, got {pages}"
    );
    if assert_budget() {
        assert!(
            best < 50.0,
            "{pages} pages took {best:.2} ms, over the 50 ms budget"
        );
    }
}

/// Typing on page 90 of 120 lays out a page or two, not the script
/// (layout.md §15). The timed span is one incremental update: the flow
/// assembled from measured elements, and the pages that can differ.
#[test]
fn an_edit_on_page_90_is_incremental() {
    let mut script = coldopen_layout::synthetic::feature_script(1);
    let format = ValidFormat::default();
    let options = Options::default();
    let mut inc = Incremental::new(&script, &format, &options);
    let total = inc.layout().pages.len();
    // The first dialogue that starts on page 90.
    let at = (0..script.elements.len())
        .find(|&e| {
            script.elements[e].kind == ElementKind::Dialogue
                && inc.layout().first_page(e) == Some(90)
        })
        .expect("a speech on page 90");
    let mut times = Vec::new();
    let mut laid_out = 0;
    for k in 0..40 {
        let t = script.elements[at].text.plain_text();
        let t = if k % 2 == 0 {
            format!("{t} x")
        } else {
            t[..t.len() - 2].to_string()
        };
        script.elements[at].text = t.into();
        let start = Instant::now();
        inc.update(&script);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        laid_out = laid_out.max(inc.stats().laid_out);
    }
    times.sort_by(f64::total_cmp);
    let median = times[times.len() / 2];
    let worst = times[times.len() - 1];
    eprintln!(
        "incremental edit on page 90 of {total}: median {median:.2} ms, worst {worst:.2} ms, at most {laid_out} pages laid out"
    );
    assert!(laid_out <= 3, "{laid_out} pages laid out for one edit");
    assert_eq!(inc.layout(), &Engine::new(&script, &format, &options).run());
    if assert_budget() {
        assert!(median < 2.0, "an incremental edit took {median:.2} ms");
    }
}

/// One element with thousands of sentence starts: each is a split
/// candidate, so pagination must not rewrap the rest of the element for
/// every one of them (that was quadratic).
#[test]
fn long_paragraphs_are_fast() {
    let text = "Go now. ".repeat(20_000);
    let script = Script {
        elements: vec![Element::new(ElementKind::Action, text.trim_end())],
        ..Default::default()
    };
    let format = ValidFormat::default();
    let options = Options::default();
    let mut best = f64::MAX;
    let mut pages = 0;
    for _ in 0..5 {
        let t = Instant::now();
        pages = Engine::new(&script, &format, &options).run().pages.len();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    eprintln!("long paragraph: {pages} pages in {best:.2} ms (best of 5)");
    if assert_budget() {
        assert!(
            best < 50.0,
            "{pages} pages took {best:.2} ms, over the 50 ms budget"
        );
    }
}
