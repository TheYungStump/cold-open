//! Script file formats behind one seam (review finding F18.1): reading a
//! file's bytes into a document, writing a document back into bytes with
//! what the format could not hold, and the form a document is laid out in.
//! The dispatcher, recovery and the views ask the session's format and
//! never name a format's crate themselves.

use std::path::Path;

use coldopen_fountain::SourceMap;
use coldopen_screenplay::Script;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::document::Document;
use crate::error::CommandError;
use crate::io::{self, SourceStyle};

/// A script file format. Journal headers record it (absent means Fountain,
/// docs/specs/journal.md, "Header"), and so does every open session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatId {
    #[default]
    Fountain,
}

impl FormatId {
    /// Every format, in the order messages list them.
    pub const ALL: [FormatId; 1] = [FormatId::Fountain];

    /// The name commands use, such as `"fountain"`.
    pub fn name(self) -> &'static str {
        match self {
            FormatId::Fountain => "fountain",
        }
    }

    /// The format a command names, ignoring ASCII case.
    pub fn from_name(name: &str) -> Option<FormatId> {
        FormatId::ALL
            .into_iter()
            .find(|f| f.name().eq_ignore_ascii_case(name))
    }

    /// The format a path's extension names, ignoring ASCII case.
    pub fn for_path(path: &Path) -> Option<FormatId> {
        let ext = path.extension()?.to_str()?;
        FormatId::ALL.into_iter().find(|f| {
            f.get()
                .extensions()
                .iter()
                .any(|e| e.eq_ignore_ascii_case(ext))
        })
    }

    /// The format itself.
    pub fn get(self) -> &'static dyn Format {
        match self {
            FormatId::Fountain => &Fountain,
        }
    }

    /// For `skip_serializing_if`: Fountain is the default and is not written.
    pub fn is_fountain(&self) -> bool {
        *self == FormatId::Fountain
    }
}

/// A file read into a document.
pub struct ReadOut {
    pub doc: Document,
    /// How the bytes stored their text, so a save can keep it.
    pub style: SourceStyle,
    /// `{code, message, ...}` objects, for `doc.open` and `doc.revert`.
    pub warnings: Vec<Value>,
}

/// Something in the document the format cannot hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loss {
    /// Index of the block it concerns, or `None` for the whole document.
    pub element: Option<usize>,
    /// Stable machine-readable code, such as `shot_element`.
    pub code: String,
    pub message: String,
}

/// A document written out.
pub struct Written {
    pub bytes: Vec<u8>,
    /// Empty when reading the bytes back gives the document again.
    pub losses: Vec<Loss>,
}

/// The script a document is laid out as, with where each laid-out
/// element's text came from in the blocks (docs/specs/layout.md §4.8).
pub struct LayoutForm {
    pub script: Script,
    pub sources: SourceMap,
}

/// One script file format.
pub trait Format: Sync {
    fn id(&self) -> FormatId;

    /// File extensions that open in this format, without the dot.
    fn extensions(&self) -> &'static [&'static str];

    /// Reads a file's bytes. Fails only when the bytes cannot be this
    /// format at all; anything else is recovered and reported in warnings.
    fn read(&self, bytes: &[u8]) -> Result<ReadOut, CommandError>;

    /// How a file's bytes store their text, without reading the script.
    fn style(&self, bytes: &[u8]) -> SourceStyle;

    /// Writes the document in the given text style, listing what the
    /// format could not hold.
    fn write(&self, doc: &Document, style: SourceStyle) -> Written;

    /// The form the document is laid out in.
    fn layout_form(&self, doc: &Document) -> LayoutForm;
}

/// Fountain (docs/specs/fountain.md).
pub struct Fountain;

impl Format for Fountain {
    fn id(&self) -> FormatId {
        FormatId::Fountain
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["fountain", "spmd", "txt"]
    }

