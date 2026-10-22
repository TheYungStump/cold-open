//! The editor's features through the running app (docs/specs/editor.md
//! §3, §7–§10): every action, the palette, find and replace, zoom, the
//! draft view, the navigator, the history sheet, and the writer's keymap
//! and flow files. Driven headless with real key presses.

use std::path::Path;

use coldopen_app::actions::APP_ACTIONS;
use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot, Message};
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

const SCRIPT: &str = "INT. LAB - NIGHT\n\nRain on the roof.\n\nMAYA\nWe should go.\n\nEXT. ROOF - NIGHT\n\nRain again.\n";

fn app(home: &Path) -> Headless {
    let p = home.join("lab.fountain");
    if !p.exists() {
        std::fs::write(&p, SCRIPT).unwrap();
    }
    let mut app = App::new(Boot {
        file: Some(p),
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, iced::Size::new(1200.0, 800.0)).expect("a headless renderer")
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

fn esc(h: &mut Headless) {
    h.named(Named::Escape, Modifiers::empty());
}

#[test]
fn every_app_action_does_something() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    for a in APP_ACTIONS {
        if a.name == "app.quit" {
            continue;
        }
        h.app.banners.clear();
        h.run(vec![Message::Action(a.name.to_string())]);
        assert!(
            !h.app.banners.iter().any(|b| b.contains("not available")),
            "{}: {:?}",
            a.name,
            h.app.banners
        );
        // Whatever the action opened, Esc closes it.
        esc(&mut h);
        esc(&mut h);
        assert!(
            h.app.sheet.is_none() && h.app.export.is_none(),
            "{}",
            a.name
        );
    }
}

#[test]
fn the_palette_finds_and_runs_actions_from_the_keyboard() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    let zoom = h.app.settings.zoom;
    h.shortcut('k', false);
    // The palette's field has the keyboard: typing filters the list.
    h.type_text("zoom in");
    h.named(Named::Enter, Modifiers::empty());
    assert!(h.app.settings.zoom > zoom);
    // Typing goes back to the page afterwards.
    h.type_text("x");
    assert_eq!(texts(&h)[0], "XINT. LAB - NIGHT");
    // Arrow keys choose; Esc closes without running anything.
    h.shortcut('k', false);
    h.type_text("theme");
    h.named(Named::ArrowDown, Modifiers::empty());
    esc(&mut h);
    h.type_text("y");
    assert_eq!(texts(&h)[0], "XYINT. LAB - NIGHT");
    h.app.d.discard();
}

#[test]
fn find_and_replace() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    h.shortcut('f', false);
    h.type_text("rain");
    // Enter finds the next match and selects it.
    h.named(Named::Enter, Modifiers::empty());
    assert!(h.app.editor.has_selection());
    // Replace all, as one undo step.
    h.run(vec![
        Message::Action("edit.replace".into()),
        Message::ReplaceInput("Snow".into()),
        Message::ReplaceAll,
    ]);
    assert_eq!(texts(&h)[1], "Snow on the roof.");
    assert_eq!(texts(&h)[5], "Snow again.");
    esc(&mut h);
    h.shortcut('z', false);
    assert_eq!(texts(&h)[1], "Rain on the roof.");
    assert_eq!(texts(&h)[5], "Rain again.");
    h.app.d.discard();
}

#[test]
fn zoom_and_draft_view_keep_typing_working() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    h.shortcut('=', false);
    h.shortcut('=', false);
    assert_eq!(h.app.settings.zoom, 1.25);
    h.shortcut('0', false);
    assert_eq!(h.app.settings.zoom, 1.0);
    // The draft view: same editing, no pages.
    h.shortcut('d', true);
    assert!(h.app.settings.draft && h.app.editor.draft);
    h.named(Named::End, Modifiers::empty());
    h.named(Named::Enter, Modifiers::empty());
    h.type_text("Lightning.");
    assert_eq!(texts(&h)[1], "Lightning.");
    let (size, px) = h.frame(1.0);
    assert_eq!(px.len() as u32, size.width * size.height * 4);
    h.shortcut('d', true);
    assert!(!h.app.settings.draft);
    h.app.d.discard();
}

#[test]
fn scenes_move_from_the_navigator_and_undo_puts_them_back() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    // Alt+Page Down goes to the next scene.
    h.named(Named::PageDown, Modifiers::ALT);
    assert_eq!(texts(&h)[4], "EXT. ROOF - NIGHT");
    let roof = h.app.d.session().unwrap().doc.blocks[4].id;
    assert_eq!(h.app.editor.caret.block, roof);
    // F6 moves focus to the navigator, where Alt+Up moves the scene up.
    h.named(Named::F6, Modifiers::empty());
    h.named(Named::ArrowUp, Modifiers::ALT);
    assert_eq!(texts(&h)[0], "EXT. ROOF - NIGHT");
    assert_eq!(texts(&h)[2], "INT. LAB - NIGHT");
    h.shortcut('z', false);
    assert_eq!(texts(&h)[0], "INT. LAB - NIGHT");
    h.app.d.discard();
}

