//! The `coldopen` binary end to end: subcommands, the stdio server, and a
//! crash test that kills the server mid-session.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use coldopen_core::{Dispatcher, Options};
use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_coldopen");

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fountain")
        .join(name)
}

/// A temp copy of the full-scene fixture.
fn script(dir: &Path) -> PathBuf {
    let p = dir.join("scene.fountain");
    fs::copy(fixture("z01-short-scene.fountain"), &p).unwrap();
    p
}

fn coldopen(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn convert_to_fountain_and_json() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture("ct02-inner-spaces.fountain");
    let out = dir.path().join("out.fountain");
    let o = coldopen(&["convert", s(&input), s(&out)]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        fs::read_to_string(&out).unwrap(),
        fs::read_to_string(fixture("ct02-inner-spaces.out.fountain")).unwrap()
    );
    // Converting reads the input only: no journal is left next to it.
    assert!(fs::read_dir(input.parent().unwrap()).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .contains("coldopen-journal")
    }));

    let o = coldopen(&[
        "convert",
        s(&fixture("z01-short-scene.fountain")),
        "-",
        "--to",
        "json",
    ]);
    assert!(o.status.success());
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(
        v["blocks"][0],
        json!({"id": "b_1", "type": "section", "text": "Act One", "depth": 1})
    );
    assert_eq!(v["title_page"]["fields"][0]["key"], "Title");

    let o = coldopen(&["convert", s(&dir.path().join("missing.fountain")), "-"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("io_error"));
}

#[test]
fn scenes_table_and_json() {
    let o = coldopen(&["scenes", s(&fixture("z01-short-scene.fountain"))]);
    assert!(o.status.success());
    let text = String::from_utf8(o.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0],
        "   1  1    p. 1     3/8  EXT. LIGHTHOUSE - DUSK  (15 blocks, 39 words)"
    );
    assert!(lines[1].starts_with("   2  2    p. 1     2/8  INT. RADIO ROOM - NIGHT"));

    let o = coldopen(&["scenes", "--json", s(&fixture("z01-short-scene.fountain"))]);
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["scenes"][1]["number"], "2");
    assert_eq!(v["scenes"][1]["page"], 1);
    assert_eq!(v["scenes"][1]["eighths"], 2);
}

#[test]
fn export_and_pages() {
    let dir = tempfile::tempdir().unwrap();
    let input = script(dir.path());
    let out = dir.path().join("out.pdf");
    let o = coldopen(&["export", s(&input), s(&out)]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let first = fs::read(&out).unwrap();
    assert!(first.starts_with(b"%PDF-"));
    // Exporting again gives the same bytes.
    let o = coldopen(&["export", s(&input), s(&out)]);
    assert!(o.status.success());
    assert_eq!(fs::read(&out).unwrap(), first);
    // Options change the output; a watermark and scene numbers both draw.
    let o = coldopen(&[
        "export",
        s(&input),
        s(&out),
        "--watermark",
        "DRAFT",
        "--scene-numbers",
        "--no-title-page",
    ]);
    assert!(o.status.success());
    assert_ne!(fs::read(&out).unwrap(), first);
    // Reading only: no journal or temp file is left next to the input.
    for e in fs::read_dir(dir.path()).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains("coldopen-journal") && !name.contains("coldopen-save"),
            "{name}"
        );
    }
    // A missing glyph is a warning, not a failure.
    let odd = dir.path().join("odd.fountain");
    fs::write(&odd, "Snow falls on 日本.\n").unwrap();
    let o = coldopen(&["export", s(&odd), s(&out)]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("日本"));

    let o = coldopen(&["pages", s(&input)]);
    assert!(o.status.success());
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(text.starts_with("=== page 1 · title ===\n"), "{text}");
    assert!(
        text.contains("=== page 2 · body 1 · explicit ===\n"),
        "{text}"
    );
    assert!(
        text.contains("               EXT. LIGHTHOUSE - DUSK\n"),
        "{text}"
    );
}

#[test]
fn convert_refuses_pdf_and_export_refuses_other_paths() {
    let dir = tempfile::tempdir().unwrap();
    let input = script(dir.path());
    let o = coldopen(&["convert", s(&input), s(&dir.path().join("x.pdf"))]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("coldopen export"));
    let o = coldopen(&["export", s(&input), s(&dir.path().join("x.fountain"))]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("unsupported_format"));
}

