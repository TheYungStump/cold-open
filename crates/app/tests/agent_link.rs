//! The app's side of the agent link (docs/specs/mcp.md §5), driven by the
//! same client `coldopen mcp` uses: the session file, the handshake and its
//! refusals, requests run as labelled agent steps the writer can undo,
//! `not_your_step`, refused commands, `changed` and `closed` events,
//! margin marks, and the off switch.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot, Message};
use coldopen_link::{AttachError, Client, LinkClient, SessionFile};
use serde_json::{Value, json};

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain.\n";

fn app(home: &Path) -> (Headless, PathBuf) {
    let p = home.join("lab.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    let mut app = App::new(Boot {
        file: Some(p.clone()),
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    let h = Headless::new(app, iced::Size::new(1000.0, 700.0)).expect("a headless renderer");
    (h, p)
}

fn session(home: &Path) -> Option<SessionFile> {
    let run = Dirs::under(home).run;
    std::fs::read_dir(&run)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .and_then(|p| SessionFile::read(&p))
}

fn client(name: &str) -> Client {
    Client {
        name: name.into(),
        version: Some("1.0".into()),
    }
}

/// Keeps the app running until `rx` has the response.
fn answer(h: &mut Headless, rx: Receiver<Value>) -> Value {
    let start = Instant::now();
    loop {
        h.pump();
        if let Ok(v) = rx.try_recv() {
            return v;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "no answer");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn call(h: &mut Headless, l: &LinkClient, command: &str, params: Value, tool: &str) -> Value {
    let rx = l.request(command, params, Some(tool)).unwrap();
    answer(h, rx)
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
fn an_agent_edits_the_open_script_live_and_the_writer_can_undo_it() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, script) = app(home.path());
    let s = session(home.path()).expect("a session file while a script is open");
    assert_eq!(s.pid, std::process::id());
    assert_eq!(s.token.len(), 64);
    assert!(s.has_doc(&script));

    // Refusals first: a wrong token, then a script that isn't open.
    let mut bad = s.clone();
    bad.token = "0".repeat(64);
    match LinkClient::attach(&bad, &client("x"), &script) {
        Err(AttachError::Refused(code, _)) => assert_eq!(code, "bad_token"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    match LinkClient::attach(&s, &client("x"), &home.path().join("other.fountain")) {
        Err(AttachError::Refused(code, _)) => assert_eq!(code, "wrong_doc"),
        other => panic!("{:?}", other.map(|_| ())),
    }

    let l = LinkClient::attach(&s, &client("claude-code"), &script).unwrap();
    assert_eq!(l.hello.rev, 0);
    // The agent reads the scene list...
    let r = call(&mut h, &l, "scene.list", json!({}), "scene_list");
    assert_eq!(
        r["result"]["scenes"][0]["heading"], "INT. LAB - NIGHT",
        "{r}"
    );
    // ...and edits: the writer sees it at once, with the block marked.
    let r = call(
        &mut h,
        &l,
        "block.insert",
        json!({"type": "action", "text": "Thunder."}),
        "block_insert",
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(texts(&h), ["INT. LAB - NIGHT", "Rain.", "Thunder."]);
    assert_eq!(h.app.agent_marks.len(), 1);
    assert_eq!(h.app.agents.values().next().unwrap().edits, 1);
    let label = h
        .app
        .d
        .session()
        .unwrap()
        .undo_steps()
        .next()
        .unwrap()
        .1
        .to_string();
    assert_eq!(label, "claude-code · block_insert");
    // A `changed` event follows.
    let ev = l
        .next_event(Duration::from_secs(5))
        .expect("a changed event");
    assert_eq!(ev["event"], "changed");

    // Commands that would replace or close the writer's script are refused.
    let r = call(
        &mut h,
        &l,
        "doc.close",
        json!({"discard": true}),
        "doc_close",
    );
    assert_eq!(r["error"]["code"], "not_allowed_attached", "{r}");
    let r = call(
        &mut h,
        &l,
        "doc.save",
        json!({"path": "x.fountain"}),
        "doc_save",
    );
    assert_eq!(r["error"]["code"], "not_allowed_attached", "{r}");

    // The writer types; the agent may not undo the writer's step...
    h.named(
        iced::keyboard::key::Named::ArrowDown,
        iced::keyboard::Modifiers::empty(),
    );
    h.named(
        iced::keyboard::key::Named::End,
        iced::keyboard::Modifiers::empty(),
    );
    h.type_text(" Wind.");
    let r = call(&mut h, &l, "undo", json!({}), "undo");
    assert_eq!(r["error"]["code"], "not_your_step", "{r}");
    // ...and the writer's Mod+Z goes through history in order: the typing,
    // then the agent's insert.
    h.shortcut('z', false);
    h.shortcut('z', false);
    assert_eq!(texts(&h), ["INT. LAB - NIGHT", "Rain."]);

    // Disconnect: the agent hears `closed`, and its link is gone.
    h.run(vec![Message::Action("app.agent_disconnect".into())]);
    assert!(h.app.agents.is_empty());
    let start = Instant::now();
    while !l.is_closed() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(l.request("scene.list", json!({}), None).is_err());
    h.app.d.discard();
}

#[test]
fn turning_agent_access_off_closes_the_link_and_removes_the_session_file() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, script) = app(home.path());
    let s = session(home.path()).unwrap();
    let l = LinkClient::attach(&s, &client("agent"), &script).unwrap();
    let r = call(&mut h, &l, "doc.get", json!({}), "doc_get");
    assert_eq!(r["ok"], true);
    assert!(h.app.agent_status().is_some());
    h.run(vec![Message::Action("app.agent_access".into())]);
    assert!(!h.app.settings.agent_access);
    assert!(h.app.link.is_none());
    assert!(session(home.path()).is_none());
    let start = Instant::now();
    while !l.is_closed() {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    // Nothing answers at the old endpoint any more.
    assert!(LinkClient::attach(&s, &client("agent"), &script).is_err());
    // On again: a new session file with a new token.
    h.run(vec![Message::Action("app.agent_access".into())]);
    let s2 = session(home.path()).unwrap();
    assert_ne!(s2.token, s.token);
    h.app.d.discard();
}

#[test]
fn at_most_four_agents() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, script) = app(home.path());
    let s = session(home.path()).unwrap();
    let links: Vec<LinkClient> = (0..4)
        .map(|i| LinkClient::attach(&s, &client(&format!("a{i}")), &script).unwrap())
        .collect();
    match LinkClient::attach(&s, &client("a5"), &script) {
        Err(AttachError::Refused(code, _)) => assert_eq!(code, "too_many_links"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    while h.app.agents.len() < 4 {
        h.pump();
    }
    assert_eq!(
        h.app.agent_status().unwrap(),
        "4 agents connected · 0 edits"
    );
    drop(links);
    h.app.d.discard();
}

/// Review finding F19: the writer marks a line as a Shot, which Fountain
/// cannot hold, so their own Mod+S would ask "Save anyway?". An attached
/// agent's `doc.save {"allow_lossy": true}` answered that question for
/// them, and the Shot was gone from the file.
#[test]
fn an_agent_cannot_answer_the_writers_lossy_save_question() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, script) = app(home.path());
    let s = session(home.path()).unwrap();
    let l = LinkClient::attach(&s, &client("claude-code"), &script).unwrap();
    h.named(
        iced::keyboard::key::Named::ArrowDown,
        iced::keyboard::Modifiers::empty(),
    );
    h.shortcut('7', false); // "Rain." becomes a Shot
    assert!(h.app.d.session().unwrap().dirty);

    let r = call(
        &mut h,
        &l,
        "doc.save",
        json!({"allow_lossy": true}),
        "doc_save",
    );
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"]["code"], "writer_only", "{r}");
    let r = call(&mut h, &l, "doc.save", json!({}), "doc_save");
    assert_eq!(r["error"]["code"], "lossy_format", "{r}");
    assert_eq!(std::fs::read_to_string(&script).unwrap(), SCRIPT);
    assert!(h.app.d.session().unwrap().dirty);
    h.app.d.discard();
}

/// Runs a command from the palette's params form, as a writer would.
fn palette_params(h: &mut Headless, command: &str, params: Value) {
    h.run(vec![
        Message::Action(command.into()),
        Message::ParamsInput(params.to_string()),
        Message::ParamsRun,
    ]);
}

/// Review finding F7: the palette's params form sent `doc.open` straight to
/// the dispatcher, past the app's own Open flow, so an attached agent stayed
/// attached and its next edit landed in the other script, which the writer
/// never shared.
#[test]
fn opening_another_script_from_the_palette_disconnects_agents() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, script) = app(home.path());
    let b = home.path().join("b.fountain");
    std::fs::write(&b, "INT. B - NIGHT\n\nScript B.\n").unwrap();
    let s = session(home.path()).unwrap();
    let l = LinkClient::attach(&s, &client("claude-code"), &script).unwrap();
    let r = call(&mut h, &l, "scene.list", json!({}), "scene_list");
    assert_eq!(r["ok"], true, "{r}");

    palette_params(&mut h, "doc.open", json!({"path": b}));
    assert_eq!(
        h.app.d.session().unwrap().path.as_deref(),
        Some(b.as_path())
    );
    assert!(
        h.app.agents.is_empty(),
        "the agent was attached to the other script"
    );
    let start = Instant::now();
    while !l.is_closed() {
        h.pump();
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the link stayed open"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        l.request(
            "block.insert",
            json!({"type": "action", "text": "AGENT TEXT MEANT FOR A."}),
            None
        )
        .is_err()
    );
    assert!(!texts(&h).iter().any(|t| t.contains("AGENT TEXT")));
    h.app.d.discard();
}

