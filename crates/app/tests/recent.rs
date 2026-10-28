//! Recent files that are gone (docs/specs/editor.md §11): found when the
//! list is shown, kept until the writer removes them, and removed from the
//! palette or from the question choosing one asks. Driven headless.

use std::path::Path;

use coldopen_app::files::{Dialogs, Dirs, Settings};
use coldopen_app::headless::Headless;
use coldopen_app::theme::Choice;
use coldopen_app::{App, Boot, Message};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

fn app(home: &Path) -> Headless {
    let mut app = App::new(Boot {
        dirs: Some(Dirs::under(home)),
        theme: Some(Choice::Light),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, iced::Size::new(1200.0, 800.0)).expect("a headless renderer")
}

#[test]
fn a_gone_recent_file_is_kept_until_the_writer_removes_it() {
    let home = tempfile::tempdir().unwrap();
    let here = home.path().join("here.fountain");
    std::fs::write(&here, "INT. LAB - NIGHT\n\nRain.\n").unwrap();
    let gone = home.path().join("unplugged").join("gone.fountain");
    let mut h = app(home.path());
    h.app.settings.recent = vec![gone.clone(), here.clone()];

    // Opening the palette checks the list off the UI thread.
    h.run(vec![Message::Action("app.palette".into())]);
    assert!(h.app.missing.contains(&gone));
    assert!(!h.app.missing.contains(&here));
    assert_eq!(h.app.settings.recent.len(), 2, "never dropped on its own");
    h.named(Named::Escape, Modifiers::empty());

    // Choosing it asks; K keeps it.
    h.run(vec![Message::OpenPath(gone.clone())]);
    assert!(matches!(
        h.app.prompt,
        Some(coldopen_app::app::Prompt::MissingRecent(_))
    ));
    h.key(Key::Character("k".into()), Modifiers::empty(), Some("k"));
    assert!(h.app.prompt.is_none());
    assert!(h.app.settings.recent.contains(&gone));

    // The palette offers to remove it.
    h.run(vec![Message::Action("app.palette".into())]);
    h.type_text("remove from recent");
    h.named(Named::Enter, Modifiers::empty());
    assert_eq!(h.app.settings.recent, vec![here.clone()]);
    let (saved, _) = Settings::load(&Dirs::under(home.path()));
    assert_eq!(
        saved.recent,
        vec![here.clone()],
        "settings.json no longer lists it"
    );
}

#[test]
fn choosing_a_gone_file_and_pressing_enter_removes_it() {
    let home = tempfile::tempdir().unwrap();
    let gone = home.path().join("gone.fdx");
    let mut h = app(home.path());
    h.app.settings.recent = vec![gone.clone()];
    h.run(vec![Message::OpenPath(gone.clone())]);
    h.named(Named::Enter, Modifiers::empty());
    assert!(h.app.settings.recent.is_empty());
    assert!(h.app.prompt.is_none());
    // The script that was open is still open.
    assert!(h.app.d.session().is_some());
}

#[test]
fn a_file_with_only_a_crash_journal_is_not_missing() {
    let home = tempfile::tempdir().unwrap();
    let p = home.path().join("never-saved.fountain");
    std::fs::write(coldopen_core::journal::path_for(&p), "").unwrap();
    assert!(!coldopen_app::app::recent_gone(&p));
}
