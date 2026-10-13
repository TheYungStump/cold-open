//! Property tests: random edit sequences against undo, redo and the
//! crash journal.

use std::fs;

use coldopen_core::{Dispatcher, Options};
use proptest::prelude::*;
use serde_json::{Value, json};

const START: &str = "\
INT. KITCHEN - DAY

Maya makes toast.

MAYA
Breakfast!

# Act Two

EXT. GARDEN - DAY

Theo waters plants.

INT. SHED - NIGHT

A lamp flickers.
";

const KINDS: &[&str] = &[
    "scene_heading",
    "action",
    "character",
    "parenthetical",
    "dialogue",
    "transition",
    "lyric",
    "section",
    "synopsis",
    "note",
    "page_break",
];

/// An edit described by positions, resolved against the document when run.
#[derive(Clone, Debug)]
enum Step {
    Insert {
        at: usize,
        kind: usize,
        text: String,
    },
    SetType {
        at: usize,
        kind: usize,
    },
    SetText {
        at: usize,
        text: String,
    },
    Delete {
        at: usize,
    },
    Replace {
        find: String,
        with: String,
        case: bool,
    },
    Move {
        scene: usize,
        before: Option<usize>,
    },
    Attrs {
        at: usize,
        space: Option<u8>,
    },
    Undo,
    Redo,
}

fn step() -> impl Strategy<Value = Step> {
    let text = prop::sample::select(&["Maya", "a", "toast", "NINA", "", "x y", "MAYA (V.O.)"][..])
        .prop_map(str::to_string);
    prop_oneof![
        (any::<usize>(), 0..KINDS.len(), text.clone()).prop_map(|(at, kind, text)| Step::Insert {
            at,
            kind,
            text
        }),
        (any::<usize>(), 0..KINDS.len()).prop_map(|(at, kind)| Step::SetType { at, kind }),
        (any::<usize>(), text.clone()).prop_map(|(at, text)| Step::SetText { at, text }),
        any::<usize>().prop_map(|at| Step::Delete { at }),
        (text.clone(), text, any::<bool>()).prop_map(|(find, with, case)| Step::Replace {
            find,
            with,
            case
        }),
        (any::<usize>(), prop::option::of(any::<usize>()))
            .prop_map(|(scene, before)| Step::Move { scene, before }),
        (any::<usize>(), prop::option::of(0u8..4))
            .prop_map(|(at, space)| Step::Attrs { at, space }),
        Just(Step::Undo),
        Just(Step::Redo),
    ]
}

fn call(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    d.handle(&json!({ "command": command, "params": params }))
}

fn blocks(d: &mut Dispatcher) -> Value {
    call(d, "doc.get", json!({}))["result"]["blocks"].clone()
}

fn ids(d: &mut Dispatcher) -> Vec<Value> {
    blocks(d)
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].clone())
        .collect()
}

fn scene_ids(d: &mut Dispatcher) -> Vec<Value> {
    call(d, "scene.list", json!({}))["result"]["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].clone())
        .collect()
}

/// Runs one step. Returns true if it changed the undo history.
fn run(d: &mut Dispatcher, s: &Step) -> bool {
    let ids = ids(d);
    let pick = |at: usize| ids.get(at % ids.len().max(1)).cloned();
    let r = match s {
        Step::Insert { at, kind, text } => {
            let mut p = json!({ "type": KINDS[*kind], "text": text });
            if KINDS[*kind] == "page_break" {
                p = json!({ "type": "page_break" });
            }
            if let Some(id) = pick(*at) {
                p["after"] = id;
            }
            call(d, "block.insert", p)
        }
        Step::SetType { at, kind } => match pick(*at) {
            Some(id) => call(
                d,
                "block.set_type",
                json!({ "id": id, "type": KINDS[*kind] }),
            ),
            None => return false,
        },
        Step::SetText { at, text } => match pick(*at) {
            Some(id) => call(d, "block.set_text", json!({ "id": id, "text": text })),
            None => return false,
        },
        Step::Delete { at } => match pick(*at) {
            Some(id) => call(d, "block.delete", json!({ "id": id })),
            None => return false,
        },
        Step::Replace { find, with, case } => call(
            d,
            "text.replace",
            json!({ "find": find, "with": with, "match_case": case }),
        ),
        Step::Move { scene, before } => {
            let scenes = scene_ids(d);
            if scenes.is_empty() {
                return false;
            }
            let mut p = json!({ "id": scenes[scene % scenes.len()] });
            if let Some(b) = before {
                p["before"] = scenes[b % scenes.len()].clone();
            }
            call(d, "scene.move", p)
        }
        Step::Attrs { at, space } => match pick(*at) {
            Some(id) => call(
                d,
                "block.set_attrs",
                json!({ "id": id, "space_before": space }),
            ),
            None => return false,
        },
        Step::Undo => call(d, "undo", json!({})),
        Step::Redo => call(d, "redo", json!({})),
    };
    r["ok"] == true
}

fn script_of(blocks: &Value) -> coldopen_screenplay::Script {
    let elements = blocks
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            let mut b = b.clone();
            b.as_object_mut().unwrap().remove("id");
            serde_json::from_value(b).unwrap()
        })
        .collect();
    coldopen_screenplay::Script {
        title_page: Default::default(),
        elements,
    }
}

