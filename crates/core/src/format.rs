//! Script file formats behind one seam (review finding F18.1): reading a
//! file's bytes into a document, writing a document back into bytes with
//! what the format could not hold, and the form a document is laid out in.
//! The dispatcher, recovery and the views ask the session's format and
//! never name a format's crate themselves.

use std::path::Path;

use std::collections::BTreeMap;

use coldopen_fountain::SourceMap;
use coldopen_screenplay::{
    Align, PageFormat, Pt, RichText, Script, TitleField, TitleLine, TitlePage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::document::{Block, BlockId, Document};
use crate::error::CommandError;
use crate::io::{self, SourceStyle};

/// A script file format. Journal headers record it (absent means Fountain,
/// docs/specs/journal.md, "Header"), and so does every open session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatId {
    #[default]
    Fountain,
    Fdx,
    /// The `.coldopen` project bundle (docs/specs/bundle.md), written
    /// `"coldopen"` in journal headers.
    #[serde(rename = "coldopen")]
    Bundle,
}

impl FormatId {
    /// Every format, in the order messages list them.
    pub const ALL: [FormatId; 3] = [FormatId::Fountain, FormatId::Fdx, FormatId::Bundle];

    /// The name commands use, such as `"fountain"`.
    pub fn name(self) -> &'static str {
        match self {
            FormatId::Fountain => "fountain",
            FormatId::Fdx => "fdx",
            FormatId::Bundle => "coldopen",
        }
    }

    /// The name people read, in messages.
    pub fn title(self) -> &'static str {
        match self {
            FormatId::Fountain => "Fountain",
            FormatId::Fdx => "An .fdx file",
            FormatId::Bundle => "A Cold Open project",
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
            FormatId::Fdx => &Fdx,
            FormatId::Bundle => &crate::bundle::Bundle,
        }
    }

    /// Whether the format's files keep tracked changes and their table, so
    /// a journal header names the table instead of copying it.
    pub fn holds_changes(self) -> bool {
        self == FormatId::Bundle
    }

    /// For `skip_serializing_if`: Fountain is the default and is not written.
    pub fn is_fountain(&self) -> bool {
        *self == FormatId::Fountain
    }
}

/// A file read into a document.
#[derive(Debug)]
pub struct ReadOut {
    pub doc: Document,
    /// How the bytes stored their text, so a save can keep it.
    pub style: SourceStyle,
    /// `{code, message, ...}` objects, for `doc.open` and `doc.revert`.
    pub warnings: Vec<Value>,
    /// For a file the reader repaired: `{fixed, dropped, issues}`
    /// (docs/specs/commands.md, `doc.open`).
    pub repaired: Option<Value>,
}

/// Something in the document the format cannot hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loss {
    /// Index of the block it concerns, or `None` for the whole document.
    pub element: Option<usize>,
    /// Stable machine-readable code, such as `shot_element`.
    pub code: String,
    pub message: String,
    pub class: LossClass,
}

/// What a loss means for a save (docs/specs/commands.md, "Losses").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LossClass {
    /// The format cannot hold it: a save refuses without `allow_lossy`.
    #[default]
    Lost,
    /// Written in the format's normal form, which prints the same: never a
    /// reason to refuse.
    Normalized,
}

impl LossClass {
    pub fn name(self) -> &'static str {
        match self {
            LossClass::Lost => "lost",
            LossClass::Normalized => "normalized",
        }
    }
}

impl Loss {
    fn lost(element: Option<usize>, code: impl Into<String>, message: impl Into<String>) -> Loss {
        Loss {
            element,
            code: code.into(),
            message: message.into(),
            class: LossClass::Lost,
        }
    }
}

impl Written {
    /// Whether anything the format could not hold was left out or changed:
    /// what refuses a save, and what the recovery check counts.
    pub fn is_lossy(&self) -> bool {
        self.losses.iter().any(|l| l.class == LossClass::Lost)
    }
}

