//! Fountain parser and writer for Cold Open.
//!
//! [`parse`] never fails: anything it cannot classify becomes Action, and
//! problems come back as [`Diagnostic`]s. [`write`] produces one canonical
//! text and reports anything Fountain cannot express as a [`Loss`].
//!
//! The rules implemented here are written down in `docs/specs/fountain.md`;
//! section numbers in comments (§n) refer to it.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod classify;
#[doc(hidden)]
pub mod fuzzing;
mod inline;
mod line;
mod norm;
mod scan;
mod title;
mod write;

use coldopen_screenplay::Script;

pub use norm::normalize;
pub use write::{Loss, Written, write};

/// Something the parser noticed and recovered from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number in the input.
    pub line: usize,
    /// Stable machine-readable code, such as `unclosed_note`.
    pub code: &'static str,
    pub message: String,
}

impl Diagnostic {
    pub(crate) fn new(line: usize, code: &'static str, message: &str) -> Diagnostic {
        Diagnostic {
            line,
            code,
            message: message.to_string(),
        }
    }
}

/// The result of [`parse`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub script: Script,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses Fountain text. Never fails and never panics.
pub fn parse(input: &str) -> Parsed {
    let text = input.strip_prefix('\u{feff}').unwrap_or(input);
    let lines = line::split_lines(text);
    let (title_page, body_start) = match title::detect(&lines) {
        Some((tp, end)) => (tp, end),
        None => (Default::default(), 0),
    };
    let mut diagnostics = Vec::new();
    let body = lines.get(body_start..).unwrap_or_default();
    let scanned = scan::scan(body, body_start + 1, &mut diagnostics);
    let elements = classify::classify(scanned.lines);
    Parsed {
        script: Script {
            title_page,
            elements,
        },
        diagnostics,
    }
}
