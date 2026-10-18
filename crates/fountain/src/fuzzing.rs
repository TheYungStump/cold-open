//! Checks shared by the cargo-fuzz targets in `fuzz/` and the stable test
//! suite, so the fuzzed properties also run on every CI build.

use coldopen_screenplay::{RichText, Run, Script, Style};

use crate::norm::without_ws_emphasis;
use crate::{Loss, normalize, parse, write};

/// Parses `text` and checks the round-trip guarantees of spec §1:
/// the parse is in normal form, writing it loses nothing it does not
/// report, the written text reads back the same apart from what was
/// reported, and, when nothing was, writing again changes nothing.
pub fn round_trip(text: &str) -> Result<(), String> {
    let first = parse(text).script;
    if normalize(&first) != first {
        return Err("parser output is not in normal form".into());
    }
    let w = write(&first);
    let second = parse(&w.text).script;
    reads_back_as_reported(&first, &second, &w.losses)
        .map_err(|why| format!("{why}:\n{}", w.text))?;
    // After a reported loss, writing the read-back text may spell it
    // differently (the lossy write escapes every `*` and `_`), so only a
    // lossless write must come out the same again.
    if w.losses.is_empty() && write(&second).text != w.text {
        return Err("writing is not idempotent".into());
    }
    Ok(())
}

/// Like [`round_trip`], for arbitrary bytes.
pub fn round_trip_bytes(data: &[u8]) -> Result<(), String> {
    round_trip(&String::from_utf8_lossy(data))
}

/// Checks that `second`, read back from what the writer made of `first`,
/// differs from it only as `losses` report (spec §9). Bold and italic on
/// whitespace, which do not print, are set aside everywhere. A style loss
/// may change bold, italic and underline in the element it names, or on
/// the title page; the text itself, and everything else, must read back
/// identical. Any other loss fails.
pub fn reads_back_as_reported(
    first: &Script,
    second: &Script,
    losses: &[Loss],
) -> Result<(), String> {
    if losses.iter().any(|l| l.code != "style") {
        return Err(format!("unexpected losses: {losses:?}"));
    }
    let (first, second) = (&without_ws_emphasis(first), &without_ws_emphasis(second));
    let restyled = |at: Option<usize>| losses.iter().any(|l| l.element == at);
    let title = |s: &Script| {
        let mut t = s.title_page.clone();
        if restyled(None) {
            for v in t.fields.iter_mut().flat_map(|f| f.values.iter_mut()) {
                *v = unstyled(v);
            }
        }
        t
    };
    let same = title(first) == title(second)
        && first.elements.len() == second.elements.len()
        && first
            .elements
            .iter()
            .zip(&second.elements)
            .enumerate()
            .all(|(i, (a, b))| {
                if restyled(Some(i)) {
                    let (mut a, mut b) = (a.clone(), b.clone());
                    a.text = unstyled(&a.text);
                    b.text = unstyled(&b.text);
                    a == b
                } else {
                    a == b
                }
            });
    if same {
        Ok(())
    } else if losses.is_empty() {
        Err("written text reads back differently".into())
    } else {
        Err(format!(
            "written text reads back differently from what the losses report: {losses:?}"
        ))
    }
}

