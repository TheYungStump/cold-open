//! Files with tens of thousands of distinct names: unknown elements,
//! entities, sections, settings types, header labels and attributes. Each
//! name used to be looked up with a linear scan, so a sub-megabyte file
//! took tens of seconds to read. Every lookup is now a `BTreeMap` or
//! `BTreeSet`, and reading such a file costs about the same as reading one
//! of the same size that repeats a single name.
//!
//! The check compares two timings on the same machine, so it holds on slow
//! runners too: the file of distinct names may take at most a few times as
//! long as the file of repeats, plus a fixed allowance.
#![allow(clippy::disallowed_methods)]

use std::time::{Duration, Instant};

const N: usize = 40_000;

fn best_time(bytes: &[u8]) -> Duration {
    let mut best = Duration::MAX;
    for _ in 0..2 {
        let t = Instant::now();
        let doc = coldopen_fdx::read(bytes).expect("an .fdx document");
        let d = t.elapsed();
        drop(doc);
        best = best.min(d);
    }
    best
}

/// Builds the two files from `item`, which gets a 6-digit name: distinct
/// numbers for the hostile file and always 0 for the baseline, so both
/// files have the same length.
fn assert_near_linear(what: &str, head: &str, item: impl Fn(String) -> String, tail: &str) {
    assert_near_linear_doc(what, |name| {
        let mut s = String::from(head);
        for i in 0..N {
            s.push_str(&item(name(i)));
        }
        s.push_str(tail);
        s
    });
}

/// Like [`assert_near_linear`] for a document built in one piece: `doc`
/// gets the name for item `i` (all distinct, or all the same).
fn assert_near_linear_doc(what: &str, doc: impl Fn(&dyn Fn(usize) -> String) -> String) {
    let build =
        |distinct: bool| doc(&|i| format!("{:06}", if distinct { i } else { 0 })).into_bytes();
    let (hostile, same) = (build(true), build(false));
    assert_eq!(hostile.len(), same.len());
    let base = best_time(&same);
    let t = best_time(&hostile);
    eprintln!(
        "{what}: {N} distinct names in {:.1} ms, {N} repeats in {:.1} ms",
        t.as_secs_f64() * 1000.0,
        base.as_secs_f64() * 1000.0
    );
    let limit = base * 8 + Duration::from_secs(1);
    assert!(
        t <= limit,
        "{what}: {N} distinct names took {t:?}, more than {limit:?} (8 times the {base:?} for repeats, plus 1 s)"
    );
}

#[test]
fn unknown_elements_are_counted_in_near_linear_time() {
    assert_near_linear(
        "unknown elements in Content",
        "<FinalDraft><Content>",
        |n| format!("<u{n}/>"),
        "</Content></FinalDraft>",
    );
}

#[test]
fn unknown_entities_are_counted_in_near_linear_time() {
    assert_near_linear(
        "unknown entities",
        "<FinalDraft><Content><Paragraph Type=\"Action\"><Text>",
        |n| format!("&e{n};"),
        "</Text></Paragraph></Content></FinalDraft>",
    );
}

#[test]
fn ignored_sections_are_listed_in_near_linear_time() {
    assert_near_linear(
        "top-level sections",
        "<FinalDraft>",
        |n| format!("<s{n}/>"),
        "</FinalDraft>",
    );
}

#[test]
fn settings_types_are_looked_up_in_near_linear_time() {
    // Every settings type is checked for repeats, and every paragraph looks
    // up its type's settings for the inline baseline.
    // Each paragraph's type differs from its settings type only in case,
    // so every lookup goes through the case-folded index.
    assert_near_linear_doc("settings types and paragraphs", |name| {
        let mut s = String::from("<FinalDraft>");
        for i in 0..N {
            s.push_str(&format!("<ElementSettings Type=\"t{}\"/>", name(i)));
        }
        s.push_str("<Content>");
        for i in 0..N {
            s.push_str(&format!("<Paragraph Type=\"T{}\"/>", name(i)));
        }
        s.push_str("</Content></FinalDraft>");
        s
    });
    assert_near_linear(
        "paragraphs after many settings types",
        "<FinalDraft>",
        |n| format!("<ElementSettings Type=\"t{n}\"/>"),
        &format!(
            "<Content>{}</Content></FinalDraft>",
            "<Paragraph Type=\"Action\"/>".repeat(N)
        ),
    );
}

#[test]
fn header_labels_are_listed_in_near_linear_time() {
    assert_near_linear(
        "header labels",
        "<FinalDraft><HeaderAndFooter><Header><Paragraph>",
        |n| format!("<DynamicLabel Type=\"L{n}\"/>"),
        "</Paragraph></Header></HeaderAndFooter></FinalDraft>",
    );
}

#[test]
fn attributes_are_deduplicated_in_near_linear_time() {
    assert_near_linear(
        "attributes on one element",
        "<FinalDraft><Content",
        |n| format!(" a{n}=\"\""),
        "/></FinalDraft>",
    );
}
