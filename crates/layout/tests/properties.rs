//! Property tests for pagination (docs/specs/layout.md §7).

use coldopen_layout::{BreakCause, Engine, LineKind, Options, Page, ValidFormat};
use coldopen_screenplay::{Element, ElementKind, PageFormat, Pt, Script, SplitMode};
use proptest::prelude::*;

const WORDS: &[&str] = &[
    "Rain",
    "falls.",
    "She",
    "runs!",
    "Mr.",
    "Smith",
    "waits",
    "...",
    "Why?",
    "the",
    "a",
    "well-known",
    "日本",
    "café",
    "x",
    "Then",
    "OK.",
    "“Go.”",
    "longwordwithoutanybreaks",
];

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec((0..WORDS.len(), 0u8..10), 0..14).prop_map(|ws| {
        let mut s = String::new();
        for (i, (w, nl)) in ws.into_iter().enumerate() {
            if i > 0 {
                s.push(if nl == 0 { '\n' } else { ' ' });
            }
            s.push_str(WORDS[w]);
        }
        s
    })
}

fn element() -> impl Strategy<Value = Element> {
    use ElementKind::*;
    let kinds = prop::sample::select(vec![
        SceneHeading,
        Action,
        Action,
        Action,
        Character,
        Character,
        Parenthetical,
        Dialogue,
        Dialogue,
        Dialogue,
        Transition,
        Lyric,
        Shot,
        General,
        PageBreak,
        Section,
        Note,
    ]);
    (kinds, text(), any::<bool>(), prop::option::of(0u8..4)).prop_map(|(k, t, flag, sp)| {
        let t = match k {
            PageBreak => String::new(),
            Character => {
                if flag {
                    "BOB (V.O.)".into()
                } else {
                    "MAYA".into()
                }
            }
            Parenthetical => "(beat)".into(),
            Transition => "CUT TO:".into(),
            _ => t,
        };
        let mut e = Element::new(k, t);
        if k == Character && flag {
            e.dual = true;
        }
        if k == Action && flag {
            e.centered = true;
        }
        if k.is_printing() {
            e.space_before = sp;
        }
        e
    })
}

fn format() -> impl Strategy<Value = PageFormat> {
    (
        3i32..20,
        0i32..12,
        8i32..40,
        8i32..30,
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(lines, extra, wide, narrow, mid, sentences, margin, inserts)| {
                let mut f = PageFormat::default();
                f.page_height = Pt(f.margin_top.0 + f.margin_bottom.0 + lines * 1200 + extra * 100);
                let set = |e: &mut coldopen_screenplay::ElementFormat, cells: i32| {
                    e.right = Pt(e.left.0 + cells * 720 + 360);
                };
                for k in [
                    ElementKind::SceneHeading,
                    ElementKind::Action,
                    ElementKind::General,
                    ElementKind::Shot,
                ] {
                    set(f.elements.get_mut(k).unwrap(), wide);
                }
                for k in [
                    ElementKind::Dialogue,
                    ElementKind::Lyric,
                    ElementKind::Character,
                ] {
                    set(f.elements.get_mut(k).unwrap(), narrow);
                }
                set(&mut f.elements.parenthetical, narrow.min(16));
                f.split_mode = if mid {
                    SplitMode::MidLine
                } else {
                    SplitMode::LineEnd
                };
                f.break_at_sentences = sentences;
                f.more_contd_in_margin = margin;
                if !inserts {
                    f.more = None;
                    f.contd = None;
                }
                f
            },
        )
}

fn script() -> impl Strategy<Value = Script> {
    prop::collection::vec(element(), 0..40).prop_map(|elements| Script {
        elements,
        ..Default::default()
    })
}

