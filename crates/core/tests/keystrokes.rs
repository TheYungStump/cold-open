//! Keystroke editing, undo grouping, authors, `batch`, find, completion,
//! history and revert (docs/specs/commands.md, phase 3 additions).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use coldopen_core::{Author, Clock, Command, Dispatcher, Options, journal};
use serde_json::{Value, json};

/// A dispatcher with a clock the test moves by hand.
struct Harness {
    d: Dispatcher,
    now: Arc<AtomicU64>,
}

impl Harness {
    fn new() -> Harness {
        Harness::with(Options::default())
    }

    fn with(opts: Options) -> Harness {
        let now = Arc::new(AtomicU64::new(0));
        let mut d = Dispatcher::new(opts);
        let n = now.clone();
        d.set_clock(Clock::new(move || n.load(Ordering::SeqCst)));
        Harness { d, now }
    }

    fn wait(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }

    fn run(&mut self, command: &str, params: Value, by: &Author) -> Result<Value, String> {
        let cmd = Command::from_parts(command, params).map_err(|e| format!("bad params: {e}"))?;
        self.d.execute_as(cmd, by).map_err(|e| e.code.to_string())
    }

    fn ok(&mut self, command: &str, params: Value) -> Value {
        match self.run(command, params, &Author::Writer) {
            Ok(v) => v,
            Err(e) => panic!("{command} failed: {e}"),
        }
    }

    fn err(&mut self, command: &str, params: Value) -> String {
        match self.run(command, params, &Author::Writer) {
            Ok(v) => panic!("{command} should fail, got {v}"),
            Err(e) => e,
        }
    }

    fn text(&mut self, id: &str) -> String {
        let blocks = self.ok("doc.get", json!({}))["blocks"].clone();
        blocks
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == id)
            .map(|b| b["text"].as_str().unwrap_or("").to_string())
            .unwrap()
    }

    fn texts(&mut self) -> Vec<String> {
        self.ok("doc.get", json!({}))["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| {
                format!(
                    "{}:{}",
                    b["type"].as_str().unwrap(),
                    b["text"].as_str().unwrap_or("")
                )
            })
            .collect()
    }

    /// Types `s` one character at a time, `gap` ms apart.
    fn type_str(&mut self, id: &str, at: usize, s: &str, gap: u64) {
        for (k, c) in s.chars().enumerate() {
            self.wait(gap);
            self.ok(
                "text.insert",
                json!({"id": id, "at": at + k, "text": c.to_string()}),
            );
        }
    }

    fn undo_labels(&mut self) -> Vec<String> {
        self.ok("history.list", json!({}))["undo"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["label"].as_str().unwrap().to_string())
            .collect()
    }
}

fn agent(tool: &str) -> Author {
    Author::agent("claude-code", Some(tool.into()))
}

