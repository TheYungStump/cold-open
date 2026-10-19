//! Tracked changes in the running app (docs/specs/editor.md §17): tracked
//! typing in each theme's colors, the caret over struck text, accepting and
//! rejecting at the caret, by scene and all with undo, the changes panel's
//! keys, the name question, the lossy question's Save as Cold Open project,
//! and a bundle saved and reopened with its changes. Driven headless with
//! real key presses.

use std::path::{Path, PathBuf};

use coldopen_app::app::{Focus, Prompt};
use coldopen_app::files::{Ask, Dialogs, Dirs};
use coldopen_app::headless::{Headless, command_modifier};
use coldopen_app::theme::{Choice, Palette};
use coldopen_app::{App, Boot, Message};
use coldopen_core::FormatId;
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;
use serde_json::{Value, json};

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain on the roof.\n\nEXT. ROOF - NIGHT\n\nWind.\n";

fn open(path: &Path, home: &Path, theme: Choice) -> Headless {
    let mut app = App::new(Boot {
        file: Some(path.to_path_buf()),
        dirs: Some(Dirs::under(home)),
        theme: Some(theme),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, iced::Size::new(1200.0, 800.0)).expect("a headless renderer")
}

fn app(home: &Path, theme: Choice) -> (Headless, PathBuf) {
    let p = home.join("lab.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    (open(&p, home, theme), p)
}

/// The app with the writer's name set and their tracking on.
fn tracking(home: &Path) -> (Headless, PathBuf) {
    let (mut h, p) = app(home, Choice::Light);
    h.app.settings.author_name = Some("Dana".into());
    h.app.d.set_writer_name(Some("Dana".into()));
    action(&mut h, "changes.track_mine");
    assert!(h.app.prompt.is_none());
    assert!(tracks(&h));
    (h, p)
}

fn tracks(h: &Headless) -> bool {
    h.app.d.session().unwrap().doc.tracking.writer
}

fn action(h: &mut Headless, name: &str) {
    h.run(vec![Message::Action(name.into())]);
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

fn pending(h: &Headless) -> usize {
    h.app.d.session().unwrap().doc.pending().len()
}

fn list(h: &mut Headless) -> Vec<Value> {
    let cmd = coldopen_core::Command::from_parts("changes.list", json!({})).unwrap();
    h.app.d.execute(cmd).unwrap()["changes"]
        .as_array()
        .unwrap()
        .clone()
}

fn press(h: &mut Headless, key: Named, n: usize) {
    for _ in 0..n {
        h.named(key, Modifiers::empty());
    }
}

/// The caret to the end of block `i`.
fn end_of(h: &mut Headless, i: usize) {
    h.doc_start();
    press(h, Named::ArrowDown, i);
    press(h, Named::End, 1);
}

fn mod_shift(h: &mut Headless, key: Named) {
    h.named(key, command_modifier() | Modifiers::SHIFT);
}

/// Whether a frame holds a pixel of `c`, give or take rounding.
fn has_color(px: &[u8], c: iced::Color) -> bool {
    let want = [c.r, c.g, c.b].map(|v| (v * 255.0).round() as i32);
    px.as_chunks::<4>()
        .0
        .iter()
        .any(|p| (0..3).all(|k| (i32::from(p[k]) - want[k]).abs() <= 2))
}

#[test]
fn tracked_typing_shows_in_the_authors_color_in_each_theme() {
    for theme in [Choice::Light, Choice::Dark, Choice::HighContrast] {
        let home = tempfile::tempdir().unwrap();
        let (mut h, _) = app(home.path(), theme);
        h.app.settings.author_name = Some("Dana".into());
        h.app.d.set_writer_name(Some("Dana".into()));
        let first = Palette::of(theme, false).changes[0];
        let (_, before) = h.frame(1.0);
        assert!(!has_color(&before, first), "{theme:?}: the color is free");
        action(&mut h, "changes.track_mine");
        end_of(&mut h, 1);
        h.type_text(" Thunder.");
        assert_eq!(texts(&h)[1], "Rain on the roof. Thunder.");
        let l = list(&mut h);
        assert_eq!(l.len(), 1, "one change for the whole word");
        assert_eq!(l[0]["by"], json!({"kind": "writer", "name": "Dana"}));
        assert_eq!(l[0]["inserted"], " Thunder.");
        let (_, after) = h.frame(1.0);
        assert!(has_color(&after, first), "{theme:?}: the mark is drawn");
        h.app.d.discard();
    }
}

#[test]
fn the_caret_steps_over_struck_text() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = tracking(home.path());
    // Strike "on " out of "Rain on the roof.".
    h.doc_start();
    press(&mut h, Named::ArrowDown, 1);
    press(&mut h, Named::ArrowRight, 5);
    for _ in 0..3 {
        h.named(Named::ArrowRight, Modifiers::SHIFT);
    }
    press(&mut h, Named::Backspace, 1);
    assert_eq!(texts(&h)[1], "Rain the roof.");
    let shown = h.app.d.session().unwrap().doc.blocks[1]
        .element
        .text
        .shown()
        .plain_text();
    assert_eq!(shown, "Rain on the roof.", "the deletion still shows");
    assert_eq!(h.app.editor.caret.offset, 5);
    let x = |h: &mut Headless| h.app.editor.caret_spot(&mut h.app.d).unwrap().x.0;
    let at5 = x(&mut h);
    press(&mut h, Named::ArrowLeft, 1);
    let at4 = x(&mut h);
    press(&mut h, Named::ArrowRight, 2);
    assert_eq!(
        h.app.editor.caret.offset, 6,
        "one step past the struck text"
    );
    let at6 = x(&mut h);
    let cell = at5 - at4;
    assert!(cell > 0);
    assert_eq!(
        at6 - at5,
        4 * cell,
        "the caret jumps the three struck cells"
    );
    h.app.d.discard();
}

#[test]
fn accept_and_reject_at_the_caret_by_scene_and_all_each_undo() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = tracking(home.path());
    end_of(&mut h, 1);
    h.type_text(" Thunder.");
    assert_eq!(pending(&h), 1);
    // At the caret: accept, undo, reject, undo.
    mod_shift(&mut h, Named::Enter);
    assert_eq!(pending(&h), 0);
    assert_eq!(texts(&h)[1], "Rain on the roof. Thunder.");
    h.shortcut('z', false);
    assert_eq!(pending(&h), 1);
    mod_shift(&mut h, Named::Backspace);
    assert_eq!(texts(&h)[1], "Rain on the roof.");
    h.shortcut('z', false);
    assert_eq!(texts(&h)[1], "Rain on the roof. Thunder.");
    assert_eq!(pending(&h), 1);
    // Nothing at the caret: a banner, no change.
    h.doc_start();
    h.app.banners.clear();
    mod_shift(&mut h, Named::Enter);
    assert!(
        h.app
            .banners
            .iter()
            .any(|b| b.contains("no tracked change"))
    );
    assert_eq!(pending(&h), 1);

    // By scene: only the caret's scene.
    end_of(&mut h, 3);
    h.type_text(" Cold.");
    assert_eq!(pending(&h), 2);
    action(&mut h, "changes.reject_scene");
    assert_eq!(texts(&h)[3], "Wind.");
    assert_eq!(pending(&h), 1);
    h.shortcut('z', false);
    assert_eq!(texts(&h)[3], "Wind. Cold.");

    // F7 and Shift+F7 move between them, round the ends.
    h.doc_start();
    press(&mut h, Named::F7, 1);
    assert_eq!(h.app.editor.caret.offset, 17);
    let first = h.app.editor.caret.block;
    press(&mut h, Named::F7, 1);
    assert_ne!(h.app.editor.caret.block, first);
    press(&mut h, Named::F7, 1);
    assert_eq!(h.app.editor.caret.block, first, "round to the first");
    h.named(Named::F7, Modifiers::SHIFT);
    assert_ne!(h.app.editor.caret.block, first, "round to the last");

    // All: asked first, Esc keeps them, Enter settles them.
    action(&mut h, "changes.accept_all");
    assert!(matches!(
        h.app.prompt,
        Some(Prompt::SettleAll {
            accept: true,
            count: 2
        })
    ));
    press(&mut h, Named::Escape, 1);
    assert!(h.app.prompt.is_none());
    assert_eq!(pending(&h), 2);
    action(&mut h, "changes.accept_all");
    press(&mut h, Named::Enter, 1);
    assert_eq!(pending(&h), 0);
    assert_eq!(texts(&h)[3], "Wind. Cold.");
    h.shortcut('z', false);
    assert_eq!(pending(&h), 2);
    h.app.d.discard();
}

#[test]
fn the_changes_panel_lists_and_settles_from_the_keyboard() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = tracking(home.path());
    end_of(&mut h, 1);
    h.type_text(" Thunder.");
    end_of(&mut h, 3);
    h.type_text(" Cold.");
    // Mod+Shift+R shows the panel; the status bar counts the changes.
    h.shortcut('r', true);
    assert!(h.app.settings.changes_panel);
    assert_eq!(h.app.review, (2, 0));
    assert_eq!(h.app.changes.rows.len(), 2);
    assert_eq!(h.app.changes.rows[0].author, "Dana");
    assert_eq!(h.app.changes.rows[0].what, "Inserted");
    assert_eq!(h.app.changes.rows[0].inserted, " Thunder.");
    h.frame(1.0);
    // F6 reaches it past the navigator.
    press(&mut h, Named::F6, 2);
    assert_eq!(h.app.focus, Focus::Changes);
    press(&mut h, Named::ArrowDown, 3);
    assert_eq!(h.app.changes.selected, 1, "it stops at the end");
    press(&mut h, Named::ArrowUp, 1);
    assert_eq!(h.app.changes.selected, 0);
    // Enter moves the caret to the change (it was at the end of the
    // script).
    press(&mut h, Named::Enter, 1);
    let b1 = h.app.d.session().unwrap().doc.blocks[1].id;
    assert_eq!(
        (h.app.editor.caret.block, h.app.editor.caret.offset),
        (b1, 17)
    );
    // A accepts it and the next row is chosen; R rejects that one.
    h.key(
        iced::keyboard::Key::Character("a".into()),
        Modifiers::empty(),
        Some("a"),
    );
    assert_eq!(h.app.changes.rows.len(), 1);
    assert_eq!(h.app.changes.rows[0].inserted, " Cold.");
    assert_eq!(texts(&h)[1], "Rain on the roof. Thunder.", "accepted");
    h.key(
        iced::keyboard::Key::Character("r".into()),
        Modifiers::empty(),
        Some("r"),
    );
    assert!(h.app.changes.rows.is_empty());
    assert_eq!(texts(&h)[3], "Wind.", "rejected");
    assert_eq!(h.app.review, (0, 0));
    h.frame(1.0);
    // Esc gives the keyboard back to the page.
    press(&mut h, Named::Escape, 1);
    assert_eq!(h.app.focus, Focus::Page);
    // The filter shows one author's changes.
    h.type_text("x");
    // Typing in body text refreshes the panel at the next tick.
    h.run(vec![Message::Tick]);
    h.run(vec![Message::ChangesFilter("Someone else".into())]);
    assert!(h.app.changes.visible().is_empty());
    h.run(vec![Message::ChangesFilter(
        coldopen_app::app::ALL_AUTHORS.into(),
    )]);
    assert_eq!(h.app.changes.visible().len(), 1);
    h.app.d.discard();
}

