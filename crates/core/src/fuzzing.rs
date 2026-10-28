//! Checks shared by the cargo-fuzz targets in `fuzz/` and the stable test
//! suite, so the fuzzed properties also run on every CI build.

use serde_json::{Value, json};

use crate::author::Author;
use crate::command::{Class, Command};
use crate::dispatcher::{Dispatcher, Options};
use crate::document::Document;

/// Feeds each line of `data` to a dispatcher holding an untitled document.
/// Every line must get exactly one well-formed response echoing its id.
pub fn envelope(data: &[u8]) -> Result<(), String> {
    let mut d = Dispatcher::new(Options {
        journal: false,
        fsync: false,
    });
    d.handle(&json!({ "command": "doc.new" }));
    d.handle(&json!({ "command": "block.insert", "params": { "type": "scene_heading", "text": "INT. X" } }));
    for line in String::from_utf8_lossy(data).lines() {
        // File commands are skipped so fuzzing never touches the disk.
        if reaches_disk(line) {
            continue;
        }
        let response: Value = serde_json::from_str(&d.handle_line(line))
            .map_err(|e| format!("response is not JSON: {e}"))?;
        if response["v"] != 1 || !response["ok"].is_boolean() {
            return Err(format!("malformed response: {response}"));
        }
        if response["ok"] == true && response.get("result").is_none() {
            return Err(format!("success without a result: {response}"));
        }
        if response["ok"] == false && !response["error"]["code"].is_string() {
            return Err(format!("failure without an error code: {response}"));
        }
    }
    Ok(())
}

/// Whether the dispatcher could open, save or export a file for `line`.
///
/// The line is decoded the way `Dispatcher::handle_line` decodes it, so a
/// name spelled with JSON escapes (`"export\u002epdf"`) is caught. Matching
/// the raw text would miss it. Session commands (`doc.new`, `doc.open`,
/// `doc.save`, `doc.close`) and output commands (`export.pdf`) count. The
/// check errs towards skipping: it ignores `v` and unknown fields, and also
/// tries params the dispatcher would refuse as not being an object.
fn reaches_disk(line: &str) -> bool {
    let Ok(Value::Object(request)) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    let Some(name) = request.get("command").and_then(Value::as_str) else {
        return false;
    };
    let params = match request.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(p) => p.clone(),
    };
    // A command that does not build is refused by the dispatcher before it
    // can run, so the line still exercises that error path.
    Command::from_parts(name, params)
        .is_ok_and(|cmd| matches!(cmd.class(), Class::Session | Class::Output | Class::File))
}

/// Runs real recovery over arbitrary bytes as a journal, for a script that
/// has no file and for one that does. It must never panic, and:
/// - a recovered document keeps the id rules (review finding C7);
/// - it is exactly what the trusted entries give: recovering only the bytes
///   up to where they end yields the same document, with nothing left to
///   distrust (review finding F9).
pub fn journal_replay(data: &[u8]) -> Result<(), String> {
    let file = b"INT. HOUSE - DAY\n\nRain.\n";
    for disk in [None, Some(&file[..])] {
        let r = crate::dispatcher::recovery_check(data, disk);
        let Some(doc) = &r.doc else {
            continue;
        };
        doc.check()
            .map_err(|e| format!("a recovered document breaks the id rules: {e}"))?;
        let cut = usize::try_from(r.valid_len)
            .unwrap_or(usize::MAX)
            .min(data.len());
        let prefix = crate::dispatcher::recovery_check(&data[..cut], disk);
        if prefix.doc.as_ref() != Some(doc) {
            return Err("the recovered document is not what its trusted entries give".into());
        }
        if prefix.problem.is_some() {
            return Err(format!(
                "the trusted entries alone still have a problem: {:?}",
                prefix.problem
            ));
        }
    }
    Ok(())
}

