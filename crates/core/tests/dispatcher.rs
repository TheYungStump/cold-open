//! The dispatcher through its JSON envelope, as every front end uses it.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Dispatcher, Options, journal};
use serde_json::{Value, json};

struct Harness {
    d: Dispatcher,
    next: u64,
}

impl Harness {
    fn new() -> Harness {
        Harness {
            d: Dispatcher::new(Options::default()),
            next: 0,
        }
    }

    fn raw(&mut self, command: &str, params: Value) -> Value {
        self.next += 1;
        self.d
            .handle(&json!({ "v": 1, "id": self.next, "command": command, "params": params }))
    }

    /// Runs a command that must succeed and returns its result.
    fn ok(&mut self, command: &str, params: Value) -> Value {
        let r = self.raw(command, params);
        assert_eq!(r["ok"], true, "{command} failed: {r}");
        assert_eq!(r["id"], self.next);
        r["result"].clone()
    }

    /// Runs a command that must fail and returns its error code.
    fn err(&mut self, command: &str, params: Value) -> String {
        let r = self.raw(command, params);
        assert_eq!(r["ok"], false, "{command} should fail: {r}");
        r["error"]["code"].as_str().unwrap().to_string()
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

    fn blocks(&mut self) -> Value {
        self.ok("doc.get", json!({}))["blocks"].clone()
    }
}

const SCRIPT: &str = "\
Title: Test

INT. KITCHEN - DAY

Maya makes toast.

MAYA
Breakfast!

# Act Two

EXT. GARDEN - DAY

Theo waters plants.
";

fn write_script(dir: &Path) -> PathBuf {
    let p = dir.join("test.fountain");
    fs::write(&p, SCRIPT).unwrap();
    p
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
fn envelope_errors() {
    let mut d = Dispatcher::new(Options::default());
    let r: Value = serde_json::from_str(&d.handle_line("not json")).unwrap();
    assert_eq!(r["error"]["code"], "invalid_request");
    assert_eq!(r["id"], Value::Null);
    let r = d.handle(&json!({"v": 2, "id": "a", "command": "undo"}));
    assert_eq!(
        (r["id"].as_str(), r["error"]["code"].as_str()),
        (Some("a"), Some("unsupported_version"))
    );
    let r = d.handle(&json!({"id": 1, "command": "block.explode"}));
    assert_eq!(r["error"]["code"], "unknown_command");
    let r = d.handle(&json!({"id": 1, "command": "undo", "extra": true}));
    assert_eq!(r["error"]["code"], "invalid_request");
    let r = d.handle(&json!({"id": 1, "command": "block.delete", "params": {"idd": "b_1"}}));
    assert_eq!(r["error"]["code"], "invalid_params");
    let r = d.handle(&json!({"id": 1, "command": "undo"}));
    assert_eq!(r["error"]["code"], "no_document");
    // v and params are optional.
    let r = d.handle(&json!({"command": "doc.new"}));
    assert_eq!(r["ok"], true);
    assert_eq!(r["v"], 1);
}

#[test]
fn edits_undo_and_redo() {
    let mut h = Harness::new();
    h.ok("doc.new", json!({}));
    let a = h.ok(
        "block.insert",
        json!({"type": "scene_heading", "text": "INT. LAB - NIGHT"}),
    )["id"]
        .clone();
    assert_eq!(a, "b_1");
    let b = h.ok(
        "block.insert",
        json!({"type": "action", "text": "Bubbles.", "after": a}),
    )["id"]
        .clone();
    let c = h.ok(
        "block.insert",
        json!({"type": "character", "text": "MAYA", "before": b}),
    )["id"]
        .clone();
    assert_eq!(
        h.texts(),
        [
            "scene_heading:INT. LAB - NIGHT",
            "character:MAYA",
            "action:Bubbles."
        ]
    );

    h.ok("block.set_type", json!({"id": c, "type": "action"}));
    h.ok(
        "block.set_text",
        json!({"id": c, "runs": [{"text": "Maya ", }, {"text": "runs", "bold": true}]}),
    );
    assert_eq!(
        h.blocks()[1]["runs"][1],
        json!({"text": "runs", "bold": true})
    );
    h.ok("block.delete", json!({"id": b}));
    assert_eq!(
        h.texts(),
        ["scene_heading:INT. LAB - NIGHT", "action:Maya runs"]
    );

    for _ in 0..3 {
        h.ok("undo", json!({}));
    }
    assert_eq!(
        h.texts(),
        [
            "scene_heading:INT. LAB - NIGHT",
            "character:MAYA",
            "action:Bubbles."
        ]
    );
    // Undoing a delete brings back the same id.
    assert_eq!(h.blocks()[2]["id"], b);
    for _ in 0..3 {
        h.ok("redo", json!({}));
    }
    assert_eq!(
        h.texts(),
        ["scene_heading:INT. LAB - NIGHT", "action:Maya runs"]
    );
    assert_eq!(h.err("redo", json!({})), "nothing_to_redo");

    // A new edit clears redo.
    h.ok("undo", json!({}));
    h.ok(
        "block.insert",
        json!({"type": "transition", "text": "CUT TO:"}),
    );
    assert_eq!(h.err("redo", json!({})), "nothing_to_redo");
    // Ids are never reused, even after undo.
    let undone = h.ok("undo", json!({}));
    assert_eq!(undone["undone"], "block.insert");
    let d = h.ok("block.insert", json!({"type": "action", "text": "x"}))["id"].clone();
    assert_eq!(d, "b_5");
}

#[test]
fn set_type_clears_attributes_and_undo_restores_them() {
    let mut h = Harness::new();
    h.ok("doc.new", json!({}));
    let id = h.ok(
        "block.insert",
        json!({"type": "scene_heading", "text": "INT. X", "scene_number": "4A"}),
    )["id"]
        .clone();
    h.ok("block.set_type", json!({"id": id, "type": "action"}));
    assert_eq!(h.blocks()[0].get("scene_number"), None);
    h.ok("undo", json!({}));
    assert_eq!(h.blocks()[0]["scene_number"], "4A");
}

#[test]
fn model_rules_are_enforced() {
    let mut h = Harness::new();
    h.ok("doc.new", json!({}));
    assert_eq!(
        h.err("block.insert", json!({"type": "action", "dual": true})),
        "invalid_attr"
    );
    let id = h.ok("block.insert", json!({"type": "character", "text": "MAYA"}))["id"].clone();
    assert_eq!(
        h.err("block.set_attrs", json!({"id": id, "scene_number": "1"})),
        "invalid_attr"
    );
    h.ok(
        "block.set_attrs",
        json!({"id": id, "dual": true, "space_before": 2}),
    );
    assert_eq!(h.blocks()[0]["dual"], true);
    h.ok("block.set_attrs", json!({"id": id, "space_before": null}));
    assert_eq!(h.blocks()[0].get("space_before"), None);
    assert_eq!(
        h.err("block.delete", json!({"id": "b_99"})),
        "block_not_found"
    );
    assert_eq!(
        h.err(
            "block.insert",
            json!({"type": "action", "after": "b_1", "before": "b_1"})
        ),
        "invalid_params"
    );
    assert_eq!(h.err("block.set_text", json!({"id": id})), "invalid_params");
    assert_eq!(h.err("undo", json!({"x": 1})), "invalid_params");
}

#[test]
fn text_replace_is_one_undo_step_and_respects_scope() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.ok("block.insert", json!({"type": "action", "runs": [{"text": "MA", "bold": true}, {"note": "n"}, {"text": "YA waits."}]}));
    let r = h.ok(
        "text.replace",
        json!({"find": "maya", "with": "Nina", "match_case": false}),
    );
    assert_eq!(r["count"], 3);
    let blocks = h.blocks();
    assert_eq!(blocks[1]["text"], "Nina makes toast.");
    let last = blocks.as_array().unwrap().last().unwrap();
    assert_eq!(
        last["runs"],
        json!([{"text": "Nina", "bold": true}, {"note": "n"}, {"text": " waits."}])
    );
    h.ok("undo", json!({}));
    assert_eq!(h.blocks()[1]["text"], "Maya makes toast.");

    let garden = h.blocks()[5]["id"].clone();
    let r = h.ok(
        "text.replace",
        json!({"find": "a", "with": "4", "scope": {"scene": garden}}),
    );
    assert_eq!(
        r["blocks"].as_array().unwrap().len(),
        2,
        "the garden action and the appended one"
    );
    assert_eq!(h.blocks()[1]["text"], "Maya makes toast.");
    // No match: nothing changes and nothing is added to history.
    let r = h.ok("text.replace", json!({"find": "zebra", "with": "x"}));
    assert_eq!(r["count"], 0);
    h.ok("undo", json!({}));
    assert_eq!(h.blocks()[6]["text"], "Theo waters plants.");
    assert_eq!(
        h.err("text.replace", json!({"find": "", "with": "x"})),
        "invalid_params"
    );
}

#[test]
fn scenes_list_and_move_around_sections() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    let scenes = h.ok("scene.list", json!({}))["scenes"].clone();
    assert_eq!(scenes.as_array().unwrap().len(), 2);
    assert_eq!(scenes[0]["heading"], "INT. KITCHEN - DAY");
    assert_eq!(scenes[0]["blocks"], 4);
    assert_eq!(scenes[0]["words"], 9);
    let kitchen = scenes[0]["id"].clone();
    let garden = scenes[1]["id"].clone();
    let section = h.blocks()[4]["id"].clone();

