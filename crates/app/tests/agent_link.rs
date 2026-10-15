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
