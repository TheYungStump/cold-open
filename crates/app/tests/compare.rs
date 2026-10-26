//! Comparing drafts in the running app (docs/specs/editor.md §21): the
//! compare view with its list of changed scenes and its PDF, and a draft
//! brought in as tracked changes. Driven headless with real key presses and
//! scripted file dialogs.

use std::path::{Path, PathBuf};

use coldopen_app::files::{Ask, Dialogs, Dirs};
use coldopen_app::headless::{Headless, command_modifier};
use coldopen_app::theme::Choice;
use coldopen_app::{App, Boot, Message};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../diff/tests/fixtures")
}

/// The app with a copy of draft A open, and the dialogs answering with
/// draft B for a draft and `pdf` for a PDF.
fn app(home: &Path) -> Headless {
    let (a, b) = (home.join("a.fountain"), home.join("b.fountain"));
    std::fs::copy(fixtures().join("a.fountain"), &a).unwrap();
    std::fs::copy(fixtures().join("b.fountain"), &b).unwrap();
    let pdf = home.join("changes.pdf");
    let mut app = App::new(Boot {
        file: Some(a),
        dirs: Some(Dirs::under(home)),
        theme: Some(Choice::Light),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(move |ask| match ask {
        Ask::Compare | Ask::ReviewDraft => Some(b.clone()),
        Ask::DiffPdf(_) => Some(pdf.clone()),
        _ => None,
    }));
    Headless::new(app, iced::Size::new(1400.0, 900.0)).expect("a headless renderer")
}

#[test]
fn compare_with_lists_the_changed_scenes_and_saves_a_pdf() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    h.run(vec![Message::Action("app.compare".into())]);
    let c = h.app.compare.as_ref().expect("the view opens");
    let labels: Vec<&str> = c.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels.len(), 5, "{labels:?}");
    assert!(
        labels[3].starts_with("Scene 4 (was 2) EXT. ROOF - NIGHT: moved after scene 3"),
        "{labels:?}"
    );
    assert!(
        c.rows.iter().all(|r| r.block.is_some()),
        "every row has a place on the pages"
    );
    // Down twice, then up: the selection stops at neither end yet.
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::ArrowUp, Modifiers::empty());
    assert_eq!(h.app.compare.as_ref().unwrap().selected, 1);
    h.named(Named::Enter, Modifiers::empty());
    // Mod+Shift+E saves the marked draft.
    h.key(
        Key::Character("e".into()),
        command_modifier() | Modifiers::SHIFT,
        Some("E"),
    );
    let pdf = std::fs::read(home.path().join("changes.pdf")).expect("the PDF is written");
    assert!(pdf.starts_with(b"%PDF"));
    // The open script is untouched, and Esc closes the view.
    let session = h.app.d.session().unwrap();
    assert!(!session.dirty);
    assert!(!session.doc.has_changes());
    h.named(Named::Escape, Modifiers::empty());
    assert!(h.app.compare.is_none());
}

#[test]
fn a_draft_comes_in_as_one_change_under_the_name_given() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    h.run(vec![Message::Action("app.review_draft".into())]);
    // The name is asked, prefilled with the draft's file name and
    // selected, so typing replaces it.
    h.type_text("Sam");
    h.named(Named::Enter, Modifiers::empty());
    let doc = &h.app.d.session().unwrap().doc;
    assert!(doc.has_changes());
    assert!(h.app.settings.changes_panel, "the changes panel opens");
    let history = h
        .app
        .d
        .execute(coldopen_core::Command::from_parts("history.list", serde_json::json!({})).unwrap())
        .unwrap();
    assert_eq!(history["undo"][0]["label"], "Changes from Sam");
    let changes = h
        .app
        .d
        .execute(coldopen_core::Command::from_parts("changes.list", serde_json::json!({})).unwrap())
        .unwrap();
    assert_eq!(changes["changes"][0]["by"]["name"], "Sam");
    // One undo takes it all back.
    h.shortcut('z', false);
    assert!(!h.app.d.session().unwrap().doc.has_changes());
}
