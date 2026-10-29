//! The scenes, characters and locations reports (docs/specs/reports.md)
//! against hand counts of our own fixtures, and a golden of the synthetic
//! script so their order never depends on the OS.
//!
//! `COLDOPEN_BLESS=1 cargo test -p coldopen-core --test reports` rewrites
//! the golden; review the diff before committing it.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Command, Dispatcher, Options};
use serde_json::{Value, json};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/reports")
}

fn run(d: &mut Dispatcher, command: &str) -> Value {
    d.execute(Command::from_parts(command, json!({})).unwrap())
        .unwrap_or_else(|e| panic!("{command}: {e}"))
}

/// The three reports of a script, with scene ids written as the scenes'
/// indices, so they compare with counts written by hand.
fn reports(text: &str) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, text).unwrap();
    let mut d = Dispatcher::new(Options::default());
    d.execute(Command::from_parts("doc.open", json!({"path": p})).unwrap())
        .unwrap();
    let scenes = run(&mut d, "report.scenes")["scenes"].clone();
    let index = |id: &Value| -> Value {
        scenes
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == *id)
            .map_or(Value::Null, |s| s["index"].clone())
    };
    let ids =
        |v: &Value| -> Value { Value::Array(v.as_array().unwrap().iter().map(index).collect()) };
    let mut characters = run(&mut d, "report.characters")["characters"].clone();
    for c in characters.as_array_mut().unwrap() {
        c["scenes"] = ids(&c["scenes"]);
    }
    let mut locations = run(&mut d, "report.locations")["locations"].clone();
    for l in locations.as_array_mut().unwrap() {
        l["scenes"] = ids(&l["scenes"]);
        for s in l["settings"].as_array_mut().unwrap() {
            s["scenes"] = ids(&s["scenes"]);
            for t in s["times"].as_array_mut().unwrap() {
                t["scenes"] = ids(&t["scenes"]);
            }
        }
    }
    d.execute(Command::from_parts("doc.close", json!({"discard": true})).unwrap())
        .unwrap();
    json!({ "scenes": scenes, "characters": characters, "locations": locations })
}

/// Only the fields `want` names, at every level.
fn pick(got: &Value, want: &Value) -> Value {
    match (got, want) {
        (Value::Object(g), Value::Object(w)) => Value::Object(
            w.keys()
                .map(|k| (k.clone(), pick(g.get(k).unwrap_or(&Value::Null), &w[k])))
                .collect(),
        ),
        (Value::Array(g), Value::Array(w)) if g.len() == w.len() => {
            Value::Array(g.iter().zip(w).map(|(a, b)| pick(a, b)).collect())
        }
        _ => got.clone(),
    }
}

#[test]
fn reports_match_hand_counts() {
    for name in ["kitchen", "hall"] {
        let text = fs::read_to_string(fixtures().join(format!("{name}.fountain"))).unwrap();
        let mut want: Value = serde_json::from_str(
            &fs::read_to_string(fixtures().join(format!("{name}.counts.json"))).unwrap(),
        )
        .unwrap();
        want.as_object_mut().unwrap().remove("comment");
        let got = reports(&text);
        assert_eq!(pick(&got, &want), want, "{name}");
    }
}

#[test]
fn the_long_speech_breaks_across_a_page_and_counts_whole() {
    let text = fs::read_to_string(fixtures().join("hall.fountain")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, text).unwrap();
    let mut d = Dispatcher::new(Options::default());
    d.execute(Command::from_parts("doc.open", json!({"path": p})).unwrap())
        .unwrap();
    let pages = run(&mut d, "doc.export");
    let view = d
        .execute(Command::from_parts("doc.export", json!({"format": "pages"})).unwrap())
        .unwrap();
    let text = view["text"].as_str().unwrap();
    assert!(text.contains("=== page 2"), "{pages}");
    assert!(text.contains("(MORE)"));
    let orator = &run(&mut d, "report.characters")["characters"][0];
    assert_eq!(orator["first_page"], 1);
}