/// Reads arbitrary bytes as a `.coldopen` bundle (docs/specs/bundle.md).
/// It must never panic, and whatever it reads keeps the model rules and
/// writes a bundle that reads back as the same document, with nothing to
/// repair, and writes again to the same bytes.
pub fn bundle_read(data: &[u8]) -> Result<(), String> {
    let Ok(read) = crate::bundle::read(data) else {
        return Ok(());
    };
    read.doc
        .check()
        .map_err(|e| format!("a bundle read into a document that breaks a rule: {e}"))?;
    let bytes = crate::bundle::write(&read.doc);
    let back = crate::bundle::read(&bytes)
        .map_err(|e| format!("a bundle Cold Open wrote cannot be read: {e}"))?;
    if back.repaired.is_some() {
        return Err(format!(
            "a bundle Cold Open wrote needs repair: {:?}",
            back.repaired
        ));
    }
    if back.doc != read.doc {
        return Err("a bundle does not read back as the document it was written from".into());
    }
    if crate::bundle::write(&back.doc) != bytes {
        return Err("writing a bundle twice gives different bytes".into());
    }
    Ok(())
}

/// The script the `changes` check edits.
const CHANGES_SCRIPT: &str = "INT. KITCHEN - DAY\n\nMaya makes toast.\n\nMAYA\nBreakfast is ready.\n\nEXT. GARDEN - DAY\n\nTheo waters plants.\n\nINT. SHED - NIGHT\n\nA lamp flickers.\n";

/// A dispatcher with the `changes` script open in memory, untracked.
fn script_session() -> Dispatcher {
    let mut d = Dispatcher::new(Options {
        journal: false,
        fsync: false,
    });
    d.handle(&json!({ "command": "doc.new" }));
    let script = coldopen_fountain::parse(CHANGES_SCRIPT).script;
    for e in script.elements {
        let Ok(mut p) = serde_json::to_value(&e) else {
            continue;
        };
        p["tracked"] = json!(false);
        d.handle(&json!({ "command": "block.insert", "params": p }));
    }
    d
}

/// The authors the edit checks take turns as.
fn authors() -> [Author; 3] {
    [
        Author::named_writer("Maya"),
        Author::agent("agent-a", None),
        Author::agent("agent-b", None),
    ]
}

