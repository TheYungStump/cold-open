//! Fountain parser and writer: never panic, and round-trip exactly.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_fountain::fuzzing::round_trip_bytes(data) {
        panic!("{e}");
    }
});
