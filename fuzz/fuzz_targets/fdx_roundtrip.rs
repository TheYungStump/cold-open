//! .fdx round trip (docs/specs/fdx.md §16.1): whatever the reader accepts,
//! written back unedited, is byte for byte the input when nothing was
//! repaired, reads back as the same document with nothing to repair, and
//! writes the same bytes again. Its title lines, edited as the input's last
//! bytes choose, are written back with the lines left alone as the file had
//! them (§13.5).
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_fdx::fuzzing::roundtrip(data) {
        panic!("{e}");
    }
    let ops = &data[data.len().saturating_sub(8)..];
    if let Err(e) = coldopen_fdx::fuzzing::title_edit(data, ops) {
        panic!("{e}");
    }
});
