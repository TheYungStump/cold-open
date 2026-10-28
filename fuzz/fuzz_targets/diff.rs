//! See coldopen_core::fuzzing::diff: two drafts from arbitrary bytes are
//! compared without a panic, each against itself is empty, the counts match
//! the lists, and bringing one into the other accepts to it and rejects
//! back (docs/specs/diff.md §7).
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_core::fuzzing::diff(data) {
        panic!("{e}");
    }
});
