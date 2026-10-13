//! Crash journal reading and replay: arbitrary bytes never panic.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_core::fuzzing::journal_replay(data) {
        panic!("{e}");
    }
});
