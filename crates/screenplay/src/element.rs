//! Script elements: one paragraph of a screenplay each.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::text::{RichText, Run, TextRepr};

/// The kind of a script element. The serde names are part of the public
/// command API and must never change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    SceneHeading,
    Action,
    Character,
    Parenthetical,
    Dialogue,
    Transition,
    Lyric,
    Shot,
    General,
    PageBreak,
    Section,
    Synopsis,
    Note,
    Boneyard,
}

impl ElementKind {
    pub const ALL: [ElementKind; 14] = [
        ElementKind::SceneHeading,
        ElementKind::Action,
        ElementKind::Character,
        ElementKind::Parenthetical,
        ElementKind::Dialogue,
        ElementKind::Transition,
        ElementKind::Lyric,
        ElementKind::Shot,
        ElementKind::General,
        ElementKind::PageBreak,
        ElementKind::Section,
        ElementKind::Synopsis,
        ElementKind::Note,
        ElementKind::Boneyard,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ElementKind::SceneHeading => "scene_heading",
            ElementKind::Action => "action",
            ElementKind::Character => "character",
            ElementKind::Parenthetical => "parenthetical",
            ElementKind::Dialogue => "dialogue",
            ElementKind::Transition => "transition",
            ElementKind::Lyric => "lyric",
            ElementKind::Shot => "shot",
            ElementKind::General => "general",
            ElementKind::PageBreak => "page_break",
            ElementKind::Section => "section",
            ElementKind::Synopsis => "synopsis",
            ElementKind::Note => "note",
            ElementKind::Boneyard => "boneyard",
        }
    }

    pub fn from_name(name: &str) -> Option<ElementKind> {
        ElementKind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Transparent elements take up no lines in the printed script and are
    /// skipped when measuring spacing. Page breaks count as transparent:
    /// whatever follows starts at the top of a page.
    pub fn is_transparent(self) -> bool {
        matches!(
            self,
            ElementKind::Section
                | ElementKind::Synopsis
                | ElementKind::Note
                | ElementKind::Boneyard
                | ElementKind::PageBreak
        )
    }

    pub fn is_printing(self) -> bool {
        !self.is_transparent()
    }

    /// Elements that can follow a character inside a dialogue group.
    pub fn is_dialogue_member(self) -> bool {
        matches!(
            self,
            ElementKind::Parenthetical | ElementKind::Dialogue | ElementKind::Lyric
        )
    }

    /// Elements whose text is stored verbatim, with no styles or hidden runs.
    pub fn is_raw_text(self) -> bool {
        matches!(self, ElementKind::Note | ElementKind::Boneyard)
    }

    /// The usual number of blank lines above an element of this kind, given
    /// the previous printing element: none inside a dialogue group, one
    /// everywhere else (docs/specs/fountain.md §7). Fountain stores a gap
    /// only when it differs from this, and layout treats a stored gap equal
    /// to it as unset (docs/specs/layout.md §6).
    pub fn default_gap(self, prev: Option<ElementKind>) -> u8 {
        let in_group = prev.is_some_and(|p| p == ElementKind::Character || p.is_dialogue_member());
        if self.is_dialogue_member() && in_group {
            0
        } else {
            1
        }
    }
}

impl std::fmt::Display for ElementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// One paragraph of a screenplay.
///
/// Attributes only mean something for some kinds; [`Element::problems`]
/// lists violations and [`Element::clear_inapplicable`] drops them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Element {
    pub kind: ElementKind,
    pub text: RichText,
    /// Scene headings only, e.g. "12A".
    pub scene_number: Option<String>,
    /// Characters only: this speaker talks at the same time as the dialogue
    /// group just before.
    pub dual: bool,
    /// Action only.
    pub centered: bool,
    /// Sections only: 1 for `#`, 2 for `##`, and so on. Zero elsewhere.
    pub depth: u8,
    /// Blank lines above this element when they differ from the usual
    /// spacing for its position. `None` means the usual spacing.
    pub space_before: Option<u8>,
}

