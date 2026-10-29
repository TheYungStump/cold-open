//! The `.coldopen` bundle (docs/specs/bundle.md) through the dispatcher:
//! a lossless save and reopen, ids and changes that survive it, and the
//! crash journal that names the change table instead of copying it.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_core::{Author, Command, CommandError, Dispatcher, Options, journal};
use serde_json::{Value, json};

const SCRIPT: &str = "\
INT. KITCHEN - DAY

Maya makes toast.

MAYA
Breakfast is ready.

EXT. GARDEN - DAY

Theo waters plants.
";

fn writer() -> Author {
    Author::named_writer("Maya")
}

fn agent() -> Author {
    Author::agent("claude-code", Some("text_replace".into()))
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

fn get(d: &mut Dispatcher) -> Value {
    ok(d, &writer(), "doc.get", json!({}))
}

fn changes(d: &mut Dispatcher) -> Value {
    ok(d, &writer(), "changes.list", json!({}))["changes"].clone()
}

fn header(d: &mut Dispatcher) -> Value {
    let j = String::from_utf8(d.journal_bytes().unwrap()).unwrap();
    serde_json::from_str(j.lines().next().unwrap()).unwrap()
}

/// A Fountain script with an agent's tracked change, saved as a bundle.
fn project(dir: &Path) -> (Dispatcher, PathBuf) {
    let f = dir.join("s.fountain");
    fs::write(&f, SCRIPT).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &writer(), "doc.open", json!({"path": f}));
    ok(
        &mut d,
        &agent(),
        "text.replace",
        json!({"find": "toast", "with": "eggs"}),
    );
    let b = dir.join("s.coldopen");
    let r = ok(&mut d, &writer(), "doc.save", json!({"path": b}));
    assert_eq!(r["format"], "coldopen");
    assert_eq!(r["losses"], json!([]), "a bundle holds everything: {r}");
    (d, b)
}

#[test]
fn a_bundle_keeps_ids_changes_and_switches_across_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(&mut d, &writer(), "changes.track", json!({"writer": true}));
    ok(
        &mut d,
        &writer(),
        "block.insert",
        json!({"after": "b_2", "type": "action", "text": "The toaster pops."}),
    );
    ok(&mut d, &writer(), "doc.save", json!({}));
    let before = (get(&mut d)["blocks"].clone(), changes(&mut d));
    let id = header(&mut d)["doc_id"].clone();
    assert!(id.is_string(), "{}", header(&mut d));
    d.shutdown();

    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert_eq!(r["format"], "coldopen");
    assert!(r["recovered"].is_null() && r["repaired"].is_null(), "{r}");
    assert_eq!((get(&mut d)["blocks"].clone(), changes(&mut d)), before);
    // Ids survive, so a new block continues from them.
    let r = ok(
        &mut d,
        &writer(),
        "block.insert",
        json!({"type": "action", "text": "Later."}),
    );
    assert_eq!(r["id"], "b_8", "{r}");
    // The switches came back: the writer's edit is tracked.
    assert!(r["change"].is_string(), "{r}");
    // A save in place keeps the identity.
    ok(&mut d, &writer(), "doc.save", json!({}));
    assert_eq!(header(&mut d)["doc_id"], id);
    d.shutdown();
}

#[test]
fn the_journal_names_the_change_table_instead_of_copying_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _b) = project(dir.path());
    let h = header(&mut d);
    assert_eq!(h["format"], "coldopen");
    assert_eq!(h["semantics"], journal::SEMANTICS);
    assert!(h.get("changes").is_none(), "{h}");
    assert_eq!(h["changes_ref"]["count"], 1, "{h}");
    // A Fountain journal, which has no file to hold the table, copies it.
    ok(
        &mut d,
        &writer(),
        "doc.save",
        json!({"path": dir.path().join("t.fountain"), "allow_lossy": true}),
    );
    let h = header(&mut d);
    assert_eq!(h["changes"]["changes"][0]["id"], "c_1", "{h}");
    assert!(
        h.get("doc_id").is_none(),
        "another file is another document"
    );
    d.discard();
}

