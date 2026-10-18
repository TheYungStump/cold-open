//! Property tests for the round-trip guarantees in docs/specs/fountain.md §1.

use coldopen_fountain::{normalize, parse, write};
use coldopen_screenplay::{
    Element, ElementKind, RichText, Run, Script, Style, TitleField, TitlePage,
};
use proptest::prelude::*;

/// Fragments chosen to hit every rule of the syntax, including the awkward
/// ones, so random lines look like real (and broken) Fountain.
const FRAGMENTS: &[&str] = &[
    "INT. HOUSE - DAY",
    "EXT. ROAD",
    "int. car",
    "I/E BOOTH",
    "INT./EXT. VAN",
    "E/I CAR",
    "EXT/INT. BUS",
    "EXT/INTERIOR",
    "EST",
    ".FORCED",
    "..ellipsis",
    "!ACTION",
    "!",
    "@name",
    "@",
    "BOB",
    "BOB ^",
    "MAYA (V.O.)",
    "THEO\\^",
    "R2D2",
    "42",
    "(beat)",
    "(multi",
    "line)",
    "(",
    ")",
    "Hello there.",
    "she said",
    "~la la",
    "~",
    ">CENTER<",
    "> spaced <",
    ">TRANS",
    ">",
    "<",
    "CUT TO:",
    "CUT TO: ",
    "TO:",
    "===",
    "=====",
    "==",
    "# Act",
    "## Seq",
    "#",
    "= synopsis",
    "=",
    "[[note]]",
    "[[open",
    "close]]",
    "]]",
    "[[",
    "/* bone",
    "bone */",
    "/*",
    "*/",
    "*italic*",
    "**bold**",
    "***both***",
    "_under_",
    "_",
    "*",
    "**",
    "\\*",
    "\\\\",
    "\\",
    "a*b",
    "snake_case",
    "  ",
    "    indented",
    "\tTabbed",
    "Title: X",
    "Author:",
    "Draft date: 1/2/3",
    "    cont",
    "^",
    "#1#",
    "#1A#",
    " #2#",
    "...",
    "x: y",
    ":",
    "Ünïcödé",
    "日本語",
    "TÉLÉ",
    "—",
    "\u{a0}",
    // A byte-order mark not at the start of the input is text (spec §3).
    "\u{feff}",
    " ",
    "  trailing  ",
];

fn fountain_line() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => prop::collection::vec(prop::sample::select(FRAGMENTS), 1..4)
            .prop_map(|f| f.concat()),
        1 => prop::collection::vec(prop::sample::select(FRAGMENTS), 1..4)
            .prop_map(|f| f.join(" ")),
        2 => Just(String::new()),
    ]
}

fn has_packed_delimiters(s: &str) -> bool {
    let c: Vec<char> = s.chars().collect();
    c.windows(4)
        .any(|w| w.iter().all(|&x| x == '*' || x == '_'))
}

fn fountain_text() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(fountain_line(), 0..40),
        prop::sample::select(&["\n", "\r\n", "\r"][..]),
        any::<bool>(),
    )
        .prop_map(|(lines, nl, trailing)| {
            let mut s = lines.join(nl);
            if trailing {
                s.push_str(nl);
            }
            s
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// Any text at all parses without panicking.
    #[test]
    fn arbitrary_text_never_panics(s in any::<String>()) {
        let p = parse(&s);
        let _ = write(&p.script);
    }

    /// The parser's output is already in normal form, and writing it loses
    /// nothing and reads back identically.
    #[test]
    fn parse_write_parse_is_stable(s in fountain_text()) {
        let first = parse(&s).script;
        prop_assert_eq!(&normalize(&first), &first, "parser output is not in normal form");
        let w = write(&first);
        // Spec §9: emphasis built from four or more packed delimiters may
        // have no writable form; that, and only that, may be reported.
        if !w.losses.is_empty()
            && w.losses.iter().all(|l| l.code == "style")
            && has_packed_delimiters(&s)
        {
            return Ok(());
        }
        prop_assert!(w.losses.is_empty(), "losses {:?}\n--- input\n{}\n--- written\n{}", w.losses, s, w.text);
        let second = parse(&w.text).script;
        prop_assert_eq!(&second, &first, "\n--- input\n{}\n--- written\n{}", s, w.text);
        prop_assert_eq!(write(&second).text, w.text);
    }
}

// ---- Generated scripts ----

const WORDS: &[&str] = &[
    "Maya",
    "THEO",
    "door",
    "rain",
    "the",
    "a",
    "*",
    "_",
    "\\",
    "[",
    "]",
    "/",
    "#",
    "(",
    ")",
    "~",
    ">",
    "<",
    "!",
    "@",
    ".",
    "...",
    ":",
    "TO:",
    "^",
    "=",
    "é",
    "日本",
    "x*y",
    "--",
    "'",
    "\"",
    "E/I CAR",
    "EXT/INT. BUS",
    "EXT/INTERIOR",
];

fn style() -> impl Strategy<Value = Style> {
    prop_oneof![
        6 => Just(Style::PLAIN),
        1 => Just(Style { bold: true, ..Style::PLAIN }),
        1 => Just(Style { italic: true, ..Style::PLAIN }),
        1 => Just(Style { underline: true, ..Style::PLAIN }),
        1 => Just(Style { bold: true, italic: true, underline: true }),
    ]
}

/// One line of text: words with whole-word styles and the odd inline note.
fn line_runs() -> impl Strategy<Value = Vec<Run>> {
    prop::collection::vec(
        (
            prop::sample::select(WORDS),
            style(),
            prop::bool::weighted(0.05),
        ),
        1..6,
    )
    .prop_map(|words| {
        let mut runs = Vec::new();
        for (k, (w, st, note)) in words.into_iter().enumerate() {
            if k > 0 {
                runs.push(Run::plain(" "));
            }
            runs.push(Run::styled(w, st));
            if note {
                runs.push(Run::Note("check this".into()));
            }
        }
        runs
    })
}

fn rich(lines: Vec<Vec<Run>>) -> RichText {
    let mut runs = Vec::new();
    for (k, l) in lines.into_iter().enumerate() {
        if k > 0 {
            runs.push(Run::plain("\n"));
        }
        runs.extend(l);
    }
    RichText::new(runs)
}

fn plain_words(n: std::ops::Range<usize>) -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(&["Maya", "door", "rain", "night", "the", "x"][..]),
        n,
    )
    .prop_map(|w| w.join(" "))
}