#[test]
fn the_history_sheet_steps_through_undo() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    h.named(Named::End, Modifiers::empty());
    h.type_text(" (2)");
    h.shortcut('h', true);
    assert!(h.app.sheet.is_some());
    // Typing doesn't reach the page while the sheet is open...
    h.type_text("q");
    assert_eq!(texts(&h)[0], "INT. LAB - NIGHT (2)");
    // ...but undo and redo do, so the writer can watch the history.
    h.shortcut('z', false);
    assert_eq!(texts(&h)[0], "INT. LAB - NIGHT");
    h.shortcut('z', true);
    assert_eq!(texts(&h)[0], "INT. LAB - NIGHT (2)");
    esc(&mut h);
    assert!(h.app.sheet.is_none());
    h.app.d.discard();
}

#[test]
fn the_writer_s_keymap_and_flow_files_apply_and_bad_entries_are_reported() {
    let home = tempfile::tempdir().unwrap();
    let config = Dirs::under(home.path()).config;
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(
        config.join("keymap.json"),
        r#"{"v": 1, "bindings": [
            {"keys": "Mod+J", "action": "view.zoom_in"},
            {"keys": "Mod+=", "action": null},
            {"keys": "Mod+J+J", "action": "view.zoom_in"}
        ]}"#,
    )
    .unwrap();
    std::fs::write(
        config.join("flow.json"),
        r#"{"v": 1, "kinds": {"action": {"tab": "transition"}, "nope": {"tab": "action"}}}"#,
    )
    .unwrap();
    let mut h = app(home.path());
    assert_eq!(h.app.banners.len(), 2, "{:?}", h.app.banners);
    h.shortcut('=', false);
    assert_eq!(h.app.settings.zoom, 1.0, "Mod+= was unbound");
    h.shortcut('j', false);
    assert_eq!(h.app.settings.zoom, 1.1);
    // Tab on an empty action now gives a transition.
    h.named(Named::ArrowDown, Modifiers::empty());
    h.named(Named::End, Modifiers::empty());
    h.named(Named::Enter, Modifiers::empty());
    h.named(Named::Tab, Modifiers::empty());
    let s = h.app.d.session().unwrap();
    let kind = s.doc.get(h.app.editor.caret.block).unwrap().element.kind;
    assert_eq!(format!("{kind:?}"), "Transition");
    h.app.d.discard();
}

#[test]
fn esc_dismisses_messages_once_nothing_else_is_open() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    h.app.banners.push("A message.".into());
    h.app.leftover.push(home.path().join("lost.fountain"));
    // A selection goes first...
    h.named(Named::End, Modifiers::SHIFT);
    assert!(h.app.editor.has_selection());
    esc(&mut h);
    assert!(!h.app.editor.has_selection());
    assert_eq!(h.app.banners.len(), 1);
    // ...then the banner, then the leftover-work offer.
    esc(&mut h);
    assert!(h.app.banners.is_empty());
    assert_eq!(h.app.leftover.len(), 1);
    esc(&mut h);
    assert!(h.app.leftover.is_empty());
    h.app.d.discard();
}

/// A recent script deep in long folders shows on one line in the palette:
/// its name first, its folder shortened in the middle (editor.md §9,
/// 2026-10-21). A long folder once took the whole row, and the title,
/// squeezed to nothing, wrapped a letter a line into a tall empty row.
#[test]
fn palette_rows_with_long_paths_stay_one_line() {
    use coldopen_app::app::iced_settings;
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    let deep = home
        .path()
        .join("Writing")
        .join("Feature drafts for the spring fellowship")
        .join("Second act rewrites after the table read");
    h.app.settings.recent = vec![deep.join("low-tide.fdx")];
    h.run(vec![
        Message::Action("app.palette".into()),
        Message::PaletteInput("open".into()),
    ]);
    let mut sim = iced_test::Simulator::with_size(
        iced_settings(),
        iced::Size::new(1200.0, 800.0),
        h.app.view(),
    );
    let title = sim.find("Open recent: low-tide.fdx").unwrap().bounds();
    let folder = sim.find(r"C:\…\Second act re…he table read");
    let row = sim.find("Open a script").unwrap().bounds();
    assert!(
        title.height <= row.height + 1.0 && title.width > 100.0,
        "the title is {title:?}, a one-line title is {row:?}"
    );
    // The folder, shortened: on Windows its drive, else the root.
    let shown = if cfg!(windows) {
        folder.is_ok()
    } else {
        sim.find("/…/Second act rew…the table read").is_ok()
    };
    assert!(shown, "the folder is not shortened as expected");
    drop(sim);
    h.app.d.discard();
}

