//! Script diff in core (docs/specs/diff.md): reading the drafts, their
//! comparison form (§2), the `diff` result (§4), and the tracked edits that
//! bring a draft in (`changes.import`, §6). The comparison itself is
//! `coldopen_diff`.

use std::path::Path;

use coldopen_diff::{BlockOpKind, Draft, Edit, Title};
use coldopen_screenplay::{Element, ElementKind, RichText, Run, TitleLine, TitlePage};
use serde_json::{Value, json};

use crate::command::BatchItem;
use crate::document::Document;
use crate::error::CommandError;
use crate::format::FormatId;

/// A draft read from a file, as `doc.open` reads it, with no session or
/// journal (§2).
pub fn read_draft(path: &Path) -> Result<(Document, FormatId), CommandError> {
    let format = FormatId::for_path(path).ok_or_else(|| {
        CommandError::new(
            "unsupported_format",
            format!(
                "{} is not a .fountain, .fdx or .coldopen file",
                path.display()
            ),
        )
    })?;
    let bytes = std::fs::read(path).map_err(|e| CommandError::io(path, &e))?;
    Ok((format.get().read(&bytes)?.doc, format))
}

/// A document in comparison form (§2), with the block each element came
/// from.
pub struct Form {
    pub elements: Vec<Element>,
    /// The id of the block each element starts in.
    pub ids: Vec<String>,
    pub title_page: TitlePage,
    pub title_lines: Vec<TitleLine>,
    /// The kinds the document's page format prints in capitals.
    pub caps: Vec<ElementKind>,
    /// Each element is its own block, unchanged, so edits can name blocks
    /// one for one.
    pub exact: bool,
}

impl Form {
    fn draft(&self, keys: bool) -> Draft<'_> {
        Draft {
            elements: &self.elements,
            ids: &self.ids,
            title: if keys {
                Title::Keys(&self.title_page)
            } else {
                Title::Lines(&self.title_lines)
            },
            caps: &self.caps,
        }
    }
}

/// The accepted form of a document: what another reader would see.
fn accepted(doc: &Document) -> std::borrow::Cow<'_, Document> {
    if doc.has_changes() {
        std::borrow::Cow::Owned(doc.accepted())
    } else {
        std::borrow::Cow::Borrowed(doc)
    }
}

/// The kinds a document's page format prints in capitals.
fn caps(doc: &Document) -> Vec<ElementKind> {
    let format = crate::view::page_format(doc);
    ElementKind::ALL
        .into_iter()
        .filter(|&k| format.get().elements.get(k).is_some_and(|f| f.caps))
        .collect()
}

/// Equal as a block, spacing aside.
fn same_element(a: &Element, b: &Element) -> bool {
    a.kind == b.kind
        && a.text == b.text
        && a.scene_number == b.scene_number
        && a.dual == b.dual
        && a.centered == b.centered
        && a.depth == b.depth
}

/// A document's comparison form (§2): accepted; with the kinds Fountain
/// cannot hold as Fountain writes them, when `fountain`; in Fountain
/// normal form. **Resolution (2026-10-25):** "as Fountain holds it" is
/// these kind changes (a Shot reads back as a scene heading, General text
/// as action, `docs/specs/fountain.md` §9), not a write and a read, so each
/// element still knows its block.
pub fn comparison_form(doc: &Document, fountain: bool) -> Form {
    let doc = accepted(doc);
    let mut script = doc.to_script();
    if fountain {
        for e in &mut script.elements {
            e.kind = match e.kind {
                ElementKind::Shot => ElementKind::SceneHeading,
                ElementKind::General => ElementKind::Action,
                k => k,
            };
        }
    }
    let (norm, map) = coldopen_fountain::normalize_with_map(&script);
    let exact = norm.elements.len() == doc.blocks.len()
        && map.iter().enumerate().all(|(i, &s)| s == i)
        && norm
            .elements
            .iter()
            .zip(&doc.blocks)
            .all(|(e, b)| same_element(e, &b.element));
    let ids = map
        .iter()
        .map(|&s| {
            doc.blocks
                .get(s)
                .map(|b| b.id.to_string())
                .unwrap_or_default()
        })
        .collect();
    Form {
        elements: norm.elements,
        ids,
        title_page: doc.title_page.clone(),
        title_lines: doc.title_lines.clone(),
        caps: caps(&doc),
        exact,
    }
}

