//! Whatever the .fdx reader accepts, layout and PDF export accept too: no
//! panic, a valid format, and deterministic output. The cargo-fuzz target
//! `fdx_read` checks the same chain.

use std::fs;
use std::path::Path;

use coldopen_layout::pdf::{self, PdfOptions};
use coldopen_layout::{AllGlyphs, Engine, Fonts, Options, ValidFormat};
use proptest::prelude::*;

fn chain(bytes: &[u8], fonts: &Fonts) -> Result<(), String> {
    chain_with(bytes, fonts, false).map(|_| ())
}

/// Reads, lays out and exports `bytes` twice. Returns the page view, or
/// `None` when the reader refused the input.
fn chain_with(bytes: &[u8], fonts: &Fonts, scene_numbers: bool) -> Result<Option<String>, String> {
    let Ok(doc) = coldopen_fdx::read(bytes) else {
        return Ok(None);
    };
    let format = ValidFormat::new(doc.format.clone())
        .map_err(|p| format!("the reader returned an invalid format: {p:?}"))?;
    let options = Options {
        scene_numbers,
        overrides: doc.overrides.clone(),
        title_lines: Some(doc.title_lines.clone()),
        ..Options::default()
    };
    let layout = Engine::new(&doc.script, &format, &options).run();
    for p in &layout.pages {
        if p.next <= p.start {
            return Err("pagination made no progress".into());
        }
    }
    let a = pdf::write(&layout, fonts, &PdfOptions::default()).map_err(|e| e.to_string())?;
    let b = pdf::write(&layout, fonts, &PdfOptions::default()).map_err(|e| e.to_string())?;
    if a != b {
        return Err("PDF output is not deterministic".into());
    }
    Ok(Some(layout.page_view(&AllGlyphs)))
}

/// Files a review found to panic in debug builds, or to lose text, before
/// the reader bounded what it takes from a file (docs/specs/fdx.md §5 to
/// §7). Each is written for the purpose.
fn hostile() -> Vec<(&'static str, String)> {
    let actions = |space: &str, n: usize| -> String {
        (1..=n)
            .map(|i| {
                format!(
                    r#"<Paragraph Type="Action" SpaceBefore="{space}"><Text>Para {i}.</Text></Paragraph>"#
                )
            })
            .collect()
    };
    let wrap = |inner: String| format!("<FinalDraft>{inner}</FinalDraft>");
    vec![
        (
            "a top margin of minus the largest length",
            wrap(r#"<PageLayout TopMargin="-21474836.48"/>"#.into()),
        ),
        (
            "a negative page height",
            wrap(r#"<PageLayout><PageSize Height="-298261"/></PageLayout>"#.into()),
        ),
        (
            "margins that wrap the body around",
            wrap(format!(
                r#"<PageLayout TopMargin="21474836.47" BottomMargin="21474836.47"/><Content>{}</Content>"#,
                actions("12", 1)
            )),
        ),
        (
            "the largest paragraph space",
            wrap(format!(
                r#"<Content><Paragraph Type="Action"><Text>First.</Text></Paragraph>{}</Content>"#,
                actions("21474836.47", 2)
            )),
        ),
        (
            "large negative paragraph spaces",
            wrap(format!("<Content>{}</Content>", actions("-15000000", 3))),
        ),
        (
            "small negative paragraph spaces",
            wrap(format!("<Content>{}</Content>", actions("-12", 20))),
        ),
        (
            "the largest title-page space",
            wrap(
                r#"<TitlePage><Content><Paragraph><Text>TITLE</Text></Paragraph><Paragraph SpaceBefore="21474836.47"><Text>below</Text></Paragraph></Content></TitlePage>"#
                    .into(),
            ),
        ),
        (
            "a space and spacing whose product overflows",
            wrap(
                r#"<ElementSettings Type="Action"><ParagraphSpec SpaceBefore="-21474836.48" Spacing="-21474836.48"/></ElementSettings>"#
                    .into(),
            ),
        ),
        (
            "a spacing too large to apply",
            wrap(format!(
                r#"<ElementSettings Type="Action"><ParagraphSpec SpaceBefore="12" Spacing="10000000"/></ElementSettings><Content>{}</Content>"#,
                actions("12", 2)
            )),
        ),
        (
            "a scene number far off the page",
            wrap(
                r#"<SceneNumberOptions RightLocation="298261"/><Content><Paragraph Type="Scene Heading" Number="1234"><Text>INT. ATTIC - NIGHT</Text></Paragraph></Content>"#
                    .into(),
            ),
        ),
        (
            "a page-number suffix of a hundred thousand characters",
            wrap(format!(
                r#"<HeaderAndFooter HeaderVisible="Yes" HeaderFirstPage="Yes"><Header><Paragraph><DynamicLabel Type="Page #"/><Text>{}</Text></Paragraph></Header></HeaderAndFooter><Content>{}</Content>"#,
                ".".repeat(100_000),
                actions("0", 1)
            )),
        ),
        (
            "insert strings over many lines",
            wrap(format!(
                r#"<MoresAndContinueds><DialogueBreaks BottomOfPage="Yes" DialogueBottom="{0}" TopOfNext="Yes" DialogueTop="{0}"/></MoresAndContinueds><Content><Paragraph Type="Character"><Text>MAYA</Text></Paragraph><Paragraph Type="Dialogue"><Text>{1}</Text></Paragraph></Content>"#,
                "(more)&#10;".repeat(1_000),
                "Words go on. ".repeat(400)
            )),
        ),
    ]
}

#[test]
fn hostile_files_lay_out_and_keep_their_text() {
    let fonts = Fonts::new().unwrap();
    for (what, input) in hostile() {
        let doc = coldopen_fdx::read(input.as_bytes()).unwrap();
        for scene_numbers in [false, true] {
            let view = chain_with(input.as_bytes(), &fonts, scene_numbers)
                .unwrap_or_else(|e| panic!("{what}: {e}"))
                .expect("an .fdx document");
            for e in doc.script.elements.iter().filter(|e| !e.text.is_empty()) {
                let text = e.text.plain_text();
                let first = text
                    .split_whitespace()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(
                    view.contains(&first),
                    "{what}: {first:?} is missing:\n{view}"
                );
            }
        }
    }
}

fn fixtures() -> Vec<Vec<u8>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx");
    let mut names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "fdx"))
        .collect();
    names.sort();
    names.into_iter().map(|p| fs::read(p).unwrap()).collect()
}

#[test]
fn every_fdx_fixture_lays_out_and_exports() {
    let fonts = Fonts::new().unwrap();
    for (i, f) in fixtures().iter().enumerate() {
        chain(f, &fonts).unwrap_or_else(|e| panic!("fixture {i}: {e}"));
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

    #[test]
    fn mutated_fixtures_never_break_layout(
        pick in any::<prop::sample::Index>(),
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..8),
    ) {
        let fixtures = fixtures();
        let mut bytes = pick.get(&fixtures).clone();
        for (at, b) in edits {
            if !bytes.is_empty() {
                let i = at.index(bytes.len());
                bytes[i] = b;
            }
        }
        let fonts = Fonts::new().unwrap();
        prop_assert_eq!(chain(&bytes, &fonts), Ok(()));
    }
}
