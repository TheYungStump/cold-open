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

/// CL7 (the reviews of phases 4–5b, 2026-10-28): the draft is compared on a
/// worker thread, as a save is written. Until it is done the status bar
/// says so and the editor stays usable; Esc stops waiting, and an answer
/// that comes after that is let go.
#[test]
fn a_comparison_runs_off_the_ui_thread_and_esc_stops_waiting() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    let b = home.path().join("b.fountain");
    // The worker's answer, held back until the test runs it.
    let pending = h.app.update(Message::Chosen(Ask::Compare, Some(b)));
    assert!(
        h.app.working.is_some(),
        "the status bar says it is comparing"
    );
    assert!(h.app.compare.is_none());
    // The editor takes keys meanwhile.
    h.type_text("x");
    assert!(h.app.d.session().unwrap().dirty);
    h.named(Named::Escape, Modifiers::empty());
    assert!(h.app.working.is_none());
    h.run_task(pending);
    assert!(
        h.app.compare.is_none(),
        "a stopped comparison does not open"
    );
}

/// "Save as PDF…" lays out and writes off the UI thread (2026-10-30,
/// editor.md §21): the update returns with the PDF still to come, the
/// status bar says so, and the banner comes with the worker's answer.
#[test]
fn the_diff_pdf_is_written_off_the_ui_thread() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    h.run(vec![Message::Action("app.compare".into())]);
    assert!(h.app.compare.is_some());
    h.app.banners.clear();
    let pdf = home.path().join("changes.pdf");
    let chosen = || Message::Chosen(Ask::DiffPdf("changes.pdf".into()), Some(pdf.clone()));
    let task = h.app.update(chosen());
    assert_eq!(h.app.pdf_writing.as_deref(), Some("changes.pdf"));
    assert!(h.app.banners.is_empty(), "{:?}", h.app.banners);
    // A second one waits its turn.
    let _ = h.app.update(chosen());
    assert!(
        h.app
            .banners
            .iter()
            .any(|b| b.contains("still being written"))
    );
    h.app.banners.clear();
    h.run_task(task);
    assert!(h.app.pdf_writing.is_none());
    assert!(
        h.app
            .banners
            .iter()
            .any(|b| b.contains("Saved changes.pdf")),
        "{:?}",
        h.app.banners
    );
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
}