#[test]
fn the_name_question_keeps_a_name_on_enter_and_goes_on_without_on_esc() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = app(home.path(), Choice::Light);
    action(&mut h, "changes.track_mine");
    match &h.app.prompt {
        Some(Prompt::AuthorName { name, .. }) => assert_eq!(name, "Writer"),
        other => panic!("{other:?}"),
    }
    // The field has the keyboard, with the default selected.
    h.type_text("Dana");
    match &h.app.prompt {
        Some(Prompt::AuthorName { name, .. }) => assert_eq!(name, "Dana"),
        other => panic!("{other:?}"),
    }
    press(&mut h, Named::Enter, 1);
    assert!(h.app.prompt.is_none());
    assert_eq!(h.app.settings.author_name.as_deref(), Some("Dana"));
    assert!(tracks(&h), "the switch went on");
    let saved =
        std::fs::read_to_string(Dirs::under(home.path()).config.join("settings.json")).unwrap();
    assert!(saved.contains("\"author_name\": \"Dana\""), "{saved}");
    end_of(&mut h, 1);
    h.type_text("!");
    assert_eq!(list(&mut h)[0]["by"]["name"], "Dana");
    h.app.d.discard();

    // Esc: tracking goes on, the changes say "Writer", and the question
    // is not asked again this session.
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = app(home.path(), Choice::Light);
    action(&mut h, "changes.track_mine");
    press(&mut h, Named::Escape, 1);
    assert!(h.app.prompt.is_none());
    assert!(tracks(&h));
    assert_eq!(h.app.settings.author_name, None);
    end_of(&mut h, 1);
    h.type_text("!");
    assert_eq!(list(&mut h)[0]["by"], json!({"kind": "writer"}));
    action(&mut h, "changes.track_mine");
    action(&mut h, "changes.track_mine");
    assert!(h.app.prompt.is_none(), "asked once");
    assert!(tracks(&h));
    // The palette can still set it.
    action(&mut h, "app.author_name");
    assert!(matches!(h.app.prompt, Some(Prompt::AuthorName { .. })));
    h.type_text("Sam");
    press(&mut h, Named::Enter, 1);
    assert_eq!(h.app.settings.author_name.as_deref(), Some("Sam"));
    h.app.d.discard();
}

