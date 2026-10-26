//! The title page sheet in the running app (docs/specs/editor.md §19):
//! keys and positioned lines edited from the keyboard, each field left
//! one undo step, and styles and .fdx attributes kept. Driven headless with
//! real key presses.

use std::path::Path;

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::{Headless, command_modifier};
use coldopen_app::theme::Choice;
use coldopen_app::{App, Boot};
use coldopen_screenplay::{Align, Run, Style};
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

fn app(home: &Path, name: &str, text: &str) -> Headless {
    let p = home.join(name);
    std::fs::write(&p, text).unwrap();
    let mut app = App::new(Boot {
        file: Some(p),
        dirs: Some(Dirs::under(home)),
        theme: Some(Choice::Light),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, iced::Size::new(1200.0, 800.0)).expect("a headless renderer")
}

fn open_sheet(h: &mut Headless) {
    h.key(
        Key::Character("t".into()),
        command_modifier() | Modifiers::SHIFT,
        Some("T"),
    );
    assert!(h.app.title_sheet.is_some(), "the sheet opens");
}

/// Selects everything in the focused field and types over it.
fn retype(h: &mut Headless, text: &str) {
    h.shortcut('a', false);
    h.type_text(text);
}

fn tab(h: &mut Headless, shift: bool) {
    let m = if shift {
        Modifiers::SHIFT
    } else {
        Modifiers::empty()
    };
    h.named(Named::Tab, m);
}

fn doc(h: &Headless) -> &coldopen_core::Document {
    &h.app.d.session().unwrap().doc
}

const BOLD: Style = Style {
    bold: true,
    italic: false,
    underline: false,
};

#[test]
fn keys_are_edited_one_field_at_a_time_and_keep_their_style() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(
        home.path(),
        "low.fountain",
        "Title: **Low Tide**\nAuthor: Dana Reyes\n\nINT. LAB - NIGHT\n\nRain.\n",
    );
    open_sheet(&mut h);
    let keys: Vec<String> = h
        .app
        .title_sheet
        .as_ref()
        .unwrap()
        .key_fields()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        keys,
        [
            "Title",
            "Credit",
            "Author",
            "Source",
            "Draft date",
            "Contact",
            "Notes",
            "Copyright"
        ],
        "the usual keys, empty ones too"
    );
    // The Title field has the keyboard: typing over it and leaving it is
    // one step, and the bold stays.
    retype(&mut h, "High Tide");
    assert_eq!(
        doc(&h).title_page.get("Title").unwrap().values[0].plain_text(),
        "Low Tide",
        "nothing is sent while the field has the keyboard"
    );
    tab(&mut h, false);
    let title = &doc(&h).title_page.get("Title").unwrap().values[0];
    assert_eq!(title.runs(), &[Run::styled("High Tide", BOLD)]);
    // Two values for Author, one per line.
    tab(&mut h, false);
    retype(&mut h, "Dana Reyes\nSam Ortiz");
    h.named(Named::Escape, Modifiers::empty());
    assert!(h.app.title_sheet.is_none(), "Esc closes it");
    let authors: Vec<String> = doc(&h)
        .title_page
        .get("Author")
        .unwrap()
        .values
        .iter()
        .map(|v| v.plain_text())
        .collect();
    assert_eq!(authors, ["Dana Reyes", "Sam Ortiz"]);
    let history = h
        .app
        .d
        .execute(coldopen_core::Command::from_parts("history.list", serde_json::json!({})).unwrap())
        .unwrap();
    assert_eq!(history["undo"][0]["label"], "Title page");
    assert_eq!(history["undo"][1]["label"], "Title page");
    // Each field is one undo step.
    h.shortcut('z', false);
    assert_eq!(doc(&h).title_page.get("Author").unwrap().values.len(), 1);
    h.shortcut('z', false);
    assert_eq!(
        doc(&h).title_page.get("Title").unwrap().values[0].plain_text(),
        "Low Tide"
    );
}

#[test]
fn a_new_key_goes_after_the_keys_before_it() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(
        home.path(),
        "low.fountain",
        "Title: Low Tide\nAuthor: Dana Reyes\n\nINT. LAB - NIGHT\n\nRain.\n",
    );
    open_sheet(&mut h);
    // Shift+Tab from the first field wraps to "Add a key…".
    tab(&mut h, true);
    h.type_text("Rewrite by");
    h.named(Named::Enter, Modifiers::empty());
    h.type_text("Ana Ruiz");
    h.named(Named::Escape, Modifiers::empty());
    // An empty usual key, filled in, goes after the keys before it.
    open_sheet(&mut h);
    for _ in 0..4 {
        tab(&mut h, false);
    }
    h.type_text("October 25, 2026");
    h.named(Named::Escape, Modifiers::empty());
    let keys: Vec<&str> = doc(&h)
        .title_page
        .fields
        .iter()
        .map(|f| f.key.as_str())
        .collect();
    assert_eq!(keys, ["Title", "Author", "Draft date", "Rewrite by"]);
    // A field emptied removes its key.
    open_sheet(&mut h);
    tab(&mut h, false);
    tab(&mut h, false);
    retype(&mut h, "");
    h.named(Named::Backspace, Modifiers::empty());
    h.named(Named::Escape, Modifiers::empty());
    assert!(doc(&h).title_page.get("Author").is_none());
}

