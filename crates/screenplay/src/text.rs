//! Rich text: a normalized list of runs.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Character styles that screenplays use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Style {
    pub const PLAIN: Style = Style {
        bold: false,
        italic: false,
        underline: false,
    };

    pub fn is_plain(self) -> bool {
        self == Self::PLAIN
    }
}

/// A tracked change's id, written `c_<n>` (docs/specs/changes.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChangeId(pub u64);

impl fmt::Display for ChangeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "c_{}", self.0)
    }
}

impl ChangeId {
    pub fn parse(s: &str) -> Option<ChangeId> {
        let n = s.strip_prefix("c_")?;
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        n.parse().ok().filter(|&n| n > 0).map(ChangeId)
    }
}

impl Serialize for ChangeId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ChangeId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<ChangeId, D::Error> {
        let s = String::deserialize(d)?;
        ChangeId::parse(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("\"{s}\" is not a change id")))
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for ChangeId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ChangeId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A tracked change's id, such as \"c_4\".",
            "type": "string",
            "pattern": "^c_[1-9][0-9]*$"
        })
    }
}

/// A production revision set's id, written `r_<n>`
/// (docs/specs/revisions.md §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RevisionId(pub u64);

impl fmt::Display for RevisionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "r_{}", self.0)
    }
}

impl RevisionId {
    pub fn parse(s: &str) -> Option<RevisionId> {
        let n = s.strip_prefix("r_")?;
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        n.parse().ok().filter(|&n| n > 0).map(RevisionId)
    }
}

impl Serialize for RevisionId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for RevisionId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<RevisionId, D::Error> {
        let s = String::deserialize(d)?;
        RevisionId::parse(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("\"{s}\" is not a revision set id")))
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for RevisionId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RevisionId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A production revision set's id, such as \"r_2\".",
            "type": "string",
            "pattern": "^r_[1-9][0-9]*$"
        })
    }
}

/// The marks of a text run: tracked changes (docs/specs/changes.md §2.1)
/// and the production revision set that last changed it
/// (docs/specs/revisions.md §4.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Marks {
    /// The run is a pending insertion by this change.
    pub ins: Option<ChangeId>,
    /// The run is a pending deletion by this change. It is not part of the
    /// current text: it takes no offsets and is left out of
    /// [`RichText::plain_text`].
    pub del: Option<ChangeId>,
    /// Print forms only: the deletion prints, struck through, so it counts
    /// as printed text. Never stored or read from JSON.
    pub show: bool,
    /// The newest revision set that changed this text. Not a tracked
    /// change: it never takes offsets or prints differently.
    pub rev: Option<RevisionId>,
}

impl Marks {
    pub const NONE: Marks = Marks {
        ins: None,
        del: None,
        show: false,
        rev: None,
    };

    /// No tracked-change marks (a revision mark may still be there).
    pub fn is_none(&self) -> bool {
        self.ins.is_none() && self.del.is_none()
    }

    /// No marks at all, tracked or revision.
    pub fn is_clear(&self) -> bool {
        self.is_none() && self.rev.is_none()
    }

    /// Whether the run is left out of the current text.
    pub fn is_deleted(&self) -> bool {
        self.del.is_some() && !self.show
    }
}

/// One piece of a block's text.
///
/// `Note` and `Boneyard` runs are hidden: they never print and take no width,
/// but they keep their position in the text so they survive a round trip.
/// A text run marked as a pending deletion is treated like a hidden run for
/// offsets (docs/specs/commands.md, "Positions in text").
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Run {
    Text {
        text: String,
        style: Style,
        marks: Marks,
    },
    Note(String),
    Boneyard(String),
}

impl Run {
    pub fn plain(text: impl Into<String>) -> Run {
        Run::styled(text, Style::PLAIN)
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Run {
        Run::marked(text, style, Marks::NONE)
    }

    pub fn marked(text: impl Into<String>, style: Style, marks: Marks) -> Run {
        Run::Text {
            text: text.into(),
            style,
            marks,
        }
    }

    pub fn is_hidden(&self) -> bool {
        !matches!(self, Run::Text { .. })
    }

    /// A text run that is not part of the current text: a pending deletion.
    pub fn is_deleted(&self) -> bool {
        matches!(self, Run::Text { marks, .. } if marks.is_deleted())
    }

    /// Hidden, or a pending deletion: takes no offsets.
    pub fn takes_no_offsets(&self) -> bool {
        self.is_hidden() || self.is_deleted()
    }

    pub fn marks(&self) -> Marks {
        match self {
            Run::Text { marks, .. } => *marks,
            _ => Marks::NONE,
        }
    }
}

/// Rich text, always kept in normal form: no empty text runs, and no two
/// neighbouring text runs with the same style and marks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RichText {
    runs: Vec<Run>,
}

