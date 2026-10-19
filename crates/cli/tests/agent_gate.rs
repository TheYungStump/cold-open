//! The phase 3 agent gate: a real `coldopen mcp` process, driven over MCP
//! (JSON-RPC on its standard input and output), attaches to a script open
//! in the desktop app, reads the scene list, and edits the script live. The
//! edit is labelled with the agent's name and tool, the writer can undo it,
//! and the agent can't undo the writer's work.
//!
//! The app runs headless in this test process, with the real update loop
//! and agent link; `coldopen mcp` finds it through its session file.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot};
use serde_json::{Value, json};

const SCRIPT: &str = "INT. LIGHTHOUSE - NIGHT\n\nThe lamp turns.\n\nEXT. CLIFF - DAWN\n\nGulls.\n";

struct Mcp {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Value>,
    next: u64,
    /// Notifications seen while waiting for responses.
    notes: Vec<Value>,
}

impl Mcp {
    fn start(home: &Path, extra: &[&str]) -> Mcp {
        let mut child = Command::new(env!("CARGO_BIN_EXE_coldopen"))
            .arg("mcp")
            .args(extra)
            .env("COLDOPEN_HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(v) = serde_json::from_str::<Value>(&line)
                    && tx.send(v).is_err()
                {
                    break;
                }
            }
        });
        Mcp {
            child,
            stdin,
            lines: rx,
            next: 1,
            notes: Vec::new(),
        }
    }

    fn send(&mut self, v: &Value) {
        writeln!(self.stdin, "{v}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Sends a request and waits for its response, keeping the app running
    /// meanwhile so it can answer requests from the link.
    fn request(&mut self, h: &mut Headless, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let start = Instant::now();
        loop {
            h.pump();
            match self.lines.recv_timeout(Duration::from_millis(2)) {
                Ok(v) if v["id"] == id => return v,
                Ok(v) => self.notes.push(v),
                Err(_) => {}
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "no answer to {method}"
            );
        }
    }

    fn tool(&mut self, h: &mut Headless, name: &str, args: Value) -> Value {
        let r = self.request(h, "tools/call", json!({"name": name, "arguments": args}));
        r["result"].clone()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn texts(h: &Headless) -> Vec<String> {
    h.app
        .d
        .session()
        .unwrap()
        .doc
        .blocks
        .iter()
        .map(|b| b.element.text.plain_text())
        .collect()
}

#[test]
fn an_agent_through_coldopen_mcp_edits_the_script_open_in_the_app() {
    let home = tempfile::tempdir().unwrap();
    let script = home.path().join("lighthouse.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    let mut app = App::new(Boot {
        file: Some(script.clone()),
        dirs: Some(Dirs::under(home.path())),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    let mut h = Headless::new(app, iced::Size::new(1000.0, 700.0)).unwrap();

    let mut mcp = Mcp::start(home.path(), &[]);
    let init = mcp.request(
        &mut h,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "gate-agent", "version": "1.0"}
        }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "coldopen", "{init}");
    assert!(
        init["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("doc_open")
    );
    mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    // Every command is a tool, with a schema.
    let tools = mcp.request(&mut h, "tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), coldopen_core::Command::NAMES.len());
    assert!(names.contains(&"block_insert") && names.contains(&"scene_list"));

    // doc_open finds the app through its session file and attaches.
    let r = mcp.tool(&mut h, "doc_open", json!({"path": script}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["attached"], true, "{r}");
    assert_eq!(h.app.agents.len(), 1);

    // It reads the scene list from the app...
    let r = mcp.tool(&mut h, "scene_list", json!({}));
    let scenes = r["structuredContent"]["scenes"].as_array().unwrap().clone();
    assert_eq!(scenes.len(), 2);
    assert_eq!(scenes[1]["heading"], "EXT. CLIFF - DAWN");
    // ...and as a resource.
    let res = mcp.request(
        &mut h,
        "resources/read",
        json!({"uri": "coldopen://doc/scenes"}),
    );
    assert!(
        res["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("CLIFF")
    );
    let cliff = scenes[1]["id"].as_str().unwrap().to_string();
    let res = mcp.request(
        &mut h,
        "resources/read",
        json!({"uri": format!("coldopen://scene/{cliff}")}),
    );
    assert_eq!(
        res["result"]["contents"][0]["text"], "EXT. CLIFF - DAWN\n\nGulls.\n",
        "{res}"
    );

    // It edits: the writer's window has the new block at once.
    let r = mcp.tool(
        &mut h,
        "block_insert",
        json!({"type": "action", "text": "A ship's horn, far off."}),
    );
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(texts(&h).last().unwrap(), "A ship's horn, far off.");
    let session = h.app.d.session().unwrap();
    let (_, label, by) = session.undo_steps().next().unwrap();
    assert_eq!(label, "gate-agent · block_insert");
    assert!(!by.is_writer());
    // The edit is a tracked change waiting for the writer (editor.md §12).
    assert_eq!(
        h.app.agent_status().unwrap(),
        "gate-agent connected · 1 edit · 1 to review"
    );

    // Refused while attached, with the reason in the tool result.
    let r = mcp.tool(
        &mut h,
        "doc_save",
        json!({"path": home.path().join("x.fountain")}),
    );
    assert_eq!(r["isError"], true);
    assert_eq!(r["structuredContent"]["code"], "not_allowed_attached");

    // The writer types; the agent can't undo that.
    h.named(
        iced::keyboard::key::Named::ArrowUp,
        iced::keyboard::Modifiers::CTRL,
    );
    h.type_text("X");
    let r = mcp.tool(&mut h, "undo", json!({}));
    assert_eq!(r["structuredContent"]["code"], "not_your_step", "{r}");
    // The writer's Mod+Z: their own typing, then the agent's insert.
    h.shortcut('z', false);
    h.shortcut('z', false);
    assert_eq!(texts(&h).len(), 4);

    // doc_close detaches and leaves the writer's script open.
    let r = mcp.tool(&mut h, "doc_close", json!({}));
    assert_eq!(r["structuredContent"]["detached"], true, "{r}");
    let start = Instant::now();
    while !h.app.agents.is_empty() {
        h.pump();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(h.app.d.session().is_some());
    h.app.d.discard();
}

#[test]
fn without_the_app_coldopen_mcp_opens_the_file_itself() {
    let home = tempfile::tempdir().unwrap();
    let script = home.path().join("cliff.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    // No app: nothing to attach to. The pump below has nothing to do.
    let app = App::new(Boot {
        dirs: Some(Dirs::under(&home.path().join("unused"))),
        ..Boot::default()
    });
    let mut h = Headless::new(app, iced::Size::new(800.0, 600.0)).unwrap();
    // Agent edits in a .fountain file it opens itself can be saved there
    // only untracked: the person running the server chose that.
    let mut mcp = Mcp::start(
        home.path(),
        &["--doc", script.to_str().unwrap(), "--untracked"],
    );
    mcp.request(
        &mut h,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "solo", "version": "1.0"}
        }),
    );
    mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let r = mcp.tool(
        &mut h,
        "text_replace",
        json!({"find": "Gulls", "with": "Terns"}),
    );
    assert_eq!(r["isError"], false, "{r}");
    let r = mcp.tool(&mut h, "history_list", json!({}));
    assert_eq!(
        r["structuredContent"]["undo"][0]["label"], "solo · text_replace",
        "{r}"
    );
    let r = mcp.tool(&mut h, "doc_save", json!({}));
    assert_eq!(r["isError"], false, "{r}");
    assert!(std::fs::read_to_string(&script).unwrap().contains("Terns."));
    h.app.d.discard();
}

/// Review finding F4: with the app gone after a crash, `coldopen mcp`
/// opens the script itself and recovers the writer's journal. Closing it
/// with `discard` used to delete that work for good.
#[test]
fn an_agent_cannot_discard_the_writers_recovered_work() {
    let home = tempfile::tempdir().unwrap();
    let script = home.path().join("s.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    {
        // The writer's session, which then crashes.
        let mut d = coldopen_core::Dispatcher::new(coldopen_core::Options::default());
        for (command, params) in [
            ("doc.open", json!({"path": script})),
            (
                "block.insert",
                json!({"type": "action", "text": "THE WRITER'S UNSAVED PAGE."}),
            ),
        ] {
            let cmd = coldopen_core::Command::from_parts(command, params).unwrap();
            d.execute(cmd).unwrap();
        }
    }
    let journal = coldopen_core::journal::path_for(&script);
    let before = std::fs::read(&journal).unwrap();

    let app = App::new(Boot {
        dirs: Some(Dirs::under(&home.path().join("unused"))),
        ..Boot::default()
    });
    let mut h = Headless::new(app, iced::Size::new(800.0, 600.0)).unwrap();
    let mut mcp = Mcp::start(home.path(), &[]);
    mcp.request(
        &mut h,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "careless", "version": "1.0"}
        }),
    );
    mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let r = mcp.tool(&mut h, "doc_open", json!({"path": script}));
    assert_eq!(r["structuredContent"]["recovered"]["commands"], 1, "{r}");
    let r = mcp.tool(&mut h, "doc_close", json!({"discard": true}));
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(r["structuredContent"]["code"], "not_your_step", "{r}");
    drop(mcp);
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    h.app.d.discard();
}

/// Review finding C5 (and Antigravity's 5): `coldopen mcp` shut its open
/// document down before it checked a `doc_open`, so a failed open (a path
/// that does not exist) threw away an unsaved untitled script for good,
/// and moved past the unsaved-changes rule for a saved one.
#[test]
fn a_failed_doc_open_keeps_the_open_script() {
    let home = tempfile::tempdir().unwrap();
    let a = home.path().join("a.fountain");
    std::fs::write(&a, SCRIPT).unwrap();
    let app = App::new(Boot {
        dirs: Some(Dirs::under(&home.path().join("unused"))),
        ..Boot::default()
    });
    let mut h = Headless::new(app, iced::Size::new(800.0, 600.0)).unwrap();
    let mut mcp = Mcp::start(home.path(), &["--no-attach", "--untracked"]);
    mcp.request(
        &mut h,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "solo", "version": "1.0"}
        }),
    );
    mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let missing = home.path().join("missing.fountain");

    // Codex's steps: an untitled script, unsaved, then a failed open.
    let r = mcp.tool(&mut h, "doc_new", json!({}));
    assert_eq!(r["isError"], false, "{r}");
    let r = mcp.tool(
        &mut h,
        "block_insert",
        json!({"type": "action", "text": "Unsaved MCP work."}),
    );
    assert_eq!(r["isError"], false, "{r}");
    let r = mcp.tool(&mut h, "doc_open", json!({"path": missing}));
    assert_eq!(r["isError"], true, "{r}");
    let r = mcp.tool(&mut h, "doc_get", json!({}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(
        r["structuredContent"]["blocks"][0]["text"], "Unsaved MCP work.",
        "{r}"
    );
    let r = mcp.tool(&mut h, "doc_close", json!({"discard": true}));
    assert_eq!(r["isError"], false, "{r}");

    // Antigravity's steps: a saved script with unsaved edits.
    let r = mcp.tool(&mut h, "doc_open", json!({"path": a}));
    assert_eq!(r["isError"], false, "{r}");
    mcp.tool(
        &mut h,
        "block_insert",
        json!({"type": "action", "text": "Edit."}),
    );
    let r = mcp.tool(&mut h, "doc_open", json!({"path": missing}));
    assert_eq!(r["structuredContent"]["code"], "unsaved_changes", "{r}");
    let r = mcp.tool(&mut h, "doc_get", json!({}));
    assert!(
        r["structuredContent"]["path"]
            .as_str()
            .unwrap()
            .ends_with("a.fountain"),
        "{r}"
    );

    // A clean script stays open when the new one cannot be opened.
    mcp.tool(&mut h, "doc_save", json!({}));
    let r = mcp.tool(&mut h, "doc_open", json!({"path": missing}));
    assert_eq!(r["structuredContent"]["code"], "io_error", "{r}");
    let r = mcp.tool(&mut h, "doc_get", json!({}));
    assert!(
        r["structuredContent"]["path"]
            .as_str()
            .unwrap()
            .ends_with("a.fountain"),
        "{r}"
    );
    h.app.d.discard();
}

fn fdx_fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fdx")
        .join(name)
}

