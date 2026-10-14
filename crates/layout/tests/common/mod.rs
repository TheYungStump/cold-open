//! Golden-file checks shared by the layout tests.
//!
//! `COLDOPEN_BLESS=1` rewrites goldens that differ. Blessing is refused when
//! the `CI` variable is set, so CI can only compare. On a mismatch the actual
//! output is written under Cargo's per-test temp directory, never next to
//! the golden.
#![allow(dead_code, clippy::disallowed_methods)]

pub mod pdf_read;

use std::fs;
use std::path::{Path, PathBuf};

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/layout")
}

fn blessing() -> bool {
    match std::env::var("COLDOPEN_BLESS").as_deref() {
        Ok("1") => {
            assert!(
                std::env::var_os("CI").is_none(),
                "refusing to bless golden files under CI"
            );
            true
        }
        Ok("0") | Ok("") | Err(_) => false,
        Ok(other) => panic!("COLDOPEN_BLESS must be 1 or 0, not {other:?}"),
    }
}

/// Compares `actual` with the golden file at `path`; blesses when asked.
pub fn assert_golden(path: &Path, actual: &[u8]) {
    let expected = fs::read(path).ok();
    if expected.as_deref() == Some(actual) {
        return;
    }
    if blessing() {
        fs::write(path, actual).unwrap();
        eprintln!("blessed {}", path.display());
        return;
    }
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden");
    fs::create_dir_all(&out).unwrap();
    let actual_path = out.join(&name);
    fs::write(&actual_path, actual).unwrap();
    let Some(expected) = expected else {
        panic!(
            "missing golden {name}; the output is in {}. Review it, then run with COLDOPEN_BLESS=1 to create it.",
            actual_path.display()
        );
    };
    let (e, a) = (
        String::from_utf8_lossy(&expected),
        String::from_utf8_lossy(actual),
    );
    let line = e
        .lines()
        .zip(a.lines())
        .position(|(x, y)| x != y)
        .unwrap_or_else(|| e.lines().count().min(a.lines().count()));
    panic!(
        "{name} differs from its golden at line {}:\n  golden: {:?}\n  actual: {:?}\nthe actual output is in {}",
        line + 1,
        e.lines().nth(line).unwrap_or("<end>"),
        a.lines().nth(line).unwrap_or("<end>"),
        actual_path.display()
    );
}

/// Applies a JSON merge patch (RFC 7386) to `base`.
pub fn merge_patch(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(b), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    b.remove(k);
                } else {
                    merge_patch(b.entry(k.clone()).or_insert(serde_json::Value::Null), v);
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// A readable dump of a PDF written by us: its objects as text, with Flate
/// streams decompressed and font programs summarized. Used for reviewing
/// PDF golden changes; the bytes themselves are the golden.
pub fn pdf_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let Some(rel) = find_stream(&bytes[i..]) else {
            out.push_str(&printable(&bytes[i..]));
            break;
        };
        let at = i + rel;
        // Text up to and including the "stream" keyword.
        out.push_str(&printable(&bytes[i..at + 7]));
        let dict_start = rfind(&bytes[..at], b"<<").unwrap_or(i);
        let dict = String::from_utf8_lossy(&bytes[dict_start..at]).into_owned();
        let len: usize = dict
            .split("/Length ")
            .nth(1)
            .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let data = &bytes[at + 7..(at + 7 + len).min(bytes.len())];
        let inflated = if dict.contains("/FlateDecode") {
            miniz_oxide::inflate::decompress_to_vec_zlib(data).unwrap_or_else(|_| data.to_vec())
        } else {
            data.to_vec()
        };
        if dict.contains("/Length1") {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for &b in &inflated {
                h ^= u64::from(b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            out.push_str(&format!(
                "[font program: {} bytes, fnv64 {h:016x}]",
                inflated.len()
            ));
        } else {
            out.push_str(&printable(&inflated));
        }
        i = at + 7 + len;
    }
    out
}

/// The next `stream` keyword that starts a stream (not the end of an
/// `endstream`).
fn find_stream(hay: &[u8]) -> Option<usize> {
    let needle = b"stream\n";
    (0..hay.len().saturating_sub(needle.len() - 1))
        .find(|&k| &hay[k..k + needle.len()] == needle && !(k >= 3 && &hay[k - 3..k] == b"end"))
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

fn printable(b: &[u8]) -> String {
    let mut s = String::new();
    for &c in b {
        match c {
            b'\n' | b'\t' | 0x20..=0x7e => s.push(char::from(c)),
            _ => s.push_str(&format!("\\x{c:02x}")),
        }
    }
    s
}