fn undo_all(d: &mut Dispatcher) -> usize {
    let mut n = 0;
    while call(d, "undo", json!({}))["ok"] == true {
        n += 1;
    }
    n
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// Undoing everything returns the starting document; redoing returns
    /// the final one. Ids come back unchanged.
    #[test]
    fn undo_and_redo_are_exact(steps in prop::collection::vec(step(), 0..40)) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.fountain");
        fs::write(&path, START).unwrap();
        let mut d = Dispatcher::new(Options { journal: false, fsync: false });
        prop_assert_eq!(call(&mut d, "doc.open", json!({ "path": path }))["ok"].clone(), json!(true));
        let start = blocks(&mut d);
        for s in &steps {
            run(&mut d, s);
        }
        let end = blocks(&mut d);
        let n = undo_all(&mut d);
        prop_assert_eq!(blocks(&mut d), start);
        for _ in 0..n {
            prop_assert_eq!(call(&mut d, "redo", json!({}))["ok"].clone(), json!(true));
        }
        prop_assert_eq!(blocks(&mut d), end);
    }

    /// After a crash, the journal brings back exactly the live document.
    #[test]
    fn journal_replay_matches_the_live_session(steps in prop::collection::vec(step(), 0..30)) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.fountain");
        fs::write(&path, START).unwrap();
        let opts = Options { journal: true, fsync: false };
        let mut d = Dispatcher::new(opts);
        call(&mut d, "doc.open", json!({ "path": path }));
        for s in &steps {
            run(&mut d, s);
        }
        let live = blocks(&mut d);
        drop(d); // a crash: no shutdown, the journal stays

        let mut d2 = Dispatcher::new(opts);
        let r = call(&mut d2, "doc.open", json!({ "path": path }));
        prop_assert_eq!(r["ok"].clone(), json!(true), "{}", r);
        prop_assert_eq!(r["result"]["warnings"].clone(), json!([]));
        let recovered = blocks(&mut d2);
        if r["result"]["recovered"].is_null() {
            // Nothing to recover: the live document already writes out as
            // the file on disk, so ids were renumbered on open. Compare the
            // content in Fountain normal form instead.
            prop_assert_eq!(script_of(&recovered), coldopen_fountain::normalize(&script_of(&live)));
        } else {
            prop_assert_eq!(recovered, live);
        }
    }
}

fn request_line() -> impl Strategy<Value = String> {
    let name = prop::sample::select(coldopen_core::Command::NAMES.to_vec());
    let param = prop_oneof![
        Just(json!({})),
        Just(json!({"id": "b_1"})),
        Just(json!({"id": "b_2", "type": "action"})),
        Just(json!({"type": "dialogue", "text": "hi", "after": "b_1"})),
        Just(json!({"find": "X", "with": "Y"})),
        Just(json!({"id": "b_1", "before": "b_1"})),
        Just(json!({"id": "nope"})),
        Just(json!([1, 2])),
    ];
    prop_oneof![
        (name, param, any::<i64>()).prop_map(|(n, p, id)| {
            json!({"v": 1, "id": id, "command": n, "params": p}).to_string()
        }),
        any::<String>(),
        Just("{\"v\":1}".to_string()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// The envelope fuzz target's check, on stable.
    #[test]
    fn fuzz_envelope_check(lines in prop::collection::vec(request_line(), 0..30)) {
        let data = lines.join("\n");
        prop_assert_eq!(coldopen_core::fuzzing::envelope(data.as_bytes()), Ok(()));
    }

    /// The journal fuzz target's check, on stable: arbitrary bytes, and
    /// real journals with random damage.
    #[test]
    fn fuzz_journal_check(data in prop::collection::vec(any::<u8>(), 0..300), cut in any::<usize>(), flip in any::<usize>()) {
        prop_assert_eq!(coldopen_core::fuzzing::journal_replay(&data), Ok(()));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.fountain");
        fs::write(&path, START).unwrap();
        let mut d = Dispatcher::new(Options { journal: true, fsync: false });
        call(&mut d, "doc.open", json!({ "path": path }));
        call(&mut d, "block.insert", json!({ "type": "action", "text": "x" }));
        call(&mut d, "text.replace", json!({ "find": "a", "with": "b" }));
        drop(d);
        let mut bytes = fs::read(coldopen_core::journal::path_for(&path)).unwrap();
        let cut = cut % (bytes.len() + 1);
        bytes.truncate(cut);
        if !bytes.is_empty() {
            let i = flip % bytes.len();
            bytes[i] ^= 0x20;
        }
        prop_assert_eq!(coldopen_core::fuzzing::journal_replay(&bytes), Ok(()));
    }
}