/// Phase 4 (docs/specs/editor.md §14): an agent attached to an .fdx script
/// open in the app saves it in place as .fdx, writes copies as .fdx and as
/// Fountain to new paths, repairs a damaged file into a new one, and lists
/// recovered versions; replacing a file stays the writer's to decide.
#[test]
fn an_agent_saves_an_fdx_script_and_writes_copies() {
    let home = tempfile::tempdir().unwrap();
    let script = home.path().join("basic.fdx");
    std::fs::copy(fdx_fixture("basic.fdx"), &script).unwrap();
    let mut app = App::new(Boot {
        file: Some(script.clone()),
        dirs: Some(Dirs::under(home.path())),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    // The writer turns agent tracking off for this script, so the agent's
    // edit can be saved to .fdx, which cannot hold tracked changes.
    app.d
        .execute(
            coldopen_core::Command::from_parts("changes.track", json!({"agents": false})).unwrap(),
        )
        .unwrap();
    let mut h = Headless::new(app, iced::Size::new(1000.0, 700.0)).unwrap();
    let mut mcp = Mcp::start(home.path(), &[]);
    mcp.request(
        &mut h,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "gate-agent", "version": "1.0"}
        }),
    );
    mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let r = mcp.tool(&mut h, "doc_open", json!({"path": script}));
    assert_eq!(r["structuredContent"]["attached"], true, "{r}");
    let r = mcp.tool(&mut h, "doc_get", json!({}));
    assert_eq!(r["structuredContent"]["format"], "fdx", "{r}");

    // An edit, saved in place: still an .fdx file, with the edit in it.
    let r = mcp.tool(
        &mut h,
        "block_insert",
        json!({"type": "action", "text": "The kettle whistles at last."}),
    );
    assert_eq!(r["isError"], false, "{r}");
    let r = mcp.tool(&mut h, "doc_save", json!({}));
    assert_eq!(r["isError"], false, "{r}");
    assert!(!h.app.d.session().unwrap().dirty);
    let saved = std::fs::read_to_string(&script).unwrap();
    assert!(saved.starts_with("<?xml"));
    assert!(saved.contains("The kettle whistles at last."));

    // Copies to new paths; the script stays as it is.
    let copy = home.path().join("copy.fdx");
    let r = mcp.tool(&mut h, "export_fdx", json!({"path": copy}));
    assert_eq!(r["isError"], false, "{r}");
    assert!(
        std::fs::read_to_string(&copy)
            .unwrap()
            .contains("The kettle whistles")
    );
    // As Fountain, what Fountain can't hold is reported, and accepting
    // the losses is the writer's call.
    let fountain = home.path().join("copy.fountain");
    let r = mcp.tool(&mut h, "export_fountain", json!({"path": fountain}));
    if r["isError"] == true {
        assert_eq!(r["structuredContent"]["code"], "lossy_format", "{r}");
        let r = mcp.tool(
            &mut h,
            "export_fountain",
            json!({"path": fountain, "allow_lossy": true}),
        );
        assert_eq!(r["structuredContent"]["code"], "writer_only", "{r}");
        assert!(!fountain.exists());
    } else {
        assert!(
            std::fs::read_to_string(&fountain)
                .unwrap()
                .contains("The kettle whistles")
        );
    }
    let s = h.app.d.session().unwrap();
    assert_eq!(s.path.as_deref(), Some(script.as_path()));
    assert_eq!(s.format, coldopen_core::FormatId::Fdx);
    // Replacing a file is the writer's call.
    let r = mcp.tool(
        &mut h,
        "export_fdx",
        json!({"path": copy, "overwrite": true}),
    );
    assert_eq!(r["structuredContent"]["code"], "writer_only", "{r}");

    // A damaged file, repaired into a new one; the input is never written.
    let broken = home.path().join("broken.fdx");
    std::fs::copy(fdx_fixture("broken-markup.fdx"), &broken).unwrap();
    let before = std::fs::read(&broken).unwrap();
    let out = home.path().join("mended.fdx");
    let r = mcp.tool(&mut h, "file_repair", json!({"path": broken, "out": out}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["repaired"], true, "{r}");
    assert!(out.exists());
    assert_eq!(std::fs::read(&broken).unwrap(), before);

    // Recovered versions: listed, but restoring is the writer's.
    let r = mcp.tool(&mut h, "recovered_list", json!({}));
    assert_eq!(r["isError"], false, "{r}");
    h.app.d.discard();
}

