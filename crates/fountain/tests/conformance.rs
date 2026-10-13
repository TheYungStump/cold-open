//! Fountain conformance: every case in tests/fixtures/fountain.
//!
//! Each `<case>.fountain` has a hand-written `<case>.expected.jsonl`, and a
//! `<case>.out.fountain` when the canonical text differs from the input.
//! Optional `<case>.diagnostics` lists the diagnostic codes, one per line.
//! See tests/fixtures/fountain/README.md.

use std::fs;
use std::path::{Path, PathBuf};

use coldopen_fountain::{parse, write};
use coldopen_screenplay::{Element, Script, TitlePage};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fountain")
}

struct Case {
    name: String,
    input: String,
    expected: Script,
    canonical: String,
    diagnostics: Vec<String>,
}

fn read(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    String::from_utf8(bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn load_expected(path: &Path) -> Script {
    let mut script = Script::default();
    for (n, line) in read(path).lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("{}:{}: {e}", path.display(), n + 1));
        if let Some(tp) = v.get("title_page") {
            script.title_page = serde_json::from_value::<TitlePage>(tp.clone())
                .unwrap_or_else(|e| panic!("{}:{}: {e}", path.display(), n + 1));
        } else {
            script.elements.push(
                serde_json::from_value::<Element>(v)
                    .unwrap_or_else(|e| panic!("{}:{}: {e}", path.display(), n + 1)),
            );
        }
    }
    script
}

fn cases() -> Vec<Case> {
    let dir = fixtures();
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".fountain") && !n.ends_with(".out.fountain"))
        .map(|n| n.trim_end_matches(".fountain").to_string())
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let input = read(&dir.join(format!("{name}.fountain")));
            let out = dir.join(format!("{name}.out.fountain"));
            let canonical = if out.exists() {
                read(&out)
            } else {
                input.clone()
            };
            let diag = dir.join(format!("{name}.diagnostics"));
            let diagnostics = if diag.exists() {
                read(&diag)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            } else {
                Vec::new()
            };
            Case {
                expected: load_expected(&dir.join(format!("{name}.expected.jsonl"))),
                name,
                input,
                canonical,
                diagnostics,
            }
        })
        .collect()
}

fn dump(s: &Script) -> String {
    let mut out = String::new();
    if !s.title_page.is_empty() {
        out.push_str(&format!(
            "{{\"title_page\":{}}}\n",
            serde_json::to_string(&s.title_page).unwrap()
        ));
    }
    for e in &s.elements {
        out.push_str(&serde_json::to_string(e).unwrap());
        out.push('\n');
    }
    out
}

fn finish(kind: &str, total: usize, failures: Vec<String>) {
    assert!(total >= 80, "expected at least 80 cases, found {total}");
    if !failures.is_empty() {
        panic!(
            "{} of {total} {kind} cases failed:\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

/// Parsing each fixture gives exactly the hand-written expectation.
#[test]
fn parse_matches_expected() {
    let cases = cases();
    let mut failures = Vec::new();
    for c in &cases {
        let parsed = parse(&c.input);
        if parsed.script != c.expected {
            failures.push(format!(
                "[{}] parse mismatch\n--- expected\n{}--- actual\n{}",
                c.name,
                dump(&c.expected),
                dump(&parsed.script)
            ));
        }
        let codes: Vec<&str> = parsed.diagnostics.iter().map(|d| d.code).collect();
        if codes != c.diagnostics {
            failures.push(format!(
                "[{}] diagnostics: expected {:?}, got {:?}",
                c.name, c.diagnostics, codes
            ));
        }
    }
    finish("parse", cases.len(), failures);
}

/// Writing gives the canonical text, without losses; the canonical text
/// parses back to the same script; writing again changes nothing.
#[test]
fn write_is_canonical_and_round_trips() {
    let cases = cases();
    let mut failures = Vec::new();
    for c in &cases {
        let written = write(&c.expected);
        if written.text != c.canonical {
            failures.push(format!(
                "[{}] canonical text mismatch\n--- expected\n{}--- actual\n{}",
                c.name, c.canonical, written.text
            ));
        }
        if !written.losses.is_empty() {
            failures.push(format!(
                "[{}] unexpected losses: {:?}",
                c.name, written.losses
            ));
        }
        let reparsed = parse(&c.canonical).script;
        if reparsed != c.expected {
            failures.push(format!(
                "[{}] canonical text does not parse back\n--- expected\n{}--- actual\n{}",
                c.name,
                dump(&c.expected),
                dump(&reparsed)
            ));
        }
        let again = write(&parse(&written.text).script).text;
        if again != written.text {
            failures.push(format!(
                "[{}] writing is not idempotent\n--- first\n{}--- second\n{}",
                c.name, written.text, again
            ));
        }
    }
    finish("write", cases.len(), failures);
}
