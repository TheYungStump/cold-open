//! .fdx writer (docs/specs/fdx.md §14 to §16.2): a document made from the
//! input, written as a new file, reads back with nothing to repair as its
//! .fdx normal form, less what the writer listed as lost.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let (elements, spacing) = coldopen_fdx::fuzzing::document_from_bytes(data);
    if let Err(e) = coldopen_fdx::fuzzing::write_document(&elements, &spacing) {
        panic!("{e}");
    }
});