fn new_doc(h: &mut Harness) -> String {
    h.ok("doc.new", json!({}));
    h.ok("block.insert", json!({"type": "action", "text": ""}))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The `recovered` report without `last`, which comes from the wall clock;
/// `last` must be a recent time.
fn recovered(r: &Value) -> Value {
    let mut v = r["recovered"].clone();
    let last = v.as_object_mut().and_then(|o| o.remove("last"));
    assert!(
        last.and_then(|l| l.as_u64())
            .is_some_and(|t| t > 1_600_000_000_000),
        "{r}"
    );
    v
}

#[test]
fn insert_delete_style_split_join() {
    let mut h = Harness::new();
    let id = new_doc(&mut h);
    h.ok(
        "text.insert",
        json!({"id": id, "at": 0, "text": "Rain on the roof."}),
    );
    h.ok("text.delete", json!({"id": id, "from": 4, "to": 7}));
    assert_eq!(h.text(&id), "Rain the roof.");
    h.ok(
        "text.style",
        json!({"id": id, "from": 0, "to": 4, "bold": true}),
    );
    let b = h.ok("doc.get", json!({}))["blocks"][0].clone();
    assert_eq!(b["runs"][0], json!({"text": "Rain", "bold": true}));

    let second = h.ok(
        "block.split",
        json!({"id": id, "at": 5, "type": "character"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(h.texts(), ["action:Rain ", "character:the roof."]);
    let joined = h.ok("block.join", json!({"id": second}));
    assert_eq!(joined, json!({"id": id, "at": 5}));
    assert_eq!(h.texts(), ["action:Rain the roof."]);

    // Accented letters count as one offset each.
    h.ok(
        "text.insert",
        json!({"id": id, "at": 0, "text": "Café ñu! "}),
    );
    h.ok("text.insert", json!({"id": id, "at": 4, "text": "s"}));
    assert_eq!(h.text(&id), "Cafés ñu! Rain the roof.");
}

#[test]
fn keystroke_errors() {
    let mut h = Harness::new();
    let id = new_doc(&mut h);
    h.ok("text.insert", json!({"id": id, "at": 0, "text": "abc"}));
    assert_eq!(
        h.err("text.insert", json!({"id": id, "at": 4, "text": "x"})),
        "invalid_range"
    );
    assert_eq!(
        h.err("text.delete", json!({"id": id, "from": 2, "to": 1})),
        "invalid_range"
    );
    assert_eq!(
        h.err("block.split", json!({"id": id, "at": 9})),
        "invalid_range"
    );
    assert_eq!(h.err("block.join", json!({"id": id})), "invalid_params");
    assert_eq!(
        h.err("text.insert", json!({"id": "b_99", "at": 0, "text": "x"})),
        "block_not_found"
    );
    let pb = h.ok("block.insert", json!({"type": "page_break"}))["id"].clone();
    assert_eq!(
        h.err("text.insert", json!({"id": pb, "at": 0, "text": "x"})),
        "invalid_attr"
    );
    let note = h.ok("block.insert", json!({"type": "note", "text": "n"}))["id"].clone();
    assert_eq!(
        h.err(
            "text.insert",
            json!({"id": note, "at": 1, "text": "x", "style": {"bold": true}})
        ),
        "invalid_attr"
    );
    assert_eq!(
        h.err(
            "text.style",
            json!({"id": note, "from": 0, "to": 1, "italic": true})
        ),
        "invalid_attr"
    );
    // Joining styled text into a note breaks the model, so it is refused.
    let after = h.ok(
        "block.insert",
        json!({"type": "action", "runs": [{"text": "x", "bold": true}]}),
    )["id"]
        .clone();
    assert_eq!(h.err("block.join", json!({"id": after})), "invalid_attr");
    // Empty changes succeed without an undo step.
    let before = h.undo_labels().len();
    h.ok("text.insert", json!({"id": id, "at": 1, "text": ""}));
    h.ok("text.delete", json!({"id": id, "from": 1, "to": 1}));
    assert_eq!(h.undo_labels().len(), before);
}

#[test]
fn typing_groups_into_one_undo_step() {
    let mut h = Harness::new();
    let id = new_doc(&mut h);
    h.type_str(&id, 0, "Hello", 200);
    // A pause of 1.5 s or more starts a new step.
    h.wait(1500);
    h.type_str(&id, 5, " you", 100);
    assert_eq!(h.text(&id), "Hello you");
    assert_eq!(h.undo_labels(), ["Typing", "Typing", "Insert block"]);

    // A backspace run is one step; it does not join the typing.
    h.wait(100);
    h.ok("text.delete", json!({"id": id, "from": 8, "to": 9}));
    h.ok("text.delete", json!({"id": id, "from": 7, "to": 8}));
    assert_eq!(h.text(&id), "Hello y");
    assert_eq!(h.undo_labels()[0], "Delete");
    assert_eq!(h.undo_labels().len(), 4);

    let r = h.ok("undo", json!({}));
    assert_eq!(r, json!({"undone": "typing", "label": "Delete"}));
    assert_eq!(h.text(&id), "Hello you");
    h.ok("undo", json!({}));
    assert_eq!(h.text(&id), "Hello");
    // After an undo, typing starts a new step even within the time limit.
    h.type_str(&id, 5, "!", 10);
    h.type_str(&id, 6, "!", 10);
    assert_eq!(h.undo_labels(), ["Typing", "Typing", "Insert block"]);
    h.ok("undo", json!({}));
    assert_eq!(h.text(&id), "Hello");

    // Typing somewhere else starts a new step.
    h.type_str(&id, 0, "Oh", 10);
    h.type_str(&id, 7, ".", 10);
    assert_eq!(h.text(&id), "OhHello.");
    h.ok("undo", json!({}));
    assert_eq!(h.text(&id), "OhHello");
    h.ok("redo", json!({}));
    assert_eq!(h.text(&id), "OhHello.");
}

#[test]
fn a_single_insert_keeps_its_own_command_name() {
    let mut h = Harness::new();
    let id = new_doc(&mut h);
    h.ok("text.insert", json!({"id": id, "at": 0, "text": "x"}));
    let r = h.ok("undo", json!({}));
    assert_eq!(r, json!({"undone": "text.insert", "label": "Typing"}));
}

#[test]
fn agents_get_labelled_steps_and_only_undo_their_own() {
    let mut h = Harness::new();
    let id = new_doc(&mut h);
    let a = agent("text_insert");
    // Agent keystrokes never group, even when contiguous and fast.
    h.run("text.insert", json!({"id": id, "at": 0, "text": "a"}), &a)
        .unwrap();
    h.run("text.insert", json!({"id": id, "at": 1, "text": "b"}), &a)
        .unwrap();
    assert_eq!(
        h.undo_labels(),
        [
            "claude-code · text_insert",
            "claude-code · text_insert",
            "Insert block"
        ]
    );
    let hist = h.ok("history.list", json!({}));
    assert_eq!(
        hist["undo"][0]["by"],
        json!({"kind": "agent", "name": "claude-code", "tool": "text_insert"})
    );
    assert_eq!(hist["undo"][2]["by"], json!({"kind": "writer"}));

    // The agent undoes its own step, under any tool name.
    h.run("undo", json!({}), &agent("undo")).unwrap();
    assert_eq!(h.text(&id), "a");
    // The writer types; now the agent may not undo the writer's step.
    h.type_str(&id, 1, "z", 10);
    assert_eq!(
        h.run("undo", json!({}), &agent("undo")).unwrap_err(),
        "not_your_step"
    );
    assert_eq!(h.text(&id), "az");
    // Nor redo a writer's step.
    h.ok("undo", json!({}));
    assert_eq!(
        h.run("redo", json!({}), &agent("redo")).unwrap_err(),
        "not_your_step"
    );
    // The writer may undo anyone's step.
    h.ok("undo", json!({}));
    assert_eq!(h.text(&id), "");
    // Another agent is someone else too.
    h.ok("redo", json!({}));
    let other = Author::agent("other-agent", Some("undo".into()));
    assert_eq!(
        h.run("undo", json!({}), &other).unwrap_err(),
        "not_your_step"
    );
}

#[test]
fn batch_is_atomic_and_one_step() {
    let mut h = Harness::new();
    let id = new_doc(&mut h);
    let r = h.ok(
        "batch",
        json!({"label": "Add a beat", "commands": [
            {"command": "text.insert", "params": {"id": id, "at": 0, "text": "Thunder."}},
            {"command": "block.insert", "params": {"type": "character", "text": "MAYA", "after": id}},
            {"command": "block.insert", "params": {"type": "dialogue", "text": "Run!"}},
        ]}),
    );
    assert_eq!(r["results"].as_array().unwrap().len(), 3);
    assert_eq!(r["results"][1]["id"], "b_2");
    assert_eq!(
        h.texts(),
        ["action:Thunder.", "character:MAYA", "dialogue:Run!"]
    );
    assert_eq!(h.undo_labels()[0], "Add a beat");
    let u = h.ok("undo", json!({}));
    assert_eq!(u["undone"], "batch");
    assert_eq!(h.texts(), ["action:"]);
    h.ok("redo", json!({}));
    assert_eq!(h.texts().len(), 3);

    // A failure part-way leaves nothing behind, and names its position.
    let before = h.texts();
    let cmd = Command::from_parts(
        "batch",
        json!({"commands": [
            {"command": "block.insert", "params": {"type": "action", "text": "x"}},
            {"command": "block.delete", "params": {"id": "b_99"}},
        ]}),
    )
    .unwrap();
    let e = h.d.execute(cmd).unwrap_err();
    assert_eq!(e.code, "block_not_found");
    assert_eq!(e.details.as_ref().unwrap()["index"], 1);
    assert_eq!(h.texts(), before);
    // The failed batch's id was never handed out.
    let next = h.ok("block.insert", json!({"type": "action", "text": "y"}))["id"].clone();
    assert_eq!(next, "b_4");

    // Only edits may appear, checked before anything runs.
    for bad in [
        json!({"commands": [{"command": "block.insert", "params": {"type": "action"}}, {"command": "undo"}]}),
        json!({"commands": [{"command": "doc.get"}]}),
        json!({"commands": [{"command": "batch", "params": {"commands": []}}]}),
        json!({"commands": [{"command": "doc.save"}]}),
        json!({"commands": [{"command": "no.such"}]}),
    ] {
        assert_eq!(h.err("batch", bad), "invalid_params");
    }
    assert_eq!(h.texts().len(), 4);
    // An empty batch changes nothing.
    let n = h.undo_labels().len();
    h.ok("batch", json!({"commands": []}));
    assert_eq!(h.undo_labels().len(), n);
}

#[test]
fn find_replace_one_and_complete() {
    let mut h = Harness::new();
    h.ok("doc.new", json!({}));
    let s = h.ok(
        "block.insert",
        json!({"type": "scene_heading", "text": "INT. LAB - NIGHT"}),
    )["id"]
        .clone();
    let a = h.ok(
        "block.insert",
        json!({"type": "action", "text": "Maya and maya and MAYA."}),
    )["id"]
        .clone();
    h.ok("block.insert", json!({"type": "note", "text": "maya"}));
    let m = h.ok("text.find", json!({"find": "maya", "match_case": false}));
    assert_eq!(
        m["matches"],
        json!([
            {"id": a, "from": 0, "to": 4},
            {"id": a, "from": 9, "to": 13},
            {"id": a, "from": 18, "to": 22},
        ])
    );
    assert_eq!(h.err("text.find", json!({"find": ""})), "invalid_params");
    let m = h.ok("text.find", json!({"find": "LAB", "scope": {"scene": s}}));
    assert_eq!(m["matches"], json!([{"id": s, "from": 5, "to": 8}]));

    let r = h.ok(
        "text.replace",
        json!({"find": "maya", "with": "Nina", "match_case": false, "at": {"id": a, "from": 9}}),
    );
    assert_eq!(r, json!({"count": 1, "blocks": [a]}));
    assert_eq!(h.text(a.as_str().unwrap()), "Maya and Nina and MAYA.");
    let r = h.ok(
        "text.replace",
        json!({"find": "maya", "with": "Nina", "match_case": false, "at": {"id": a, "from": 3}}),
    );
    assert_eq!(r["count"], 0);
    assert_eq!(
        h.err(
            "text.replace",
            json!({"find": "a", "with": "b", "at": {"id": a, "from": 0}, "scope": {"scene": s}})
        ),
        "invalid_params"
    );

    let c = h.ok("block.insert", json!({"type": "character", "text": "MAYA"}));
    let c2 = h.ok("block.insert", json!({"type": "character", "text": "ma"}))["id"].clone();
    let r = h.ok("text.complete", json!({"id": c2, "at": 2}));
    assert_eq!(r["context"], "character");
    assert_eq!(r["items"], json!([{"text": "MAYA", "count": 1}]));
    assert_eq!((r["from"].clone(), r["to"].clone()), (json!(0), json!(2)));
    assert_eq!(
        h.err("text.complete", json!({"id": c2, "at": 3})),
        "invalid_range"
    );
    let _ = c;
}

#[test]
fn rev_counts_changes() {
    let mut h = Harness::new();
    let id = new_doc(&mut h);
    assert_eq!(h.ok("doc.get", json!({}))["rev"], 1);
    h.type_str(&id, 0, "ab", 10);
    assert_eq!(h.ok("doc.get", json!({}))["rev"], 3);
    h.ok("undo", json!({}));
    assert_eq!(h.ok("doc.get", json!({}))["rev"], 4);
    h.ok("text.insert", json!({"id": id, "at": 0, "text": ""}));
    assert_eq!(h.ok("doc.get", json!({}))["rev"], 4);
}

fn script(dir: &Path) -> PathBuf {
    let p = dir.join("keys.fountain");
    fs::write(&p, "INT. LAB - NIGHT\n\nRain.\n").unwrap();
    p
}

/// Dropping a dispatcher without shutting down is what a crash looks like
/// to the files on disk.
fn crash(h: Harness) {
    drop(h);
}

#[test]
fn recovery_rebuilds_groups_authors_and_labels() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.type_str("b_2", 5, " Wind", 100);
    h.wait(2000);
    h.type_str("b_2", 10, ".", 100);
    h.run(
        "block.insert",
        json!({"type": "action", "text": "Thunder."}),
        &agent("block_insert"),
    )
    .unwrap();
    let labels = h.undo_labels();
    let text = h.texts();
    crash(h);

    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(
        recovered(&r),
        json!({"commands": 7, "external_change": false})
    );
    assert_eq!(h2.texts(), text);
    assert_eq!(h2.undo_labels(), labels);
    assert_eq!(labels, ["claude-code · block_insert", "Typing", "Typing"]);
    // The agent's step is still the agent's after recovery.
    h2.run("undo", json!({}), &agent("undo")).unwrap();
    assert_eq!(
        h2.run("undo", json!({}), &agent("undo")).unwrap_err(),
        "not_your_step"
    );
    h2.ok("undo", json!({}));
    assert_eq!(h2.text("b_2"), "Rain. Wind");
}

#[test]
fn a_never_saved_document_survives_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("Untitled-1.fountain");
    let mut h = Harness::new();
    h.ok("doc.new", json!({"path": p}));
    let id = h.ok("block.insert", json!({"type": "action", "text": ""}))["id"]
        .as_str()
        .unwrap()
        .to_string();
    h.type_str(&id, 0, "Lost?", 10);
    crash(h);
    assert!(!p.exists());

    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["external_change"], false);
    assert_eq!(h2.texts(), ["action:Lost?"]);
    assert_eq!(h2.ok("doc.get", json!({}))["dirty"], true);

    // Without a journal, a missing file is still an error.
    let mut h3 = Harness::new();
    assert_eq!(
        h3.err(
            "doc.open",
            json!({"path": dir.path().join("nope.fountain")})
        ),
        "io_error"
    );
}

#[test]
fn revert_reloads_and_keeps_the_journal_aside() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.type_str("b_2", 5, " Wind", 10);
    let rev = h.ok("doc.get", json!({}))["rev"].as_u64().unwrap();
    let r = h.ok("doc.revert", json!({}));
    assert_eq!(r["blocks"], 2);
    assert_eq!(r["warnings"][0]["code"], "journal_discarded");
    assert_eq!(
        h.texts(),
        ["scene_heading:INT. LAB - NIGHT", "action:Rain."]
    );
    let got = h.ok("doc.get", json!({}));
    assert_eq!(got["dirty"], false);
    assert_eq!(got["rev"].as_u64().unwrap(), rev + 1);
    assert_eq!(h.err("undo", json!({})), "nothing_to_undo");
    let aside = fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".discarded-")
        })
        .count();
    assert_eq!(aside, 1);
    // The reread blocks get fresh ids, so an id from before the revert
    // never names another block (review finding F12): "Rain." is b_4 now.
    assert_eq!(
        h.err("text.insert", json!({"id": "b_2", "at": 0, "text": "x"})),
        "block_not_found"
    );
    // The journal goes on: a crash now recovers new edits only.
    h.type_str("b_4", 5, "!", 10);
    crash(h);
    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1);
    assert_eq!(h2.text("b_4"), "Rain.!");

    let mut h3 = Harness::new();
    h3.ok("doc.new", json!({}));
    assert_eq!(h3.err("doc.revert", json!({})), "no_path");
}