    // Move the kitchen scene to the end: the section stays put.
    h.ok("scene.move", json!({"id": kitchen}));
    let kinds: Vec<String> = h.texts();
    assert_eq!(kinds[0], "section:Act Two");
    assert_eq!(kinds[1], "scene_heading:EXT. GARDEN - DAY");
    assert_eq!(kinds[3], "scene_heading:INT. KITCHEN - DAY");
    h.ok("undo", json!({}));
    assert_eq!(h.texts()[0], "scene_heading:INT. KITCHEN - DAY");

    // Move the garden before the section, then before the kitchen.
    h.ok("scene.move", json!({"id": garden, "before": section}));
    assert_eq!(h.texts()[4], "scene_heading:EXT. GARDEN - DAY");
    h.ok("scene.move", json!({"id": garden, "before": kitchen}));
    assert_eq!(h.texts()[0], "scene_heading:EXT. GARDEN - DAY");

    let action = h.blocks()[1]["id"].clone();
    assert_eq!(
        h.err("scene.move", json!({"id": action})),
        "scene_not_found"
    );
    assert_eq!(
        h.err("scene.move", json!({"id": garden, "before": action})),
        "scene_not_found"
    );
}

#[test]
fn open_save_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    let r = h.ok("doc.open", json!({"path": p}));
    assert_eq!(
        (r["blocks"].as_u64(), r["scenes"].as_u64()),
        (Some(7), Some(2))
    );
    assert!(journal::path_for(&p).exists());
    h.ok(
        "block.insert",
        json!({"type": "action", "text": "The end."}),
    );
    let r = h.ok("doc.save", json!({}));
    assert_eq!(r["losses"], json!([]));
    assert!(
        fs::read_to_string(&p)
            .unwrap()
            .ends_with("Theo waters plants.\n\nThe end.\n")
    );
    h.ok("doc.close", json!({}));
    assert!(
        !journal::path_for(&p).exists(),
        "a clean close removes the journal"
    );

    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(r["blocks"], 8);
    assert_eq!(r["recovered"], Value::Null);
}

