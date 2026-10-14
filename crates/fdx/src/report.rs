//! The reader's report: one [`Issue`] per thing it could not carry over
//! (docs/specs/fdx.md §9).

use std::collections::BTreeMap;

use serde::Serialize;

/// Something the reader noticed and recovered from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Issue {
    /// A stable code from [`codes`].
    pub code: &'static str,
    /// A description in English. It never quotes script text.
    pub message: String,
    /// The 0-based index of the `Paragraph` in `Content`, in document order,
    /// when the issue concerns one. Paragraphs nested in dual dialogue share
    /// their outer paragraph's index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<usize>,
}

/// Every report code, as listed in docs/specs/fdx.md §9.
pub mod codes {
    pub const ENCODING_UTF16: &str = "encoding_utf16";
    pub const ENCODING_WINDOWS_1252: &str = "encoding_windows_1252";
    pub const ENCODING_LOSSY: &str = "encoding_lossy";
    pub const TRUNCATED_XML: &str = "truncated_xml";
    pub const MISMATCHED_END_TAG: &str = "mismatched_end_tag";
    pub const STRAY_END_TAG: &str = "stray_end_tag";
    pub const UNCLOSED_ELEMENT: &str = "unclosed_element";
    pub const TOO_DEEP: &str = "too_deep";
    pub const CONTENT_AFTER_ROOT: &str = "content_after_root";
    pub const UNKNOWN_ENTITY: &str = "unknown_entity";
    pub const BAD_CHAR_REF: &str = "bad_char_ref";
    pub const ATTRIBUTE_SYNTAX: &str = "attribute_syntax";
    pub const ATTRIBUTE_VALUE: &str = "attribute_value";
    pub const DUPLICATE_ATTRIBUTE: &str = "duplicate_attribute";
    pub const SECTIONS_IGNORED: &str = "sections_ignored";
    pub const DUPLICATE_SECTION: &str = "duplicate_section";
    pub const SECTION_MISSING: &str = "section_missing";
    pub const INVALID_VALUE: &str = "invalid_value";
    pub const VALUE_ROUNDED: &str = "value_rounded";
    pub const SETTINGS_FOR_UNMODELLED_TYPE: &str = "settings_for_unmodelled_type";
    pub const DUPLICATE_SETTINGS: &str = "duplicate_settings";
    pub const SETTINGS_MISSING: &str = "settings_missing";
    pub const ALIGNMENT_FULL: &str = "alignment_full";
    pub const LINE_SPACING: &str = "line_spacing";
    pub const UNKNOWN_PAGINATE_AS: &str = "unknown_paginate_as";
    pub const SETTINGS_FONT_IGNORED: &str = "settings_font_ignored";
    pub const DOCUMENT_LEADING: &str = "document_leading";
    pub const SCENE_CONTINUEDS_UNSUPPORTED: &str = "scene_continueds_unsupported";
    pub const SCENE_NUMBER_SIDE: &str = "scene_number_side";
    pub const STARTING_PAGE: &str = "starting_page";
    pub const HEADER_CONTENT_IGNORED: &str = "header_content_ignored";
    pub const FOOTER_IGNORED: &str = "footer_ignored";
    pub const INVALID_FORMAT: &str = "invalid_format";
    pub const NO_CONTENT: &str = "no_content";
    pub const UNKNOWN_PARAGRAPH_TYPE: &str = "unknown_paragraph_type";
    pub const INVALID_SCENE_NUMBER: &str = "invalid_scene_number";
    pub const SCENE_NUMBER_IGNORED: &str = "scene_number_ignored";
    pub const SCENE_PROPERTIES_IGNORED: &str = "scene_properties_ignored";
    pub const RUN_ALL_CAPS: &str = "run_all_caps";
    pub const UNKNOWN_STYLE: &str = "unknown_style";
    pub const RUN_FONT: &str = "run_font";
    pub const RUN_SIZE: &str = "run_size";
    pub const RUN_COLOR: &str = "run_color";
    pub const RUN_BACKGROUND: &str = "run_background";
    pub const RUN_ADORNMENT: &str = "run_adornment";
    pub const RUN_REVISION: &str = "run_revision";
    pub const TAB_CHARACTER: &str = "tab_character";
    pub const ELEMENT_IGNORED: &str = "element_ignored";
    pub const PARAGRAPH_FORMAT_IGNORED: &str = "paragraph_format_ignored";
    pub const DUAL_DIALOGUE_SHAPE: &str = "dual_dialogue_shape";
    pub const DUAL_DIALOGUE_TEXT: &str = "dual_dialogue_text";
    pub const TRAILING_EMPTY_DROPPED: &str = "trailing_empty_dropped";
    pub const TITLE_FORMAT_IGNORED: &str = "title_format_ignored";