/// The document a format that holds neither tracked changes nor revision
/// sets writes: as accepting every change leaves it (docs/specs/changes.md
/// §9), without revision sets or marks (docs/specs/revisions.md §9), with a
/// loss for each it leaves out.
fn plain_form(doc: &Document) -> Option<(Document, Vec<Loss>)> {
    let changes = doc.has_changes();
    let revisions = doc.has_revision_data();
    if !changes && !revisions {
        return None;
    }
    let mut losses = Vec::new();
    let mut plain = if changes {
        let n = doc.pending().len();
        losses.push(Loss::lost(
            None,
            "tracked_changes",
            format!(
                "{n} pending tracked change{} cannot be kept, so the script is written as accepting {} leaves it",
                if n == 1 { "" } else { "s" },
                if n == 1 { "it" } else { "them all" }
            ),
        ));
        doc.accepted()
    } else {
        doc.clone()
    };
    if revisions {
        let sets = doc.revisions.sets.len();
        let blocks = doc.blocks.iter().filter(|b| b.has_revisions()).count();
        losses.push(Loss::lost(
            None,
            "revision_marks",
            format!(
                "{sets} revision set{} and the revision marks on {blocks} block{} cannot be kept",
                if sets == 1 { "" } else { "s" },
                if blocks == 1 { "" } else { "s" },
            ),
        ));
        plain = plain.without_revisions();
    }
    Some((plain, losses))
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
            repaired: None,
        })
    }

    fn style(&self, bytes: &[u8]) -> SourceStyle {
        io::decode(bytes).style
    }

    fn write(&self, doc: &Document, style: SourceStyle) -> Written {
        let accepted = plain_form(doc);
        let doc = accepted.as_ref().map_or(doc, |(d, _)| d);
        let (script, mut losses) = fountain_script(doc);
        let written = coldopen_fountain::write(&script);
        let mut all: Vec<Loss> = written
            .losses
            .into_iter()
            .map(|l| Loss::lost(l.element, l.code, l.message))
            .collect();
        all.append(&mut losses);
        all.extend(accepted.into_iter().flat_map(|(_, l)| l));
        all.extend(written.normalized.into_iter().map(|l| Loss {
            element: l.element,
            code: l.code.to_string(),
            message: l.message,
            class: LossClass::Normalized,
        }));
        Written {
            bytes: io::encode(&written.text, style),
            losses: all,
        }
    }

    fn layout_form(&self, doc: &Document) -> LayoutForm {
        let (script, sources) = coldopen_fountain::normalize_with_source_map(&doc.to_script());
        LayoutForm { script, sources }
    }
}

/// The bytes a Fountain save of `doc` writes, with every pending change
/// accepted and the default style, found without checking what is lost:
/// for a copy that is never read back, a bundle's `script.fountain`
/// (docs/specs/bundle.md §6). The same bytes as [`Fountain`]'s `write`.
pub(crate) fn fountain_text(doc: &Document) -> Vec<u8> {
    let accepted = doc.has_changes().then(|| doc.accepted());
    let (script, _) = fountain_script(accepted.as_ref().unwrap_or(doc));
    io::encode(
        &coldopen_fountain::write_text(&script),
        SourceStyle::default(),
    )
}

/// The script a document writes as Fountain, with what Fountain cannot hold
/// of a document read from .fdx (docs/specs/fountain.md §11). A Fountain
/// document has none of it, so it writes exactly as before.
fn fountain_script(doc: &Document) -> (Script, Vec<Loss>) {
    let mut script = doc.to_script();
    let mut losses = Vec::new();
    if doc
        .page_format
        .as_deref()
        .is_some_and(|f| *f != PageFormat::default())
    {
        losses.push(Loss::lost(
            None,
            "page_format",
            "the page and element settings are not kept; Fountain always prints with the default format",
        ));
    }
    let line = doc
        .page_format
        .as_deref()
        .map_or(PageFormat::default().line_height, |f| f.line_height)
        .0
        .max(1);
    for (id, pt) in &doc.spacing {
        let Some(i) = doc.index_of(*id) else {
            continue;
        };
        let e = &mut script.elements[i];
        if !e.kind.is_printing() {
            continue;
        }
        let lines = u8::try_from((pt.0.max(0) + line / 2) / line).unwrap_or(u8::MAX);
        e.space_before = Some(lines);
        if pt.0 % line != 0 {
            losses.push(Loss::lost(
                Some(i),
                "paragraph_spacing",
                "space above that is not a whole number of lines is written as the nearest number of blank lines",
            ));
        }
    }
    if !doc.title_lines.is_empty() && doc.title_page.is_empty() {
        script.title_page = title_keys(&doc.title_lines);
        losses.push(Loss::lost(
            None,
            "title_page_layout",
            "the title page's positions are not kept; its lines are written as title-page keys",
        ));
    }
    if let Some(kept) = &doc.fdx {
        let names = kept.section_names();
        losses.push(Loss::lost(
            None,
            "fdx_data",
            if names.is_empty() {
                "data the .fdx file holds for other apps is not kept".into()
            } else {
                format!(
                    "data the .fdx file holds for other apps is not kept: {}",
                    names.join(", ")
                )
            },
        ));
    }
    (script, losses)
}

