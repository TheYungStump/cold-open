//! Fountain title keys turned into .fdx title lines print exactly as the
//! keys do (docs/specs/fdx.md §14.3, docs/specs/layout.md §9.3).

use coldopen_layout::{Options, ValidFormat, layout, title_lines};
use coldopen_screenplay::{RichText, Script, TitleField, TitlePage};

fn field(key: &str, values: &[&str]) -> TitleField {
    TitleField::new(key, values.iter().map(|v| RichText::plain(*v)).collect())
}

#[test]
fn title_lines_print_like_the_keys() {
    let pages = |tp: TitlePage| {
        let format = ValidFormat::default();
        let lines = title_lines(&tp, format.get());
        let keys = layout(
            &Script {
                title_page: tp,
                elements: Vec::new(),
            },
            &format,
            &Options::default(),
        );
        let positioned = layout(
            &Script::default(),
            &format,
            &Options {
                title_lines: Some(lines),
                ..Options::default()
            },
        );
        let shape = |l: &coldopen_layout::Layout| -> Vec<Vec<(i32, i32, String)>> {
            l.title
                .iter()
                .map(|p| {
                    p.lines
                        .iter()
                        .map(|x| (x.x.0, x.y.0, x.text.clone()))
                        .collect()
                })
                .collect()
        };
        (shape(&keys), shape(&positioned))
    };
    for tp in [
        TitlePage {
            fields: vec![
                field("Title", &["The Lighthouse", "Part Two"]),
                field("Credit", &["written by"]),
                field("Author", &["A. Writer"]),
                field("Source", &["based on a story"]),
                field("Draft date", &["16 October 2026"]),
                field("Contact", &["someone@example.com", "555 0100"]),
            ],
        },
        TitlePage {
            fields: vec![field("Title", &["Only a title"])],
        },
        TitlePage {
            fields: vec![field("Contact", &["Bottom only"])],
        },
    ] {
        let (keys, lines) = pages(tp);
        assert!(!keys.is_empty() && !keys[0].is_empty());
        assert_eq!(keys, lines);
    }
}

/// Paragraphs cannot share rows, so with blocks at both bottom corners the
/// right one goes just above the left (docs/specs/fdx.md §14.3).
#[test]
fn both_bottom_blocks_stay_on_the_page() {
    let tp = TitlePage {
        fields: vec![
            field("Title", &["T"]),
            field("Contact", &["one", "two"]),
            field("Notes", &["note"]),
        ],
    };
    let format = ValidFormat::default();
    let l = layout(
        &Script::default(),
        &format,
        &Options {
            title_lines: Some(title_lines(&tp, format.get())),
            ..Options::default()
        },
    );
    assert_eq!(l.title.len(), 1);
    let texts: Vec<&str> = l.title[0].lines.iter().map(|x| x.text.as_str()).collect();
    assert_eq!(texts, ["T", "note", "one", "two"]);
}
