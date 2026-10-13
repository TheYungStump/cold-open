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

pub mod command;
pub mod dispatcher;
pub mod document;
pub mod error;
#[doc(hidden)]
pub mod fuzzing;
pub mod io;
pub mod journal;
pub mod ops;
pub mod replace;
pub mod session;

pub use command::Command;
pub use dispatcher::{Dispatcher, Options, to_line};
pub use document::{Block, BlockId, Document};
pub use error::CommandError;
