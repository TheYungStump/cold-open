//! Incremental layout must equal a full layout after every edit
//! (docs/specs/layout.md §15.1).
// PROPTEST_CASES only changes how many cases run, never a layout.
#![allow(clippy::disallowed_methods)]

use coldopen_layout::{Engine, Incremental, Options, ValidFormat};
use coldopen_screenplay::{Element, ElementKind, PageFormat, Pt, Script};
use proptest::prelude::*;

const WORDS: &[&str] = &[
    "Rain",
    "falls.",
    "She",
    "runs!",
    "Mr.",
    "Smith",
    "waits",
    "Why?",
    "the",
    "a",
    "well-known",
    "café",
    "OK.",
    "Then",
];

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec((0..WORDS.len(), 0u8..12), 0..16).prop_map(|ws| {
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
        PageBreak,
        Section,
        Note,
    ]);
    (kinds, text(), any::<bool>(), prop::option::of(0u8..3)).prop_map(|(k, t, flag, sp)| {
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
            SceneHeading => "INT. LAB - NIGHT".into(),
            _ => t,
        };
        let mut e = Element::new(k, t);
        if k == Character && flag {
            e.dual = true;
        }
        if k.is_printing() {
            e.space_before = sp;
        }
        e
    })
}

#[derive(Clone, Debug)]
enum Edit {
    Insert(usize, Element),
    Delete(usize),
    Replace(usize, Element),
    /// Types a word at the end of an element, as the editor does.
    Type(usize, &'static str),
    /// Moves a run of elements elsewhere, as `scene.move` does.
    Move(usize, usize, usize),
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        1 => (any::<usize>(), element()).prop_map(|(i, e)| Edit::Insert(i, e)),
        1 => any::<usize>().prop_map(Edit::Delete),
        1 => (any::<usize>(), element()).prop_map(|(i, e)| Edit::Replace(i, e)),
        3 => (any::<usize>(), prop::sample::select(&[" x", " Rain.", "y", "\nWhy?"][..]))
            .prop_map(|(i, w)| Edit::Type(i, w)),
        1 => (any::<usize>(), 1usize..6, any::<usize>()).prop_map(|(a, n, b)| Edit::Move(a, n, b)),
    ]
}

fn apply(s: &mut Script, e: &Edit) {
    let n = s.elements.len();
    match e {
        Edit::Insert(i, el) => s.elements.insert(i % (n + 1), el.clone()),
        Edit::Delete(i) if n > 0 => {
            s.elements.remove(i % n);
        }
        Edit::Replace(i, el) if n > 0 => s.elements[i % n] = el.clone(),
        Edit::Type(i, w) if n > 0 => {
            let el = &mut s.elements[i % n];
            if el.kind != ElementKind::PageBreak && !el.kind.is_raw_text() {
                let t = format!("{}{w}", el.text.plain_text());
                el.text = t.into();
            }
        }
        Edit::Move(a, len, b) if n > 0 => {
            let a = a % n;
            let len = (*len).min(n - a);
            let moved: Vec<Element> = s.elements.drain(a..a + len).collect();
            let b = b % (s.elements.len() + 1);
            s.elements.splice(b..b, moved);
        }
        _ => {}
    }
}

fn small_format(lines: i32, contd_after_action: bool) -> ValidFormat {
    let mut f = PageFormat::default();
    f.page_height = Pt(f.margin_top.0 + f.margin_bottom.0 + lines * 1200);
    f.contd_after_action = contd_after_action;
    ValidFormat::new(f).unwrap()
}

fn check(start: Vec<Element>, edits: Vec<Edit>, format: ValidFormat, options: Options) {
    let mut script = Script {
        elements: start,
        ..Default::default()
    };
    let mut inc = Incremental::new(&script, &format, &options);
    assert_eq!(
        inc.layout(),
        &Engine::new(&script, &format, &options).run(),
        "initial layout"
    );
    for (k, e) in edits.iter().enumerate() {
        apply(&mut script, e);
        let got = inc.update(&script).clone();
        let want = Engine::new(&script, &format, &options).run();
        assert_eq!(got, want, "after edit {k} ({e:?}); stats {:?}", inc.stats());
    }
}

/// The usual number of cases, or `PROPTEST_CASES` for a deeper run.
fn cases(usual: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(usual)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(200), ..ProptestConfig::default() })]

    #[test]
    fn equals_a_full_layout(
        start in prop::collection::vec(element(), 0..60),
        edits in prop::collection::vec(edit(), 1..12),
        lines in 3i32..20,
        contd in any::<bool>(),
        numbers in any::<bool>(),
    ) {
        let options = Options { scene_numbers: numbers, ..Default::default() };
        check(start, edits, small_format(lines, contd), options);
    }
}

#[test]
fn typing_late_in_a_long_script_lays_out_few_pages() {
    let mut elements = Vec::new();
    for k in 0..300 {
        elements.push(Element::new(
            ElementKind::SceneHeading,
            format!("INT. ROOM {k} - DAY"),
        ));
        elements.push(Element::new(
            ElementKind::Action,
            "The radiator ticks. Rain runs down the glass in long lines. Nobody moves.",
        ));
        elements.push(Element::new(ElementKind::Character, "MAYA"));
        elements.push(Element::new(
            ElementKind::Dialogue,
            "We should have left before the roads closed.",
        ));
    }
    let mut script = Script {
        elements,
        ..Default::default()
    };
    let format = ValidFormat::default();
    let options = Options::default();
    let mut inc = Incremental::new(&script, &format, &options);
    let pages = inc.layout().pages.len();
    assert!(pages >= 40, "{pages}");
    let at = script.elements.len() * 3 / 4 + 1;
    script.elements[at].text =
        format!("{} More rain.", script.elements[at].text.plain_text()).into();
    inc.update(&script);
    let st = inc.stats().clone();
    assert!(st.laid_out <= 4, "{st:?}");
    assert!(st.reused_before + st.reused_after + st.laid_out == inc.layout().pages.len());
    assert_eq!(inc.layout(), &Engine::new(&script, &format, &options).run());
}

#[test]
fn draft_lines_match_a_single_page_and_mark_page_starts() {
    use coldopen_layout::LineKind;
    let short = Script {
        elements: vec![
            Element::new(ElementKind::SceneHeading, "INT. LAB - NIGHT"),
            Element::new(ElementKind::Action, "Rain. ".repeat(30)),
            Element::new(ElementKind::Character, "MAYA"),
            Element::new(ElementKind::Parenthetical, "(beat)"),
            Element::new(ElementKind::Dialogue, "We should go. Now."),
        ],
        ..Default::default()
    };
    let format = ValidFormat::default();
    let options = Options::default();
    let e = Engine::new(&short, &format, &options);
    let layout = e.run();
    assert_eq!(layout.pages.len(), 1);
    let content: Vec<_> = layout.pages[0]
        .lines
        .iter()
        .filter(|l| matches!(l.kind, LineKind::Content(_)))
        .cloned()
        .collect();
    let draft = e.draft(&layout);
    assert_eq!(draft.lines, content);
    assert_eq!(draft.page_tops, vec![coldopen_screenplay::Pt::ZERO]);

    let long = coldopen_layout::synthetic::feature_script(1);
    let inc = Incremental::new(&long, &format, &options);
    let d = inc.draft();
    let e = Engine::new(&long, &format, &options);
    assert_eq!(d, e.draft(&e.run()));
    assert_eq!(d.page_tops.len(), inc.layout().pages.len());
    assert!(d.page_tops.windows(2).all(|w| w[0] < w[1]));
    assert!(d.height.0 > d.page_tops.last().unwrap().0);
}
