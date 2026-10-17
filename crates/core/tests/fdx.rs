//! .fdx through the dispatcher (docs/specs/commands.md, docs/specs/fdx.md,
//! docs/specs/journal.md): open, edit, save and reopen with nothing lost;
//! crash recovery; conversion both ways; exports; repair; damaged
//! originals.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Author, Command, Dispatcher, Options};
use serde_json::{Value, json};

fn fixture(name: &str) -> Vec<u8> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fdx")
        .join(name);
    fs::read(p).unwrap()
}

fn copy(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, fixture(name)).unwrap();
    p
}

fn run(d: &mut Dispatcher, command: &str, params: Value) -> Result<Value, (String, Value)> {
    d.execute(Command::from_parts(command, params).unwrap())
        .map_err(|e| (e.code.to_string(), serde_json::to_value(&e).unwrap()))
}

fn ok(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    run(d, command, params).unwrap_or_else(|e| panic!("{command}: {e:?}"))
}

fn blocks(d: &mut Dispatcher) -> Vec<Value> {
    ok(d, "doc.get", json!({}))["blocks"]
        .as_array()
        .unwrap()
        .clone()
}

fn first(d: &mut Dispatcher, kind: &str) -> String {
    blocks(d).into_iter().find(|b| b["type"] == kind).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn asides(dir: &Path, part: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().contains(part))
        .collect();
    v.sort();
    v
}

#[test]
fn an_fdx_opens_edits_saves_and_reopens_with_nothing_lost() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(dir.path(), "scene-numbers.fdx");
    let original = fs::read(&p).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": p}));
    assert_eq!(r["format"], "fdx");
    assert_eq!(r["repaired"], Value::Null);
    assert_eq!(ok(&mut d, "doc.get", json!({}))["format"], "fdx");

    // Saved unedited, the file is byte for byte as it was.
    let s = ok(&mut d, "doc.save", json!({}));
    assert_eq!(s["format"], "fdx");
    assert_eq!(fs::read(&p).unwrap(), original);

    // An edit changes only its paragraph; the scene properties stay.
    let id = first(&mut d, "action");
    ok(
        &mut d,
        "block.set_text",
        json!({"id": id, "text": "Changed in Cold Open."}),
    );
    ok(&mut d, "doc.save", json!({}));
    let saved = String::from_utf8(fs::read(&p).unwrap()).unwrap();
    assert!(saved.contains("Changed in Cold Open."), "{saved}");
    assert!(saved.contains("<SceneProperties"), "{saved}");
    ok(&mut d, "doc.close", json!({}));

    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let texts: Vec<String> = blocks(&mut d)
        .iter()
        .map(|b| b["text"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(
        texts.iter().any(|t| t == "Changed in Cold Open."),
        "{texts:?}"
    );
}

/// A crash keeps the passthrough store in the journal's header, so the
/// recovered script still writes back everything the file held.
#[test]
fn a_crash_keeps_what_the_file_held_for_other_apps() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(dir.path(), "scene-numbers.fdx");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let id = first(&mut d, "action");
    ok(
        &mut d,
        "block.set_text",
        json!({"id": id, "text": "Typed before the crash."}),
    );
    drop(d); // a crash

    let header = fs::read_to_string(coldopen_core::journal::path_for(&p)).unwrap();
    let header = header.lines().next().unwrap();
    assert!(header.contains("\"format\":\"fdx\""), "{header}");
    assert!(header.contains("\"fdx\":"), "the store is in the header");

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    ok(&mut d, "doc.save", json!({}));
    let saved = String::from_utf8(fs::read(&p).unwrap()).unwrap();
    assert!(saved.contains("Typed before the crash."));
    assert!(saved.contains("<SceneProperties"), "{saved}");
}

#[test]
fn opening_unedited_with_a_journal_is_not_a_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(dir.path(), "dual-dialogue.fdx");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let id = first(&mut d, "action");
    ok(&mut d, "block.set_text", json!({"id": id, "text": "x"}));
    ok(&mut d, "undo", json!({}));
    drop(d);
    // The document equals the file again: written as .fdx it is the
    // file's bytes, so there is nothing to recover.
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"], Value::Null, "{r}");
}

