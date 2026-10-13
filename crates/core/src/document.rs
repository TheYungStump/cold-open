//! The editable document: screenplay elements with stable ids.

use std::fmt;

use coldopen_screenplay::{Element, ElementKind, ElementRepr, Script, TitlePage};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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

impl<'de> Deserialize<'de> for BlockId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<BlockId, D::Error> {
        let s = String::deserialize(d)?;
        BlockId::parse(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("\"{s}\" is not a block id")))
    }
}

/// One element of the document with its id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub element: Element,
}

#[derive(Serialize)]
struct BlockOut {
    id: BlockId,
    #[serde(flatten)]
    element: ElementRepr,
}

impl Serialize for Block {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BlockOut {
            id: self.id,
            element: ElementRepr::from_element(&self.element),
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
        let element: Element =
            serde_json::from_value(serde_json::Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Block { id, element })
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
            .map(|(i, element)| Block {
                id: BlockId(i as u64 + 1),
                element,
            })
            .collect();
        Document {
            title_page: script.title_page,
            next_id: blocks.len() as u64 + 1,
            blocks,
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

    /// Hands out a fresh id.
    pub fn allocate_id(&mut self) -> BlockId {
        let id = BlockId(self.next_id);
        self.next_id += 1;
        id
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
        let b = Block {
            id: BlockId(3),
            element: Element::new(ElementKind::Character, "MAYA").dual(),
        };
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(
            json,
            r#"{"id":"b_3","type":"character","text":"MAYA","dual":true}"#
        );
        assert_eq!(serde_json::from_str::<Block>(&json).unwrap(), b);
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