#[test]
fn tracked_changes_in_a_bundle_survive_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(&mut d, &agent(), "block.delete", json!({"id": "b_6"}));
    let before = (get(&mut d)["blocks"].clone(), changes(&mut d));
    drop(d); // a crash
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert_eq!(r["recovered"]["external_change"], false, "{r}");
    assert_eq!((get(&mut d)["blocks"].clone(), changes(&mut d)), before);
    d.discard();
}

#[test]
fn a_crash_between_a_save_and_the_journal_restart_loses_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(&mut d, &agent(), "block.delete", json!({"id": "b_6"}));
    let old_journal = d.journal_bytes().unwrap();
    // The save writes the bundle, with a longer table, then restarts the
    // journal; put the old journal back, as a crash between the two
    // would leave it.
    ok(&mut d, &writer(), "doc.save", json!({}));
    let saved = (get(&mut d)["blocks"].clone(), changes(&mut d));
    drop(d);
    let hidden_attr_safe = journal::path_for(&b);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&hidden_attr_safe)
        .unwrap();
    std::io::Write::write_all(&mut f, &old_journal).unwrap();
    drop(f);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert!(r["recovered"].is_null(), "the file holds it all: {r}");
    assert!(
        !r["warnings"].to_string().contains("change_data_missing"),
        "{r}"
    );
    assert_eq!((get(&mut d)["blocks"].clone(), changes(&mut d)), saved);
    d.shutdown();
}

#[test]
fn another_project_at_the_path_is_a_change_by_another_program() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(
        &mut d,
        &writer(),
        "block.insert",
        json!({"type": "action", "text": "UNSAVED."}),
    );
    drop(d); // a crash with unsaved work
    // Someone puts a different project with the same name there.
    let other = dir.path().join("other");
    fs::create_dir(&other).unwrap();
    let (mut d2, b2) = project(&other);
    d2.shutdown();
    fs::copy(&b2, &b).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert_eq!(r["recovered"]["external_change"], true, "{r}");
    assert!(
        get(&mut d)["blocks"].to_string().contains("UNSAVED."),
        "the work is recovered, not dropped"
    );
    d.discard();
}

#[test]
fn undo_and_redo_work_across_a_bundle_save_and_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, b) = project(dir.path());
    ok(&mut d, &writer(), "changes.accept", json!({"all": true}));
    ok(&mut d, &writer(), "doc.save", json!({}));
    // Undo the accept after the save: the marks come back.
    ok(&mut d, &writer(), "undo", json!({}));
    assert_eq!(changes(&mut d).as_array().unwrap().len(), 1);
    let before = (get(&mut d)["blocks"].clone(), changes(&mut d));
    drop(d); // a crash
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": b}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    assert_eq!((get(&mut d)["blocks"].clone(), changes(&mut d)), before);
    d.discard();
}

#[test]
fn saving_a_bundle_as_fountain_asks_and_converting_back_makes_a_new_identity() {
    let dir = tempfile::tempdir().unwrap();
    let (mut d, _b) = project(dir.path());
    let id = header(&mut d)["doc_id"].clone();
    let e = run(
        &mut d,
        &writer(),
        "doc.save",
        json!({"path": dir.path().join("c.fountain")}),
    )
    .unwrap_err();
    assert_eq!(e.code, "lossy_format");
    ok(
        &mut d,
        &writer(),
        "doc.save",
        json!({"path": dir.path().join("c.fountain"), "allow_lossy": true}),
    );
    let c = dir.path().join("c.coldopen");
    ok(&mut d, &writer(), "doc.save", json!({"path": c}));
    assert_ne!(header(&mut d)["doc_id"], id);
    d.shutdown();
}

#[test]
fn doc_new_at_a_bundle_path_starts_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("new.coldopen");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &writer(), "doc.new", json!({"path": p}));
    assert!(header(&mut d)["doc_id"].is_string());
    ok(
        &mut d,
        &writer(),
        "block.insert",
        json!({"type": "scene_heading", "text": "INT. LAB - NIGHT"}),
    );
    ok(&mut d, &writer(), "doc.save", json!({}));
    d.shutdown();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &writer(), "doc.open", json!({"path": p}));
    assert_eq!(r["blocks"], 1, "{r}");
    let e = run(
        &mut d,
        &writer(),
        "doc.export",
        json!({"format": "coldopen"}),
    )
    .unwrap_err();
    assert_eq!(e.code, "unsupported_format");
    d.shutdown();
}

