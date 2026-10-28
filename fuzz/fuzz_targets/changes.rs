//! See coldopen_core::fuzzing::changes and settle_keeps: arbitrary bytes
//! never panic, and the checks' properties hold.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_core::fuzzing::changes(data) {
        panic!("{e}");
    }
    if let Err(e) = coldopen_core::fuzzing::settle_keeps(data) {
        panic!("{e}");
    }
});