impl RichText {
    pub fn new(runs: Vec<Run>) -> RichText {
        let mut out: Vec<Run> = Vec::with_capacity(runs.len());
        for run in runs {
            match run {
                Run::Text { text, style, marks } => {
                    if text.is_empty() {
                        continue;
                    }
                    if let Some(Run::Text {
                        text: prev,
                        style: prev_style,
                        marks: prev_marks,
                    }) = out.last_mut()
                        && *prev_style == style
                        && *prev_marks == marks
                    {
                        prev.push_str(&text);
                        continue;
                    }
                    out.push(Run::Text { text, style, marks });
                }
                hidden => out.push(hidden),
            }
        }
        RichText { runs: out }
    }

    pub fn plain(text: impl Into<String>) -> RichText {
        RichText::new(vec![Run::plain(text)])
    }

    pub fn empty() -> RichText {
        RichText::default()
    }

    pub fn runs(&self) -> &[Run] {
        &self.runs
    }

    pub fn into_runs(self) -> Vec<Run> {
        self.runs
    }

    /// The printed text: text runs only, hidden runs skipped. With tracked
    /// changes this is the current text: pending deletions are left out
    /// (docs/specs/changes.md §2.1).
    pub fn plain_text(&self) -> String {
        let mut s = String::new();
        for run in &self.runs {
            if let Run::Text { text, marks, .. } = run
                && !marks.is_deleted()
            {
                s.push_str(text);
            }
        }
        s
    }

    /// True when there are no runs at all (hidden runs count as content).
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// True when the text is a single unstyled, unmarked run or empty, so
    /// `text` alone describes it fully.
    pub fn is_simple(&self) -> bool {
        self.runs.iter().all(|r| match r {
            Run::Text { style, marks, .. } => style.is_plain() && marks.is_clear(),
            _ => false,
        })
    }

    /// True when the text has no styles and no hidden runs, as note and
    /// boneyard text must be. Tracked-change marks are allowed.
    pub fn is_raw(&self) -> bool {
        self.runs.iter().all(|r| match r {
            Run::Text { style, .. } => style.is_plain(),
            _ => false,
        })
    }

    /// The text with styles taken off and hidden runs left out, keeping
    /// tracked-change marks.
    pub fn to_raw(&self) -> RichText {
        RichText::new(
            self.runs
                .iter()
                .filter_map(|r| match r {
                    Run::Text { text, marks, .. } => {
                        Some(Run::marked(text.clone(), Style::PLAIN, *marks))
                    }
                    _ => None,
                })
                .collect(),
        )
    }

    /// Whether any run carries a tracked-change mark.
    pub fn has_marks(&self) -> bool {
        self.runs.iter().any(|r| !r.marks().is_none())
    }

    /// Whether any run carries a revision mark.
    pub fn has_revisions(&self) -> bool {
        self.runs.iter().any(|r| r.marks().rev.is_some())
    }

    /// The text with its revision marks taken off.
    pub fn without_revisions(&self) -> RichText {
        RichText::new(
            self.runs
                .iter()
                .map(|r| match r {
                    Run::Text { text, style, marks } => Run::marked(
                        text.clone(),
                        *style,
                        Marks {
                            rev: None,
                            ..*marks
                        },
                    ),
                    hidden => hidden.clone(),
                })
                .collect(),
        )
    }

    pub fn has_hidden(&self) -> bool {
        self.runs.iter().any(Run::is_hidden)
    }

    pub fn has_styles(&self) -> bool {
        self.runs.iter().any(|r| match r {
            Run::Text { style, .. } => !style.is_plain(),
            _ => false,
        })
    }
}

impl From<&str> for RichText {
    fn from(s: &str) -> RichText {
        RichText::plain(s)
    }
}

impl From<String> for RichText {
    fn from(s: String) -> RichText {
        RichText::plain(s)
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// JSON form of a run: exactly one of `text`, `note` or `boneyard`, plus
/// style flags and tracked-change marks on text runs.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRepr {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    underline: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ins: Option<ChangeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    del: Option<ChangeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rev: Option<RevisionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    boneyard: Option<String>,
}

impl Serialize for Run {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let repr = match self {
            Run::Text { text, style, marks } => RunRepr {
                text: Some(text.clone()),
                bold: style.bold,
                italic: style.italic,
                underline: style.underline,
                ins: marks.ins,
                del: marks.del,
                rev: marks.rev,
                note: None,
                boneyard: None,
            },
            Run::Note(n) => RunRepr {
                text: None,
                bold: false,
                italic: false,
                underline: false,
                ins: None,
                del: None,
                rev: None,
                note: Some(n.clone()),
                boneyard: None,
            },
            Run::Boneyard(b) => RunRepr {
                text: None,
                bold: false,
                italic: false,
                underline: false,
                ins: None,
                del: None,
                rev: None,
                note: None,
                boneyard: Some(b.clone()),
            },
        };
        repr.serialize(s)
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for Run {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Run".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A run of text with its style, or a hidden note or boneyard run. Exactly one of text, note and boneyard. ins and del name a pending tracked change; in input a del run is left out and ins is ignored. rev names the revision set that last changed the text; in input it is ignored.",
            "type": "object",
            "properties": {
                "text": {"type": "string"},
                "bold": {"type": "boolean"},
                "italic": {"type": "boolean"},
                "underline": {"type": "boolean"},
                "ins": {"type": "string", "pattern": "^c_[1-9][0-9]*$"},
                "del": {"type": "string", "pattern": "^c_[1-9][0-9]*$"},
                "rev": {"type": "string", "pattern": "^r_[1-9][0-9]*$"},
                "note": {"type": "string"},
                "boneyard": {"type": "string"}
            },
            "additionalProperties": false
        })
    }
}

