//! The writer's rules (docs/specs/fdx.md §13 to §15), on small files we
//! write ourselves.

use std::collections::BTreeMap;

use coldopen_fdx::{Blk, Doc, FdxDoc, read, write};
use coldopen_screenplay::{Element, ElementKind, PageFormat, Pt, RichText, Run, Style};

const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"4\">\n\n  <Content>";
const TAIL: &str = "\n  </Content>\n\n  <Revisions ActiveSet=\"1\">\n    <Revision ID=\"1\" Name=\"First\"/>\n  </Revisions>\n</FinalDraft>\n";

fn file(paragraphs: &str) -> String {
    format!("{HEAD}{paragraphs}{TAIL}")
}

/// Writes `doc` with its elements replaced by `elements` (same ids where
/// the index is kept, new ids past the end).
fn rewrite(doc: &FdxDoc, elements: &[(u64, Element)]) -> coldopen_fdx::Written {
    let blocks = elements
        .iter()
        .map(|(id, e)| Blk {
            id: *id,
            element: e,
            spacing: None,
        })
        .collect();
    write(&Doc {
        blocks,
        format: &doc.format,
        title_lines: &doc.title_lines,
        kept: Some(&doc.kept),
        declare_utf8: false,
    })
}

fn ids(doc: &FdxDoc) -> Vec<(u64, Element)> {
    doc.script
        .elements
        .iter()
        .enumerate()
        .map(|(i, e)| (i as u64 + 1, e.clone()))
        .collect()
}

#[test]
fn an_unknown_section_and_scene_properties_survive_an_edit() {
    let text = file(
        "\n    <Paragraph Number=\"4\" Type=\"Scene Heading\">\n      <SceneProperties Length=\"3/8\" Page=\"2\" Title=\"\"/>\n      <Text>INT. LAB - NIGHT</Text>\n    </Paragraph>\n    <Paragraph Type=\"Action\">\n      <Text>Rain.</Text>\n    </Paragraph>",
    );
    let doc = read(text.as_bytes()).unwrap();
    assert_eq!(
        doc.kept.paragraphs.len(),
        1,
        "the heading is kept for its properties"
    );
    assert_eq!(doc.write_back().text, text);

    let mut els = ids(&doc);
    els[0].1.text = RichText::plain("INT. LAB - DAY");
    let out = rewrite(&doc, &els).text;
    assert!(
        out.contains("<SceneProperties Length=\"3/8\" Page=\"2\" Title=\"\"/>"),
        "{out}"
    );
    assert!(
        out.contains("<Paragraph Number=\"4\" Type=\"Scene Heading\">"),
        "{out}"
    );
    assert!(out.contains("INT. LAB - DAY"), "{out}");
    assert!(out.ends_with(TAIL), "the revisions list stays: {out}");
    // Everything but the edited paragraph is as it was.
    assert!(out.contains("<Text>Rain.</Text>"));

    // As an action, the heading's scene properties have no place.
    els[0].1.kind = ElementKind::Action;
    els[0].1.scene_number = None;
    let out = rewrite(&doc, &els).text;
    assert!(!out.contains("SceneProperties"), "{out}");
}

/// Run attributes we don't model follow the characters they were on
/// (§14.4): an insertion takes the attributes of the character before it.
#[test]
fn run_attributes_follow_their_characters() {
    let text = file(
        "\n    <Paragraph Type=\"Action\">\n      <Text Font=\"Courier\" RevisionID=\"1\">Old words </Text>\n      <Text Font=\"Courier\" RevisionID=\"2\">new words.</Text>\n    </Paragraph>",
    );
    let doc = read(text.as_bytes()).unwrap();
    assert_eq!(doc.write_back().text, text);
    let mut els = ids(&doc);
    els[0].1.text = RichText::plain("Old words and more new words.");
    let out = rewrite(&doc, &els).text;
    assert!(
        out.contains("<Text Font=\"Courier\" RevisionID=\"1\">Old words and more </Text>"),
        "{out}"
    );
    assert!(
        out.contains("<Text Font=\"Courier\" RevisionID=\"2\">new words.</Text>"),
        "{out}"
    );
    // Styles are always the model's.
    els[0].1.text = RichText::new(vec![
        Run::styled(
            "Old",
            Style {
                bold: true,
                ..Style::PLAIN
            },
        ),
        Run::plain(" words new words."),
    ]);
    let out = rewrite(&doc, &els).text;
    assert!(
        out.contains("<Text Font=\"Courier\" RevisionID=\"1\" Style=\"Bold\">Old</Text>"),
        "{out}"
    );
}

#[test]
fn an_unknown_type_stays_while_the_block_stays_general() {
    let text = file(
        "\n    <Paragraph StartsNewPage=\"Yes\" Type=\"New Act\">\n      <Text>ACT TWO</Text>\n    </Paragraph>",
    );
    let doc = read(text.as_bytes()).unwrap();
    let mut els = ids(&doc);
    let k = els
        .iter()
        .position(|(_, e)| e.kind == ElementKind::General)
        .unwrap();
    els[k].1.text = RichText::plain("ACT THREE");
    let out = rewrite(&doc, &els).text;
    assert!(out.contains("Type=\"New Act\""), "{out}");
    els[k].1.kind = ElementKind::Action;
    let out = rewrite(&doc, &els).text;
    assert!(
        out.contains("Type=\"Action\"") && !out.contains("New Act"),
        "{out}"
    );
}

