//! Script diff on our own fixtures (docs/specs/diff.md §7), with the JSON
//! and the summary checked in, and the properties every diff must have.
//!
//! `COLDOPEN_BLESS=1 cargo test -p coldopen-diff --test fixtures` rewrites
//! the goldens; review the diff before committing them.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_diff::{BlockOpKind, Diff, Draft, SceneOpKind, Title, WordOp, compare, summary_text};
use coldopen_screenplay::{Element, ElementKind, Script};
use proptest::prelude::*;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A Fountain draft in comparison form, with ids `b_1`… by element.
fn draft(text: &str) -> (Script, Vec<String>) {
    let script = coldopen_fountain::normalize(&coldopen_fountain::parse(text).script);
    let ids = (1..=script.elements.len())
        .map(|i| format!("b_{i}"))
        .collect();
    (script, ids)
}

fn diff(a: &(Script, Vec<String>), b: &(Script, Vec<String>)) -> Diff {
    compare(
        &Draft {
            elements: &a.0.elements,
            ids: &a.1,
            title: Title::Keys(&a.0.title_page),
            caps: &[],
        },
        &Draft {
            elements: &b.0.elements,
            ids: &b.1,
            title: Title::Keys(&b.0.title_page),
            caps: &[],
        },
        true,
    )
}

fn golden(name: &str, got: &str) {
    let path = fixtures().join(name);
    if std::env::var("COLDOPEN_BLESS").is_ok_and(|v| v == "1") {
        fs::write(&path, got).unwrap();
    }
    let want = fs::read_to_string(&path)
        .unwrap_or_else(|_| {
            panic!(
                "{} is missing; run with COLDOPEN_BLESS=1 and review it",
                path.display()
            )
        })
        .replace("\r\n", "\n");
    assert!(want == got, "{} differs:\n{got}", path.display());
}

#[test]
fn the_fixtures_find_moves_edits_and_the_title_page() {
    let a = draft(&fs::read_to_string(fixtures().join("a.fountain")).unwrap());
    let b = draft(&fs::read_to_string(fixtures().join("b.fountain")).unwrap());
    let d = diff(&a, &b);
    golden(
        "a-b.json",
        &(serde_json::to_string_pretty(&d).unwrap() + "\n"),
    );
    golden("a-b.txt", &summary_text(&d, "a.fountain", "b.fountain"));

    let ops: Vec<(SceneOpKind, String)> = d
        .scenes
        .iter()
        .map(|s| (s.op, s.b.as_ref().or(s.a.as_ref()).unwrap().heading.clone()))
        .collect();
    assert_eq!(
        ops,
        [
            (SceneOpKind::Changed, "INT. LAB - NIGHT".to_string()),
            (SceneOpKind::Changed, "INT. KITCHEN - MORNING".to_string()),
            (SceneOpKind::Deleted, "EXT. STREET - DUSK".to_string()),
            (SceneOpKind::Moved, "EXT. ROOF - NIGHT".to_string()),
            (SceneOpKind::Added, "INT. CAR - NIGHT".to_string()),
        ]
    );
    let lab: Vec<BlockOpKind> = d.scenes[0].blocks.iter().map(|b| b.op).collect();
    assert_eq!(lab, [BlockOpKind::Restyled, BlockOpKind::Retyped]);
    let words = &d.scenes[1].blocks[0].words;
    assert_eq!(
        words
            .iter()
            .map(|w| (w.op, w.text.as_str()))
            .collect::<Vec<_>>(),
        [
            (WordOp::Same, "I told you "),
            (WordOp::Del, "twice"),
            (WordOp::Ins, "already"),
            (WordOp::Same, "."),
        ]
    );
    let roof = &d.scenes[3];
    assert_eq!(roof.after, Some(Some(3)), "after the kitchen");
    assert_eq!(roof.blocks[0].op, BlockOpKind::Changed);
    assert_eq!(d.summary.scenes.same, 1, "the hall");
    assert_eq!(d.title_page.len(), 1);
    assert_eq!(d.title_page[0].key.as_deref(), Some("Draft date"));
}

