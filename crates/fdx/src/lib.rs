//! .fdx reader for Cold Open.
//!
//! [`read`] turns the bytes of an .fdx file into a [`Script`], the
//! [`PageFormat`] its settings describe, per-element spacing overrides and
//! the title page. It fails only when the input is not XML at all or its
//! root element is not `FinalDraft`; everything else is recovered, and what
//! could not be carried over is listed in the report as [`Issue`]s with
//! stable [`codes`].
//!
//! This is the phase 2 read-only loader. The rules are written down in
//! `docs/specs/fdx.md`; section numbers in comments (§n) refer to it.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod content;
mod decode;
#[doc(hidden)]
pub mod fuzzing;
mod report;
mod settings;
mod title;
mod values;
mod xml;

use std::collections::BTreeMap;
use std::fmt;

use coldopen_screenplay::{PageFormat, Pt, Script, TitleLine};
use serde::Serialize;

pub use decode::decode_input;
pub use report::{Issue, codes};

/// Everything the reader takes from an .fdx file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FdxDoc {
    /// The script. Its Fountain-style title page is always empty; the
    /// .fdx title page is in [`FdxDoc::title_lines`].
    pub script: Script,
    /// The page format from the file's settings (§5), or the default when
    /// those fail validation.
    pub format: PageFormat,
    /// Space above an element, by index into `script.elements`, where a
    /// paragraph's own spacing differs from its type's (§6.4).
    pub overrides: BTreeMap<usize, Pt>,
    /// The title page as positioned paragraphs (§7).
    pub title_lines: Vec<TitleLine>,
    /// Scene properties recorded on scene headings (§6.3).
    pub scenes: Vec<SceneProps>,
    /// What the reader could not carry over, in the order of §9.
    pub report: Vec<Issue>,
}

/// The properties an .fdx file records for one scene, kept as written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SceneProps {
    /// Index of the scene heading in `script.elements`.
    pub element: usize,
    /// The page the scene starts on, as recorded.
    pub page: Option<String>,
    /// The scene's length, as recorded (eighths of a page in the samples).
    pub length: Option<String>,
    pub title: Option<String>,
}

/// Why an input could not be read at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FdxError {
    /// No XML element was found before the end of the input or the first
    /// syntax error.
    NotXml(String),
    /// The first element is not `FinalDraft`; holds the name found.
    NoFinalDraftRoot(String),
}

impl fmt::Display for FdxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FdxError::NotXml(why) => write!(f, "not an XML file ({why})"),
            FdxError::NoFinalDraftRoot(name) => {
                write!(
                    f,
                    "not an .fdx file: the root element is <{name}>, not <FinalDraft>"
                )
            }
        }
    }
}

impl std::error::Error for FdxError {}

