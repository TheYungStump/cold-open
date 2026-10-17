//! Repair (docs/specs/fdx.md §17) on our broken fixtures: what the reader
//! repaired is written back as `<case>.repaired.out`, byte for byte, and
//! that output reads with nothing left to repair.
//!
//! `COLDOPEN_BLESS=1` rewrites the `.repaired.out` files, as for the
//! fixtures' expectations; review the diff before committing.

use std::fs;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx")
}

#[test]
fn repaired_fixtures_write_back_as_expected() {
    let bless =
        std::env::var("COLDOPEN_BLESS").is_ok_and(|v| v == "1") && std::env::var("CI").is_err();
    let mut names: Vec<PathBuf> = fs::read_dir(fixtures())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "fdx"))
        .collect();
    names.sort();
    let mut repaired = 0;
    for path in names {
        let doc = coldopen_fdx::read(&fs::read(&path).unwrap()).unwrap();
        let out_path = path.with_extension("repaired.out");
        if !doc.repaired() {
            assert!(!out_path.exists(), "{} is not repaired", path.display());
            continue;
        }
        repaired += 1;
        let out = coldopen_fdx::encode(&doc.write_back().text, doc.encoding);
        let again = coldopen_fdx::read(&out).unwrap();
        assert!(
            !again.repaired(),
            "{}: the repaired file still needs repair: {:?}",
            path.display(),
            again.report
        );
        if bless {
            fs::write(&out_path, &out).unwrap();
            continue;
        }
        let want = fs::read(&out_path)
            .unwrap_or_else(|_| panic!("{} is missing; bless it", out_path.display()));
        assert_eq!(
            String::from_utf8_lossy(&out),
            String::from_utf8_lossy(&want),
            "{}",
            path.display()
        );
    }
    assert!(repaired >= 4, "only {repaired} repaired fixtures");
}
