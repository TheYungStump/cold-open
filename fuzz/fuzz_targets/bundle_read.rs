//! See coldopen_core::fuzzing::bundle_read: arbitrary bytes never panic, and the
//! check's properties hold. The same bytes also build a crafted bundle
//! (fuzzing::bundle_hostile): absurd mark ids, false sizes, a newer version.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_core::fuzzing::bundle_read(data) {
        panic!("{e}");
    }
    if let Err(e) = coldopen_core::fuzzing::bundle_hostile(data) {
        panic!("{e}");
    }
});