/// `text` with every style removed.
fn unstyled(text: &RichText) -> RichText {
    RichText::new(
        text.runs()
            .iter()
            .map(|r| match r {
                Run::Text { text, .. } => Run::styled(text.clone(), Style::PLAIN),
                hidden => hidden.clone(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use coldopen_screenplay::{Element, ElementKind};

    use super::*;

    fn action(runs: Vec<Run>) -> Element {
        Element::new(ElementKind::Action, RichText::new(runs))
    }

    fn script(elements: Vec<Element>) -> Script {
        Script {
            elements,
            ..Script::default()
        }
    }

    fn style_loss(element: Option<usize>) -> Loss {
        Loss {
            element,
            code: "style",
            message: String::new(),
        }
    }

    const BOLD: Style = Style {
        bold: true,
        ..Style::PLAIN
    };

    /// Underline on `.é` between other punctuation reads back only from a
    /// spelling with packed delimiter runs (`___.é__*'*_*a\_*`), found by a
    /// brute-force search on 2026-10-17, and the writer never writes those.
    /// It reports a style loss, the text reads back otherwise identical, and
    /// the check passes.
    #[test]
    fn a_reported_style_loss_passes() {
        for input in [
            "z: \u{fffd}\u{fffd}_._\u{e9}_*'*_*a_*",
            "\u{fffd}\u{fffd}_._\u{e9}_*'*_*a_*",
        ] {
            let first = parse(input).script;
            let w = write(&first);
            assert!(!w.losses.is_empty(), "{input:?}");
            assert!(w.losses.iter().all(|l| l.code == "style"), "{input:?}");
            assert_ne!(parse(&w.text).script, first, "{input:?}");
            assert_eq!(round_trip(input), Ok(()), "{input:?}");
        }
    }

    #[test]
    fn an_unreported_difference_fails() {
        let first = script(vec![
            action(vec![Run::styled("loud", BOLD)]),
            action(vec![Run::plain("quiet")]),
        ]);
        let second = script(vec![
            action(vec![Run::plain("loud")]),
            action(vec![Run::plain("quiet")]),
        ]);
        let err = reads_back_as_reported(&first, &second, &[]).unwrap_err();
        assert_eq!(err, "written text reads back differently");
        // A style loss reported for another element, or for the title
        // page, does not cover it.
        for elsewhere in [Some(1), None] {
            let err =
                reads_back_as_reported(&first, &second, &[style_loss(elsewhere)]).unwrap_err();
            assert!(err.contains("from what the losses report"), "{err}");
        }
        assert_eq!(
            reads_back_as_reported(&first, &second, &[style_loss(Some(0))]),
            Ok(())
        );
    }

    /// Bold and italic do not print on a space, so they may come off one
    /// without a report; underline prints, so it may not.
    #[test]
    fn bold_and_italic_on_spaces_are_set_aside() {
        let first = script(vec![action(vec![Run::styled("a b", BOLD)])]);
        let second = script(vec![action(vec![
            Run::styled("a", BOLD),
            Run::plain(" "),
            Run::styled("b", BOLD),
        ])]);
        assert_eq!(reads_back_as_reported(&first, &second, &[]), Ok(()));
        let under = Style {
            underline: true,
            ..Style::PLAIN
        };
        let first = script(vec![action(vec![Run::styled("a b", under)])]);
        let second = script(vec![action(vec![
            Run::styled("a", under),
            Run::plain(" "),
            Run::styled("b", under),
        ])]);
        assert!(reads_back_as_reported(&first, &second, &[]).is_err());
    }

    #[test]
    fn a_loss_in_the_text_itself_fails() {
        let first = script(vec![action(vec![Run::styled("loud", BOLD)])]);
        for second in [
            script(vec![action(vec![Run::plain("lou")])]),
            script(vec![action(vec![
                Run::plain("loud"),
                Run::Note("n".into()),
            ])]),
            script(vec![Element::new(ElementKind::Dialogue, "loud")]),
            script(vec![]),
        ] {
            let err = reads_back_as_reported(&first, &second, &[style_loss(Some(0))]).unwrap_err();
            assert!(err.contains("from what the losses report"), "{err}");
        }
    }

    #[test]
    fn any_other_loss_fails() {
        let first = script(vec![action(vec![Run::plain("a")])]);
        let other = Loss {
            element: Some(0),
            code: "line_break",
            message: String::new(),
        };
        let err = reads_back_as_reported(&first, &first, &[other]).unwrap_err();
        assert!(err.starts_with("unexpected losses"), "{err}");
    }
}
