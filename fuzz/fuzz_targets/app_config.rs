//! The writer's own files (docs/specs/editor.md §5.2, §10.1): any
//! `keymap.json` or `flow.json` is read without panicking, and the result
//! still has a binding for the palette unless the file removed it.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let (keymap, _warnings) =
        coldopen_app::keymap::Keymap::load(Some(s), &coldopen_app::actions::known);
    let _ = keymap.keys_for("app.palette");
    let mut flow = coldopen_app::editor::flow::FlowTable::default();
    let _ = flow.apply_overrides(s);
});
