//! The `.coldopen` project bundle (docs/specs/bundle.md): a zip archive of
//! `meta.json`, `document.json`, `changes.json` and `script.fountain`,
//! written with the same bytes every time and read back tolerantly.

use std::io::Write;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::changes::{ChangeId, Changes, ChangesFile};
use crate::document::Document;
use crate::error::CommandError;
use crate::format::{Format, FormatId, Fountain, ReadOut};
use crate::io::SourceStyle;
use crate::revisions::Revisions;

pub const META: &str = "meta.json";
pub const DOCUMENT: &str = "document.json";
pub const CHANGES: &str = "changes.json";
/// The revision table, in version 2 bundles (docs/specs/bundle.md §5.1).
pub const REVISIONS: &str = "revisions.json";
pub const SCRIPT: &str = "script.fountain";

/// The bundle format's version (docs/specs/bundle.md §3).
pub const V: u32 = 2;
const KIND: &str = "coldopen-bundle";

/// The most an entry may inflate to, and the most entries an archive may
/// hold (docs/specs/bundle.md §2).
const MAX_ENTRY: usize = 256 << 20;
const MAX_ENTRIES: u64 = 64;
/// Deflate level for Cold Open's own writer: level 2 deflates about four
/// times as fast as 6 for a file 14% larger (docs/specs/bundle.md §2).
const LEVEL: u8 = 2;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    format: String,
    v: u32,
    app: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    doc_id: Option<String>,
}

/// A new document identity: a random version 4 UUID (RFC 9562), in
/// lower-case hex with hyphens.
pub fn new_doc_id() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        // No random source: the time still tells documents apart.
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        b = t.to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// `document.json` of a version 2 bundle: the document without its
/// revision table, which is `revisions.json`. Every field is written as
/// `Document` writes it.
#[derive(Serialize)]
struct DocumentOut<'a> {
    title_page: &'a coldopen_screenplay::TitlePage,
    blocks: &'a [crate::document::Block],
    next_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    page_format: &'a Option<Box<coldopen_screenplay::PageFormat>>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    title_lines: &'a [coldopen_screenplay::TitleLine],
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    spacing: &'a std::collections::BTreeMap<crate::document::BlockId, coldopen_screenplay::Pt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fdx: &'a Option<Box<coldopen_fdx::Kept>>,
    #[serde(skip_serializing_if = "crate::changes::Tracking::is_default")]
    tracking: &'a crate::changes::Tracking,
}

/// Writes the document as a bundle. Infallible: everything it holds is
/// JSON or text, and the archive is built in memory. A document whose
/// revision table is the default is version 1, byte for byte as before
/// phase 5b; any other is version 2, with `revisions.json`.
pub fn write(doc: &Document) -> Vec<u8> {
    let v2 = !doc.revisions.is_default();
    let meta = Meta {
        format: KIND.into(),
        v: if v2 { 2 } else { 1 },
        app: env!("CARGO_PKG_VERSION").into(),
        doc_id: doc.doc_id.clone(),
    };
    let mut meta_json = serde_json::to_vec(&meta).unwrap_or_default();
    meta_json.push(b'\n');
    let mut doc_json = if v2 {
        serde_json::to_vec(&DocumentOut {
            title_page: &doc.title_page,
            blocks: &doc.blocks,
            next_id: doc.next_id,
            page_format: &doc.page_format,
            title_lines: &doc.title_lines,
            spacing: &doc.spacing,
            fdx: &doc.fdx,
            tracking: &doc.tracking,
        })
    } else {
        serde_json::to_vec(doc)
    }
    .unwrap_or_default();
    doc_json.push(b'\n');
    let mut changes_json = serde_json::to_vec(&doc.changes.to_file()).unwrap_or_default();
    changes_json.push(b'\n');
    let revisions_json = v2.then(|| {
        let mut j = serde_json::to_vec(&doc.revisions).unwrap_or_default();
        j.push(b'\n');
        j
    });
    let script = crate::format::fountain_text(doc);

    let mut out = Vec::new();
    let mut archive = rawzip::ZipArchiveWriter::new(&mut out);
    let mut parts: Vec<(&str, &[u8])> = vec![
        (META, &meta_json),
        (DOCUMENT, &doc_json),
        (CHANGES, &changes_json),
    ];
    if let Some(r) = &revisions_json {
        parts.push((REVISIONS, r));
    }
    parts.push((SCRIPT, &script));
    for (name, data) in parts {
        // Writing to memory cannot fail.
        let _ = put(&mut archive, name, data);
    }
    let _ = archive.finish();
    out
}

