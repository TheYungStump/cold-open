//! The `Command` enum: every action Cold Open can take, with stable dotted
//! names (docs/specs/commands.md).

use coldopen_screenplay::{ElementKind, ElementRepr, Run, Style};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::document::BlockId;

/// Every command, as sent by any front end. The serde form is the JSON
/// envelope's `command` and `params`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", content = "params")]
pub enum Command {
    #[serde(rename = "doc.new")]
    DocNew(DocNew),
    #[serde(rename = "doc.open")]
    DocOpen(DocOpen),
    #[serde(rename = "doc.save")]
    DocSave(DocSave),
    #[serde(rename = "doc.close")]
    DocClose(DocClose),
    #[serde(rename = "doc.get")]
    DocGet(Empty),
    #[serde(rename = "doc.export")]
    DocExport(DocExport),
    #[serde(rename = "block.insert")]
    BlockInsert(BlockInsert),
    #[serde(rename = "block.set_type")]
    BlockSetType(BlockSetType),
    #[serde(rename = "block.set_text")]
    BlockSetText(BlockSetText),
    #[serde(rename = "block.set_attrs")]
    BlockSetAttrs(BlockSetAttrs),
    #[serde(rename = "block.delete")]
    BlockDelete(BlockDelete),
    #[serde(rename = "text.replace")]
    TextReplace(TextReplace),
    #[serde(rename = "scene.list")]
    SceneList(Empty),
    #[serde(rename = "scene.move")]
    SceneMove(SceneMove),
    #[serde(rename = "export.pdf")]
    ExportPdf(ExportPdf),
    #[serde(rename = "undo")]
    Undo(Empty),
    #[serde(rename = "redo")]
    Redo(Empty),
    #[serde(rename = "doc.revert")]
    DocRevert(Empty),
    #[serde(rename = "text.insert")]
    TextInsert(TextInsert),
    #[serde(rename = "text.delete")]
    TextDelete(TextDelete),
    #[serde(rename = "text.style")]
    TextStyle(TextStyle),
    #[serde(rename = "block.split")]
    BlockSplit(BlockSplit),
    #[serde(rename = "block.join")]
    BlockJoin(BlockJoin),
    #[serde(rename = "batch")]
    Batch(Batch),
    #[serde(rename = "text.find")]
    TextFind(TextFind),
    #[serde(rename = "text.complete")]
    TextComplete(TextComplete),
    #[serde(rename = "history.list")]
    HistoryList(Empty),
    #[serde(rename = "export.fdx")]
    ExportFdx(ExportFile),
    #[serde(rename = "export.fountain")]
    ExportFountain(ExportFile),
    #[serde(rename = "file.repair")]
    FileRepair(FileRepair),
}

/// How a command interacts with undo and the journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Session,
    Query,
    Edit,
    History,
    /// Writes a file from the document without changing it; not undoable
    /// and not journaled.
    Output,
    /// Works on files other than the open document and needs none; not
    /// undoable and not journaled (2026-10-16).
    File,
}

