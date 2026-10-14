//! Reads every .fdx file in the private corpus of public sample files.
//!
//! The corpus is never committed (see CLAUDE.md). This test skips cleanly
//! when `corpus/` is missing; set `COLDOPEN_CORPUS` to use another
//! directory. It prints counts and report codes, never script text.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use coldopen_fdx::read;
use coldopen_screenplay::ElementKind;

fn corpus_dir() -> PathBuf {
    match std::env::var_os("COLDOPEN_CORPUS") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
    }
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

#[test]
fn corpus_reads() {
    let dir = corpus_dir();
    let mut files = Vec::new();
    fdx_files(&dir, &mut files);
    if files.is_empty() {
        eprintln!("skipping: no .fdx files under {}", dir.display());
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        let name = f.strip_prefix(&dir).unwrap_or(f).display().to_string();
        let bytes = std::fs::read(f).unwrap();
        let start = Instant::now();
        let result = read(&bytes);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let doc = match result {
            Ok(doc) => doc,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let mut kinds: BTreeMap<ElementKind, usize> = BTreeMap::new();
        for e in &doc.script.elements {
            *kinds.entry(e.kind).or_default() += 1;
        }
        let dual = doc.script.elements.iter().filter(|e| e.dual).count();
        let codes: Vec<&str> = doc.report.iter().map(|i| i.code).collect();
        eprintln!(
            "{name}: {} elements {kinds:?}, {dual} dual, {} overrides, {} title lines, {} scenes, read in {ms:.1} ms\n  report: {codes:?}",
            doc.script.elements.len(),
            doc.overrides.len(),
            doc.title_lines.len(),
            doc.scenes.len(),
        );
        if doc.script.elements.is_empty() {
            failures.push(format!("{name}: no elements"));
        }
        let problems = doc.format.problems();
        if !problems.is_empty() {
            failures.push(format!("{name}: invalid format {problems:?}"));
        }
        if let Err(e) = coldopen_fdx::fuzzing::read(&bytes) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
