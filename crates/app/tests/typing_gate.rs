//! The phase 3 typing gate: a short scene typed key by key into the real
//! app (view, page widget, update and its tasks, run headless), using smart
//! element flow, autocomplete, accented letters from the keyboard and from
//! an input method, an undo, and Mod+S with its save dialog. The saved file
//! must equal `tests/fixtures/app/gate-scene.fountain` byte for byte. Since
//! phase 4 the same keys are also typed into a script saved as .fdx first,
//! which must read back as the same scene (docs/specs/editor.md §14), and
//! since phase 5a into a script saved as a project.
//!
//! After every key (2026-10-21) the caret must show just after what it
//! typed (editor.md §4): one cell right of where the typed character
//! starts, or on a later line where the text wrapped, and in the same place
//! in all three formats. A space typed into a .fdx script or a project once
//! left the caret where it was until the next letter came.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use coldopen_app::editor::geometry::{Caret, Spot};
use coldopen_app::files::{Ask, Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot};
use coldopen_core::{Clock, FormatId};
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

/// Where the caret showed after each key, with the key.
type Trail = Vec<(String, Spot)>;

fn caret(h: &mut Headless) -> Spot {
    h.app
        .editor
        .caret_spot(&mut h.app.d)
        .expect("the caret shows")
}

/// One grid cell's width on the page.
fn pitch(h: &Headless) -> i32 {
    h.app.d.cached_view().unwrap().layout().format().pitch.0
}

/// Checks that the caret, just after the character `c` it typed, shows one
/// cell right of where that character starts, or on a later line where
/// the text wrapped. Comparing with the position one character back, as
/// the pages now stand, holds for every alignment: a right-aligned
/// transition grows to the left under a caret that stays put.
fn advanced(h: &mut Headless, c: &str) -> Spot {
    let after = caret(h);
    let at = h.app.editor.caret;
    assert!(at.offset > 0, "typing {c:?} left the caret at {at:?}");
    let back = Caret {
        offset: at.offset - 1,
        ..at
    };
    let before = h
        .app
        .editor
        .with_surface(&mut h.app.d, |doc, s| s.spot(doc, back))
        .flatten()
        .expect("the typed character shows");
    if (after.page, after.top) == (before.page, before.top) {
        assert_eq!(
            after.x.0 - before.x.0,
            pitch(h),
            "after typing {c:?} the caret shows at {after:?}, its character at {before:?}; blocks: {:?}",
            blocks(h)
        );
    } else {
        assert!(
            (after.page, after.top.0) > (before.page, before.top.0),
            "after typing {c:?} the caret shows at {after:?}, before its character at {before:?}"
        );
    }
    after
}

/// Types text a key at a time, as [`Headless::type_text`] does, checking
/// the caret after every key.
fn type_text(h: &mut Headless, trail: &mut Trail, text: &str) {
    for c in text.chars() {
        let s = c.to_string();
        h.type_text(&s);
        let after = advanced(h, &s);
        trail.push((s, after));
    }
}

/// Text committed by an input method, checked as a typed key is.
fn commit(h: &mut Headless, trail: &mut Trail, text: &str) {
    h.commit(text);
    let after = advanced(h, text);
    trail.push((format!("commit {text}"), after));
}

fn named(h: &mut Headless, trail: &mut Trail, key: Named) {
    h.named(key, Modifiers::empty());
    let after = caret(h);
    trail.push((format!("{key:?}"), after));
}

/// Types the gate scene from an empty scene heading at the caret. `now`
/// is the app's clock, moved for the pause before the undo. Returns where
/// the caret showed after each key.
fn type_scene(h: &mut Headless, now: &AtomicU64) -> Trail {
    let t = &mut Trail::new();
    // A new script starts on an empty scene heading. The prefix and
    // location are typed in lower case; headings print in capitals. The
    // é comes from an input method (as with a dead key or the macOS accent
    // menu), the ñ straight from the keyboard layout.
    assert_eq!(kind_at_caret(h), "SceneHeading");
    type_text(h, t, "int. caf");
    commit(h, t, "é");
    type_text(h, t, " niño - ni");
    // Autocomplete offers the time of day; Tab takes it.
    let c = h.app.editor.completion.clone().expect("time suggestions");
    assert_eq!(c.items, ["NIGHT"], "{:?}", blocks(h));
    named(h, t, Named::Tab);
    assert!(h.app.editor.completion.is_none());

    // Enter after a heading gives action.
    named(h, t, Named::Enter);
    assert_eq!(kind_at_caret(h), "Action");
    type_text(
        h,
        t,
        "Rain streaks the window. LENA (30s) counts coins on the counter.",
    );

    // Enter, then Tab on the empty action, gives a character; Enter after
    // a character gives dialogue.
    named(h, t, Named::Enter);
    named(h, t, Named::Tab);
    assert_eq!(kind_at_caret(h), "Character");
    type_text(h, t, "lena");
    named(h, t, Named::Enter);
    assert_eq!(kind_at_caret(h), "Dialogue");
    type_text(h, t, "Two coffees. Or one, if Bruno is late.");

    // Enter after dialogue gives the next character.
    named(h, t, Named::Enter);
    assert_eq!(kind_at_caret(h), "Character");
    type_text(h, t, "bruno");
    named(h, t, Named::Enter);
    // Tab on an empty dialogue makes a parenthetical, brackets included;
    // Enter in it goes on to dialogue.
    named(h, t, Named::Tab);
    assert_eq!(kind_at_caret(h), "Parenthetical");
    type_text(h, t, "out of breath");
    named(h, t, Named::Enter);
    assert_eq!(kind_at_caret(h), "Dialogue");
    type_text(h, t, "I'm here. I brought müsli.");

    // One letter, and autocomplete offers the other speaker.
    named(h, t, Named::Enter);
    type_text(h, t, "l");
    let c = h.app.editor.completion.clone().expect("name suggestions");
    assert_eq!(c.items, ["LENA"]);
    named(h, t, Named::Tab);
    named(h, t, Named::Enter);
    type_text(h, t, "You're late.");

    // A pause, a false start, and Mod+Z takes back only the false start:
    // typing after a pause is its own undo step.
    now.fetch_add(2_000, Ordering::SeqCst);
    type_text(h, t, " Again.");
    h.shortcut('z', false);
    assert!(
        blocks(h).last().unwrap().ends_with("You're late."),
        "{:?}",
        blocks(h)
    );
    let after = caret(h);
    t.push(("Mod+Z".into(), after));

    // Enter, then Tab on the empty character, gives a transition.
    named(h, t, Named::Enter);
    named(h, t, Named::Tab);
    assert_eq!(kind_at_caret(h), "Transition");
    type_text(h, t, "cut to:");
    std::mem::take(t)
}