#[test]
fn the_synthetic_scripts_reports_match_the_golden() {
    let script = coldopen_layout::synthetic::feature_script(1);
    let text = coldopen_fountain::write(&script).text;
    let got = serde_json::to_string_pretty(&reports(&text)).unwrap() + "\n";
    let golden = fixtures().join("synthetic.golden.json");
    if std::env::var("COLDOPEN_BLESS").is_ok_and(|v| v == "1") {
        fs::write(&golden, &got).unwrap();
    }
    let want = fs::read_to_string(&golden)
        .unwrap_or_else(|_| {
            panic!(
                "{} is missing; run with COLDOPEN_BLESS=1 and review it",
                golden.display()
            )
        })
        .replace("\r\n", "\n");
    assert!(want == got, "the reports differ from {}", golden.display());
    // Sorted as the spec says.
    let v = reports(&text);
    let words: Vec<u64> = v["characters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["words"].as_u64().unwrap())
        .collect();
    assert!(words.windows(2).all(|w| w[0] >= w[1]));
    let names: Vec<String> = v["locations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["location"].as_str().unwrap().to_uppercase())
        .collect();
    assert!(names.windows(2).all(|w| w[0] <= w[1]), "{names:?}");
}

/// AR6 (the reviews of phases 4–5b, 2026-10-28): a heading or section whose
/// deletion is pending is not in the current text, so it neither makes a
/// scene nor ends one in `scene.list` and the reports, and a pending
/// deleted block inside a speech does not end it (Antigravity's
/// reproduction, and a deleted action inside ALICE's speech).
#[test]
fn ar6_pending_deletions_make_and_end_no_scenes_or_speeches() {
    let mut d = Dispatcher::new(Options {
        journal: false,
        fsync: false,
    });
    let mut exec = |command: &str, params: Value| {
        d.execute(Command::from_parts(command, params).unwrap())
            .unwrap_or_else(|e| panic!("{command}: {e}"))
    };
    exec("doc.new", json!({}));
    for (after, kind, text) in [
        (None, "scene_heading", "INT. KITCHEN - DAY"),
        (Some("b_1"), "character", "ALICE"),
        (Some("b_2"), "dialogue", "Hello there."),
        (Some("b_3"), "section", "Beat two"),
        (Some("b_4"), "character", "BOB"),
        (Some("b_5"), "dialogue", "General Kenobi."),
        (Some("b_6"), "scene_heading", "EXT. ROOF - NIGHT"),
        (Some("b_7"), "character", "ALICE"),
        (Some("b_8"), "dialogue", "One."),
        (Some("b_9"), "action", "She turns."),
        (Some("b_10"), "dialogue", "Two three."),
    ] {
        exec(
            "block.insert",
            json!({"after": after, "type": kind, "text": text}),
        );
    }
    exec("changes.track", json!({"writer": true}));
    for id in ["b_4", "b_7", "b_10"] {
        exec("block.delete", json!({"id": id}));
    }
    let list = exec("scene.list", json!({}))["scenes"].clone();
    let rows = list.as_array().unwrap();
    assert_eq!(rows.len(), 1, "{list}");
    assert_eq!(rows[0]["id"], "b_1");
    // Every current block from the heading to the end: the section, the
    // second heading and the action are pending deletions.
    assert_eq!(rows[0]["blocks"], 8, "{list}");
    let scenes = exec("report.scenes", json!({}))["scenes"].clone();
    assert_eq!(scenes.as_array().unwrap().len(), 1, "{scenes}");
    assert_eq!(scenes[0]["characters"], json!(["ALICE", "BOB"]));
    let characters = exec("report.characters", json!({}))["characters"].clone();
    let row = |name: &str| {
        characters
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("{name}: {characters}"))
            .clone()
    };
    assert_eq!(row("BOB")["scenes"], json!(["b_1"]));
    // Hello there. One. Two three.
    assert_eq!(row("ALICE")["words"], 5, "{characters}");
    assert_eq!(row("ALICE")["speeches"], 2, "{characters}");
}