/// A document's blocks as they stand, accepted: the form edits are
/// planned on when the comparison form is not one block per element.
fn raw_form(doc: &Document) -> Form {
    let doc = accepted(doc);
    Form {
        elements: doc.blocks.iter().map(|b| b.element.clone()).collect(),
        ids: doc.blocks.iter().map(|b| b.id.to_string()).collect(),
        title_page: doc.title_page.clone(),
        title_lines: doc.title_lines.clone(),
        caps: caps(&doc),
        exact: true,
    }
}

/// Two drafts' forms, ready to compare: whether transparent elements are
/// compared, and whether the title pages are compared as keys (§3.5).
struct Pair {
    a: Form,
    b: Form,
    transparent: bool,
    keys: bool,
}

fn pair(a: Form, fa: FormatId, b: Form, fb: FormatId, fountain: bool) -> Pair {
    let transparent = fa != FormatId::Fdx && fb != FormatId::Fdx;
    let lines = |f: &Form| !f.title_lines.is_empty();
    // Lines are compared as lines only when both drafts have them and
    // neither is Fountain; otherwise they become keys, as a Fountain save
    // makes them.
    let keys = fountain || !(lines(&a) && lines(&b));
    let as_keys = |mut f: Form| {
        if keys && f.title_page.is_empty() && !f.title_lines.is_empty() {
            f.title_page = crate::format::title_keys(&f.title_lines);
        }
        f
    };
    Pair {
        a: as_keys(a),
        b: as_keys(b),
        transparent,
        keys,
    }
}

impl Pair {
    fn compare(&self) -> coldopen_diff::Diff {
        coldopen_diff::compare(
            &self.a.draft(self.keys),
            &self.b.draft(self.keys),
            self.transparent,
        )
    }
}

fn is_fountain(a: FormatId, b: FormatId) -> bool {
    a == FormatId::Fountain || b == FormatId::Fountain
}

/// What the result calls a draft: its path and format.
pub struct Named<'a> {
    pub doc: &'a Document,
    pub format: FormatId,
    pub path: Option<&'a Path>,
}

