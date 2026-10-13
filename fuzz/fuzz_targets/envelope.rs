//! The JSON command envelope: every line gets one well-formed response.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_core::fuzzing::envelope(data) {
        panic!("{e}");
    }
});
