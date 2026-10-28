//! The title page commands (docs/specs/commands.md, "The title page
//! commands"): keys and lines, their forms, undo, the agent rule, and that
//! they are never tracked or revision-marked.

use std::fs;
use std::path::Path;

use coldopen_core::{Author, Command, CommandError, Dispatcher, Options};
use proptest::prelude::*;
use serde_json::{Value, json};

fn writer() -> Author {
    Author::writer()
}

fn agent() -> Author {
    Author::agent("claude-code", Some("title_set".into()))
}

fn run(
    d: &mut Dispatcher,
    by: &Author,
    command: &str,
    params: Value,
) -> Result<Value, CommandError> {
    d.execute_as(Command::from_parts(command, params).unwrap(), by)
}

fn ok(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    run(d, &writer(), command, params).unwrap_or_else(|e| panic!("{command}: {e}"))
}

fn open(dir: &Path, name: &str, text: &str) -> Dispatcher {
    let p = dir.join(name);
    fs::write(&p, text).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    d
}

const KEYS: &str = "Title: Low Tide\nAuthor: Dana Reyes\n\nINT. LAB - NIGHT\n\nRain.\n";

#[test]
fn keys_are_set_added_removed_and_undone() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = open(dir.path(), "s.fountain", KEYS);
    let t = ok(&mut d, "title.get", json!({}));
    assert_eq!(t["form"], "keys");
    assert_eq!(t["keys"][0]["values"][0]["text"], "Low Tide");
    ok(
        &mut d,
        "title.set",
        json!({"key": "title", "values": ["High Tide"]}),
    );
    ok(
        &mut d,
        "title.set",
        json!({"key": "Draft date", "values": ["October 22, 2026"], "at": 1}),
    );
    ok(&mut d, "title.set", json!({"key": "Author", "values": []}));
    let t = ok(&mut d, "title.get", json!({}));
    let keys: Vec<&str> = t["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["Title", "Draft date"]);
    assert_eq!(t["keys"][0]["values"][0]["text"], "High Tide");
    // The file holds it, and undo takes each step back.
    let text = ok(&mut d, "doc.export", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        text.starts_with("Title: High Tide\nDraft date: October 22, 2026\n"),
        "{text}"
    );
    let h = ok(&mut d, "history.list", json!({}));
    assert_eq!(h["undo"][0]["label"], "Title page");
    for _ in 0..3 {
        ok(&mut d, "undo", json!({}));
    }
    assert_eq!(
        ok(&mut d, "title.get", json!({}))["keys"][1]["values"][0]["text"],
        "Dana Reyes"
    );
}

#[test]
fn bad_keys_and_values_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = open(dir.path(), "s.fountain", KEYS);
    for p in [
        json!({"key": "", "values": ["x"]}),
        json!({"key": "Ti:tle", "values": ["x"]}),
        json!({"key": " Title", "values": ["x"]}),
        json!({"key": "Title", "values": ["two\nlines"]}),
        json!({"key": "Title", "values": [""]}),
        json!({"key": "Title", "values": [" Low Tide"]}),
        json!({"key": "Title", "values": ["Low Tide "]}),
    ] {
        let e = run(&mut d, &writer(), "title.set", p.clone()).unwrap_err();
        assert_eq!(e.code, "invalid_params", "{p}");
    }
}

#[test]
fn the_other_form_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = open(dir.path(), "s.fountain", KEYS);
    let e = run(&mut d, &writer(), "title.set_lines", json!({"lines": []})).unwrap_err();
    assert_eq!(e.code, "title_form");
    assert_eq!(e.details.unwrap()["form"], "keys");
}

#[test]
fn lines_are_checked_set_and_undone() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = open(dir.path(), "s.fountain", "INT. LAB - NIGHT\n\nRain.\n");
    assert_eq!(ok(&mut d, "title.get", json!({}))["form"], "none");
    let lines = json!({"lines": [
        {"text": "LOW TIDE", "align": "center", "left": 72, "right": 540, "space_before": 216},
        {"text": "by Dana Reyes", "align": "center", "left": 72, "right": 540},
    ]});
    ok(&mut d, "title.set_lines", lines);
    let t = ok(&mut d, "title.get", json!({}));
    assert_eq!(t["form"], "lines");
    assert_eq!(t["lines"][0]["space_before"], 216);
    assert_eq!(t["lines"][1]["align"], "center");
    // Now the keys are the other form.
    let e = run(
        &mut d,
        &writer(),
        "title.set",
        json!({"key": "Title", "values": ["x"]}),
    )
    .unwrap_err();
    assert_eq!(e.code, "title_form");
    // A column off the page, or space taller than the body, is refused.
    for bad in [
        json!({"lines": [{"text": "x", "left": 500, "right": 400}]}),
        json!({"lines": [{"text": "x", "left": 72, "right": 900}]}),
        json!({"lines": [{"text": "x", "left": 72, "right": 540, "space_before": 9999}]}),
    ] {
        let e = run(&mut d, &writer(), "title.set_lines", bad.clone()).unwrap_err();
        assert_eq!(e.code, "invalid_attr", "{bad}");
    }
    // The pages show it: a title page before the body.
    let pages = ok(&mut d, "doc.export", json!({"format": "pages"}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(pages.starts_with("=== page 1 · title"), "{pages}");
    assert!(pages.contains("LOW TIDE"));
    ok(&mut d, "undo", json!({}));
    assert_eq!(ok(&mut d, "title.get", json!({}))["form"], "none");
}

#[test]
fn agents_edit_the_title_page_only_while_their_edits_are_untracked() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = open(dir.path(), "s.fountain", KEYS);
    let e = run(
        &mut d,
        &agent(),
        "title.set",
        json!({"key": "Title", "values": ["Agent's Tide"]}),
    )
    .unwrap_err();
    assert_eq!(e.code, "writer_only");
    ok(&mut d, "changes.track", json!({"agents": false}));
    run(
        &mut d,
        &agent(),
        "title.set",
        json!({"key": "Title", "values": ["Agent's Tide"]}),
    )
    .unwrap();
    assert_eq!(
        ok(&mut d, "title.get", json!({}))["keys"][0]["values"][0]["text"],
        "Agent's Tide"
    );
}