/// The command four bytes describe on `doc`: an edit, a settle, undo or
/// redo, and with `revisions` the revision commands too.
fn edit_for(op: &[u8], doc: &Document, revisions: bool) -> Option<(&'static str, Value)> {
    const WORDS: [&str; 4] = ["toast", "a", "plants", " "];
    const KINDS: [&str; 5] = ["action", "dialogue", "character", "scene_heading", "note"];
    let [kind, _, x, y] = [0, 1, 2, 3].map(|i| op.get(i).copied().unwrap_or(0));
    let blocks: Vec<(String, usize)> = doc
        .blocks
        .iter()
        .map(|b| (b.id.to_string(), b.element.text.char_len()))
        .collect();
    let heads: Vec<String> = doc
        .blocks
        .iter()
        .filter(|b| b.element.kind == coldopen_screenplay::ElementKind::SceneHeading)
        .map(|b| b.id.to_string())
        .collect();
    let pending: Vec<String> = doc.pending().iter().map(|c| c.to_string()).collect();
    let pick = |n: u8| -> (String, usize) {
        if blocks.is_empty() {
            ("b_1".into(), 0)
        } else {
            blocks[usize::from(n) % blocks.len()].clone()
        }
    };
    let (id, len) = pick(x);
    let at = usize::from(y) % (len + 1);
    let word = WORDS[usize::from(y) % WORDS.len()];
    let change = |n: u8| -> Value {
        match pending.get(usize::from(n) % pending.len().max(1)) {
            Some(c) => json!({ "id": c }),
            None => json!({ "all": true }),
        }
    };
    let settle = |n: u8| -> Value {
        match n % 3 {
            0 => change(n / 3),
            1 => match heads.get(usize::from(n) % heads.len().max(1)) {
                Some(h) => json!({ "scene": h }),
                None => json!({ "all": true }),
            },
            _ => json!({ "all": true }),
        }
    };
    let kinds = if revisions { 20 } else { 16 };
    let (name, mut params) = match kind % kinds {
        0 => ("text.insert", json!({ "id": id, "at": at, "text": word })),
        1 => (
            "text.delete",
            json!({ "id": id, "from": at, "to": (at + 1 + usize::from(y % 4)).min(len) }),
        ),
        2 => (
            "text.replace",
            json!({ "find": word, "with": WORDS[usize::from(x) % 4] }),
        ),
        3 => (
            "block.set_text",
            json!({ "id": id, "text": format!("{word} plants {y}") }),
        ),
        4 => (
            "block.insert",
            json!({ "after": id, "type": KINDS[usize::from(y) % KINDS.len()], "text": word }),
        ),
        5 => ("block.delete", json!({ "id": id })),
        6 => (
            "block.set_type",
            json!({ "id": id, "type": KINDS[usize::from(y) % KINDS.len()] }),
        ),
        7 => ("block.split", json!({ "id": id, "at": at })),
        8 => ("block.join", json!({ "id": id })),
        9 => match (
            heads.first(),
            heads.get(usize::from(y) % heads.len().max(1)),
        ) {
            (Some(_), Some(to)) => (
                "scene.move",
                json!({ "id": heads[usize::from(x) % heads.len()], "before": to }),
            ),
            _ => return None,
        },
        10 => ("changes.accept", settle(y)),
        11 => ("changes.reject", settle(y)),
        12 => ("undo", json!({})),
        13 => ("redo", json!({})),
        14 => (
            "text.style",
            json!({ "id": id, "from": 0, "to": at, "bold": true }),
        ),
        15 => ("changes.track", json!({ "writer": y % 2 == 0 })),
        16 => ("revision.start", json!({})),
        17 => ("revision.end", json!({})),
        18 => (
            "revision.clear",
            json!({ "set": format!("r_{}", 1 + usize::from(y) % doc.revisions.sets.len().max(1)) }),
        ),
        _ => ("revision.clear", json!({ "all": true, "sets": y % 4 == 0 })),
    };
    if matches!(kind % kinds, 0..=9) {
        match kind / kinds % 4 {
            1 => params["tracked"] = json!(false),
            2 => params["tracked"] = json!(true),
            _ => {}
        }
    }
    Some((name, params))
}

/// Runs the edits `data` describes, checking the model rules after each.
fn run_edits(d: &mut Dispatcher, data: &[u8], revisions: bool) -> Result<(), String> {
    let authors = authors();
    for op in data.chunks(4) {
        let who = op.get(1).copied().unwrap_or(0);
        let by = &authors[usize::from(who) % authors.len()];
        let Some(s) = d.session() else {
            return Err("the document went away".into());
        };
        let Some((name, params)) = edit_for(op, &s.doc, revisions) else {
            continue;
        };
        let Ok(cmd) = Command::from_parts(name, params) else {
            continue;
        };
        let _ = d.execute_as(cmd, by);
        let Some(s) = d.session() else {
            return Err("the document went away".into());
        };
        s.doc
            .check()
            .map_err(|e| format!("after {name}, the document breaks a rule: {e}"))?;
    }
    Ok(())
}