/// One entry, deflated, with no time and no extra fields.
fn put(
    archive: &mut rawzip::ZipArchiveWriter<&mut Vec<u8>>,
    name: &str,
    data: &[u8],
) -> Result<(), rawzip::Error> {
    let (mut entry, _) = archive
        .new_file(name)
        .compression_method(rawzip::CompressionMethod::DEFLATE)
        .start()?;
    let packed = miniz_oxide::deflate::compress_to_vec(data, LEVEL);
    entry.write_all(&packed)?;
    entry.finish(rawzip::DataDescriptorOutput::new(
        rawzip::crc32(data),
        data.len() as u64,
    ))?;
    Ok(())
}

/// The entries of an archive that could be read, by name: the first of
/// each name, with what went wrong with the rest.
struct Entries {
    found: Vec<(String, Vec<u8>)>,
    /// Names of entries that failed their checks.
    damaged: Vec<String>,
    /// Duplicate or unknown entries, ignored.
    ignored: Vec<String>,
}

impl Entries {
    fn get(&self, name: &str) -> Option<&[u8]> {
        self.found
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d.as_slice())
    }
}

/// Reads the archive's entries, checking each one's size and CRC. Fails
/// only when the bytes are not a zip archive at all.
fn entries(bytes: &[u8]) -> Result<Entries, String> {
    let archive = rawzip::ZipArchive::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut out = Entries {
        found: Vec::new(),
        damaged: Vec::new(),
        ignored: Vec::new(),
    };
    if archive.entries_hint() > MAX_ENTRIES {
        return Err(format!(
            "the archive says it holds {} entries; a bundle holds at most 5",
            archive.entries_hint()
        ));
    }
    let mut list = archive.entries();
    let mut seen = 0u64;
    loop {
        let e = match list.next_entry() {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e) => {
                out.damaged.push(format!("the central directory ({e})"));
                break;
            }
        };
        seen += 1;
        if seen > MAX_ENTRIES {
            out.damaged.push("entries past the 64th".into());
            break;
        }
        let name = String::from_utf8_lossy(e.file_path().as_ref()).into_owned();
        if ![META, DOCUMENT, CHANGES, REVISIONS, SCRIPT].contains(&name.as_str())
            || out.found.iter().any(|(n, _)| *n == name)
        {
            out.ignored.push(name);
            continue;
        }
        let size = e.uncompressed_size_hint();
        let crc = e.crc32();
        let method = e.compression_method();
        let data = archive
            .get_entry(e.wayfinder())
            .map_err(|e| e.to_string())
            .and_then(|entry| {
                let raw = entry.data();
                if size > MAX_ENTRY as u64 {
                    return Err("it is too large".to_string());
                }
                match method {
                    rawzip::CompressionMethod::STORE => Ok(raw.to_vec()),
                    rawzip::CompressionMethod::DEFLATE => {
                        miniz_oxide::inflate::decompress_to_vec_with_limit(raw, MAX_ENTRY)
                            .map_err(|_| "its data does not inflate".to_string())
                    }
                    other => Err(format!(
                        "its compression method {} is not read",
                        other.as_u16()
                    )),
                }
            })
            .and_then(|data| {
                if data.len() as u64 != size || rawzip::crc32(&data) != crc {
                    Err("its size or checksum does not match".into())
                } else {
                    Ok(data)
                }
            });
        match data {
            Ok(d) => out.found.push((name, d)),
            Err(why) => out.damaged.push(format!("{name}: {why}")),
        }
    }
    Ok(out)
}

/// What a bundle's report says (docs/specs/bundle.md §7), as `.fdx` issues
/// are reported (`docs/specs/fdx.md` §9).
fn issue(code: &str, class: &str, message: String) -> Value {
    json!({ "code": code, "class": class, "message": message })
}

