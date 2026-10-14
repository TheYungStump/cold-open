//! Inputs that come from files and are not checked with the format: space
//! overrides, title-page spacing, very large groups and very long elements.
//! Layout must place every line on the page and finish (layout.md §1).

use std::collections::BTreeMap;

use coldopen_layout::{AllGlyphs, Engine, Layout, LineKind, Options, ValidFormat};
use coldopen_screenplay::{Align, Element, ElementKind, Pt, RichText, Script, TitleLine};

fn script(elements: Vec<Element>) -> Script {
    Script {
        elements,
        ..Default::default()
    }
}

/// Every body line lies inside the body, and nothing was given up on.
fn assert_on_the_page(layout: &Layout, format: &ValidFormat) {
    let f = format.get();
    let top = f.margin_top.0;
    let bottom = f.margin_top.0 + f.body_height().0;
    for (n, page) in layout.pages.iter().enumerate() {
        for l in page.lines.iter().filter(|l| l.source.is_some()) {
            assert!(
                l.y.0 > top && l.y.0 <= bottom,
                "page {}: {:?} line at y {} is outside the body {top}..{bottom}",
                n + 1,
                l.kind,
                l.y.0
            );
        }
    }
    let all = layout
        .diagnostics
        .iter()
        .chain(layout.pages.iter().flat_map(|p| &p.diagnostics));
    for d in all {
        assert_ne!(d.code, "no_progress", "{}", d.message);
    }
}

#[test]
fn space_overrides_are_kept_inside_the_body() {
    let format = ValidFormat::default();
    let s = script(vec![
        Element::new(ElementKind::SceneHeading, "INT. HALL - DAY"),
        Element::new(ElementKind::Action, "First."),
        Element::new(ElementKind::Action, "Negative space above."),
        Element::new(ElementKind::Action, "Enormous space above."),
        Element::new(ElementKind::Action, "Last."),
    ]);
    for (neg, huge) in [(-60_000, 2_147_483_000), (i32::MIN, i32::MAX)] {
        let options = Options {
            overrides: BTreeMap::from([(2, Pt(neg)), (3, Pt(huge))]),
            ..Default::default()
        };
        let layout = Engine::new(&s, &format, &options).run();
        assert_on_the_page(&layout, &format);
        let view = layout.page_view(&AllGlyphs);
        for text in [
            "First.",
            "Negative space above.",
            "Enormous space above.",
            "Last.",
        ] {
            assert!(view.contains(text), "{text:?} is missing:\n{view}");
        }
        // A space as tall as the body pushes its element to a new page.
        assert_eq!(layout.pages.len(), 2, "{view}");
    }
}

#[test]
fn huge_title_page_spacing_moves_at_most_one_page() {
    let format = ValidFormat::default();
    let f = format.get();
    let line = |text: &str, space: i32| TitleLine {
        text: RichText::plain(text),
        align: Align::Center,
        left: f.elements.action.left,
        right: f.elements.action.right,
        space_before: Pt(space),
    };
    let options = Options {
        title_lines: Some(vec![
            line("A TITLE", 0),
            line("FAR BELOW", i32::MAX),
            line("FURTHER STILL", i32::MAX),
        ]),
        ..Default::default()
    };
    let s = script(vec![Element::new(ElementKind::Action, "Body.")]);
    let layout = Engine::new(&s, &format, &options).run();
    // A space taller than a page moves to the next page and no further, and
    // nothing is lost.
    let pages: Vec<Vec<&str>> = layout
        .title
        .iter()
        .map(|p| p.lines.iter().map(|l| l.text.as_str()).collect())
        .collect();
    assert_eq!(pages, [["A TITLE"], ["FAR BELOW"], ["FURTHER STILL"]]);
    assert!(
        layout.title[0]
            .diagnostics
            .iter()
            .any(|d| d.code == "title_continued")
    );
    assert_eq!(layout.pages[0].number, Some(1));
}

#[test]
fn a_group_with_more_members_than_fit_in_16_bits_is_laid_out() {
    let format = ValidFormat::default();
    let mut elements = vec![Element::new(ElementKind::Character, "MAYA")];
    let n = 70_000;
    elements.extend((0..n).map(|i| Element::new(ElementKind::Dialogue, format!("L{i}"))));
    elements.push(Element::new(ElementKind::Action, "The end."));
    let layout = Engine::new(&script(elements), &format, &Options::default()).run();
    assert_on_the_page(&layout, &format);
    let last = layout.pages.last().expect("pages");
    let texts: Vec<&str> = last
        .lines
        .iter()
        .filter(|l| matches!(l.kind, LineKind::Content(_)))
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(texts.last(), Some(&"The end."));
    let dialogue = layout
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .filter(|l| matches!(l.kind, LineKind::Content(_)) && l.text.starts_with('L'))
        .count();
    assert_eq!(dialogue, n);
}

#[test]
fn a_very_long_paragraph_of_short_sentences_is_laid_out() {
    // Thousands of sentence starts in one element: every one is a split
    // candidate, so counting each remainder must not rewrap the whole tail.
    let format = ValidFormat::default();
    let text = "Go now. ".repeat(20_000);
    let s = script(vec![Element::new(ElementKind::Action, text.trim_end())]);
    let layout = Engine::new(&s, &format, &Options::default()).run();
    assert_on_the_page(&layout, &format);
    // Sentences may wrap across lines, so count words.
    let words: usize = layout
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .filter(|l| matches!(l.kind, LineKind::Content(_)))
        .map(|l| l.text.split_whitespace().count())
        .sum();
    assert_eq!(words, 40_000);
}
