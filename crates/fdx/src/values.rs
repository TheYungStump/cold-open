//! Parsing attribute values with integer arithmetic (docs/specs/fdx.md §4).

use coldopen_screenplay::{ElementKind, Pt, Style};

use crate::report::{Report, codes};

/// Where a value came from, for messages: a paragraph index when it
/// belongs to one, and a description such as `PageLayout TopMargin`.
pub(crate) struct At<'a> {
    pub(crate) what: &'a str,
    pub(crate) paragraph: Option<usize>,
}

impl<'a> At<'a> {
    pub(crate) fn new(what: &'a str) -> At<'a> {
        At {
            what,
            paragraph: None,
        }
    }

    pub(crate) fn para(what: &'a str, paragraph: usize) -> At<'a> {
        At {
            what,
            paragraph: Some(paragraph),
        }
    }
}

fn invalid(r: &mut Report, at: &At<'_>, raw: &str, expected: &str) {
    r.push(
        codes::INVALID_VALUE,
        format!(
            "{} = \"{raw}\" is not {expected}; the default is kept",
            at.what
        ),
        at.paragraph,
    );
}

fn rounded(r: &mut Report, at: &At<'_>, raw: &str, unit: &str, pt: Pt) {
    r.push(
        codes::VALUE_ROUNDED,
        format!("{} = \"{raw}\" {unit} was rounded to {pt} pt", at.what),
        at.paragraph,
    );
}

/// A length in inches.
pub(crate) fn inches(raw: &str, at: &At<'_>, r: &mut Report) -> Option<Pt> {
    match Pt::parse_inches(raw) {
        Some((pt, true)) => Some(pt),
        Some((pt, false)) => {
            rounded(r, at, raw, "in", pt);
            Some(pt)
        }
        None => {
            invalid(r, at, raw, "a length in inches");
            None
        }
    }
}

/// A length in points, rounded to hundredths when it has more decimals.
pub(crate) fn points(raw: &str, at: &At<'_>, r: &mut Report) -> Option<Pt> {
    match points_rounding(raw) {
        Some((pt, true)) => Some(pt),
        Some((pt, false)) => {
            rounded(r, at, raw, "pt", pt);
            Some(pt)
        }
        None => {
            invalid(r, at, raw, "a length in points");
            None
        }
    }
}

/// Points as a `Pt`, and whether no rounding was needed. More than two
/// decimals are rounded half away from zero on the digits themselves.
pub(crate) fn points_rounding(raw: &str) -> Option<(Pt, bool)> {
    if let Some(pt) = Pt::parse_points(raw) {
        return Some((pt, true));
    }
    let s = raw.trim();
    let (negative, unsigned) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (int, frac) = unsigned.split_once('.')?;
    if frac.len() <= 2 || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let int = if int.is_empty() { "0" } else { int };
    let base = Pt::parse_points(&format!("{int}.{}", &frac[..2]))?;
    let up = frac.as_bytes()[2] >= b'5';
    let magnitude = if up { base.0.checked_add(1)? } else { base.0 };
    Some((Pt(if negative { -magnitude } else { magnitude }), false))
}

/// A plain decimal such as `Spacing`, in hundredths: `1` is 100.
pub(crate) fn hundredths(raw: &str, at: &At<'_>, r: &mut Report) -> Option<i32> {
    match points_rounding(raw) {
        Some((pt, exact)) => {
            if !exact {
                r.push(
                    codes::VALUE_ROUNDED,
                    format!("{} = \"{raw}\" was rounded to two decimals", at.what),
                    at.paragraph,
                );
            }
            Some(pt.0)
        }
        None => {
            invalid(r, at, raw, "a number");
            None
        }
    }
}

/// `Yes` or `No`, exactly.
pub(crate) fn yes_no(raw: &str, at: &At<'_>, r: &mut Report) -> Option<bool> {
    match raw {
        "Yes" => Some(true),
        "No" => Some(false),
        _ => {
            invalid(r, at, raw, "Yes or No");
            None
        }
    }
}

/// An .fdx alignment, before `Full` is folded into left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FdxAlign {
    Left,
    Center,
    Right,
    Full,
}

impl FdxAlign {
    /// `Full` prints like `Left` for our purposes.
    pub(crate) fn folded(self) -> FdxAlign {
        match self {
            FdxAlign::Full => FdxAlign::Left,
            a => a,
        }
    }

    pub(crate) fn to_align(self) -> coldopen_screenplay::Align {
        use coldopen_screenplay::Align;
        match self {
            FdxAlign::Left | FdxAlign::Full => Align::Left,
            FdxAlign::Center => Align::Center,
            FdxAlign::Right => Align::Right,
        }
    }