/// After a save that converted the document to `format`, the parts the
/// format cannot hold, which that save reported, are given the form it
/// can, so later saves do not ask again (docs/specs/commands.md,
/// "Converting"). None of these parts is changed by an undoable command.
pub fn adopt(doc: &mut Document, format: FormatId) {
    // The identity belongs to a bundle; another file is another document.
    if format != FormatId::Bundle {
        doc.doc_id = None;
    }
    match format {
        // A bundle holds everything.
        FormatId::Bundle => {}
        FormatId::Fountain => {
            doc.fdx = None;
            doc.page_format = None;
            if doc.title_page.is_empty() && !doc.title_lines.is_empty() {
                doc.title_page = title_keys(&doc.title_lines);
            }
            doc.title_lines.clear();
        }
        FormatId::Fdx => {
            if doc.title_lines.is_empty() && !doc.title_page.is_empty() {
                let format = doc.page_format.as_deref().cloned().unwrap_or_default();
                doc.title_lines = coldopen_layout::title_lines(&doc.title_page, &format);
            }
            doc.title_page = TitlePage::default();
        }
    }
}

/// Title lines as Fountain title-page keys, by the rule of
/// docs/specs/fountain.md §11.
fn title_keys(lines: &[TitleLine]) -> TitlePage {
    // Blocks of lines, split at empty lines and at space above.
    let mut blocks: Vec<(Align, Vec<RichText>)> = Vec::new();
    let mut open = false;
    for l in lines {
        if l.text.plain_text().trim().is_empty() {
            open = false;
            continue;
        }
        let starts =
            !open || l.space_before > Pt::ZERO || blocks.last().is_some_and(|b| b.0 != l.align);
        if starts {
            blocks.push((l.align, Vec::new()));
        }
        if let Some(b) = blocks.last_mut() {
            b.1.push(l.text.clone());
        }
        open = true;
    }
    let mut fields: Vec<TitleField> = Vec::new();
    let mut push = |key: &str, values: Vec<RichText>| match fields.iter_mut().find(|f| f.key == key)
    {
        Some(f) => f.values.extend(values),
        None => fields.push(TitleField::new(key, values)),
    };
    let mut title_seen = false;
    let mut after_credit = false;
    for (align, values) in blocks {
        match align {
            Align::Center if !title_seen => {
                title_seen = true;
                push("Title", values);
            }
            Align::Center => {
                let credit = values.len() == 1
                    && values[0].plain_text().trim().to_lowercase().ends_with("by");
                if credit {
                    after_credit = true;
                    push("Credit", values);
                } else if after_credit {
                    after_credit = false;
                    push("Author", values);
                } else {
                    push("Source", values);
                }
            }
            Align::Left => push("Contact", values),
            Align::Right => push("Notes", values),
        }
    }
    TitlePage { fields }
}

/// .fdx (docs/specs/fdx.md).
pub struct Fdx;

impl Fdx {
    /// How the file's text encoding maps onto ours: windows-1252 and
    /// repaired bytes are written as UTF-8, which a save reports.
    fn source_style(enc: coldopen_fdx::TextEncoding) -> SourceStyle {
        use coldopen_fdx::Encoding as E;
        SourceStyle {
            bom: enc.bom,
            // Line ends are the store's: they are written as they were.
            crlf: false,
            encoding: match enc.encoding {
                E::Utf8 => io::Encoding::Utf8,
                E::Utf16Le => io::Encoding::Utf16Le,
                E::Utf16Be => io::Encoding::Utf16Be,
                E::Windows1252 | E::Lossy => io::Encoding::Windows1252,
            },
        }
    }
}

impl Format for Fdx {
    fn id(&self) -> FormatId {
        FormatId::Fdx
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["fdx"]
    }