#[test]
fn title_edits_are_never_tracked_or_marked() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = open(dir.path(), "s.fountain", KEYS);
    ok(&mut d, "changes.track", json!({"writer": true}));
    ok(&mut d, "revision.start", json!({}));
    ok(
        &mut d,
        "title.set",
        json!({"key": "Title", "values": ["High Tide"]}),
    );
    assert_eq!(ok(&mut d, "changes.list", json!({}))["changes"], json!([]));
    let list = ok(&mut d, "revision.list", json!({}));
    assert_eq!(list["sets"][0]["blocks"], 0);
}

#[test]
fn title_edits_survive_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = open(dir.path(), "s.fountain", KEYS);
    ok(
        &mut d,
        "title.set",
        json!({"key": "Title", "values": ["High Tide"]}),
    );
    let want = ok(&mut d, "title.get", json!({}));
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(
        &mut d,
        "doc.open",
        json!({"path": dir.path().join("s.fountain")}),
    );
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert_eq!(ok(&mut d, "title.get", json!({})), want);
}

/// An .fdx file's title lines, edited, are written back (fdx.md §13.5), and
/// the lines nobody touched keep their attributes.
#[test]
fn fdx_title_lines_are_edited_and_written_back() {
    let dir = tempfile::tempdir().unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx/title-page.fdx");
    let text = fs::read_to_string(fixture).unwrap();
    let mut d = open(dir.path(), "t.fdx", &text);
    let t = ok(&mut d, "title.get", json!({}));
    assert_eq!(t["form"], "lines");
    let mut lines = t["lines"].clone();
    lines[2]["text"] = json!("Story by");
    ok(&mut d, "title.set_lines", json!({"lines": lines}));
    ok(&mut d, "doc.save", json!({}));
    let saved = fs::read_to_string(dir.path().join("t.fdx")).unwrap();
    assert!(saved.contains("<Text>Story by</Text>"), "{saved}");
    assert!(!saved.contains("Written by"), "{saved}");
    assert!(
        saved.contains("<Tabstop Position=\"6.00\""),
        "an untouched line keeps its tab stops"
    );
    ok(&mut d, "doc.close", json!({}));
    let mut d = Dispatcher::new(Options::default());
    ok(
        &mut d,
        "doc.open",
        json!({"path": dir.path().join("t.fdx")}),
    );
    let again = ok(&mut d, "title.get", json!({}));
    assert_eq!(again["lines"], lines);
}

/// Words with Fountain's markup characters among them, so a value that
/// looks like emphasis, a note or a key line still comes back as it was.
const WORDS: [&str; 14] = [
    "Low", "Tide", "*", "_", "**x**", "a: b", "[[", "]]", "/*", "\\", "Ünï", "#1", "=", "INT.",
];

fn value() -> impl Strategy<Value = Value> {
    (prop::collection::vec(0..WORDS.len(), 1..4), any::<u8>()).prop_map(|(w, s)| {
        let text = w.iter().map(|&i| WORDS[i]).collect::<Vec<_>>().join(" ");
        if s % 3 == 0 {
            json!(text)
        } else {
            let mut run = json!({ "text": text });
            for (bit, name) in [(1, "bold"), (2, "italic"), (4, "underline")] {
                if s & bit != 0 {
                    run[name] = json!(true);
                }
            }
            json!([run])
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// Whatever `title.set` accepts, a Fountain file holds: saved and read
    /// again, the title page is the same.
    #[test]
    fn what_title_set_accepts_survives_fountain(
        sets in prop::collection::vec((0..5usize, prop::collection::vec(value(), 0..3)), 1..6)
    ) {
        const NAMES: [&str; 5] = ["Title", "Credit", "Author", "Draft date", "Rewrite-by"];
        let dir = tempfile::tempdir().unwrap();
        let mut d = open(dir.path(), "s.fountain", KEYS);
        for (k, values) in sets {
            // What a Fountain file could not hold is refused, never stored.
            let _ = run(&mut d, &writer(), "title.set", json!({"key": NAMES[k], "values": values}));
        }
        let want = ok(&mut d, "title.get", json!({}));
        let text = ok(&mut d, "doc.export", json!({}))["text"].as_str().unwrap().to_string();
        let mut again = open(dir.path(), "again.fountain", &text);
        let got = ok(&mut again, "title.get", json!({}));
        prop_assert_eq!(&got["keys"], &want["keys"], "{}", text);
    }
}

/// The laid-out pages follow the title lines: a view made before
/// `title.set_lines` shows the new lines after it.
#[test]
fn the_view_follows_title_lines() {
    let dir = tempfile::tempdir().unwrap();
    let mut d = open(dir.path(), "s.fountain", "INT. LAB - NIGHT\n\nRain.\n");
    let before = ok(&mut d, "doc.export", json!({"format": "pages"}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!before.contains("· title"), "{before}");
    let lines =
        json!({"lines": [{"text": "LOW TIDE", "align": "center", "left": 72, "right": 540}]});
    ok(&mut d, "title.set_lines", lines);
    let after = ok(&mut d, "doc.export", json!({"format": "pages"}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(after.contains("LOW TIDE"), "{after}");
    ok(&mut d, "undo", json!({}));
    let undone = ok(&mut d, "doc.export", json!({"format": "pages"}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(undone, before);
}