impl Element {
    pub fn new(kind: ElementKind, text: impl Into<RichText>) -> Element {
        Element {
            kind,
            text: text.into(),
            scene_number: None,
            dual: false,
            centered: false,
            depth: if kind == ElementKind::Section { 1 } else { 0 },
            space_before: None,
        }
    }

    pub fn scene_number(mut self, n: impl Into<String>) -> Element {
        self.scene_number = Some(n.into());
        self
    }

    pub fn dual(mut self) -> Element {
        self.dual = true;
        self
    }

    pub fn centered(mut self) -> Element {
        self.centered = true;
        self
    }

    pub fn depth(mut self, depth: u8) -> Element {
        self.depth = depth;
        self
    }

    pub fn space_before(mut self, lines: u8) -> Element {
        self.space_before = Some(lines);
        self
    }

    /// The speaker's name for a character element: the text before any
    /// parenthetical extension, trimmed. `None` for other kinds.
    pub fn character_name(&self) -> Option<String> {
        if self.kind != ElementKind::Character {
            return None;
        }
        let text = self.text.plain_text();
        let name = match text.find('(') {
            Some(i) => &text[..i],
            None => &text[..],
        };
        Some(name.trim().to_string())
    }

    /// Model rules every element must satisfy, regardless of file format.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let k = self.kind;
        if self.scene_number.is_some() && k != ElementKind::SceneHeading {
            p.push(format!("scene_number does not apply to {k}"));
        }
        if let Some(n) = &self.scene_number
            && (n.is_empty() || n.contains(['\n', '\r']))
        {
            p.push("scene_number must be non-empty and on one line".to_string());
        }
        if self.dual && k != ElementKind::Character {
            p.push(format!("dual does not apply to {k}"));
        }
        if self.centered && k != ElementKind::Action {
            p.push(format!("centered does not apply to {k}"));
        }
        if k == ElementKind::Section && self.depth == 0 {
            p.push("a section needs a depth of at least 1".to_string());
        }
        if k != ElementKind::Section && self.depth != 0 {
            p.push(format!("depth does not apply to {k}"));
        }
        if k == ElementKind::PageBreak && !self.text.is_empty() {
            p.push("a page break has no text".to_string());
        }
        if k.is_raw_text() && !self.text.is_simple() {
            p.push(format!(
                "{k} text is plain: no styles, notes or boneyard runs"
            ));
        }
        if k.is_transparent() && self.space_before.is_some() {
            p.push(format!("space_before does not apply to {k}"));
        }
        p
    }

    /// Drops attributes that do not apply to the current kind, and gives a
    /// section its default depth. Used after a type change.
    pub fn clear_inapplicable(&mut self) {
        let k = self.kind;
        if k != ElementKind::SceneHeading {
            self.scene_number = None;
        }
        if k != ElementKind::Character {
            self.dual = false;
        }
        if k != ElementKind::Action {
            self.centered = false;
        }
        if k == ElementKind::Section {
            if self.depth == 0 {
                self.depth = 1;
            }
        } else {
            self.depth = 0;
        }
        if k == ElementKind::PageBreak {
            self.text = RichText::empty();
        }
        if k.is_raw_text() && !self.text.is_simple() {
            self.text = RichText::plain(self.text.plain_text());
        }
        if k.is_transparent() {
            self.space_before = None;
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// JSON form of an element. `text` is always the printed string; `runs` is
/// present only for styled text or text with hidden runs; attributes appear
/// only when set.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementRepr {
    #[serde(rename = "type")]
    pub kind: ElementKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<Vec<Run>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_number: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dual: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub centered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_before: Option<u8>,
}

impl ElementRepr {
    pub fn from_element(e: &Element) -> ElementRepr {
        let t = TextRepr::from_rich(&e.text);
        ElementRepr {
            kind: e.kind,
            text: if e.kind == ElementKind::PageBreak {
                None
            } else {
                t.text
            },
            runs: t.runs,
            scene_number: e.scene_number.clone(),
            dual: e.dual,
            centered: e.centered,
            depth: (e.kind == ElementKind::Section || e.depth != 0).then_some(e.depth),
            space_before: e.space_before,
        }
    }

    /// Builds the element. Model rules are not checked here; call
    /// [`Element::problems`] for that.
    pub fn into_element(self) -> Result<Element, String> {
        let text = TextRepr {
            text: self.text,
            runs: self.runs,
        }
        .into_rich()?;
        Ok(Element {
            kind: self.kind,
            text,
            scene_number: self.scene_number,
            dual: self.dual,
            centered: self.centered,
            depth: match self.depth {
                Some(d) => d,
                None if self.kind == ElementKind::Section => 1,
                None => 0,
            },
            space_before: self.space_before,
        })
    }
}

impl Serialize for Element {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        ElementRepr::from_element(self).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Element {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Element, D::Error> {
        ElementRepr::deserialize(d)?
            .into_element()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_round_trip() {
        for k in ElementKind::ALL {
            assert_eq!(ElementKind::from_name(k.name()), Some(k));
            let json = serde_json::to_string(&k).unwrap();
            assert_eq!(json, format!("\"{}\"", k.name()));
        }
    }

    #[test]
    fn element_json() {
        let e = Element::new(ElementKind::SceneHeading, "INT. LAB - NIGHT").scene_number("4");
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(
            json,
            r#"{"type":"scene_heading","text":"INT. LAB - NIGHT","scene_number":"4"}"#
        );
        assert_eq!(serde_json::from_str::<Element>(&json).unwrap(), e);

        let s = Element::new(ElementKind::Section, "Act One");
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#"{"type":"section","text":"Act One","depth":1}"#);
        assert_eq!(serde_json::from_str::<Element>(&json).unwrap(), s);

        let pb = Element::new(ElementKind::PageBreak, "");
        assert_eq!(
            serde_json::to_string(&pb).unwrap(),
            r#"{"type":"page_break"}"#
        );
        assert_eq!(
            serde_json::from_str::<Element>(r#"{"type":"page_break"}"#).unwrap(),
            pb
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(serde_json::from_str::<Element>(r#"{"type":"action","txt":"x"}"#).is_err());
        assert!(serde_json::from_str::<Element>(r#"{"type":"actoin"}"#).is_err());
    }

    #[test]
    fn problems_and_clearing() {
        let mut e = Element::new(ElementKind::Action, "BOB").centered();
        assert!(e.problems().is_empty());
        e.kind = ElementKind::Character;
        assert_eq!(e.problems().len(), 1);
        e.clear_inapplicable();
        assert!(e.problems().is_empty());
        assert!(!e.centered);

        e.kind = ElementKind::Section;
        e.clear_inapplicable();
        assert_eq!(e.depth, 1);
    }

    #[test]
    fn default_gaps() {
        use ElementKind::*;
        assert_eq!(Dialogue.default_gap(Some(Character)), 0);
        assert_eq!(Parenthetical.default_gap(Some(Dialogue)), 0);
        assert_eq!(Lyric.default_gap(Some(Lyric)), 0);
        assert_eq!(Dialogue.default_gap(Some(Action)), 1);
        assert_eq!(Dialogue.default_gap(None), 1);
        assert_eq!(Character.default_gap(Some(Dialogue)), 1);
        assert_eq!(SceneHeading.default_gap(Some(Action)), 1);
    }

    #[test]
    fn character_name_strips_extension() {
        let e = Element::new(ElementKind::Character, "MOM (O.S.)");
        assert_eq!(e.character_name().as_deref(), Some("MOM"));
        let e = Element::new(ElementKind::Action, "MOM");
        assert_eq!(e.character_name(), None);
    }
}
