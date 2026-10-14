//! Runs on the private reference corpus when it is present. `corpus/` is
//! gitignored, so this test skips cleanly in CI and in fresh checkouts.
//! Set REFPDF_CORPUS to a directory of PDFs to point it elsewhere.
//! Assertion messages name files only, never script text.

use std::path::{Path, PathBuf};

use refpdf::extract::extract_pdf;
use refpdf::reffile;

fn pdfs(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let ft = e.file_type().unwrap();
        if ft.is_dir() {
            pdfs(&e.path(), out);
        } else if ft.is_file()
            && e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("pdf"))
        {
            out.push(e.path());
        }
    }
}

#[test]
fn reference_pdfs_extract_without_failures() {
    let dir = std::env::var_os("REFPDF_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fd11"));
    if !dir.is_dir() {
        eprintln!("skipping: no reference corpus at {}", dir.display());
        return;
    }
    let mut files = Vec::new();
    pdfs(&dir, &mut files);
    for f in files {
        let name = f.display();
        let bytes = std::fs::read(&f).unwrap();
        let doc = extract_pdf(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let an = reffile::analyze(&doc);
        let r = reffile::build(reffile::sha256_hex(&bytes), &doc, &an);
        let failed: Vec<u32> = r
            .pages
            .iter()
            .filter(|p| p.error.is_some())
            .map(|p| p.index)
            .collect();
        assert!(failed.is_empty(), "{name}: failed pages {failed:?}");
        let pitch = r.pitch_x.unwrap_or_else(|| panic!("{name}: no pitch"));
        assert!((650..=750).contains(&pitch), "{name}: pitch {pitch}");
        for c in reffile::checks(&r, &an) {
            assert_ne!(c.level, reffile::Level::Fail, "{name}: {}", c.msg);
        }
    }
}
