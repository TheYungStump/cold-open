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
    assert_eq!(
        h.app.agent_status().unwrap(),
        "gate-agent connected · 1 edit"
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
    let mut mcp = Mcp::start(home.path(), &["--doc", script.to_str().unwrap()]);
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
