//! Cold Open core: the document model with stable ids, the `Command` enum,
//! the dispatcher, undo and redo, the crash journal and atomic saves.
//!
//! Every change to a document is a [`Command`] sent to a [`Dispatcher`].
//! Front ends (desktop app, CLI, JSON over stdio, MCP) never change a
//! document any other way. The behaviour is specified in
//! `docs/specs/commands.md` and `docs/specs/journal.md`.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod author;
pub mod command;
pub mod complete;
pub mod dispatcher;
pub mod document;
pub mod error;
pub mod flush;
#[doc(hidden)]
pub mod fuzzing;
pub mod io;
pub mod journal;
pub mod ops;
pub mod replace;
pub mod session;
pub mod view;

pub use author::{Author, Clock};
pub use command::Command;
pub use dispatcher::{Dispatcher, Options, PreparedSave, to_line};
pub use document::{Block, BlockId, Document};
pub use error::CommandError;
pub use flush::{Durability, FlushStatus};
pub use session::{Exec, Merge};
pub use view::View;