impl<'de> Deserialize<'de> for Run {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Run, D::Error> {
        use serde::de::Error;
        let r = RunRepr::deserialize(d)?;
        let style = Style {
            bold: r.bold,
            italic: r.italic,
            underline: r.underline,
        };
        let marks = Marks {
            ins: r.ins,
            del: r.del,
            show: false,
            rev: r.rev,
        };
        match (r.text, r.note, r.boneyard) {
            (Some(text), None, None) => Ok(Run::Text { text, style, marks }),
            (None, Some(_), None) | (None, None, Some(_)) if !style.is_plain() => Err(
                D::Error::custom("bold, italic and underline apply only to text runs"),
            ),
            (None, Some(_), None) | (None, None, Some(_)) if !marks.is_clear() => {
                Err(D::Error::custom("ins, del and rev apply only to text runs"))
            }
            (None, Some(n), None) => Ok(Run::Note(n)),
            (None, None, Some(b)) => Ok(Run::Boneyard(b)),
            _ => Err(D::Error::custom(
                "a run needs exactly one of \"text\", \"note\" or \"boneyard\"",
            )),
        }
    }
}

/// Text as it appears in JSON: `text` is always the printed string; `runs`
/// is present only when the text has styles or hidden runs.
#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TextRepr {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<Vec<Run>>,
}

impl TextRepr {
    pub fn from_rich(t: &RichText) -> TextRepr {
        TextRepr {
            text: Some(t.plain_text()),
            runs: if t.is_simple() {
                None
            } else {
                Some(t.runs().to_vec())
            },
        }
    }

    /// Builds rich text from `text`, `runs` or both. When both are given,
    /// `text` must equal the printed text of `runs`.
    pub fn into_rich(self) -> Result<RichText, String> {
        match (self.text, self.runs) {
            (None, None) => Ok(RichText::empty()),
            (Some(t), None) => Ok(RichText::plain(t)),
            (t, Some(runs)) => {
                let rich = RichText::new(runs);
                if let Some(t) = t
                    && t != rich.plain_text()
                {
                    return Err(
                        "\"text\" does not match the printed text of \"runs\"; send one or the other"
                            .to_string(),
                    );
                }
                Ok(rich)
            }
        }
    }
}

impl Serialize for RichText {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        TextRepr::from_rich(self).serialize(s)
    }
}

impl<'de> Deserialize<'de> for RichText {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<RichText, D::Error> {
        TextRepr::deserialize(d)?
            .into_rich()
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOLD: Style = Style {
        bold: true,
        italic: false,
        underline: false,
    };

    #[test]
    fn normalizes_runs() {
        let t = RichText::new(vec![
            Run::plain("a"),
            Run::plain(""),
            Run::plain("b"),
            Run::styled("c", BOLD),
            Run::styled("d", BOLD),
            Run::Note("n".into()),
            Run::plain("e"),
        ]);
        assert_eq!(
            t.runs(),
            &[
                Run::plain("ab"),
                Run::styled("cd", BOLD),
                Run::Note("n".into()),
                Run::plain("e"),
            ]
        );
        assert_eq!(t.plain_text(), "abcde");
    }

    #[test]
    fn json_shape() {
        let simple = RichText::plain("MAYA");
        assert_eq!(
            serde_json::to_string(&simple).unwrap(),
            r#"{"text":"MAYA"}"#
        );
        let rich = RichText::new(vec![
            Run::plain("so "),
            Run::styled("very", BOLD),
            Run::Note("fix".into()),
        ]);
        let json = serde_json::to_string(&rich).unwrap();
        assert_eq!(
            json,
            r#"{"text":"so very","runs":[{"text":"so "},{"text":"very","bold":true},{"note":"fix"}]}"#
        );
        let back: RichText = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rich);
    }

    #[test]
    fn rejects_bad_runs() {
        assert!(serde_json::from_str::<Run>(r#"{"note":"x","bold":true}"#).is_err());
        assert!(serde_json::from_str::<Run>(r#"{"text":"x","note":"y"}"#).is_err());
        assert!(serde_json::from_str::<Run>(r#"{}"#).is_err());
        assert!(serde_json::from_str::<RichText>(r#"{"text":"x","runs":[{"text":"y"}]}"#).is_err());
    }
}
