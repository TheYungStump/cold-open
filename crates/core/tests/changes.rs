//! Track changes (docs/specs/changes.md): what each edit records when it
//! is tracked, settling, who may do what, and that changes survive a crash.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Author, Command, CommandError, Dispatcher, Options, journal};
use proptest::prelude::*;
use serde_json::{Value, json};

const SCRIPT: &str = "\
INT. KITCHEN - DAY

Maya makes toast.

MAYA
Breakfast is ready.

EXT. GARDEN - DAY

Theo waters plants.

INT. SHED - NIGHT

A lamp flickers.
";

fn writer() -> Author {
    Author::named_writer("Maya")
}

fn agent() -> Author {
    Author::agent("claude-code", Some("block_insert".into()))
}

fn other_agent() -> Author {
    Author::agent("other-agent", None)
}

fn run(
    d: &mut Dispatcher,
    by: &Author,
    command: &str,
    params: Value,
) -> Result<Value, CommandError> {
    d.execute_as(Command::from_parts(command, params).unwrap(), by)
}

fn ok(d: &mut Dispatcher, by: &Author, command: &str, params: Value) -> Value {
    run(d, by, command, params).unwrap_or_else(|e| panic!("{command} failed: {e}"))
}

fn err(d: &mut Dispatcher, by: &Author, command: &str, params: Value) -> CommandError {
    match run(d, by, command, params) {
        Ok(r) => panic!("{command} should fail: {r}"),
        Err(e) => e,
    }
}

fn open(dir: &Path) -> (Dispatcher, PathBuf) {
    let p = dir.join("s.fountain");
    fs::write(&p, SCRIPT).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &writer(), "doc.open", json!({"path": p}));
    (d, p)
}

fn blocks(d: &mut Dispatcher) -> Vec<Value> {
    ok(d, &writer(), "doc.get", json!({}))["blocks"]
        .as_array()
        .unwrap()
        .clone()
}

fn block(d: &mut Dispatcher, id: &str) -> Value {
    blocks(d)
        .into_iter()
        .find(|b| b["id"] == id)
        .unwrap_or_else(|| panic!("no block {id}"))
}

fn texts(d: &mut Dispatcher) -> Vec<String> {
    blocks(d)
        .iter()
        .map(|b| b["text"].as_str().unwrap_or("").to_string())
        .collect()
}

fn changes(d: &mut Dispatcher) -> Vec<Value> {
    ok(d, &writer(), "changes.list", json!({}))["changes"]
        .as_array()
        .unwrap()
        .clone()
}

fn track_writer(d: &mut Dispatcher) {
    ok(d, &writer(), "changes.track", json!({"writer": true}));
}

#[test]
fn tracked_typing_is_one_change_and_backspace_takes_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    track_writer(&mut d);
    // b_2 is "Maya makes toast."
    let r = ok(
        &mut d,
        &writer(),
        "text.insert",
        json!({"id": "b_2", "at": 17, "text": " Then"}),
    );
    assert_eq!(r["change"], "c_1", "{r}");
    let r = ok(
        &mut d,
        &writer(),
        "text.insert",
        json!({"id": "b_2", "at": 22, "text": " tea."}),
    );
    assert_eq!(r["change"], "c_1", "typing on joins its change: {r}");
    let b = block(&mut d, "b_2");
    assert_eq!(b["text"], "Maya makes toast. Then tea.");
    assert_eq!(b["runs"][1]["ins"], "c_1", "{b}");
    // A backspace over the writer's own insertion just takes it back.
    let r = ok(
        &mut d,
        &writer(),
        "text.delete",
        json!({"id": "b_2", "from": 26, "to": 27}),
    );
    assert!(r.get("change").is_none(), "nothing is left to review: {r}");
    let b = block(&mut d, "b_2");
    assert_eq!(b["text"], "Maya makes toast. Then tea");
    assert!(!b.to_string().contains("\"del\""), "{b}");
    // Deleting the original text strikes it instead.
    ok(
        &mut d,
        &writer(),
        "text.delete",
        json!({"id": "b_2", "from": 0, "to": 5}),
    );
    let b = block(&mut d, "b_2");
    assert_eq!(b["text"], "makes toast. Then tea");
    assert_eq!(b["runs"][0], json!({"text": "Maya ", "del": "c_2"}), "{b}");
    let list = changes(&mut d);
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(list[0]["id"], "c_2", "document order: {list:?}");
    assert_eq!(list[0]["by"], json!({"kind": "writer", "name": "Maya"}));
    assert_eq!(list[1]["kinds"], json!(["insert"]));
    assert_eq!(list[1]["inserted"], " Then tea");
}

