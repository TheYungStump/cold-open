//! The editor model, driven directly: smart flow, capitals, autocomplete,
//! deleting and joining, undo, paste and caret motion
//! (docs/specs/editor.md §4–§6).

use coldopen_app::editor::{Editor, Motion};
use coldopen_core::{Command, Dispatcher, Options};
use coldopen_screenplay::Pt;
use serde_json::{Value, json};

fn new_script() -> (Dispatcher, Editor) {
    let mut d = Dispatcher::new(Options {
        journal: false,
        fsync: false,
    });
    d.execute(Command::from_parts("doc.new", json!({})).unwrap())
        .unwrap();
    d.execute(
        Command::from_parts("block.insert", json!({"type": "scene_heading", "text": ""})).unwrap(),
    )
    .unwrap();
    let e = Editor::new(&d.session().unwrap().doc);
    (d, e)
}

fn blocks(d: &mut Dispatcher) -> Vec<String> {
    let r = d
        .execute(Command::from_parts("doc.get", json!({})).unwrap())
        .unwrap();
    r["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            format!(
                "{}:{}",
                b["type"].as_str().unwrap(),
                b["text"].as_str().unwrap_or("")
            )
        })
        .collect()
}

fn fountain(d: &mut Dispatcher) -> String {
    let r: Value = d
        .execute(Command::from_parts("doc.export", json!({})).unwrap())
        .unwrap();
    r["text"].as_str().unwrap().to_string()
}

fn typ(d: &mut Dispatcher, e: &mut Editor, s: &str) {
    for c in s.chars() {
        e.insert_text(d, &c.to_string());
    }
}

#[test]
fn smart_flow_writes_a_scene() {
    let (mut d, mut e) = new_script();
    typ(&mut d, &mut e, "int. lab - night");
    e.enter(&mut d);
    typ(&mut d, &mut e, "Rain hammers the roof.");
    e.enter(&mut d);
    e.tab(&mut d, false); // action -> character
    typ(&mut d, &mut e, "maya");
    e.enter(&mut d); // character -> dialogue
    typ(&mut d, &mut e, "We should go.");
    e.enter(&mut d); // dialogue -> character
    typ(&mut d, &mut e, "theo");
    e.enter(&mut d);
    e.tab(&mut d, false); // dialogue -> parenthetical, with brackets
    typ(&mut d, &mut e, "quietly");
    e.enter(&mut d); // parenthetical -> dialogue, from before the ")"
    typ(&mut d, &mut e, "Not yet.");
    assert!(e.problems.is_empty(), "{:?}", e.problems);
    assert_eq!(
        blocks(&mut d),
        [
            "scene_heading:INT. LAB - NIGHT",
            "action:Rain hammers the roof.",
            "character:MAYA",
            "dialogue:We should go.",
            "character:THEO",
            "parenthetical:(quietly)",
            "dialogue:Not yet.",
        ]
    );
    // Enter on the empty block after dialogue... leaves it an action.
    e.enter(&mut d);
    e.enter(&mut d);
    assert_eq!(blocks(&mut d).last().unwrap(), "action:");
}

#[test]
fn autocomplete_finishes_names_and_accepts_with_tab() {
    let (mut d, mut e) = new_script();
    typ(&mut d, &mut e, "INT. LAB - NIGHT");
    e.enter(&mut d);
    e.tab(&mut d, false);
    typ(&mut d, &mut e, "MAYA");
    e.enter(&mut d);
    typ(&mut d, &mut e, "Hi.");
    e.enter(&mut d);
    typ(&mut d, &mut e, "m");
    let c = e.completion.clone().expect("suggestions");
    assert_eq!(c.items, ["MAYA"]);
    e.tab(&mut d, false);
    assert!(e.completion.is_none());
    assert_eq!(blocks(&mut d).last().unwrap(), "character:MAYA");
    // Esc keeps the list closed for this text.
    e.enter(&mut d);
    typ(&mut d, &mut e, "Again.");
    e.enter(&mut d);
    typ(&mut d, &mut e, "m");
    e.dismiss_completion(&d);
    e.refresh_completion(&mut d);
    assert!(e.completion.is_none());
    // Scene headings complete their prefix, location and time.
    e.enter(&mut d);
    e.enter(&mut d);
    let last = blocks(&mut d);
    assert!(
        last.last().unwrap().starts_with("action:")
            || last.last().unwrap().starts_with("scene_heading:")
    );
}

#[test]
fn accents_and_capitals() {
    let (mut d, mut e) = new_script();
    typ(&mut d, &mut e, "ext. café - día");
    e.enter(&mut d);
    typ(&mut d, &mut e, "Señor Müller orders crème brûlée.");
    assert_eq!(
        blocks(&mut d),
        [
            "scene_heading:EXT. CAFÉ - DÍA",
            "action:Señor Müller orders crème brûlée."
        ]
    );
}