/// Notices (editor.md §13, 2026-10-21): the bar shows the newest with
/// "+N more", which opens the full list; Dismiss closes one and shows the
/// next; Dismiss all closes them all; Esc closes the list, then the rest.
#[test]
fn notices_dismiss_one_at_a_time_and_open_as_a_list() {
    use coldopen_app::app::iced_settings;
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path());
    let set = |h: &mut Headless, texts: &[&str]| {
        h.app.banners = texts.iter().map(|t| t.to_string().into()).collect();
    };
    let shown =
        |h: &Headless| -> Vec<String> { h.app.banners.iter().map(|b| b.text.clone()).collect() };
    // Clicks the button labelled `label` in the drawn window, as the
    // mouse would, and runs what it sends.
    let click = |h: &mut Headless, label: &str| {
        let mut sim = iced_test::Simulator::with_size(
            iced_settings(),
            iced::Size::new(1200.0, 800.0),
            h.app.view(),
        );
        sim.click(label)
            .unwrap_or_else(|e| panic!("no {label:?} button: {e:?}"));
        let sent: Vec<Message> = sim.into_messages().collect();
        h.run(sent);
    };
    let on_screen = |h: &Headless, label: &str| {
        iced_test::Simulator::with_size(
            iced_settings(),
            iced::Size::new(1200.0, 800.0),
            h.app.view(),
        )
        .find(label)
        .is_ok()
    };

    set(&mut h, &["First.", "Second.", "Third."]);
    assert!(on_screen(&h, "Third.") && on_screen(&h, "+2 more"));
    assert!(!on_screen(&h, "Second."));
    // Dismiss closes the one shown, and the next takes its place.
    click(&mut h, "Dismiss");
    assert_eq!(shown(&h), ["First.", "Second."]);
    assert!(on_screen(&h, "Second.") && on_screen(&h, "+1 more"));
    // "+1 more" opens the list, newest first, each with its own Dismiss.
    click(&mut h, "+1 more");
    assert!(h.app.banners_open);
    assert!(on_screen(&h, "2 notices") && on_screen(&h, "First.") && on_screen(&h, "Second."));
    click(&mut h, "Dismiss");
    assert_eq!(shown(&h), ["First."], "the newest row's Dismiss");
    assert!(!h.app.banners_open, "one notice needs no list");
    assert!(!on_screen(&h, "+0 more") && !on_screen(&h, "Dismiss all"));
    // Dismiss all.
    set(&mut h, &["A.", "B.", "C."]);
    click(&mut h, "Dismiss all");
    assert!(h.app.banners.is_empty());
    // Esc closes the list first, then dismisses the rest.
    set(&mut h, &["A.", "B."]);
    click(&mut h, "+1 more");
    h.named(Named::Escape, Modifiers::empty());
    assert!(!h.app.banners_open);
    assert_eq!(shown(&h), ["A.", "B."]);
    h.named(Named::Escape, Modifiers::empty());
    assert!(h.app.banners.is_empty());
    h.app.d.discard();
}

/// With both side panels open in a window 1280 pixels wide, a page at 100%
/// is wider than the page area, which never scrolls sideways: the page is
/// drawn at the largest zoom that fits, and the status bar says so
/// (editor.md §3, 2026-10-21). Once the window is wide enough, the
/// writer's zoom comes back.
#[test]
fn the_page_fits_between_both_panels_at_1280() {
    use coldopen_app::page_view::MARGIN;
    let home = tempfile::tempdir().unwrap();
    let p = home.path().join("lab.fountain");
    std::fs::write(&p, SCRIPT.repeat(40)).unwrap();
    let mut app = App::new(Boot {
        file: Some(p),
        dirs: Some(Dirs::under(home.path())),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    let mut h = Headless::new(app, iced::Size::new(1280.0, 860.0)).expect("a headless renderer");
    assert!(h.app.settings.navigator, "the navigator shows by default");
    h.run(vec![Message::Action("view.changes".into())]);
    assert_eq!(h.app.settings.zoom, 1.0);
    // The page area reports its size when it is redrawn, as the window
    // does every frame.
    let redraw = |h: &mut Headless| {
        h.events(vec![iced::Event::Window(
            iced::window::Event::RedrawRequested(std::time::Instant::now()),
        )]);
    };
    redraw(&mut h);
    let width = h
        .app
        .page_area_width()
        .expect("the page area reported its size");
    let page = |h: &Headless| {
        let f = h.app.d.cached_view().unwrap().layout().format();
        f.page_width.0 as f32 * h.app.shown_zoom() * 96.0 / 72.0 / 100.0
    };
    assert!(
        width < 816.0 + 2.0 * MARGIN,
        "a page at 100% fits {width} pixels; the test needs a narrower area"
    );
    assert!(h.app.shown_zoom() < 1.0);
    assert!(
        page(&h) + 2.0 * MARGIN <= width + 0.5,
        "the page ({}) is wider than the page area ({width})",
        page(&h)
    );
    // Zoom out works from what is shown; zoom in stops at the fit.
    let fitted = h.app.shown_zoom();
    h.run(vec![Message::Action("view.zoom_out".into())]);
    assert!(h.app.shown_zoom() < fitted);
    h.run(vec![Message::Action("view.zoom_reset".into())]);
    assert_eq!(h.app.shown_zoom(), fitted);
    // Without the changes panel the page fits at 100% again.
    h.run(vec![Message::Action("view.changes".into())]);
    redraw(&mut h);
    assert!(h.app.page_area_width().unwrap() > width);
    assert_eq!(h.app.shown_zoom(), 1.0);
    h.app.d.discard();
}
