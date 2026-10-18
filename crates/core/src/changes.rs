//! Tracked changes (docs/specs/changes.md): the change table, block marks,
//! the tracking switches, and the two print forms of a document.

use coldopen_screenplay::{ElementKind, Marks, RichText, Run, Settle};
use serde::{Deserialize, Serialize};

use crate::author::Author;
use crate::document::{Block, BlockId, Document};
use crate::io::Fingerprint;

pub use coldopen_screenplay::ChangeId;

/// The name a writer's changes show when the writer has none
/// (docs/specs/changes.md §3).
pub const WRITER: &str = "Writer";

/// Who made a change, as the change table records it: the author without
/// the MCP tool, since a change can span several tools.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeBy {
    pub kind: ByKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByKind {
    Writer,
    Agent,
}

impl ChangeBy {
    pub fn of(author: &Author) -> ChangeBy {
        match author {
            Author::Writer { name } => ChangeBy {
                kind: ByKind::Writer,
                name: name.clone(),
            },
            Author::Agent { name, .. } => ChangeBy {
                kind: ByKind::Agent,
                name: Some(name.clone()),
            },
        }
    }

    /// The author of a change whose entry was lost (docs/specs/bundle.md
    /// §7): treated as a writer's, which is the cautious choice.
    pub fn unknown() -> ChangeBy {
        ChangeBy {
            kind: ByKind::Writer,
            name: Some("Unknown".into()),
        }
    }

    /// Whether `author` made this change: the same kind and name.
    pub fn is(&self, author: &Author) -> bool {
        *self == ChangeBy::of(author)
    }

    pub fn is_writer(&self) -> bool {
        self.kind == ByKind::Writer
    }

    /// The name shown for the author.
    pub fn display(&self) -> &str {
        match (&self.name, self.kind) {
            (Some(n), _) => n,
            (None, ByKind::Writer) => WRITER,
            (None, ByKind::Agent) => "agent",
        }
    }
}

/// One entry of the change table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub id: ChangeId,
    pub by: ChangeBy,
    /// Milliseconds since the Unix epoch.
    pub t: u64,
}

/// The change table: entries `c_1`, `c_2`, … with no gaps. Append-only:
/// entries are never removed or rewritten (docs/specs/changes.md §3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes(Vec<Change>);

/// The table's JSON form, as `changes.json` and a journal header hold it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangesFile {
    pub v: u32,
    pub changes: Vec<Change>,
}

/// The version of the table's JSON form.
pub const CHANGES_V: u32 = 1;

impl Changes {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn entries(&self) -> &[Change] {
        &self.0
    }

    pub fn get(&self, id: ChangeId) -> Option<&Change> {
        let i = usize::try_from(id.0).ok()?.checked_sub(1)?;
        self.0.get(i)
    }

    /// Adds an entry and returns its id.
    pub fn push(&mut self, by: ChangeBy, t: u64) -> ChangeId {
        let id = ChangeId(self.0.len() as u64 + 1);
        self.0.push(Change { id, by, t });
        id
    }

    /// Takes back entries a failed or empty edit added before it was
    /// journaled: nothing outside the edit ever saw them.
    pub(crate) fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }

    /// The table from entries read back, which must be numbered from 1
    /// with no gaps.
    pub fn from_entries(entries: Vec<Change>) -> Result<Changes, String> {
        for (i, c) in entries.iter().enumerate() {
            if c.id != ChangeId(i as u64 + 1) {
                return Err(format!(
                    "the change table's entry {} is {}, not c_{}",
                    i + 1,
                    c.id,
                    i + 1
                ));
            }
        }
        Ok(Changes(entries))
    }

    /// The JSON form, `{"v": 1, "changes": [...]}`.
    pub fn to_file(&self) -> ChangesFile {
        ChangesFile {
            v: CHANGES_V,
            changes: self.0.clone(),
        }
    }

    pub fn from_file(f: ChangesFile) -> Result<Changes, String> {
        if f.v != CHANGES_V {
            return Err(format!(
                "the change table is version {}; this version reads {CHANGES_V}",
                f.v
            ));
        }
        Changes::from_entries(f.changes)
    }

    /// The fingerprint of the first `count` entries written as one compact
    /// JSON array, as a journal header names a bundle's table
    /// (docs/specs/journal.md, "The change data").
    pub fn fingerprint(&self, count: usize) -> Option<Fingerprint> {
        let entries = self.0.get(..count)?;
        let bytes = serde_json::to_vec(entries).ok()?;
        Some(Fingerprint::of(&bytes))
    }

    /// The first `count` entries, as their own table.
    pub fn prefix(&self, count: usize) -> Option<Changes> {
        self.0.get(..count).map(|e| Changes(e.to_vec()))
    }

    /// Adds entries for changes up to `id` that the table lacks, with an
    /// unknown author and time 0 (docs/specs/bundle.md §7).
    pub fn fill_to(&mut self, id: ChangeId) {
        while (self.0.len() as u64) < id.0 {
            self.push(ChangeBy::unknown(), 0);
        }
    }
}

