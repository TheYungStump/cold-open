//! Files in the running app (docs/specs/editor.md §11, §13): saving in
//! place on the worker thread, the unsaved-changes prompt on close and
//! quit, the recovery dialog after a crash, the start-up offer of leftover
//! work, and PDF export. Driven headless through real key presses.

use std::path::{Path, PathBuf};

use coldopen_app::files::{Ask, Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot, Message};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain.\n\nEXT. ROOF - NIGHT\n\nWind.\n";

fn app(home: &Path, file: Option<&Path>, answer: Option<PathBuf>) -> Headless {
    let mut app = App::new(Boot {
        file: file.map(Path::to_path_buf),
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(move |_| answer.clone()));
    Headless::new(app, iced::Size::new(1100.0, 800.0)).expect("a headless renderer")
}

fn script(dir: &Path) -> PathBuf {
    let p = dir.join("lab.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    p
}

fn press(h: &mut Headless, c: &str) {
    h.key(Key::Character(c.into()), Modifiers::empty(), Some(c));
}

/// Moves the caret to the end of the first action ("Rain.").
fn to_first_action(h: &mut Headless) {
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::End, Modifiers::empty());
}

#[test]
fn save_in_place_runs_on_the_worker_and_leaves_the_script_clean() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), Some(&p), None);
    to_first_action(&mut h);
    h.type_text(" Thunder.");
    assert!(h.app.d.session().unwrap().dirty);
    h.shortcut('s', false);
    let s = h.app.d.session().unwrap();
    assert!(!s.dirty, "{:?}", h.app.banners);
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        SCRIPT.replace("Rain.", "Rain. Thunder.")
    );
    h.app.d.shutdown();
}

#[test]
fn closing_with_changes_asks_and_each_answer_does_what_it_says() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), Some(&p), None);
    to_first_action(&mut h);
    h.type_text(" Thunder.");

    // Mod+W asks; Esc cancels and typing goes on in the page.
    h.shortcut('w', false);
    assert!(h.app.prompt.is_some());
    press(&mut h, "x"); // not an answer: nothing happens
    assert!(h.app.prompt.is_some());
    h.named(Named::Escape, Modifiers::empty());
    assert!(h.app.prompt.is_none());
    h.type_text("!");
    assert!(h.app.d.session().is_some());

    // D: don't save. The script closes and its journal goes with it.
    h.shortcut('w', false);
    press(&mut h, "d");
    assert!(h.app.d.session().is_none());
    assert_eq!(std::fs::read_to_string(&p).unwrap(), SCRIPT);
    assert!(!coldopen_core::journal::path_for(&p).exists());

    // The start screen: Mod+N starts a new script there.
    h.shortcut('n', false);
    assert!(h.app.d.session().is_some());
}

#[test]
fn quitting_with_changes_can_save_first() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), Some(&p), None);
    to_first_action(&mut h);
    h.type_text(" Thunder.");
    // The window's close button.
    h.run(vec![Message::CloseRequested]);
    assert!(h.app.prompt.is_some());
    assert!(!h.exited);
    h.named(Named::Enter, Modifiers::empty()); // Save
    assert!(h.exited);
    assert!(
        std::fs::read_to_string(&p)
            .unwrap()
            .contains("Rain. Thunder.")
    );
    assert!(!coldopen_core::journal::path_for(&p).exists());
}

#[test]
fn a_new_script_saved_as_moves_out_of_unsaved() {
    let home = tempfile::tempdir().unwrap();
    let target = home.path().join("pilot.fountain");
    let mut h = app(home.path(), None, Some(target.clone()));
    let first = h.app.d.session().unwrap().path.clone().unwrap();
    assert!(first.starts_with(Dirs::under(home.path()).unsaved()));
    // A fresh script is clean: quitting asks nothing.
    assert!(!h.app.d.session().unwrap().dirty);
    h.type_text("ext. pier - day");
    // Save as from the palette's action, by its shortcut.
    h.shortcut('s', true);
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "EXT. PIER - DAY\n"
    );
    assert!(!coldopen_core::journal::path_for(&first).exists());
    h.app.d.shutdown();
}