impl Command {
    /// Every command name, in a stable order (for a command palette).
    pub const NAMES: [&'static str; 30] = [
        "doc.new",
        "doc.open",
        "doc.save",
        "doc.close",
        "doc.get",
        "doc.export",
        "block.insert",
        "block.set_type",
        "block.set_text",
        "block.set_attrs",
        "block.delete",
        "text.replace",
        "scene.list",
        "scene.move",
        "export.pdf",
        "undo",
        "redo",
        "doc.revert",
        "text.insert",
        "text.delete",
        "text.style",
        "block.split",
        "block.join",
        "batch",
        "text.find",
        "text.complete",
        "history.list",
        "export.fdx",
        "export.fountain",
        "file.repair",
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Command::DocNew(_) => "doc.new",
            Command::DocOpen(_) => "doc.open",
            Command::DocSave(_) => "doc.save",
            Command::DocClose(_) => "doc.close",
            Command::DocGet(_) => "doc.get",
            Command::DocExport(_) => "doc.export",
            Command::BlockInsert(_) => "block.insert",
            Command::BlockSetType(_) => "block.set_type",
            Command::BlockSetText(_) => "block.set_text",
            Command::BlockSetAttrs(_) => "block.set_attrs",
            Command::BlockDelete(_) => "block.delete",
            Command::TextReplace(_) => "text.replace",
            Command::SceneList(_) => "scene.list",
            Command::SceneMove(_) => "scene.move",
            Command::ExportPdf(_) => "export.pdf",
            Command::Undo(_) => "undo",
            Command::Redo(_) => "redo",
            Command::DocRevert(_) => "doc.revert",
            Command::TextInsert(_) => "text.insert",
            Command::TextDelete(_) => "text.delete",
            Command::TextStyle(_) => "text.style",
            Command::BlockSplit(_) => "block.split",
            Command::BlockJoin(_) => "block.join",
            Command::Batch(_) => "batch",
            Command::TextFind(_) => "text.find",
            Command::TextComplete(_) => "text.complete",
            Command::HistoryList(_) => "history.list",
            Command::ExportFdx(_) => "export.fdx",
            Command::ExportFountain(_) => "export.fountain",
            Command::FileRepair(_) => "file.repair",
        }
    }

    pub fn class(&self) -> Class {
        match self {
            Command::DocNew(_)
            | Command::DocOpen(_)
            | Command::DocSave(_)
            | Command::DocClose(_)
            | Command::DocRevert(_) => Class::Session,
            Command::DocGet(_)
            | Command::DocExport(_)
            | Command::SceneList(_)
            | Command::TextFind(_)
            | Command::TextComplete(_)
            | Command::HistoryList(_) => Class::Query,
            Command::Undo(_) | Command::Redo(_) => Class::History,
            Command::ExportPdf(_) | Command::ExportFdx(_) | Command::ExportFountain(_) => {
                Class::Output
            }
            Command::FileRepair(_) => Class::File,
            // Listed one by one so a new command cannot fall into a class
            // by accident.
            Command::BlockInsert(_)
            | Command::BlockSetType(_)
            | Command::BlockSetText(_)
            | Command::BlockSetAttrs(_)
            | Command::BlockDelete(_)
            | Command::TextReplace(_)
            | Command::SceneMove(_)
            | Command::TextInsert(_)
            | Command::TextDelete(_)
            | Command::TextStyle(_)
            | Command::BlockSplit(_)
            | Command::BlockJoin(_)
            | Command::Batch(_) => Class::Edit,
        }
    }

    /// Builds a command from its name and params, as found in an envelope.
    pub fn from_parts(name: &str, params: serde_json::Value) -> Result<Command, String> {
        let v = serde_json::json!({ "command": name, "params": params });
        serde_json::from_value(v).map_err(|e| e.to_string())
    }

    /// The params as JSON, as written to the journal.
    pub fn params(&self) -> serde_json::Value {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::Object(mut m)) => {
                m.remove("params").unwrap_or_else(|| serde_json::json!({}))
            }
            _ => serde_json::json!({}),
        }
    }
}