/// The counts match the lists, whatever the drafts.
fn counts_match(d: &Diff) {
    let s = &d.summary.scenes;
    let by = |op: SceneOpKind| d.scenes.iter().filter(|x| x.op == op).count();
    assert_eq!(s.changed, by(SceneOpKind::Changed));
    assert_eq!(s.moved, by(SceneOpKind::Moved));
    assert_eq!(s.added, by(SceneOpKind::Added));
    assert_eq!(s.deleted, by(SceneOpKind::Deleted));
    let blocks = |op: BlockOpKind| {
        d.scenes
            .iter()
            .flat_map(|x| &x.blocks)
            .filter(|b| b.op == op)
            .count()
    };
    assert_eq!(d.summary.blocks.added, blocks(BlockOpKind::Added));
    assert_eq!(d.summary.blocks.deleted, blocks(BlockOpKind::Deleted));
    assert_eq!(d.summary.blocks.changed, blocks(BlockOpKind::Changed));
    assert_eq!(d.summary.title_page, d.title_page.len());
}

const HEADINGS: [&str; 6] = [
    "INT. LAB - NIGHT",
    "EXT. ROOF - NIGHT",
    "INT. HALL - DAY",
    "INT. KITCHEN - MORNING",
    "EXT. STREET - DUSK",
    "INT. CAR - NIGHT",
];
const WORDS: [&str; 12] = [
    "rain", "the", "pumps", "stopped", "Maya", "listens", "wind", "rail", "kettle", "clicks",
    "off", "twice",
];

/// A small script: scenes of a heading, then action and speeches.
fn script() -> impl Strategy<Value = Vec<Element>> {
    let block = (0..3u8, prop::collection::vec(0..WORDS.len(), 1..8));
    let scene = (0..HEADINGS.len(), prop::collection::vec(block, 0..4));
    prop::collection::vec(scene, 0..7).prop_map(|scenes| {
        let mut out = Vec::new();
        for (h, blocks) in scenes {
            out.push(Element::new(ElementKind::SceneHeading, HEADINGS[h]));
            for (kind, w) in blocks {
                let text = w.iter().map(|&i| WORDS[i]).collect::<Vec<_>>().join(" ") + ".";
                match kind {
                    0 => out.push(Element::new(ElementKind::Action, text)),
                    1 => {
                        out.push(Element::new(ElementKind::Character, "MAYA"));
                        out.push(Element::new(ElementKind::Dialogue, text));
                    }
                    _ => out.push(Element::new(ElementKind::Transition, "CUT TO:")),
                }
            }
        }
        out
    })
}

fn as_draft(elements: Vec<Element>) -> (Script, Vec<String>) {
    let ids = (1..=elements.len()).map(|i| format!("b_{i}")).collect();
    (
        Script {
            title_page: Default::default(),
            elements,
        },
        ids,
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// A script diffed with itself is empty.
    #[test]
    fn a_script_against_itself_is_empty(s in script()) {
        let a = as_draft(s);
        let d = diff(&a, &a);
        prop_assert!(d.scenes.is_empty(), "{:?}", d.scenes);
        prop_assert!(d.title_page.is_empty());
    }

    /// A against B and B against A swap added and deleted, find the same
    /// number of changed and moved scenes, and the counts match the lists.
    #[test]
    fn swapping_the_drafts_swaps_added_and_deleted(x in script(), y in script()) {
        let (a, b) = (as_draft(x), as_draft(y));
        let ab = diff(&a, &b);
        let ba = diff(&b, &a);
        counts_match(&ab);
        counts_match(&ba);
        prop_assert_eq!(ab.summary.scenes.added, ba.summary.scenes.deleted);
        prop_assert_eq!(ab.summary.scenes.deleted, ba.summary.scenes.added);
        prop_assert_eq!(ab.summary.scenes.same + ab.summary.scenes.changed + ab.summary.scenes.moved,
            ba.summary.scenes.same + ba.summary.scenes.changed + ba.summary.scenes.moved);
    }
}