#[test]
fn backspace_joins_and_deletes_empty_blocks() {
    let (mut d, mut e) = new_script();
    typ(&mut d, &mut e, "INT. LAB");
    e.enter(&mut d);
    typ(&mut d, &mut e, "One.");
    e.enter(&mut d); // new empty action
    e.delete(&mut d, false, false); // deletes the empty block
    assert_eq!(blocks(&mut d), ["scene_heading:INT. LAB", "action:One."]);
    assert_eq!(e.caret.offset, 4);
    // Split in the middle, then join back.
    e.motion(&mut d, Motion::Left, false, Pt::ZERO);
    e.enter(&mut d);
    assert_eq!(
        blocks(&mut d),
        ["scene_heading:INT. LAB", "action:One", "action:."]
    );
    e.delete(&mut d, false, false);
    assert_eq!(blocks(&mut d), ["scene_heading:INT. LAB", "action:One."]);
    assert_eq!(e.caret.offset, 3);
    // Word delete.
    typ(&mut d, &mut e, " more words");
    e.motion(&mut d, Motion::DocEnd, false, Pt::ZERO);
    e.delete(&mut d, false, true);
    // A word and the punctuation after it go together.
    assert_eq!(blocks(&mut d)[1], "action:One more ");
}

#[test]
fn undo_takes_back_a_burst_and_the_caret_follows() {
    let (mut d, mut e) = new_script();
    typ(&mut d, &mut e, "INT. LAB");
    e.enter(&mut d);
    typ(&mut d, &mut e, "Rain.");
    e.undo(&mut d, false);
    assert_eq!(blocks(&mut d), ["scene_heading:INT. LAB", "action:"]);
    e.undo(&mut d, false);
    assert_eq!(blocks(&mut d), ["scene_heading:INT. LAB"]);
    // The caret moved back into a block that exists.
    assert!(d.session().unwrap().doc.get(e.caret.block).is_some());
    e.undo(&mut d, true);
    e.undo(&mut d, true);
    assert_eq!(blocks(&mut d), ["scene_heading:INT. LAB", "action:Rain."]);
}

#[test]
fn paste_fountain_makes_blocks_and_copy_gives_fountain() {
    let (mut d, mut e) = new_script();
    typ(&mut d, &mut e, "INT. LAB");
    e.enter(&mut d);
    typ(&mut d, &mut e, "Before after.");
    for _ in 0.."after.".len() {
        e.motion(&mut d, Motion::Left, false, Pt::ZERO);
    }
    e.paste(&mut d, "MAYA\nHello.\n\nTHEO\nHi.\n");
    assert_eq!(
        blocks(&mut d),
        [
            "scene_heading:INT. LAB",
            "action:Before ",
            "character:MAYA",
            "dialogue:Hello.",
            "character:THEO",
            "dialogue:Hi.",
            "action:after.",
        ]
    );
    // One undo takes the whole paste back.
    e.undo(&mut d, false);
    assert_eq!(
        blocks(&mut d),
        ["scene_heading:INT. LAB", "action:Before after."]
    );
    e.select_all(&mut d);
    let text = e.copy(&d).unwrap();
    assert!(
        text.contains("INT. LAB") && text.contains("Before after."),
        "{text}"
    );
    // Plain text without line breaks stays in the block.
    e.motion(&mut d, Motion::DocEnd, false, Pt::ZERO);
    e.paste(&mut d, " Then.");
    assert_eq!(blocks(&mut d)[1], "action:Before after. Then.");
}

#[test]
fn an_action_that_starts_like_a_heading_becomes_one() {
    let (mut d, mut e) = new_script();
    typ(&mut d, &mut e, "INT. LAB");
    e.enter(&mut d);
    typ(&mut d, &mut e, "ext.");
    assert_eq!(blocks(&mut d)[1], "scene_heading:ext.");
    typ(&mut d, &mut e, " road");
    assert_eq!(blocks(&mut d)[1], "scene_heading:ext. ROAD");
    // One undo reverses the conversion and keeps the text.
    e.undo(&mut d, false);
    e.undo(&mut d, false);
    assert_eq!(blocks(&mut d)[1], "action:ext.");
    let f = fountain(&mut d);
    assert!(f.contains("ext."), "{f}");
}

#[test]
fn motion_and_selection_across_blocks() {
    let (mut d, mut e) = new_script();
    typ(&mut d, &mut e, "INT. LAB");
    e.enter(&mut d);
    typ(&mut d, &mut e, "Rain falls.");
    e.motion(&mut d, Motion::DocStart, false, Pt::ZERO);
    assert_eq!(e.caret.offset, 0);
    e.motion(&mut d, Motion::WordRight, true, Pt::ZERO);
    assert_eq!(e.caret.offset, 3);
    assert!(e.has_selection());
    e.motion(&mut d, Motion::Down, false, Pt::ZERO);
    let doc = &d.session().unwrap().doc;
    assert_eq!(
        doc.get(e.caret.block).unwrap().element.text.plain_text(),
        "Rain falls."
    );
    e.motion(&mut d, Motion::LineEnd, false, Pt::ZERO);
    assert_eq!(e.caret.offset, 11);
    e.motion(&mut d, Motion::Up, true, Pt::ZERO);
    e.delete(&mut d, false, false);
    assert_eq!(blocks(&mut d), ["scene_heading:INT. LAB"]);
}