fn initialize(mcp: &mut Mcp, h: &mut Headless, name: &str) {
    mcp.request(
        h,
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": name, "version": "1.0"}
        }),
    );
    mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
}

fn writer_run(h: &mut Headless, command: &str, params: Value) -> Value {
    let cmd = coldopen_core::Command::from_parts(command, params).unwrap();
    h.app
        .d
        .execute(cmd)
        .unwrap_or_else(|e| panic!("{command}: {e}"))
}

/// Phase 5a's gate: through a real `coldopen mcp` attached to the app, an
/// agent's edits arrive as tracked changes under its name; the writer
/// settles them, and the agent can settle none of the writer's.
#[test]
fn through_coldopen_mcp_agent_edits_arrive_as_tracked_changes() {
    let home = tempfile::tempdir().unwrap();
    let script = home.path().join("lighthouse.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    let mut app = App::new(Boot {
        file: Some(script.clone()),
        dirs: Some(Dirs::under(home.path())),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    let mut h = Headless::new(app, iced::Size::new(1000.0, 700.0)).unwrap();
    let mut mcp = Mcp::start(home.path(), &[]);
    initialize(&mut mcp, &mut h, "gate-agent");
    let r = mcp.tool(&mut h, "doc_open", json!({"path": script}));
    assert_eq!(r["structuredContent"]["attached"], true, "{r}");

    let r = mcp.tool(
        &mut h,
        "text_replace",
        json!({"find": "Gulls", "with": "Terns"}),
    );
    assert_eq!(r["structuredContent"]["change"], "c_1", "{r}");
    assert_eq!(texts(&h)[3], "Terns.");
    let list = mcp.tool(&mut h, "changes_list", json!({}));
    assert_eq!(
        list["structuredContent"]["changes"][0]["by"],
        json!({"kind": "agent", "name": "gate-agent"}),
        "{list}"
    );
    // The changes resource says the same.
    let res = mcp.request(
        &mut h,
        "resources/read",
        json!({"uri": "coldopen://doc/changes"}),
    );
    assert!(
        res["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("gate-agent"),
        "{res}"
    );
    // The agent cannot accept, nor opt out.
    let r = mcp.tool(&mut h, "changes_accept", json!({"all": true}));
    assert_eq!(r["structuredContent"]["code"], "writer_only", "{r}");
    let r = mcp.tool(&mut h, "changes_track", json!({"agents": false}));
    assert_eq!(r["structuredContent"]["code"], "writer_only", "{r}");

    // The writer's own tracked change is out of the agent's reach.
    writer_run(&mut h, "changes.track", json!({"writer": true}));
    writer_run(
        &mut h,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "Slowly, "}),
    );
    let r = mcp.tool(&mut h, "changes_reject", json!({"id": "c_2"}));
    assert_eq!(r["structuredContent"]["code"], "not_your_change", "{r}");
    // The writer rejects the agent's change; then the agent proposes again
    // and withdraws it itself.
    writer_run(&mut h, "changes.reject", json!({"id": "c_1"}));
    assert_eq!(texts(&h)[3], "Gulls.");
    mcp.tool(
        &mut h,
        "block_insert",
        json!({"type": "action", "text": "A ship."}),
    );
    let r = mcp.tool(&mut h, "changes_reject", json!({"all": true}));
    assert_eq!(r["structuredContent"]["count"], 1, "{r}");
    assert!(!texts(&h).iter().any(|t| t == "A ship."));
    // What is left is the writer's, which the writer accepts.
    writer_run(&mut h, "changes.accept", json!({"all": true}));
    assert_eq!(texts(&h)[1], "Slowly, The lamp turns.");
    let list = mcp.tool(&mut h, "changes_list", json!({}));
    assert_eq!(list["structuredContent"]["changes"], json!([]));
    mcp.tool(&mut h, "doc_close", json!({}));
    h.app.d.discard();
}