    pub(crate) fn from_align(a: coldopen_screenplay::Align) -> FdxAlign {
        use coldopen_screenplay::Align;
        match a {
            Align::Left => FdxAlign::Left,
            Align::Center => FdxAlign::Center,
            Align::Right => FdxAlign::Right,
        }
    }
}

pub(crate) fn alignment(raw: &str, at: &At<'_>, r: &mut Report) -> Option<FdxAlign> {
    match raw {
        "Left" => Some(FdxAlign::Left),
        "Center" => Some(FdxAlign::Center),
        "Right" => Some(FdxAlign::Right),
        "Full" => Some(FdxAlign::Full),
        _ => {
            invalid(r, at, raw, "Left, Center, Right or Full");
            None
        }
    }
}

/// The paragraph types we model (docs/specs/fdx.md §6.1), matched ignoring
/// ASCII case and surrounding spaces.
pub(crate) fn kind_of_type(name: &str) -> Option<ElementKind> {
    const TYPES: [(&str, ElementKind); 9] = [
        ("Scene Heading", ElementKind::SceneHeading),
        ("Action", ElementKind::Action),
        ("Character", ElementKind::Character),
        ("Parenthetical", ElementKind::Parenthetical),
        ("Dialogue", ElementKind::Dialogue),
        ("Transition", ElementKind::Transition),
        ("Shot", ElementKind::Shot),
        ("General", ElementKind::General),
        ("Lyric", ElementKind::Lyric),
    ];
    let name = name.trim();
    TYPES
        .iter()
        .find(|(t, _)| t.eq_ignore_ascii_case(name))
        .map(|(_, k)| *k)
}

/// The .fdx type name of one of our nine kinds.
pub(crate) fn type_of_kind(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::SceneHeading => "Scene Heading",
        ElementKind::Action => "Action",
        ElementKind::Character => "Character",
        ElementKind::Parenthetical => "Parenthetical",
        ElementKind::Dialogue => "Dialogue",
        ElementKind::Transition => "Transition",
        ElementKind::Shot => "Shot",
        ElementKind::Lyric => "Lyric",
        _ => "General",
    }
}

/// A `Style` attribute: run styles, `AllCaps`, and unknown tokens.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct StyleTokens {
    pub(crate) style: Style,
    pub(crate) all_caps: bool,
    pub(crate) unknown: Vec<String>,
}

pub(crate) fn style_tokens(raw: &str) -> StyleTokens {
    let mut out = StyleTokens::default();
    for token in raw.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        if token.eq_ignore_ascii_case("Bold") {
            out.style.bold = true;
        } else if token.eq_ignore_ascii_case("Italic") {
            out.style.italic = true;
        } else if token.eq_ignore_ascii_case("Underline") {
            out.style.underline = true;
        } else if token.eq_ignore_ascii_case("AllCaps") {
            out.all_caps = true;
        } else {
            out.unknown.push(token.to_string());
        }
    }
    out
}

/// Whether a `#hex` color has every digit equal to `digit` (`0` for black,
/// `F` for white). Anything that is not such a color is false.
pub(crate) fn color_is(raw: &str, digit: u8) -> bool {
    let Some(hex) = raw.trim().strip_prefix('#') else {
        return false;
    };
    !hex.is_empty() && hex.bytes().all(|b| b.eq_ignore_ascii_case(&digit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_round_on_digits() {
        assert_eq!(points_rounding("12"), Some((Pt(1200), true)));
        assert_eq!(points_rounding("12.345"), Some((Pt(1235), false)));
        assert_eq!(points_rounding("12.344"), Some((Pt(1234), false)));
        assert_eq!(points_rounding("-0.125"), Some((Pt(-13), false)));
        assert_eq!(points_rounding(".999"), Some((Pt(100), false)));
        assert_eq!(points_rounding("1.2x5"), None);
        assert_eq!(points_rounding("abc"), None);
        assert_eq!(points_rounding(""), None);
    }

    #[test]
    fn values_report_failures_and_rounding() {
        let mut r = Report::default();
        let at = At::new("X");
        assert_eq!(inches("1.50", &at, &mut r), Some(Pt::inch100(150)));
        assert_eq!(inches("0.0001", &at, &mut r), Some(Pt(1)));
        assert_eq!(inches("one", &at, &mut r), None);
        assert_eq!(yes_no("yes", &at, &mut r), None);
        assert_eq!(yes_no("Yes", &at, &mut r), Some(true));
        assert_eq!(hundredths("1.5", &at, &mut r), Some(150));
        assert_eq!(alignment("Full", &at, &mut r), Some(FdxAlign::Full));
        assert_eq!(alignment("Middle", &at, &mut r), None);
        let codes: Vec<_> = r.finish().iter().map(|i| i.code).collect();
        assert_eq!(
            codes,
            [
                codes::VALUE_ROUNDED,
                codes::INVALID_VALUE,
                codes::INVALID_VALUE,
                codes::INVALID_VALUE
            ]
        );
    }

    #[test]
    fn styles_and_colors() {
        let s = style_tokens("Bold+Underline+AllCaps+Strike+");
        assert!(s.style.bold && s.style.underline && !s.style.italic && s.all_caps);
        assert_eq!(s.unknown, ["Strike"]);
        assert_eq!(style_tokens(""), StyleTokens::default());
        assert!(color_is("#000000000000", b'0'));
        assert!(color_is("#ffffff", b'F'));
        assert!(!color_is("#FF0000", b'0'));
        assert!(!color_is("black", b'0'));
        assert!(!color_is("#", b'0'));
    }

    #[test]
    fn type_names() {
        assert_eq!(
            kind_of_type(" scene heading "),
            Some(ElementKind::SceneHeading)
        );
        assert_eq!(kind_of_type("New Act"), None);
        for (k, _) in coldopen_screenplay::PageFormat::default().elements.iter() {
            assert_eq!(kind_of_type(type_of_kind(k)), Some(k));
        }
    }
}
