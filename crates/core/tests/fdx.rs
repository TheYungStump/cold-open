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
            let job = d.save_prepare(&Default::default()).unwrap();
            let prepared = d.save_mark(job.render()).unwrap();
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
        // Every store a real file gives passes the checks a bundle's
        // document must (GR7, GP13).
        let check = d.session().unwrap().doc.check();
        assert!(check.is_ok(), "{}: {check:?}", path.display());
        let view = d.view().unwrap().layout();
        assert_eq!(view.pages, direct.pages, "{}", path.display());
        assert_eq!(view.title, direct.title, "{}", path.display());
    }
}

// ---- The reviews of phases 4–5b (2026-10-28) ------------------------------

/// Opens `text` as an .fdx file of its own.
fn open_fdx(dir: &Path, text: &str) -> (Dispatcher, PathBuf) {
    let p = dir.join("kept.fdx");
    fs::write(&p, text).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    (d, p)
}

/// The `<Paragraph …>…</Paragraph>` in `xml` that holds `text`.
fn paragraph_with<'a>(xml: &'a str, text: &str) -> &'a str {
    let at = xml
        .find(text)
        .unwrap_or_else(|| panic!("no {text:?} in {xml}"));
    let start = xml[..at].rfind("<Paragraph").unwrap();
    let end = at + xml[at..].find("</Paragraph>").unwrap();
    &xml[start..end]
}

const KEPT_HEADING: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"4\">\n  <Content>\n    <Paragraph Type=\"Action\">\n      <Text>Before.</Text>\n    </Paragraph>\n    <Paragraph Type=\"Scene Heading\" StartsNewPage=\"Yes\">\n      <SceneProperties Length=\"1/8\" Page=\"2\" Title=\"two\"/>\n      <Text>INT. TWO - NIGHT</Text>\n    </Paragraph>\n    <Paragraph Type=\"Action\">\n      <Text>Rain.</Text>\n    </Paragraph>\n    <Paragraph Type=\"Scene Heading\">\n      <SceneProperties Length=\"1/8\" Page=\"2\" Title=\"three\"/>\n      <Text>INT. THREE - DAY</Text>\n    </Paragraph>\n    <Paragraph Type=\"Action\">\n      <Text>Sun.</Text>\n    </Paragraph>\n  </Content>\n</FinalDraft>\n";

/// GR8 = GP2 (A): a page break read before a paragraph belongs to its
/// group, and the store kept the paragraph under the break's id. So
/// deleting the break, adding one before a kept paragraph, or moving the
/// scene away from its break lost the paragraph's kept parts, or gave them
/// to the wrong paragraph.
#[test]
fn gr8_a_kept_paragraph_follows_its_block_not_its_page_break() {
    let dir = tempfile::tempdir().unwrap();

    // The break deleted: the heading keeps its scene properties.
    let (mut d, p) = open_fdx(dir.path(), KEPT_HEADING);
    let kinds: Vec<Value> = blocks(&mut d).iter().map(|b| b["type"].clone()).collect();
    assert_eq!(kinds[1], "page_break", "{kinds:?}");
    let br = blocks(&mut d)[1]["id"].clone();
    ok(&mut d, "block.delete", json!({"id": br}));
    ok(&mut d, "doc.save", json!({}));
    let xml = fs::read_to_string(&p).unwrap();
    let two = paragraph_with(&xml, "INT. TWO - NIGHT");
    assert!(two.contains("Title=\"two\""), "{two}");
    assert!(!two.contains("StartsNewPage"), "{two}");
    d.discard();

    // The scene moved away from its break: the break stays, now before the
    // next heading, which keeps its own parts and gets none of the moved
    // one's; the moved heading keeps its own.
    let (mut d, p) = open_fdx(dir.path(), KEPT_HEADING);
    let two_id = first(&mut d, "scene_heading");
    ok(&mut d, "scene.move", json!({"id": two_id}));
    ok(&mut d, "doc.save", json!({}));
    let xml = fs::read_to_string(&p).unwrap();
    let three = paragraph_with(&xml, "INT. THREE - DAY");
    assert!(three.contains("Title=\"three\""), "{three}");
    assert!(!three.contains("Title=\"two\""), "{three}");
    let two = paragraph_with(&xml, "INT. TWO - NIGHT");
    assert!(two.contains("Title=\"two\""), "{two}");
    d.discard();

    // A break inserted before a kept paragraph (Astra's GP2 A).
    let (mut d, p) = open_fdx(
        dir.path(),
        r#"<FinalDraft><Content><Paragraph Type="Action"><Text>Rain.</Text></Paragraph><Paragraph Type="Action" Custom="keep-me"><Text Color="red">The door opens.</Text><ScriptNote>Remember the key.</ScriptNote></Paragraph></Content></FinalDraft>"#,
    );
    ok(
        &mut d,
        "block.insert",
        json!({"before": "b_2", "type": "page_break"}),
    );
    let r = ok(&mut d, "doc.save", json!({}));
    assert_eq!(r["losses"], json!([]), "{r}");
    let xml = fs::read_to_string(&p).unwrap();
    let door = paragraph_with(&xml, "The door opens.");
    for part in [
        "Custom=\"keep-me\"",
        "Color=\"red\"",
        "Remember the key.",
        "StartsNewPage=\"Yes\"",
    ] {
        assert!(door.contains(part), "{part} in {door}");
    }
    d.discard();
}

