//! The editable document: screenplay elements with stable ids.

use std::collections::BTreeMap;
use std::fmt;

use coldopen_screenplay::{
    Element, ElementKind, ElementRepr, PageFormat, Pt, Script, TitleLine, TitlePage,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::changes::{BlockMarks, Changes, Tracking};
use crate::revisions::{RevisionId, Revisions};

/// A block's stable id. Opaque to clients; written as `b_<n>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u64);

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b_{}", self.0)
    }
}

impl BlockId {
    pub fn parse(s: &str) -> Option<BlockId> {
        let n = s.strip_prefix("b_")?;
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        n.parse().ok().map(BlockId)
    }
}

impl Serialize for BlockId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for BlockId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "BlockId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A block id, such as \"b_12\".",
            "type": "string",
            "pattern": "^b_[0-9]+$"
        })
    }
}

impl<'de> Deserialize<'de> for BlockId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<BlockId, D::Error> {
        let s = String::deserialize(d)?;
        BlockId::parse(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("\"{s}\" is not a block id")))
    }
}

/// One element of the document with its id, its structural tracked
/// changes (docs/specs/changes.md §2.2), and the revision set that last
/// changed the whole block (docs/specs/revisions.md §4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub element: Element,
    pub marks: BlockMarks,
    pub rev: Option<RevisionId>,
}

impl Block {
    /// A block with no marks.
    pub fn new(id: BlockId, element: Element) -> Block {
        Block {
            id,
            element,
            marks: BlockMarks::default(),
            rev: None,
        }
    }
}

#[derive(Serialize)]
struct BlockOut<'a> {
    id: BlockId,
    #[serde(flatten)]
    element: ElementRepr,
    #[serde(skip_serializing_if = "BlockMarks::is_empty")]
    change: &'a BlockMarks,
    #[serde(skip_serializing_if = "Option::is_none")]
    rev: Option<RevisionId>,
}

impl Serialize for Block {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BlockOut {
            id: self.id,
            element: ElementRepr::from_element(&self.element),
            change: &self.marks,
            rev: self.rev,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Block {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Block, D::Error> {
        use serde::de::Error;
        let mut map = serde_json::Map::deserialize(d)?;
        let id = map
            .remove("id")
            .ok_or_else(|| D::Error::custom("a block needs an \"id\""))?;
        let id: BlockId = serde_json::from_value(id).map_err(D::Error::custom)?;
        let marks: BlockMarks = match map.remove("change") {
            Some(v) => serde_json::from_value(v).map_err(D::Error::custom)?,
            None => BlockMarks::default(),
        };
        let rev: Option<RevisionId> = match map.remove("rev") {
            Some(v) => Some(serde_json::from_value(v).map_err(D::Error::custom)?),
            None => None,
        };
        let element: Element =
            serde_json::from_value(serde_json::Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Block {
            id,
            element,
            marks,
            rev,
        })
    }
}

/// The document being edited. Changed only through [`crate::ops::Op`]s.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    #[serde(default)]
    pub title_page: TitlePage,
    #[serde(default)]
    pub blocks: Vec<Block>,
    /// The next id to hand out. Ids are never reused.
    pub next_id: u64,
    /// The page and element settings of a file that has its own, such as
    /// an .fdx file; `None` is the default format. Every field from here on
    /// is left out of the JSON when empty, so Fountain journals are as
    /// before (docs/specs/journal.md, "The document in the header").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_format: Option<Box<PageFormat>>,
    /// A title page given as positioned lines (from .fdx), used instead of
    /// the title-page keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title_lines: Vec<TitleLine>,
    /// Paragraph spacing overrides: the space above a block, replacing its
    /// usual spacing (docs/specs/layout.md §6). Keyed by id, so edits never
    /// move them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub spacing: BTreeMap<BlockId, Pt>,
    /// Everything an .fdx file holds that the blocks do not, kept to write
    /// the file back (docs/specs/fdx.md §13). Its block ids are this
    /// document's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fdx: Option<Box<coldopen_fdx::Kept>>,
    /// The tracking switches (docs/specs/changes.md §4); absent means the
    /// defaults.
    #[serde(default, skip_serializing_if = "Tracking::is_default")]
    pub tracking: Tracking,
    /// The revision table (docs/specs/revisions.md §3); absent when it is
    /// the default. A bundle keeps it in `revisions.json` instead
    /// (docs/specs/bundle.md §5.1).
    #[serde(default, skip_serializing_if = "Revisions::is_default")]
    pub revisions: Revisions,
    /// The change table. Not part of the document's JSON: a journal header
    /// holds or names it, and a bundle keeps it in `changes.json`
    /// (docs/specs/journal.md, "The change data").
    #[serde(skip)]
    pub changes: Changes,
    /// A bundle's identity apart from its path (docs/specs/bundle.md §8).
    /// Kept in `meta.json` and the journal header, not in the JSON.
    #[serde(skip)]
    pub doc_id: Option<String>,
    /// Entries another tool added to the bundle the document was read
    /// from, by name, kept to write back (docs/specs/bundle.md §2,
    /// 2026-10-28). Not in the JSON: recovery takes them from the bundle on
    /// disk.
    #[serde(skip)]
    pub extra_entries: Vec<(String, Vec<u8>)>,
}

/// A scene derived from the block list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    /// Index of the heading block.
    pub start: usize,
    /// One past the last block of the scene.
    pub end: usize,
}

