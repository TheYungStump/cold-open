//! Property tests: random and mutated input through `fuzzing::read`, the
//! same checks the cargo-fuzz target runs (no panic, the same result twice,
//! and a document that satisfies the model rules).

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
    coldopen_fdx::fuzzing::read(data).map_err(TestCaseError::fail)
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
}

#[test]
fn every_fixture_passes_the_fuzz_checks() {
    for f in fixtures() {
        coldopen_fdx::fuzzing::read(f).unwrap();
    }
}
