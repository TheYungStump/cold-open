//! .fdx round trip (docs/specs/fdx.md §16.1): whatever the reader accepts,
//! written back unedited, is byte for byte the input when nothing was
//! repaired, reads back as the same document with nothing to repair, and
//! writes the same bytes again.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_fdx::fuzzing::roundtrip(data) {
        panic!("{e}");
    }
});
