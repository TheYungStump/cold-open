//! Property tests: random and mutated input through `fuzzing::read` and
//! `fuzzing::roundtrip`, the same checks the cargo-fuzz targets run (no
//! panic, the same result twice, a document that satisfies the model
//! rules, and the round trip of docs/specs/fdx.md §16.1); and generated
//! documents through `fuzzing::write_document` (§16.2).

use std::path::Path;
use std::sync::OnceLock;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;

fn fixtures() -> &'static [Vec<u8>] {
    static FIXTURES: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx");
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "fdx"))
            .collect();
        paths.sort();
        paths.iter().map(|p| std::fs::read(p).unwrap()).collect()
    })
}

fn check(data: &[u8]) -> Result<(), TestCaseError> {
    coldopen_fdx::fuzzing::read(data).map_err(TestCaseError::fail)?;
    coldopen_fdx::fuzzing::roundtrip(data).map_err(TestCaseError::fail)
}

/// One edit of a byte string: replace, delete, insert, truncate or
/// duplicate.
fn mutate(bytes: &mut Vec<u8>, at: Index, kind: u8, byte: u8, len: usize) {
    const INTERESTING: [u8; 10] = [b'<', b'>', b'&', b'/', b'"', b'=', b';', 0, 0xFF, b'\n'];
    if bytes.is_empty() {
        bytes.push(byte);
        return;
    }
    let pos = at.index(bytes.len());
    let end = (pos + len).min(bytes.len());
    match kind {
        0 => bytes[pos] = byte,
        1 => {
            bytes.drain(pos..end);
        }
        2 => bytes.insert(pos, INTERESTING[usize::from(byte) % INTERESTING.len()]),
        3 => bytes.truncate(pos),
        _ => {
            let copy = bytes[pos..end].to_vec();
            bytes.splice(pos..pos, copy);
        }
    }
}

const TOKENS: &[&str] = &[
    "<Content>",
    "</Content>",
    "<Paragraph Type=\"Action\">",
    "<Paragraph Type=\"Character\">",
    "<Paragraph Type=\"Dialogue\">",
    "<Paragraph Type=\"Scene Heading\" Number=\" 7 \">",
    "<Paragraph Type=\"New Act\">",
    "<Paragraph StartsNewPage=\"Yes\" Type=\"General\">",
    "<Paragraph Alignment=\"Center\" SpaceBefore=\"24\" LeftIndent=\"2.5\" Type=\"Action\">",
    "<Paragraph SpaceBefore=\"-21474836.48\" Type=\"Action\">",
    "<Paragraph SpaceBefore=\"21474836.47\" Type=\"Dialogue\">",
    "<Paragraph>",
    "</Paragraph>",
    "<DualDialogue>",
    "</DualDialogue>",
    "<Text>",
    "<Text Style=\"Bold+AllCaps\">",
    "<Text Font=\"Arial\" RevisionID=\"3\">",
    "</Text>",
    "<Text/>",
    "<SceneProperties Page=\"2\" Length=\"1/8\"/>",
    "words ",
    "  spaced  ",
    "\n",
    "\t",
    "&amp;",
    "&#x2019;",
    "&nope;",
    "<![CDATA[<raw>]]>",
    "<ElementSettings Type=\"Action\"><ParagraphSpec LeftIndent=\"1.5\" RightIndent=\"7.5\" SpaceBefore=\"12\" Spacing=\"2\"/><FontSpec Style=\"Underline\"/><Behavior PaginateAs=\"Shot\"/></ElementSettings>",
    "<ElementSettings Type=\"Dialogue\"><ParagraphSpec LeftIndent=\"9\" RightIndent=\"1\"/></ElementSettings>",
    "<ElementSettings Type=\"Shot\"><ParagraphSpec SpaceBefore=\"-21474836.48\" Spacing=\"-21474836.48\"/></ElementSettings>",
    "<ElementSettings Type=\"Lyric\"><ParagraphSpec SpaceBefore=\"12\" Spacing=\"10000000\"/></ElementSettings>",
    "<PageLayout TopMargin=\"72\" BottomMargin=\"72\" HeaderMargin=\"36\"><PageSize Width=\"8.5\" Height=\"11\"/></PageLayout>",
    "<PageLayout TopMargin=\"700\"/>",
    "<MoresAndContinueds><DialogueBreaks BottomOfPage=\"Yes\" DialogueBottom=\" \" TopOfNext=\"No\"/><SceneBreaks SceneTopOfNext=\"Yes\"/></MoresAndContinueds>",
    "<HeaderAndFooter HeaderVisible=\"Yes\"><Header><Paragraph><DynamicLabel Type=\"Page #\"/><Text>.\tx</Text></Paragraph></Header></HeaderAndFooter>",
    "<TitlePage><Content><Paragraph Alignment=\"Full\" LeftIndent=\"8\" RightIndent=\"1\"><Text>t</Text></Paragraph></Content></TitlePage>",
    "<TitlePage><Content><Paragraph/><Paragraph SpaceBefore=\"21474836.47\"><Text>t</Text></Paragraph></Content></TitlePage>",
    "</Bogus>",
    "<a>",
    // Markup holding a character XML does not allow (2026-10-17).
    "<?p \u{0}?>",
    "<!--\u{0}-->",
    "<![CDATA[]]\u{0}>]]>",
    "\u{0}",
];