/// Reads an .fdx file. Never panics.
pub fn read(bytes: &[u8]) -> Result<FdxDoc, FdxError> {
    let mut report = report::Report::default();
    let (text, notes) = decode_input(bytes);
    report.extend(notes);
    let root = xml::parse(&text, &mut report)?;
    let settings = settings::read(&root, &mut report);
    let content = content::read(&root, &settings, &mut report);
    let title_lines = title::read(&root, &settings.format, &mut report);
    Ok(FdxDoc {
        script: content.script,
        format: settings.format,
        overrides: content.overrides,
        title_lines,
        scenes: content.scenes,
        report: report.finish(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors() {
        assert!(matches!(read(b""), Err(FdxError::NotXml(_))));
        assert!(matches!(read(b"just words"), Err(FdxError::NotXml(_))));
        assert!(matches!(read(b"\x00\x01\x02"), Err(FdxError::NotXml(_))));
        let e = read(b"<?xml version=\"1.0\"?><svg/>").unwrap_err();
        assert_eq!(e, FdxError::NoFinalDraftRoot("svg".into()));
        assert_eq!(
            e.to_string(),
            "not an .fdx file: the root element is <svg>, not <FinalDraft>"
        );
    }

    /// The paragraph index of every issue with `code`, in report order.
    fn issues_of(doc: &FdxDoc, code: &str) -> Vec<Option<usize>> {
        doc.report
            .iter()
            .filter(|i| i.code == code)
            .map(|i| i.paragraph)
            .collect()
    }

    #[test]
    fn space_before_overrides_stay_within_the_body() {
        // The default format: an 11 in page with 1 in margins, so the body
        // is 648 pt. Action keeps its default 12 pt above.
        let doc = read(
            br#"<FinalDraft><Content>
<Paragraph Type="Action"><Text>a</Text></Paragraph>
<Paragraph Type="Action" SpaceBefore="24"><Text>b</Text></Paragraph>
<Paragraph Type="Action" SpaceBefore="-12"><Text>c</Text></Paragraph>
<Paragraph Type="Action" SpaceBefore="21474836.47"><Text>d</Text></Paragraph>
<Paragraph Type="Action" SpaceBefore="648"><Text>e</Text></Paragraph>
<Paragraph Type="Action" SpaceBefore="648.01"><Text>f</Text></Paragraph>
</Content></FinalDraft>"#,
        )
        .unwrap();
        assert_eq!(
            doc.overrides,
            BTreeMap::from([(1, Pt::points(24)), (4, Pt::points(648))])
        );
        assert_eq!(
            issues_of(&doc, codes::INVALID_VALUE),
            [Some(2), Some(3), Some(5)]
        );

        // The bound is the body of the file's own format: 5 in less two
        // 1 in margins leaves 216 pt.
        let doc = read(
            br#"<FinalDraft><PageLayout TopMargin="72" BottomMargin="72"><PageSize Height="5"/></PageLayout><Content>
<Paragraph Type="Action" SpaceBefore="216"><Text>a</Text></Paragraph>
<Paragraph Type="Action" SpaceBefore="300"><Text>b</Text></Paragraph>
</Content></FinalDraft>"#,
        )
        .unwrap();
        assert_eq!(doc.format.body_height(), Pt::points(216));
        assert_eq!(doc.overrides, BTreeMap::from([(0, Pt::points(216))]));
        assert_eq!(issues_of(&doc, codes::INVALID_VALUE), [Some(1)]);
    }

    #[test]
    fn title_space_before_is_clamped_to_the_body() {
        let doc = read(
            br#"<FinalDraft><TitlePage><Content>
<Paragraph><Text>A</Text></Paragraph>
<Paragraph SpaceBefore="21474836.47"><Text>B</Text></Paragraph>
<Paragraph SpaceBefore="-5"><Text>C</Text></Paragraph>
<Paragraph SpaceBefore="648"><Text>D</Text></Paragraph>
</Content></TitlePage></FinalDraft>"#,
        )
        .unwrap();
        let spaces: Vec<Pt> = doc.title_lines.iter().map(|l| l.space_before).collect();
        assert_eq!(
            spaces,
            [Pt::ZERO, Pt::points(648), Pt::ZERO, Pt::points(648)]
        );
        assert_eq!(issues_of(&doc, codes::INVALID_VALUE), [None, None]);
    }

    #[test]
    fn hostile_page_layout_falls_back_to_the_default_format() {
        // Margins at the limits of a length, and a negative page height.
        // The body must not wrap around to a plausible size, and reading
        // must not overflow (§5.4).
        for input in [
            &br#"<FinalDraft><PageLayout TopMargin="-21474836.48"/></FinalDraft>"#[..],
            br#"<FinalDraft><PageLayout><PageSize Height="-298261"/></PageLayout></FinalDraft>"#,
            br#"<FinalDraft><PageLayout TopMargin="21474836.47" BottomMargin="21474836.47"/><Content><Paragraph Type="Action"><Text>Still here.</Text></Paragraph></Content></FinalDraft>"#,
        ] {
            let doc = read(input).unwrap();
            assert_eq!(doc.format, PageFormat::default());
            assert_eq!(issues_of(&doc, codes::INVALID_FORMAT), [None]);
            fuzzing::read(input).unwrap();
        }
    }

    #[test]
    fn long_inserts_and_suffix_are_cut_to_the_format_bounds() {
        let more = "(MORE)".repeat(10);
        let dashes = "-".repeat(40);
        let input = format!(
            r#"<FinalDraft><MoresAndContinueds><DialogueBreaks BottomOfPage="Yes" DialogueBottom="{more}" TopOfNext="Yes" DialogueTop="(cont'd)&#10;and more"/></MoresAndContinueds><HeaderAndFooter><Header><Paragraph><DynamicLabel Type="Page #"/><Text>.{dashes}</Text></Paragraph></Header></HeaderAndFooter></FinalDraft>"#
        );
        let doc = read(input.as_bytes()).unwrap();
        assert_eq!(doc.format.more.as_deref(), Some(&more[..32]));
        assert_eq!(doc.format.contd.as_deref(), Some("(cont'd)"));
        assert_eq!(
            doc.format.page_numbers.suffix,
            format!(".{}", &dashes[..15])
        );
        assert_eq!(issues_of(&doc, codes::INVALID_VALUE), [None, None, None]);
        assert!(issues_of(&doc, codes::INVALID_FORMAT).is_empty());
        fuzzing::read(input.as_bytes()).unwrap();

        // A first line with no text keeps the default string.
        let doc = read(
            br#"<FinalDraft><MoresAndContinueds><DialogueBreaks BottomOfPage="Yes" DialogueBottom=" &#10;(MORE)"/></MoresAndContinueds></FinalDraft>"#,
        )
        .unwrap();
        assert_eq!(doc.format, PageFormat::default());
        assert_eq!(issues_of(&doc, codes::INVALID_VALUE), [None]);
        assert!(issues_of(&doc, codes::INVALID_FORMAT).is_empty());
    }

    #[test]
    fn spacing_that_overflows_is_reported() {
        // 12 pt times a Spacing of ten million does not fit a length.
        let doc = read(
            br#"<FinalDraft><ElementSettings Type="Action"><ParagraphSpec SpaceBefore="12" Spacing="10000000"/></ElementSettings></FinalDraft>"#,
        )
        .unwrap();
        assert_eq!(doc.format.elements.action.space_before, Pt::points(12));
        assert_eq!(issues_of(&doc, codes::INVALID_VALUE), [None]);
        assert!(issues_of(&doc, codes::LINE_SPACING).is_empty());

        // The most negative pair: their product, doubled for rounding, does
        // not fit an i64. The space left is negative, so the format falls
        // back to the default.
        let doc = read(
            br#"<FinalDraft><ElementSettings Type="Action"><ParagraphSpec SpaceBefore="-21474836.48" Spacing="-21474836.48"/></ElementSettings></FinalDraft>"#,
        )
        .unwrap();
        assert_eq!(doc.format, PageFormat::default());
        assert_eq!(issues_of(&doc, codes::INVALID_VALUE), [None]);
        assert_eq!(issues_of(&doc, codes::INVALID_FORMAT), [None]);
    }

    #[test]
    fn minimal_file() {
        let doc = read(b"<FinalDraft/>").unwrap();
        assert!(doc.script.elements.is_empty());
        assert_eq!(doc.format, PageFormat::default());
        let codes: Vec<_> = doc.report.iter().map(|i| i.code).collect();
        assert_eq!(
            codes,
            [
                codes::SECTION_MISSING,
                codes::SETTINGS_MISSING,
                codes::NO_CONTENT
            ]
        );
    }
}
