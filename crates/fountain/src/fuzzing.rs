//! Checks shared by the cargo-fuzz targets in `fuzz/` and the stable test
//! suite, so the fuzzed properties also run on every CI build.

use crate::{normalize, parse, write};

/// Emphasis built from packed delimiters may have no writable form
/// (docs/specs/fountain.md §9); only that may be reported as a loss.
fn has_packed_delimiters(s: &str) -> bool {
    let c: Vec<char> = s.chars().collect();
    c.windows(4)
        .any(|w| w.iter().all(|&x| x == '*' || x == '_'))
}

/// Parses `text` and checks the round-trip guarantees of spec §1:
/// the parse is in normal form, writing it loses nothing, the written text
/// reads back the same, and writing again changes nothing.
pub fn round_trip(text: &str) -> Result<(), String> {
    let first = parse(text).script;
    if normalize(&first) != first {
        return Err("parser output is not in normal form".into());
    }
    let w = write(&first);
    if !w.losses.is_empty() {
        let tolerated = w.losses.iter().all(|l| l.code == "style") && has_packed_delimiters(text);
        if !tolerated {
            return Err(format!("unexpected losses: {:?}", w.losses));
        }
        return Ok(());
    }
    let second = parse(&w.text).script;
    if second != first {
        return Err(format!("written text reads back differently:\n{}", w.text));
    }
    if write(&second).text != w.text {
        return Err("writing is not idempotent".into());
    }
    Ok(())
}

/// Like [`round_trip`], for arbitrary bytes.
pub fn round_trip_bytes(data: &[u8]) -> Result<(), String> {
    round_trip(&String::from_utf8_lossy(data))
}