fn file_name(p: Option<&Path>) -> String {
    p.and_then(|p| p.file_name()).map_or_else(
        || "the open script".into(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The `diff` result (§4.1), with `text`, the summary (§4.2).
pub fn result(a: &Named<'_>, b: &Named<'_>) -> Value {
    let fountain = is_fountain(a.format, b.format);
    let pair = pair(
        comparison_form(a.doc, fountain),
        a.format,
        comparison_form(b.doc, fountain),
        b.format,
        fountain,
    );
    let d = pair.compare();
    let text = coldopen_diff::summary_text(&d, &file_name(a.path), &file_name(b.path));
    let mut v = serde_json::to_value(&d).unwrap_or_default();
    let side = |n: &Named<'_>| {
        json!({
            "path": n.path.map(|p| p.display().to_string()),
            "format": n.format.name(),
        })
    };
    if let Value::Object(m) = &mut v {
        let mut out = serde_json::Map::new();
        out.insert("v".into(), json!(1));
        out.insert("a".into(), side(a));
        out.insert("b".into(), side(b));
        out.extend(std::mem::take(m));
        out.insert("text".into(), json!(text));
        return Value::Object(out);
    }
    v
}

/// An element as an edit carries it: hidden runs left out when the other
/// draft cannot hold them.
fn runs(e: &Element, transparent: bool) -> Vec<Run> {
    e.text
        .runs()
        .iter()
        .filter(|r| transparent || !r.is_hidden())
        .cloned()
        .collect()
}

/// The tracked edits that bring the draft `b` into `doc` (§6), as `batch`
/// items, and what cannot be brought in. The edits are planned on the
/// comparison form when each of its elements is a block, and on the blocks
/// as they stand otherwise.
pub fn import_edits(
    doc: &Document,
    fa: FormatId,
    b: &Document,
    fb: FormatId,
) -> (Vec<BatchItem>, Vec<Value>) {
    let fountain = is_fountain(fa, fb);
    let a_form = comparison_form(doc, fountain);
    let a_form = if a_form.exact { a_form } else { raw_form(doc) };
    let pair = pair(a_form, fa, comparison_form(b, fountain), fb, fountain);
    let d = pair.compare();
    let mut skipped: Vec<Value> = Vec::new();
    for s in &d.scenes {
        for op in &s.blocks {
            // A changed or retyped block keeps its attributes too, which
            // are never tracked (found by a local run of the `diff` check,
            // 2026-10-26).
            let attributes = op.op == BlockOpKind::Changed
                && match (&op.a, &op.b) {
                    (Some(x), Some(y)) => {
                        other_attributes(&pair.a.elements[x.at], &pair.b.elements[y.at])
                    }
                    _ => false,
                };
            if matches!(op.op, BlockOpKind::Restyled | BlockOpKind::Attributes) || attributes {
                skipped.push(json!({
                    "op": if attributes { BlockOpKind::Attributes } else { op.op },
                    "id": op.a.as_ref().map(|r| r.id.clone()),
                }));
            }
        }
    }
    for t in &d.title_page {
        skipped.push(json!({ "title_page": t }));
    }
    let (ea, eb) = (&pair.a, &pair.b);
    let id = |i: usize| json!(ea.ids[i]);
    let mut restyled: Vec<Value> = Vec::new();
    let item = |command: &str, params: Value| BatchItem {
        command: command.into(),
        params,
    };
    let items = d
        .edits
        .iter()
        .map(|e| match *e {
            Edit::Move { unit, before, last } => item(
                "scene.move",
                with_before(
                    json!({ "id": id(unit), "last": id(last), "tracked": true }),
                    before.map(id),
                ),
            ),
            Edit::Delete { a } => item("block.delete", json!({ "id": id(a), "tracked": true })),
            Edit::Retype { a, b } => item(
                "block.set_type",
                json!({ "id": id(a), "type": eb.elements[b].kind, "tracked": true }),
            ),
            Edit::Retext { a, b } => {
                // Words both drafts have keep A's style, and A's notes and
                // boneyard stay: neither can be tracked, so what differs is
                // listed as skipped instead.
                let want = RichText::new(runs(&eb.elements[b], pair.transparent));
                let kept = crate::track::import_text(&ea.elements[a].text, &want);
                let split = |t: &RichText, hidden: bool| -> Vec<Run> {
                    t.runs()
                        .iter()
                        .filter(|r| r.is_hidden() == hidden)
                        .cloned()
                        .collect()
                };
                if split(&kept, false) != split(&want, false) {
                    restyled.push(json!({ "op": "restyled", "id": ea.ids[a] }));
                }
                if split(&kept, true) != split(&want, true) {
                    restyled.push(json!({ "op": "notes", "id": ea.ids[a] }));
                }
                item(
                    "block.set_text",
                    json!({ "id": id(a), "runs": kept.runs(), "tracked": true }),
                )
            }
            Edit::Insert { b, before } => {
                let e = &eb.elements[b];
                let mut p = json!({
                    "type": e.kind,
                    "runs": runs(e, pair.transparent),
                    "tracked": true,
                });
                if let Some(n) = &e.scene_number {
                    p["scene_number"] = json!(n);
                }
                if e.dual {
                    p["dual"] = json!(true);
                }
                if e.centered {
                    p["centered"] = json!(true);
                }
                // The blank lines above it too: without them, two speeches
                // or lyrics in a row read back as one block.
                if let Some(n) = e.space_before {
                    p["space_before"] = json!(n);
                }
                if e.kind == ElementKind::Section {
                    p["depth"] = json!(e.depth);
                }
                item("block.insert", with_before(p, before.map(id)))
            }
        })
        .collect();
    skipped.extend(restyled);
    (items, skipped)
}

/// Whether `a`, given `b`'s kind, would print with other attributes than
/// `b`: a scene number, dual dialogue, centering or a section's depth.
fn other_attributes(a: &Element, b: &Element) -> bool {
    let mut e = a.clone();
    e.kind = b.kind;
    e.clear_inapplicable();
    (e.scene_number.as_ref(), e.dual, e.centered, e.depth)
        != (b.scene_number.as_ref(), b.dual, b.centered, b.depth)
}

fn with_before(mut p: Value, before: Option<Value>) -> Value {
    if let Some(b) = before {
        p["before"] = b;
    }
    p
}

/// B with the changes marked (§4.3): A, accepted, with B brought in as the
/// tracked changes of a writer called `name`. Built for printing and
/// showing only; never saved or journaled.
pub fn marked(
    a: &Document,
    fa: FormatId,
    b: &Document,
    fb: FormatId,
    name: &str,
) -> Result<Document, CommandError> {
    let base = accepted(a).into_owned();
    let mut s = crate::session::Session::new(base, None, fa, crate::io::SourceStyle::default());
    let (items, _) = import_edits(&s.doc, fa, b, fb);
    if !items.is_empty() {
        let batch = crate::command::Command::Batch(crate::command::Batch {
            commands: items,
            label: None,
        });
        s.execute_with(
            &batch,
            &crate::session::Exec::by(crate::author::Author::named_writer(name)),
        )?;
    }
    Ok(s.doc)
}

/// The name a draft's changes carry by default: its file name without its
/// extension.
pub fn default_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Draft".into())
}