/// GP2 (B): a dual-dialogue block made into two ordinary speeches carries
/// each nested paragraph's kept parts to its speech's paragraph. What the
/// outer paragraph held that has no place now is a loss the save asks
/// about, never dropped without a word.
#[test]
fn gp2b_undualling_keeps_each_speeches_parts_and_reports_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, p) = open_fdx(
        dir.path(),
        r#"<FinalDraft><Content><Paragraph Type="General"><DualDialogue><Paragraph Type="Character"><Text>ADA</Text></Paragraph><Paragraph Type="Dialogue"><Text>Wait.</Text></Paragraph><Paragraph Type="Character" Custom="keep-me"><Text>BEN</Text></Paragraph><Paragraph Type="Dialogue"><Text Color="red">Go.</Text><ScriptNote>Remember the key.</ScriptNote></Paragraph></DualDialogue></Paragraph></Content></FinalDraft>"#,
    );
    ok(
        &mut d,
        "block.set_attrs",
        json!({"id": "b_3", "dual": false}),
    );
    let r = ok(&mut d, "doc.save", json!({}));
    assert_eq!(r["losses"], json!([]), "{r}");
    let xml = fs::read_to_string(&p).unwrap();
    assert!(!xml.contains("DualDialogue"), "{xml}");
    assert!(
        paragraph_with(&xml, "BEN").contains("Custom=\"keep-me\""),
        "{xml}"
    );
    let go = paragraph_with(&xml, "Go.");
    assert!(
        go.contains("Color=\"red\"") && go.contains("Remember the key."),
        "{go}"
    );
    d.discard();

    // An unknown attribute or element on the outer paragraph has no place
    // in two ordinary speeches.
    let (mut d, _) = open_fdx(
        dir.path(),
        r#"<FinalDraft><Content><Paragraph Type="General" Zork="keepme"><Zed/><DualDialogue><Paragraph Type="Character"><Text>ADA</Text></Paragraph><Paragraph Type="Dialogue"><Text>Wait.</Text></Paragraph><Paragraph Type="Character"><Text>BEN</Text></Paragraph><Paragraph Type="Dialogue"><Text>Go.</Text></Paragraph></DualDialogue></Paragraph></Content></FinalDraft>"#,
    );
    ok(
        &mut d,
        "block.set_attrs",
        json!({"id": "b_3", "dual": false}),
    );
    let (code, e) = run(&mut d, "doc.save", json!({})).unwrap_err();
    assert_eq!(code, "lossy_format", "{e}");
    let losses = e["details"]["losses"].to_string();
    assert!(losses.contains("kept_parts"), "{losses}");
    d.discard();
}

/// CL8: an edit inside a dual-dialogue block keeps the outer paragraph's
/// `StartsNewPage="No"`, as §14.4 carries it for any paragraph whose page
/// break did not change.
#[test]
fn cl8_editing_a_dual_speech_keeps_the_outer_starts_new_page() {
    let dir = tempfile::tempdir().unwrap();
    let p = copy(dir.path(), "dual-dialogue.fdx");
    let before = fs::read_to_string(&p).unwrap();
    let outer = paragraph_with(&before, "<DualDialogue>").to_string();
    assert!(outer.contains("StartsNewPage=\"No\""), "{outer}");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    let id = first(&mut d, "dialogue");
    ok(
        &mut d,
        "text.insert",
        json!({"id": id, "at": 0, "text": "Zz "}),
    );
    ok(&mut d, "doc.save", json!({}));
    let after = fs::read_to_string(&p).unwrap();
    let outer = paragraph_with(&after, "<DualDialogue>");
    assert!(outer.contains("StartsNewPage=\"No\""), "{outer}");
    d.discard();
}

fn corpus_fdx(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            corpus_fdx(&p, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fdx")) {
            out.push(p);
        }
    }
}

