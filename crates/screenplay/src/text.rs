//! Rich text: a normalized list of runs.

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

/// One piece of a block's text.
///
/// `Note` and `Boneyard` runs are hidden: they never print and take no width,
/// but they keep their position in the text so they survive a round trip.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Run {
    Text { text: String, style: Style },
    Note(String),
    Boneyard(String),
}

impl Run {
    pub fn plain(text: impl Into<String>) -> Run {
        Run::Text {
            text: text.into(),
            style: Style::PLAIN,
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Run {
        Run::Text {
            text: text.into(),
            style,
        }
    }

    pub fn is_hidden(&self) -> bool {
        !matches!(self, Run::Text { .. })
    }
}

/// Rich text, always kept in normal form: no empty text runs, and no two
/// neighbouring text runs with the same style.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RichText {
    runs: Vec<Run>,
}

impl RichText {
    pub fn new(runs: Vec<Run>) -> RichText {
        let mut out: Vec<Run> = Vec::with_capacity(runs.len());
        for run in runs {
            match run {
                Run::Text { text, style } => {
                    if text.is_empty() {
                        continue;
                    }
                    if let Some(Run::Text {
                        text: prev,
                        style: prev_style,
                    }) = out.last_mut()
                        && *prev_style == style
                    {
                        prev.push_str(&text);
                        continue;
                    }
                    out.push(Run::Text { text, style });
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

    /// The printed text: text runs only, hidden runs skipped.
    pub fn plain_text(&self) -> String {
        let mut s = String::new();
        for run in &self.runs {
            if let Run::Text { text, .. } = run {
                s.push_str(text);
            }
        }
        s
    }

    /// True when there are no runs at all (hidden runs count as content).
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// True when the text is a single unstyled run or empty, so `text` alone
    /// describes it fully.
    pub fn is_simple(&self) -> bool {
        self.runs.iter().all(|r| match r {
            Run::Text { style, .. } => style.is_plain(),
            _ => false,
        })
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
/// style flags on text runs.
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
    note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    boneyard: Option<String>,
}

impl Serialize for Run {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let repr = match self {
            Run::Text { text, style } => RunRepr {
                text: Some(text.clone()),
                bold: style.bold,
                italic: style.italic,
                underline: style.underline,
                note: None,
                boneyard: None,
            },
            Run::Note(n) => RunRepr {
                text: None,
                bold: false,
                italic: false,
                underline: false,
                note: Some(n.clone()),
                boneyard: None,
            },
            Run::Boneyard(b) => RunRepr {
                text: None,
                bold: false,
                italic: false,
                underline: false,
                note: None,
                boneyard: Some(b.clone()),
            },
        };
        repr.serialize(s)
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
        match (r.text, r.note, r.boneyard) {
            (Some(text), None, None) => Ok(Run::Text { text, style }),
            (None, Some(_), None) | (None, None, Some(_)) if !style.is_plain() => Err(
                D::Error::custom("bold, italic and underline apply only to text runs"),
            ),
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
