//! The tool list (docs/specs/mcp.md §3): one tool per command, in a fixed
//! order, with descriptions in our own words and input schemas generated
//! from the command param types.

use std::sync::Arc;

use coldopen_core::Command;
use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{Value, json};

/// (command, description), in the order the tools are listed.
pub const TOOLS: &[(&str, &str)] = &[
    (
        "doc.new",
        "Start a new, empty script. With `path`, it is journaled next to that path from the first change.",
    ),
    (
        "doc.open",
        "Open a .fountain, .spmd or .txt script. If the Cold Open app has it open, attach to the app instead, so edits appear live for the writer (`attach`: \"auto\" or \"never\").",
    ),
    (
        "doc.save",
        "Save the script to its file, or to `path` (not allowed while attached to the app).",
    ),
    (
        "doc.close",
        "Close the script, or detach from the app and leave the writer's script alone. Refuses unsaved changes unless `discard` is true.",
    ),
    (
        "doc.revert",
        "Throw away unsaved changes and read the file again (not allowed while attached).",
    ),
    (
        "doc.get",
        "The title page and every block with its id, type, text and attributes, plus `rev`, a change counter.",
    ),
    (
        "doc.export",
        "The script as text without saving it: `fountain` (default), `json`, or `pages` (the plain-text page view).",
    ),
    (
        "scene.list",
        "The scenes in order: heading, number, page, length in eighths of a page, and the heading block's id.",
    ),
    (
        "text.find",
        "Find text in the printed text of blocks; returns each match as a block id and character offsets.",
    ),
    (
        "text.complete",
        "Suggestions for finishing the text before an offset in a block, as the editor's autocomplete gives them.",
    ),
    (
        "history.list",
        "The undo and redo steps, newest first, with each step's label and author.",
    ),
    (
        "block.insert",
        "Insert a block of a given type after or before a block id, or at the end. Returns the new block's id.",
    ),
    ("block.set_type", "Change a block's element type."),
    ("block.set_text", "Replace a block's whole text."),
    (
        "block.set_attrs",
        "Set a block's attributes: scene number, dual dialogue, centered, section depth, space before.",
    ),
    ("block.delete", "Delete a block."),
    (
        "block.split",
        "Split a block at a character offset; the second part becomes a new block of the given type (or the same type).",
    ),
    (
        "block.join",
        "Join a block onto the end of the block before it, keeping the earlier block's type.",
    ),
    (
        "text.insert",
        "Insert text at a character offset in a block, optionally bold, italic or underlined.",
    ),
    (
        "text.delete",
        "Delete the characters between two offsets in a block. Notes and boneyard are never deleted.",
    ),
    (
        "text.style",
        "Turn bold, italic or underline on or off between two offsets in a block.",
    ),
    (
        "text.replace",
        "Replace text everywhere, in one scene, or at one match (`at`), as one undo step.",
    ),
    (
        "scene.move",
        "Move a scene, heading and all, before another scene's heading, or to the end.",
    ),
    (
        "batch",
        "Run several edits as one undo step: all of them apply, or none do.",
    ),
    (
        "undo",
        "Undo this agent's newest step. Fails with not_your_step when the newest step is someone else's.",
    ),
    ("redo", "Redo this agent's step that was undone last."),
    (
        "export.pdf",
        "Write the script as a PDF to `path`, with optional scene numbers and watermark. The script is not changed.",
    ),
    (
        "export.fdx",
        "Write a copy of the script as an .fdx file to a new `path`. The open script keeps its own file and format.",
    ),
    (
        "export.fountain",
        "Write a copy of the script as a Fountain file to a new `path`. The open script keeps its own file and format.",
    ),
    (
        "recovered.list",
        "List the copies Cold Open set aside for a script (unsaved changes kept when they were discarded, journals that could not be replayed, damaged originals).",
    ),
    (
        "recovered.restore",
        "Restore a set-aside copy as a new script. Only the writer may; an agent gets writer_only.",
    ),
    (
        "recovered.delete",
        "Delete a set-aside copy. Only the writer may; an agent gets writer_only.",
    ),
    (
        "file.repair",
        "Read the .fdx file at `path`, repairing what it can, and write the repaired copy to a new path `out`; returns the repair report. `path` is never written.",
    ),
];

/// A command's tool name: dots become underscores.
pub fn tool_name(command: &str) -> String {
    command.replace('.', "_")
}

/// The command for a tool name.
pub fn command_for(tool: &str) -> Option<&'static str> {
    TOOLS.iter().map(|(c, _)| *c).find(|c| tool_name(c) == tool)
}

fn read_only(command: &str) -> bool {
    matches!(
        command,
        "doc.get" | "doc.export" | "scene.list" | "text.find" | "text.complete" | "history.list"
    )
}

/// Every tool, in order.
pub fn tools() -> Vec<Tool> {
    TOOLS
        .iter()
        .filter_map(|(command, description)| {
            let mut schema = Command::params_schema(command)?;
            if *command == "doc.open"
                && let Some(props) = schema["properties"].as_object_mut()
            {
                props.insert(
                    "attach".into(),
                    json!({
                        "description": "\"auto\" (the default) attaches to the Cold Open app when it has the script open; \"never\" always opens the file here.",
                        "type": "string",
                        "enum": ["auto", "never"]
                    }),
                );
            }
            let Value::Object(obj) = schema else {
                return None;
            };
            let mut tool = Tool::new(tool_name(command), *description, Arc::new(obj));
            let mut a = ToolAnnotations::default();
            a.read_only_hint = Some(read_only(command));
            a.destructive_hint = Some(matches!(
                *command,
                "doc.close" | "doc.revert" | "block.delete" | "block.set_text"
            ));
            tool.annotations = Some(a);
            Some(tool)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_a_tool_once_with_a_schema() {
        let tools = tools();
        assert_eq!(tools.len(), Command::NAMES.len());
        for name in Command::NAMES {
            let t = tool_name(name);
            assert_eq!(tools.iter().filter(|x| x.name == t).count(), 1, "{name}");
            assert_eq!(command_for(&t), Some(name));
            // The Claude API's rule for tool names.
            assert!(t.len() <= 64);
            assert!(
                t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            );
        }
        let open = tools.iter().find(|t| t.name == "doc_open").unwrap();
        assert!(open.input_schema["properties"]["attach"].is_object());
        assert_eq!(command_for("nope"), None);
    }
}