// ---- The reviews of phases 4–5b (2026-10-28) ------------------------------

/// A bundle with the usual entries and one another tool added.
fn with_extra_entry(dir: &Path) -> PathBuf {
    let p = dir.join("extra.coldopen");
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &Author::writer(), "doc.new", json!({"path": p}));
    ok(
        &mut d,
        &Author::writer(),
        "block.insert",
        json!({"type": "action", "text": "Rain."}),
    );
    ok(&mut d, &Author::writer(), "doc.save", json!({}));
    d.shutdown();
    let bytes = fs::read(&p).unwrap();
    let archive = rawzip::ZipArchive::from_slice(&bytes).unwrap();
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut list = archive.entries();
    while let Some(e) = list.next_entry().unwrap() {
        let name = String::from_utf8_lossy(e.file_path().as_ref()).into_owned();
        let raw = archive.get_entry(e.wayfinder()).unwrap().data().to_vec();
        let data = miniz_oxide::inflate::decompress_to_vec(&raw).unwrap();
        parts.push((name, data));
    }
    parts.push((
        "notes.json".into(),
        br#"{"notes":["Keep the opening scene."]}"#.to_vec(),
    ));
    let mut out = Vec::new();
    let mut zw = rawzip::ZipArchiveWriter::new(&mut out);
    for (name, data) in &parts {
        let (mut entry, _) = zw
            .new_file(name)
            .compression_method(rawzip::CompressionMethod::STORE)
            .start()
            .unwrap();
        std::io::Write::write_all(&mut entry, data).unwrap();
        entry
            .finish(rawzip::DataDescriptorOutput::new(
                rawzip::crc32(data),
                data.len() as u64,
            ))
            .unwrap();
    }
    zw.finish().unwrap();
    fs::write(&p, out).unwrap();
    p
}

fn entry_names(bytes: &[u8]) -> Vec<String> {
    let archive = rawzip::ZipArchive::from_slice(bytes).unwrap();
    let mut names = Vec::new();
    let mut list = archive.entries();
    while let Some(e) = list.next_entry().unwrap() {
        names.push(String::from_utf8_lossy(e.file_path().as_ref()).into_owned());
    }
    names
}

/// GP3: an entry another tool added to a project is kept: saving writes it
/// back, unchanged, after Cold Open's own; so does a crash and recovery.
/// Converting to Fountain asks first, with a `bundle_entries` loss.
#[test]
fn gp3_another_tools_entries_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let p = with_extra_entry(dir.path());
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &Author::writer(), "doc.open", json!({"path": p}));
    ok(
        &mut d,
        &Author::writer(),
        "block.insert",
        json!({"type": "action", "text": "Wind."}),
    );
    ok(&mut d, &Author::writer(), "doc.save", json!({}));
    let names = entry_names(&fs::read(&p).unwrap());
    assert_eq!(
        names.last().map(String::as_str),
        Some("notes.json"),
        "{names:?}"
    );

    // A crash: the recovered document still writes it back.
    ok(
        &mut d,
        &Author::writer(),
        "block.insert",
        json!({"type": "action", "text": "Hail."}),
    );
    drop(d);
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, &Author::writer(), "doc.open", json!({"path": p}));
    assert_eq!(r["recovered"]["commands"], 1, "{r}");
    ok(&mut d, &Author::writer(), "doc.save", json!({}));
    assert!(entry_names(&fs::read(&p).unwrap()).contains(&"notes.json".to_string()));

    // Converting leaves it out only when asked.
    let f = dir.path().join("extra.fountain");
    let e = d
        .execute(Command::from_parts("doc.save", json!({"path": f})).unwrap())
        .unwrap_err();
    assert_eq!(e.code, "lossy_format", "{e}");
    assert!(
        format!("{:?}", e.details).contains("bundle_entries"),
        "{:?}",
        e.details
    );
    d.discard();
}