fn content_lines(p: &Page) -> impl Iterator<Item = &coldopen_layout::PlacedLine> {
    p.lines
        .iter()
        .filter(|l| matches!(l.kind, LineKind::Content(_)))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn pagination_properties(script in script(), format in format()) {
        let Ok(vf) = ValidFormat::new(format.clone()) else {
            return Err(TestCaseError::reject("invalid format"));
        };
        let engine = Engine::new(&script, &vf, &Options::default());
        let layout = engine.run();

        // P8: deterministic.
        let again = Engine::new(&script, &vf, &Options::default()).run();
        prop_assert_eq!(&again.pages, &layout.pages);

        let lh = format.line_height.0;
        let top = format.margin_top.0;
        let body = format.body_height().0;
        for (i, p) in layout.pages.iter().enumerate() {
            // P1: progress. P9: a page can be laid out again from its start.
            prop_assert!(p.next > p.start, "page {} made no progress", i + 1);
            prop_assert_eq!(engine.page_from(p.start).next, p.next);
            // P4: every page holds script text.
            prop_assert!(content_lines(p).next().is_some(), "page {} is empty", i + 1);
            // P3: content stays inside the body.
            for l in content_lines(p) {
                let line_top = l.y.0 - format.baseline.0 - top;
                prop_assert!(line_top >= 0 && line_top + lh <= body,
                    "page {} line {:?} at {} outside the body (body {})\n{}\n{:?}", i + 1, l.text, line_top, body,
                    layout.page_view(&coldopen_layout::AllGlyphs),
                    script.elements.iter().map(|e| (e.kind, e.text.plain_text(), e.space_before, e.dual)).collect::<Vec<_>>());
            }
            if !format.more_contd_in_margin {
                for l in p.lines.iter().filter(|l| matches!(l.kind, LineKind::More | LineKind::ContdName)) {
                    let line_top = l.y.0 - format.baseline.0 - top;
                    prop_assert!(line_top >= 0 && line_top + lh <= body,
                        "page {} insert {:?} at {} outside the body (body {})\n{}\n{:?}", i + 1, l.text, line_top, body,
                        layout.page_view(&coldopen_layout::AllGlyphs),
                        script.elements.iter().map(|e| (e.kind, e.text.plain_text(), e.space_before, e.dual)).collect::<Vec<_>>());
                }
            }
            // P7: (MORE) at the foot of a page exactly when the next page
            // starts with a continued name.
            if format.more.is_some() && format.contd.is_some() {
                let more = p.lines.iter().any(|l| l.kind == LineKind::More);
                let next_contd = layout.pages.get(i + 1)
                    .is_some_and(|n| n.lines.iter().any(|l| l.kind == LineKind::ContdName));
                let dropped = layout.pages.get(i + 1)
                    .is_some_and(|n| n.diagnostics.iter().any(|d| d.code == "inserts_dropped"))
                    || p.diagnostics.iter().any(|d| d.code == "inserts_dropped");
                if !dropped {
                    prop_assert_eq!(more, next_contd, "page {}", i + 1);
                }
            }
            // P5: at a natural break the last line is not a heading, name or
            // parenthetical, and the next page doesn't open with a transition.
            if p.cause == BreakCause::Natural {
                if let Some(last) = content_lines(p).max_by_key(|l| l.y) {
                    prop_assert!(!matches!(last.kind,
                        LineKind::Content(ElementKind::SceneHeading | ElementKind::Shot
                            | ElementKind::Character | ElementKind::Parenthetical)),
                        "page {} ends with {:?}\n{}\n{:?}", i + 1, last.kind,
                        layout.page_view(&coldopen_layout::AllGlyphs),
                        script.elements.iter().map(|e| (e.kind, e.text.plain_text(), e.space_before, e.dual)).collect::<Vec<_>>());
                }
                if let Some(n) = layout.pages.get(i + 1) {
                    let first = content_lines(n).min_by_key(|l| l.y);
                    prop_assert!(!first.is_some_and(|l| l.kind == LineKind::Content(ElementKind::Transition)),
                        "page {} opens with a transition", i + 2);
                }
            }
        }

        // P2: every visible character of every element prints exactly once.
        for (e, el) in script.elements.iter().enumerate() {
            if !el.kind.is_printing() || el.kind == ElementKind::Character {
                continue;
            }
            let src = el.text.plain_text();
            let mut count = vec![0u32; src.len()];
            for p in &layout.pages {
                for l in content_lines(p) {
                    if let Some((le, a, b)) = l.source
                        && le == e
                    {
                        for c in count.iter_mut().take(b as usize).skip(a as usize) {
                            *c += 1;
                        }
                    }
                }
            }
            for (i, ch) in src.char_indices() {
                if ch.is_whitespace() || ch.is_control() {
                    continue;
                }
                prop_assert_eq!(count[i], 1, "element {} byte {} ({:?}) printed {} times in {:?}",
                    e, i, ch, count[i], src);
            }
        }
    }
}