#[test]
fn saving_refuses_losses_unless_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("new.fountain");
    let mut h = Harness::new();
    h.ok("doc.new", json!({"path": p}));
    let shot = h.ok(
        "block.insert",
        json!({"type": "shot", "text": "CLOSE ON MAYA"}),
    )["id"]
        .clone();
    let r = h.raw("doc.save", json!({}));
    assert_eq!(r["error"]["code"], "lossy_format");
    assert_eq!(r["error"]["details"]["losses"][0]["id"], shot);
    assert!(!p.exists(), "nothing is written when a save is refused");
    let r = h.ok("doc.save", json!({"allow_lossy": true}));
    assert_eq!(r["losses"][0]["code"], "shot_element");
    assert!(p.exists());
}

#[test]
fn session_rules() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    assert_eq!(h.err("doc.new", json!({"path": p})), "already_exists");
    h.ok("doc.open", json!({"path": p}));
    h.ok("block.insert", json!({"type": "action", "text": "x"}));
    assert_eq!(h.err("doc.open", json!({"path": p})), "unsaved_changes");
    assert_eq!(h.err("doc.close", json!({})), "unsaved_changes");
    assert_eq!(
        h.err("doc.save", json!({"path": dir.path().join("x.pdf")})),
        "unsupported_format"
    );
    assert_eq!(
        h.err(
            "doc.open",
            json!({"path": dir.path().join("missing.fountain")})
        ),
        "unsaved_changes"
    );

    // A second process (here, a second dispatcher) is locked out.
    let mut other = Harness::new();
    assert_eq!(other.err("doc.open", json!({"path": p})), "doc_locked");

    h.ok("doc.close", json!({"discard": true}));
    assert!(!journal::path_for(&p).exists());
    assert_eq!(
        h.err(
            "doc.open",
            json!({"path": dir.path().join("missing.fountain")})
        ),
        "io_error"
    );
    let mut untitled = Harness::new();
    assert_eq!(
        untitled.ok("doc.new", json!({}))["warnings"][0]["code"],
        "unjournaled"
    );
    assert_eq!(untitled.err("doc.save", json!({})), "no_path");
}