/// The tracking switches (docs/specs/changes.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tracking {
    /// Track the writer's edits. Off by default.
    #[serde(default)]
    pub writer: bool,
    /// Track every agent's edits. On by default.
    #[serde(default = "yes")]
    pub agents: bool,
}

fn yes() -> bool {
    true
}

impl Default for Tracking {
    fn default() -> Tracking {
        Tracking {
            writer: false,
            agents: true,
        }
    }
}

impl Tracking {
    pub fn is_default(&self) -> bool {
        *self == Tracking::default()
    }

    /// Whether an author's edits are tracked when the edit does not say.
    pub fn for_author(&self, by: &Author) -> bool {
        if by.is_writer() {
            self.writer
        } else {
            self.agents
        }
    }
}

/// A block's type before a tracked retype, with the attributes the retype
/// cleared.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retyped {
    pub change: ChangeId,
    pub from: ElementKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_number: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dual: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub centered: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub depth: u8,
}

/// Where a moved scene stood: before `before`, or at the end; and the last
/// block the move took, so undoing it moves the same blocks back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Moved {
    pub change: ChangeId,
    pub before: Option<BlockId>,
    pub last: BlockId,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// A block's structural marks (docs/specs/changes.md §2.2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockMarks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inserted: Option<ChangeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<ChangeId>,
    /// Each tracked retype, oldest first: they are undone newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retyped: Vec<Retyped>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<ChangeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joined: Option<ChangeId>,
    /// Each tracked move of the scene, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<Moved>,
}

impl BlockMarks {
    pub fn is_empty(&self) -> bool {
        *self == BlockMarks::default()
    }

    /// Every change the marks name.
    pub fn ids(&self) -> impl Iterator<Item = ChangeId> + '_ {
        [self.inserted, self.deleted, self.split, self.joined]
            .into_iter()
            .flatten()
            .chain(self.retyped.iter().map(|r| r.change))
            .chain(self.moved.iter().map(|m| m.change))
    }
}

/// Every change id the text's runs name.
pub fn text_ids(text: &RichText) -> impl Iterator<Item = ChangeId> + '_ {
    text.runs()
        .iter()
        .flat_map(|r| {
            let m = r.marks();
            [m.ins, m.del]
        })
        .flatten()
}

impl Block {
    /// Every change this block's marks and text name.
    pub fn change_ids(&self) -> impl Iterator<Item = ChangeId> + '_ {
        self.marks.ids().chain(text_ids(&self.element.text))
    }

    pub fn has_marks(&self) -> bool {
        !self.marks.is_empty() || self.element.text.has_marks()
    }
}

impl Document {
    /// Whether any block holds a pending change.
    pub fn has_changes(&self) -> bool {
        self.blocks.iter().any(Block::has_marks)
    }

    /// The pending changes, in document order of their first mark.
    pub fn pending(&self) -> Vec<ChangeId> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for b in &self.blocks {
            for id in b.change_ids() {
                if seen.insert(id) {
                    out.push(id);
                }
            }
        }
        out
    }

    /// The document as accepting every change would leave it, with no
    /// marks (docs/specs/changes.md §9). Joined blocks are joined, deleted
    /// blocks and text left out.
    pub fn accepted(&self) -> Document {
        let mut out = Document {
            blocks: Vec::with_capacity(self.blocks.len()),
            ..self.clone()
        };
        for b in &self.blocks {
            if b.marks.deleted.is_some() {
                continue;
            }
            let text = b.element.text.settle(Settle::Accept, &|_| true);
            if b.marks.joined.is_some()
                && let Some(prev) = out.blocks.last_mut()
            {
                prev.element.text = prev.element.text.concat(&text);
                continue;
            }
            let mut element = b.element.clone();
            element.text = text;
            out.blocks.push(Block::new(b.id, element));
        }
        out
    }

    /// The document as its marked print form shows it: blocks as they
    /// stand, pending deletions printed (docs/specs/changes.md §9).
    pub fn marked(&self) -> Document {
        let mut out = self.clone();
        for b in &mut out.blocks {
            if b.element.text.has_marks() {
                b.element.text = b.element.text.shown();
            }
        }
        out
    }
}

