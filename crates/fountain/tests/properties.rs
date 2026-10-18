//! Property tests for the round-trip guarantees in docs/specs/fountain.md §1.

use coldopen_fountain::{Loss, normalize, parse, write};
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
    /// nothing it does not report and reads back identically.
    #[test]
    fn parse_write_parse_is_stable(s in fountain_text()) {
        let first = parse(&s).script;
        prop_assert_eq!(&normalize(&first), &first, "parser output is not in normal form");
        let w = write(&first);
        let second = parse(&w.text).script;
        // Spec §9: a reported style loss may change styles where it was
        // reported, and nothing else.
        prop_assert_eq!(
            coldopen_fountain::fuzzing::reads_back_as_reported(&first, &second, &w.losses),
            Ok(()),
            "\n--- input\n{}\n--- written\n{}", s, w.text
        );
        // As in the fuzz check: after a reported loss the next write may
        // spell the read-back text differently.
        if w.losses.is_empty() {
            prop_assert_eq!(write(&second).text, w.text);
        }
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

/// Found by the `fountain_roundtrip` fuzz target (run 39083887560,
/// 2026-10-17): "unexpected losses: style". Underline, then italic and
/// underline, then italic, next to characters the flanking rules count as
/// punctuation (U+FFFD from invalid UTF-8). The input itself spells it, so
/// the writer must too, with no loss: the ordered arrangement (spec §9).
#[test]
fn mixed_markers_next_to_punctuation_write_back() {
    for input in [
        &b"\xff\xff\xff\xff\xdd~_*_\xd6_*\xff\xdd\xab_*_\xd6_\x89*\xd6*"[..],
        "_\u{76b}_*_\u{fffd}_\u{fffd}*".as_bytes(),
    ] {
        let text = String::from_utf8_lossy(input);
        let w = write(&parse(&text).script);
        assert!(w.losses.is_empty(), "{text:?}: {:?}", w.losses);
        assert_eq!(
            coldopen_fountain::fuzzing::round_trip(&text),
            Ok(()),
            "{text:?}"
        );
    }
}

// ---- Styled prose ----

/// Ordinary words, some with accented letters or an apostrophe.
const PROSE_WORDS: &[&str] = &[
    "Maya", "Theo", "Zoë", "José", "the", "a", "door", "rain", "night", "slowly", "opens", "looks",
    "back", "café", "naïve", "résumé", "señor", "déjà", "vu", "über", "façade", "don't", "it's",
    "o'clock", "42", "well", "--",
];

/// One word with the punctuation around it: (before, word, after).
fn prose_word() -> impl Strategy<Value = (&'static str, &'static str, &'static str)> {
    (
        prop_oneof![8 => Just(""), 1 => Just("\""), 1 => Just("("), 1 => Just("'")],
        prop::sample::select(PROSE_WORDS),
        prop_oneof![
            12 => Just(""),
            2 => Just("."),
            2 => Just(","),
            1 => Just("!"),
            1 => Just("?"),
            1 => Just("..."),
            1 => Just(":"),
            1 => Just(";"),
            1 => Just("\""),
            1 => Just(")"),
            1 => Just(".\""),
            1 => Just("'s"),
        ],
    )
}

/// One line of prose: words separated by single spaces, with bold, italic
/// or underline applied to whole words or phrases, the way an editor
/// applies them to a selection. Spans may nest, touch or overlap. A span
/// includes the punctuation inside it, and the punctuation at its ends only
/// when `lead` or `trail` says so.
fn prose_line() -> impl Strategy<Value = Vec<Run>> {
    (
        prop::collection::vec(prose_word(), 1..12),
        prop::collection::vec(
            (
                0..3u8,
                any::<prop::sample::Index>(),
                any::<prop::sample::Index>(),
                any::<bool>(),
                any::<bool>(),
            ),
            0..4,
        ),
    )
        .prop_map(|(words, spans)| {
            let n = words.len();
            let spans: Vec<(u8, usize, usize, bool, bool)> = spans
                .into_iter()
                .map(|(flag, s, len, lead, trail)| {
                    let s = s.index(n);
                    let e = s + len.index(n - s);
                    (flag, s, e, lead, trail)
                })
                .collect();
            let set = |st: &mut Style, flag: u8| match flag {
                0 => st.bold = true,
                1 => st.italic = true,
                _ => st.underline = true,
            };
            let mut runs = Vec::new();
            for (i, (before, word, after)) in words.into_iter().enumerate() {
                let (mut b, mut w, mut a, mut gap) =
                    (Style::PLAIN, Style::PLAIN, Style::PLAIN, Style::PLAIN);
                for &(flag, s, e, lead, trail) in &spans {
                    if (s..=e).contains(&i) {
                        set(&mut w, flag);
                        if i > s || lead {
                            set(&mut b, flag);
                        }
                        if i < e || trail {
                            set(&mut a, flag);
                        }
                    }
                    if s <= i && i < e {
                        set(&mut gap, flag);
                    }
                }
                runs.push(Run::styled(before, b));
                runs.push(Run::styled(word, w));
                runs.push(Run::styled(after, a));
                if i + 1 < n {
                    runs.push(Run::styled(" ", gap));
                }
            }
            runs
        })
}

/// A script of styled prose: action, centered action, dialogue, lyrics
/// and a title page.
fn prose_script() -> impl Strategy<Value = Script> {
    let lines = || prop::collection::vec(prose_line(), 1..3);
    let block = prop_oneof![
        lines().prop_map(|l| vec![Element::new(ElementKind::Action, rich(l))]),
        prose_line().prop_map(|l| {
            let mut e = Element::new(ElementKind::Action, rich(vec![l]));
            e.centered = true;
            vec![e]
        }),
        lines().prop_map(|l| vec![
            Element::new(ElementKind::Character, "MAYA"),
            Element::new(ElementKind::Dialogue, rich(l)),
        ]),
        lines().prop_map(|l| {
            let mut e = Element::new(ElementKind::Lyric, rich(l));
            e.space_before = Some(1);
            vec![e]
        }),
    ];
    (
        prop::option::of(prose_line()),
        prop::collection::vec(block, 1..5),
    )
        .prop_map(|(title, blocks)| Script {
            title_page: TitlePage {
                fields: title
                    .map(|l| vec![TitleField::new("Title", vec![rich(vec![l])])])
                    .unwrap_or_default(),
            },
            elements: blocks.into_iter().flatten().collect(),
        })
}

/// One printed character and its style.
type Cell = (char, Style);

/// The printed characters of rich text with their styles, line by line.
fn styled_lines(t: &RichText) -> Vec<Vec<Cell>> {
    let mut lines = vec![Vec::new()];
    for r in t.runs() {
        if let Run::Text { text, style } = r {
            for c in text.chars() {
                if c == '\n' {
                    lines.push(Vec::new());
                } else if let Some(l) = lines.last_mut() {
                    l.push((c, *style));
                }
            }
        }
    }
    lines
}

fn on(s: &Style, f: usize) -> bool {
    [s.bold, s.italic, s.underline][f]
}

/// The spans of style `f` in `cells`, as index ranges.
fn spans(cells: &[Cell], f: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < cells.len() {
        if on(&cells[k].1, f) {
            let start = k;
            while k < cells.len() && on(&cells[k].1, f) {
                k += 1;
            }
            out.push((start, k));
        } else {
            k += 1;
        }
    }
    out
}

/// Whether two different styles cross: each starts inside the other's
/// span and ends outside it.
fn styles_cross(cells: &[Cell]) -> bool {
    let all = [spans(cells, 0), spans(cells, 1), spans(cells, 2)];
    (0..3).any(|f| {
        (0..3).filter(|&g| g != f).any(|g| {
            all[f]
                .iter()
                .any(|&(a0, a1)| all[g].iter().any(|&(b0, b1)| a0 < b0 && b0 < a1 && a1 < b1))
        })
    })
}

/// Whether a style starts or ends between two punctuation marks, as a
/// bold `--` inside an italic `"--"` does.
fn style_edge_between_punctuation(cells: &[Cell]) -> bool {
    let punct = |c: char| !c.is_alphanumeric() && !c.is_whitespace();
    cells.windows(2).any(|w| {
        let ((a, sa), (b, sb)) = (w[0], w[1]);
        punct(a) && punct(b) && (0..3).any(|f| on(&sa, f) != on(&sb, f))
    })
}

/// Which characters belong to a word in which two styles cross once
/// whitespace carries none: the in-word case (spec §9).
fn in_crossing_word(cells: &[Cell]) -> Vec<bool> {
    let mut out = vec![false; cells.len()];
    let mut k = 0;
    while k < cells.len() {
        if cells[k].0.is_whitespace() {
            k += 1;
            continue;
        }
        let start = k;
        while k < cells.len() && !cells[k].0.is_whitespace() {
            k += 1;
        }
        if styles_cross(&cells[start..k]) {
            out[start..k].fill(true);
        }
    }
    out
}

/// What writing did to a line that Fountain cannot spell as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Outcome {
    /// It reads back exactly.
    Exact,
    /// Bold or italic came off whitespace, where they do not print.
    SpacesBoldItalic,
    /// Underline came off whitespace (reported).
    SpacesUnderline,
    /// A crossing style came off a word (reported).
    InWord,
}