#[test]
fn save_as_converts_and_lists_what_is_lost_first() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(dir.path(), "settings.fdx");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let out = dir.path().join("as.fountain");
    let (code, e) = run(&mut d, "doc.save", json!({"path": out})).unwrap_err();
    assert_eq!(code, "lossy_format");
    let codes: Vec<&str> = e["details"]["losses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"page_format"), "{codes:?}");
    assert!(codes.contains(&"fdx_data"), "{codes:?}");
    assert!(
        !out.exists(),
        "nothing is written before the writer allows it"
    );
    let r = ok(
        &mut d,
        "doc.save",
        json!({"path": out, "allow_lossy": true}),
    );
    assert_eq!(r["format"], "fountain");
    assert_eq!(ok(&mut d, "doc.get", json!({}))["format"], "fountain");

    // And back: a Fountain script saved as .fdx gets a file of our own.
    let back = dir.path().join("back.fdx");
    let r = ok(&mut d, "doc.save", json!({"path": back}));
    assert_eq!(r["losses"], json!([]), "{r}");
    let text = fs::read_to_string(&back).unwrap();
    assert!(
        text.contains("Version=\"1\"") && text.contains("Courier Prime"),
        "{text}"
    );
    // The explicit format must agree with the extension.
    let (code, _) = run(
        &mut d,
        "doc.save",
        json!({"path": dir.path().join("x.fdx"), "format": "fountain"}),
    )
    .unwrap_err();
    assert_eq!(code, "unsupported_format");
}

#[test]
fn a_fountain_script_with_notes_lists_them_as_fdx_losses() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(
        &p,
        "Title: The Lighthouse\nAuthor: A. Writer\n\nINT. LAB - NIGHT\n\n[[a note]]\n\nRain.\n",
    )
    .unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let out = dir.path().join("s.fdx");
    let (code, e) = run(&mut d, "doc.save", json!({"path": out})).unwrap_err();
    assert_eq!(code, "lossy_format");
    let codes: Vec<&str> = e["details"]["losses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["title_page_keys", "note_element"], "{e}");
}

#[test]
fn exports_write_a_copy_and_leave_the_script_alone() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, "INT. LAB - NIGHT\n\nRain.\n").unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let out = dir.path().join("copy.fdx");
    let r = ok(&mut d, "export.fdx", json!({"path": out}));
    assert_eq!(r["format"], "fdx");
    let get = ok(&mut d, "doc.get", json!({}));
    assert_eq!(get["format"], "fountain");
    assert_eq!(get["path"], p.display().to_string());
    assert_eq!(get["dirty"], false);
    // A copy that would replace a file needs overwrite, which only the
    // writer may pass.
    let (code, _) = run(&mut d, "export.fdx", json!({"path": out})).unwrap_err();
    assert_eq!(code, "already_exists");
    let agent = Author::agent("claude-code", None);
    let cmd = Command::from_parts("export.fdx", json!({"path": out, "overwrite": true})).unwrap();
    assert_eq!(d.execute_as(cmd, &agent).unwrap_err().code, "writer_only");
    // Wrong extension, or the open script itself.
    let (code, _) = run(
        &mut d,
        "export.fdx",
        json!({"path": dir.path().join("x.pdf")}),
    )
    .unwrap_err();
    assert_eq!(code, "unsupported_format");
    let (code, _) = run(
        &mut d,
        "export.fountain",
        json!({"path": p, "overwrite": true}),
    )
    .unwrap_err();
    assert_eq!(code, "would_overwrite_document");

    // The copy reads back as the same script.
    let mut e = Dispatcher::new(Options {
        journal: false,
        fsync: false,
    });
    ok(&mut e, "doc.open", json!({"path": out}));
    let texts: Vec<String> = blocks(&mut e)
        .iter()
        .map(|b| b["text"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(texts, ["INT. LAB - NIGHT", "Rain."]);
}

#[test]
fn repair_writes_a_copy_and_never_the_input() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(dir.path(), "broken-markup.fdx");
    let original = fs::read(&p).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let out = dir.path().join("fixed.fdx");
    let r = ok(&mut d, "file.repair", json!({"path": p, "out": out}));
    assert_eq!(r["repaired"], true);
    assert!(
        r["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["class"] == "dropped")
    );
    assert_eq!(
        fs::read(&p).unwrap(),
        original,
        "the input is never written"
    );
    assert_eq!(
        fs::read(&out).unwrap(),
        fixture("broken-markup.repaired.out")
    );
    // The input, however it is spelled, is refused as the output.
    let spelled = dir.path().join(".").join("broken-markup.fdx");
    let (code, _) = run(
        &mut d,
        "file.repair",
        json!({"path": p, "out": spelled, "overwrite": true}),
    )
    .unwrap_err();
    assert_eq!(code, "would_overwrite_input");
    let (code, _) = run(&mut d, "file.repair", json!({"path": p, "out": out})).unwrap_err();
    assert_eq!(code, "already_exists");
    let (code, _) = run(
        &mut d,
        "file.repair",
        json!({"path": dir.path().join("none.fdx"), "out": dir.path().join("o.fdx")}),
    )
    .unwrap_err();
    assert_eq!(code, "io_error");
}

/// The first save over a file the reader repaired keeps the file as it
/// was (docs/specs/journal.md, "Damaged originals").
#[test]
fn the_first_save_over_a_repaired_file_keeps_the_original() {
    for off_the_ui_thread in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let p = copy(dir.path(), "broken-markup.fdx");
        let original = fs::read(&p).unwrap();
        let mut d = Dispatcher::new(Options::default());
        let r = ok(&mut d, "doc.open", json!({"path": p}));
        assert!(r["repaired"]["dropped"].as_u64().unwrap() > 0, "{r}");
        assert!(!r["warnings"].as_array().unwrap().is_empty());
        let saved = if off_the_ui_thread {
            let prepared = d.save_prepare(&Default::default()).unwrap();
            let written = coldopen_core::io::atomic_write(&prepared.target, &prepared.bytes);
            d.save_commit(prepared, written).unwrap()
        } else {
            ok(&mut d, "doc.save", json!({}))
        };
        assert_eq!(saved["warnings"][0]["code"], "original_kept", "{saved}");
        let kept = asides(dir.path(), ".coldopen-original-");
        assert_eq!(kept.len(), 1);
        assert_eq!(fs::read(&kept[0]).unwrap(), original);
        assert_eq!(fs::read(&p).unwrap(), fixture("broken-markup.repaired.out"));
        // Later saves keep no more copies.
        let id = first(&mut d, "action");
        ok(
            &mut d,
            "block.set_text",
            json!({"id": id, "text": "Again."}),
        );
        let again = ok(&mut d, "doc.save", json!({}));
        assert_eq!(again["warnings"], json!([]));
        assert_eq!(asides(dir.path(), ".coldopen-original-").len(), 1);
    }
}

