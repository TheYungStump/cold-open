//! Plain screenplay document types shared by the Cold Open format libraries.
//!
//! A [`Script`] is an optional [`TitlePage`] plus a flat list of
//! [`Element`]s, one per paragraph. Text is [`RichText`]: a normalized list of
//! styled runs, with inline notes and boneyard kept as hidden runs.
//!
//! A [`PageFormat`] holds every number layout uses: paper, margins, the
//! character grid and one [`ElementFormat`] per printing kind.
//!
//! This crate holds data only. Parsing and writing live in the format crates;
//! layout lives in `coldopen-layout`; ids, commands and undo live in the
//! application core.
#![forbid(unsafe_code)]

mod edit;
mod element;
mod format;
mod script;
mod text;

pub use edit::{MarkedSpan, RangeError, Settle, StyleChange};
pub use element::{Element, ElementKind, ElementRepr};
pub use format::{
    Align, DualColumn, DualFormat, ElementFormat, Elements, MAX_INSERT, MAX_PAGE_SUFFIX,
    PageFormat, PageNumbers, Pt, SceneContinueds, SplitMode, TitleLine,
};
pub use script::{Script, TitleField, TitlePage};
pub use text::{ChangeId, Marks, RichText, Run, Style, TextRepr};