/// Reads a bundle (docs/specs/bundle.md §7).
pub fn read(bytes: &[u8]) -> Result<ReadOut, CommandError> {
    let unreadable = |why: String| {
        CommandError::new(
            "unsupported_format",
            format!("the file is not a Cold Open project Cold Open can read: {why}"),
        )
    };
    let e = entries(bytes).map_err(unreadable)?;
    let mut issues: Vec<Value> = Vec::new();
    for d in &e.damaged {
        issues.push(issue(
            "bundle_entry",
            "dropped",
            format!("an entry could not be read: {d}"),
        ));
    }
    for n in &e.ignored {
        issues.push(issue(
            "bundle_entries",
            "kept",
            format!(
                "the entry \"{n}\" is not part of a bundle, or a second copy of one, and is ignored"
            ),
        ));
    }

    // meta.json: refuse a newer format; keep going without it.
    let meta = e
        .get(META)
        .and_then(|m| serde_json::from_slice::<Meta>(m).ok())
        .filter(|m| m.format == KIND);
    if let Some(m) = &meta
        && m.v > V
    {
        return Err(CommandError::new(
            "unsupported_format",
            format!(
                "this Cold Open project was made by a newer Cold Open (bundle version {}); this version reads version {V}",
                m.v
            ),
        ));
    }
    let mut doc_id = meta
        .as_ref()
        .and_then(|m| m.doc_id.clone())
        .filter(|s| !s.is_empty());
    if meta.is_none() {
        issues.push(issue(
            "bundle_meta",
            "fixed",
            "meta.json is missing or unreadable; the project gets a new identity".into(),
        ));
        doc_id = Some(new_doc_id());
    }

    // changes.json, then document.json, which must name only changes the
    // table has.
    let table = e
        .get(CHANGES)
        .and_then(|c| serde_json::from_slice::<ChangesFile>(c).ok())
        .and_then(|f| Changes::from_file(f).ok());
    let parsed = e
        .get(DOCUMENT)
        .ok_or_else(|| "document.json is missing".to_string())
        .and_then(|d| serde_json::from_slice::<Document>(d).map_err(|e| e.to_string()));
    let doc = match parsed {
        Ok(mut doc) => {
            let mut changes = table.clone().unwrap_or_default();
            let highest = doc
                .blocks
                .iter()
                .flat_map(|b| b.change_ids().collect::<Vec<_>>())
                .max();
            if table.is_none() || highest.is_some_and(|h| changes.get(h).is_none()) {
                if let Some(h) = highest {
                    changes.fill_to(h);
                }
                issues.push(issue(
                    "bundle_changes",
                    "fixed",
                    "changes.json is missing, unreadable or incomplete; the changes it lacks show the author \"Unknown\"".into(),
                ));
            }
            doc.changes = changes;
            doc.doc_id = doc_id.clone();
            // revisions.json, in a version 2 bundle (docs/specs/bundle.md
            // §5.1): the marks are kept even when the table is lost.
            if meta.as_ref().is_some_and(|m| m.v >= 2) || e.get(REVISIONS).is_some() {
                let read = e.get(REVISIONS).map(Revisions::from_json);
                let highest = doc
                    .blocks
                    .iter()
                    .flat_map(|b| b.rev_ids().collect::<Vec<_>>())
                    .max();
                let readable = matches!(read, Some(Ok(_)));
                let mut table = match read {
                    Some(Ok(t)) => t,
                    _ => Revisions::default(),
                };
                if !readable || highest.is_some_and(|h| table.get(h).is_none()) {
                    if let Some(h) = highest {
                        table.fill_to(h.0);
                    }
                    issues.push(issue(
                        "bundle_revisions",
                        "fixed",
                        "revisions.json is missing, unreadable or incomplete; the revision sets it lacks show the color \"Unknown\"".into(),
                    ));
                }
                doc.revisions = table;
            }
            match doc.check() {
                Ok(()) => Ok(doc),
                Err(why) => Err(why),
            }
        }
        Err(why) => Err(why),
    };
    let (doc, warnings) = match doc {
        Ok(doc) => (doc, Vec::new()),
        Err(why) => {
            // The readable copy, as a Fountain file.
            let script = e.get(SCRIPT).ok_or_else(|| {
                unreadable(format!(
                    "document.json cannot be used ({why}) and there is no script.fountain"
                ))
            })?;
            let read = Fountain.read(script)?;
            issues.push(issue(
                "bundle_document",
                "dropped",
                format!(
                    "document.json cannot be used ({why}); the script was read from script.fountain, so block ids, tracked changes and settings were lost"
                ),
            ));
            let mut doc = read.doc;
            doc.doc_id = doc_id;
            (doc, read.warnings)
        }
    };
    let repaired_issues: Vec<Value> = issues
        .iter()
        .filter(|i| i["class"] == "fixed" || i["class"] == "dropped")
        .cloned()
        .collect();
    let count = |class: &str| issues.iter().filter(|i| i["class"] == class).count();
    let repaired = (!repaired_issues.is_empty()).then(|| {
        json!({
            "fixed": count("fixed"),
            "dropped": count("dropped"),
            "issues": repaired_issues,
        })
    });
    let mut warnings = warnings;
    warnings.extend(issues.into_iter().filter(|i| i["class"] != "kept"));
    Ok(ReadOut {
        doc,
        style: SourceStyle::default(),
        warnings,
        repaired,
    })
}

