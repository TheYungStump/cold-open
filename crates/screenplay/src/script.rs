//! A whole screenplay: title page plus elements.

use serde::{Deserialize, Serialize};

use crate::element::Element;
use crate::text::RichText;

/// One title-page entry. Keys keep their original spelling; a key may have
/// several values (one per line, such as address lines).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleField {
    pub key: String,
    pub values: Vec<RichText>,
}

impl TitleField {
    pub fn new(key: impl Into<String>, values: Vec<RichText>) -> TitleField {
        TitleField {
            key: key.into(),
            values,
        }
    }
}

/// The title page, in source order. Empty means the script has none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitlePage {
    pub fields: Vec<TitleField>,
}

impl TitlePage {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// The first field whose key matches, ignoring case. "Author" and
    /// "Authors" are treated as the same key.
    pub fn get(&self, key: &str) -> Option<&TitleField> {
        let want = canonical_key(key);
        self.fields.iter().find(|f| canonical_key(&f.key) == want)
    }
}

fn canonical_key(key: &str) -> String {
    let k = key.trim().to_lowercase();
    if k == "authors" {
        "author".to_string()
    } else {
        k
    }
}

/// A screenplay as plain data: no ids, no editing state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    #[serde(default, skip_serializing_if = "TitlePage::is_empty")]
    pub title_page: TitlePage,
    #[serde(default)]
    pub elements: Vec<Element>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_lookup_ignores_case_and_plural() {
        let tp = TitlePage {
            fields: vec![
                TitleField::new("Draft date", vec!["1/2/26".into()]),
                TitleField::new("Authors", vec!["A. Writer".into()]),
            ],
        };
        assert!(tp.get("draft DATE").is_some());
        assert_eq!(tp.get("Author").unwrap().key, "Authors");
        assert!(tp.get("Contact").is_none());
    }
}
