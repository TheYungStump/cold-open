//! The local usage summary (docs/specs/usage-summary.md §10): nothing
//! typed, named or saved gets into it; off means no writes; Reset; and
//! Share feedback sends what its sheet shows. Driven headless.

use std::path::{Path, PathBuf};

use coldopen_app::app::ShareInput;
use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::usage::{self, Read};
use coldopen_app::{App, Boot, Message};
use coldopen_core::Command;
use coldopen_screenplay::ElementKind;
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;
use iced::widget::text_editor;
use serde_json::json;

const TYPED: &str = "Zqxtypedsecret";
const CHARACTER: &str = "ZQXCHARSECRET";
const SCENE: &str = "ZQXSCENESECRET";
const FOLDER: &str = "Zqx folder secret";
const FILE: &str = "zqx-file-secret";

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain on the glass.\n\nMAYA\nWe should go.\n";

/// A script in a folder whose name, like the file's, must never be kept.
fn script(home: &Path) -> PathBuf {
    let dir = home.join(FOLDER);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(format!("{FILE}.fountain"));
    std::fs::write(&p, SCRIPT).unwrap();
    p
}

fn app(home: &Path, file: &Path, answer: Option<PathBuf>) -> Headless {
    let mut app = App::new(Boot {
        file: Some(file.to_path_buf()),
        dirs: Some(Dirs::under(home)),
        count_usage: true,
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(move |_| answer.clone()));
    Headless::new(app, iced::Size::new(1100.0, 800.0)).expect("a headless renderer")
}

fn usage_file(home: &Path) -> PathBuf {
    usage::path(&Dirs::under(home).data)
}

/// Sets a block's text through the dispatcher, as the writer.
fn rename(h: &mut Headless, kind: ElementKind, text: &str) {
    let id = h
        .app
        .d
        .session()
        .unwrap()
        .doc
        .blocks
        .iter()
        .find(|b| b.element.kind == kind)
        .unwrap()
        .id;
    let cmd = Command::from_parts("block.set_text", json!({ "id": id, "text": text })).unwrap();
    h.app.d.execute(cmd).unwrap();
    let _ = h.app.refresh();
}

/// Typing, renaming a character and a scene, saving in place and saving
/// as a new file with a new name.
fn session(h: &mut Headless, save_as: &Path) {
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::End, Modifiers::empty());
    h.type_text(&format!(" {TYPED}."));
    rename(h, ElementKind::Character, CHARACTER);
    rename(h, ElementKind::SceneHeading, &format!("INT. {SCENE} - DAY"));
    h.shortcut('s', false);
    assert!(!h.app.d.session().unwrap().dirty, "{:?}", h.app.banners);
    h.run(vec![Message::Action("app.save_as".into())]);
    assert!(save_as.exists(), "{:?}", h.app.banners);
}

fn private_words(home: &Path) -> Vec<String> {
    let mut v = vec![
        TYPED.to_string(),
        CHARACTER.to_string(),
        SCENE.to_string(),
        FOLDER.to_string(),
        FILE.to_string(),
        "Rain on the glass".to_string(),
        "MAYA".to_string(),
        home.display().to_string(),
    ];
    for var in ["USERNAME", "USER"] {
        if let Ok(u) = std::env::var(var)
            && u.len() > 2
        {
            v.push(u);
        }
    }
    v
}

fn assert_clean(text: &str, home: &Path) {
    let lower = text.to_lowercase();
    for w in private_words(home) {
        assert!(
            !lower.contains(&w.to_lowercase()),
            "{w:?} got into the usage summary:\n{text}"
        );
    }
}

#[test]
fn nothing_typed_named_or_saved_gets_into_the_summary() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let save_as = p.with_file_name(format!("{FILE} renamed.fountain"));
    let mut h = app(home.path(), &p, Some(save_as.clone()));
    session(&mut h, &save_as);
    // The View sheet writes what's waiting first (§5).
    h.run(vec![Message::Action("app.usage_summary".into())]);
    let shown = h.app.usage.view.clone();

    let raw = std::fs::read_to_string(usage_file(home.path())).expect("usage.json was written");
    assert_clean(&raw, home.path());
    assert_clean(&shown, home.path());
    let Read::Ok(s) = usage::read(&usage_file(home.path())) else {
        panic!("usage.json can't be read back:\n{raw}")
    };
    assert!(s.keys_known(), "{raw}");
    assert_eq!(s.sessions, 1);
    assert_eq!(s.opened.get("fountain"), Some(&1));
    assert_eq!(s.saved.get("fountain"), Some(&2), "{raw}");
    assert!(s.commands["text.insert"] >= 1, "{raw}");
    assert_eq!(s.commands.get("block.set_text"), Some(&2), "{raw}");
    assert!(s.commands.contains_key("doc.save"), "{raw}");
    assert!(s.commands.contains_key("app.save_as"), "{raw}");
    assert!(s.commands.contains_key("app.usage_summary"), "{raw}");
    assert!(s.agent_commands.is_empty());
    assert_eq!(s.since, usage::today_utc());
    assert_eq!(shown, s.text());
    h.app.d.shutdown();
}