/// A bundle's identity and change table, as recovery needs them to find
/// the table a journal header names (docs/specs/journal.md, "The change
/// data"). `None` when the bytes are not a readable bundle.
pub fn identity(bytes: &[u8]) -> Option<(Option<String>, Option<Changes>)> {
    let e = entries(bytes).ok()?;
    let doc_id = e
        .get(META)
        .and_then(|m| serde_json::from_slice::<Meta>(m).ok())
        .and_then(|m| m.doc_id);
    let table = e
        .get(CHANGES)
        .and_then(|c| serde_json::from_slice::<ChangesFile>(c).ok())
        .and_then(|f| Changes::from_file(f).ok());
    Some((doc_id, table))
}

/// The bundle format (docs/specs/bundle.md).
pub struct Bundle;

impl Format for Bundle {
    fn id(&self) -> FormatId {
        FormatId::Bundle
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["coldopen"]
    }

    fn read(&self, bytes: &[u8]) -> Result<ReadOut, CommandError> {
        read(bytes)
    }

    fn style(&self, _bytes: &[u8]) -> SourceStyle {
        SourceStyle::default()
    }

    fn write(&self, doc: &Document, _style: SourceStyle) -> crate::format::Written {
        crate::format::Written {
            bytes: write(doc),
            losses: Vec::new(),
        }
    }

    fn layout_form(&self, doc: &Document) -> crate::format::LayoutForm {
        // As the blocks stand (docs/specs/bundle.md §4).
        crate::format::Fdx.layout_form(doc)
    }
}

