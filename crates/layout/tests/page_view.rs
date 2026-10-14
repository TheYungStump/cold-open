//! The plain-text page view (layout.md §11).

use coldopen_layout::{AllGlyphs, Engine, Options, ValidFormat};
use coldopen_screenplay::{Element, ElementKind, Script};

#[test]
fn an_empty_line_at_the_foot_of_a_page_leaves_no_row() {
    let script = Script {
        elements: vec![
            Element::new(ElementKind::Action, "First."),
            Element::new(ElementKind::Action, ""),
            Element::new(ElementKind::PageBreak, ""),
            Element::new(ElementKind::Action, "Next."),
        ],
        ..Default::default()
    };
    let layout = Engine::new(&script, &ValidFormat::default(), &Options::default()).run();
    let view = layout.page_view(&AllGlyphs);
    let page1: Vec<&str> = view
        .split("=== page 2")
        .next()
        .unwrap_or_default()
        .lines()
        .collect();
    assert_eq!(page1.last().map(|l| l.trim()), Some("First."), "{view}");
    assert!(view.lines().all(|l| !l.ends_with(' ')), "{view}");
}
