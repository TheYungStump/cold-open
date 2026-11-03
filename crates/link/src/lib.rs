//! The agent link between the Cold Open desktop app and `coldopen mcp`
//! (docs/specs/mcp.md §5): session files that say where the app listens,
//! the handshake, bounded line reading, and a socket or pipe that only the
//! same user on the same machine can use.
//!
//! The app is the server, `coldopen mcp` the client. Neither side needs
//! anything else from the other crate, so the app never links the MCP SDK.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod client;
mod hello;
mod lines;
mod session;
mod transport;

use std::time::Duration;

pub use client::{AttachError, LinkClient};
pub use hello::{
    Client, Hello, HelloError, HelloOk, clean_name, hello_line, parse_hello, parse_reply,
    reply_err, reply_ok, tokens_equal,
};
pub use lines::LineReader;
pub use session::{SessionFile, canonical, find_sessions, new_token, random_hex, remove_stale};
pub use transport::{Conn, Server};

/// The link protocol's version.
pub const VERSION: u32 = 1;
/// The most links the app accepts at once.
pub const MAX_LINKS: usize = 4;
/// The longest first line, the handshake.
pub const HELLO_LIMIT: usize = 64 * 1024;
/// The longest request line after the handshake.
pub const LINE_LIMIT: usize = 8 * 1024 * 1024;
/// The most commands in one `batch` over a link.
pub const BATCH_LIMIT: usize = 10_000;
/// How long a new connection has to send its handshake.
pub const HELLO_DEADLINE: Duration = Duration::from_secs(5);
/// The shortest time between two `changed` events on a link.
pub const CHANGED_EVERY: Duration = Duration::from_millis(50);

/// The folder for session files (docs/specs/editor.md §11), the same one
/// the app uses:
/// `%LOCALAPPDATA%/Cold Open/run` on Windows, `~/Library/Application
/// Support/Cold Open/run` on macOS, `$XDG_RUNTIME_DIR/coldopen` or
/// `~/.local/share/coldopen/run` elsewhere. `COLDOPEN_HOME`, when set, puts
/// it at `<COLDOPEN_HOME>/run`, as the app's other folders.
pub fn run_dir() -> Option<std::path::PathBuf> {
    run_dir_in(&|name| std::env::var_os(name))
}

/// [`run_dir`] with the environment given, for tests of a first start.
pub fn run_dir_in(get: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let env = |name: &str| get(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(home) = env("COLDOPEN_HOME") {
        return Some(home.join("run"));
    }
    if cfg!(windows) {
        let local = env("LOCALAPPDATA").or_else(|| env("APPDATA"))?;
        return Some(local.join("Cold Open").join("run"));
    }
    let home = env("HOME")?;
    if cfg!(target_os = "macos") {
        return Some(home.join("Library/Application Support/Cold Open/run"));
    }
    if let Some(r) = env("XDG_RUNTIME_DIR") {
        return Some(r.join("coldopen"));
    }
    let data = env("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
    Some(data.join("coldopen").join("run"))
}

/// The line a client sends when it is done (mcp.md §5.4).
pub const BYE: &str = r#"{"bye":true}"#;

/// Commands an attached agent may not run (mcp.md §5.4): they would
/// replace, close or rename the writer's document, or restore or delete
/// the writer's set-aside copies. `doc.save` is refused only with a path.
pub fn refused(command: &str, params: &serde_json::Value) -> bool {
    match command {
        "doc.new" | "doc.open" | "doc.close" | "doc.revert" | "recovered.restore"
        | "recovered.delete" => true,
        "doc.save" => params.get("path").is_some_and(|p| !p.is_null()),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn refused_commands() {
        assert!(refused("doc.open", &json!({"path": "x"})));
        assert!(refused("doc.save", &json!({"path": "x"})));
        assert!(!refused("doc.save", &json!({})));
        assert!(!refused("block.insert", &json!({})));
    }
}