/// Runs the edits arbitrary bytes describe on a short script: tracked
/// edits by a writer and two agents, settling by id, scene and all, and
/// undo and redo (docs/specs/changes.md). It must never panic; after every
/// step the document keeps the model rules and every mark names a change
/// the table has; accepting every change leaves no mark, and undoing that
/// gives the marks back.
pub fn changes(data: &[u8]) -> Result<(), String> {
    let mut d = script_session();
    run_edits(&mut d, data, false)?;
    let Some(before) = d.session().map(|s| s.doc.clone()) else {
        return Err("the document went away".into());
    };
    let accepted = d.handle(&json!({ "command": "changes.accept", "params": { "all": true } }));
    let Some(s) = d.session() else {
        return Err("the document went away".into());
    };
    if before.has_changes() {
        if accepted["ok"] != true {
            return Err(format!("accepting every change failed: {accepted}"));
        }
        if s.doc.has_changes() {
            return Err("accepting every change left marks".into());
        }
        if s.doc.blocks != before.accepted().blocks {
            return Err("accepting every change is not the accepted form".into());
        }
        d.handle(&json!({ "command": "undo" }));
        if d.session().map(|s| &s.doc.blocks) != Some(&before.blocks) {
            return Err("undoing an accept does not give the marks back".into());
        }
    }
    Ok(())
}

/// The same edits with revision sets started, ended and cleared among them
/// (docs/specs/revisions.md). It must never panic; after every step the
/// document keeps the model rules and every revision mark names a set the
/// table has; the document written as a bundle reads back as it was; and
/// undoing every step leaves no revision mark and the default table.
pub fn revisions(data: &[u8]) -> Result<(), String> {
    let mut d = script_session();
    run_edits(&mut d, data, true)?;
    let Some(doc) = d.session().map(|s| s.doc.clone()) else {
        return Err("the document went away".into());
    };
    let back = crate::bundle::read(&crate::bundle::write(&doc))
        .map_err(|e| format!("a bundle Cold Open wrote cannot be read: {e}"))?;
    if back.repaired.is_some() {
        return Err(format!(
            "a bundle Cold Open wrote needs repair: {:?}",
            back.repaired
        ));
    }
    if back.doc.blocks != doc.blocks || back.doc.revisions != doc.revisions {
        return Err("a bundle does not read back with its revision sets and marks".into());
    }
    // Every step, the script's own insertions included, so nothing can be
    // left that an edit made.
    while d.handle(&json!({ "command": "undo" }))["ok"] == true {}
    let Some(s) = d.session() else {
        return Err("the document went away".into());
    };
    if s.doc.has_revision_data() {
        return Err("undoing every step leaves revision data".into());
    }
    Ok(())
}

