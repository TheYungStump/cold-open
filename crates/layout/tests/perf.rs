//! Repagination speed (docs/spec.md: a full repagination of a 120-page
//! script in under 50 ms). The timed span is flow + wrap + paginate, from a
//! script and format to pages; parsing and PDF writing are not included.
//!
//! The timing is always printed. It is asserted only in an optimized build
//! with `COLDOPEN_PERF=1`, because shared CI runners are noisy; CI sets it on
//! one Linux job.
#![allow(clippy::disallowed_methods)]

use std::time::Instant;

use coldopen_layout::{Engine, Options, ValidFormat};
use coldopen_screenplay::{Element, ElementKind, Script};

/// A deterministic script of our own, about 130 US Letter pages long.
fn long_script() -> Script {
    let mut elements = Vec::new();
    let names = ["MAYA", "THEO", "DR. IVES", "NINA (V.O.)"];
    for scene in 0..200 {
        elements.push(
            Element::new(
                ElementKind::SceneHeading,
                format!("INT. ROOM {scene} - NIGHT"),
            )
            .scene_number(format!("{}", scene + 1)),
        );
        elements.push(Element::new(
            ElementKind::Action,
            "The room is quiet except for the radiator, which ticks and hisses as the heat \
             comes up. Rain runs down the window in long silver lines.",
        ));
        for turn in 0..4 {
            let n = names[(scene + turn) % names.len()];
            elements.push(Element::new(ElementKind::Character, n));
            if turn % 2 == 1 {
                elements.push(Element::new(ElementKind::Parenthetical, "(quietly)"));
            }
            elements.push(Element::new(
                ElementKind::Dialogue,
                "I keep thinking about what you said. Maybe you were right. Maybe we should \
                 have left when we had the chance, before the roads closed.",
            ));
        }
        elements.push(Element::new(
            ElementKind::Action,
            "Nobody moves. Somewhere below, a door slams.",
        ));
        if scene % 5 == 4 {
            elements.push(Element::new(ElementKind::Transition, "CUT TO:"));
        }
    }
    Script {
        elements,
        ..Default::default()
    }
}

#[test]
fn full_repagination_is_fast() {
    let script = long_script();
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
        pages >= 120,
        "the synthetic script should be at least 120 pages, got {pages}"
    );
    if !cfg!(debug_assertions) && std::env::var("COLDOPEN_PERF").as_deref() == Ok("1") {
        assert!(
            best < 50.0,
            "{pages} pages took {best:.2} ms, over the 50 ms budget"
        );
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
    if !cfg!(debug_assertions) && std::env::var("COLDOPEN_PERF").as_deref() == Ok("1") {
        assert!(
            best < 50.0,
            "{pages} pages took {best:.2} ms, over the 50 ms budget"
        );
    }
}