/// Params for commands that take none. Accepts `{}`.
/// The JSON Schema of each command's params, with every subschema inline,
/// for MCP tool input schemas (docs/specs/mcp.md §3). Generated from the
/// same types the dispatcher deserializes, so the two cannot drift apart.
#[cfg(feature = "schema")]
impl Command {
    pub fn params_schema(name: &str) -> Option<Value> {
        fn of<T: schemars::JsonSchema>() -> Value {
            let generator = schemars::generate::SchemaSettings::draft2020_12()
                .with(|s| s.inline_subschemas = true)
                .into_generator();
            let mut v = generator.into_root_schema_for::<T>().to_value();
            if let Some(o) = v.as_object_mut() {
                o.remove("$schema");
                o.remove("title");
            }
            v
        }
        Some(match name {
            "doc.new" => of::<DocNew>(),
            "doc.open" => of::<DocOpen>(),
            "doc.save" => of::<DocSave>(),
            "doc.close" => of::<DocClose>(),
            "doc.export" => of::<DocExport>(),
            "block.insert" => of::<BlockInsert>(),
            "block.set_type" => of::<BlockSetType>(),
            "block.set_text" => of::<BlockSetText>(),
            "block.set_attrs" => of::<BlockSetAttrs>(),
            "block.delete" => of::<BlockDelete>(),
            "text.replace" => of::<TextReplace>(),
            "scene.move" => of::<SceneMove>(),
            "export.pdf" => of::<ExportPdf>(),
            "export.fdx" | "export.fountain" => of::<ExportFile>(),
            "file.repair" => of::<FileRepair>(),
            "text.insert" => of::<TextInsert>(),
            "text.delete" => of::<TextDelete>(),
            "text.style" => of::<TextStyle>(),
            "block.split" => of::<BlockSplit>(),
            "block.join" => of::<BlockJoin>(),
            "batch" => of::<Batch>(),
            "text.find" => of::<TextFind>(),
            "text.complete" => of::<TextComplete>(),
            "doc.get" | "scene.list" | "undo" | "redo" | "doc.revert" | "history.list" => {
                of::<Empty>()
            }
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DocNew {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The kind of one empty block the document starts with, so an editor
    /// has somewhere to type. Part of the new document, not an edit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<ElementKind>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Recover {
    #[default]
    Auto,
    Discard,
    /// Recover even a journal written on another computer or for another
    /// path, which `auto` only offers (docs/specs/journal.md, "Header").
    Apply,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DocOpen {
    pub path: String,
    #[serde(default)]
    pub recover: Recover,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DocSave {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default)]
    pub allow_lossy: bool,
    /// Replace the target even though it is a file other than the open
    /// document, or another program changed it since Cold Open last read
    /// or saved it (docs/specs/commands.md). Writers only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub overwrite: bool,
}

fn fountain() -> String {
    "fountain".into()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DocExport {
    #[serde(default = "fountain")]
    pub format: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DocClose {
    #[serde(default)]
    pub discard: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BlockInsert {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<BlockId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<BlockId>,
    #[serde(rename = "type")]
    pub kind: ElementKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<Vec<Run>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_number: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dual: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub centered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_before: Option<u8>,
}

impl BlockInsert {
    /// A plain insert, appended at the end.
    pub fn new(kind: ElementKind, text: impl Into<String>) -> BlockInsert {
        BlockInsert {
            after: None,
            before: None,
            kind,
            text: Some(text.into()),
            runs: None,
            scene_number: None,
            dual: false,
            centered: false,
            depth: None,
            space_before: None,
        }
    }

    pub fn element_repr(&self) -> ElementRepr {
        ElementRepr {
            kind: self.kind,
            text: self.text.clone(),
            runs: self.runs.clone(),
            scene_number: self.scene_number.clone(),
            dual: self.dual,
            centered: self.centered,
            depth: self.depth,
            space_before: self.space_before,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BlockSetType {
    pub id: BlockId,
    #[serde(rename = "type")]
    pub kind: ElementKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BlockSetText {
    pub id: BlockId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<Vec<Run>>,
}

/// Distinguishes an absent field (`None`) from an explicit `null`
/// (`Some(None)`).
fn double_option<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BlockSetAttrs {
    pub id: BlockId,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub scene_number: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centered: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub space_before: Option<Option<u8>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BlockDelete {
    pub id: BlockId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub scene: BlockId,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TextReplace {
    pub find: String,
    pub with: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    #[serde(default = "yes")]
    pub match_case: bool,
    /// Replace only the match starting here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<ReplaceAt>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ReplaceAt {
    pub id: BlockId,
    pub from: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SceneMove {
    pub id: BlockId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<BlockId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExportPdf {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scene_numbers: bool,
    #[serde(default = "yes")]
    pub title_page: bool,
    /// Replace a file already at `path`. Writers only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub overwrite: bool,
}

/// Params for `export.fdx` and `export.fountain`: a copy of the document
/// in that format (docs/specs/commands.md, "Output").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExportFile {
    pub path: String,
    /// Write even though the format cannot hold everything. Writers only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_lossy: bool,
    /// Replace a file already at `path`. Writers only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub overwrite: bool,
}

/// Params for `file.repair` (docs/specs/commands.md, "Files").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FileRepair {
    /// The .fdx file to read. It is never written.
    pub path: String,
    /// Where the repaired copy goes.
    pub out: String,
    /// Replace a file already at `out`. Writers only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub overwrite: bool,
}

/// A style given in full, for `text.insert`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct StyleSpec {
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
}

impl From<StyleSpec> for Style {
    fn from(s: StyleSpec) -> Style {
        Style {
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
        }
    }
}

impl From<Style> for StyleSpec {
    fn from(s: Style) -> StyleSpec {
        StyleSpec {
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TextInsert {
    pub id: BlockId,
    pub at: usize,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TextDelete {
    pub id: BlockId,
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TextStyle {
    pub id: BlockId,
    pub from: usize,
    pub to: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BlockSplit {
    pub id: BlockId,
    pub at: usize,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ElementKind>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BlockJoin {
    pub id: BlockId,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// One edit inside a `batch`, as `{"command", "params"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BatchItem {
    pub command: String,
    #[serde(default = "empty_object")]
    pub params: Value,
}

impl BatchItem {
    pub fn of(cmd: &Command) -> BatchItem {
        BatchItem {
            command: cmd.name().to_string(),
            params: cmd.params(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub commands: Vec<BatchItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TextFind {
    pub find: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    #[serde(default = "yes")]
    pub match_case: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TextComplete {
    pub id: BlockId,
    pub at: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(feature = "schema")]
    #[test]
    fn every_command_has_a_closed_object_schema() {
        for name in Command::NAMES {
            let s = Command::params_schema(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(s["type"], "object", "{name}: {s}");
            assert_eq!(s["additionalProperties"], false, "{name}: {s}");
            assert!(!s.to_string().contains("$ref"), "{name}: {s}");
        }
        let s = Command::params_schema("block.insert").unwrap();
        assert_eq!(s["required"], json!(["type"]));
        assert_eq!(s["properties"]["after"]["pattern"], "^b_[0-9]+$");
        eprintln!("{}", serde_json::to_string_pretty(&s).unwrap());
    }

    #[test]
    fn names_match_serde() {
        for name in Command::NAMES {
            let params = match name {
                "doc.open" => json!({"path": "x.fountain"}),
                "doc.export" => json!({"format": "json"}),
                "block.insert" => json!({"type": "action"}),
                "block.set_type" => json!({"id": "b_1", "type": "action"}),
                "block.set_text" | "block.set_attrs" | "block.delete" => json!({"id": "b_1"}),
                "text.replace" => json!({"find": "a", "with": "b"}),
                "scene.move" => json!({"id": "b_1"}),
                "export.pdf" => json!({"path": "x.pdf"}),
                "export.fdx" | "export.fountain" => json!({"path": "x"}),
                "file.repair" => json!({"path": "a.fdx", "out": "b.fdx"}),
                "text.insert" => json!({"id": "b_1", "at": 0, "text": "x"}),
                "text.delete" | "text.style" => json!({"id": "b_1", "from": 0, "to": 1}),
                "block.split" | "text.complete" => json!({"id": "b_1", "at": 0}),
                "block.join" => json!({"id": "b_1"}),
                "batch" => json!({"commands": []}),
                "text.find" => json!({"find": "x"}),
                _ => json!({}),
            };
            let cmd = Command::from_parts(name, params).unwrap();
            assert_eq!(cmd.name(), name);
        }
    }

    #[test]
    fn rejects_unknown_params() {
        let e = Command::from_parts("block.delete", json!({"id": "b_1", "idd": 2})).unwrap_err();
        assert!(e.contains("idd"), "{e}");
        assert!(Command::from_parts("undo", json!({"x": 1})).is_err());
        assert!(Command::from_parts("block.delete", json!({"id": "7"})).is_err());
    }

    #[test]
    fn set_attrs_null_vs_absent() {
        let Command::BlockSetAttrs(a) = Command::from_parts(
            "block.set_attrs",
            json!({"id": "b_1", "scene_number": null}),
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(a.scene_number, Some(None));
        assert_eq!(a.space_before, None);
    }

    #[test]
    fn export_pdf_defaults() {
        let Command::ExportPdf(p) =
            Command::from_parts("export.pdf", json!({"path": "a.pdf"})).unwrap()
        else {
            panic!()
        };
        assert!(p.title_page);
        assert!(!p.scene_numbers);
        assert_eq!(p.watermark, None);
        assert_eq!(Command::ExportPdf(p).class(), Class::Output);
        assert!(Command::from_parts("export.pdf", json!({})).is_err());
    }

    #[test]
    fn params_round_trip() {
        let cmd = Command::TextReplace(TextReplace {
            find: "MAYA".into(),
            with: "NINA".into(),
            scope: None,
            match_case: true,
            at: None,
        });
        let back = Command::from_parts(cmd.name(), cmd.params()).unwrap();
        assert_eq!(back, cmd);
    }
}
