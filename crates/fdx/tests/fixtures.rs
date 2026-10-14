//! .fdx fixtures: every case in tests/fixtures/fdx.
//!
//! Each `<case>.fdx` has a `<case>.expected.json` holding the script, the
//! overrides, the title lines, the scene properties, the report codes in
//! order, and the page format as a JSON merge patch (RFC 7386) against
//! `PageFormat::default()`. See tests/fixtures/fdx/README.md.
//!
//! `COLDOPEN_BLESS=1 cargo test -p coldopen-fdx --test fixtures` rewrites
//! the expectations from the reader's output; it refuses to when `CI` is
//! set, so CI can never approve its own output.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_fdx::{FdxDoc, read};
use coldopen_screenplay::PageFormat;
use serde_json::{Map, Value};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx")
}

/// The merge patch that turns `from` into `to`.
fn merge_patch(from: &Value, to: &Value) -> Value {
    match (from, to) {
        (Value::Object(a), Value::Object(b)) => {
            let mut patch = Map::new();
            for (k, va) in a {
                match b.get(k) {
                    None => {
                        patch.insert(k.clone(), Value::Null);
                    }
                    Some(vb) if vb != va => {
                        patch.insert(k.clone(), merge_patch(va, vb));
                    }
                    Some(_) => {}
                }
            }
            for (k, vb) in b {
                if !a.contains_key(k) {
                    patch.insert(k.clone(), vb.clone());
                }
            }
            Value::Object(patch)
        }
        _ => to.clone(),
    }
}

/// Applies a merge patch (RFC 7386 §2).
fn apply_patch(target: &Value, patch: &Value) -> Value {
    let Value::Object(p) = patch else {
        return patch.clone();
    };
    let mut out = match target {
        Value::Object(t) => t.clone(),
        _ => Map::new(),
    };
    for (k, v) in p {
        if v.is_null() {
            out.remove(k);
        } else {
            let current = out.get(k).cloned().unwrap_or(Value::Null);
            out.insert(k.clone(), apply_patch(&current, v));
        }
    }
    Value::Object(out)
}

/// The fields compared, in the order they are written.
fn actual_fields(doc: &FdxDoc) -> Vec<(&'static str, Value)> {
    let default = serde_json::to_value(PageFormat::default()).unwrap();
    let format = serde_json::to_value(&doc.format).unwrap();
    vec![
        ("script", serde_json::to_value(&doc.script).unwrap()),
        ("overrides", serde_json::to_value(&doc.overrides).unwrap()),
        (
            "title_lines",
            serde_json::to_value(&doc.title_lines).unwrap(),
        ),
        ("scenes", serde_json::to_value(&doc.scenes).unwrap()),
        (
            "report",
            Value::Array(
                doc.report
                    .iter()
                    .map(|i| Value::String(i.code.to_string()))
                    .collect(),
            ),
        ),
        ("format", merge_patch(&default, &format)),
    ]
}

/// The expectation file's text: the fields in a fixed order, pretty
/// printed, ending with a newline.
fn render(fields: &[(&'static str, Value)]) -> String {
    let mut s = String::from("{\n");
    for (n, (key, value)) in fields.iter().enumerate() {
        let pretty = serde_json::to_string_pretty(value).unwrap();
        let indented = pretty.replace('\n', "\n  ");
        let comma = if n + 1 < fields.len() { "," } else { "" };
        s.push_str(&format!("  \"{key}\": {indented}{comma}\n"));
    }
    s.push_str("}\n");
    s
}

fn bless_requested() -> bool {
    let bless = std::env::var("COLDOPEN_BLESS").is_ok_and(|v| v == "1");
    if bless && std::env::var("CI").is_ok_and(|v| !v.is_empty()) {
        panic!("refusing to bless fixture expectations with CI set");
    }
    bless
}

fn case_names() -> Vec<String> {
    let dir = fixtures();
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter_map(|n| n.strip_suffix(".fdx").map(str::to_string))
        .collect();
    names.sort();
    names
}

#[test]
fn fixtures_match_expectations() {
    let bless = bless_requested();
    let dir = fixtures();
    let names = case_names();
    assert!(!names.is_empty(), "no fixtures in {}", dir.display());
    let mut failures = Vec::new();
    for name in &names {
        let input = dir.join(format!("{name}.fdx"));
        let expected_path = dir.join(format!("{name}.expected.json"));
        let bytes = fs::read(&input).unwrap_or_else(|e| panic!("{}: {e}", input.display()));
        let doc = match read(&bytes) {
            Ok(doc) => doc,
            Err(e) => {
                failures.push(format!("{name}: read failed: {e}"));
                continue;
            }
        };
        if let Err(e) = coldopen_fdx::fuzzing::read(&bytes) {
            failures.push(format!("{name}: {e}"));
        }
        let fields = actual_fields(&doc);
        if bless {
            fs::write(&expected_path, render(&fields))
                .unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()));
            continue;
        }
        let Ok(text) = fs::read_to_string(&expected_path) else {
            failures.push(format!(
                "{name}: {} is missing; write it, or run with COLDOPEN_BLESS=1 and review it",
                expected_path.display()
            ));
            continue;
        };
        let expected: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()));
        let default = serde_json::to_value(PageFormat::default()).unwrap();
        for (key, actual) in &fields {
            let want = expected.get(*key).cloned().unwrap_or(Value::Null);
            let (got, want) = if *key == "format" {
                (apply_patch(&default, actual), apply_patch(&default, &want))
            } else {
                (actual.clone(), want)
            };
            if got != want {
                failures.push(format!(
                    "{name}: {key} differs\n--- expected\n{}\n--- actual\n{}",
                    serde_json::to_string_pretty(&want).unwrap(),
                    serde_json::to_string_pretty(&got).unwrap()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn every_expectation_has_a_fixture() {
    let names = case_names();
    for entry in fs::read_dir(fixtures()).unwrap().filter_map(Result::ok) {
        let file = entry.file_name().to_string_lossy().into_owned();
        if let Some(stem) = file.strip_suffix(".expected.json") {
            assert!(
                names.iter().any(|n| n == stem),
                "{file} has no matching .fdx"
            );
        }
    }
}

#[test]
fn merge_patch_round_trips() {
    let a: Value = serde_json::json!({"x": 1, "y": {"p": 1, "q": 2}, "z": "s"});
    let b: Value = serde_json::json!({"x": 1, "y": {"p": 3}, "w": true, "z": null});
    let patch = merge_patch(&a, &b);
    assert_eq!(
        patch,
        serde_json::json!({"y": {"p": 3, "q": null}, "w": true, "z": null})
    );
    // A null value in `to` is removed by the patch, as RFC 7386 says.
    let mut expected = b.as_object().unwrap().clone();
    expected.remove("z");
    assert_eq!(apply_patch(&a, &patch), Value::Object(expected));
}
