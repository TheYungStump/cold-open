use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot};

#[test]
fn types_through_the_page() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(Boot {
        dirs: Some(coldopen_app::files::Dirs::under(dir.path())),
        ..Boot::default()
    });
    let mut h = Headless::new(app, iced::Size::new(1280.0, 860.0)).unwrap();
    h.type_text("int. lab - night\nRain.");
    let doc = &h.app.d.session().unwrap().doc;
    let t: Vec<String> = doc
        .blocks
        .iter()
        .map(|b| b.element.text.plain_text())
        .collect();
    assert_eq!(t, ["INT. LAB - NIGHT", "Rain."]);
    let (size, px) = h.frame(1.0);
    assert_eq!(px.len(), (size.width * size.height * 4) as usize);
    h.app.d.discard();
}

#[test]
fn shift_click_extends_the_selection() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::new(Boot {
        dirs: Some(coldopen_app::files::Dirs::under(dir.path())),
        ..Boot::default()
    });
    let mut h = Headless::new(app, iced::Size::new(1280.0, 860.0)).unwrap();
    h.type_text("int. lab - night\nRain hammers the roof.\nWind.");
    // Somewhere on the heading's line, then Shift and a click lower down.
    let first = h.app.editor.caret;
    h.click(iced::Point::new(530.0, 170.0));
    let at_heading = h.app.editor.caret;
    assert_ne!(at_heading.block, first.block, "the click moved the caret");
    assert!(!h.app.editor.has_selection());
    h.events(vec![iced::Event::Keyboard(
        iced::keyboard::Event::ModifiersChanged(iced::keyboard::Modifiers::SHIFT),
    )]);
    h.click(iced::Point::new(560.0, 240.0));
    assert!(h.app.editor.has_selection());
    assert_eq!(h.app.editor.anchor, at_heading);
    h.app.d.discard();
}