    /// Every code, in the order of the spec's table.
    pub const ALL: [&str; 53] = [
        ENCODING_UTF16,
        ENCODING_WINDOWS_1252,
        ENCODING_LOSSY,
        TRUNCATED_XML,
        MISMATCHED_END_TAG,
        STRAY_END_TAG,
        UNCLOSED_ELEMENT,
        TOO_DEEP,
        CONTENT_AFTER_ROOT,
        UNKNOWN_ENTITY,
        BAD_CHAR_REF,
        ATTRIBUTE_SYNTAX,
        ATTRIBUTE_VALUE,
        DUPLICATE_ATTRIBUTE,
        SECTIONS_IGNORED,
        DUPLICATE_SECTION,
        SECTION_MISSING,
        INVALID_VALUE,
        VALUE_ROUNDED,
        SETTINGS_FOR_UNMODELLED_TYPE,
        DUPLICATE_SETTINGS,
        SETTINGS_MISSING,
        ALIGNMENT_FULL,
        LINE_SPACING,
        UNKNOWN_PAGINATE_AS,
        SETTINGS_FONT_IGNORED,
        DOCUMENT_LEADING,
        SCENE_CONTINUEDS_UNSUPPORTED,
        SCENE_NUMBER_SIDE,
        STARTING_PAGE,
        HEADER_CONTENT_IGNORED,
        FOOTER_IGNORED,
        INVALID_FORMAT,
        NO_CONTENT,
        UNKNOWN_PARAGRAPH_TYPE,
        INVALID_SCENE_NUMBER,
        SCENE_NUMBER_IGNORED,
        SCENE_PROPERTIES_IGNORED,
        RUN_ALL_CAPS,
        UNKNOWN_STYLE,
        RUN_FONT,
        RUN_SIZE,
        RUN_COLOR,
        RUN_BACKGROUND,
        RUN_ADORNMENT,
        RUN_REVISION,
        TAB_CHARACTER,
        ELEMENT_IGNORED,
        PARAGRAPH_FORMAT_IGNORED,
        DUAL_DIALOGUE_SHAPE,
        DUAL_DIALOGUE_TEXT,
        TRAILING_EMPTY_DROPPED,
        TITLE_FORMAT_IGNORED,
    ];
}

/// Builds the message of a counted issue from its key and count.
pub(crate) type Describe = fn(key: &str, count: usize) -> String;

struct Tally {
    code: &'static str,
    key: String,
    count: usize,
    paragraph: Option<usize>,
    describe: Describe,
}

/// Collects issues while reading. Counted issues (marked ‡ in the spec)
/// come after all others, once per code and key, in order of first
/// occurrence.
#[derive(Default)]
pub(crate) struct Report {
    issues: Vec<Issue>,
    /// In order of first occurrence.
    tallies: Vec<Tally>,
    /// The position in `tallies` of each code and key, so a file with many
    /// distinct keys is counted in O(n log n) rather than O(n²).
    index: BTreeMap<&'static str, BTreeMap<String, usize>>,
}

impl Report {
    pub(crate) fn push(
        &mut self,
        code: &'static str,
        message: impl Into<String>,
        paragraph: Option<usize>,
    ) {
        self.issues.push(Issue {
            code,
            message: message.into(),
            paragraph,
        });
    }

    pub(crate) fn extend(&mut self, issues: impl IntoIterator<Item = Issue>) {
        self.issues.extend(issues);
    }

    /// Counts one occurrence of `code` for `key`. `paragraph` is kept from
    /// the first occurrence.
    pub(crate) fn tally(
        &mut self,
        code: &'static str,
        key: &str,
        paragraph: Option<usize>,
        describe: Describe,
    ) {
        let keys = self.index.entry(code).or_default();
        if let Some(t) = keys.get(key).and_then(|&n| self.tallies.get_mut(n)) {
            t.count += 1;
            return;
        }
        keys.insert(key.to_string(), self.tallies.len());
        self.tallies.push(Tally {
            code,
            key: key.to_string(),
            count: 1,
            paragraph,
            describe,
        });
    }

    pub(crate) fn finish(self) -> Vec<Issue> {
        let mut out = self.issues;
        out.extend(self.tallies.into_iter().map(|t| Issue {
            code: t.code,
            message: (t.describe)(&t.key, t.count),
            paragraph: t.paragraph,
        }));
        out
    }
}

/// "1 paragraph", "3 paragraphs".
pub(crate) fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_snake_case() {
        let mut seen = std::collections::BTreeSet::new();
        for c in codes::ALL {
            assert!(seen.insert(c), "duplicate code {c}");
            assert!(
                c.bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit()),
                "{c}"
            );
        }
    }

    #[test]
    fn every_code_is_in_the_spec_table() {
        let spec = include_str!("../../../docs/specs/fdx.md");
        for c in codes::ALL {
            assert!(
                spec.contains(&format!("| `{c}`"))
                    || spec.contains(&format!(", `{c}`"))
                    || spec.contains(&format!("`{c}` ‡")),
                "{c} is missing from docs/specs/fdx.md §9"
            );
        }
    }

    #[test]
    fn tallies_come_last_in_first_seen_order() {
        let mut r = Report::default();
        r.tally("b", "x", Some(3), |k, n| format!("{k}{n}"));
        r.push("a", "first", None);
        r.tally("c", "y", None, |k, n| format!("{k}{n}"));
        r.tally("b", "x", Some(9), |k, n| format!("{k}{n}"));
        let out = r.finish();
        let got: Vec<_> = out
            .iter()
            .map(|i| (i.code, i.message.as_str(), i.paragraph))
            .collect();
        assert_eq!(
            got,
            [
                ("a", "first", None),
                ("b", "x2", Some(3)),
                ("c", "y1", None)
            ]
        );
    }
}