    fn read(&self, bytes: &[u8]) -> Result<ReadOut, CommandError> {
        let read = coldopen_fdx::read(bytes).map_err(|e| {
            CommandError::new(
                "unsupported_format",
                format!("the file is not an .fdx file Cold Open can read: {e}"),
            )
        })?;
        let style = Fdx::source_style(read.encoding);
        let issue = |i: &coldopen_fdx::Issue| serde_json::to_value(i).unwrap_or(Value::Null);
        // Repairs, and a format layout could not use, are warnings; what the
        // file keeps for other apps is not something to warn about.
        let warnings: Vec<Value> = read
            .report
            .iter()
            .filter(|i| i.is_repair() || i.code == coldopen_fdx::codes::INVALID_FORMAT)
            .map(issue)
            .collect();
        let repaired = read.repaired().then(|| {
            let count = |class: &str| read.report.iter().filter(|i| i.class == class).count();
            json!({
                "fixed": count(coldopen_fdx::codes::FIXED),
                "dropped": count(coldopen_fdx::codes::DROPPED),
                "issues": read.report.iter().filter(|i| i.is_repair()).map(issue).collect::<Vec<_>>(),
            })
        });
        let blocks: Vec<Block> = read
            .script
            .elements
            .into_iter()
            .enumerate()
            .map(|(i, element)| Block::new(BlockId(i as u64 + 1), element))
            .collect();
        let spacing: BTreeMap<BlockId, Pt> = read
            .overrides
            .iter()
            .map(|(&i, &pt)| (BlockId(i as u64 + 1), pt))
            .collect();
        let doc = Document {
            title_page: TitlePage::default(),
            next_id: blocks.len() as u64 + 1,
            blocks,
            page_format: (read.format != PageFormat::default()).then(|| Box::new(read.format)),
            title_lines: read.title_lines,
            spacing,
            fdx: Some(Box::new(read.kept)),
            ..Document::default()
        };
        Ok(ReadOut {
            doc,
            style,
            warnings,
            repaired,
        })
    }

    fn style(&self, bytes: &[u8]) -> SourceStyle {
        Fdx::source_style(coldopen_fdx::decode(bytes).2)
    }

    fn write(&self, doc: &Document, style: SourceStyle) -> Written {
        let accepted = plain_form(doc);
        let doc = accepted.as_ref().map_or(doc, |(d, _)| d);
        let format = doc.page_format.as_deref().cloned().unwrap_or_default();
        let mut losses: Vec<Loss> = accepted.iter().flat_map(|(_, l)| l.clone()).collect();
        // A Fountain title page is written as the lines it prints as
        // (docs/specs/fdx.md §14.3).
        let converted;
        let title_lines: &[TitleLine] = if doc.title_lines.is_empty() && !doc.title_page.is_empty()
        {
            converted = coldopen_layout::title_lines(&doc.title_page, &format);
            losses.push(Loss::lost(
                None,
                "title_page_keys",
                "the title page's key names (Title, Author...) have no place in .fdx; its lines are kept",
            ));
            &converted
        } else {
            &doc.title_lines
        };
        let blocks: Vec<coldopen_fdx::Blk<'_>> = doc
            .blocks
            .iter()
            .map(|b| coldopen_fdx::Blk {
                id: b.id.0,
                element: &b.element,
                spacing: doc.spacing.get(&b.id).copied(),
            })
            .collect();
        let written = coldopen_fdx::write(&coldopen_fdx::Doc {
            blocks,
            format: &format,
            title_lines,
            kept: doc.fdx.as_deref(),
            declare_utf8: style.encoding == io::Encoding::Windows1252,
        });
        losses.extend(written.losses.into_iter().map(|l| {
            Loss::lost(
                l.block.and_then(|id| doc.index_of(BlockId(id))),
                l.code,
                l.message,
            )
        }));
        Written {
            bytes: io::encode(
                &written.text,
                SourceStyle {
                    crlf: false,
                    ..style
                },
            ),
            losses,
        }
    }

    fn layout_form(&self, doc: &Document) -> LayoutForm {
        // Laid out as the blocks stand (docs/specs/layout.md §4.8).
        let script = doc.to_script();
        let sources = SourceMap::identity(&script);
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
            "{} is not a script Cold Open opens: a Fountain file (.fountain, .spmd or .txt), an .fdx file or a Cold Open project (.coldopen)",
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