/// Checks how a line that Fountain cannot spell as it is reads back: only
/// bold or italic on whitespace may change unreported, and with a reported
/// style loss also underline on whitespace and the styles of a word in
/// which two styles cross. No style may be added.
fn crossing_line(want: &[Cell], back: &[Cell], reported: bool) -> Result<Outcome, String> {
    let words = in_crossing_word(want);
    let mut outcome = Outcome::Exact;
    for (k, (&(c, w), &(_, b))) in want.iter().zip(back).enumerate() {
        if (0..3).any(|f| on(&b, f) && !on(&w, f)) {
            return Err(format!("a style was added at {k}"));
        }
        if w == b {
            continue;
        }
        let step = if c.is_whitespace() && w.underline == b.underline {
            Outcome::SpacesBoldItalic
        } else if c.is_whitespace() {
            Outcome::SpacesUnderline
        } else if words[k] {
            Outcome::InWord
        } else {
            return Err(format!(
                "a style came off {c:?} at {k}, outside a crossing word"
            ));
        };
        if step > Outcome::SpacesBoldItalic && !reported {
            return Err(format!("{step:?} at {k} was not reported"));
        }
        outcome = outcome.max(step);
    }
    Ok(outcome)
}

/// `t` with every style removed.
fn unstyled(t: &RichText) -> RichText {
    RichText::new(
        t.runs()
            .iter()
            .map(|r| match r {
                Run::Text { text, .. } => Run::plain(text.clone()),
                hidden => hidden.clone(),
            })
            .collect(),
    )
}

