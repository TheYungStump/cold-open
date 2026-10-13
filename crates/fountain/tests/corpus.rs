//! Round trips over the private corpus of real scripts.
//!
//! The corpus is copyrighted and never committed (see CLAUDE.md). This test
//! skips cleanly when `corpus/` is missing. Set `COLDOPEN_CORPUS` to use
//! another directory.

use std::path::{Path, PathBuf};
use std::time::Instant;

use coldopen_fountain::{normalize, parse, write};

fn corpus_dir() -> PathBuf {
    match std::env::var_os("COLDOPEN_CORPUS") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
    }
}

fn fountain_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            fountain_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "fountain") {
            out.push(p);
        }
    }
}

#[test]
fn corpus_round_trips() {
    let dir = corpus_dir();
    let mut files = Vec::new();
    fountain_files(&dir, &mut files);
    if files.is_empty() {
        eprintln!("skipping: no .fountain files under {}", dir.display());
        return;
    }
    let mut failures = Vec::new();
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let start = Instant::now();
        let first = parse(&text).script;
        let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let w = write(&first);
        let write_ms = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "{}: {} elements, parse {parse_ms:.1} ms, write {write_ms:.1} ms",
            f.display(),
            first.elements.len()
        );
        if normalize(&first) != first {
            failures.push(format!(
                "{}: parser output is not in normal form",
                f.display()
            ));
        }
        if !w.losses.is_empty() {
            failures.push(format!("{}: losses {:?}", f.display(), w.losses));
        }
        let second = parse(&w.text).script;
        if second != first {
            let at = first
                .elements
                .iter()
                .zip(&second.elements)
                .position(|(a, b)| a != b);
            failures.push(format!(
                "{}: does not read back the same (first difference at element {at:?})",
                f.display()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
