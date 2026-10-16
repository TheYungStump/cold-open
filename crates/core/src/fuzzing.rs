//! Checks shared by the cargo-fuzz targets in `fuzz/` and the stable test
//! suite, so the fuzzed properties also run on every CI build.

use serde_json::{Value, json};

use crate::command::{Class, Command};
use crate::dispatcher::{Dispatcher, Options};

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
        .is_ok_and(|cmd| matches!(cmd.class(), Class::Session | Class::Output))
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
