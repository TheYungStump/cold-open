//! Who made a change (docs/specs/commands.md, "Authors and undo steps").

use std::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// The author of an edit, `undo` or `redo`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Author {
    /// A person: the desktop app, the CLI, a stdio client. `name` is the
    /// name their tracked changes carry (2026-10-18); without one they
    /// show "Writer".
    Writer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// An agent through MCP: its client name and the tool it called.
    Agent {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
    },
}

impl Default for Author {
    fn default() -> Author {
        Author::writer()
    }
}

impl Author {
    /// A writer with no name.
    pub fn writer() -> Author {
        Author::Writer { name: None }
    }

    /// A writer whose tracked changes carry `name`.
    pub fn named_writer(name: impl Into<String>) -> Author {
        Author::Writer {
            name: Some(name.into()),
        }
    }

    pub fn agent(name: impl Into<String>, tool: Option<String>) -> Author {
        Author::Agent {
            name: name.into(),
            tool,
        }
    }

    pub fn is_writer(&self) -> bool {
        matches!(self, Author::Writer { .. })
    }

    /// The writer with no name, which the journal leaves out.
    pub fn is_plain_writer(&self) -> bool {
        matches!(self, Author::Writer { name: None })
    }

    /// The same person or agent, whatever tool it used. Any two writers are
    /// the same here: names matter only for tracked changes
    /// (docs/specs/commands.md, "Authors and undo steps").
    pub fn same_actor(&self, other: &Author) -> bool {
        match (self, other) {
            (Author::Writer { .. }, Author::Writer { .. }) => true,
            (Author::Agent { name: a, .. }, Author::Agent { name: b, .. }) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Author::Writer { .. } => f.write_str("writer"),
            Author::Agent {
                name,
                tool: Some(t),
            } => write!(f, "{name} · {t}"),
            Author::Agent { name, tool: None } => f.write_str(name),
        }
    }
}

/// A monotonic clock in milliseconds, used only to group typing into undo
/// steps. Tests replace it to make grouping deterministic.
#[derive(Clone)]
pub struct Clock(Arc<dyn Fn() -> u64 + Send + Sync>);

impl Clock {
    pub fn new(f: impl Fn() -> u64 + Send + Sync + 'static) -> Clock {
        Clock(Arc::new(f))
    }

    pub fn now_ms(&self) -> u64 {
        (self.0)()
    }
}

impl Default for Clock {
    fn default() -> Clock {
        static START: OnceLock<Instant> = OnceLock::new();
        let start = *START.get_or_init(Instant::now);
        Clock::new(move || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX))
    }
}

impl fmt::Debug for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Clock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape() {
        assert_eq!(
            serde_json::to_string(&Author::writer()).unwrap(),
            r#"{"kind":"writer"}"#
        );
        assert_eq!(
            serde_json::from_str::<Author>(r#"{"kind":"writer"}"#).unwrap(),
            Author::writer()
        );
        let named = Author::named_writer("Maya");
        let json = serde_json::to_string(&named).unwrap();
        assert_eq!(json, r#"{"kind":"writer","name":"Maya"}"#);
        assert_eq!(serde_json::from_str::<Author>(&json).unwrap(), named);
        let a = Author::agent("claude-code", Some("block_insert".into()));
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"agent","name":"claude-code","tool":"block_insert"}"#
        );
        assert_eq!(serde_json::from_str::<Author>(&json).unwrap(), a);
        assert_eq!(a.to_string(), "claude-code · block_insert");
    }

    #[test]
    fn same_actor_ignores_the_tool() {
        let a = Author::agent("x", Some("undo".into()));
        let b = Author::agent("x", Some("text_insert".into()));
        assert!(a.same_actor(&b));
        assert!(!a.same_actor(&Author::agent("y", None)));
        assert!(!a.same_actor(&Author::writer()));
        assert!(Author::writer().same_actor(&Author::named_writer("Maya")));
    }
}