/// Checks a written prose script against its normal form `want`. Lines
/// Fountain can spell (no styles that cross, and no style edge between two
/// punctuation marks) must read back exactly, and an element made only of
/// them must report no loss. Other lines are checked by [`crossing_line`].
/// Returns the outcome for each of those other lines.
fn check_prose(want: &Script, back: &Script, losses: &[Loss]) -> Result<Vec<Outcome>, String> {
    if let Some(l) = losses.iter().find(|l| l.code != "style") {
        return Err(format!("unexpected loss {l:?}"));
    }
    let mut texts: Vec<(Option<usize>, &RichText, &RichText)> = Vec::new();
    let keys = |s: &Script| -> Vec<(String, usize)> {
        s.title_page
            .fields
            .iter()
            .map(|f| (f.key.clone(), f.values.len()))
            .collect()
    };
    if keys(want) != keys(back) {
        return Err("the title page reads back differently".into());
    }
    for (a, b) in want.title_page.fields.iter().zip(&back.title_page.fields) {
        texts.extend(a.values.iter().zip(&b.values).map(|(x, y)| (None, x, y)));
    }
    if want.elements.len() != back.elements.len() {
        return Err("the elements read back differently".into());
    }
    for (i, (a, b)) in want.elements.iter().zip(&back.elements).enumerate() {
        let (mut x, mut y) = (a.clone(), b.clone());
        x.text = RichText::empty();
        y.text = RichText::empty();
        if x != y {
            return Err(format!("element {i} reads back differently"));
        }
        texts.push((Some(i), &a.text, &b.text));
    }
    let mut outcomes = Vec::new();
    for (at, a, b) in texts {
        if unstyled(a) != unstyled(b) {
            return Err(format!("the text of {at:?} reads back differently"));
        }
        let reported = losses.iter().any(|l| l.element == at);
        let mut all_spellable = true;
        for (x, y) in styled_lines(a).iter().zip(&styled_lines(b)) {
            if styles_cross(x) || style_edge_between_punctuation(x) {
                all_spellable = false;
                outcomes.push(crossing_line(x, y, reported).map_err(|e| format!("{at:?}: {e}"))?);
            } else if x != y {
                return Err(format!(
                    "a line of {at:?} Fountain can spell reads back differently"
                ));
            }
        }
        if all_spellable && reported {
            return Err(format!(
                "a loss reported for {at:?}, which Fountain can spell"
            ));
        }
    }
    Ok(outcomes)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// Ordinary prose, with bold, italic and underline on whole words and
    /// phrases. What Fountain can spell writes with no loss and reads back
    /// exactly; crossing styles lose only what spec §9 lists.
    #[test]
    fn styled_prose_writes_back(s in prose_script()) {
        let w = write(&s);
        let back = parse(&w.text).script;
        let checked = check_prose(&normalize(&s), &back, &w.losses);
        prop_assert!(checked.is_ok(), "{:?}\nlosses {:?}\n--- written\n{}", checked, w.losses, w.text);
    }
}