#[test]
fn agent_edits_are_tracked_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    let r = ok(
        &mut d,
        &agent(),
        "block.insert",
        json!({"after": "b_2", "type": "action", "text": "The toaster pops."}),
    );
    assert_eq!(r["change"], "c_1", "{r}");
    let id = r["id"].as_str().unwrap().to_string();
    let b = block(&mut d, &id);
    assert_eq!(b["change"], json!({"inserted": "c_1"}), "{b}");
    assert_eq!(b["runs"][0]["ins"], "c_1");
    let list = changes(&mut d);
    assert_eq!(
        list[0]["by"],
        json!({"kind": "agent", "name": "claude-code"})
    );
    assert_eq!(list[0]["kinds"], json!(["insert", "block_insert"]));
    // An agent cannot opt out.
    let e = err(
        &mut d,
        &agent(),
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "x", "tracked": false}),
    );
    assert_eq!(e.code, "writer_only", "{e}");
    // The writer's own edits are not tracked until they say so.
    let r = ok(
        &mut d,
        &writer(),
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "So "}),
    );
    assert!(r.get("change").is_none(), "{r}");
    // A writer may ask for one edit to be tracked.
    let r = ok(
        &mut d,
        &writer(),
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "And ", "tracked": true}),
    );
    assert_eq!(r["change"], "c_2", "{r}");
}

#[test]
fn only_the_writer_accepts_and_agents_reject_only_their_own() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    track_writer(&mut d);
    ok(
        &mut d,
        &writer(),
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Slowly, "}),
    );
    ok(
        &mut d,
        &agent(),
        "text.replace",
        json!({"find": "toast", "with": "eggs"}),
    );
    ok(
        &mut d,
        &other_agent(),
        "text.insert",
        json!({"id": "b_6", "at": 0, "text": "Quietly, "}),
    );
    let e = err(&mut d, &agent(), "changes.accept", json!({"id": "c_2"}));
    assert_eq!(e.code, "writer_only");
    let e = err(&mut d, &agent(), "changes.reject", json!({"id": "c_1"}));
    assert_eq!(e.code, "not_your_change");
    // By all, an agent settles only its own.
    let r = ok(&mut d, &agent(), "changes.reject", json!({"all": true}));
    assert_eq!(r["changes"], json!(["c_2"]), "{r}");
    assert_eq!(texts(&mut d)[1], "Slowly, Maya makes toast.");
    assert_eq!(changes(&mut d).len(), 2);
    let e = err(&mut d, &agent(), "changes.track", json!({"agents": false}));
    assert_eq!(e.code, "writer_only");
    // The writer accepts everything.
    let r = ok(&mut d, &writer(), "changes.accept", json!({"all": true}));
    assert_eq!(r["count"], 2, "{r}");
    assert!(changes(&mut d).is_empty());
    assert!(
        !blocks(&mut d)
            .iter()
            .any(|b| b.to_string().contains("\"ins\""))
    );
    // Undo gives the marks back.
    ok(&mut d, &writer(), "undo", json!({}));
    assert_eq!(changes(&mut d).len(), 2);
}

