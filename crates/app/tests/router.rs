//! One command router (docs/specs/editor.md §16): every source sends what
//! it wants done through `App::dispatch`, which runs the writer's session
//! commands through the app's own flows, answers agents without them, and
//! holds a step that leaves the script until a save in flight is written.

use std::path::{Path, PathBuf};

use coldopen_app::app::{Origin, Prompt, Request};
use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot, Message};
use coldopen_core::{Command, Dispatcher, Options};
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;
use serde_json::{Value, json};

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain.\n";

fn app(home: &Path, file: Option<&Path>) -> Headless {
    let mut app = App::new(Boot {
        file: file.map(Path::to_path_buf),
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, iced::Size::new(1100.0, 800.0)).expect("a headless renderer")
}

fn script(dir: &Path) -> PathBuf {
    let p = dir.join("lab.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    p
}

fn cmd(name: &str, params: Value) -> Request {
    Request::Command(Command::from_parts(name, params).unwrap())
}

fn agent() -> Origin {
    Origin::Agent {
        name: "claude-code".into(),
        tool: None,
    }
}

/// A copy of unsaved work set aside with Don't save, made by core alone.
fn set_aside(dir: &Path) -> (PathBuf, PathBuf) {
    let p = dir.join("old.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    let mut d = Dispatcher::new(Options::default());
    d.execute(Command::from_parts("doc.open", json!({ "path": p })).unwrap())
        .unwrap();
    d.execute(
        Command::from_parts(
            "block.insert",
            json!({ "type": "action", "text": "Thunder, set aside." }),
        )
        .unwrap(),
    )
    .unwrap();
    let r = d
        .execute(Command::from_parts("doc.close", json!({ "discard": true })).unwrap())
        .unwrap();
    let copy = PathBuf::from(r["warnings"][0]["details"]["copy"].as_str().unwrap());
    (p, copy)
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
fn agents_get_answers_and_never_the_writers_flows() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), Some(&p));
    let code = |out: coldopen_app::app::Outcome| out.response.unwrap().unwrap_err().code;

    let out = h.app.dispatch(Request::Action("app.close".into()), agent());
    assert_eq!(code(out), "not_allowed_attached");
    let out = h.app.dispatch(cmd("doc.close", json!({})), agent());
    assert_eq!(code(out), "not_allowed_attached");
    let (_, copy) = set_aside(home.path());
    let out = h
        .app
        .dispatch(cmd("recovered.delete", json!({ "copy": copy })), agent());
    assert_eq!(code(out), "not_allowed_attached");
    assert!(copy.exists());
    assert!(h.app.prompt.is_none());
    assert!(h.app.d.session().is_some());

    // An edit runs as the agent and says it changed the script.
    let out = h.app.dispatch(
        cmd(
            "block.insert",
            json!({ "type": "action", "text": "Agent line." }),
        ),
        agent(),
    );
    assert!(out.edited);
    assert!(out.response.unwrap().is_ok());
    assert!(texts(&h).iter().any(|t| t == "Agent line."));
}

#[test]
fn the_form_runs_session_commands_through_the_app() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), Some(&p));

    // A query answers in the form.
    let out = h.app.dispatch(cmd("doc.get", json!({})), Origin::Form);
    assert_eq!(out.response.unwrap().unwrap()["dirty"], false);

    // Closing a script with changes asks first, as Mod+W does.
    let out = h.app.dispatch(
        cmd("block.insert", json!({ "type": "action", "text": "x" })),
        Origin::Form,
    );
    assert!(out.edited);
    let out = h.app.dispatch(cmd("doc.close", json!({})), Origin::Form);
    assert!(out.response.is_none());
    assert!(matches!(h.app.prompt, Some(Prompt::Unsaved(_))));
    assert!(h.app.d.session().is_some());
    h.named(Named::Escape, Modifiers::empty());
    assert!(h.app.prompt.is_none());
}

