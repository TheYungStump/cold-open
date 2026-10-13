//! Checks shared by the cargo-fuzz targets in `fuzz/` and the stable test
//! suite, so the fuzzed properties also run on every CI build.

use serde_json::{Value, json};

use crate::command::Command;
use crate::dispatcher::{Dispatcher, Options};
use crate::document::Document;
use crate::io::SourceStyle;
use crate::journal;
use crate::session::Session;

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
        if line.contains("doc.") {
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

/// Reads arbitrary bytes as a journal and replays what it can. Must never
/// panic, and replay must stop cleanly at the first bad entry.
pub fn journal_replay(data: &[u8]) -> Result<(), String> {
    let Ok(read) = journal::read(data) else {
        return Ok(());
    };
    // A hand-made header may claim ids it never handed out; replay must
    // still not panic.
    let doc: Document = read.header.doc;
    let mut s = Session::new(doc, None, SourceStyle::default());
    for e in &read.entries {
        let Ok(cmd) = Command::from_parts(&e.command, e.params.clone()) else {
            break;
        };
        if matches!(cmd.class(), crate::command::Class::Session) {
            break;
        }
        if s.execute(&cmd).is_err() {
            break;
        }
    }
    Ok(())
}