#[test]
fn the_lossy_question_saves_as_a_project_and_the_bundle_reopens_with_its_changes() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, script) = tracking(home.path());
    end_of(&mut h, 1);
    h.type_text(" Thunder.");
    let bundle = home.path().join("lab.coldopen");
    let to = bundle.clone();
    h.app.set_dialogs(Dialogs::scripted(move |ask| match ask {
        Ask::SaveAsProject(name) => {
            assert_eq!(name, "lab.coldopen");
            Some(to.clone())
        }
        _ => None,
    }));
    // Mod+S on the Fountain script asks; Enter takes the project.
    h.shortcut('s', false);
    match &h.app.prompt {
        Some(Prompt::Lossy {
            tracked, message, ..
        }) => {
            assert_eq!(*tracked, 1);
            assert!(
                message.contains("can't hold tracked changes (1)"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    h.frame(1.0);
    press(&mut h, Named::Enter, 1);
    assert!(h.app.prompt.is_none());
    let s = h.app.d.session().unwrap();
    assert_eq!(s.format, FormatId::Bundle);
    assert_eq!(s.path.as_deref(), Some(bundle.as_path()));
    assert!(!s.dirty);
    assert_eq!(pending(&h), 1);
    assert_eq!(
        std::fs::read_to_string(&script).unwrap(),
        SCRIPT,
        "the Fountain file was left as it was"
    );

    // Closed and reopened, the change is still there, with its author.
    action(&mut h, "app.close");
    h.run(vec![Message::OpenPath(bundle.clone())]);
    assert_eq!(pending(&h), 1);
    assert!(tracks(&h), "the bundle keeps the switch");
    assert!(
        h.app.prompt.is_none(),
        "the name is set, so nothing is asked"
    );
    assert_eq!(list(&mut h)[0]["by"]["name"], "Dana");
    assert_eq!(texts(&h)[1], "Rain on the roof. Thunder.");
    h.app.d.discard();

    // Without a name, opening a script that tracks the writer asks once.
    let home2 = tempfile::tempdir().unwrap();
    let mut h = open(&bundle, home2.path(), Choice::Light);
    assert!(matches!(h.app.prompt, Some(Prompt::AuthorName { .. })));
    press(&mut h, Named::Escape, 1);
    assert!(h.app.prompt.is_none());
    assert!(tracks(&h), "Esc changes nothing");
    h.app.d.discard();
}

#[test]
fn save_anyway_writes_the_current_text_and_p_and_s_answer_the_question() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, script) = tracking(home.path());
    end_of(&mut h, 1);
    h.type_text(" Thunder.");
    h.shortcut('s', false);
    assert!(matches!(h.app.prompt, Some(Prompt::Lossy { .. })));
    // P opens the project dialog; cancelling it saves nothing.
    h.key(
        iced::keyboard::Key::Character("p".into()),
        Modifiers::empty(),
        Some("p"),
    );
    assert!(h.app.prompt.is_none());
    assert!(h.app.d.session().unwrap().dirty);
    // S saves anyway: the file holds the script with the change accepted,
    // and the change stays pending here.
    h.shortcut('s', false);
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
    let text = std::fs::read_to_string(&script).unwrap();
    assert!(text.contains("Rain on the roof. Thunder."), "{text}");
    assert_eq!(pending(&h), 1);
    h.app.d.discard();
}

#[test]
fn the_export_form_prints_the_current_text_on_mod_3() {
    let home = tempfile::tempdir().unwrap();
    let (mut h, _) = tracking(home.path());
    end_of(&mut h, 1);
    h.type_text(" Thunder.");
    let pdf = home.path().join("lab.pdf");
    let to = pdf.clone();
    h.app.set_dialogs(Dialogs::scripted(move |ask| match ask {
        Ask::ExportPdf(_) => Some(to.clone()),
        _ => None,
    }));
    h.shortcut('e', true);
    assert!(h.app.export.is_some());
    h.frame(1.0);
    h.shortcut('3', false);
    assert!(h.app.export.as_ref().unwrap().accepted);
    press(&mut h, Named::Enter, 1);
    assert!(pdf.exists());
    assert!(h.app.export.is_none());
    h.app.d.discard();
}