impl Document {
    /// Builds a document from a plain script, numbering blocks from `b_1`.
    pub fn from_script(script: Script) -> Document {
        let blocks: Vec<Block> = script
            .elements
            .into_iter()
            .enumerate()
            .map(|(i, element)| Block::new(BlockId(i as u64 + 1), element))
            .collect();
        Document {
            title_page: script.title_page,
            next_id: blocks.len() as u64 + 1,
            blocks,
            ..Document::default()
        }
    }

    pub fn to_script(&self) -> Script {
        Script {
            title_page: self.title_page.clone(),
            elements: self.blocks.iter().map(|b| b.element.clone()).collect(),
        }
    }

    pub fn index_of(&self, id: BlockId) -> Option<usize> {
        self.blocks.iter().position(|b| b.id == id)
    }

    pub fn get(&self, id: BlockId) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Hands out a fresh id, or `None` when every id has been used.
    pub fn allocate_id(&mut self) -> Option<BlockId> {
        let id = BlockId(self.next_id);
        self.next_id = self.next_id.checked_add(1)?;
        Some(id)
    }

    /// Numbers the blocks again from `first`, for a document read back into
    /// a session that has already handed out ids below it, and every
    /// reference to a block id with them: spacing, the .fdx store, pending
    /// moves' places and the active revision set's starting points
    /// (2026-10-28, review finding GP9). An id no block has keeps its
    /// number, which is never handed out again. `None` if the ids would
    /// run out.
    pub fn renumbered_from(mut self, first: u64) -> Option<Document> {
        let mut next = first;
        let mut spacing = BTreeMap::new();
        let mut ids = BTreeMap::new();
        for b in &mut self.blocks {
            let id = BlockId(next);
            if let Some(&pt) = self.spacing.get(&b.id) {
                spacing.insert(id, pt);
            }
            ids.insert(b.id.0, id.0);
            b.id = id;
            next = next.checked_add(1)?;
        }
        self.spacing = spacing;
        let map = |old: u64| ids.get(&old).copied().unwrap_or(old);
        let block = |old: BlockId| BlockId(map(old.0));
        if let Some(kept) = self.fdx.take() {
            self.fdx = Some(Box::new(kept.renumbered(&map)));
        }
        for b in &mut self.blocks {
            for m in &mut b.marks.moved {
                m.before = m.before.map(block);
                m.last = block(m.last);
            }
        }
        self.revisions.base = std::mem::take(&mut self.revisions.base)
            .into_iter()
            .map(|(id, base)| (block(id), base))
            .collect();
        self.next_id = next.max(first);
        (self.next_id < u64::MAX).then_some(self)
    }

