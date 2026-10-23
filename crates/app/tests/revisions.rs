//! Production revision sets in the running app (docs/specs/editor.md §18):
//! the start form, marked text in the set's ink in each theme, the question
//! before starting while a set is active, clearing from the keyboard, the
//! lossy question, and the export form's colored pages. Driven headless with
//! real key presses.

use std::path::{Path, PathBuf};

use coldopen_app::app::Prompt;
use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::{Headless, command_modifier};
use coldopen_app::theme::{Choice, Palette};
use coldopen_app::{App, Boot, Message};
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain on the roof.\n\nEXT. ROOF - NIGHT\n\nWind.\n";

fn app(home: &Path, theme: Choice) -> (Headless, PathBuf) {
    let p = home.join("lab.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    let mut app = App::new(Boot {
        file: Some(p.clone()),
        dirs: Some(Dirs::under(home)),
        theme: Some(theme),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    (
        Headless::new(app, iced::Size::new(1200.0, 800.0)).expect("a headless renderer"),
        p,
    )
}

fn action(h: &mut Headless, name: &str) {
    h.run(vec![Message::Action(name.into())]);
}

fn press(h: &mut Headless, key: Named, n: usize) {
    for _ in 0..n {
        h.named(key, Modifiers::empty());
    }
}

fn end_of(h: &mut Headless, i: usize) {
    h.doc_start();
    press(h, Named::ArrowDown, i);
    press(h, Named::End, 1);
}

fn has_color(px: &[u8], c: iced::Color) -> bool {
    let want = [c.r, c.g, c.b].map(|v| (v * 255.0).round() as i32);
    px.as_chunks::<4>()
        .0
        .iter()
        .any(|p| (0..3).all(|k| (i32::from(p[k]) - want[k]).abs() <= 2))
}

fn active(h: &Headless) -> Option<String> {
    let doc = &h.app.d.session().unwrap().doc;
    doc.revisions.active_set().map(|s| s.color.clone())
}

/// Starts a set through the form, taking what it offers.
fn start(h: &mut Headless) {
    action(h, "app.revision_start");
    assert!(matches!(h.app.prompt, Some(Prompt::RevisionStart(_))));
    press(h, Named::Enter, 1);
    assert!(h.app.prompt.is_none());
}

#[test]
fn the_form_offers_the_next_color_and_starts_on_enter() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = app(home.path(), Choice::Light);
    action(&mut h, "app.revision_start");
    let Some(Prompt::RevisionStart(f)) = &h.app.prompt else {
        panic!("no form");
    };
    assert_eq!(f.color, "Blue");
    assert_eq!(f.label, "Blue Rev.");
    assert_eq!(f.date.len(), 10);
    // Down steps to the next color, and the label follows it.
    press(&mut h, Named::ArrowDown, 1);
    let Some(Prompt::RevisionStart(f)) = &h.app.prompt else {
        panic!("no form");
    };
    assert_eq!((f.color.as_str(), f.label.as_str()), ("Pink", "Pink Rev."));
    press(&mut h, Named::ArrowUp, 1);
    press(&mut h, Named::Enter, 1);
    assert_eq!(active(&h).as_deref(), Some("Blue"));
    // Esc on a new form changes nothing.
    action(&mut h, "app.revision_end");
    action(&mut h, "app.revision_start");
    press(&mut h, Named::Escape, 1);
    assert!(h.app.prompt.is_none());
    assert_eq!(active(&h), None);
}

#[test]
fn marked_text_shows_in_the_sets_ink_in_each_theme() {
    for theme in [Choice::Light, Choice::Dark, Choice::HighContrast] {
        let home = tempfile::tempdir().unwrap();
        let (mut h, _) = app(home.path(), theme);
        let shade = Palette::of(theme, false).shade;
        let (r, g, b) = coldopen_core::revisions::shades("Blue").inks[shade].unwrap();
        let ink = iced::Color::from_rgb8(r, g, b);
        let (_, before) = h.frame(1.0);
        assert!(!has_color(&before, ink), "{theme:?}: the ink is free");
        start(&mut h);
        end_of(&mut h, 1);
        h.type_text(" Thunder.");
        let (_, after) = h.frame(1.0);
        assert!(has_color(&after, ink), "{theme:?}: the mark is drawn");
        h.app.d.discard();
    }
}

#[test]
fn starting_while_a_set_is_active_asks_to_end_it_first() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = app(home.path(), Choice::Light);
    start(&mut h);
    action(&mut h, "app.revision_start");
    assert!(matches!(
        h.app.prompt,
        Some(Prompt::RevisionEndFirst { .. })
    ));
    press(&mut h, Named::Enter, 1);
    let Some(Prompt::RevisionStart(f)) = &h.app.prompt else {
        panic!("the form follows");
    };
    assert_eq!(f.color, "Pink");
    assert_eq!(active(&h), None, "the Blue set ended");
    press(&mut h, Named::Enter, 1);
    assert_eq!(active(&h).as_deref(), Some("Pink"));
}

#[test]
fn clearing_marks_from_the_keyboard_and_undo() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = app(home.path(), Choice::Light);
    start(&mut h);
    end_of(&mut h, 1);
    h.type_text(" Thunder.");
    let marked = |h: &Headless| {
        h.app
            .d
            .session()
            .unwrap()
            .doc
            .blocks
            .iter()
            .any(|b| b.has_revisions())
    };
    assert!(marked(&h));
    action(&mut h, "app.revision_clear");
    assert!(matches!(
        h.app.prompt,
        Some(Prompt::RevisionClear { choice: 0 })
    ));
    press(&mut h, Named::ArrowDown, 1);
    assert!(matches!(
        h.app.prompt,
        Some(Prompt::RevisionClear { choice: 1 })
    ));
    press(&mut h, Named::Enter, 1);
    assert!(!marked(&h));
    h.key(
        iced::keyboard::Key::Character("z".into()),
        command_modifier(),
        Some("z"),
    );
    assert!(marked(&h));
}

#[test]
fn saving_revision_marks_to_fountain_asks_and_save_anyway_takes_them_out() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, p) = app(home.path(), Choice::Light);
    start(&mut h);
    end_of(&mut h, 1);
    h.type_text(" Thunder.");
    h.shortcut('s', false);
    let Some(Prompt::Lossy { revisions, .. }) = &h.app.prompt else {
        panic!("the lossy question: {:?}", h.app.prompt);
    };
    assert!(*revisions);
    // S: Save anyway. The marks go, and the file holds the text.
    h.key(
        iced::keyboard::Key::Character("s".into()),
        Modifiers::empty(),
        Some("s"),
    );
    // The save runs on the worker; wait for it.
    for _ in 0..200 {
        if !h.app.d.session().unwrap().dirty {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run(vec![]);
    }
    assert!(h.app.prompt.is_none());
    let doc = &h.app.d.session().unwrap().doc;
    assert!(!doc.has_revision_data());
    assert!(
        std::fs::read_to_string(&p)
            .unwrap()
            .contains("Rain on the roof. Thunder.")
    );
}

#[test]
fn the_export_form_offers_colored_pages_with_revision_marks() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = app(home.path(), Choice::Light);
    start(&mut h);
    end_of(&mut h, 1);
    h.type_text(" Thunder.");
    action(&mut h, "app.export_pdf");
    assert!(h.app.export.as_ref().is_some_and(|f| !f.colored));
    h.shortcut('4', false);
    assert!(h.app.export.as_ref().is_some_and(|f| f.colored));
}