/// The highest change id a document's marks name.
pub fn highest_change(doc: &Document) -> Option<ChangeId> {
    doc.blocks
        .iter()
        .flat_map(|b| b.change_ids().collect::<Vec<_>>())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::author::Author;
    use crate::changes::ChangeBy;
    use crate::document::{Block, BlockId};
    use coldopen_screenplay::{Element, ElementKind, Marks, RichText, Run, Style};

    fn sample() -> Document {
        let mut doc = Document {
            next_id: 3,
            doc_id: Some("6f0f3c1e-8a52-4d6b-9a8e-0b8f4ac2d3e7".into()),
            ..Document::default()
        };
        let c = doc.changes.push(
            ChangeBy::of(&Author::agent("claude", None)),
            1760356801123,
            None,
        );
        doc.blocks.push(Block::new(
            BlockId(1),
            Element::new(ElementKind::SceneHeading, "INT. LAB - NIGHT"),
        ));
        let mut b = Block::new(
            BlockId(2),
            Element::new(
                ElementKind::Action,
                RichText::new(vec![
                    Run::plain("Rain "),
                    Run::marked(
                        "falls",
                        Style::PLAIN,
                        Marks {
                            ins: Some(c),
                            ..Marks::NONE
                        },
                    ),
                ]),
            ),
        );
        b.marks.inserted = Some(c);
        doc.blocks.push(b);
        doc
    }

    #[test]
    fn a_bundle_reads_back_as_it_was_written_byte_for_byte() {
        let doc = sample();
        let bytes = write(&doc);
        assert_eq!(write(&doc), bytes, "the same document, the same bytes");
        let back = read(&bytes).unwrap();
        assert!(back.repaired.is_none(), "{:?}", back.repaired);
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        assert_eq!(back.doc, doc);
        assert_eq!(write(&back.doc), bytes);
        let (id, table) = identity(&bytes).unwrap();
        assert_eq!(id, doc.doc_id);
        assert_eq!(table.unwrap(), doc.changes);
    }

    #[test]
    fn the_readable_copy_holds_the_accepted_script() {
        let bytes = write(&sample());
        let e = entries(&bytes).unwrap();
        let names: Vec<&str> = e.found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec![META, DOCUMENT, CHANGES, SCRIPT]);
        assert_eq!(
            std::str::from_utf8(e.get(SCRIPT).unwrap()).unwrap(),
            "INT. LAB - NIGHT\n\nRain falls\n"
        );
    }

    #[test]
    fn doc_ids_are_random_version_4_uuids() {
        let a = new_doc_id();
        let b = new_doc_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert!("89ab".contains(&a[19..20]));
    }

    /// Writes an archive with the given entries, stored, for damage tests.
    fn with_revisions() -> Document {
        use crate::revisions::{RevisionId, RevisionSet};
        let mut doc = sample();
        doc.revisions.sets.push(RevisionSet {
            id: RevisionId(1),
            color: "Blue".into(),
            label: "Blue Rev.".into(),
            date: "2026-10-22".into(),
            started: 1,
            ended: None,
        });
        doc.revisions.active = Some(RevisionId(1));
        doc.blocks[0].rev = Some(RevisionId(1));
        doc
    }

    fn meta_v(bytes: &[u8]) -> u32 {
        let e = entries(bytes).unwrap();
        serde_json::from_slice::<Meta>(e.get(META).unwrap())
            .unwrap()
            .v
    }

    #[test]
    fn revision_sets_make_a_version_2_bundle() {
        assert_eq!(meta_v(&write(&sample())), 1, "no revision data: version 1");
        let doc = with_revisions();
        let bytes = write(&doc);
        assert_eq!(meta_v(&bytes), 2);
        let e = entries(&bytes).unwrap();
        let names: Vec<&str> = e.found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec![META, DOCUMENT, CHANGES, REVISIONS, SCRIPT]);
        let json: Value = serde_json::from_slice(e.get(DOCUMENT).unwrap()).unwrap();
        assert!(
            json.get("revisions").is_none(),
            "the table is in revisions.json"
        );
        let back = read(&bytes).unwrap();
        assert!(back.repaired.is_none(), "{:?}", back.repaired);
        assert_eq!(back.doc, doc);
        assert_eq!(write(&back.doc), bytes);
    }

    #[test]
    fn a_damaged_revision_table_keeps_the_marks() {
        let doc = with_revisions();
        let bytes = write(&doc);
        let e = entries(&bytes).unwrap();
        let without: Vec<(&str, &[u8])> = e
            .found
            .iter()
            .filter(|(n, _)| n != REVISIONS)
            .map(|(n, d)| (n.as_str(), d.as_slice()))
            .collect();
        let back = read(&archive(&without)).unwrap();
        let repaired = back.repaired.unwrap();
        assert!(
            repaired.to_string().contains("bundle_revisions"),
            "{repaired}"
        );
        assert_eq!(back.doc.blocks[0].rev, doc.blocks[0].rev);
        let set = &back.doc.revisions.sets[0];
        assert_eq!(set.color, "Unknown");
        assert!(back.doc.revisions.active.is_none());
        back.doc.check().unwrap();
    }

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut a = rawzip::ZipArchiveWriter::new(&mut out);
        for (name, data) in entries {
            let (mut entry, config) = a.new_file(*name).start().unwrap();
            let mut w = config.wrap(&mut entry);
            w.write_all(data).unwrap();
            let (_, desc) = w.finish().unwrap();
            entry.finish(desc).unwrap();
        }
        a.finish().unwrap();
        out
    }

    #[test]
    fn a_damaged_document_falls_back_to_the_readable_copy() {
        let bytes = archive(&[
            (
                META,
                br#"{"format":"coldopen-bundle","v":1,"app":"0","doc_id":"x"}"#,
            ),
            (DOCUMENT, b"{not json"),
            (SCRIPT, b"INT. LAB - NIGHT\n\nRain.\n"),
        ]);
        let r = read(&bytes).unwrap();
        assert_eq!(r.doc.blocks.len(), 2);
        assert_eq!(r.doc.doc_id.as_deref(), Some("x"));
        let rep = r.repaired.unwrap();
        assert_eq!(rep["dropped"], 1, "{rep}");
        assert_eq!(rep["issues"][0]["code"], "bundle_document");
    }

    #[test]
    fn missing_change_entries_are_kept_with_an_unknown_author() {
        let doc = sample();
        let doc_json = serde_json::to_vec(&doc).unwrap();
        let bytes = archive(&[(DOCUMENT, &doc_json)]);
        let r = read(&bytes).unwrap();
        assert_eq!(r.doc.blocks, doc.blocks);
        assert_eq!(r.doc.changes.len(), 1);
        assert_eq!(r.doc.changes.entries()[0].by, ChangeBy::unknown());
        assert!(r.doc.doc_id.is_some(), "a new identity");
        let codes: Vec<&str> = r.repaired.as_ref().unwrap()["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["code"].as_str().unwrap())
            .collect();
        assert_eq!(codes, vec!["bundle_meta", "bundle_changes"]);
    }

    #[test]
    fn a_newer_bundle_and_noise_are_refused() {
        let newer = archive(&[(META, br#"{"format":"coldopen-bundle","v":2,"app":"9"}"#)]);
        assert_eq!(read(&newer).unwrap_err().code, "unsupported_format");
        assert_eq!(read(b"not a zip").unwrap_err().code, "unsupported_format");
        assert_eq!(read(b"").unwrap_err().code, "unsupported_format");
        let empty = archive(&[]);
        assert_eq!(read(&empty).unwrap_err().code, "unsupported_format");
    }

    #[test]
    fn a_corrupted_entry_is_treated_as_missing() {
        let doc = sample();
        let mut bytes = write(&doc);
        // Flip a byte inside document.json's data.
        let at = bytes
            .windows(DOCUMENT.len())
            .position(|w| w == DOCUMENT.as_bytes())
            .unwrap()
            + DOCUMENT.len()
            + 4;
        bytes[at] ^= 0xff;
        let r = read(&bytes).unwrap();
        // The readable copy gives the text; the damage is reported.
        assert_eq!(r.doc.blocks.len(), 2);
        assert!(r.repaired.is_some());
    }

    /// `script.fountain` comes from the writer's quick path, which skips
    /// the checks a save needs; its bytes are still exactly a Fountain
    /// save's, for every Fountain and .fdx fixture and with pending changes
    /// (docs/specs/bundle.md §6).
    #[test]
    fn the_readable_copy_has_a_fountain_saves_bytes() {
        let save = |doc: &Document| Fountain.write(doc, SourceStyle::default()).bytes;
        let doc = sample();
        assert!(doc.has_changes());
        assert_eq!(crate::format::fountain_text(&doc), save(&doc));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let mut n = 0;
        for dir in ["fountain", "fdx", "journal", "app"] {
            let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
                continue;
            };
            for e in entries {
                let p = e.unwrap().path();
                let Some(format @ (FormatId::Fountain | FormatId::Fdx)) = FormatId::for_path(&p)
                else {
                    continue;
                };
                let Ok(read) = format.get().read(&std::fs::read(&p).unwrap()) else {
                    continue;
                };
                assert_eq!(
                    crate::format::fountain_text(&read.doc),
                    save(&read.doc),
                    "{}",
                    p.display()
                );
                n += 1;
            }
        }
        assert!(n > 60, "{n} fixtures");
    }
}