/// The print form a caller wants of a document with pending changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PrintForm {
    /// Pending changes as the editor shows them.
    #[default]
    Marked,
    /// Every change accepted.
    Accepted,
}

/// Rich text marked as inserted by `change`, for a tracked new block.
pub fn mark_inserted(text: &RichText, change: ChangeId) -> RichText {
    let runs = text
        .runs()
        .iter()
        .map(|r| match r {
            Run::Text { text, style, marks } if marks.del.is_none() => Run::marked(
                text.clone(),
                *style,
                Marks {
                    ins: Some(change),
                    ..*marks
                },
            ),
            other => other.clone(),
        })
        .collect();
    RichText::new(runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use coldopen_screenplay::{Element, Style};

    #[test]
    fn the_table_is_numbered_from_one() {
        let mut t = Changes::default();
        let a = t.push(ChangeBy::of(&Author::writer()), 5);
        let b = t.push(ChangeBy::of(&Author::agent("claude", None)), 6);
        assert_eq!((a, b), (ChangeId(1), ChangeId(2)));
        assert_eq!(t.get(ChangeId(2)).unwrap().by.display(), "claude");
        assert_eq!(t.get(ChangeId(1)).unwrap().by.display(), "Writer");
        assert!(t.get(ChangeId(3)).is_none());
        let json = serde_json::to_string(&t.to_file()).unwrap();
        assert_eq!(
            json,
            r#"{"v":1,"changes":[{"id":"c_1","by":{"kind":"writer"},"t":5},{"id":"c_2","by":{"kind":"agent","name":"claude"},"t":6}]}"#
        );
        let back = Changes::from_file(serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(back, t);
        // A gap is refused.
        let mut gap = t.entries().to_vec();
        gap.remove(0);
        assert!(Changes::from_entries(gap).is_err());
        // The fingerprint of a prefix stays the same as the table grows.
        let fp = t.fingerprint(2).unwrap();
        t.push(ChangeBy::unknown(), 7);
        assert_eq!(t.fingerprint(2), Some(fp));
        assert_ne!(t.fingerprint(3), Some(fp));
        assert!(t.fingerprint(4).is_none());
    }

    #[test]
    fn block_marks_json() {
        let m = BlockMarks {
            inserted: Some(ChangeId(5)),
            retyped: vec![Retyped {
                change: ChangeId(7),
                from: ElementKind::Action,
                scene_number: None,
                dual: false,
                centered: true,
                depth: 0,
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(
            json,
            r#"{"inserted":"c_5","retyped":[{"change":"c_7","from":"action","centered":true}]}"#
        );
        assert_eq!(serde_json::from_str::<BlockMarks>(&json).unwrap(), m);
        assert_eq!(m.ids().collect::<Vec<_>>(), vec![ChangeId(5), ChangeId(7)]);
    }

    #[test]
    fn print_forms() {
        let del = Marks {
            del: Some(ChangeId(1)),
            ..Marks::NONE
        };
        let mut doc = Document::from_script(coldopen_screenplay::Script {
            title_page: Default::default(),
            elements: vec![
                Element::new(
                    ElementKind::Action,
                    RichText::new(vec![
                        Run::plain("Rain "),
                        Run::marked("falls", Style::PLAIN, del),
                    ]),
                ),
                Element::new(ElementKind::Action, "Gone."),
                Element::new(ElementKind::Action, " again."),
            ],
        });
        doc.blocks[1].marks.deleted = Some(ChangeId(1));
        doc.blocks[2].marks.joined = Some(ChangeId(2));
        let a = doc.accepted();
        assert_eq!(a.blocks.len(), 1);
        assert_eq!(a.blocks[0].element.text.plain_text(), "Rain  again.");
        assert!(!a.has_changes());
        let m = doc.marked();
        assert_eq!(m.blocks[0].element.text.plain_text(), "Rain falls");
        assert_eq!(doc.pending(), vec![ChangeId(1), ChangeId(2)]);
    }
}