#[test]
fn save_as_moves_the_journal() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let q = dir.path().join("copy.fountain");
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.ok("block.insert", json!({"type": "action", "text": "x"}));
    h.ok("doc.save", json!({"path": q}));
    assert!(!journal::path_for(&p).exists());
    assert!(journal::path_for(&q).exists());
    assert_eq!(h.ok("doc.get", json!({}))["path"], q.display().to_string());
}

/// Dropping a dispatcher without shutting down is what a crash looks like
/// to the files: the lock is released and the journal stays.
fn crash(h: Harness) {
    drop(h);
}

#[test]
fn recovery_after_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.ok("block.insert", json!({"type": "action", "text": "One."}));
    h.ok("block.insert", json!({"type": "action", "text": "Two."}));
    h.ok("undo", json!({}));
    h.ok("text.replace", json!({"find": "Maya", "with": "Nina"}));
    let before = h.blocks();
    crash(h);

    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(
        recovered(&r),
        json!({"commands": 4, "external_change": false})
    );
    assert_eq!(h2.blocks(), before);
    assert_eq!(h2.ok("doc.get", json!({}))["dirty"], true);
    // Undo history survives recovery.
    h2.ok("undo", json!({}));
    assert_eq!(h2.blocks()[1]["text"], "Maya makes toast.");
    // Further edits keep journaling, and a second crash recovers them too.
    h2.ok("block.insert", json!({"type": "action", "text": "Three."}));
    let before = h2.blocks();
    crash(h2);
    let mut h3 = Harness::new();
    let r = h3.ok("doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 6);
    assert_eq!(h3.blocks(), before);
}

#[test]
fn a_torn_last_entry_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.ok("block.insert", json!({"type": "action", "text": "Kept."}));
    crash(h);
    let j = journal::path_for(&p);
    let mut bytes = fs::read(&j).unwrap();
    bytes.extend_from_slice(br#"{"seq":2,"t":1,"command":"block.ins"#);
    fs::write(&j, &bytes).unwrap();

    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1);
    assert_eq!(r["warnings"], json!([]));
    h2.ok("block.insert", json!({"type": "action", "text": "Next."}));
    crash(h2);
    let mut h3 = Harness::new();
    assert_eq!(
        h3.ok("doc.open", json!({"path": p}))["recovered"]["commands"],
        2
    );
}

