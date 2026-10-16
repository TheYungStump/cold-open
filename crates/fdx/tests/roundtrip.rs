//! The round-trip gate (docs/specs/fdx.md §16.1), on our own fixtures and,
//! when it is present, on the private corpus of public sample files.
//!
//! - G1: a file with no repair, written back unedited, is byte for byte the
//!   file.
//! - G2: reading what was written gives the same document, with no repair,
//!   and writing that again gives the same bytes.
//!
//! The corpus is never committed (see CLAUDE.md). Without it the corpus
//! half skips with a banner on standard error, as the reference gate does;
//! `COLDOPEN_REQUIRE_CORPUS=1` makes a skip fail. Nothing printed here ever
//! holds script text: differences are shown with text and attribute values
//! masked.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use std::collections::BTreeMap;

use coldopen_fdx::{Blk, Doc, FdxDoc, decode, encode, read};
use coldopen_screenplay::{Element, ElementKind, Pt, RichText, Run};

fn corpus_dir() -> PathBuf {
    match std::env::var_os("COLDOPEN_CORPUS") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
    }
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx")
}

fn fdx_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            fdx_files(&p, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fdx")) {
            out.push(p);
        }
    }
}

/// Markup with text and attribute values masked: letters and digits in
/// them become `x`.
fn mask(s: &str) -> String {
    let mut out = String::new();
    let (mut tag, mut quote) = (false, None::<char>);
    for c in s.chars() {
        match (tag, quote) {
            (true, Some(q)) => {
                if c == q {
                    quote = None;
                }
                out.push(if c.is_alphanumeric() { 'x' } else { c });
                continue;
            }
            (true, None) => match c {
                '"' | '\'' => quote = Some(c),
                '>' => tag = false,
                _ => {}
            },
            (false, _) => {
                if c == '<' {
                    tag = true;
                } else {
                    out.push(if c.is_alphanumeric() { 'x' } else { c });
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Where two byte strings first differ, with the markup around it masked.
fn first_difference(a: &[u8], b: &[u8]) -> String {
    let at = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let line = a[..at].iter().filter(|&&c| c == b'\n').count() + 1;
    let window = |s: &[u8]| {
        let start = at.saturating_sub(80);
        let end = (at + 80).min(s.len());
        mask(&String::from_utf8_lossy(&s[start..end]))
    };
    format!(
        "byte {at} (line {line}; lengths {} and {})\n  file:    {:?}\n  written: {:?}",
        a.len(),
        b.len(),
        window(a),
        window(b)
    )
}

/// The parts of a document G2 compares.
fn model(d: &FdxDoc) -> String {
    format!(
        "{:?}\n{:?}\n{:?}\n{:?}",
        d.script, d.format, d.overrides, d.title_lines
    )
}

struct Outcome {
    identical: bool,
    repaired: bool,
    kept: usize,
    store: usize,
    ms: u128,
}

/// Runs G1 and G2 on one file. Returns what to report, or why it failed.
fn check(path: &Path) -> Result<Outcome, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let doc = read(&bytes).map_err(|e| format!("cannot read: {e}"))?;
    let written = doc.write_back();
    let out = encode(&written.text, doc.encoding);
    let ms = start.elapsed().as_millis();
    if !written.losses.is_empty() {
        return Err(format!("writing back reports losses: {:?}", written.losses));
    }
    let repaired = doc.repaired();
    let identical = out == bytes;
    if !repaired && !identical {
        return Err(format!(
            "G1: bytes differ at {}",
            first_difference(&bytes, &out)
        ));
    }
    // G2.
    let again = read(&out).map_err(|e| format!("G2: cannot read what was written: {e}"))?;
    if again.repaired() {
        let codes: Vec<&str> = again
            .report
            .iter()
            .filter(|i| i.is_repair())
            .map(|i| i.code)
            .collect();
        return Err(format!("G2: what was written needs repair: {codes:?}"));
    }
    if model(&again) != model(&doc) {
        return Err("G2: reading what was written gives another document".into());
    }
    if !repaired && again.kept != doc.kept {
        return Err("G2: reading what was written gives another store".into());
    }
    let twice = encode(&again.write_back().text, again.encoding);
    if twice != out {
        return Err(format!(
            "G2: writing again gives other bytes, at {}",
            first_difference(&out, &twice)
        ));
    }
    if !repaired {
        edits_stay_local(&doc, &bytes)?;
    }
    let store = serde_json::to_string(&doc.kept).map_or(0, |s| s.len());
    Ok(Outcome {
        identical,
        repaired,
        kept: doc.kept.paragraphs.len(),
        store,
        ms,
    })
}

/// G3: scripted edits change only the edited paragraphs. One printing
/// block's text is changed, a block inserted, one deleted and a scene
/// moved to the end; everything outside `Content` must be as it was, and
/// all but the edited paragraphs must come back as they were.
fn edits_stay_local(doc: &FdxDoc, bytes: &[u8]) -> Result<(), String> {
    let mut blocks: Vec<(u64, Element, Option<Pt>)> = doc
        .blocks()
        .iter()
        .map(|b| (b.id, b.element.clone(), b.spacing))
        .collect();
    if blocks.len() < 8 {
        return Ok(());
    }
    let plain = |e: &Element| {
        matches!(e.kind, ElementKind::Action | ElementKind::Dialogue) && !e.text.is_empty()
    };
    // Away from page breaks and dual dialogue, whose shape an edit there
    // would change.
    let lone = |b: &[(u64, Element, Option<Pt>)], k: usize| {
        (k == 0 || b[k - 1].1.kind != ElementKind::PageBreak)
            && b.get(k + 1)
                .is_none_or(|n| n.1.kind != ElementKind::PageBreak)
            && !b[k.saturating_sub(4)..(k + 5).min(b.len())]
                .iter()
                .any(|n| n.1.dual)
    };
    // Change one block's text.
    let Some(edit) = (0..blocks.len()).find(|&k| plain(&blocks[k].1) && lone(&blocks, k)) else {
        return Ok(());
    };
    let mut runs: Vec<Run> = blocks[edit].1.text.runs().to_vec();
    runs.push(Run::plain(" Then the rain stops."));
    blocks[edit].1.text = RichText::new(runs);
    // Insert one after it.
    let next_id = blocks.iter().map(|b| b.0).max().unwrap_or(0) + 1;
    blocks.insert(
        edit + 1,
        (
            next_id,
            Element::new(ElementKind::Action, "A new line."),
            None,
        ),
    );
    // Delete another plain one.
    if let Some(del) = (edit + 3..blocks.len()).find(|&k| plain(&blocks[k].1) && lone(&blocks, k)) {
        blocks.remove(del);
    }
    // Move the second scene to the end.
    let heads: Vec<usize> = (0..blocks.len())
        .filter(|&k| blocks[k].1.kind == ElementKind::SceneHeading)
        .collect();
    if heads.len() >= 3 {
        let mut start = heads[1];
        while start > 0 && blocks[start - 1].1.kind == ElementKind::PageBreak {
            start -= 1;
        }
        let mut end = heads[2];
        while end > 0 && blocks[end - 1].1.kind == ElementKind::PageBreak {
            end -= 1;
        }
        let scene: Vec<_> = blocks.drain(start..end).collect();
        blocks.extend(scene);
    }

    let blks: Vec<Blk<'_>> = blocks
        .iter()
        .map(|(id, element, spacing)| Blk {
            id: *id,
            element,
            spacing: *spacing,
        })
        .collect();
    let written = coldopen_fdx::write(&Doc {
        blocks: blks,
        format: &doc.format,
        title_lines: &doc.title_lines,
        kept: Some(&doc.kept),
        declare_utf8: false,
    });
    if !written.losses.is_empty() {
        return Err(format!("G3: the edits report losses: {:?}", written.losses));
    }
    let again = read(written.text.as_bytes()).map_err(|e| format!("G3: {e}"))?;
    if again.repaired() {
        return Err("G3: the edited file needs repair".into());
    }
    let want: Vec<Element> = blocks.iter().map(|b| b.1.clone()).collect();
    let spacing: BTreeMap<usize, Pt> = blocks
        .iter()
        .enumerate()
        .filter_map(|(k, b)| b.2.map(|p| (k, p)))
        .collect();
    if again.script.elements != want || again.overrides != spacing {
        return Err("G3: the edited file reads back as another document".into());
    }
    let (text, _, _) = decode(bytes);
    let (outside_o, ch_o) =
        coldopen_fdx::fuzzing::content_children(&text).ok_or("G3: cannot split the file")?;
    let (outside_w, ch_w) = coldopen_fdx::fuzzing::content_children(&written.text)
        .ok_or("G3: cannot split what was written")?;
    if outside_o != outside_w {
        return Err(format!(
            "G3: something outside Content changed, at {}",
            first_difference(outside_o.as_bytes(), outside_w.as_bytes())
        ));
    }
    fn count(v: &[String]) -> BTreeMap<&str, i64> {
        let mut m: BTreeMap<&str, i64> = BTreeMap::new();
        for c in v {
            *m.entry(c.as_str()).or_default() += 1;
        }
        m
    }
    let (o, w) = (count(&ch_o), count(&ch_w));
    let only = |a: &BTreeMap<&str, i64>, b: &BTreeMap<&str, i64>| -> i64 {
        a.iter()
            .map(|(k, n)| (n - b.get(k).copied().unwrap_or(0)).max(0))
            .sum()
    };
    let (gone, new) = (only(&o, &w), only(&w, &o));
    if gone > 2 || new > 2 {
        return Err(format!(
            "G3: {gone} paragraphs changed or went and {new} are new; at most 2 each was expected"
        ));
    }
    Ok(())
}

fn run(files: &[PathBuf], base: &Path, report: &mut String) -> Vec<String> {
    let mut failures = Vec::new();
    for f in files {
        let name = f.strip_prefix(base).unwrap_or(f).display().to_string();
        match check(f) {
            Ok(o) => {
                let _ = writeln!(
                    report,
                    "| {name} | {} | {} | {} | {} | {} |",
                    if o.identical {
                        "identical"
                    } else {
                        "differs (repaired)"
                    },
                    if o.repaired { "yes" } else { "no" },
                    o.kept,
                    o.store,
                    o.ms
                );
            }
            Err(why) => {
                let _ = writeln!(report, "| {name} | FAILED | | | | |");
                failures.push(format!("{name}: {why}"));
            }
        }
    }
    failures
}

const HEADER: &str = "| File | Bytes | Repaired | Kept paragraphs | Store (JSON bytes) | ms |\n| --- | --- | --- | --- | --- | --- |\n";

#[test]
fn our_fixtures_round_trip() {
    let mut files = Vec::new();
    fdx_files(&fixtures_dir(), &mut files);
    assert!(files.len() >= 20, "fixtures missing");
    let mut report = String::from(HEADER);
    let failures = run(&files, &fixtures_dir(), &mut report);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn the_corpus_round_trips() {
    let dir = corpus_dir();
    let mut files = Vec::new();
    fdx_files(&dir, &mut files);
    if files.is_empty() {
        let require = std::env::var_os("COLDOPEN_REQUIRE_CORPUS").is_some_and(|v| v == "1");
        let banner = format!(
            "\n==== ROUND-TRIP GATE NOT RUN: no .fdx files under {} ====\n",
            dir.display()
        );
        let _ = std::io::stderr().write_all(banner.as_bytes());
        assert!(!require, "COLDOPEN_REQUIRE_CORPUS=1 and no corpus");
        return;
    }
    let mut report = String::from("# Round-trip gate (docs/specs/fdx.md §16)\n\n");
    report.push_str(HEADER);
    let failures = run(&files, &dir, &mut report);
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("coldopen-roundtrip");
    let _ = std::fs::create_dir_all(&out);
    let _ = std::fs::write(out.join("report.md"), &report);
    let _ = std::io::stderr().write_all(report.as_bytes());
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