#[test]
fn counting_off_writes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let config = Dirs::under(home.path()).config;
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("settings.json"), r#"{"count_usage": false}"#).unwrap();
    let p = script(home.path());
    let save_as = p.with_file_name("second.fountain");
    let mut h = app(home.path(), &p, Some(save_as.clone()));
    session(&mut h, &save_as);
    h.run(vec![Message::Action("app.usage_summary".into())]);
    assert!(
        !usage_file(home.path()).exists(),
        "off, yet a file was written"
    );
    assert!(
        h.app.usage.view.contains("counting is off"),
        "{}",
        h.app.usage.view
    );
    h.app.d.shutdown();

    // With a file already there, off leaves its bytes and time alone.
    let f = usage_file(home.path());
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    let before = r#"{"version":1,"since":"2026-11-01","commands":{"undo":3}}"#;
    std::fs::write(&f, before).unwrap();
    let modified = std::fs::metadata(&f).unwrap().modified().unwrap();
    let third = p.with_file_name("third.fountain");
    let mut h = app(home.path(), &p, Some(third.clone()));
    session(&mut h, &third);
    h.run(vec![Message::Action("app.usage_summary".into())]);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), before);
    assert_eq!(std::fs::metadata(&f).unwrap().modified().unwrap(), modified);
    // Viewing still shows it.
    assert!(h.app.usage.view.contains("undo 3"), "{}", h.app.usage.view);
    h.app.d.shutdown();
}

#[test]
fn turning_counting_off_drops_what_waited_and_reset_deletes_the_file() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), &p, None);
    h.run(vec![Message::Action("app.usage_summary".into())]);
    let f = usage_file(home.path());
    assert!(f.exists());
    h.type_text("x");
    h.run(vec![Message::Action("app.usage_toggle".into())]);
    assert!(!h.app.settings.count_usage);
    let off = std::fs::read_to_string(&f).unwrap();
    h.type_text("y");
    h.run(vec![Message::Action("app.usage_summary".into())]);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), off);

    // Reset asks, then deletes the file.
    h.run(vec![Message::Usage(coldopen_app::app::UsageInput::Reset)]);
    assert!(f.exists());
    h.named(Named::Enter, Modifiers::empty());
    assert!(!f.exists());
    h.app.d.shutdown();
}

#[test]
fn share_feedback_sends_only_what_the_sheet_shows() {
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), &p, None);
    h.type_text("z");
    h.run(vec![Message::Action("app.share_feedback".into())]);
    for c in "Lovely app.".chars() {
        h.run(vec![Message::Share(ShareInput::Edit(
            text_editor::Action::Edit(text_editor::Edit::Insert(c)),
        ))]);
    }
    h.run(vec![Message::Share(ShareInput::Include)]);
    // Copy all leaves the sheet open, the text and the tick as they were
    // (editor.md §23): the issue below still carries both.
    h.run(vec![Message::Share(ShareInput::CopyAll)]);
    assert_eq!(h.app.sheet, Some(coldopen_app::app::Sheet::ShareFeedback));
    h.run(vec![Message::Share(ShareInput::Issue)]);
    let opened = h.app.opened.clone().unwrap();
    let url = opened.last().expect("an address was opened");
    assert!(url.contains("template=feedback.yml"), "{url}");
    assert!(url.contains("&what=Lovely%20app."), "{url}");
    assert!(
        url.contains("&usage=Usage%20summary%20from%20Cold%20Open"),
        "{url}"
    );
    assert_clean(&url.replace("%20", " "), home.path());

    // Without the tick, nothing of it.
    h.run(vec![Message::Action("app.share_feedback".into())]);
    h.run(vec![Message::Share(ShareInput::Issue)]);
    let url = h.app.opened.clone().unwrap().last().unwrap().clone();
    assert!(!url.contains("usage="), "{url}");
    h.app.d.shutdown();
}

#[test]
fn settings_works_from_the_keyboard_alone() {
    use coldopen_app::app::Sheet;
    use coldopen_app::files::CrashReports;
    use coldopen_app::theme::Choice;
    let home = tempfile::tempdir().unwrap();
    let p = script(home.path());
    let mut h = app(home.path(), &p, None);
    // Mod+Comma opens it (editor.md §24).
    h.shortcut(',', false);
    assert_eq!(h.app.sheet, Some(Sheet::Settings));
    // Theme, the first row: Right steps it.
    assert_eq!(h.app.settings.theme, Choice::System);
    h.named(Named::ArrowRight, Modifiers::empty());
    assert_eq!(h.app.settings.theme, Choice::Light);
    h.named(Named::ArrowLeft, Modifiers::empty());
    assert_eq!(h.app.settings.theme, Choice::System);
    // Interface size, the second.
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::ArrowRight, Modifiers::empty());
    assert_eq!(h.app.settings.interface_size, 110);
    // Crash reports, the sixth: Space switches it.
    for _ in 0..4 {
        h.named(Named::ArrowDown, Modifiers::empty());
    }
    h.named(Named::Space, Modifiers::empty());
    assert_eq!(h.app.settings.crash_reports, CrashReports::Off);
    // The usage summary, the last: Down stops there.
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::Space, Modifiers::empty());
    assert!(!h.app.settings.count_usage);
    h.named(Named::Space, Modifiers::empty());
    assert!(h.app.settings.count_usage);
    // Tab to View…, Enter opens the summary.
    h.named(Named::Tab, Modifiers::empty());
    h.named(Named::Enter, Modifiers::empty());
    assert_eq!(h.app.sheet, Some(Sheet::Usage));
    h.named(Named::Escape, Modifiers::empty());
    assert_eq!(h.app.sheet, None);
    // Kept in settings.json.
    let saved =
        std::fs::read_to_string(Dirs::under(home.path()).config.join("settings.json")).unwrap();
    assert!(saved.contains("\"interface_size\": 110"), "{saved}");
    h.app.d.shutdown();
}
