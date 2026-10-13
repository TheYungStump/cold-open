//! Command errors with stable codes (docs/specs/commands.md).

use serde::Serialize;
use serde_json::Value;

use crate::document::BlockId;

#[derive(Clone, Debug, PartialEq, Serialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl CommandError {
    pub fn new(code: &'static str, message: impl Into<String>) -> CommandError {
        CommandError {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> CommandError {
        self.details = Some(details);
        self
    }

    pub fn invalid_params(message: impl Into<String>) -> CommandError {
        CommandError::new("invalid_params", message)
    }

    pub fn no_document() -> CommandError {
        CommandError::new("no_document", "no document is open")
    }

    pub fn block_not_found(id: BlockId) -> CommandError {
        CommandError::new("block_not_found", format!("no block {id}"))
            .with_details(serde_json::json!({ "id": id }))
    }

    pub fn scene_not_found(id: BlockId) -> CommandError {
        CommandError::new("scene_not_found", format!("{id} is not a scene heading"))
            .with_details(serde_json::json!({ "id": id }))
    }

    pub fn io(path: &std::path::Path, e: &std::io::Error) -> CommandError {
        CommandError::new("io_error", format!("{}: {e}", path.display()))
            .with_details(serde_json::json!({ "path": path.display().to_string() }))
    }
}
