//! The agent link's parsers (docs/specs/mcp.md §5): the handshake and the
//! app's reply never panic, and the bounded line reader never returns a
//! line over its limit.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = coldopen_link::parse_hello(s);
        let _ = coldopen_link::parse_reply(s);
        let _ = coldopen_link::clean_name(s);
        let _ = coldopen_link::refused(s, &serde_json::Value::Null);
    }
    let mut r = coldopen_link::LineReader::new(data);
    while let Ok(Some(line)) = r.read_line(64) {
        assert!(line.len() <= 64);
    }
});
