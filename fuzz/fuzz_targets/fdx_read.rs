//! .fdx reader: never panic, read the same twice, and produce a document
//! that satisfies the model rules (docs/specs/fdx.md §10). Whatever it
//! accepts must also lay out and export as a PDF without a panic.
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Err(e) = coldopen_fdx::fuzzing::read(data) {
        panic!("{e}");
    }
    if let Ok(doc) = coldopen_fdx::read(data) {
        let format = coldopen_layout::ValidFormat::new(doc.format).expect("the reader returns valid formats");
        let options = coldopen_layout::Options {
            overrides: doc.overrides,
            title_lines: Some(doc.title_lines),
            ..Default::default()
        };
        let layout = coldopen_layout::layout(&doc.script, &format, &options);
        let fonts = coldopen_layout::Fonts::new().expect("bundled fonts");
        coldopen_layout::pdf::write(&layout, &fonts, &Default::default()).expect("pdf");
    }
});