/// CL8, on the corpus (Claude's reproduction): in every public .fdx file
/// with dual dialogue that reads without repair, three characters typed
/// into a dual speech change nothing outside that dual-dialogue group
/// (fdx.md §16.1, G3), and the group's outer `StartsNewPage` keeps its
/// value. Copies only; skips cleanly without `corpus/`.
#[test]
fn an_edit_inside_dual_dialogue_stays_local_in_the_corpus() {
    let mut files = Vec::new();
    corpus_fdx(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
        &mut files,
    );
    let mut tried = 0;
    for src in files {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.fdx");
        fs::copy(&src, &p).unwrap();
        let before = fs::read(&p).unwrap();
        let mut d = Dispatcher::new(Options {
            journal: false,
            ..Options::default()
        });
        let r = ok(&mut d, "doc.open", json!({"path": p}));
        if !r["repaired"].is_null() {
            continue;
        }
        let bs = blocks(&mut d);
        let Some(k) = bs.iter().position(|b| b["dual"] == true) else {
            continue;
        };
        let Some(line) = bs[k..].iter().find(|b| b["type"] == "dialogue") else {
            continue;
        };
        ok(
            &mut d,
            "text.insert",
            json!({"id": line["id"], "at": 0, "text": "Zz "}),
        );
        ok(&mut d, "doc.save", json!({}));
        let after = fs::read(&p).unwrap();
        let pre = before
            .iter()
            .zip(&after)
            .take_while(|(a, b)| a == b)
            .count();
        let suf = before[pre..]
            .iter()
            .rev()
            .zip(after[pre..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        // The group: the outer paragraph around the dual block edited.
        let text = String::from_utf8_lossy(&before);
        // The first change is inside the group: in the dual block, or in
        // the whitespace just before it, which is generated too.
        let dd = text[..pre]
            .rfind("<DualDialogue>")
            .filter(|&k| !text[k..pre].contains("</DualDialogue>"))
            .or_else(|| text[pre..].find("<DualDialogue>").map(|k| pre + k))
            .expect("the edit is in a dual block");
        let start = text[..dd].rfind("<Paragraph").unwrap();
        let tag_end = start + text[start..].find('>').unwrap() + 1;
        let close = dd + text[dd..].find("</DualDialogue>").unwrap();
        let end = close + text[close..].find("</Paragraph>").unwrap() + "</Paragraph>".len();
        let name = src.display();
        // The outer start tag is generated when the group is (§14.7), but
        // what it carried of the file's keeps its value (§14.4).
        let attr = |tag: &str| {
            tag.find("StartsNewPage=\"")
                .map(|k| tag[k..].split('"').nth(1).unwrap_or("").to_string())
        };
        let new_text = String::from_utf8_lossy(&after);
        let new_tag = &new_text[start..start + new_text[start..].find('>').unwrap() + 1];
        assert_eq!(
            attr(new_tag),
            attr(&text[start..tag_end]),
            "{name}: {new_tag}"
        );
        assert!(
            before.len() - suf <= end,
            "{name}: bytes after the group changed"
        );
        assert!(
            String::from_utf8_lossy(&after[pre..after.len() - suf]).contains("Zz "),
            "{name}"
        );
        tried += 1;
    }
    if tried == 0 {
        eprintln!("an_edit_inside_dual_dialogue_stays_local_in_the_corpus: no corpus, skipped");
    }
}

/// AR5: once an .fdx script with space above that is not a whole number of
/// lines is converted to Fountain, that space is blank lines in the
/// document too, so a later save in place does not ask again
/// (docs/specs/commands.md, "Converting"; Antigravity's reproduction).
#[test]
fn ar5_converting_to_fountain_takes_the_spacing_as_blank_lines() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _) = open_fdx(
        dir.path(),
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"5\">\n  <Content>\n    <Paragraph Type=\"Scene Heading\">\n      <Text>INT. OFFICE - DAY</Text>\n    </Paragraph>\n    <Paragraph Type=\"Action\" SpaceBefore=\"18\">\n      <Text>Alice waits.</Text>\n    </Paragraph>\n  </Content>\n</FinalDraft>\n",
    );
    let action = first(&mut d, "action");
    let p = dir.path().join("test.fountain");
    ok(&mut d, "doc.save", json!({"path": p, "allow_lossy": true}));
    ok(
        &mut d,
        "text.insert",
        json!({"id": action, "at": 12, "text": " Impatiently."}),
    );
    ok(&mut d, "doc.save", json!({}));
    // 18 pt is a line and a half: the nearest whole number is two lines,
    // as the converted file wrote it.
    let written = fs::read_to_string(&p).unwrap();
    let action = blocks(&mut d)
        .into_iter()
        .find(|b| b["type"] == "action")
        .unwrap();
    assert_eq!(action["space_before"], 2, "{action}");
    assert!(
        written.contains("DAY\n\n\nAlice waits. Impatiently."),
        "{written:?}"
    );
    d.discard();
}