/// Pieces of lines built from packed and mixed emphasis and boneyard
/// markers, the class three fuzz findings came from (2026-10-16 and -17):
/// markers next to letters, non-ASCII letters, punctuation, spaces and
/// invalid UTF-8 (read as U+FFFD, which counts as punctuation).
const MARKER_PIECES: &[&[u8]] = &[
    b"*",
    b"**",
    b"***",
    b"_",
    b"__",
    b"*_",
    b"_*",
    b"_*_",
    b"*_*",
    b"/*",
    b"*/",
    b"/**/",
    b"/* x */",
    b"\\",
    b"a",
    b"bc",
    "\u{76b}".as_bytes(),
    "\u{e9}".as_bytes(),
    b".",
    b"-",
    b"'",
    b" ",
    b"  ",
    b"\xff",
    b"\xd6",
];

/// Where a generated line goes: an action line, a title page value, a line
/// of dialogue and a lyric.
const MARKER_CONTEXTS: &[&[u8]] = &[b"", b"z: ", b"BOB\n", b"~"];

fn marker_line() -> impl Strategy<Value = Vec<u8>> {
    (
        prop::sample::select(MARKER_CONTEXTS),
        prop::collection::vec(prop::sample::select(MARKER_PIECES), 1..16),
    )
        .prop_map(|(context, pieces)| {
            let mut data = context.to_vec();
            for p in pieces {
                data.extend_from_slice(p);
            }
            data
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// Packed and mixed markers may lose styles that cannot be spelled,
    /// but never silently: the fuzz target's check. How often styles are
    /// lost is measured by `styling_rates`, not gated.
    #[test]
    fn packed_and_mixed_markers_lose_nothing_silently(data in marker_line()) {
        prop_assert_eq!(
            coldopen_fountain::fuzzing::round_trip_bytes(&data),
            Ok(()),
            "{:?}",
            String::from_utf8_lossy(&data)
        );
    }
}

/// How often the writer cannot spell styles, per 100,000 generated cases
/// (recorded in docs/progress/phase-4.md, "Deferred"). Run by hand:
/// `cargo test --release -p coldopen-fountain --test properties
/// styling_rates -- --ignored --nocapture`. Set `PROPTEST_CASES` for
/// another count.
#[test]
#[ignore]
fn styling_rates() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    let n: usize = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let per = |k: usize| k as f64 * 100_000.0 / n as f64;

    let mut runner = TestRunner::deterministic();
    let strategy = prose_script();
    let mut counts = std::collections::BTreeMap::<Outcome, usize>::new();
    for _ in 0..n {
        let s = strategy.new_tree(&mut runner).unwrap().current();
        let w = write(&s);
        let back = parse(&w.text).script;
        let outcomes = check_prose(&normalize(&s), &back, &w.losses)
            .unwrap_or_else(|e| panic!("{e}\n--- written\n{}", w.text));
        for o in outcomes {
            *counts.entry(o).or_default() += 1;
        }
    }
    println!(
        "styled prose, {n} scripts; lines Fountain cannot spell as they are, per 100,000 scripts:"
    );
    for (o, k) in &counts {
        println!("  {o:?}: {k} ({:.1})", per(*k));
    }

    let mut runner = TestRunner::deterministic();
    let strategy = marker_line();
    let (mut unreported, mut reported) = (0, 0);
    for _ in 0..n {
        let data = strategy.new_tree(&mut runner).unwrap().current();
        let text = String::from_utf8_lossy(&data);
        assert_eq!(
            coldopen_fountain::fuzzing::round_trip(&text),
            Ok(()),
            "{text:?}"
        );
        let first = parse(&text).script;
        let w = write(&first);
        if !w.losses.is_empty() {
            reported += 1;
        } else if parse(&w.text).script != first {
            unreported += 1;
        }
    }
    println!("packed and mixed markers, {n} cases, per 100,000:");
    println!(
        "  bold or italic off whitespace only: {unreported} ({:.1})",
        per(unreported)
    );
    println!("  reported style loss: {reported} ({:.1})", per(reported));
}