#[test]
fn a_crash_is_recovered_with_a_dialog_and_discard_goes_back_to_the_file() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    {
        let mut h = app(home.path(), Some(&p), None);
        to_first_action(&mut h);
        h.type_text(" Thunder.");
        // Dropped without closing: to the files on disk, a crash.
    }
    let mut h = app(home.path(), Some(&p), None);
    let r = h.app.recovery.clone().expect("the recovery dialog");
    // One command per key: " Thunder." is nine.
    assert_eq!(r.commands, 9);
    assert_eq!(r.scenes, ["INT. LAB - NIGHT"]);
    assert!(!r.external_change && !r.never_saved);
    assert!(r.last.is_some());
    // While the dialog is open, typing goes nowhere.
    press(&mut h, "q");
    assert!(
        h.app.d.session().unwrap().doc.blocks[1]
            .element
            .text
            .plain_text()
            .ends_with("Thunder.")
    );
    press(&mut h, "d"); // Discard
    assert!(h.app.recovery.is_none());
    let s = h.app.d.session().unwrap();
    assert_eq!(s.doc.blocks[1].element.text.plain_text(), "Rain.");
    assert!(!s.dirty);
    // The recovered work was set aside, not deleted.
    let aside = std::fs::read_dir(home.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains(".discarded-"));
    assert!(aside);
    h.app.d.shutdown();
}

#[test]
fn keep_carries_on_with_the_recovered_script() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    {
        let mut h = app(home.path(), Some(&p), None);
        to_first_action(&mut h);
        h.type_text(" Thunder.");
    }
    let mut h = app(home.path(), Some(&p), None);
    h.named(Named::Enter, Modifiers::empty()); // Keep
    assert!(h.app.recovery.is_none());
    let s = h.app.d.session().unwrap();
    assert!(s.dirty);
    assert_eq!(s.doc.blocks[1].element.text.plain_text(), "Rain. Thunder.");
    h.shortcut('s', false);
    assert!(
        std::fs::read_to_string(&p)
            .unwrap()
            .contains("Rain. Thunder.")
    );
    h.app.d.shutdown();
}

#[test]
fn unsaved_work_from_a_crash_is_offered_at_the_next_start() {
    let home = tempfile::tempdir().unwrap();
    {
        let mut h = app(home.path(), None, None);
        h.type_text("int. attic - dawn\nDust.");
    }
    let mut h = app(home.path(), None, None);
    assert_eq!(h.app.leftover.len(), 1);
    let left = h.app.leftover[0].clone();
    // The new script that start-up made is untouched, so reopening the
    // leftover work replaces it without a question.
    h.run(vec![Message::OpenPath(left)]);
    let r = h.app.recovery.clone().expect("the recovery dialog");
    assert!(r.never_saved);
    assert_eq!(r.scenes, ["INT. ATTIC - DAWN"]);
    h.named(Named::Enter, Modifiers::empty());
    let texts: Vec<String> = h
        .app
        .d
        .session()
        .unwrap()
        .doc
        .blocks
        .iter()
        .map(|b| b.element.text.plain_text())
        .collect();
    assert_eq!(texts, ["INT. ATTIC - DAWN", "Dust."]);
    assert!(h.app.leftover.is_empty());
    h.app.d.discard();
}

#[test]
fn export_pdf_takes_options_from_the_keyboard() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let pdf = home.path().join("lab");
    let mut h = app(home.path(), Some(&p), Some(pdf.clone()));
    h.shortcut('e', true);
    let f = h.app.export.clone().expect("the export form");
    assert!(!f.scene_numbers && f.title_page);
    // Mod+1 turns scene numbers on; the watermark field has focus.
    h.shortcut('1', false);
    h.type_text("DRAFT");
    let f = h.app.export.clone().unwrap();
    assert!(f.scene_numbers);
    assert_eq!(f.watermark, "DRAFT");
    h.named(Named::Enter, Modifiers::empty());
    assert!(h.app.export.is_none());
    let bytes = std::fs::read(pdf.with_extension("pdf")).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        h.app.banners.last().unwrap().starts_with("Exported 1 page"),
        "{:?}",
        h.app.banners
    );
    // Export never touches the script.
    assert!(!h.app.d.session().unwrap().dirty);
    let _ = Ask::Open;
    h.app.d.shutdown();
}