/// A new app with its home in `home`, its save dialog answering `answer`,
/// and a clock the test moves, so the typing pause before the undo is
/// exact rather than a sleep. Returns the asks the dialog saw too.
fn app_at(
    home: &Path,
    answer: PathBuf,
) -> (Headless, Arc<AtomicU64>, Arc<std::sync::Mutex<Vec<Ask>>>) {
    let mut app = App::new(Boot {
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = asked.clone();
    app.set_dialogs(Dialogs::scripted(move |ask| {
        log.lock().unwrap().push(ask.clone());
        Some(answer.clone())
    }));
    let now = Arc::new(AtomicU64::new(1_000));
    let n = now.clone();
    app.d
        .set_clock(Clock::new(move || n.load(Ordering::SeqCst)));
    let h = Headless::new(app, iced::Size::new(1280.0, 860.0)).expect("a headless renderer");
    (h, now, asked)
}

/// The caret's trail in a new, untitled Fountain script: the place it must
/// show in the other formats too.
fn fountain_trail() -> &'static Trail {
    static TRAIL: OnceLock<Trail> = OnceLock::new();
    TRAIL.get_or_init(|| {
        let home = tempfile::tempdir().unwrap();
        let (mut h, now, _) = app_at(home.path(), home.path().join("unused"));
        let trail = type_scene(&mut h, &now);
        h.app.d.shutdown();
        trail
    })
}

/// Compares a trail with the Fountain script's, naming the first key where
/// the caret showed somewhere else.
fn same_trail(format: &str, got: &Trail) {
    let want = fountain_trail();
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!(
            g, w,
            "in the {format} script, key {k} ({:?}) left the caret somewhere else",
            g.0
        );
    }
    assert_eq!(got.len(), want.len());
}

#[test]
fn a_short_scene_typed_through_the_ui_saves_as_expected() {
    let home = tempfile::tempdir().unwrap();
    let out = home.path().join("Scripts").join("gate-scene");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    // The save dialog answers with a name the writer typed without an
    // extension; the app adds ".fountain".
    let (mut h, now, asked) = app_at(home.path(), out.clone());

    type_scene(&mut h, &now);

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

/// The same keys typed into a script saved as `name` before the first key,
/// so every key goes through that format's layout. Saved with Mod+S, it
/// reads back as the same scene, and the caret showed where it shows in a
/// Fountain script. `magic` is how the saved file starts.
fn typed_into(name: &str, format: FormatId, magic: &[u8]) {
    let home = tempfile::tempdir().unwrap();
    let out = home.path().join(name);
    let (mut h, now, _) = app_at(home.path(), out.clone());
    h.run(vec![coldopen_app::Message::Action("app.save_as".into())]);
    let session = h.app.d.session().unwrap();
    assert_eq!(session.format, format, "{:?}", h.app.banners);
    assert_eq!(session.path.as_deref(), Some(out.as_path()));

    let trail = type_scene(&mut h, &now);
    same_trail(name, &trail);
    h.shortcut('s', false);
    assert!(!h.app.d.session().unwrap().dirty, "{:?}", h.app.banners);
    assert!(h.app.banners.is_empty(), "{:?}", h.app.banners);
    let typed = blocks(&h);
    h.app.d.shutdown();
    assert!(std::fs::read(&out).unwrap().starts_with(magic));

    // Read back: the same blocks, and as Fountain the gate's own file.
    let mut d = coldopen_core::Dispatcher::new(coldopen_core::Options::default());
    let open = coldopen_core::Command::from_parts("doc.open", serde_json::json!({ "path": out }));
    d.execute(open.unwrap()).unwrap();
    let doc = &d.session().unwrap().doc;
    let back: Vec<String> = doc
        .blocks
        .iter()
        .map(|b| format!("{:?}: {}", b.element.kind, b.element.text.plain_text()))
        .collect();
    assert_eq!(back, typed);
    let want = std::fs::read_to_string(fixture())
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(coldopen_fountain::write(&doc.to_script()).text, want);
    d.shutdown();
}

#[test]
fn the_scene_typed_into_an_fdx_script_reads_back_the_same() {
    typed_into("gate-scene.fdx", FormatId::Fdx, b"<?xml");
}

#[test]
fn the_scene_typed_into_a_project_reads_back_the_same() {
    typed_into("gate-scene.coldopen", FormatId::Bundle, b"PK");
}