#[test]
fn saved_markers_are_skipped_and_name_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.type_str("b_2", 5, " Wind", 10);
    crash(h);
    // Pretend a save of other bytes started: append a marker by hand, then
    // put those bytes in the file.
    let jp = journal::path_for(&p);
    let new_bytes = b"INT. LAB - NIGHT\n\nRain. Wind\n";
    let fp = coldopen_core::io::Fingerprint::of(new_bytes);
    let mut text = fs::read_to_string(&jp).unwrap();
    let seq = text.lines().count();
    text.push_str(
        &serde_json::to_string(&json!({
            "seq": seq, "t": 0, "command": "doc.saved",
            "params": {"disk": fp}, "result": {}
        }))
        .unwrap(),
    );
    text.push('\n');
    fs::write(&jp, text).unwrap();
    fs::write(&p, b"INT. LAB - NIGHT\n\nRain. Win\n").unwrap();
    // The file matches neither the header nor the marker: an outside change.
    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(
        recovered(&r),
        json!({"commands": 5, "external_change": true})
    );
    crash(h2);
    // The marker's own bytes are not an outside change; and since they
    // equal the replayed document, there is nothing to recover.
    fs::write(&p, new_bytes).unwrap();
    let mut h3 = Harness::new();
    let r = h3.ok("doc.open", json!({"path": p}));
    assert_eq!(r["recovered"], Value::Null);
}

