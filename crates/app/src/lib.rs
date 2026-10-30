//! The Cold Open desktop app: a page-view screenplay editor on iced
//! (docs/specs/editor.md). Every change the app makes is a command sent to
//! the core dispatcher; the app never edits a document directly.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod about;
pub mod actions;
pub mod app;
pub mod bench;
pub mod editor;
pub mod files;
pub mod fonts;
pub mod headless;
pub mod keymap;
pub mod link;
pub mod page_view;
pub mod shield;
pub mod theme;

pub use app::{App, Boot, Message};