#[test]
fn licenses_include_the_font_license() {
    let o = coldopen(&["licenses"]);
    assert!(o.status.success());
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(text.contains("GNU General Public License"));
    assert!(text.contains("SIL OPEN FONT LICENSE Version 1.1"));
    assert!(text.contains("Courier Prime"));
    assert!(text.contains("Atkinson Hyperlegible Next Project Authors"));
}

#[test]
fn run_a_batch() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let edits = dir.path().join("edits.json");
    fs::write(
        &edits,
        json!([
            {"id": 1, "command": "text.replace", "params": {"find": "MAYA", "with": "NINA"}},
            {"id": 2, "command": "block.insert", "params": {"type": "action", "text": "Fade out."}},
        ])
        .to_string(),
    )
    .unwrap();
    let o = coldopen(&["run", s(&edits), "--doc", s(&p), "--save"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let responses: Vec<Value> = String::from_utf8(o.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(responses.len(), 4);
    assert!(responses.iter().all(|r| r["ok"] == true));
    assert_eq!(responses[1]["result"]["count"], 3);
    let saved = fs::read_to_string(&p).unwrap();
    assert!(saved.contains("NINA\n(to herself)"));
    assert!(saved.ends_with("THE END\n\nFade out.\n"));
    assert!(!coldopen_core::journal::path_for(&p).exists());

    // NDJSON input; the first failure stops the batch with exit code 1.
    fs::write(
        &edits,
        "{\"command\":\"block.delete\",\"params\":{\"id\":\"b_999\"}}\n{\"command\":\"undo\"}\n",
    )
    .unwrap();
    let o = coldopen(&["run", s(&edits), "--doc", s(&p)]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(String::from_utf8(o.stdout).unwrap().lines().count(), 2);
    let o = coldopen(&["run", s(&edits), "--doc", s(&p), "--keep-going"]);
    assert_eq!(String::from_utf8(o.stdout).unwrap().lines().count(), 3);

    // Unsaved changes at the end of a batch are discarded, not journaled.
    fs::write(
        &edits,
        "{\"command\":\"block.insert\",\"params\":{\"type\":\"action\",\"text\":\"x\"}}\n",
    )
    .unwrap();
    let o = coldopen(&["run", s(&edits), "--doc", s(&p)]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("discarded"));
    assert!(!coldopen_core::journal::path_for(&p).exists());
}

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn start() -> Server {
        let mut child = Command::new(BIN)
            .args(["serve", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, line: &str) {
        let w = self.stdin.as_mut().unwrap();
        writeln!(w, "{line}").unwrap();
        w.flush().unwrap();
    }

    fn recv(&mut self) -> Option<Value> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(serde_json::from_str(&line).unwrap()),
        }
    }

    fn call(&mut self, command: &str, params: Value) -> Value {
        self.send(&json!({ "command": command, "params": params }).to_string());
        self.recv().unwrap()
    }
}

#[test]
fn serve_stdio_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut srv = Server::start();
    // Windows PowerShell begins what it pipes with a byte-order mark.
    srv.send("\u{feff}{\"id\": 0, \"command\": \"scene.list\"}");
    let r = srv.recv().unwrap();
    assert_eq!(r["id"], 0);
    assert_eq!(r["error"]["code"], "no_document", "{r}");
    srv.send("this is not json");
    let r = srv.recv().unwrap();
    assert_eq!(
        (r["ok"].clone(), r["error"]["code"].clone()),
        (json!(false), json!("invalid_request"))
    );
    srv.send(
        &json!({"v": 1, "id": "open", "command": "doc.open", "params": {"path": s(&p)}})
            .to_string(),
    );
    let r = srv.recv().unwrap();
    assert_eq!(r["id"], "open");
    assert_eq!(r["result"]["scenes"], 2);
    let r = srv.call("block.insert", json!({"type": "action", "text": "More."}));
    assert_eq!(r["result"]["id"], "b_23");
    let r = srv.call("doc.save", json!({}));
    assert_eq!(r["ok"], true);
    // Closing stdin ends the server cleanly; the saved document leaves no journal.
    drop(srv.stdin.take());
    assert!(srv.child.wait().unwrap().success());
    assert!(!coldopen_core::journal::path_for(&p).exists());
    assert!(
        fs::read_to_string(&p)
            .unwrap()
            .ends_with("THE END\n\nMore.\n")
    );
}

#[test]
fn serve_keeps_unsaved_work_for_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let mut srv = Server::start();
    srv.call("doc.open", json!({"path": s(&p)}));
    srv.call(
        "block.insert",
        json!({"type": "action", "text": "Unsaved."}),
    );
    drop(srv.stdin.take());
    assert!(srv.child.wait().unwrap().success());
    assert!(coldopen_core::journal::path_for(&p).exists());
    let mut srv = Server::start();
    let r = srv.call("doc.open", json!({"path": s(&p)}));
    assert_eq!(r["result"]["recovered"]["commands"], 1);
}