/// A one-block bundle whose block is marked with change `c`, or revision
/// set `r`, and has no table for it: as a few hundred bytes a crafted file
/// can be.
fn sparse(change: &str, rev: Option<&str>) -> Vec<u8> {
    let mut block =
        json!({"id": "b_1", "type": "action", "text": "x", "change": {"inserted": change}});
    if let Some(r) = rev {
        block["rev"] = json!(r);
    }
    let doc = json!({"blocks": [block], "next_id": 2});
    let meta = format!(
        "{{\"format\":\"coldopen-bundle\",\"v\":{},\"app\":\"0.1.0\",\"doc_id\":\"x\"}}",
        if rev.is_some() { 2 } else { 1 }
    );
    let mut out = Vec::new();
    let mut zw = rawzip::ZipArchiveWriter::new(&mut out);
    for (name, data) in [
        ("meta.json", meta.into_bytes()),
        ("document.json", serde_json::to_vec(&doc).unwrap()),
        ("script.fountain", b"x\n".to_vec()),
    ] {
        let (mut entry, _) = zw
            .new_file(name)
            .compression_method(rawzip::CompressionMethod::STORE)
            .start()
            .unwrap();
        std::io::Write::write_all(&mut entry, &data).unwrap();
        entry
            .finish(rawzip::DataDescriptorOutput::new(
                rawzip::crc32(&data),
                data.len() as u64,
            ))
            .unwrap();
    }
    zw.finish().unwrap();
    out
}

/// GR2 = CL4 = GP12: a mark naming a change, or a revision set, the bundle
/// has no table for is given a stand-in, and so is every id below it. An id
/// past what the journal ever accepts (10,000,000 changes; 10,000 sets) is
/// not a real project's: `document.json` counts as damaged, and the project
/// opens from its readable copy, at once. Before, `c_18446744073709551614`
/// ran until memory ran out.
#[test]
fn gr2_a_mark_naming_an_absurd_id_is_not_filled_up_to() {
    for bytes in [
        sparse("c_10000001", None),
        sparse("c_18446744073709551614", None),
        sparse("c_1", Some("r_10001")),
        sparse("c_1", Some("r_18446744073709551614")),
    ] {
        let start = std::time::Instant::now();
        let r = coldopen_core::bundle::read(&bytes).unwrap();
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
        assert!(r.doc.changes.len() <= 1, "{}", r.doc.changes.len());
        assert!(r.doc.revisions.sets.len() <= 1);
        let codes = r.repaired.unwrap()["issues"].to_string();
        assert!(codes.contains("bundle_document"), "{codes}");
    }
    // Within the limits, the stand-ins are made as before.
    let r = coldopen_core::bundle::read(&sparse("c_1000", None)).unwrap();
    assert_eq!(r.doc.changes.len(), 1000);
}

/// GR7 = GP13: a project whose .fdx store holds a kept paragraph with no
/// blocks, or one not stored under its first block, or naming an id at or
/// past `next_id`, fails `Document::check`, so it opens from its readable
/// copy instead of reaching the writer (Astra's probe, as a bundle).
#[test]
fn gr7_a_kept_paragraph_with_no_blocks_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fdx");
    fs::write(
        &p,
        "<?xml version=\"1.0\"?>\n<FinalDraft>\n  <Content>\n    <Paragraph Type=\"Action\" Custom=\"x\">\n      <Text>Rain.</Text>\n    </Paragraph>\n  </Content>\n</FinalDraft>\n",
    )
    .unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, &Author::writer(), "doc.open", json!({"path": p}));
    let doc = d.session().unwrap().doc.clone();
    d.discard();
    let key = *doc.fdx.as_ref().unwrap().paragraphs.keys().next().unwrap();
    for broken in 0..3 {
        let mut doc = doc.clone();
        let kept = doc.fdx.as_mut().unwrap();
        let kp = kept.paragraphs.get_mut(&key).unwrap();
        match broken {
            0 => kp.blocks.clear(),
            1 => kp.blocks[0].id = 7,
            _ => {
                let mut extra = kp.blocks[0].clone();
                extra.id = 99;
                kp.blocks.push(extra);
            }
        }
        assert!(doc.check().is_err(), "case {broken}");
        let read = coldopen_core::bundle::read(&coldopen_core::bundle::write(&doc)).unwrap();
        let issues = read.repaired.expect("repaired")["issues"].to_string();
        assert!(
            issues.contains("bundle_document"),
            "case {broken}: {issues}"
        );
    }
}