    fn read(&self, bytes: &[u8]) -> Result<ReadOut, CommandError> {
        let decoded = io::decode(bytes);
        let parsed = coldopen_fountain::parse(&decoded.text);
        let mut warnings = encoding_warnings(&decoded);
        for d in &parsed.diagnostics {
            warnings.push(json!({ "code": d.code, "message": d.message, "line": d.line }));
        }
        Ok(ReadOut {
            doc: Document::from_script(parsed.script),
            style: decoded.style,
            warnings,
        })
    }

    fn style(&self, bytes: &[u8]) -> SourceStyle {
        io::decode(bytes).style
    }

    fn write(&self, doc: &Document, style: SourceStyle) -> Written {
        let written = coldopen_fountain::write(&doc.to_script());
        Written {
            bytes: io::encode(&written.text, style),
            losses: written
                .losses
                .into_iter()
                .map(|l| Loss {
                    element: l.element,
                    code: l.code.to_string(),
                    message: l.message,
                })
                .collect(),
        }
    }

    fn layout_form(&self, doc: &Document) -> LayoutForm {
        let (script, sources) = coldopen_fountain::normalize_with_source_map(&doc.to_script());
        LayoutForm { script, sources }
    }
}

fn warning(code: &str, message: &str) -> Value {
    json!({ "code": code, "message": message })
}

/// Warnings about how a file's bytes were read as text.
fn encoding_warnings(decoded: &io::Decoded) -> Vec<Value> {
    let mut out = Vec::new();
    match decoded.style.encoding {
        io::Encoding::Windows1252 => out.push(warning(
            "not_utf8",
            "the file is not UTF-8; it was read as Windows-1252, and saving it writes UTF-8",
        )),
        io::Encoding::Utf16Le | io::Encoding::Utf16Be => out.push(warning(
            "utf16",
            "the file is UTF-16; it is saved as UTF-16 again",
        )),
        io::Encoding::Utf8 => {}
    }
    if decoded.replaced {
        out.push(warning(
            "utf16_damaged",
            "the UTF-16 text held invalid sequences; they were replaced with U+FFFD",
        ));
    }
    out
}

/// The error for a path no format opens.
pub fn unsupported(path: &Path) -> CommandError {
    CommandError::new(
        "unsupported_format",
        format!(
            "{} is not a Fountain file (.fountain, .spmd or .txt); other formats come later",
            path.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_found_by_name_and_extension() {
        assert_eq!(FormatId::from_name("Fountain"), Some(FormatId::Fountain));
        assert_eq!(FormatId::from_name("pdf"), None);
        for p in ["a.fountain", "b.SPMD", "c.Txt"] {
            assert_eq!(
                FormatId::for_path(Path::new(p)),
                Some(FormatId::Fountain),
                "{p}"
            );
        }
        for p in ["a.pdf", "noext", "a.fountain.bak"] {
            assert_eq!(FormatId::for_path(Path::new(p)), None, "{p}");
        }
        for id in FormatId::ALL {
            assert_eq!(id.get().id(), id);
            assert_eq!(FormatId::from_name(id.name()), Some(id));
        }
    }

    #[test]
    fn fountain_reads_and_writes_through_the_seam() {
        let read = Fountain
            .read(b"\xEF\xBB\xBFINT. LAB - NIGHT\r\n\r\nRain.\r\n")
            .unwrap();
        assert!(read.style.bom && read.style.crlf);
        assert_eq!(read.doc.blocks.len(), 2);
        let written = Fountain.write(&read.doc, read.style);
        assert_eq!(
            written.bytes,
            b"\xEF\xBB\xBFINT. LAB - NIGHT\r\n\r\nRain.\r\n".to_vec()
        );
        assert!(written.losses.is_empty());
        let form = Fountain.layout_form(&read.doc);
        assert_eq!(form.script.elements.len(), 2);
        assert_eq!(form.sources.elements.len(), 2);
    }

    #[test]
    fn fountain_is_the_default_and_is_not_written() {
        assert!(FormatId::default().is_fountain());
        assert_eq!(
            serde_json::to_value(FormatId::Fountain).unwrap(),
            "fountain"
        );
    }
}
