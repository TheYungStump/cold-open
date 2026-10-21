//! The parts of a document an .fdx file gives it besides the blocks: its
//! page format, title lines and paragraph spacing (docs/specs/journal.md,
//! "The document in the header"; docs/specs/layout.md §4.8, §15.2).

use std::collections::BTreeMap;

use coldopen_core::document::{Block, BlockId, Document};
use coldopen_core::io::SourceStyle;
use coldopen_core::session::Session;
use coldopen_core::{Command, FormatId};
use coldopen_screenplay::{Align, Element, ElementKind, PageFormat, Pt, TitleLine};
use serde_json::{Value, json};

fn action(i: u64, text: &str) -> Block {
    Block::new(BlockId(i), Element::new(ElementKind::Action, text))
}

/// A short script with a format of its own, a title line and an override
/// on its third block.
fn sample() -> Document {
    let format = PageFormat {
        margin_top: Pt::points(81),
        margin_bottom: Pt::points(54),
        ..PageFormat::default()
    };
    let mut blocks = vec![Block::new(
        BlockId(1),
        Element::new(ElementKind::SceneHeading, "INT. LAB - NIGHT"),
    )];
    for i in 2..=40 {
        blocks.push(action(
            i,
            &format!("Line {i}. The rain keeps falling on the roof."),
        ));
    }
    Document {
        next_id: 41,
        blocks,
        page_format: Some(Box::new(format)),
        title_lines: vec![TitleLine {
            text: coldopen_screenplay::RichText::plain("A TITLE"),
            align: Align::Center,
            left: Pt::inch100(100),
            right: Pt::inch100(750),
            space_before: Pt::points(240),
        }],
        spacing: BTreeMap::from([(BlockId(3), Pt::points(36))]),
        ..Document::default()
    }
}

#[test]
fn a_fountain_document_writes_none_of_them() {
    let doc = Document::from_script(coldopen_fountain::parse("INT. A - DAY\n\nRain.\n").script);
    let v = serde_json::to_value(&doc).unwrap();
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["blocks", "next_id", "title_page"]);
}

#[test]
fn they_round_trip_through_json_and_are_checked() {
    let doc = sample();
    let v = serde_json::to_value(&doc).unwrap();
    assert_eq!(v["spacing"], json!({"b_3": 36}));
    assert!(v["page_format"]["margin_top"].is_number(), "{v}");
    let back: Document = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(back, doc);
    assert!(back.check().is_ok());

    // A header naming a block id not below next_id, a space outside the
    // body, or a format layout refuses is not trusted.
    let bad = |f: &dyn Fn(&mut Value)| {
        let mut w = v.clone();
        f(&mut w);
        serde_json::from_value::<Document>(w).unwrap().check()
    };
    assert!(bad(&|w| w["spacing"] = json!({"b_41": 12})).is_err());
    assert!(bad(&|w| w["spacing"] = json!({"b_3": -1})).is_err());
    assert!(bad(&|w| w["spacing"] = json!({"b_3": 100000})).is_err());
    assert!(bad(&|w| w["title_lines"][0]["space_before"] = json!(-5)).is_err());
    assert!(bad(&|w| w["page_format"]["page_height"] = json!(10)).is_err());
}

#[test]
fn revert_renumbering_carries_the_spacing() {
    let doc = sample().renumbered_from(100).unwrap();
    assert_eq!(doc.blocks[2].id, BlockId(102));
    assert_eq!(
        doc.spacing,
        BTreeMap::from([(BlockId(102), Pt::points(36))])
    );
}

fn run(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(&Command::from_parts(command, params).unwrap())
        .unwrap()
}

fn full_layout(s: &Session) -> coldopen_layout::Layout {
    let doc = &s.doc;
    let form = FormatId::Fountain.get().layout_form(doc);
    let map: Vec<usize> = form.sources.elements.iter().map(|e| e.source).collect();
    let options = coldopen_layout::Options {
        overrides: doc.overrides(&map),
        title_lines: Some(doc.title_lines.clone()),
        ..Default::default()
    };
    let format = coldopen_layout::ValidFormat::new(*doc.page_format.clone().unwrap()).unwrap();
    coldopen_layout::layout(&form.script, &format, &options)
}

/// Checks the view against a full layout, and returns the overrides it
/// laid out with.
fn check(s: &mut Session) -> BTreeMap<usize, Pt> {
    let full = full_layout(s);
    let view = s.view();
    assert_eq!(view.layout(), &full);
    let map = view.map.clone();
    s.doc.overrides(&map)
}

/// The view lays the document out with its own format, title lines and
/// overrides, and keeps each override on its block as edits move it.
#[test]
fn the_view_uses_them_and_follows_edits() {
    let mut s = Session::new(sample(), None, FormatId::Fountain, SourceStyle::default());
    assert_eq!(check(&mut s), BTreeMap::from([(2, Pt::points(36))]));
    assert_eq!(s.view().layout().title.len(), 1);

    // Blocks inserted and deleted before the overridden block shift its
    // index; the override must stay with it.
    run(
        &mut s,
        "block.insert",
        json!({"before": "b_2", "type": "action", "text": "New."}),
    );
    run(
        &mut s,
        "block.insert",
        json!({"before": "b_2", "type": "action", "text": "More."}),
    );
    assert_eq!(check(&mut s), BTreeMap::from([(4, Pt::points(36))]));
    run(&mut s, "block.delete", json!({"id": "b_2"}));
    assert_eq!(check(&mut s), BTreeMap::from([(3, Pt::points(36))]));
    run(&mut s, "block.delete", json!({"id": "b_3"}));
    assert!(check(&mut s).is_empty());
    run(&mut s, "undo", json!({}));
    assert_eq!(check(&mut s), BTreeMap::from([(3, Pt::points(36))]));

    // The PDF path lays out the same pages.
    let (pdf, _) = s.layout(&coldopen_layout::Options::default());
    assert_eq!(pdf.pages, full_layout(&s).pages);
}