/// The params form's `doc.close {"discard": true}` closed the script with
/// no question; now it goes through the app's Close, which asks.
#[test]
fn closing_from_the_palette_asks_like_close_does() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _script) = app(home.path());
    h.named(
        iced::keyboard::key::Named::ArrowDown,
        iced::keyboard::Modifiers::empty(),
    );
    h.type_text(" Wind.");
    palette_params(&mut h, "doc.close", json!({"discard": true}));
    assert!(
        matches!(h.app.prompt, Some(coldopen_app::app::Prompt::Unsaved(_))),
        "{:?}",
        h.app.prompt
    );
    assert!(h.app.d.session().unwrap().dirty);
    h.app.d.discard();
}

/// Review findings F20 and C8 (and Antigravity's 10): each link's reader
/// passed every request line to the app as it arrived, so a client that
/// sent 100 requests without waiting queued all 100 in front of the
/// writer's own input. Now a link has at most one request waiting.
#[test]
fn a_link_has_at_most_one_request_waiting_for_the_app() {
    use coldopen_app::link::{LinkEvent, LinkServer};
    let home = tempfile::tempdir().unwrap();
    let run = Dirs::under(home.path()).run;
    std::fs::create_dir_all(&run).unwrap();
    let script = home.path().join("lab.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    let server = LinkServer::start(&run, vec![coldopen_link::canonical(&script)], 0).unwrap();
    let s = SessionFile::read(&server.session_file()).unwrap();
    let l = std::sync::Arc::new(LinkClient::attach(&s, &client("flood"), &script).unwrap());
    // On its own thread: once the server stops reading, the client's writes
    // block in its own socket, which is the point.
    let sender = l.clone();
    std::thread::spawn(move || {
        let mut answers = Vec::new();
        for _ in 0..100 {
            match sender.request("doc.get", json!({}), None) {
                Ok(rx) => answers.push(rx),
                Err(_) => break,
            }
        }
    });
    // Nobody runs the app: whatever the link passes on stays in the inbox.
    std::thread::sleep(Duration::from_millis(500));
    let requests = server
        .key
        .inbox
        .drain()
        .into_iter()
        .filter(|e| matches!(e, LinkEvent::Request { .. }))
        .count();
    assert_eq!(requests, 1);
}

/// Review finding F20: every connection got a thread before its handshake,
/// held for up to five seconds, with no cap until after the hello. Now a
/// connection beyond the cap is turned away at once.
#[test]
fn connections_still_in_their_handshake_are_capped() {
    use coldopen_app::link::LinkServer;
    use std::io::Read;
    let home = tempfile::tempdir().unwrap();
    let run = Dirs::under(home.path()).run;
    std::fs::create_dir_all(&run).unwrap();
    let script = home.path().join("lab.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    let server = LinkServer::start(&run, vec![coldopen_link::canonical(&script)], 0).unwrap();
    let endpoint = server.endpoint.clone();
    let _idle: Vec<coldopen_link::Conn> = (0..8)
        .map(|_| coldopen_link::Conn::connect(&endpoint).unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(300));
    let mut extra = coldopen_link::Conn::connect(&endpoint).unwrap();
    // Without the cap, the server would wait five seconds for a hello.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut got = Vec::new();
        let mut buf = [0u8; 256];
        while !got.contains(&b'\n') {
            match extra.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
            }
        }
        let _ = tx.send(got);
    });
    let got = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("an answer at once, not after the hello deadline");
    let reply = String::from_utf8_lossy(&got);
    assert!(reply.contains("too_many_links"), "{reply:?}");
}