/// Reads arbitrary bytes as two Fountain drafts, split at the first zero
/// byte (or in the middle), and compares them (docs/specs/diff.md §7). It
/// must never panic, and:
/// - each draft against itself is empty;
/// - the counts match the lists, and swapping the drafts swaps added and
///   deleted scenes;
/// - bringing B into A as tracked changes always applies; accepting all of
///   it leaves only what cannot be tracked (restyles, attributes, among
///   them blank lines and centering, and the title page), and rejecting all
///   of it gives A back.
pub fn diff(data: &[u8]) -> Result<(), String> {
    use crate::format::FormatId;
    use crate::io::SourceStyle;
    use crate::session::{Exec, Session};

    let cut = data.iter().position(|&b| b == 0).unwrap_or(data.len() / 2);
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let (ta, tb) = (text(&data[..cut]), text(data.get(cut + 1..).unwrap_or(&[])));
    let doc = |t: &str| Document::from_script(coldopen_fountain::parse(t).script);
    let (a, b) = (doc(&ta), doc(&tb));
    fn named(d: &Document) -> crate::diff::Named<'_> {
        crate::diff::Named {
            doc: d,
            format: FormatId::Fountain,
            path: None,
        }
    }
    let result = |x: &Document, y: &Document| crate::diff::result(&named(x), &named(y));
    for d in [&a, &b] {
        let r = result(d, d);
        if r["scenes"] != json!([]) || r["title_page"] != json!([]) {
            return Err("a draft against itself has differences".into());
        }
    }
    let (ab, ba) = (result(&a, &b), result(&b, &a));
    for r in [&ab, &ba] {
        let count = |op: &str| {
            r["scenes"]
                .as_array()
                .map_or(0, |s| s.iter().filter(|x| x["op"] == op).count() as u64)
        };
        for op in ["changed", "moved", "added", "deleted"] {
            if r["summary"]["scenes"][op].as_u64() != Some(count(op)) {
                return Err(format!("the {op} count does not match the list"));
            }
        }
    }
    if ab["summary"]["scenes"]["added"] != ba["summary"]["scenes"]["deleted"] {
        return Err("swapping the drafts does not swap added and deleted".into());
    }
    // Bringing B in.
    let (items, _) = crate::diff::import_edits(&a, FormatId::Fountain, &b, FormatId::Fountain);
    if items.is_empty() {
        return Ok(());
    }
    let mut s = Session::new(a.clone(), None, FormatId::Fountain, SourceStyle::default());
    let batch = Command::Batch(crate::command::Batch {
        commands: items,
        label: None,
    });
    s.execute_with(&batch, &Exec::by(Author::named_writer("B")))
        .map_err(|e| format!("the import did not apply: {e}"))?;
    let all =
        |name: &str| Command::from_parts(name, json!({"all": true})).map_err(|e| e.to_string());
    s.execute(&all("changes.accept")?)
        .map_err(|e| format!("accepting the import failed: {e}"))?;
    // Blank lines and centering are attributes, never tracked, and they
    // change text as it reads back: a block with no blank line above it
    // joins the paragraph before it, and a centered line loses the spaces
    // it starts with. So they are left out here.
    let unspaced = |d: &Document| {
        let mut d = d.clone();
        for block in &mut d.blocks {
            block.element.space_before = None;
            block.element.centered = false;
        }
        d
    };
    let left = result(&unspaced(&s.doc), &unspaced(&b));
    for scene in left["scenes"].as_array().into_iter().flatten() {
        for block in scene["blocks"].as_array().into_iter().flatten() {
            if block["op"] != "restyled" && block["op"] != "attributes" {
                return Err(format!(
                    "after accepting the import, a block is still {}",
                    block["op"]
                ));
            }
        }
    }
    s.execute(&Command::from_parts("undo", json!({})).map_err(|e| e.to_string())?)
        .map_err(|e| format!("undoing the accept failed: {e}"))?;
    s.execute(&all("changes.reject")?)
        .map_err(|e| format!("rejecting the import failed: {e}"))?;
    let elements = |d: &Document| -> Vec<coldopen_screenplay::Element> {
        d.blocks.iter().map(|b| b.element.clone()).collect()
    };
    if elements(&s.doc) != elements(&a) {
        return Err("rejecting the import does not give A back".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review finding F17.2: the journal check now runs real recovery, so it
    /// sees what the old copy of replay could not, such as F9's diverged
    /// entry (with the F9 fix taken out, this test fails).
    #[test]
    fn the_journal_check_runs_real_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.fountain");
        std::fs::write(&p, "INT. HOUSE - DAY\n\nRain.\n").unwrap();
        let mut d = Dispatcher::new(Options {
            journal: true,
            fsync: false,
        });
        d.handle(&json!({"command": "doc.open", "params": {"path": p}}));
        for text in ["Line A.", "Line B."] {
            d.handle(
                &json!({"command": "block.insert", "params": {"type": "action", "text": text}}),
            );
        }
        drop(d);
        let journal = std::fs::read_to_string(crate::journal::path_for(&p)).unwrap();
        assert_eq!(journal_replay(journal.as_bytes()), Ok(()));
        // Entry 2 recorded an id replay will not give.
        let diverged = journal.replace(r#""result":{"id":"b_4"}"#, r#""result":{"id":"b_99"}"#);
        assert_ne!(diverged, journal);
        assert_eq!(journal_replay(diverged.as_bytes()), Ok(()));
    }

    /// `"export\u002epdf"` and `"doc\u002esave"` decode to file commands,
    /// although the raw lines contain neither `export.` nor `doc.`.
    fn escaped_file_lines(dir: &std::path::Path) -> Vec<String> {
        let pdf = serde_json::to_string(&dir.join("escaped.pdf")).unwrap();
        let fountain = serde_json::to_string(&dir.join("escaped.fountain")).unwrap();
        vec![
            format!(r#"{{"command":"export\u002epdf","params":{{"path":{pdf}}}}}"#),
            format!(r#"{{"command":"doc\u002esave","params":{{"path":{fountain}}}}}"#),
        ]
    }

    #[test]
    fn escaped_file_commands_are_skipped() {
        for line in escaped_file_lines(std::path::Path::new("x")) {
            assert!(
                !line.contains("export.") && !line.contains("doc."),
                "{line}"
            );
            assert!(reaches_disk(&line), "{line}");
        }
    }

    #[test]
    fn fuzzing_an_escaped_file_command_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let data = escaped_file_lines(dir.path()).join("\n");
        assert_eq!(envelope(data.as_bytes()), Ok(()));
        let written: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(written.is_empty(), "fuzzing wrote {written:?}");
    }

    #[test]
    fn edits_queries_and_refused_requests_still_run() {
        for line in [
            r#"{"command":"block\u002einsert","params":{"type":"action","text":"A"}}"#,
            r#"{"command":"doc.get"}"#,
            r#"{"command":"doc.export","params":{"format":"pages"}}"#,
            r#"{"command":"undo","params":null}"#,
            // Refused before anything runs: bad params, unknown name, not JSON.
            r#"{"command":"export.pdf","params":{"path":7}}"#,
            r#"{"command":"export.zip"}"#,
            r#"{"command":"#,
        ] {
            assert!(!reaches_disk(line), "{line}");
        }
    }

    #[test]
    fn the_bundle_and_change_checks_pass_on_simple_input() {
        assert_eq!(bundle_read(b""), Ok(()));
        assert_eq!(bundle_read(b"PK\x03\x04 not really"), Ok(()));
        let doc =
            crate::document::Document::from_script(coldopen_fountain::parse(CHANGES_SCRIPT).script);
        assert_eq!(bundle_read(&crate::bundle::write(&doc)), Ok(()));
        // An agent replaces words, the writer splits and joins, then
        // settles by scene and all, with undos between.
        let ops: Vec<u8> = vec![
            2, 1, 0, 0, 7, 0, 1, 3, 8, 0, 2, 0, 10, 0, 0, 1, 12, 0, 0, 0, 11, 2, 0, 2, 13, 0, 0, 0,
            5, 1, 4, 0, 9, 2, 1, 0, 3, 1, 5, 7,
        ];
        assert_eq!(changes(&ops), Ok(()));
        assert_eq!(changes(&[]), Ok(()));
        // Start a set, type, delete, end it, clear it, with undos.
        let ops: Vec<u8> = vec![
            16, 0, 0, 0, 0, 0, 1, 3, 1, 0, 3, 1, 17, 0, 0, 0, 6, 0, 2, 1, 16, 0, 0, 0, 9, 0, 1, 0,
            18, 0, 0, 0, 12, 0, 0, 0, 19, 0, 0, 0,
        ];
        assert_eq!(revisions(&ops), Ok(()));
        assert_eq!(revisions(&[]), Ok(()));
        assert_eq!(diff(b""), Ok(()));
        let drafts = b"INT. LAB - NIGHT\n\nRain.\n\nEXT. ROOF - NIGHT\n\nWind.\n\0EXT. ROOF - NIGHT\n\nWind howls.\n\nINT. LAB - NIGHT\n\nRain.\n\nINT. CAR - DAY\n\nWipers.\n";
        assert_eq!(diff(drafts), Ok(()));
    }

    #[test]
    fn file_commands_are_skipped_even_when_the_envelope_is_refused() {
        for line in [
            r#"{"v":2,"command":"export.pdf","params":{"path":"x.pdf"}}"#,
            r#"{"command":"doc.save","params":{"path":"x.fountain"},"extra":1}"#,
            r#"{"command":"doc.close"}"#,
        ] {
            assert!(reaches_disk(line), "{line}");
        }
    }
}