    /// Checks what a document read from outside (a journal header) must
    /// hold before anything trusts it: ids are unique and below `next_id`,
    /// and every block keeps the model rules.
    pub fn check(&self) -> Result<(), String> {
        if self.next_id == u64::MAX {
            return Err("next_id leaves no id to hand out".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for b in &self.blocks {
            if !seen.insert(b.id) {
                return Err(format!("the id {} is used twice", b.id));
            }
            if b.id.0 >= self.next_id {
                return Err(format!(
                    "the id {} is not below next_id {}",
                    b.id, self.next_id
                ));
            }
            let problems = b.element.problems();
            if !problems.is_empty() {
                return Err(format!(
                    "block {} breaks a model rule: {}",
                    b.id,
                    problems.join("; ")
                ));
            }
            // Every mark names a change the table has (docs/specs/bundle.md
            // §4), and print-only marks never reach a document.
            if let Some(c) = b.change_ids().find(|&c| self.changes.get(c).is_none()) {
                return Err(format!(
                    "block {} names {c}, which the change table does not have",
                    b.id
                ));
            }
            if b.element.text.runs().iter().any(|r| r.marks().show) {
                return Err(format!("block {} holds a print-only mark", b.id));
            }
            // Every revision mark names a set the table has
            // (docs/specs/revisions.md §3).
            if let Some(r) = b.rev_ids().find(|&r| self.revisions.get(r).is_none()) {
                return Err(format!(
                    "block {} names {r}, which the revision table does not have",
                    b.id
                ));
            }
            if b.marks.moved.iter().any(|m| {
                m.before.is_some_and(|id| id.0 >= self.next_id) || m.last.0 >= self.next_id
            }) {
                return Err(format!(
                    "block {}'s move names a block not below next_id",
                    b.id
                ));
            }
        }
        // What layout is given must be what layout accepts.
        let format = self.page_format.as_deref().cloned().unwrap_or_default();
        let problems = format.problems();
        if !problems.is_empty() {
            return Err(format!(
                "the page format is invalid: {}",
                problems.join("; ")
            ));
        }
        let body = format.body_height();
        for (id, pt) in &self.spacing {
            if id.0 >= self.next_id {
                return Err(format!("spacing names {id}, which is not below next_id"));
            }
            if *pt < Pt::ZERO || *pt > body {
                return Err(format!("the spacing of {id} is outside the page body"));
            }
        }
        for line in &self.title_lines {
            if line.space_before < Pt::ZERO || line.space_before > body {
                return Err("a title line's space above is outside the page body".into());
            }
        }
        if let Some(kept) = &self.fdx
            && kept.paragraphs.keys().any(|&id| id >= self.next_id)
        {
            return Err("the .fdx store names a block id not below next_id".into());
        }
        Ok(())
    }

    /// The layout overrides for a script laid out from this document:
    /// `map[k]` is the block laid-out element `k` starts in, and an element
    /// gets its block's spacing when it is the first one from that block.
    pub fn overrides(&self, map: &[usize]) -> BTreeMap<usize, Pt> {
        let mut out = BTreeMap::new();
        if self.spacing.is_empty() {
            return out;
        }
        let mut last = None;
        for (k, &b) in map.iter().enumerate() {
            if last != Some(b)
                && let Some(&pt) = self.blocks.get(b).and_then(|blk| self.spacing.get(&blk.id))
            {
                out.insert(k, pt);
            }
            last = Some(b);
        }
        out
    }

    /// Scenes in order. A scene runs from its heading to the next heading
    /// or section.
    pub fn scenes(&self) -> Vec<Scene> {
        let mut out = Vec::new();
        let n = self.blocks.len();
        for (i, b) in self.blocks.iter().enumerate() {
            if b.element.kind != ElementKind::SceneHeading {
                continue;
            }
            let end = (i + 1..n)
                .find(|&j| {
                    matches!(
                        self.blocks[j].element.kind,
                        ElementKind::SceneHeading | ElementKind::Section
                    )
                })
                .unwrap_or(n);
            out.push(Scene { start: i, end });
        }
        out
    }

    /// The scene whose heading has this id.
    pub fn scene(&self, id: BlockId) -> Option<Scene> {
        let i = self.index_of(id)?;
        self.scenes().into_iter().find(|s| s.start == i)
    }
}

/// Number of words in an element's printed text.
pub fn word_count(e: &Element) -> usize {
    if e.kind.is_transparent() {
        return 0;
    }
    e.text.plain_text().split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review finding C7: allocation added without checking, so a huge
    /// `next_id` overflowed (a panic in debug builds).
    #[test]
    fn allocation_never_overflows() {
        let mut doc = Document {
            next_id: u64::MAX - 1,
            ..Default::default()
        };
        assert_eq!(doc.allocate_id(), Some(BlockId(u64::MAX - 1)));
        assert_eq!(doc.allocate_id(), None);
        assert_eq!(doc.next_id, u64::MAX);
        assert!(doc.check().is_err());
    }

    fn doc(kinds: &[ElementKind]) -> Document {
        Document::from_script(Script {
            title_page: TitlePage::default(),
            elements: kinds.iter().map(|&k| Element::new(k, "x")).collect(),
        })
    }

    #[test]
    fn ids_round_trip() {
        assert_eq!(BlockId::parse("b_12"), Some(BlockId(12)));
        assert_eq!(BlockId::parse("b_"), None);
        assert_eq!(BlockId::parse("b_-1"), None);
        assert_eq!(BlockId::parse("12"), None);
        assert_eq!(BlockId(7).to_string(), "b_7");
    }

    #[test]
    fn block_json() {
        let mut b = Block::new(
            BlockId(3),
            Element::new(ElementKind::Character, "MAYA").dual(),
        );
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(
            json,
            r#"{"id":"b_3","type":"character","text":"MAYA","dual":true}"#
        );
        assert_eq!(serde_json::from_str::<Block>(&json).unwrap(), b);
        b.marks.inserted = Some(crate::changes::ChangeId(2));
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(
            json,
            r#"{"id":"b_3","type":"character","text":"MAYA","dual":true,"change":{"inserted":"c_2"}}"#
        );
        assert_eq!(serde_json::from_str::<Block>(&json).unwrap(), b);
    }

    #[test]
    fn marks_must_name_changes_the_table_has() {
        use crate::changes::{ChangeBy, ChangeId};
        let mut d = doc(&[ElementKind::Action]);
        d.blocks[0].marks.deleted = Some(ChangeId(1));
        assert!(d.check().is_err());
        d.changes.push(ChangeBy::unknown(), 0, None);
        assert!(d.check().is_ok());
    }

    #[test]
    fn scenes_stop_at_headings_and_sections() {
        use ElementKind::*;
        let d = doc(&[
            Action,
            SceneHeading,
            Action,
            Section,
            SceneHeading,
            Dialogue,
        ]);
        assert_eq!(
            d.scenes(),
            vec![Scene { start: 1, end: 3 }, Scene { start: 4, end: 6 }]
        );
    }

    #[test]
    fn snapshot_round_trip() {
        use ElementKind::*;
        let d = doc(&[SceneHeading, Action]);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Document>(&json).unwrap(), d);
    }
}