fn spacing() -> impl Strategy<Value = Option<u8>> {
    prop_oneof![8 => Just(None), 1 => Just(Some(2)), 1 => Just(Some(3))]
}

/// A paragraph-level block of one or more elements, in grammatical order.
fn block() -> impl Strategy<Value = Vec<Element>> {
    prop_oneof![
        // Scene heading: by prefix, by a reversed prefix (always written
        // forced), or forced (starts with a letter).
        (
            prop_oneof![
                plain_words(1..3).prop_map(|w| format!("INT. {}", w.to_uppercase())),
                (
                    prop::sample::select(&["E/I", "EXT/INT."][..]),
                    plain_words(1..3)
                )
                    .prop_map(|(p, w)| format!("{p} {}", w.to_uppercase())),
                plain_words(1..3),
            ],
            prop::option::of(prop::sample::select(&["1", "12A", "I-1-A", "3."][..])),
            spacing()
        )
            .prop_map(|(t, n, sp)| {
                let mut e = Element::new(ElementKind::SceneHeading, t);
                e.scene_number = n.map(str::to_string);
                e.space_before = sp;
                vec![e]
            }),
        // Action, possibly indented or centered.
        (
            prop::collection::vec(line_runs(), 1..4),
            any::<bool>(),
            prop::bool::weighted(0.2),
            spacing()
        )
            .prop_map(|(lines, indent, centered, sp)| {
                let lines = if indent && !centered {
                    lines
                        .into_iter()
                        .map(|mut l| {
                            l.insert(0, Run::plain("    "));
                            l
                        })
                        .collect()
                } else {
                    lines
                };
                let mut e = Element::new(ElementKind::Action, rich(lines));
                e.centered = centered;
                e.space_before = sp;
                vec![e]
            }),
        // A dialogue group.
        (
            line_runs(),
            any::<bool>(),
            prop::collection::vec((0..3u8, prop::collection::vec(line_runs(), 1..3)), 1..4),
            spacing()
        )
            .prop_map(|(name, dual, members, sp)| {
                let mut c = Element::new(ElementKind::Character, rich(vec![name]));
                c.dual = dual;
                c.space_before = sp;
                let mut out = vec![c];
                let mut last = 9;
                for (kind, lines) in members {
                    let kind = if kind == last { (kind + 1) % 3 } else { kind };
                    last = kind;
                    out.push(match kind {
                        0 => Element::new(ElementKind::Dialogue, rich(lines)),
                        1 => {
                            let mut l = lines.into_iter().next().unwrap_or_default();
                            l.insert(0, Run::plain("("));
                            l.push(Run::plain(")"));
                            Element::new(ElementKind::Parenthetical, rich(vec![l]))
                        }
                        _ => Element::new(ElementKind::Lyric, rich(lines)),
                    });
                }
                out
            }),
        (
            prop_oneof![Just("CUT TO:".to_string()), plain_words(1..3)],
            spacing()
        )
            .prop_map(|(t, sp)| {
                let mut e = Element::new(ElementKind::Transition, t);
                e.space_before = sp;
                vec![e]
            }),
        (1..4u8, line_runs())
            .prop_map(|(d, l)| vec![Element::new(ElementKind::Section, rich(vec![l])).depth(d)]),
        line_runs().prop_map(|l| vec![Element::new(ElementKind::Synopsis, rich(vec![l]))]),
        plain_words(0..4).prop_map(|t| vec![Element::new(ElementKind::Note, t)]),
        plain_words(0..4).prop_map(|t| vec![Element::new(
            ElementKind::Boneyard,
            format!(" {t}\n\nmore ")
        )]),
        Just(vec![Element::new(ElementKind::PageBreak, "")]),
        // A standalone lyric. Spacing is explicit: with the default, a lyric
        // straight after dialogue would join that dialogue group.
        (prop::collection::vec(line_runs(), 1..3), spacing()).prop_map(|(lines, sp)| {
            let mut e = Element::new(ElementKind::Lyric, rich(lines));
            e.space_before = Some(sp.unwrap_or(1));
            vec![e]
        }),
    ]
}

