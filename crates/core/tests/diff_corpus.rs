//! The Big Fish variants in the private corpus (docs/specs/diff.md §7):
//! each pair compared, with the counts pinned, so a change to the diff that
//! makes differences appear or vanish shows here. What each difference is
//! is explained in docs/progress/phase-5b.md.
//!
//! The corpus is never committed (see CLAUDE.md). Without it this test
//! skips with a banner, and `COLDOPEN_REQUIRE_CORPUS=1` makes a skip fail.
//! Nothing printed here holds script text.

use std::path::{Path, PathBuf};

use coldopen_core::{Command, Dispatcher, Options};
use serde_json::{Value, json};

fn corpus_dir() -> PathBuf {
    match std::env::var_os("COLDOPEN_CORPUS") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
    }
}

/// Scenes: same, changed, moved, added, deleted. Blocks: changed, added,
/// deleted, retyped, restyled, attributes. Then title-page differences.
type Counts = ([u64; 5], [u64; 6], u64);

const PAIRS: [(&str, &str, Counts); 6] = [
    (
        "fountain-io/Big-Fish.fountain",
        "fountain-io/Big-Fish.fdx",
        ([0, 193, 0, 0, 0], [5, 4, 2, 12, 0, 193], 3),
    ),
    (
        "fountain-io/Big-Fish.fountain",
        "jumpcut/corpus_public_big-fish_source_source.fdx",
        ([193, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0], 3),
    ),
    (
        "fountain-io/Big-Fish.fountain",
        "screenplain/examples_Big-Fish.fdx",
        ([188, 3, 0, 0, 2], [0, 21, 22, 2, 0, 0], 6),
    ),
    (
        "fountain-io/Big-Fish.fdx",
        "jumpcut/corpus_public_big-fish_source_source.fdx",
        ([0, 193, 0, 0, 0], [5, 2, 4, 14, 0, 192], 49),
    ),
    (
        "fountain-io/Big-Fish.fdx",
        "screenplain/examples_Big-Fish.fdx",
        ([0, 191, 0, 0, 2], [5, 22, 25, 14, 0, 188], 6),
    ),
    (
        "jumpcut/corpus_public_big-fish_source_source.fdx",
        "screenplain/examples_Big-Fish.fdx",
        ([188, 3, 0, 0, 2], [0, 21, 22, 2, 0, 0], 4),
    ),
];

fn counts(r: &Value) -> Counts {
    let s = &r["summary"];
    let n = |v: &Value| v.as_u64().unwrap();
    let sc = &s["scenes"];
    let bl = &s["blocks"];
    (
        [
            n(&sc["same"]),
            n(&sc["changed"]),
            n(&sc["moved"]),
            n(&sc["added"]),
            n(&sc["deleted"]),
        ],
        [
            n(&bl["changed"]),
            n(&bl["added"]),
            n(&bl["deleted"]),
            n(&bl["retyped"]),
            n(&bl["restyled"]),
            n(&bl["attributes"]),
        ],
        n(&s["title_page"]),
    )
}

#[test]
fn the_big_fish_variants_differ_only_as_explained() {
    let dir = corpus_dir();
    if !dir.join("fountain-io/Big-Fish.fountain").exists() {
        if std::env::var("COLDOPEN_REQUIRE_CORPUS").is_ok_and(|v| v == "1") {
            panic!("COLDOPEN_REQUIRE_CORPUS=1 but the Big Fish variants are missing");
        }
        eprintln!("\n*** corpus/ is missing: the Big Fish diff check is skipped ***\n");
        return;
    }
    let mut d = Dispatcher::new(Options::default());
    for (a, b, want) in PAIRS {
        let params = json!({"a": dir.join(a), "b": dir.join(b)});
        let r = d
            .execute(Command::from_parts("diff", params).unwrap())
            .unwrap_or_else(|e| panic!("{a} against {b}: {e}"));
        assert_eq!(counts(&r), want, "{a} against {b}");
        // The other way round: added and deleted swap, the rest holds.
        let params = json!({"a": dir.join(b), "b": dir.join(a)});
        let back = counts(
            &d.execute(Command::from_parts("diff", params).unwrap())
                .unwrap(),
        );
        assert_eq!(back.0[3], want.0[4], "{b} against {a}: added");
        assert_eq!(back.0[4], want.0[3], "{b} against {a}: deleted");
        assert_eq!(back.0[0], want.0[0], "{b} against {a}: same");
    }
}
