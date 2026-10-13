//! Plain screenplay document types shared by the Cold Open format libraries.
//!
//! A [`Script`] is an optional [`TitlePage`] plus a flat list of
//! [`Element`]s, one per paragraph. Text is [`RichText`]: a normalized list of
//! styled runs, with inline notes and boneyard kept as hidden runs.
//!
//! This crate holds data only. Parsing and writing live in the format crates;
//! ids, commands and undo live in the application core.
#![forbid(unsafe_code)]

mod element;
mod script;
mod text;

pub use element::{Element, ElementKind, ElementRepr};
pub use script::{Script, TitleField, TitlePage};
pub use text::{RichText, Run, Style, TextRepr};
