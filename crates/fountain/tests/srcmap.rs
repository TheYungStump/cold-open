//! Property tests for the normal form's source map (docs/specs/fountain.md
//! §10): on arbitrary scripts, not just normal-form ones, every segment is
//! a real copy, nothing printed is lost or mapped twice, and positions
//! round-trip.

use coldopen_fountain::{normalize_with_map, normalize_with_source_map};
use coldopen_screenplay::{Element, ElementKind, RichText, Run, Script, Style};
use proptest::prelude::*;

const PIECES: &[&str] = &[
    "Rain", " ", "  ", "\t", "\n", "\n\n", "é", "ñu", "MAYA", "x y", ".", " (V.O.)", "日本",
];

fn kind() -> impl Strategy<Value = ElementKind> {
    prop::sample::select(vec![
        ElementKind::SceneHeading,
        ElementKind::Action,
        ElementKind::Character,
        ElementKind::Parenthetical,
        ElementKind::Dialogue,
        ElementKind::Transition,
        ElementKind::Lyric,
        ElementKind::General,
        ElementKind::Note,
        ElementKind::Synopsis,
    ])
}

fn rich() -> impl Strategy<Value = RichText> {
    prop::collection::vec(
        prop_oneof![
            4 => (prop::sample::select(PIECES), any::<bool>()).prop_map(|(t, b)| Run::styled(
                t,
                Style { bold: b, italic: false, underline: false }
            )),
            1 => Just(Run::Note("n".into())),
        ],
        0..6,
    )
    .prop_map(RichText::new)
}

fn element() -> impl Strategy<Value = Element> {
    (kind(), rich(), any::<bool>(), prop::option::of(0u8..3)).prop_map(|(k, t, c, sp)| {
        let mut e = Element::new(k, t);
        if k.is_raw_text() {
            e.text = RichText::plain(e.text.plain_text());
        }
        if k == ElementKind::Action {
            e.centered = c;
        }
        if k.is_printing() {
            e.space_before = sp;
        }
        e
    })
}

fn is_char_boundary(s: &str, b: u32) -> bool {
    s.is_char_boundary(b as usize)
}

/// The usual number of cases, or `PROPTEST_CASES` for a deeper run. An
/// explicit `cases` in the config would otherwise ignore the variable.
fn cases(usual: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usual)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(512), ..ProptestConfig::default() })]

    #[test]
    fn segments_copy_and_cover(elements in prop::collection::vec(element(), 0..8)) {
        let script = Script { elements, ..Default::default() };
        let (norm, map) = normalize_with_source_map(&script);
        let (_, plain_map) = normalize_with_map(&script);
        prop_assert_eq!(map.elements.len(), norm.elements.len());
        let sources: Vec<String> = script.elements.iter().map(|e| e.text.plain_text()).collect();
        // Source bytes covered, to check nothing is mapped twice.
        let mut covered: Vec<Vec<bool>> = sources.iter().map(|s| vec![false; s.len()]).collect();
        let mut last_source = 0usize;
        for (k, (e, m)) in norm.elements.iter().zip(&map.elements).enumerate() {
            prop_assert_eq!(m.source, plain_map[k]);
            let t = e.text.plain_text();
            let mut prev_end = 0u32;
            for seg in &m.segments {
                prop_assert!(seg.norm.start >= prev_end, "segments overlap or go back");
                prev_end = seg.norm.end;
                prop_assert!(seg.source >= last_source, "sources go back");
                last_source = seg.source;
                let n = &t[seg.norm.start as usize..seg.norm.end as usize];
                let src = &sources[seg.source];
                let len = seg.norm.end - seg.norm.start;
                let tab = n == " " && src[seg.at as usize..].starts_with('\t');
                if tab {
                    covered[seg.source][seg.at as usize] = true;
                    continue;
                }
                let o = &src[seg.at as usize..(seg.at + len) as usize];
                prop_assert_eq!(n, o);
                for b in seg.at..seg.at + len {
                    prop_assert!(!covered[seg.source][b as usize], "byte mapped twice");
                    covered[seg.source][b as usize] = true;
                }
                // Positions inside a copied segment round-trip.
                for b in seg.norm.start..seg.norm.end {
                    if is_char_boundary(&t, b) {
                        let (src_el, src_b) = map.to_source(k, b).unwrap();
                        prop_assert_eq!(map.to_norm(src_el, src_b), Some((k, b)));
                    }
                }
            }
            // Text the normal form added is only joining newlines and the
            // spaces of widened tabs.
            let mapped: u32 = m.segments.iter().map(|s| s.norm.end - s.norm.start).sum();
            let unmapped: String = {
                let mut out = String::new();
                let mut i = 0u32;
                for seg in &m.segments {
                    out.push_str(&t[i as usize..seg.norm.start as usize]);
                    i = seg.norm.end;
                }
                out.push_str(&t[i as usize..]);
                out
            };
            prop_assert!(unmapped.chars().all(|c| c == '\n'), "unmapped {:?}", unmapped);
            let _ = mapped;
        }
        // Every printed character that is not whitespace is mapped.
        for (i, s) in sources.iter().enumerate() {
            for (b, c) in s.char_indices() {
                if !c.is_whitespace() {
                    prop_assert!(covered[i][b], "element {} byte {} ({:?}) unmapped", i, b, c);
                }
            }
        }
    }
}
