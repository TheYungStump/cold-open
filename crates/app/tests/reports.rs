//! The report view in the running app (docs/specs/editor.md §20): opened
//! with its key, tabs changed with Ctrl+Tab, and a row taking the caret to
//! its scene. Driven headless with real key presses.

use std::path::Path;

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::{Headless, command_modifier};
use coldopen_app::theme::Choice;
use coldopen_app::{App, Boot};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain on the roof.\n\nMAYA\nListen.\n\nEXT. ROOF - NIGHT\n\nWind.\n\nTHEO\nWhat was that?\n";

fn app(home: &Path) -> Headless {
    let p = home.join("lab.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    let mut app = App::new(Boot {
        file: Some(p),
        dirs: Some(Dirs::under(home)),
        theme: Some(Choice::Light),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, iced::Size::new(1200.0, 800.0)).expect("a headless renderer")
}

#[test]
fn the_report_view_opens_switches_and_goes_to_a_scene() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    h.key(
        Key::Character("i".into()),
        command_modifier() | Modifiers::SHIFT,
        Some("I"),
    );
    let r = h.app.reports.as_ref().expect("the view opens");
    assert_eq!(r.tab, 0);
    let scenes = r.table().unwrap();
    assert_eq!(scenes.rows.len(), 2);
    assert_eq!(scenes.rows[1][2], "EXT. ROOF - NIGHT");
    // Ctrl+Tab: the characters, MAYA and THEO.
    h.named(Named::Tab, Modifiers::CTRL);
    let r = h.app.reports.as_ref().unwrap();
    assert_eq!(r.tab, 1);
    let names: Vec<&str> = r
        .table()
        .unwrap()
        .rows
        .iter()
        .map(|row| row[0].as_str())
        .collect();
    assert_eq!(names, ["THEO", "MAYA"], "THEO has more words");
    // Back to the scenes; Down and Enter go to the second scene.
    h.named(Named::Tab, Modifiers::CTRL | Modifiers::SHIFT);
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::Enter, Modifiers::empty());
    assert!(h.app.reports.is_none());
    let doc = &h.app.d.session().unwrap().doc;
    let at = doc.index_of(h.app.editor.caret.block).unwrap();
    assert_eq!(
        doc.blocks[at].element.text.plain_text(),
        "EXT. ROOF - NIGHT"
    );
}
