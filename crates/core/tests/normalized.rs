//! Review finding F21 (and Antigravity's A12): what the Fountain normal form
//! changes is reported as a `normalized` loss, which never refuses a save
//! (docs/specs/fountain.md §10, docs/specs/commands.md "Losses").

use std::fs;

use coldopen_core::{Command, Dispatcher, Options};
use serde_json::{Value, json};

fn run(
    d: &mut Dispatcher,
    command: &str,
    params: Value,
) -> Result<Value, coldopen_core::CommandError> {
    d.execute(Command::from_parts(command, params).unwrap())
}

fn ok(d: &mut Dispatcher, command: &str, params: Value) -> Value {
    run(d, command, params).unwrap_or_else(|e| panic!("{command} failed: {e}"))
}

fn open(text: &str) -> (Dispatcher, tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("s.fountain");
    fs::write(&p, text).unwrap();
    let mut d = Dispatcher::new(Options::default());
    ok(&mut d, "doc.open", json!({"path": p}));
    (d, dir, p)
}

fn normalized(losses: &Value) -> Vec<(String, String)> {
    losses
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["class"] == "normalized")
        .map(|l| {
            (
                l["id"].as_str().unwrap_or("").to_string(),
                l["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn trailing_blank_lines_in_the_last_action_are_reported_not_refused() {
    // A12's reproduction: an action ending in blank lines as the last block.
    let (mut d, _dir, p) = open("INT. LAB - NIGHT\n\nRain.\n");
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "End.\n\n"}),
    );
    let r = ok(&mut d, "doc.save", json!({}));
    let n = normalized(&r["losses"]);
    assert_eq!(n.len(), 1, "{r}");
    assert_eq!(n[0].0, "b_3");
    assert!(n[0].1.contains("end of the script"), "{n:?}");
    assert!(fs::read_to_string(&p).unwrap().ends_with("End.\n"));
    d.discard();
}

#[test]
fn each_normalisation_names_its_block_and_none_refuses_a_save() {
    let (mut d, _dir, _p) = open("INT. LAB - NIGHT\n\nMAYA\nHello.\n");
    // b_3 is the dialogue: a trailing space, then a dialogue right after it.
    ok(
        &mut d,
        "block.set_text",
        json!({"id": "b_3", "text": "Hello. "}),
    );
    ok(
        &mut d,
        "block.insert",
        json!({"after": "b_3", "type": "dialogue", "text": "Again."}),
    );
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "   "}),
    );
    let r = ok(&mut d, "doc.export", json!({}));
    let n = normalized(&r["losses"]);
    let ids: Vec<&str> = n.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, vec!["b_3", "b_4", "b_5"], "{n:?}");
    assert!(n[0].1.contains("spaces at the start or end"));
    assert!(n[1].1.contains("joined to the block before"));
    assert!(n[2].1.contains("only spaces"));
    let r = ok(&mut d, "doc.save", json!({}));
    assert_eq!(normalized(&r["losses"]).len(), 3);
    d.discard();
}

#[test]
fn a_real_loss_still_asks_first() {
    let (mut d, _dir, _p) = open("INT. LAB - NIGHT\n\nRain.\n");
    ok(
        &mut d,
        "block.insert",
        json!({"type": "shot", "text": "CLOSE ON THE DOOR"}),
    );
    let e = run(&mut d, "doc.save", json!({})).unwrap_err();
    assert_eq!(e.code, "lossy_format");
    let losses = &e.details.as_ref().unwrap()["losses"];
    assert_eq!(losses[0]["class"], "lost", "{losses}");
    d.discard();
}

#[test]
fn a_file_in_normal_form_reports_nothing() {
    let (mut d, _dir, _p) = open(
        "Title: Rain\n\nINT. LAB - NIGHT\n\nRain falls.\n\nMAYA\n(quietly)\nHello.\n\nCUT TO:\n",
    );
    let r = ok(&mut d, "doc.export", json!({}));
    assert_eq!(r["losses"], json!([]), "{r}");
    d.discard();
}