#[test]
fn loose_children_keep_their_place() {
    let text = file(
        "\n    <!-- before -->\n    <Paragraph Type=\"Action\">\n      <Text>One.</Text>\n    </Paragraph>\n    <Marker Kind=\"x\"/>\n    <Paragraph Type=\"Action\">\n      <Text>Two.</Text>\n    </Paragraph>",
    );
    let doc = read(text.as_bytes()).unwrap();
    assert_eq!(doc.write_back().text, text);
    // Deleting the paragraph a child follows moves it after the one before.
    let els: Vec<(u64, Element)> = ids(&doc).into_iter().skip(1).collect();
    let out = rewrite(&doc, &els).text;
    let marker = out.find("<Marker").unwrap();
    let two = out.find("Two.").unwrap();
    assert!(
        out.find("<!-- before -->").unwrap() < marker && marker < two,
        "{out}"
    );
}

#[test]
fn what_fdx_cannot_hold_is_listed() {
    let format = PageFormat::default();
    let mut with_note = Element::new(
        ElementKind::Action,
        RichText::new(vec![Run::plain("A line."), Run::Note("check".into())]),
    );
    with_note.space_before = Some(200);
    let elements = [
        Element::new(ElementKind::Section, "ACT ONE"),
        Element::new(ElementKind::Synopsis, "They meet."),
        Element::new(ElementKind::Note, "a note"),
        Element::new(ElementKind::Boneyard, "cut"),
        with_note,
        Element::new(ElementKind::Character, "PIP").dual(),
    ];
    let blocks = elements
        .iter()
        .enumerate()
        .map(|(i, e)| Blk {
            id: i as u64 + 1,
            element: e,
            spacing: None,
        })
        .collect();
    let written = write(&Doc {
        blocks,
        format: &format,
        title_lines: &[],
        kept: None,
        declare_utf8: false,
    });
    let codes: Vec<(&str, Option<u64>)> =
        written.losses.iter().map(|l| (l.code, l.block)).collect();
    assert_eq!(
        codes,
        [
            ("section_element", Some(1)),
            ("synopsis_element", Some(2)),
            ("note_element", Some(3)),
            ("boneyard_element", Some(4)),
            ("spacing_out_of_range", Some(5)),
            ("inline_note", Some(5)),
            ("dual_unpaired", Some(6)),
        ]
    );
}

/// A new file's settings read back as the format they were written from
/// (§14.6), and the file reads with nothing to report but what layout
/// leaves at its defaults.
#[test]
fn a_new_file_reads_back_as_written() {
    let format = PageFormat::default();
    let elements = [
        Element::new(ElementKind::SceneHeading, "INT. LAB - NIGHT").scene_number("7A"),
        Element::new(ElementKind::Action, "Rain & <wind>.").centered(),
        Element::new(ElementKind::Character, "PIP"),
        Element::new(ElementKind::Dialogue, "Left."),
        Element::new(ElementKind::Character, "POPPY").dual(),
        Element::new(ElementKind::Dialogue, "Right."),
        Element::new(ElementKind::PageBreak, ""),
        Element::new(ElementKind::Shot, "ANGLE ON THE DOOR"),
        Element::new(ElementKind::General, "Plain."),
        Element::new(ElementKind::Lyric, "La la."),
        Element::new(ElementKind::Transition, "CUT TO:"),
    ];
    let spacing = BTreeMap::from([(8usize, Pt::points(24))]);
    coldopen_fdx::fuzzing::write_document(&elements, &spacing).unwrap();
    let blocks = elements
        .iter()
        .enumerate()
        .map(|(i, e)| Blk {
            id: i as u64 + 1,
            element: e,
            spacing: spacing.get(&i).copied(),
        })
        .collect();
    let text = write(&Doc {
        blocks,
        format: &format,
        title_lines: &[],
        kept: None,
        declare_utf8: false,
    })
    .text;
    let doc = read(text.as_bytes()).unwrap();
    assert!(doc.report.is_empty(), "{:?}", doc.report);
    assert_eq!(doc.format, format);
    assert!(text.contains("Font=\"Courier Prime\""));
    assert!(text.contains("<DualDialogue>"));
}