fn title_page() -> impl Strategy<Value = TitlePage> {
    prop::collection::vec(
        (
            prop::sample::select(
                &[
                    "Title",
                    "Author",
                    "Draft date",
                    "Contact",
                    "Notes",
                    "Revision",
                ][..],
            ),
            prop::collection::vec(line_runs(), 0..3),
        ),
        0..4,
    )
    .prop_map(|fields| {
        let mut fields: Vec<TitleField> = fields
            .into_iter()
            .map(|(k, vs)| TitleField::new(k, vs.into_iter().map(|l| rich(vec![l])).collect()))
            .collect();
        if !fields.is_empty() && fields.iter().all(|f| f.values.is_empty()) {
            fields[0].values.push(RichText::plain("Untitled"));
        }
        TitlePage { fields }
    })
}

fn script() -> impl Strategy<Value = Script> {
    (title_page(), prop::collection::vec(block(), 0..12)).prop_map(|(title_page, blocks)| Script {
        title_page,
        elements: blocks.into_iter().flatten().collect(),
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// A well-formed script writes without loss and reads back as its
    /// normal form.
    #[test]
    fn generated_scripts_round_trip(s in script()) {
        let w = write(&s);
        prop_assert!(w.losses.is_empty(), "losses {:?}\n--- written\n{}", w.losses, w.text);
        let back = parse(&w.text).script;
        prop_assert_eq!(&back, &normalize(&s), "\n--- written\n{}", w.text);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// The fuzz target's check, on stable, for every CI build.
    #[test]
    fn fuzz_round_trip_check(s in fountain_text()) {
        prop_assert_eq!(coldopen_fountain::fuzzing::round_trip(&s), Ok(()));
    }

    #[test]
    fn fuzz_round_trip_bytes(data in prop::collection::vec(any::<u8>(), 0..400)) {
        prop_assert_eq!(coldopen_fountain::fuzzing::round_trip_bytes(&data), Ok(()));
    }
}

/// Found by `fuzz_round_trip_check` in CI (2026-10-16): an empty lyric line
/// followed by a note spanning lines left a line holding only the note,
/// which the normal form joins to the line before it, so the parse was not
/// in normal form.
#[test]
fn a_lyric_ending_in_a_note_parses_to_its_normal_form() {
    assert_eq!(
        coldopen_fountain::fuzzing::round_trip("~la la\n~\n[[open\n[[note]]~"),
        Ok(())
    );
}

/// Found by the `fountain_roundtrip` fuzz target in CI (2026-10-16, again
/// 2026-10-17): a blank line, then U+FEFF. The blank line is not written,
/// so the character came first in the output, where a reader removes it as
/// a byte-order mark. The writer now forces that paragraph (spec §9).
#[test]
fn an_opening_paragraph_starting_with_a_byte_order_mark_round_trips() {
    for input in [
        "\n\u{feff}",
        "\n\u{feff}BOB\nHello.\n",
        "\n\u{feff}CUT TO:\n",
    ] {
        assert_eq!(
            coldopen_fountain::fuzzing::round_trip(input),
            Ok(()),
            "{input:?}"
        );
    }
    let script = coldopen_fountain::parse("\n\u{feff}").script;
    assert_eq!(coldopen_fountain::write(&script).text, "!\u{feff}\n");
}

/// Found by the round-trip property test at 20,000 cases (2026-10-17): a
/// dual-dialogue character named `EST`, then a hold line. Written as
/// `EST ^`, the name read back as a scene heading; it is now forced.
#[test]
fn a_dual_name_that_reads_as_a_heading_is_forced() {
    for input in ["EST^\n  ", "INT^\n  "] {
        assert_eq!(
            coldopen_fountain::fuzzing::round_trip(input),
            Ok(()),
            "{input:?}"
        );
    }
    let script = coldopen_fountain::parse("EST^\n  ").script;
    assert_eq!(coldopen_fountain::write(&script).text, "@EST ^\n  \n");
}

/// Found by the `fountain_roundtrip` fuzz target in CI (run 39030169983,
/// 2026-10-17): "parser output is not in normal form". In the title value
/// `*/**/* *` the stars around an empty boneyard closed as an empty italic
/// and left " *"; title values were trimmed only before emphasis was
/// parsed. They are now brought to normal form afterwards, as element text
/// is (spec §4).
#[test]
fn a_title_value_trimmed_after_emphasis_is_in_normal_form() {
    for input in [&b"z:*/* **/*\x0b*/=* **/* "[..], b"z:*/**/* *"] {
        assert_eq!(
            coldopen_fountain::fuzzing::round_trip_bytes(input),
            Ok(()),
            "{:?}",
            String::from_utf8_lossy(input)
        );
    }
}
