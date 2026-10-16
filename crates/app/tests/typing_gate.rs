//! The phase 3 typing gate: a short scene typed key by key into the real
//! app (view, page widget, update and its tasks, run headless), using smart
//! element flow, autocomplete, accented letters from the keyboard and from
//! an input method, an undo, and Mod+S with its save dialog. The saved file
//! must equal `tests/fixtures/app/gate-scene.fountain` byte for byte.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use coldopen_app::files::{Ask, Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot};
use coldopen_core::Clock;
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/app/gate-scene.fountain")
}

/// The block types and texts, for messages when something goes wrong.
fn blocks(h: &Headless) -> Vec<String> {
    h.app
        .d
        .session()
        .map(|s| {
            s.doc
                .blocks
                .iter()
                .map(|b| format!("{:?}: {}", b.element.kind, b.element.text.plain_text()))
                .collect()
        })
        .unwrap_or_default()
}

fn kind_at_caret(h: &Headless) -> String {
    let s = h.app.d.session().unwrap();
    format!(
        "{:?}",
        s.doc.get(h.app.editor.caret.block).unwrap().element.kind
    )
}

#[test]
fn a_short_scene_typed_through_the_ui_saves_as_expected() {
    let home = tempfile::tempdir().unwrap();
    let out = home.path().join("Scripts").join("gate-scene");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let mut app = App::new(Boot {
        dirs: Some(Dirs::under(home.path())),
        ..Boot::default()
    });
    // The save dialog answers with a name the writer typed without an
    // extension; the app adds ".fountain".
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = asked.clone();
    let answer = out.clone();
    app.set_dialogs(Dialogs::scripted(move |ask| {
        log.lock().unwrap().push(ask.clone());
        Some(answer.clone())
    }));
    // A clock the test moves, so the typing pause before the undo is
    // exact rather than a sleep.
    let now = Arc::new(AtomicU64::new(1_000));
    let n = now.clone();
    app.d
        .set_clock(Clock::new(move || n.load(Ordering::SeqCst)));
    let mut h = Headless::new(app, iced::Size::new(1280.0, 860.0)).expect("a headless renderer");
    let none = Modifiers::empty();

    // A new script starts on an empty scene heading. The prefix and
    // location are typed in lower case; headings print in capitals. The
    // é comes from an input method (as with a dead key or the macOS accent
    // menu), the ñ straight from the keyboard layout.
    assert_eq!(kind_at_caret(&h), "SceneHeading");
    h.type_text("int. caf");
    h.commit("é");
    h.type_text(" niño - ni");
    // Autocomplete offers the time of day; Tab takes it.
    let c = h.app.editor.completion.clone().expect("time suggestions");
    assert_eq!(c.items, ["NIGHT"], "{:?}", blocks(&h));
    h.named(Named::Tab, none);
    assert!(h.app.editor.completion.is_none());

    // Enter after a heading gives action.
    h.named(Named::Enter, none);
    assert_eq!(kind_at_caret(&h), "Action");
    h.type_text("Rain streaks the window. LENA (30s) counts coins on the counter.");

    // Enter, then Tab on the empty action, gives a character; Enter after
    // a character gives dialogue.
    h.named(Named::Enter, none);
    h.named(Named::Tab, none);
    assert_eq!(kind_at_caret(&h), "Character");
    h.type_text("lena");
    h.named(Named::Enter, none);
    assert_eq!(kind_at_caret(&h), "Dialogue");
    h.type_text("Two coffees. Or one, if Bruno is late.");

    // Enter after dialogue gives the next character.
    h.named(Named::Enter, none);
    assert_eq!(kind_at_caret(&h), "Character");
    h.type_text("bruno");
    h.named(Named::Enter, none);
    // Tab on an empty dialogue makes a parenthetical, brackets included;
    // Enter in it goes on to dialogue.
    h.named(Named::Tab, none);
    assert_eq!(kind_at_caret(&h), "Parenthetical");
    h.type_text("out of breath");
    h.named(Named::Enter, none);
    assert_eq!(kind_at_caret(&h), "Dialogue");
    h.type_text("I'm here. I brought müsli.");

    // One letter, and autocomplete offers the other speaker.
    h.named(Named::Enter, none);
    h.type_text("l");
    let c = h.app.editor.completion.clone().expect("name suggestions");
    assert_eq!(c.items, ["LENA"]);
    h.named(Named::Tab, none);
    h.named(Named::Enter, none);
    h.type_text("You're late.");

    // A pause, a false start, and Mod+Z takes back only the false start:
    // typing after a pause is its own undo step.
    now.fetch_add(2_000, Ordering::SeqCst);
    h.type_text(" Again.");
    h.shortcut('z', false);
    assert!(
        blocks(&h).last().unwrap().ends_with("You're late."),
        "{:?}",
        blocks(&h)
    );

    // Enter, then Tab on the empty character, gives a transition.
    h.named(Named::Enter, none);
    h.named(Named::Tab, none);
    assert_eq!(kind_at_caret(&h), "Transition");
    h.type_text("cut to:");

    // Mod+S on a script never saved asks where to save it.
    assert!(h.app.d.session().unwrap().dirty);
    h.shortcut('s', false);
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        [Ask::SaveAs("Untitled.fountain".into())]
    );
    let saved = out.with_extension("fountain");
    let session = h.app.d.session().unwrap();
    assert!(!session.dirty, "{:?}", h.app.banners);
    assert_eq!(session.path.as_deref(), Some(saved.as_path()));
    assert!(h.app.banners.is_empty(), "{:?}", h.app.banners);

    let got = std::fs::read_to_string(&saved).unwrap();
    let want = std::fs::read_to_string(fixture())
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(got, want, "blocks: {:#?}", blocks(&h));

    // The untitled script's journal went with it: nothing is left in
    // Unsaved/ to offer as lost work next time.
    let dirs = Dirs::under(home.path());
    assert!(coldopen_app::files::leftover_work(&dirs, &[]).is_empty());
    assert_eq!(h.app.settings.recent.first(), Some(&saved));

    h.app.d.shutdown();
}