/// A revert reads the file again with fresh ids; the store follows them,
/// so saving after a revert still writes the file back as it was.
#[test]
fn a_revert_keeps_the_store_with_the_new_ids() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(dir.path(), "styles.fdx");
    let original = fs::read(&p).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let id = first(&mut d, "action");
    ok(&mut d, "block.delete", json!({"id": id}));
    ok(&mut d, "doc.revert", json!({}));
    ok(&mut d, "doc.save", json!({}));
    assert_eq!(fs::read(&p).unwrap(), original);
}

/// After converting, the journal starts from the converted document, so a
/// crash right after the save recovers nothing.
#[test]
fn a_crash_right_after_converting_recovers_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(dir.path(), "settings.fdx");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let out = dir.path().join("as.fountain");
    ok(
        &mut d,
        "doc.save",
        json!({"path": out, "allow_lossy": true}),
    );
    // Saving again asks nothing: the document is in Fountain's form now.
    let again = ok(&mut d, "doc.save", json!({}));
    assert_eq!(again["losses"], json!([]), "{again}");
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": out}));
    assert_eq!(r["recovered"], Value::Null, "{r}");
}

/// Core's view lays an .fdx out exactly as the reference comparison does
/// (docs/specs/layout.md §4.8, §13): the reader's script with the file's
/// format, overrides and title lines. On the private corpus; skips without
/// it.
#[test]
fn the_view_lays_out_an_fdx_as_the_reference_gate_does() {
    fn files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut v: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
        v.sort();
        for p in v {
            if p.is_dir() {
                files(&p, out);
            } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fdx")) {
                out.push(p);
            }
        }
    }
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut all = Vec::new();
    files(&corpus, &mut all);
    if all.is_empty() {
        eprintln!("skipping: no corpus");
        return;
    }
    for path in all {
        let bytes = fs::read(&path).unwrap();
        let read = coldopen_fdx::read(&bytes).unwrap();
        let direct = coldopen_layout::layout(
            &read.script,
            &coldopen_layout::ValidFormat::new(read.format.clone()).unwrap(),
            &coldopen_layout::Options {
                overrides: read.overrides.clone(),
                title_lines: Some(read.title_lines.clone()),
                ..Default::default()
            },
        );
        let mut d = Dispatcher::new(Options {
            journal: false,
            fsync: false,
        });
        ok(&mut d, "doc.open", json!({"path": path}));
        let view = d.view().unwrap().layout();
        assert_eq!(view.pages, direct.pages, "{}", path.display());
        assert_eq!(view.title, direct.title, "{}", path.display());
    }
}