proptest! {
    // 256 cases by default; set PROPTEST_CASES for a longer run.
    #![proptest_config(ProptestConfig::default())]

    #[test]
    fn random_bytes_never_panic(data in vec(any::<u8>(), 0..1024)) {
        check(&data)?;
        let mut framed = b"<FinalDraft>".to_vec();
        framed.extend_from_slice(&data);
        check(&framed)?;
    }

    #[test]
    fn mutated_fixtures_never_panic(
        which in any::<Index>(),
        edits in vec((any::<Index>(), 0u8..5, any::<u8>(), 0usize..64), 1..8),
    ) {
        let all = fixtures();
        prop_assume!(!all.is_empty());
        let mut bytes = all[which.index(all.len())].clone();
        for (at, kind, byte, len) in edits {
            mutate(&mut bytes, at, kind, byte, len);
        }
        check(&bytes)?;
    }

    #[test]
    fn random_documents_never_panic(tokens in vec(any::<Index>(), 0..60)) {
        let mut doc = String::from("<?xml version=\"1.0\"?><FinalDraft>");
        for t in tokens {
            doc.push_str(TOKENS[t.index(TOKENS.len())]);
        }
        check(doc.as_bytes())?;
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(doc.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        check(&utf16)?;
    }

    #[test]
    fn generated_documents_write_and_read_back(data in vec(any::<u8>(), 0..400)) {
        let (elements, spacing) = coldopen_fdx::fuzzing::document_from_bytes(&data);
        coldopen_fdx::fuzzing::write_document(&elements, &spacing)
            .map_err(TestCaseError::fail)?;
    }
}

#[test]
fn every_fixture_passes_the_fuzz_checks() {
    for f in fixtures() {
        coldopen_fdx::fuzzing::read(f).unwrap();
        coldopen_fdx::fuzzing::roundtrip(f).unwrap();
    }
}

/// Found by the `fdx_roundtrip` fuzz target in CI (2026-10-17): a
/// processing instruction holding a NUL. The NUL was taken out when the
/// file was written back, and `<?\0>?>` became `<?>?>`, which no longer
/// reads as one processing instruction. Markup holding such a character is
/// now reported and left out whole (fdx.md §9, `invalid_char`).
#[test]
fn markup_holding_a_character_xml_forbids_is_left_out_whole() {
    for input in [
        "<FinalDraft><?\0>?>",
        "<?\0?><FinalDraft>",
        "<FinalDraft><!--a-\0->--></FinalDraft>",
        "<FinalDraft><Content><![CDATA[a]]\0>b]]></Content></FinalDraft>",
        "<!DOCTYPE x\0><FinalDraft></FinalDraft>",
        "<FinalDraft></FinalDraft><?p \0?>",
        "<FinalDraft><PageLayout><?p \0?></PageLayout></FinalDraft>",
        // Found by the property tests at 20,000 cases: in the declaration.
        "<?xml version=\"1.\0\"?><FinalDraft></FinalDraft>",
    ] {
        check(input.as_bytes()).unwrap_or_else(|e| panic!("{input:?}: {e}"));
        let doc = coldopen_fdx::read(input.as_bytes()).unwrap();
        assert!(
            doc.report.iter().any(|i| i.code == "invalid_char"),
            "{input:?}"
        );
    }
}

/// Found by the `fdx_write` fuzz target in CI (2026-10-17): a section
/// between the second speaker's dialogue and a parenthetical. .fdx leaves
/// the section out, so reading the file back joined the parenthetical to
/// the dual dialogue and writing it again gave other text. Groups are now
/// found without what .fdx leaves out (fdx.md §14.7).
#[test]
fn a_section_inside_dual_dialogue_does_not_split_it() {
    use coldopen_screenplay::{Element, ElementKind};
    let mut second = Element::new(ElementKind::Character, "BEA");
    second.dual = true;
    let elements = [
        Element::new(ElementKind::Character, "ADA"),
        Element::new(ElementKind::Dialogue, "Left."),
        second,
        Element::new(ElementKind::Dialogue, "Right."),
        Element::new(ElementKind::Section, "Act two"),
        Element::new(ElementKind::Parenthetical, "(still right)"),
    ];
    coldopen_fdx::fuzzing::write_document(&elements, &Default::default()).unwrap();
}