#[test]
fn an_agent_cannot_edit_away_the_writers_pending_marks() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    track_writer(&mut d);
    ok(
        &mut d,
        &writer(),
        "text.insert",
        json!({"id": "b_2", "at": 17, "text": " Burnt."}),
    );
    // The writer lets agents edit untracked; an untracked delete of the
    // writer's insertion would still settle it, so it is refused.
    ok(&mut d, &writer(), "changes.track", json!({"agents": false}));
    let e = err(
        &mut d,
        &agent(),
        "text.delete",
        json!({"id": "b_2", "from": 17, "to": 24}),
    );
    assert_eq!(e.code, "not_your_change", "{e}");
    assert_eq!(texts(&mut d)[1], "Maya makes toast. Burnt.");
    // A tracked delete only proposes: the writer's insertion stays, struck.
    ok(&mut d, &writer(), "changes.track", json!({"agents": true}));
    ok(
        &mut d,
        &agent(),
        "text.delete",
        json!({"id": "b_2", "from": 17, "to": 24}),
    );
    let b = block(&mut d, "b_2");
    assert_eq!(b["text"], "Maya makes toast.");
    assert_eq!(
        b["runs"][1],
        json!({"text": " Burnt.", "ins": "c_1", "del": "c_2"}),
        "{b}"
    );
    // Rejecting the agent's deletion gives the writer's text back.
    ok(&mut d, &writer(), "changes.reject", json!({"id": "c_2"}));
    assert_eq!(texts(&mut d)[1], "Maya makes toast. Burnt.");
}

#[test]
fn deleted_blocks_stay_until_settled() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    let r = ok(&mut d, &agent(), "block.delete", json!({"id": "b_6"}));
    assert_eq!(r["change"], "c_1");
    let b = block(&mut d, "b_6");
    assert_eq!(b["text"], "", "the current text is empty: {b}");
    assert_eq!(b["change"], json!({"deleted": "c_1"}));
    let e = err(
        &mut d,
        &agent(),
        "text.insert",
        json!({"id": "b_6", "at": 0, "text": "x"}),
    );
    assert_eq!(e.code, "change_pending", "{e}");
    let e = err(
        &mut d,
        &writer(),
        "block.set_type",
        json!({"id": "b_6", "type": "character"}),
    );
    assert_eq!(e.code, "change_pending");
    // Rejected: the block and its text are back.
    ok(&mut d, &writer(), "changes.reject", json!({"id": "c_1"}));
    assert_eq!(block(&mut d, "b_6")["text"], "Theo waters plants.");
    assert!(block(&mut d, "b_6").get("change").is_none());
    // Accepted: gone.
    ok(&mut d, &agent(), "block.delete", json!({"id": "b_6"}));
    ok(&mut d, &writer(), "changes.accept", json!({"id": "c_2"}));
    assert!(!blocks(&mut d).iter().any(|b| b["id"] == "b_6"));
    // The writer can remove a pending deletion outright, untracked.
    ok(&mut d, &agent(), "block.delete", json!({"id": "b_8"}));
    ok(&mut d, &writer(), "block.delete", json!({"id": "b_8"}));
    assert!(changes(&mut d).is_empty());
}

#[test]
fn a_retype_is_rejected_with_its_attributes() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    ok(
        &mut d,
        &writer(),
        "block.set_attrs",
        json!({"id": "b_1", "scene_number": "4"}),
    );
    let r = ok(
        &mut d,
        &agent(),
        "block.set_type",
        json!({"id": "b_1", "type": "action"}),
    );
    assert_eq!(r["change"], "c_1");
    let b = block(&mut d, "b_1");
    assert_eq!(b["type"], "action");
    assert_eq!(
        b["change"],
        json!({"retyped": [{"change": "c_1", "from": "scene_heading", "scene_number": "4"}]}),
        "{b}"
    );
    ok(&mut d, &writer(), "changes.reject", json!({"id": "c_1"}));
    let b = block(&mut d, "b_1");
    assert_eq!(b["type"], "scene_heading");
    assert_eq!(b["scene_number"], "4");
    assert!(b.get("change").is_none());
}

