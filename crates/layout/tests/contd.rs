//! The automatic continued marker with a page format's own string
//! (docs/specs/layout.md §7.5).

use coldopen_layout::{LineKind, Options, ValidFormat, layout};
use coldopen_screenplay::{Element, ElementKind, PageFormat, Script};

/// The character cues as printed, with `second` as the name of the speech
/// after an action between two of MAYA's.
fn cues(second: &str) -> Vec<String> {
    let format = ValidFormat::new(PageFormat {
        contd: Some("(CONTINUED)".into()),
        contd_after_action: true,
        ..PageFormat::default()
    })
    .unwrap();
    let script = Script {
        elements: vec![
            Element::new(ElementKind::SceneHeading, "INT. HOUSE - DAY"),
            Element::new(ElementKind::Character, "MAYA"),
            Element::new(ElementKind::Dialogue, "One."),
            Element::new(ElementKind::Action, "She turns."),
            Element::new(ElementKind::Character, second),
            Element::new(ElementKind::Dialogue, "Two."),
        ],
        ..Script::default()
    };
    layout(&script, &format, &Options::default()).pages[0]
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Content(ElementKind::Character))
        .map(|l| l.text.trim().to_string())
        .collect()
}

/// AR9 (the reviews of phases 4–5b, 2026-10-28) says layout adds a second
/// `(CONTINUED)` to a name that has the page format's own marker. It does
/// not: layout looks for the marker in the whole printed name, brackets
/// included. This is that finding's evidence.
#[test]
fn a_name_with_the_formats_own_marker_gets_no_second() {
    assert_eq!(cues("MAYA"), ["MAYA", "MAYA (CONTINUED)"]);
    assert_eq!(cues("MAYA (CONTINUED)"), ["MAYA", "MAYA (CONTINUED)"]);
}
