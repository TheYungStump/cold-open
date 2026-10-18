//! See coldopen_core::fuzzing::bundle_read: arbitrary bytes never panic, and the
//! check's properties hold.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_core::fuzzing::bundle_read(data) {
        panic!("{e}");
    }
});