#[test]
fn deleting_a_recovered_version_asks_first() {
    let home = tempfile::tempdir().unwrap();
    let (_, copy) = set_aside(home.path());
    let mut h = app(home.path(), None);

    let out = h.app.dispatch(
        cmd("recovered.delete", json!({ "copy": copy })),
        Origin::Form,
    );
    h.run_task(out.task);
    assert!(matches!(h.app.prompt, Some(Prompt::DeleteCopy(_))));
    // S is not an answer here: only D or Enter delete.
    h.key(
        iced::keyboard::Key::Character("s".into()),
        Modifiers::empty(),
        Some("s"),
    );
    assert!(h.app.prompt.is_some());
    h.named(Named::Escape, Modifiers::empty());
    assert!(h.app.prompt.is_none());
    assert!(copy.exists());

    let out = h.app.dispatch(
        cmd("recovered.delete", json!({ "copy": copy })),
        Origin::Form,
    );
    h.run_task(out.task);
    h.named(Named::Enter, Modifiers::empty());
    assert!(h.app.prompt.is_none());
    assert!(!copy.exists());
}

#[test]
fn restoring_opens_unsaved_work_and_touches_nothing() {
    let home = tempfile::tempdir().unwrap();
    let (old, copy) = set_aside(home.path());
    let old_bytes = std::fs::read(&old).unwrap();
    let copy_bytes = std::fs::read(&copy).unwrap();
    let mut h = app(home.path(), None);

    let to = home.path().join("ignored.fountain");
    let out = h.app.dispatch(
        cmd("recovered.restore", json!({ "copy": copy, "path": to })),
        Origin::Form,
    );
    h.run_task(out.task);
    let s = h.app.d.session().expect("the restored script is open");
    assert!(s.dirty);
    let path = s.path.clone().unwrap();
    assert!(
        path.starts_with(Dirs::under(home.path()).unsaved()),
        "{path:?}"
    );
    assert!(path.ends_with("old (restored).fountain"), "{path:?}");
    assert!(texts(&h).iter().any(|t| t == "Thunder, set aside."));
    assert!(
        h.app.banners.iter().any(|b| b.starts_with("Restored")),
        "{:?}",
        h.app.banners
    );
    // No path is shown; nothing on disk was written.
    assert!(h.app.banners.iter().all(|b| !b.contains(".coldopen")));
    assert!(!path.exists());
    assert!(!to.exists());
    assert_eq!(std::fs::read(&old).unwrap(), old_bytes);
    assert_eq!(std::fs::read(&copy).unwrap(), copy_bytes);
    h.app.d.shutdown();
}

#[test]
fn leaving_the_script_waits_for_the_save_being_written() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), Some(&p));
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::End, Modifiers::empty());
    h.type_text(" Thunder.");

    // The save is started and its write held back.
    let write = h.app.update(Message::Action("doc.save".into()));
    // Quit, and don't save the rest: the app quits only once the save is
    // written.
    let out = h
        .app
        .dispatch(Request::Action("app.quit".into()), Origin::Key);
    h.run_task(out.task);
    assert!(matches!(h.app.prompt, Some(Prompt::Unsaved(_))));
    h.key(
        iced::keyboard::Key::Character("d".into()),
        Modifiers::empty(),
        Some("d"),
    );
    assert!(h.app.prompt.is_none());
    assert!(!h.exited, "quit before the save was written");
    h.run_task(write);
    assert!(h.exited);
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        SCRIPT.replace("Rain.", "Rain. Thunder.")
    );

    // A clean script being saved closes when the save finishes.
    let mut h = app(home.path(), Some(&p));
    let write = h.app.update(Message::Action("doc.save".into()));
    let out = h.app.dispatch(cmd("doc.close", json!({})), Origin::Form);
    h.run_task(out.task);
    assert!(h.app.prompt.is_none());
    assert!(
        h.app.d.session().is_some(),
        "closed before the save was written"
    );
    h.run_task(write);
    assert!(h.app.d.session().is_none(), "{:?}", h.app.banners);

    // Revert goes back to what the save wrote, not to the file before it.
    let mut h = app(home.path(), Some(&p));
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::End, Modifiers::empty());
    h.type_text(" Hail.");
    let write = h.app.update(Message::Action("doc.save".into()));
    h.type_text(" Snow.");
    let out = h.app.dispatch(cmd("doc.revert", json!({})), Origin::Form);
    h.run_task(out.task);
    assert!(texts(&h).iter().any(|t| t.ends_with("Hail. Snow.")));
    h.run_task(write);
    assert!(
        texts(&h).iter().any(|t| t.ends_with("Thunder. Hail.")),
        "{:?}",
        texts(&h)
    );
    h.app.d.shutdown();
}