#[test]
fn discard_and_external_change() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.ok(
        "block.insert",
        json!({"type": "action", "text": "Unsaved."}),
    );
    crash(h);

    // Another program changes the file: recovery still works and says so.
    fs::write(&p, format!("{SCRIPT}\nAdded elsewhere.\n")).unwrap();
    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(
        recovered(&r),
        json!({"commands": 1, "external_change": true})
    );
    crash(h2);

    // Discarding sets the journal aside instead of deleting it.
    let mut h3 = Harness::new();
    let r = h3.ok("doc.open", json!({"path": p, "recover": "discard"}));
    assert_eq!(r["recovered"], Value::Null);
    assert_eq!(r["warnings"][0]["code"], "journal_discarded");
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
}

#[test]
fn edits_that_cancel_out_need_no_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.ok("block.insert", json!({"type": "action", "text": "x"}));
    h.ok("undo", json!({}));
    crash(h);
    let mut h2 = Harness::new();
    assert_eq!(
        h2.ok("doc.open", json!({"path": p}))["recovered"],
        Value::Null
    );
}

#[test]
fn an_unreadable_journal_is_kept_aside() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    fs::write(journal::path_for(&p), "garbage\n").unwrap();
    let mut h = Harness::new();
    let r = h.ok("doc.open", json!({"path": p}));
    assert_eq!(r["warnings"][0]["code"], "journal_unreadable");
    assert_eq!(r["blocks"], 7);
}

#[test]
fn crlf_and_bom_are_kept_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("win.fountain");
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(SCRIPT.replace('\n', "\r\n").as_bytes());
    fs::write(&p, &bytes).unwrap();
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.ok("doc.save", json!({}));
    assert_eq!(fs::read(&p).unwrap(), bytes);
}

/// Found by the replay property test: a change that Fountain writes back
/// as the original text must still be recovered.
#[test]
fn a_change_that_writes_as_the_old_text_is_still_recovered() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    let garden = h.blocks()[5]["id"].clone();
    h.ok(
        "block.set_type",
        json!({"id": garden, "type": "parenthetical"}),
    );
    crash(h);
    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1);
    assert_eq!(h2.blocks()[5]["type"], "parenthetical");
}

#[test]
fn export_pdf_writes_without_touching_the_document() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    // The journal is append-only, so an unchanged length means nothing was
    // journaled. Its contents can't be read here: the open document holds a
    // lock on it, which Windows enforces against readers.
    let jpath = journal::path_for(&p);
    let journal_len = fs::metadata(&jpath).unwrap().len();
    let out = dir.path().join("out.pdf");
    let r = h.ok("export.pdf", json!({"path": out}));
    assert_eq!(r["pages"], 2, "title page and one body page: {r}");
    let bytes = fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.7"));
    assert_eq!(r["bytes"], bytes.len());
    assert_eq!(r["warnings"], json!([]));
    // Not an edit: nothing dirty, nothing journaled, nothing to undo.
    assert_eq!(h.ok("doc.get", json!({}))["dirty"], false);
    assert_eq!(fs::metadata(&jpath).unwrap().len(), journal_len);
    assert_eq!(h.err("undo", json!({})), "nothing_to_undo");
    // The same document gives the same bytes; options change them.
    let again = dir.path().join("again.pdf");
    h.ok("export.pdf", json!({"path": again}));
    assert_eq!(fs::read(&again).unwrap(), bytes);
    let r = h.ok(
        "export.pdf",
        json!({"path": again, "watermark": "DRAFT", "scene_numbers": true, "title_page": false, "overwrite": true}),
    );
    assert_eq!(r["pages"], 1);
    assert_ne!(fs::read(&again).unwrap(), bytes);
    // No temp files are left beside the output.
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| !n.contains("coldopen-save")),
        "{names:?}"
    );
}