/// Without the app, `coldopen mcp` tracks the agent's edits too: a
/// Fountain file cannot hold them, so the agent saves them in a bundle,
/// where the writer finds them under its name.
#[test]
fn without_the_app_an_agents_tracked_edits_go_into_a_bundle() {
    let home = tempfile::tempdir().unwrap();
    let script = home.path().join("solo.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    let app = App::new(Boot {
        dirs: Some(Dirs::under(home.path())),
        ..Boot::default()
    });
    let mut h = Headless::new(app, iced::Size::new(800.0, 600.0)).unwrap();
    let mut mcp = Mcp::start(home.path(), &["--doc", script.to_str().unwrap()]);
    initialize(&mut mcp, &mut h, "solo");
    let r = mcp.tool(
        &mut h,
        "text_replace",
        json!({"find": "Gulls", "with": "Terns"}),
    );
    assert_eq!(r["structuredContent"]["change"], "c_1", "{r}");
    let r = mcp.tool(&mut h, "doc_save", json!({}));
    assert_eq!(r["structuredContent"]["code"], "lossy_format", "{r}");
    let bundle = home.path().join("solo.coldopen");
    let r = mcp.tool(&mut h, "doc_save", json!({"path": bundle}));
    assert_eq!(r["isError"], false, "{r}");
    mcp.tool(&mut h, "doc_close", json!({}));
    drop(mcp);
    // The Fountain file is as it was; the bundle holds the change.
    assert_eq!(std::fs::read_to_string(&script).unwrap(), SCRIPT);
    let mut d = coldopen_core::Dispatcher::new(coldopen_core::Options::default());
    d.execute(coldopen_core::Command::from_parts("doc.open", json!({"path": bundle})).unwrap())
        .unwrap();
    let list = d
        .execute(coldopen_core::Command::from_parts("changes.list", json!({})).unwrap())
        .unwrap();
    assert_eq!(list["changes"][0]["by"]["name"], "solo", "{list}");
    d.shutdown();
}