#[test]
fn splits_and_joins_wait_for_review() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    track_writer(&mut d);
    // Enter at the end of a line: a new, inserted block.
    let r = ok(
        &mut d,
        &writer(),
        "block.split",
        json!({"id": "b_2", "at": 17, "type": "character"}),
    );
    let new = r["id"].as_str().unwrap().to_string();
    assert_eq!(block(&mut d, &new)["change"], json!({"inserted": "c_1"}));
    // A split in the middle moves text, and rejecting it joins it back.
    let r = ok(
        &mut d,
        &writer(),
        "block.split",
        json!({"id": "b_6", "at": 5}),
    );
    let tail = r["id"].as_str().unwrap().to_string();
    assert_eq!(block(&mut d, &tail)["text"], "waters plants.");
    assert_eq!(block(&mut d, &tail)["change"], json!({"split": "c_2"}));
    ok(&mut d, &writer(), "changes.reject", json!({"id": "c_2"}));
    assert_eq!(block(&mut d, "b_6")["text"], "Theo waters plants.");
    // A join of the writer's own original blocks is proposed.
    let r = ok(&mut d, &agent(), "block.join", json!({"id": "b_4"}));
    assert_eq!(r["id"], "b_3", "{r}");
    assert_eq!(block(&mut d, "b_4")["change"]["joined"], "c_3");
    ok(&mut d, &writer(), "changes.accept", json!({"id": "c_3"}));
    assert_eq!(block(&mut d, "b_3")["text"], "MAYABreakfast is ready.");
    assert!(!blocks(&mut d).iter().any(|b| b["id"] == "b_4"));
}

#[test]
fn a_moved_scene_goes_back_on_reject() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    // Scenes: b_1 (kitchen), b_5 (garden), b_7 (shed).
    let r = ok(
        &mut d,
        &agent(),
        "scene.move",
        json!({"id": "b_7", "before": "b_1"}),
    );
    assert_eq!(r["change"], "c_1");
    assert_eq!(blocks(&mut d)[0]["id"], "b_7");
    assert_eq!(
        block(&mut d, "b_7")["change"],
        json!({"moved": [{"change": "c_1", "before": null, "last": "b_8"}]})
    );
    ok(&mut d, &writer(), "changes.reject", json!({"id": "c_1"}));
    let ids: Vec<Value> = blocks(&mut d).iter().map(|b| b["id"].clone()).collect();
    assert_eq!(ids.last().unwrap(), "b_8");
    assert_eq!(ids[ids.len() - 2], "b_7");
    // A move whose old place is gone cannot be undone.
    ok(
        &mut d,
        &agent(),
        "scene.move",
        json!({"id": "b_1", "before": "b_7"}),
    );
    // b_1's scene stood before b_5; the writer deletes b_5's scene heading.
    ok(&mut d, &writer(), "block.delete", json!({"id": "b_5"}));
    let e = err(&mut d, &writer(), "changes.reject", json!({"id": "c_2"}));
    assert_eq!(e.code, "change_conflict", "{e}");
    let r = ok(&mut d, &writer(), "changes.reject", json!({"all": true}));
    assert_eq!(r["skipped"], json!(["c_2"]), "{r}");
}

#[test]
fn a_tracked_set_text_marks_words() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open(dir.path());
    let r = ok(
        &mut d,
        &agent(),
        "block.set_text",
        json!({"id": "b_6", "text": "Theo waters the roses."}),
    );
    assert_eq!(r["change"], "c_1");
    let b = block(&mut d, "b_6");
    assert_eq!(b["text"], "Theo waters the roses.");
    let list = changes(&mut d);
    assert_eq!(list[0]["inserted"], "the roses");
    assert_eq!(list[0]["deleted"], "plants");
    // Sending the runs back as doc.get gave them changes nothing.
    let runs = b["runs"].clone();
    let r = ok(
        &mut d,
        &agent(),
        "block.set_text",
        json!({"id": "b_6", "runs": runs}),
    );
    assert!(r.get("change").is_none(), "{r}");
    assert_eq!(block(&mut d, "b_6"), b);
}

#[test]
fn tracked_changes_survive_a_crash_with_authors_and_times() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, p) = open(dir.path());
    track_writer(&mut d);
    ok(
        &mut d,
        &writer(),
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Quickly, "}),
    );
    ok(&mut d, &agent(), "block.delete", json!({"id": "b_6"}));
    let before = changes(&mut d);
    let blocks_before = blocks(&mut d);
    drop(d); // a crash
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 3, "{r}");
    assert_eq!(changes(&mut d), before);
    assert_eq!(blocks(&mut d), blocks_before);
    // Save (losing the marks the writer agreed to lose), and crash again:
    // the header holds the table, so recovery has it.
    ok(&mut d, &agent(), "block.delete", json!({"id": "b_8"}));
    let all = changes(&mut d);
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &writer(), "doc.open", json!({"path": p}));
    assert_eq!(changes(&mut d), all);
    d.discard();
}