#[test]
fn export_pdf_refuses_risky_targets() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    assert_eq!(h.err("export.pdf", json!({"path": "x.pdf"})), "no_document");
    h.ok("doc.open", json!({"path": p}));
    let original = fs::read(&p).unwrap();
    assert_eq!(
        h.err("export.pdf", json!({"path": dir.path().join("x.txt")})),
        "unsupported_format"
    );
    // The open script and its journal are never .pdf paths, so asking for
    // either is refused before anything is written.
    assert_eq!(
        h.err("export.pdf", json!({"path": p})),
        "unsupported_format"
    );
    let jpath = journal::path_for(&p);
    assert_eq!(
        h.err("export.pdf", json!({"path": jpath})),
        "unsupported_format"
    );
    assert_eq!(fs::read(&p).unwrap(), original);
    assert!(jpath.exists());
    assert_eq!(
        h.err(
            "export.pdf",
            json!({"path": dir.path().join("out.pdf"), "colour": true})
        ),
        "invalid_params"
    );
}

#[test]
fn page_view_and_scene_pages() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    let pages = h.ok("doc.export", json!({"format": "pages"}));
    let text = pages["text"].as_str().unwrap();
    assert!(text.starts_with("=== page 1 · title ===\n"), "{text}");
    assert!(text.contains("=== page 2 · body 1 · end ==="), "{text}");
    assert!(
        text.contains("               INT. KITCHEN - DAY\n"),
        "{text}"
    );
    let scenes = h.ok("scene.list", json!({}))["scenes"].clone();
    assert_eq!(scenes[0]["page"], 1);
    assert_eq!(scenes[1]["page"], 1);
    assert!(scenes[0]["eighths"].as_u64().unwrap() >= 1);

    // Push the second scene onto page 2 and check the pages follow.
    let first = scenes[0]["id"].as_str().unwrap().to_string();
    for i in 0..60 {
        h.ok(
            "block.insert",
            json!({"after": first, "type": "action", "text": format!("Line {i}.")}),
        );
    }
    let scenes = h.ok("scene.list", json!({}))["scenes"].clone();
    // 60 one-line actions, each with a blank line before it, fill pages 1
    // and 2 (27 rows of content per 54-line page), so the next scene starts
    // on page 3.
    assert_eq!(scenes[0]["page"], 1);
    assert_eq!(scenes[1]["page"], 3);
    let long = scenes[0]["eighths"].as_u64().unwrap();
    assert!(
        long > 8,
        "a scene over a page long is more than 8/8: {long}"
    );
    // The pages are the same after a save and reload.
    let before = h.ok("doc.export", json!({"format": "pages"}))["text"].clone();
    h.ok("doc.save", json!({}));
    h.ok("doc.close", json!({}));
    h.ok("doc.open", json!({"path": p}));
    assert_eq!(
        h.ok("doc.export", json!({"format": "pages"}))["text"],
        before
    );
}