#[test]
fn fdx_lines_are_edited_added_removed_and_saved_with_what_the_file_held() {
    let home = tempfile::tempdir().unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx/title-page.fdx");
    let text = std::fs::read_to_string(fixture).unwrap();
    let mut h = app(home.path(), "t.fdx", &text);
    open_sheet(&mut h);
    let rows = h.app.title_sheet.as_ref().unwrap().line_rows();
    assert_eq!(rows.len(), 7);
    assert_eq!(rows[1].0, "THE LAST LIGHTHOUSE");
    assert_eq!(
        rows[2],
        ("Written by".into(), Align::Center, "2".into()),
        "24 pt is two lines"
    );
    // The second line's text: Tab steps text, space, text, and selects
    // what it steps to.
    tab(&mut h, false);
    tab(&mut h, false);
    h.type_text("THE LAST LAMP");
    tab(&mut h, false);
    let line = &doc(&h).title_lines[1];
    assert_eq!(line.text.plain_text(), "THE LAST LAMP");
    assert!(
        line.text
            .runs()
            .iter()
            .all(|r| matches!(r, Run::Text { style, .. } if style.bold && style.underline))
    );
    // Mod+R aligns it right.
    h.shortcut('r', false);
    assert_eq!(doc(&h).title_lines[1].align, Align::Right);
    // Enter in its text adds a line after it, in its column.
    tab(&mut h, true);
    h.named(Named::Enter, Modifiers::empty());
    h.type_text("A new line");
    tab(&mut h, false);
    assert_eq!(doc(&h).title_lines.len(), 8);
    assert_eq!(doc(&h).title_lines[2].text.plain_text(), "A new line");
    assert_eq!(doc(&h).title_lines[2].align, Align::Right);
    // Mod+Shift+K removes it again, and Mod+Z brings it back.
    tab(&mut h, true);
    h.shortcut('k', true);
    assert_eq!(doc(&h).title_lines.len(), 7);
    h.shortcut('z', false);
    assert_eq!(doc(&h).title_lines.len(), 8);
    assert_eq!(
        h.app.title_sheet.as_ref().unwrap().line_rows().len(),
        8,
        "the sheet shows the undo"
    );
    h.named(Named::Escape, Modifiers::empty());
    // Saved, the lines nobody edited are as the file had them.
    h.shortcut('s', false);
    let saved = std::fs::read_to_string(home.path().join("t.fdx")).unwrap();
    assert!(saved.contains("THE LAST LAMP"), "{saved}");
    assert!(saved.contains("A new line"));
    assert!(saved.contains(r#"<Tabstop Position="6.00" Type="Left"/>"#));
    assert!(saved.contains(r#"<Text>Ada </Text><Text Style="Italic">Quill</Text>"#));
}

fn fdx_app(home: &Path) -> Headless {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx/title-page.fdx");
    app(home, "t.fdx", &std::fs::read_to_string(fixture).unwrap())
}

/// The manual test of 2026-10-26: the title page beside the sheet scrolls,
/// and a click on it neither closes the sheet nor moves the script's caret.
#[test]
fn the_title_page_beside_the_sheet_scrolls_and_ignores_clicks() {
    use iced::{Point, mouse};
    let home = tempfile::tempdir().unwrap();
    let mut h = fdx_app(home.path());
    open_sheet(&mut h);
    let caret = h.app.editor.caret;
    let (_, before) = h.frame(1.0);
    let at = Point::new(300.0, 500.0);
    h.cursor = mouse::Cursor::Available(at);
    h.events(vec![
        iced::Event::Mouse(mouse::Event::CursorMoved { position: at }),
        iced::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: -6.0 },
        }),
    ]);
    let (_, after) = h.frame(1.0);
    assert!(before != after, "the page scrolled");
    h.click(at);
    assert!(
        h.app.title_sheet.is_some(),
        "a click on the page leaves the sheet open"
    );
    assert_eq!(h.app.editor.caret, caret);
}

/// Tab sends the line and the page shows it at once; clicking into a line
/// makes it the current one, so its alignment is the one highlighted.
#[test]
fn tab_shows_the_line_and_a_click_chooses_the_row() {
    use coldopen_app::app::iced_settings;
    let home = tempfile::tempdir().unwrap();
    let mut h = fdx_app(home.path());
    open_sheet(&mut h);
    tab(&mut h, false);
    tab(&mut h, false);
    h.type_text("THE LAST LAMP");
    tab(&mut h, false);
    let shown = format!("{:?}", h.app.d.cached_view().unwrap().layout().title);
    assert!(
        shown.contains("THE LAST LAMP"),
        "the laid-out title page has the line"
    );
    // A click into the "Draft of 13 October" line's field.
    let field = {
        let mut sim = iced_test::Simulator::with_size(
            iced_settings(),
            iced::Size::new(1200.0, 800.0),
            h.app.view(),
        );
        sim.find("Draft of 13 October").unwrap().bounds()
    };
    h.click(field.center());
    let rows = h.app.title_sheet.as_ref().unwrap().line_rows();
    let draft = rows
        .iter()
        .position(|r| r.0 == "Draft of 13 October")
        .unwrap();
    assert_eq!(h.app.title_sheet.as_ref().unwrap().current_line(), draft);
}
