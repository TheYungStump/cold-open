//! Regression tests for bugs found by hand in the Windows test build
//! (2026-10-15), each driven through the running app, headless.

use std::path::Path;

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::theme::Choice;
use coldopen_app::{App, Boot};
use coldopen_screenplay::ElementKind;
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;
use iced::{Event, Point, Size, mouse};

const W: f32 = 1000.0;
const H: f32 = 700.0;

/// A script of three pages: a heading and seventy one-line paragraphs.
fn three_pages(dir: &Path) -> std::path::PathBuf {
    let mut s = String::from("INT. LIGHTHOUSE - NIGHT\n\n");
    for i in 0..70 {
        s.push_str(&format!("The lamp turns, and the sea answers {i}.\n\n"));
    }
    let p = dir.join("three.fountain");
    std::fs::write(&p, s).unwrap();
    p
}

fn open(home: &Path, file: Option<std::path::PathBuf>) -> Headless {
    let mut app = App::new(Boot {
        file,
        dirs: Some(Dirs::under(home)),
        theme: Some(Choice::Light),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, Size::new(W, H)).expect("a headless renderer")
}

fn wheel(h: &mut Headless, dy: mouse::ScrollDelta) {
    let at = Point::new(W * 0.6, H * 0.5);
    h.cursor = mouse::Cursor::Available(at);
    h.events(vec![
        Event::Mouse(mouse::Event::CursorMoved { position: at }),
        Event::Mouse(mouse::Event::WheelScrolled { delta: dy }),
    ]);
    // A frame lets the scrollable report where it is now.
    h.frame(1.0);
}

fn none() -> Modifiers {
    Modifiers::empty()
}

// 1. Zooming kept jumping back to the top of the script. It must keep
// what is in view: the middle of the view when the caret is elsewhere, or
// the caret's line when the caret is in view.
#[test]
fn zoom_keeps_the_view_in_place() {
    let home = tempfile::tempdir().unwrap();
    let file = three_pages(home.path());
    let mut h = open(home.path(), Some(file));
    let pages = h.app.d.cached_view().unwrap().layout().pages.len();
    assert_eq!(pages, 3);
    for _ in 0..30 {
        wheel(&mut h, mouse::ScrollDelta::Lines { x: 0.0, y: -10.0 });
    }
    assert_eq!(h.app.visible_page(), Some(2), "scrolled to page 3 of 3");
    // The caret is still on page 1, out of view.
    assert_eq!(h.app.editor.caret_spot(&mut h.app.d).unwrap().page, 0);
    for (key, label) in [
        ('=', "zoom in"),
        ('=', "zoom in again"),
        ('-', "zoom out"),
        ('0', "actual size"),
    ] {
        h.shortcut(key, false);
        h.frame(1.0);
        assert_eq!(h.app.visible_page(), Some(2), "after {label}: still page 3");
    }

    // With the caret in view, its line stays where it was on screen.
    for _ in 0..6 {
        wheel(&mut h, mouse::ScrollDelta::Lines { x: 0.0, y: 10.0 });
    }
    h.click(Point::new(W * 0.5, H * 0.5));
    h.frame(1.0);
    let before = h.app.caret_screen_y().expect("the caret is in view");
    h.shortcut('=', false);
    h.frame(1.0);
    let after = h.app.caret_screen_y().expect("the caret is still in view");
    assert!(
        (after - before).abs() < 2.0,
        "caret moved from {before} to {after}"
    );
    h.app.d.discard();
}

// 2. Scrolling drew page text over the hint bar at the top, and over the
// status bar at the bottom. The bars must look the same at every scroll
// position.
#[test]
fn page_text_never_draws_over_the_bars() {
    let home = tempfile::tempdir().unwrap();
    let file = three_pages(home.path());
    let mut h = open(home.path(), Some(file));
    let bars = |px: &[u8], size: iced::Size<u32>| -> Vec<u8> {
        let row = size.width as usize * 4;
        let top = &px[..row * 26];
        let bottom = &px[px.len() - row * 18..];
        [top, bottom].concat()
    };
    let (size, px) = h.frame(1.0);
    let first = bars(&px, size);
    let mut differing = Vec::new();
    for step in 0..48 {
        wheel(&mut h, mouse::ScrollDelta::Pixels { x: 0.0, y: -5.0 });
        let (size, px) = h.frame(1.0);
        if bars(&px, size) != first {
            differing.push(step);
        }
    }
    assert!(
        differing.is_empty(),
        "the bars changed while scrolling, at steps {differing:?}"
    );
    h.app.d.discard();
}

// 3. The caret was drawn one line too low, at the left edge, in a new
// dialogue block (and the like), then jumped when the first letter came.
// It must stand where the text will appear, for every indented element.
#[test]
fn the_caret_stands_where_the_text_will_go() {
    let home = tempfile::tempdir().unwrap();
    let mut h = open(home.path(), None);
    let format = h.app.d.cached_view().unwrap().layout().format().clone();
    let pitch = format.pitch.0;
    let lh = format.line_height.0;
    let spot = |h: &mut Headless| h.app.editor.caret_spot(&mut h.app.d).unwrap();
    h.type_text("int. lab - night\nRain.\n");
    h.named(Named::Tab, none());
    h.type_text("maya");
    let maya = spot(&mut h);

    // Dialogue: right under the name, at the dialogue column.
    h.named(Named::Enter, none());
    let empty = spot(&mut h);
    assert_eq!(
        empty.top.0,
        maya.top.0 + lh,
        "dialogue sits right under the name"
    );
    assert_eq!(empty.x, format.elements.dialogue.left);
    h.type_text("H");
    let typed = spot(&mut h);
    assert_eq!((typed.top, typed.x.0), (empty.top, empty.x.0 + pitch));

    // Spaces typed first move the caret along.
    h.named(Named::Enter, none());
    h.type_text("theo\n  ");
    let spaces = spot(&mut h);
    assert_eq!(spaces.x.0, format.elements.dialogue.left.0 + 2 * pitch);
    h.named(Named::Backspace, none());
    h.named(Named::Backspace, none());

    // Parenthetical: Tab on the empty dialogue; the caret is between the
    // brackets, on the same line.
    h.named(Named::Tab, none());
    let paren = spot(&mut h);
    assert_eq!(paren.top, spaces.top);
    // The "(" hangs into the margin by its first-line indent.
    let p = &format.elements.parenthetical;
    assert_eq!(paren.x.0, p.left.0 + p.first_indent.0 + pitch);
    h.type_text("x");
    assert_eq!(spot(&mut h).top, paren.top);
    h.named(Named::Enter, none());
    h.type_text("Fine.");
    let fine = spot(&mut h);

    // Character: a blank line below the speech, at the character column.
    h.named(Named::Enter, none());
    let character = spot(&mut h);
    assert_eq!(character.top.0, fine.top.0 + 2 * lh);
    assert_eq!(character.x, format.elements.character.left);
    h.type_text("b");
    assert_eq!(spot(&mut h).top, character.top);
    h.named(Named::Backspace, none());

    // Transition: Tab on the empty character; it is right-aligned, so the
    // caret waits at the right edge.
    h.named(Named::Tab, none());
    let transition = spot(&mut h);
    assert_eq!(transition.top, character.top);
    assert_eq!(transition.x, format.elements.transition.right);
    h.type_text("c");
    let t = spot(&mut h);
    assert_eq!((t.top, t.x), (transition.top, transition.x));
    h.app.d.discard();
}

fn blocks(h: &Headless) -> Vec<(ElementKind, String)> {
    h.app
        .d
        .session()
        .unwrap()
        .doc
        .blocks
        .iter()
        .map(|b| (b.element.kind, b.element.text.plain_text()))
        .collect()
}

fn last(h: &Headless) -> (ElementKind, String) {
    blocks(h).last().unwrap().clone()
}

/// A script that ends on an empty dialogue under MAYA.
fn speech(home: &Path) -> Headless {
    let mut h = open(home, None);
    h.type_text("int. lab - night\nRain.\n");
    h.named(Named::Tab, none());
    h.type_text("maya\n");
    assert_eq!(last(&h), (ElementKind::Dialogue, String::new()));
    h
}

// 4. Tab from dialogue to parenthetical adds "()"; tabbing on without
// typing left the empty brackets behind. Brackets the editor added, with
// nothing inside, go when the writer leaves them; brackets the writer typed,
// or with text inside, stay; and every step undoes and redoes cleanly.
#[test]
fn empty_auto_brackets_never_stay_behind() {
    use ElementKind::*;
    let home = tempfile::tempdir().unwrap();

    // The Tab cycle, and undo and redo through it.
    let mut h = speech(home.path());
    let n = blocks(&h).len();
    h.named(Named::Tab, none());
    assert_eq!(last(&h), (Parenthetical, "()".into()));
    assert_eq!(h.app.editor.caret.offset, 1);
    h.named(Named::Tab, none());
    assert_eq!(last(&h), (Dialogue, String::new()));
    h.named(Named::Tab, none());
    h.named(Named::Tab, Modifiers::SHIFT);
    assert_eq!(
        last(&h),
        (Character, String::new()),
        "Shift+Tab leaves no brackets either"
    );
    assert_eq!(blocks(&h).len(), n);
    h.shortcut('z', false);
    assert_eq!(last(&h), (Parenthetical, "()".into()));
    assert_eq!(
        h.app.editor.caret.offset, 1,
        "undo puts the caret between the brackets"
    );
    h.shortcut('z', false);
    assert_eq!(last(&h), (Dialogue, String::new()));
    h.shortcut('z', false);
    assert_eq!(last(&h), (Parenthetical, "()".into()));
    h.shortcut('z', false);
    assert_eq!(last(&h), (Dialogue, String::new()));
    h.shortcut('z', true);
    h.shortcut('z', true);
    assert_eq!(last(&h), (Dialogue, String::new()));
    // After undo, the brackets are still the editor's: Tab removes them.
    h.shortcut('z', false);
    assert_eq!(last(&h), (Parenthetical, "()".into()));
    h.named(Named::Tab, none());
    assert_eq!(last(&h), (Dialogue, String::new()));
    h.app.d.discard();

    // Enter on brackets with nothing inside: like Enter on an empty block.
    let mut h = speech(home.path());
    h.named(Named::Tab, none());
    let n = blocks(&h).len();
    h.named(Named::Enter, none());
    assert_eq!(last(&h), (Dialogue, String::new()));
    assert_eq!(blocks(&h).len(), n, "no new block, no brackets left");
    h.app.d.discard();

    // Leaving with the arrow keys.
    let mut h = speech(home.path());
    h.named(Named::Tab, none());
    h.named(Named::ArrowUp, none());
    assert_eq!(last(&h), (Parenthetical, String::new()));
    h.app.d.discard();

    // Brackets with text inside stay.
    let mut h = speech(home.path());
    h.named(Named::Tab, none());
    h.type_text("beat");
    h.named(Named::Tab, none());
    assert_eq!(last(&h), (Dialogue, "(beat)".into()));
    h.app.d.discard();

    // Brackets the writer typed stay, even empty.
    let mut h = speech(home.path());
    h.type_text("(");
    assert_eq!(last(&h), (Parenthetical, "()".into()));
    h.named(Named::Tab, none());
    assert_eq!(last(&h), (Dialogue, "()".into()));
    h.app.d.discard();
}
