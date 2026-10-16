//! Scripts in encodings other than UTF-8 (docs/specs/journal.md,
//! "Encoding"; review finding F6).

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

fn utf16(text: &str, big_endian: bool, bom: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if bom {
        out.extend_from_slice(if big_endian {
            &[0xFE, 0xFF]
        } else {
            &[0xFF, 0xFE]
        });
    }
    for unit in text.encode_utf16() {
        out.extend_from_slice(&if big_endian {
            unit.to_be_bytes()
        } else {
            unit.to_le_bytes()
        });
    }
    out
}

/// Review finding F6: a UTF-16 file (what Windows PowerShell 5.1's `>` and
/// Notepad's old "Unicode" write) was read byte by byte as Windows-1252,
/// opened as one block of mojibake, and the next save wrote that mojibake
/// back as UTF-8 with no loss reported.
#[test]
fn utf16_scripts_open_as_text_and_save_as_utf16() {
    let script = "INT. CAFÉ - DAY\r\n\r\nZoë waits.\r\n";
    for (big_endian, bom) in [(false, true), (true, true), (false, false), (true, false)] {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cafe.fountain");
        fs::write(&p, utf16(script, big_endian, bom)).unwrap();
        let mut d = Dispatcher::new(Options::default());
        let r = ok(&mut d, "doc.open", json!({"path": p}));
        assert_eq!(r["scenes"], 1, "{r}");
        assert!(
            r["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .all(|w| w["code"] != "not_utf8"),
            "{r}"
        );
        let blocks = ok(&mut d, "doc.get", json!({}))["blocks"].clone();
        assert_eq!(blocks[0]["text"], "INT. CAFÉ - DAY");
        assert_eq!(blocks[1]["text"], "Zoë waits.");
        ok(
            &mut d,
            "block.insert",
            json!({"type": "action", "text": "Rain."}),
        );
        let saved = ok(&mut d, "doc.save", json!({}));
        assert_eq!(saved["losses"], json!([]), "{saved}");
        // Kept as it was found: UTF-16 in the same byte order, its mark if
        // it had one, and CRLF.
        let want = utf16(
            "INT. CAFÉ - DAY\r\n\r\nZoë waits.\r\n\r\nRain.\r\n",
            big_endian,
            bom,
        );
        assert_eq!(
            fs::read(&p).unwrap(),
            want,
            "big endian {big_endian}, mark {bom}"
        );
        d.shutdown();
    }
}

/// A file read with the Windows-1252 fallback is written back as UTF-8,
/// which other programs then show wrongly; the save now says so first.
#[test]
fn a_windows_1252_script_is_not_rewritten_as_utf8_silently() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("cafe.fountain");
    let bytes = b"INT. CAF\xC9 - DAY\n\nZo\xEB waits.\n".to_vec();
    fs::write(&p, &bytes).unwrap();
    let mut d = Dispatcher::new(Options::default());
    let r = ok(&mut d, "doc.open", json!({"path": p}));
    assert_eq!(r["warnings"][0]["code"], "not_utf8", "{r}");
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "Rain."}),
    );
    let e = run(&mut d, "doc.save", json!({})).unwrap_err();
    assert_eq!(e.code, "lossy_format", "{e}");
    assert_eq!(
        e.details.as_ref().unwrap()["losses"][0]["code"],
        "reencoded",
        "{e:?}"
    );
    assert_eq!(fs::read(&p).unwrap(), bytes);
    let saved = ok(&mut d, "doc.save", json!({"allow_lossy": true}));
    assert_eq!(saved["losses"][0]["code"], "reencoded");
    assert_eq!(
        fs::read_to_string(&p).unwrap(),
        "INT. CAFÉ - DAY\n\nZoë waits.\n\nRain.\n"
    );
    // Once it is UTF-8, saving asks nothing.
    ok(
        &mut d,
        "block.insert",
        json!({"type": "action", "text": "Wind."}),
    );
    assert_eq!(ok(&mut d, "doc.save", json!({}))["losses"], json!([]));
    d.shutdown();
}