#[test]
fn the_view_follows_edits_and_matches_a_full_layout() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("long.fountain");
    let script = coldopen_layout::synthetic::feature_script(1);
    fs::write(&p, coldopen_fountain::write(&script).text).unwrap();
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    let pages = h.d.view().unwrap().layout().pages.len();
    assert!(pages >= 118, "{pages}");
    // Type into a speech about three quarters of the way through.
    let blocks = h.ok("doc.get", json!({}))["blocks"].clone();
    let speeches: Vec<&Value> = blocks
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] == "dialogue")
        .collect();
    let target = speeches[speeches.len() * 3 / 4];
    let id = target["id"].as_str().unwrap().to_string();
    let len = target["text"].as_str().unwrap().chars().count();
    h.type_str(&id, len, " Wait.", 10);
    let doc = h.d.session().unwrap().doc.to_script();
    let view = h.d.view().unwrap();
    assert!(view.stats().laid_out <= 3, "{:?}", view.stats());
    let full = coldopen_layout::layout(
        &coldopen_fountain::normalize(&doc),
        &coldopen_layout::ValidFormat::default(),
        &coldopen_layout::Options::default(),
    );
    assert_eq!(view.layout(), &full);
    // Queries read the same view.
    let scenes = h.ok("scene.list", json!({}))["scenes"].clone();
    assert!(scenes.as_array().unwrap().len() > 100);
}

#[test]
fn a_new_document_can_start_with_an_empty_block_and_stay_clean() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("Untitled-2.fountain");
    let mut h = Harness::new();
    h.ok("doc.new", json!({"path": p, "start": "scene_heading"}));
    assert_eq!(h.texts(), ["scene_heading:"]);
    assert_eq!(h.ok("doc.get", json!({}))["dirty"], false);
    assert_eq!(h.err("undo", json!({})), "nothing_to_undo");
    // Nothing was journaled, so a crash now leaves nothing to recover and
    // the next session starts a new script instead.
    let j = h.d.session().unwrap().journal.as_ref().unwrap();
    assert_eq!(j.entries(), 0);
    // Typing is recorded against the starting block.
    h.type_str("b_1", 0, "EXT. PIER", 10);
    crash(h);
    let mut h2 = Harness::new();
    h2.ok("doc.open", json!({"path": p}));
    assert_eq!(h2.texts(), ["scene_heading:EXT. PIER"]);
    assert!(
        Harness::new()
            .err("doc.new", json!({"start": "nonsense"}))
            .starts_with("bad params")
    );
}