/// The markup style is read from the file (fdx.md §13.1, 2026-10-17): a
/// file with CRLF line breaks, `Content` four spaces in and a two-space
/// step after that needs no paragraph kept as raw text, and an edited
/// paragraph is written in that style.
#[test]
fn the_files_newline_and_indentation_are_followed() {
    let text = [
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>",
        "<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"4\">",
        "    <Content>",
        "      <Paragraph Type=\"Scene Heading\">",
        "        <Text>INT. FERRY - NIGHT</Text>",
        "      </Paragraph>",
        "      <Paragraph Type=\"Action\">",
        "        <Text>Fog on the rails.</Text>",
        "      </Paragraph>",
        "    </Content>",
        "</FinalDraft>",
        "",
    ]
    .join("\r\n");
    let doc = read(text.as_bytes()).unwrap();
    assert_eq!(doc.kept.style.newline, "\r\n");
    assert_eq!(doc.kept.style.unit, "  ");
    assert_eq!(doc.kept.style.offset, "  ");
    assert!(doc.kept.paragraphs.is_empty(), "{:?}", doc.kept.paragraphs);
    assert_eq!(doc.write_back().text, text);

    let mut elements = ids(&doc);
    elements[1].1.text = RichText::plain("Fog on the rails. A horn.");
    let out = rewrite(&doc, &elements).text;
    assert_eq!(
        out,
        text.replace("Fog on the rails.", "Fog on the rails. A horn.")
    );
}

/// GR8 (the reviews of phases 4–5b, 2026-10-28): a heading read after a
/// `StartsNewPage` break, written again without that break, keeps its scene
/// properties. The store keys its paragraph by the break's id; the writer
/// finds it by the heading's.
#[test]
fn a_kept_heading_keeps_its_parts_without_its_page_break() {
    let text = file(
        "\n    <Paragraph Type=\"Action\">\n      <Text>Before.</Text>\n    </Paragraph>\n    <Paragraph Type=\"Scene Heading\" StartsNewPage=\"Yes\">\n      <SceneProperties Length=\"1/8\" Page=\"2\" Title=\"\"/>\n      <Text>INT. LAB - NIGHT</Text>\n    </Paragraph>\n    <Paragraph Type=\"Action\">\n      <Text>Rain.</Text>\n    </Paragraph>",
    );
    let doc = read(text.as_bytes()).unwrap();
    let kinds: Vec<_> = doc.script.elements.iter().map(|e| e.kind).collect();
    assert_eq!(kinds[1], ElementKind::PageBreak, "{kinds:?}");
    let elements: Vec<(u64, Element)> = doc
        .script
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind != ElementKind::PageBreak)
        .map(|(i, e)| (i as u64 + 1, e.clone()))
        .collect();
    let out = rewrite(&doc, &elements).text;
    assert!(out.contains("SceneProperties"), "{out}");
    assert!(!out.contains("StartsNewPage"), "{out}");
}

/// P2 (found while fixing GR8, 2026-10-28): an unedited kept first
/// speaker was copied as it was even when the next speaker was made its
/// dual partner, so the dual block was never written and the second
/// speaker was reported `dual_unpaired`.
#[test]
fn a_kept_first_speaker_joins_a_new_dual_block() {
    let text = file(
        "\n    <Paragraph Type=\"Character\" Custom=\"keep\">\n      <Text>ADA</Text>\n    </Paragraph>\n    <Paragraph Type=\"Dialogue\">\n      <Text>Wait.</Text>\n    </Paragraph>\n    <Paragraph Type=\"Character\">\n      <Text>BEN</Text>\n    </Paragraph>\n    <Paragraph Type=\"Dialogue\">\n      <Text>Go.</Text>\n    </Paragraph>",
    );
    let doc = read(text.as_bytes()).unwrap();
    let mut elements: Vec<(u64, Element)> = doc
        .script
        .elements
        .iter()
        .enumerate()
        .map(|(i, e)| (i as u64 + 1, e.clone()))
        .collect();
    elements[2].1.dual = true;
    let out = rewrite(&doc, &elements);
    assert!(out.text.contains("<DualDialogue>"), "{}", out.text);
    assert!(out.losses.is_empty(), "{:?}", out.losses);
    assert!(out.text.contains("Custom=\"keep\""), "{}", out.text);
}

/// P3 (found while fixing GR8, 2026-10-28): the first paragraph's
/// `StartsNewPage="Yes"` makes no break, since nothing comes before it
/// (§6.5). Copied as it was after a paragraph is added before it, it would
/// make one when the file is read again.
#[test]
fn a_first_paragraphs_break_does_not_appear_when_something_comes_before_it() {
    let text = file(
        "\n    <Paragraph Type=\"Action\" StartsNewPage=\"Yes\" Custom=\"keep\">\n      <Text>Dawn.</Text>\n    </Paragraph>\n    <Paragraph Type=\"Action\">\n      <Text>Rain.</Text>\n    </Paragraph>",
    );
    let doc = read(text.as_bytes()).unwrap();
    assert!(
        !doc.script
            .elements
            .iter()
            .any(|e| e.kind == ElementKind::PageBreak)
    );
    let mut elements: Vec<(u64, Element)> =
        vec![(10, Element::new(ElementKind::Action, "Before."))];
    elements.extend(
        doc.script
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| (i as u64 + 1, e.clone())),
    );
    let out = rewrite(&doc, &elements).text;
    let back = read(out.as_bytes()).unwrap();
    let kinds: Vec<ElementKind> = back.script.elements.iter().map(|e| e.kind).collect();
    assert!(!kinds.contains(&ElementKind::PageBreak), "{kinds:?}\n{out}");
    assert!(out.contains("Custom=\"keep\""), "{out}");
}
