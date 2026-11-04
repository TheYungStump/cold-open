//! Crash report scrubbing: no panic (a panic in the hook aborts), never
//! over its cap, and no quoted string survives it.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_core::fuzzing::crash_scrub(data) {
        panic!("{e}");
    }
});