/// Leaves `s.fountain` with a crash journal holding one unsaved line, as
/// the app or `serve --stdio` would after a crash.
fn unsaved_in_journal(dir: &Path, line: &str) -> PathBuf {
    let p = dir.join("s.fountain");
    fs::write(&p, "INT. HOUSE - DAY\n\nRain.\n").unwrap();
    let mut srv = Server::start();
    srv.call("doc.open", json!({"path": s(&p)}));
    srv.call("block.insert", json!({"type": "action", "text": line}));
    drop(srv.stdin.take());
    assert!(srv.child.wait().unwrap().success());
    assert!(coldopen_core::journal::path_for(&p).exists());
    p
}

fn recovered_text(p: &Path) -> String {
    let mut d = Dispatcher::new(Options::default());
    let r = d.handle(&json!({"command": "doc.open", "params": {"path": s(p)}}));
    assert_eq!(r["ok"], true, "{r}");
    let t = d.handle(&json!({"command": "doc.export"}))["result"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    d.shutdown();
    t
}

/// Review finding F1: a read-only batch on a script with a crash journal
/// recovered the journal, then deleted it as "unsaved changes".
#[test]
fn run_never_discards_recovered_work() {
    const WORK: &str = "HOURS OF UNSAVED WORK.";
    let dir = tempfile::tempdir().unwrap();
    let p = unsaved_in_journal(dir.path(), WORK);
    let q = dir.path().join("q.json");

    // A query.
    fs::write(&q, r#"{"id":"q","command":"scene.list"}"#).unwrap();
    let o = coldopen(&["run", s(&q), "--doc", s(&p)]);
    assert!(o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stdout).contains(r#""recovered":{"commands":1"#),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("crash journal"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(coldopen_core::journal::path_for(&p).exists());

    // A batch whose first command fails.
    fs::write(&q, r#"{"command":"block.delete","params":{"id":"b_999"}}"#).unwrap();
    let o = coldopen(&["run", s(&q), "--doc", s(&p)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(coldopen_core::journal::path_for(&p).exists());
    assert!(recovered_text(&p).contains(WORK));
}

/// A batch that edits on top of recovered work cannot keep its own edits,
/// so the whole journal is kept aside, and the copy is named.
#[test]
fn run_keeps_a_copy_when_it_edits_recovered_work() {
    const WORK: &str = "HOURS OF UNSAVED WORK.";
    let dir = tempfile::tempdir().unwrap();
    let p = unsaved_in_journal(dir.path(), WORK);
    let q = dir.path().join("q.json");
    fs::write(
        &q,
        r#"{"command":"block.insert","params":{"type":"action","text":"From the batch."}}"#,
    )
    .unwrap();
    let o = coldopen(&["run", s(&q), "--doc", s(&p)]);
    assert!(o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains(".discarded-"), "{err}");
    let kept: Vec<PathBuf> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().contains(".discarded-"))
        .collect();
    assert_eq!(kept.len(), 1);
    assert!(fs::read_to_string(&kept[0]).unwrap().contains(WORK));
}

const EDITS: usize = 500;

fn edit(k: usize) -> Value {
    // A mix of commands so replay exercises more than inserts.
    match k % 5 {
        3 => {
            json!({"id": k, "command": "text.replace", "params": {"find": format!("Edit {}", k - 1), "with": format!("Edited {}", k - 1)}})
        }
        4 => json!({"id": k, "command": "undo"}),
        _ => {
            json!({"id": k, "command": "block.insert", "params": {"type": "action", "text": format!("Edit {k}")}})
        }
    }
}

/// The document after applying the first `n` edits, computed in-process.
fn expected_after(script: &Path, n: usize) -> Value {
    let mut d = Dispatcher::new(Options {
        journal: false,
        fsync: false,
    });
    let r = d.handle(&json!({"command": "doc.open", "params": {"path": s(script)}}));
    assert_eq!(r["ok"], true);
    for k in 0..n {
        d.handle(&edit(k));
    }
    d.handle(&json!({"command": "doc.get"}))["result"]["blocks"].clone()
}

/// Streams edits to a server, kills it after `kill_after` responses, then
/// checks that a new server recovers every acknowledged edit, in order.
fn crash_and_recover(kill_after: usize) {
    let dir = tempfile::tempdir().unwrap();
    let p = script(dir.path());
    let pristine = dir.path().join("pristine.fountain");
    fs::copy(&p, &pristine).unwrap();

    let mut srv = Server::start();
    assert_eq!(srv.call("doc.open", json!({"path": s(&p)}))["ok"], true);

    // Write every edit from another thread so requests are still in flight
    // when the process dies.
    let mut stdin = srv.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        for k in 0..EDITS {
            if writeln!(stdin, "{}", edit(k)).is_err() {
                break;
            }
        }
        let _ = stdin.flush();
        stdin
    });
    let mut acked = 0;
    while acked < kill_after {
        let r = srv.recv().expect("server stopped early");
        assert_eq!(r["id"], acked, "responses arrive in order");
        acked += 1;
    }
    srv.child.kill().unwrap();
    let _ = srv.child.wait();
    let _ = writer.join();

    let mut srv = Server::start();
    let r = srv.call("doc.open", json!({"path": s(&p)}));
    assert_eq!(r["ok"], true, "{r}");
    let recovered = r["result"]["recovered"]["commands"].as_u64().unwrap() as usize;
    assert!(
        recovered >= kill_after,
        "every acknowledged edit is recovered: {recovered} < {kill_after}"
    );
    assert!(recovered <= EDITS);
    eprintln!("killed after {kill_after} acknowledged edits; recovered {recovered} of {EDITS}");
    let blocks = srv.call("doc.get", json!({}))["result"]["blocks"].clone();
    assert_eq!(
        blocks,
        expected_after(&pristine, recovered),
        "the recovered edits are an exact prefix"
    );
    let _ = srv.child.kill();
}

#[test]
fn crash_mid_session_recovers_every_acknowledged_edit() {
    crash_and_recover(EDITS / 3);
}

#[test]
fn crash_after_the_last_edit_recovers_all() {
    crash_and_recover(EDITS);
}

fn fdx_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fdx")
        .join(name)
}

/// `coldopen repair` (docs/specs/fdx.md §17): a repaired copy and a report,
/// never the input.
#[test]
fn repair_writes_a_copy_and_a_report() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("broken.fdx");
    fs::copy(fdx_fixture("broken-markup.fdx"), &input).unwrap();
    let before = fs::read(&input).unwrap();
    let out = dir.path().join("fixed.fdx");
    let report = dir.path().join("report.json");
    let o = coldopen(&["repair", s(&input), s(&out)]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.starts_with("Repaired, with "), "{text}");
    assert!(text.contains("(skipped_bytes)"), "{text}");
    assert_eq!(fs::read(&input).unwrap(), before);
    assert_eq!(
        fs::read(&out).unwrap(),
        fs::read(fdx_fixture("broken-markup.repaired.out")).unwrap()
    );
    let o = coldopen(&[
        "repair",
        s(&input),
        s(&out),
        "--json",
        "--report",
        s(&report),
    ]);
    assert!(o.status.success());
    let v: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_eq!(v["repaired"], true);
    assert!(
        v["issues"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["class"].is_string())
    );
    let o = coldopen(&["repair", s(&input), s(&input)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("would_overwrite_input"));
    assert_eq!(fs::read(&input).unwrap(), before);
}

#[test]
fn convert_to_and_from_fdx() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("scene.fdx");
    let o = coldopen(&[
        "convert",
        s(&fixture("ct02-inner-spaces.fountain")),
        s(&out),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(fs::read_to_string(&out).unwrap().contains("<FinalDraft"));
    let back = dir.path().join("back.fountain");
    let o = coldopen(&["convert", s(&fdx_fixture("settings.fdx")), s(&back)]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "an .fdx's settings are lost in Fountain"
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("--allow-lossy"));
    let o = coldopen(&[
        "convert",
        s(&fdx_fixture("settings.fdx")),
        s(&back),
        "--allow-lossy",
    ]);
    assert!(o.status.success());
}