#[test]
fn replay_stops_at_entries_that_are_not_edits() {
    let dir = tempfile::tempdir().unwrap();
    let p = write_script(dir.path());
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    h.ok("block.insert", json!({"type": "action", "text": "Kept."}));
    crash(h);
    // Append a hand-made export entry, then another edit.
    let jpath = journal::path_for(&p);
    let mut j = fs::read_to_string(&jpath).unwrap();
    let pdf = dir.path().join("sneaky.pdf");
    j.push_str(&format!(
        "{}\n",
        json!({"seq": 2, "t": 0, "command": "export.pdf", "params": {"path": pdf}, "result": {}})
    ));
    j.push_str(&format!(
        "{}\n",
        json!({"seq": 3, "t": 0, "command": "block.insert", "params": {"type": "action", "text": "Lost."}, "result": {"id": "b_99"}})
    ));
    fs::write(&jpath, j).unwrap();
    let mut h2 = Harness::new();
    let r = h2.ok("doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1);
    assert!(!pdf.exists(), "replay must never write files");
    let texts = h2.texts();
    assert!(texts.iter().any(|t| t == "action:Kept."));
    assert!(!texts.iter().any(|t| t == "action:Lost."));
}

/// A hand-made journal for a script that was never saved: a header with
/// one block and `next_id`, then one insert recorded with `id`.
fn hand_made_journal(dir: &Path, blocks: &str, next_id: &str, id: &str) -> PathBuf {
    let p = dir.join("never-saved.fountain");
    let header = format!(
        r#"{{"journal":"coldopen","v":1,"app":"0.1.0","doc":{{"title_page":{{"fields":[]}},"blocks":{blocks},"next_id":{next_id}}},"disk":null}}"#
    );
    let entry = format!(
        r#"{{"seq":1,"t":1,"command":"block.insert","params":{{"type":"action","text":"Two."}},"result":{{"id":"{id}"}}}}"#
    );
    fs::write(journal::path_for(&p), format!("{header}\n{entry}\n")).unwrap();
    p
}

/// Review finding C7: a journal header was accepted without checking its
/// ids. One whose `next_id` was already in use gave two blocks with the
/// same id; one with the largest `next_id` panicked on the next insert.
#[test]
fn a_journal_header_with_bad_ids_is_set_aside_not_trusted() {
    for (blocks, next_id, id) in [
        (
            r#"[{"id":"b_1","type":"action","text":"One."}]"#,
            "1",
            "b_1",
        ),
        (
            r#"[{"id":"b_1","type":"action","text":"One."}]"#,
            "18446744073709551615",
            "b_18446744073709551615",
        ),
        (
            r#"[{"id":"b_2","type":"action","text":"One."},{"id":"b_2","type":"action","text":"Again."}]"#,
            "3",
            "b_3",
        ),
        (
            r#"[{"id":"b_1","type":"page_break","text":"Words."}]"#,
            "2",
            "b_2",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let p = hand_made_journal(dir.path(), blocks, next_id, id);
        let mut h = Harness::new();
        let r = h.ok("doc.open", json!({"path": p}));
        assert_eq!(r["warnings"][0]["code"], "journal_unreadable", "{r}");
        let ids: Vec<Value> = h
            .blocks()
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["id"].clone())
            .collect();
        let mut unique = ids.clone();
        unique.dedup();
        assert_eq!(ids, unique, "{ids:?}");
        h.d.discard();
    }
}

/// Review finding F12 (and Antigravity's 9): `doc.revert` numbered the
/// file's blocks from `b_1` again, inside the same open document, so an
/// agent's remembered id could name a different block afterwards.
#[test]
fn revert_never_hands_out_an_id_again() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("r.fountain");
    fs::write(&p, "INT. HOUSE - DAY\n\nRain.\n\nWind.\n").unwrap();
    let mut h = Harness::new();
    h.ok("doc.open", json!({"path": p}));
    // The reviewer's steps: a line at the top, saved, then revert.
    h.ok(
        "block.insert",
        json!({"before": "b_1", "type": "action", "text": "A new opening line."}),
    );
    h.ok("doc.save", json!({}));
    h.ok("doc.revert", json!({}));
    // An agent that learned b_3 = "Wind." must not delete "Rain." now.
    let r = h.raw("block.delete", json!({"id": "b_3"}));
    assert_eq!(r["error"]["code"], "block_not_found", "{r}");
    let texts = h.texts();
    assert!(texts.iter().any(|t| t.ends_with("Rain.")), "{texts:?}");

    // Antigravity's steps: an id allocated and deleted before a save is
    // never handed out again after a revert.
    let mut seen: Vec<String> = h
        .blocks()
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_str().unwrap().to_string())
        .collect();
    seen.extend(["b_1", "b_2", "b_3", "b_4", "b_5"].map(String::from));
    let id = h.ok("block.insert", json!({"type": "action", "text": "Later."}))["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!seen.contains(&id), "{id} was handed out before");
    h.d.discard();
}