#[test]
fn saving_pending_changes_to_fountain_asks_first() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, p) = open(dir.path());
    ok(
        &mut d,
        &agent(),
        "text.replace",
        json!({"find": "toast", "with": "eggs"}),
    );
    let e = err(&mut d, &writer(), "doc.save", json!({}));
    assert_eq!(e.code, "lossy_format");
    let losses = &e.details.as_ref().unwrap()["losses"];
    assert_eq!(losses[0]["code"], "tracked_changes", "{losses}");
    assert_eq!(losses[0]["class"], "lost");
    // An agent can never answer that question.
    let e = err(&mut d, &agent(), "doc.save", json!({"allow_lossy": true}));
    assert_eq!(e.code, "writer_only");
    // The writer can: the file holds the current text.
    ok(&mut d, &writer(), "doc.save", json!({"allow_lossy": true}));
    let saved = fs::read_to_string(&p).unwrap();
    assert!(saved.contains("Maya makes eggs."), "{saved}");
    assert!(!saved.contains("toast"), "{saved}");
    d.discard();
}

#[test]
fn a_journal_from_before_phase_5a_replays_untracked() {
    let dir = tempfile::tempdir().unwrap();
    let (d, p) = open(dir.path());
    drop(d);
    // A semantics 1 journal holding an agent's edit with no `tracked`.
    let j = journal::path_for(&p);
    let header = fs::read_to_string(&j).unwrap();
    let mut h: Value = serde_json::from_str(header.lines().next().unwrap()).unwrap();
    h["semantics"] = json!(1);
    let entry = json!({
        "seq": 1, "t": 1, "command": "block.insert",
        "params": {"type": "action", "text": "Old agent line."},
        "result": {"id": "b_9"},
        "by": {"kind": "agent", "name": "claude-code"},
    });
    fs::write(&j, format!("{h}\n{entry}\n")).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert!(changes(&mut d).is_empty());
    assert_eq!(block(&mut d, "b_9")["text"], "Old agent line.");
    // A semantics 2 entry without `tracked` cannot be trusted.
    drop(d);
    h["semantics"] = json!(2);
    fs::write(&j, format!("{h}\n{entry}\n")).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"][0]["code"], "journal_partial", "{r}");
    d.discard();
}

// ---- Properties ----------------------------------------------------------

/// One edit, by one of three authors, described by positions.
#[derive(Clone, Debug)]
enum Step {
    Insert { at: usize, off: usize, text: String },
    Delete { at: usize, off: usize, len: usize },
    Replace { word: usize, with: String },
    SetText { at: usize, text: String },
    NewBlock { at: usize, text: String },
    DeleteBlock { at: usize },
    Retype { at: usize, kind: usize },
    Split { at: usize, off: usize },
    Join { at: usize },
    Move { from: usize, to: usize },
}

const WORDS: &[&str] = &["toast", "plants", "lamp", "Maya", "ready"];
const KINDS: &[&str] = &["action", "dialogue", "character", "scene_heading"];

fn step() -> impl Strategy<Value = Step> {
    let text = "[a-z ]{1,8}";
    prop_oneof![
        (0..12usize, 0..30usize, text).prop_map(|(at, off, text)| Step::Insert { at, off, text }),
        (0..12usize, 0..30usize, 1..6usize).prop_map(|(at, off, len)| Step::Delete {
            at,
            off,
            len
        }),
        (0..WORDS.len(), "[a-z]{0,5}").prop_map(|(word, with)| Step::Replace { word, with }),
        (0..12usize, "[a-z .]{0,20}").prop_map(|(at, text)| Step::SetText { at, text }),
        (0..12usize, text).prop_map(|(at, text)| Step::NewBlock { at, text }),
        (0..12usize).prop_map(|at| Step::DeleteBlock { at }),
        (0..12usize, 0..KINDS.len()).prop_map(|(at, kind)| Step::Retype { at, kind }),
        (0..12usize, 0..30usize).prop_map(|(at, off)| Step::Split { at, off }),
        (1..12usize).prop_map(|at| Step::Join { at }),
        (0..4usize, 0..4usize).prop_map(|(from, to)| Step::Move { from, to }),
    ]
}

fn authors() -> [Author; 3] {
    [writer(), agent(), other_agent()]
}

