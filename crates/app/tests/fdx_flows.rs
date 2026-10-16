//! .fdx files and set-aside copies in the running app (docs/specs/editor.md
//! §11, §13): open, edit, save and reopen an .fdx file; Save as converts;
//! Save a copy; the repair report; Repair a file; notices that never show
//! a copy's path; Recovered versions; the start-up clean-up. Driven
//! headless through real key presses.

use std::path::{Path, PathBuf};

use coldopen_app::app::Prompt;
use coldopen_app::files::{Ask, Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot, Message};
use coldopen_core::{Command, Dispatcher, FormatId, Options};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use serde_json::json;

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain.\n";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fdx")
        .join(name)
}

/// A copy of a fixture in `dir`.
fn copied(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::copy(fixture(name), &p).unwrap();
    p
}

fn app_with(
    home: &Path,
    file: Option<&Path>,
    answer: impl Fn(&Ask) -> Option<PathBuf> + Send + Sync + 'static,
) -> Headless {
    let mut app = App::new(Boot {
        file: file.map(Path::to_path_buf),
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(answer));
    Headless::new(app, iced::Size::new(1100.0, 800.0)).expect("a headless renderer")
}

fn app(home: &Path, file: Option<&Path>) -> Headless {
    app_with(home, file, |_| None)
}

fn press(h: &mut Headless, c: &str) {
    h.key(Key::Character(c.into()), Modifiers::empty(), Some(c));
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

/// Puts the caret at the end of the block with this text.
fn to_end_of(h: &mut Headless, text: &str) {
    let s = h.app.d.session().unwrap();
    let b = s
        .doc
        .blocks
        .iter()
        .find(|b| b.element.text.plain_text() == text)
        .unwrap();
    let c = coldopen_app::editor::Caret {
        block: b.id,
        offset: b.element.text.char_len(),
    };
    h.app.editor.caret = c;
    h.app.editor.anchor = c;
}

/// A script with changes set aside with Don't save, by core alone.
fn set_aside(dir: &Path) -> (PathBuf, PathBuf) {
    let p = dir.join("old.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let mut run = |c: &str, params| d.execute(Command::from_parts(c, params).unwrap()).unwrap();
    run("doc.open", json!({ "path": p }));
    run(
        "block.insert",
        json!({ "type": "action", "text": "Thunder, set aside." }),
    );
    let r = run("doc.close", json!({ "discard": true }));
    let copy = PathBuf::from(r["warnings"][0]["details"]["copy"].as_str().unwrap());
    (p, copy)
}

fn no_paths(h: &Headless) {
    for b in &h.app.banners {
        assert!(!b.contains(".coldopen"), "{b}");
        assert!(!b.contains(std::path::MAIN_SEPARATOR), "{b}");
    }
}

#[test]
fn an_fdx_file_opens_edits_saves_and_reopens_with_nothing_lost() {
    let home = tempfile::tempdir().unwrap();
    let p = copied(home.path(), "basic.fdx");
    let before = texts(&app(home.path(), Some(&p)));
    let mut h = app(home.path(), Some(&p));
    assert_eq!(h.app.d.session().unwrap().format, FormatId::Fdx);
    assert!(h.app.repair.is_none());
    let first_action = h
        .app
        .d
        .session()
        .unwrap()
        .doc
        .blocks
        .iter()
        .find(|b| b.element.kind == coldopen_screenplay::ElementKind::Action)
        .unwrap()
        .element
        .text
        .plain_text();
    to_end_of(&mut h, &first_action);
    h.type_text(" Thunder.");
    h.shortcut('s', false);
    assert!(!h.app.d.session().unwrap().dirty, "{:?}", h.app.banners);
    h.app.d.shutdown();
    let bytes = std::fs::read_to_string(&p).unwrap();
    assert!(bytes.starts_with("<?xml"), "still an .fdx file");

    let h = app(home.path(), Some(&p));
    let after = texts(&h);
    let edited = format!("{first_action} Thunder.");
    let expected: Vec<String> = before
        .iter()
        .map(|t| {
            if *t == first_action {
                edited.clone()
            } else {
                t.clone()
            }
        })
        .collect();
    assert_eq!(after, expected);
    assert!(h.app.settings.recent.contains(&p));
}

#[test]
fn save_as_converts_and_a_bare_name_keeps_the_format() {
    let home = tempfile::tempdir().unwrap();
    let p = copied(home.path(), "basic.fdx");
    let bare = home.path().join("second");
    let to = bare.clone();
    let mut h = app_with(home.path(), Some(&p), move |a| match a {
        Ask::SaveAs(name) => {
            assert_eq!(name, "basic.fdx");
            Some(to.clone())
        }
        _ => None,
    });
    action(&mut h, "app.save_as");
    let second = home.path().join("second.fdx");
    assert_eq!(
        h.app.d.session().unwrap().path.as_deref(),
        Some(second.as_path())
    );
    assert!(
        std::fs::read_to_string(&second)
            .unwrap()
            .starts_with("<?xml")
    );

    // To Fountain: what Fountain can't hold is asked about first, in the
    // target format's words.
    let fountain = home.path().join("third.fountain");
    let to = fountain.clone();
    h.app
        .set_dialogs(Dialogs::scripted(move |_| Some(to.clone())));
    action(&mut h, "app.save_as");
    if let Some(Prompt::Lossy { path, export, .. }) = &h.app.prompt {
        assert_eq!(path.as_deref(), Some(fountain.as_path()));
        assert!(!export);
        h.named(Named::Enter, Modifiers::empty());
    }
    let s = h.app.d.session().unwrap();
    assert_eq!(s.format, FormatId::Fountain);
    assert_eq!(s.path.as_deref(), Some(fountain.as_path()));
    assert!(std::fs::read_to_string(&fountain).unwrap().contains("INT."));
    h.app.d.shutdown();
}

#[test]
fn save_a_copy_writes_the_other_format_and_leaves_the_script() {
    let home = tempfile::tempdir().unwrap();
    let p = home.path().join("lab.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    let out = home.path().join("lab-copy");
    let to = out.clone();
    let mut h = app_with(home.path(), Some(&p), move |a| match a {
        Ask::ExportFdx(name) => {
            assert_eq!(name, "lab.fdx");
            Some(to.clone())
        }
        _ => None,
    });
    action(&mut h, "app.export_fdx");
    let written = home.path().join("lab-copy.fdx");
    assert!(
        std::fs::read_to_string(&written)
            .unwrap()
            .starts_with("<?xml"),
        "{:?}",
        h.app.banners
    );
    let s = h.app.d.session().unwrap();
    assert_eq!(s.path.as_deref(), Some(p.as_path()));
    assert_eq!(s.format, FormatId::Fountain);
    assert_eq!(
        h.app.banners.last().map(|b| b.text.as_str()),
        Some("Saved a copy as lab-copy.fdx.")
    );
    h.app.d.shutdown();
}

#[test]
fn a_repaired_file_shows_its_report_and_the_first_save_keeps_the_original() {
    let home = tempfile::tempdir().unwrap();
    let p = copied(home.path(), "broken-markup.fdx");
    let damaged = std::fs::read(&p).unwrap();
    let mut h = app(home.path(), Some(&p));
    let r = h.app.repair.clone().expect("the repair report");
    assert!(r.fixed + r.dropped > 0);
    assert!(r.written.is_none());
    assert!(!r.issues.is_empty());
    // Repairs are the report's to show, not banners.
    assert!(h.app.banners.is_empty(), "{:?}", h.app.banners);
    press(&mut h, "d");
    assert!(h.app.repair.as_ref().unwrap().details);
    h.named(Named::Enter, Modifiers::empty());
    assert!(h.app.repair.is_none());
    assert_eq!(std::fs::read(&p).unwrap(), damaged, "nothing written yet");

    to_end_of(&mut h, "Sparks fly from the grinder.");
    h.type_text(" Smoke.");
    h.shortcut('s', false);
    let last = h.app.banners.last().expect("a notice");
    assert!(last.recovered, "{:?}", h.app.banners);
    assert!(last.starts_with("The file as it was before the repair was kept"));
    no_paths(&h);
    let originals = coldopen_core::recovered::of_script(&p);
    assert_eq!(originals.len(), 1);
    assert_eq!(std::fs::read(&originals[0].copy).unwrap(), damaged);
    h.app.d.shutdown();
}

#[test]
fn save_repaired_copy_runs_save_as() {
    let home = tempfile::tempdir().unwrap();
    let p = copied(home.path(), "broken-markup.fdx");
    let out = home.path().join("fixed.fdx");
    let to = out.clone();
    let mut h = app_with(home.path(), Some(&p), move |a| match a {
        Ask::SaveAs(name) => {
            assert_eq!(name, "broken-markup.fdx");
            Some(to.clone())
        }
        _ => None,
    });
    press(&mut h, "s");
    assert!(h.app.repair.is_none());
    assert!(out.exists());
    assert_eq!(
        h.app.d.session().unwrap().path.as_deref(),
        Some(out.as_path())
    );
    h.app.d.shutdown();
}

#[test]
fn repair_a_file_writes_a_repaired_copy_and_reports() {
    let home = tempfile::tempdir().unwrap();
    let input = copied(home.path(), "broken-markup.fdx");
    let damaged = std::fs::read(&input).unwrap();
    let (i, o) = (input.clone(), home.path().join("mended"));
    let mut h = app_with(home.path(), None, move |a| match a {
        Ask::RepairIn => Some(i.clone()),
        Ask::RepairOut(from, name) => {
            assert_eq!(from, &i);
            assert_eq!(name, "broken-markup (repaired).fdx");
            Some(o.clone())
        }
        _ => None,
    });
    action(&mut h, "app.repair");
    let out = home.path().join("mended.fdx");
    let r = h.app.repair.clone().expect("the report");
    assert_eq!(r.written.as_deref(), Some("mended.fdx"));
    assert!(r.fixed + r.dropped > 0);
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .contains("Sparks fly")
    );
    assert_eq!(std::fs::read(&input).unwrap(), damaged);
    h.named(Named::Escape, Modifiers::empty());
    assert!(h.app.repair.is_none());
}

#[test]
fn dont_save_says_the_changes_were_set_aside_without_a_path() {
    let home = tempfile::tempdir().unwrap();
    let p = home.path().join("lab.fountain");
    std::fs::write(&p, SCRIPT).unwrap();
    let mut h = app(home.path(), Some(&p));
    to_end_of(&mut h, "Rain.");
    h.type_text(" Thunder.");
    h.shortcut('w', false);
    press(&mut h, "d");
    let last = h.app.banners.last().expect("a notice");
    assert_eq!(
        last.text,
        "Your unsaved changes were set aside. Open Recovered versions to restore them."
    );
    assert!(last.recovered);
    no_paths(&h);
    // Its button opens Recovered versions, which lists that copy.
    action(&mut h, "app.recovered");
    let s = h.app.recovered.as_ref().expect("the screen");
    assert_eq!(s.rows.len(), 1);
    assert!(s.preview.iter().any(|(_, t)| t == "Rain. Thunder."));
}

#[test]
fn recovered_versions_previews_restores_and_deletes() {
    let home = tempfile::tempdir().unwrap();
    let (old, copy) = set_aside(home.path());
    let mut h = app(home.path(), None);
    h.app.settings.recent = vec![old.clone()];

    action(&mut h, "app.recovered");
    let s = h.app.recovered.clone().expect("the screen");
    assert_eq!(s.rows.len(), 1);
    assert_eq!(s.rows[0].copy.copy, copy);
    assert!(s.rows[0].readable && !s.rows[0].expires);
    assert!(s.preview.iter().any(|(_, t)| t == "Thunder, set aside."));
    // Shown, so marked as offered.
    let offered = coldopen_app::files::load_offered(&Dirs::under(home.path()));
    assert!(offered.contains_key(&copy.display().to_string()));

    // Delete asks; Esc keeps the copy.
    h.named(Named::Delete, Modifiers::empty());
    assert!(matches!(h.app.prompt, Some(Prompt::DeleteCopy(_))));
    h.named(Named::Escape, Modifiers::empty());
    assert!(copy.exists());
    assert!(h.app.recovered.is_some());

    // Restore: a new untitled script, nothing overwritten.
    let old_bytes = std::fs::read(&old).unwrap();
    press(&mut h, "r");
    assert!(h.app.recovered.is_none());
    let s = h.app.d.session().unwrap();
    let path = s.path.clone().unwrap();
    assert_eq!(
        path,
        Dirs::under(home.path())
            .unsaved()
            .join("old (restored).fountain")
    );
    assert!(s.dirty);
    assert!(texts(&h).iter().any(|t| t == "Thunder, set aside."));
    assert_eq!(std::fs::read(&old).unwrap(), old_bytes);
    assert!(copy.exists());
    no_paths(&h);
    // A crash: the restored script is offered at the next start.
    drop(h);
    let h = app(home.path(), None);
    assert!(h.app.leftover.contains(&path), "{:?}", h.app.leftover);

    // Delete, confirmed: the copy goes and the list says so.
    let mut h = app(home.path(), None);
    h.app.settings.recent = vec![old.clone()];
    action(&mut h, "app.recovered");
    h.named(Named::Delete, Modifiers::empty());
    h.named(Named::Enter, Modifiers::empty());
    assert!(!copy.exists());
    assert!(h.app.recovered.as_ref().unwrap().rows.is_empty());
}

#[test]
fn start_up_removes_only_old_copies_already_shown() {
    let home = tempfile::tempdir().unwrap();
    let dirs = Dirs::under(home.path());
    let script = home.path().join("s.fountain");
    std::fs::write(&script, SCRIPT).unwrap();
    let day = 24 * 60 * 60 * 1000u64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let copy = |tag: &str, at: u64| {
        let p = home
            .path()
            .join(format!(".s.fountain.coldopen-journal.{tag}-{at}"));
        std::fs::write(&p, "{}\n").unwrap();
        p
    };
    let old_shown = copy("discarded", now - 40 * day);
    let old_unseen = copy("discarded", now - 41 * day);
    let new_shown = copy("diverged", now - 2 * day);
    let gone = home.path().join(".gone.fountain.coldopen-original-1");
    let offered = [&old_shown, &new_shown, &gone]
        .iter()
        .map(|p| (p.display().to_string(), now - day))
        .collect();
    coldopen_app::files::save_offered(&dirs, &offered).unwrap();
    let mut settings = coldopen_app::files::Settings::default();
    settings.add_recent(&script);
    settings.save(&dirs).unwrap();

    let _h = app(home.path(), None);
    assert!(!old_shown.exists());
    assert!(old_unseen.exists(), "never shown, so never removed");
    assert!(new_shown.exists());
    let offered = coldopen_app::files::load_offered(&dirs);
    assert_eq!(
        offered.keys().cloned().collect::<Vec<_>>(),
        [new_shown.display().to_string()]
    );
}