/// Found while testing F20 (2026-10-16): on Windows, a non-blocking read of
/// a pipe with nothing in it yet fails with ERROR_NO_DATA, which Rust calls
/// `BrokenPipe`, so the server refused any client whose hello arrived a
/// moment after it first looked. A busy machine could turn agents away.
#[test]
fn a_hello_that_arrives_a_moment_late_is_still_read() {
    use coldopen_app::link::LinkServer;
    use std::io::{Read, Write};
    let home = tempfile::tempdir().unwrap();
    let run = Dirs::under(home.path()).run;
    std::fs::create_dir_all(&run).unwrap();
    let script = home.path().join("lab.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    let server = LinkServer::start(&run, vec![coldopen_link::canonical(&script)], 0).unwrap();
    let s = SessionFile::read(&server.session_file()).unwrap();
    let mut conn = coldopen_link::Conn::connect(&s.endpoint).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    writeln!(
        conn,
        "{}",
        coldopen_link::hello_line(&s.token, &client("slow"), &script)
    )
    .unwrap();
    conn.flush().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut got = Vec::new();
        let mut buf = [0u8; 512];
        while !got.contains(&b'\n') {
            match conn.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
            }
        }
        let _ = tx.send(got);
    });
    let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let reply = String::from_utf8_lossy(&got);
    assert!(reply.contains(r#""ok":true"#), "{reply:?}");
}

fn writer_run(h: &mut Headless, command: &str, params: Value) -> Value {
    let cmd = coldopen_core::Command::from_parts(command, params).unwrap();
    let r = h
        .app
        .d
        .execute(cmd)
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    h.pump();
    r
}

/// Phase 5a: through the real link, an agent's edits arrive as tracked
/// changes under its name; the writer settles them, and the agent can
/// settle none of the writer's (docs/specs/mcp.md §6.1).
#[test]
fn agent_edits_arrive_as_tracked_changes_the_writer_settles() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, script) = app(home.path());
    let s = session(home.path()).unwrap();
    let l = LinkClient::attach(&s, &client("claude-code"), &script).unwrap();

    let r = call(
        &mut h,
        &l,
        "text.replace",
        json!({"find": "Rain", "with": "Hail"}),
        "text_replace",
    );
    assert_eq!(r["result"]["change"], "c_1", "{r}");
    assert_eq!(texts(&h), ["INT. LAB - NIGHT", "Hail."]);
    let list = writer_run(&mut h, "changes.list", json!({}));
    assert_eq!(
        list["changes"][0]["by"],
        json!({"kind": "agent", "name": "claude-code"}),
        "{list}"
    );
    assert_eq!(list["changes"][0]["deleted"], "Rain");
    // An agent cannot opt out, nor accept.
    let r = call(
        &mut h,
        &l,
        "text.insert",
        json!({"id": "b_2", "at": 0, "text": "x", "tracked": false}),
        "text_insert",
    );
    assert_eq!(r["error"]["code"], "writer_only", "{r}");
    let r = call(
        &mut h,
        &l,
        "changes.accept",
        json!({"all": true}),
        "changes_accept",
    );
    assert_eq!(r["error"]["code"], "writer_only", "{r}");

    // The writer tracks their own typing; the agent cannot reject it.
    writer_run(&mut h, "changes.track", json!({"writer": true}));
    writer_run(
        &mut h,
        "text.insert",
        json!({"id": "b_1", "at": 16, "text": " LATER"}),
    );
    let list = writer_run(&mut h, "changes.list", json!({}));
    let mine = list["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["by"]["kind"] == "writer")
        .unwrap()["id"]
        .clone();
    let r = call(
        &mut h,
        &l,
        "changes.reject",
        json!({"id": mine}),
        "changes_reject",
    );
    assert_eq!(r["error"]["code"], "not_your_change", "{r}");
    // The agent withdraws its own; the writer's stays.
    let r = call(
        &mut h,
        &l,
        "changes.reject",
        json!({"all": true}),
        "changes_reject",
    );
    assert_eq!(r["result"]["changes"], json!(["c_1"]), "{r}");
    assert_eq!(texts(&h), ["INT. LAB - NIGHT LATER", "Rain."]);
    // A new proposal, which the writer accepts with everything else.
    call(
        &mut h,
        &l,
        "block.insert",
        json!({"type": "action", "text": "Thunder."}),
        "block_insert",
    );
    writer_run(&mut h, "changes.accept", json!({"all": true}));
    let list = writer_run(&mut h, "changes.list", json!({}));
    assert_eq!(list["changes"], json!([]));
    assert_eq!(texts(&h), ["INT. LAB - NIGHT LATER", "Rain.", "Thunder."]);
    l.bye();
}

/// The session file names a bundle's identity next to its path (F18.4).
#[test]
fn the_session_file_names_a_bundles_identity() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _script) = app(home.path());
    assert!(session(home.path()).unwrap().doc_ids.is_empty());
    let bundle = home.path().join("lab.coldopen");
    let to = bundle.clone();
    h.app
        .set_dialogs(Dialogs::scripted(move |_| Some(to.clone())));
    h.run(vec![Message::Action("app.save_as".into())]);
    let s = session(home.path()).unwrap();
    assert!(s.has_doc(&bundle), "{:?}", s.docs);
    let id = h.app.d.session().unwrap().doc.doc_id.clone();
    assert!(id.is_some());
    assert_eq!(s.doc_ids, vec![id]);
}