/// Runs a step as tracked; failures (bad offsets, refused edits) are fine.
fn apply(d: &mut Dispatcher, by: &Author, s: &Step) {
    let bs = blocks(d);
    if bs.is_empty() {
        return;
    }
    let id = |at: usize| bs[at % bs.len()]["id"].clone();
    let len = |at: usize| {
        bs[at % bs.len()]["text"]
            .as_str()
            .unwrap_or("")
            .chars()
            .count()
    };
    let (cmd, params) = match s {
        Step::Insert { at, off, text } => (
            "text.insert",
            json!({"id": id(*at), "at": off % (len(*at) + 1), "text": text}),
        ),
        Step::Delete { at, off, len: n } => {
            let l = len(*at);
            let from = off % (l + 1);
            (
                "text.delete",
                json!({"id": id(*at), "from": from, "to": (from + n).min(l)}),
            )
        }
        Step::Replace { word, with } => {
            ("text.replace", json!({"find": WORDS[*word], "with": with}))
        }
        Step::SetText { at, text } => ("block.set_text", json!({"id": id(*at), "text": text})),
        Step::NewBlock { at, text } => (
            "block.insert",
            json!({"after": id(*at), "type": "action", "text": text}),
        ),
        Step::DeleteBlock { at } => ("block.delete", json!({"id": id(*at)})),
        Step::Retype { at, kind } => (
            "block.set_type",
            json!({"id": id(*at), "type": KINDS[*kind]}),
        ),
        Step::Split { at, off } => (
            "block.split",
            json!({"id": id(*at), "at": off % (len(*at) + 1)}),
        ),
        Step::Join { at } => ("block.join", json!({"id": id(*at)})),
        Step::Move { from, to } => {
            let heads: Vec<Value> = bs
                .iter()
                .filter(|b| b["type"] == "scene_heading")
                .map(|b| b["id"].clone())
                .collect();
            if heads.is_empty() {
                return;
            }
            let from = heads[from % heads.len()].clone();
            let to = heads[to % heads.len()].clone();
            ("scene.move", json!({"id": from, "before": to}))
        }
    };
    let mut params = params;
    params["tracked"] = json!(true);
    let _ = run(d, by, cmd, params);
}

/// The blocks' JSON with marks and ids set aside: types, attributes and
/// current text.
fn plain(bs: &[Value]) -> Vec<Value> {
    bs.iter()
        .map(|b| {
            let mut b = b.clone();
            let o = b.as_object_mut().unwrap();
            o.remove("runs");
            o.remove("change");
            o.remove("id");
            b
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(64),
        ..ProptestConfig::default()
    })]

    #[test]
    fn accept_all_keeps_the_current_text_reject_all_gives_back_the_start_and_undo_restores(
        steps in proptest::collection::vec((0..3usize, step()), 1..14)
    ) {
        let mut d = Dispatcher::new(Options { journal: false, fsync: false });
        ok(&mut d, &writer(), "doc.new", json!({}));
        let start = coldopen_fountain::parse(SCRIPT).script;
        for e in start.elements {
            let mut p = serde_json::to_value(&e).unwrap();
            p["tracked"] = json!(false);
            ok(&mut d, &writer(), "block.insert", p);
        }
        let original = plain(&blocks(&mut d));
        let authors = authors();
        for (who, s) in &steps {
            apply(&mut d, &authors[*who], s);
        }
        let marked = blocks(&mut d);
        let current = plain(&marked);

        // Accepting every change leaves the current text, with no marks,
        // exactly as the accepted form has it: joined blocks are joined
        // (where they still can be), and deleted blocks go.
        let n = changes(&mut d).len();
        let form = serde_json::to_value(&d.session().unwrap().doc.accepted().blocks).unwrap();
        let r = run(&mut d, &writer(), "changes.accept", json!({"all": true}));
        if n > 0 {
            prop_assert!(r.is_ok());
        }
        let accepted = blocks(&mut d);
        prop_assert_eq!(&Value::Array(accepted.clone()), &form);
        prop_assert!(changes(&mut d).is_empty());
        prop_assert!(!accepted.iter().any(|b| b.get("change").is_some()));
        let expected: Vec<Value> = current
            .iter()
            .zip(&marked)
            .filter(|(_, m)| m["change"]["deleted"].is_null())
            .map(|(c, _)| c.clone())
            .collect();
        let no_joins = !marked.iter().any(|b| !b["change"]["joined"].is_null());
        if no_joins {
            prop_assert_eq!(plain(&accepted), expected);
        }
        let joined_text: String = accepted.iter().map(|b| b["text"].as_str().unwrap_or("").to_string()).collect();
        let current_text: String = current.iter().map(|b| b["text"].as_str().unwrap_or("").to_string()).collect();
        prop_assert_eq!(joined_text, current_text);

        // Undo gives every mark back.
        if n > 0 {
            ok(&mut d, &writer(), "undo", json!({}));
        }
        prop_assert_eq!(blocks(&mut d), marked.clone());

        // Rejecting every change gives back the start, apart from moves
        // whose place was lost.
        let r = run(&mut d, &writer(), "changes.reject", json!({"all": true}));
        let skipped = r.map(|r| !r["skipped"].as_array().unwrap().is_empty()).unwrap_or(false);
        if !skipped {
            prop_assert!(changes(&mut d).is_empty());
            prop_assert_eq!(plain(&blocks(&mut d)), original);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(32),
        ..ProptestConfig::default()
    })]

    /// Tracked edits, saves and settling, then a crash: recovery gives back
    /// every mark and every change, with its author and time.
    #[test]
    fn tracked_changes_survive_any_crash(
        steps in proptest::collection::vec((0..4usize, step()), 1..12)
    ) {
        let dir = tempfile::tempdir().unwrap();
        let (mut d, _) = open(dir.path());
        // A bundle, which keeps everything, so saves come into it too.
        let p = dir.path().join("s.coldopen");
        ok(&mut d, &writer(), "doc.save", json!({"path": p}));
        let authors = authors();
        for (k, (who, s)) in steps.iter().enumerate() {
            match who {
                3 if k % 2 == 0 => {
                    let _ = run(&mut d, &writer(), "changes.reject", json!({"id": "c_1"}));
                }
                3 => {
                    ok(&mut d, &writer(), "doc.save", json!({}));
                }
                w => apply(&mut d, &authors[*w], s),
            }
        }
        let before = (blocks(&mut d), changes(&mut d));
        drop(d); // a crash
        let mut d = Dispatcher::new(Options::default());
        ok(&mut d, &writer(), "doc.open", json!({"path": p}));
        prop_assert_eq!((blocks(&mut d), changes(&mut d)), before);
        d.discard();
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(256),
        ..ProptestConfig::default()
    })]

    /// The `changes` fuzz target's check, on stable.
    #[test]
    fn fuzz_changes_check(data in proptest::collection::vec(any::<u8>(), 0..160)) {
        prop_assert_eq!(coldopen_core::fuzzing::changes(&data), Ok(()));
    }

    /// The `bundle_read` fuzz target's check, on stable: arbitrary bytes,
    /// and real bundles cut short or with a byte changed.
    #[test]
    fn fuzz_bundle_check(
        data in proptest::collection::vec(any::<u8>(), 0..300),
        cut in any::<usize>(),
        flip in any::<usize>(),
        byte in any::<u8>(),
    ) {
        prop_assert_eq!(coldopen_core::fuzzing::bundle_read(&data), Ok(()));
        let dir = tempfile::tempdir().unwrap();
        let (mut d, _) = open(dir.path());
        ok(&mut d, &agent(), "text.replace", json!({"find": "toast", "with": "eggs"}));
        let p = dir.path().join("s.coldopen");
        ok(&mut d, &writer(), "doc.save", json!({"path": p}));
        d.shutdown();
        let mut bytes = fs::read(&p).unwrap();
        prop_assert_eq!(coldopen_core::fuzzing::bundle_read(&bytes), Ok(()));
        let i = flip % bytes.len();
        bytes[i] = byte;
        prop_assert_eq!(coldopen_core::fuzzing::bundle_read(&bytes), Ok(()));
        bytes.truncate(cut % (bytes.len() + 1));
        prop_assert_eq!(coldopen_core::fuzzing::bundle_read(&bytes), Ok(()));
    }
}
