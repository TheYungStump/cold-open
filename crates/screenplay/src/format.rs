//! The page format: every number layout uses (docs/specs/layout.md §3).
//!
//! Lengths are [`Pt`], whole hundredths of a point. They are parsed from
//! decimal text with integer arithmetic, so a format means the same thing on
//! every machine.

use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::element::ElementKind;
use crate::text::RichText;

/// A length in hundredths of a point (1/7200 inch).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pt(pub i32);

impl Pt {
    pub const ZERO: Pt = Pt(0);

    /// Whole points.
    pub const fn points(points: i32) -> Pt {
        Pt(points * 100)
    }

    /// Hundredths of an inch: `Pt::inch100(150)` is 1.5 in, or 108 pt.
    pub const fn inch100(hundredths: i32) -> Pt {
        Pt(hundredths * 72)
    }

    /// Parses a number of points written in decimal, such as `81` or
    /// `184.32`. More than two decimals, or a value out of range, is `None`.
    pub fn parse_points(s: &str) -> Option<Pt> {
        let (value, exact) = Decimal::parse(s)?.scaled(100)?;
        exact.then_some(Pt(value))
    }

    /// Parses a number of inches written in decimal, such as `2.56`. The
    /// second value is false when the text had more precision than a `Pt`
    /// holds and was rounded.
    pub fn parse_inches(s: &str) -> Option<(Pt, bool)> {
        let (value, exact) = Decimal::parse(s)?.scaled(7200)?;
        Some((Pt(value), exact))
    }
}

impl fmt::Display for Pt {
    /// Points in decimal, without trailing zeros: `81`, `184.32`, `-7.2`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let v = self.0.unsigned_abs();
        let (whole, frac) = (v / 100, v % 100);
        match frac {
            0 => write!(f, "{sign}{whole}"),
            f2 if f2 % 10 == 0 => write!(f, "{sign}{whole}.{}", f2 / 10),
            f2 => write!(f, "{sign}{whole}.{f2:02}"),
        }
    }
}

/// A decimal number as written: digits and the count of digits after the
/// point.
struct Decimal {
    negative: bool,
    digits: i128,
    scale: u32,
}

impl Decimal {
    fn parse(s: &str) -> Option<Decimal> {
        let s = s.trim();
        let (negative, rest) = match s.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int, frac) = match rest.split_once('.') {
            Some((i, f)) => (i, f),
            None => (rest, ""),
        };
        if int.is_empty() && frac.is_empty() {
            return None;
        }
        if !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) || frac.len() > 9 {
            return None;
        }
        let mut digits: i128 = 0;
        for b in int.bytes().chain(frac.bytes()) {
            digits = digits.checked_mul(10)?.checked_add(i128::from(b - b'0'))?;
            if digits > i128::from(i64::MAX) {
                return None;
            }
        }
        Some(Decimal {
            negative,
            digits,
            scale: frac.len() as u32,
        })
    }

    /// The value times `factor`, rounded half away from zero, and whether no
    /// rounding was needed.
    fn scaled(&self, factor: i128) -> Option<(i32, bool)> {
        let num = self.digits.checked_mul(factor)?;
        let den = 10i128.checked_pow(self.scale)?;
        let (q, r) = (num / den, num % den);
        let q = if r * 2 >= den { q + 1 } else { q };
        let q = if self.negative { -q } else { q };
        Some((i32::try_from(q).ok()?, r == 0))
    }
}

impl Serialize for Pt {
    /// Points as a JSON number: an integer when whole, otherwise the
    /// shortest decimal, which has at most two places.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.0 % 100 == 0 {
            s.serialize_i64(i64::from(self.0 / 100))
        } else {
            s.serialize_f64(f64::from(self.0) / 100.0)
        }
    }
}

impl<'de> Deserialize<'de> for Pt {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Pt, D::Error> {
        struct PtVisitor;

        impl Visitor<'_> for PtVisitor {
            type Value = Pt;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a length in points with at most two decimals")
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Pt, E> {
                v.checked_mul(100)
                    .and_then(|h| i32::try_from(h).ok())
                    .map(Pt)
                    .ok_or_else(|| E::custom("length out of range"))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Pt, E> {
                i64::try_from(v)
                    .map_err(|_| E::custom("length out of range"))
                    .and_then(|v| self.visit_i64(v))
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Pt, E> {
                // JSON gives us a binary float. The shortest decimal that
                // reads back as the same float is the text the writer meant,
                // so go through it rather than multiplying in floating point.
                let text = format!("{v}");
                self.visit_str(&text)
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Pt, E> {
                Pt::parse_points(v).ok_or_else(|| {
                    E::custom(format!(
                        "\"{v}\" is not a length in points with at most two decimals"
                    ))
                })
            }
        }

        d.deserialize_any(PtVisitor)
    }
}

/// Where a line sits in its column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// How action and dialogue split at a sentence end (layout.md §7.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitMode {
    /// The rest of the line after the sentence end moves to the next page
    /// and is wrapped again.
    #[default]
    MidLine,
    /// Only a sentence end that falls at a line end is a split point.
    LineEnd,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Geometry and style of one kind of element.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementFormat {
    /// Left edge of the text column, from the paper's left edge.
    pub left: Pt,
    /// Right edge of the text column, from the paper's left edge.
    pub right: Pt,
    /// Added to `left` for a paragraph's first line; may be negative.
    #[serde(default)]
    pub first_indent: Pt,
    #[serde(default)]
    pub align: Align,
    /// The usual space above this kind (layout.md §6).
    pub space_before: Pt,
    #[serde(default, skip_serializing_if = "is_false")]
    pub caps: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    /// The kind whose keep rules apply (a shot keeps like a heading).
    pub paginate_as: ElementKind,
    #[serde(default, skip_serializing_if = "is_false")]
    pub starts_new_page: bool,
    /// An explicit number of cells per line, replacing the computed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cells: Option<u16>,
}

impl ElementFormat {
    fn new(kind: ElementKind, left: Pt, right: Pt) -> ElementFormat {
        ElementFormat {
            left,
            right,
            first_indent: Pt::ZERO,
            align: Align::Left,
            space_before: Pt::points(12),
            caps: false,
            bold: false,
            italic: false,
            underline: false,
            paginate_as: kind,
            starts_new_page: false,
            cells: None,
        }
    }

    /// Cells on a paragraph's first line or on later lines, for a grid
    /// `pitch` (layout.md §5.1). At least 1.
    pub fn cells_per_line(&self, pitch: Pt, first_line: bool) -> u32 {
        if pitch.0 <= 0 {
            return 1;
        }
        let p = i64::from(pitch.0);
        let width = i64::from(self.right.0) - i64::from(self.left.0);
        let n = match (self.cells, first_line) {
            (Some(c), false) => i64::from(c),
            (Some(c), true) => i64::from(c) - div_round(i64::from(self.first_indent.0), p),
            (None, false) => width.div_euclid(p),
            (None, true) => (width - i64::from(self.first_indent.0)).div_euclid(p),
        };
        u32::try_from(n.max(1)).unwrap_or(u32::MAX)
    }
}

/// `a / b` rounded half away from zero, for `b > 0`.
fn div_round(a: i64, b: i64) -> i64 {
    let q = (a.abs() * 2 + b) / (b * 2);
    if a < 0 { -q } else { q }
}

/// One format per printing kind.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Elements {
    pub scene_heading: ElementFormat,
    pub action: ElementFormat,
    pub character: ElementFormat,
    pub parenthetical: ElementFormat,
    pub dialogue: ElementFormat,
    pub transition: ElementFormat,
    pub lyric: ElementFormat,
    pub shot: ElementFormat,
    pub general: ElementFormat,
}

impl Elements {
    /// The format for a printing kind; `None` for transparent kinds.
    pub fn get(&self, kind: ElementKind) -> Option<&ElementFormat> {
        use ElementKind::*;
        Some(match kind {
            SceneHeading => &self.scene_heading,
            Action => &self.action,
            Character => &self.character,
            Parenthetical => &self.parenthetical,
            Dialogue => &self.dialogue,
            Transition => &self.transition,
            Lyric => &self.lyric,
            Shot => &self.shot,
            General => &self.general,
            PageBreak | Section | Synopsis | Note | Boneyard => return None,
        })
    }

    pub fn get_mut(&mut self, kind: ElementKind) -> Option<&mut ElementFormat> {
        use ElementKind::*;
        Some(match kind {
            SceneHeading => &mut self.scene_heading,
            Action => &mut self.action,
            Character => &mut self.character,
            Parenthetical => &mut self.parenthetical,
            Dialogue => &mut self.dialogue,
            Transition => &mut self.transition,
            Lyric => &mut self.lyric,
            Shot => &mut self.shot,
            General => &mut self.general,
            PageBreak | Section | Synopsis | Note | Boneyard => return None,
        })
    }

    /// Printing kinds with their formats, in a fixed order.
    pub fn iter(&self) -> impl Iterator<Item = (ElementKind, &ElementFormat)> {
        ElementKind::ALL
            .into_iter()
            .filter_map(|k| self.get(k).map(|f| (k, f)))
    }
}

/// Page-number settings (layout.md §9.1).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageNumbers {
    pub visible: bool,
    /// The first body page that shows its number; body page 1 is the page
    /// after the title page.
    pub first: u32,
    pub suffix: String,
    /// Right edge of the number, from the paper's left edge.
    pub right: Pt,
}

/// One column of dual dialogue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DualColumn {
    pub left: Pt,
    pub right: Pt,
}

/// The two columns of dual dialogue and the indents inside them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DualFormat {
    pub left: DualColumn,
    pub right: DualColumn,
    /// Where the character name starts, from the column's left edge.
    pub character_indent: Pt,
    /// How far a parenthetical sits in from each side of the column. Its
    /// first line also takes the parenthetical's `first_indent`.
    pub parenthetical_inset: Pt,
}

/// Scene "CONTINUED" lines. Modelled so a format can say they are on; they
/// are not printed yet (layout.md §15).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneContinueds {
    pub bottom: Option<String>,
    pub top: Option<String>,
}

/// Every setting layout uses for a script.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageFormat {
    pub page_width: Pt,
    pub page_height: Pt,
    pub margin_top: Pt,
    pub margin_bottom: Pt,
    /// Top of the header line, which holds the page number.
    pub header_top: Pt,
    pub font_size: Pt,
    /// Width of one grid cell.
    pub pitch: Pt,
    pub line_height: Pt,
    /// Distance from a line's top to its baseline.
    pub baseline: Pt,
    pub elements: Elements,
    pub break_at_sentences: bool,
    pub split_mode: SplitMode,
    pub ellipsis_ends_sentence: bool,
    /// A line may break after a hyphen between a letter or digit and a
    /// letter (layout.md §5.2).
    pub hyphen_breaks: bool,
    /// A hyphen that ends a line may sit one cell past the line's last cell
    /// (layout.md §5.2).
    #[serde(default)]
    pub hyphen_overhang: bool,
    /// Printed below the last dialogue line before a split; `None` turns it
    /// off.
    pub more: Option<String>,
    /// Added to the name at the top of the next page; `None` turns it off.
    pub contd: Option<String>,
    pub contd_after_action: bool,
    pub more_contd_in_margin: bool,
    pub page_numbers: PageNumbers,
    pub scene_number_left: Pt,
    pub scene_number_right: Pt,
    pub dual: DualFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_continueds: Option<SceneContinueds>,
}

impl Default for PageFormat {
    /// US Letter, feature film (layout.md §3.2).
    fn default() -> PageFormat {
        use ElementKind::*;
        let inch = Pt::inch100;
        let full = |k| ElementFormat::new(k, inch(150), inch(750));
        let caps = |mut f: ElementFormat| {
            f.caps = true;
            f
        };
        let mut parenthetical = ElementFormat::new(Parenthetical, inch(320), inch(520));
        parenthetical.first_indent = inch(-10);
        let mut transition = caps(ElementFormat::new(Transition, inch(550), inch(750)));
        transition.align = Align::Right;
        let mut lyric = ElementFormat::new(Lyric, inch(250), inch(600));
        lyric.italic = true;
        let mut shot = caps(full(Shot));
        shot.paginate_as = SceneHeading;
        PageFormat {
            page_width: inch(850),
            page_height: inch(1100),
            margin_top: inch(100),
            margin_bottom: inch(100),
            header_top: inch(50),
            font_size: Pt::points(12),
            pitch: Pt(720),
            line_height: Pt::points(12),
            baseline: Pt::points(9),
            elements: Elements {
                scene_heading: caps(full(SceneHeading)),
                action: full(Action),
                character: caps(ElementFormat::new(Character, inch(370), inch(750))),
                parenthetical,
                dialogue: ElementFormat::new(Dialogue, inch(250), inch(600)),
                transition,
                lyric,
                shot,
                general: full(General),
            },
            break_at_sentences: true,
            split_mode: SplitMode::MidLine,
            ellipsis_ends_sentence: false,
            hyphen_breaks: true,
            hyphen_overhang: false,
            more: Some("(MORE)".into()),
            contd: Some("(CONT'D)".into()),
            contd_after_action: false,
            more_contd_in_margin: false,
            page_numbers: PageNumbers {
                visible: true,
                first: 2,
                suffix: ".".into(),
                right: inch(750),
            },
            scene_number_left: inch(75),
            scene_number_right: inch(760),
            dual: DualFormat {
                left: DualColumn {
                    left: inch(150),
                    right: inch(440),
                },
                right: DualColumn {
                    left: inch(460),
                    right: inch(750),
                },
                character_indent: inch(90),
                parenthetical_inset: inch(40),
            },
            scene_continueds: None,
        }
    }
}

/// Longest page-number suffix a format may carry (layout.md §3.3).
pub const MAX_PAGE_SUFFIX: usize = 16;

/// Longest (MORE) or (CONT'D) string a format may carry (layout.md §3.3).
pub const MAX_INSERT: usize = 32;

impl PageFormat {
    /// Height of the body area. Saturates rather than wrapping, so an
    /// unvalidated format can be asked; validation rejects a body that is
    /// not positive.
    pub fn body_height(&self) -> Pt {
        Pt(self
            .page_height
            .0
            .saturating_sub(self.margin_top.0)
            .saturating_sub(self.margin_bottom.0))
    }

    /// Every way this format breaks the rules in layout.md §3.3. Empty
    /// means layout can use it.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        let max_page = Pt::inch100(20_000);
        let max_type = Pt::points(72);
        for (name, v) in [
            ("page_width", self.page_width),
            ("page_height", self.page_height),
        ] {
            if v < Pt::inch100(100) || v > max_page {
                p.push(format!("{name} must be between 1 and 200 inches"));
            }
        }
        for (name, v) in [
            ("font_size", self.font_size),
            ("pitch", self.pitch),
            ("line_height", self.line_height),
        ] {
            if v.0 <= 0 || v > max_type {
                p.push(format!("{name} must be more than 0 and at most 72 pt"));
            }
        }
        if self.baseline < Pt::ZERO || self.baseline > self.line_height {
            p.push("baseline must lie within the line height".into());
        }
        for (name, v) in [
            ("margin_top", self.margin_top),
            ("margin_bottom", self.margin_bottom),
            ("header_top", self.header_top),
        ] {
            if v < Pt::ZERO || v > self.page_height {
                p.push(format!("{name} must lie between 0 and the page height"));
            }
        }
        // In i64: hostile margins must not wrap around to a plausible body.
        let body = i64::from(self.page_height.0)
            - i64::from(self.margin_top.0)
            - i64::from(self.margin_bottom.0);
        if self.line_height.0 > 0 && body < 3 * i64::from(self.line_height.0) {
            p.push("the body must hold at least three lines".into());
        }
        for (name, v) in [
            ("scene_number_left", self.scene_number_left),
            ("scene_number_right", self.scene_number_right),
            ("page_numbers.right", self.page_numbers.right),
        ] {
            if v < Pt::ZERO || v > self.page_width {
                p.push(format!("{name} must lie on the page"));
            }
        }
        let suffix = &self.page_numbers.suffix;
        if suffix.contains(['\n', '\r']) || suffix.chars().count() > MAX_PAGE_SUFFIX {
            p.push(format!(
                "page_numbers.suffix must be one line of at most {MAX_PAGE_SUFFIX} characters"
            ));
        }
        if i64::from(self.header_top.0) + i64::from(self.line_height.0)
            > i64::from(self.page_height.0)
        {
            p.push("the header line falls off the page".into());
        }
        for (kind, f) in self.elements.iter() {
            if f.left < Pt::ZERO || f.right > self.page_width || f.left >= f.right {
                p.push(format!(
                    "{kind}: the column must lie on the page, left of its right edge"
                ));
            }
            if i64::from(f.left.0) + i64::from(f.first_indent.0) < 0
                || f.left.0.saturating_add(f.first_indent.0) >= f.right.0
            {
                p.push(format!(
                    "{kind}: the first line must start on the page, inside the column"
                ));
            }
            if self.pitch.0 > 0 {
                let w = i64::from(f.right.0) - i64::from(f.left.0);
                if f.cells.is_none() && w < i64::from(self.pitch.0) {
                    p.push(format!("{kind}: the column is narrower than one cell"));
                }
            }
            if f.cells == Some(0) {
                p.push(format!("{kind}: cells must be at least 1"));
            }
            if f.space_before < Pt::ZERO || i64::from(f.space_before.0) > body {
                p.push(format!(
                    "{kind}: space_before must be between 0 and the body height"
                ));
            }
        }
        for (name, s) in [("more", &self.more), ("contd", &self.contd)] {
            if let Some(s) = s
                && (s.trim().is_empty()
                    || s.contains(['\n', '\r'])
                    || s.chars().count() > MAX_INSERT)
            {
                p.push(format!(
                    "{name} must be a non-empty single line of at most {MAX_INSERT} characters"
                ));
            }
        }
        let d = &self.dual;
        if d.left.left >= d.left.right || d.right.left >= d.right.right {
            p.push("each dual-dialogue column must be wider than nothing".into());
        }
        if d.left.right > d.right.left {
            p.push("the dual-dialogue columns must not overlap".into());
        }
        if d.left.left < Pt::ZERO || d.right.right > self.page_width {
            p.push("the dual-dialogue columns must lie on the page".into());
        }
        if d.character_indent < Pt::ZERO || d.parenthetical_inset < Pt::ZERO {
            p.push("dual-dialogue indents must not be negative".into());
        }
        p
    }
}

/// One paragraph of a title page given as positioned text, as an .fdx file
/// stores it (layout.md §9.3).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleLine {
    pub text: RichText,
    #[serde(default)]
    pub align: Align,
    pub left: Pt,
    pub right: Pt,
    #[serde(default)]
    pub space_before: Pt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_points_and_inches_exactly() {
        assert_eq!(Pt::parse_points("81"), Some(Pt(8100)));
        assert_eq!(Pt::parse_points("184.32"), Some(Pt(18432)));
        assert_eq!(Pt::parse_points("-7.2"), Some(Pt(-720)));
        assert_eq!(Pt::parse_points("0.29"), Some(Pt(29)));
        assert_eq!(Pt::parse_points("1.005"), None);
        assert_eq!(Pt::parse_points(""), None);
        assert_eq!(Pt::parse_points("."), None);
        assert_eq!(Pt::parse_points("1e3"), None);
        assert_eq!(Pt::parse_inches("2.56"), Some((Pt(18432), true)));
        assert_eq!(Pt::parse_inches("-0.10"), Some((Pt(-720), true)));
        assert_eq!(Pt::parse_inches("7.375"), Some((Pt(53100), true)));
        assert_eq!(Pt::parse_inches("0.001"), Some((Pt(7), false)));
        assert_eq!(Pt::parse_inches("99999999999"), None);
    }

    #[test]
    fn displays_and_serializes_as_points() {
        for (pt, text) in [
            (Pt(8100), "81"),
            (Pt(18432), "184.32"),
            (Pt(-720), "-7.2"),
            (Pt(5), "0.05"),
            (Pt(0), "0"),
        ] {
            assert_eq!(pt.to_string(), text);
            assert_eq!(serde_json::to_string(&pt).unwrap(), text);
            assert_eq!(serde_json::from_str::<Pt>(text).unwrap(), pt);
        }
        assert_eq!(serde_json::from_str::<Pt>("0.29").unwrap(), Pt(29));
        assert_eq!(serde_json::from_str::<Pt>("\"2.5\"").unwrap(), Pt(250));
        assert!(serde_json::from_str::<Pt>("1.234").is_err());
    }

    #[test]
    fn default_format_is_valid_and_matches_the_spec() {
        let f = PageFormat::default();
        assert_eq!(f.problems(), Vec::<String>::new());
        assert_eq!(f.body_height(), Pt::points(648));
        let p = f.pitch;
        let cells = |k| f.elements.get(k).unwrap().cells_per_line(p, false);
        assert_eq!(cells(ElementKind::Action), 60);
        assert_eq!(cells(ElementKind::Dialogue), 35);
        assert_eq!(cells(ElementKind::Character), 38);
        assert_eq!(cells(ElementKind::Parenthetical), 20);
        assert_eq!(f.elements.parenthetical.cells_per_line(p, true), 21);
        assert_eq!(cells(ElementKind::Transition), 20);
    }

    #[test]
    fn explicit_cells_override() {
        let mut f = PageFormat::default().elements.parenthetical;
        f.cells = Some(19);
        assert_eq!(f.cells_per_line(Pt(720), false), 19);
        assert_eq!(f.cells_per_line(Pt(720), true), 20);
    }

    #[test]
    fn format_json_round_trips() {
        let f = PageFormat::default();
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<PageFormat>(&json).unwrap(), f);
        assert!(json.contains("\"pitch\":7.2"), "{json}");
    }

    #[test]
    fn validation_catches_bad_formats() {
        let mut f = PageFormat {
            margin_top: Pt::points(700),
            ..PageFormat::default()
        };
        f.elements.dialogue.left = Pt::points(500);
        f.elements.dialogue.right = Pt::points(400);
        f.more = Some(String::new());
        f.dual.left.right = Pt::inch100(500);
        let p = f.problems();
        assert!(p.iter().any(|s| s.contains("three lines")), "{p:?}");
        assert!(p.iter().any(|s| s.starts_with("dialogue")), "{p:?}");
        assert!(p.iter().any(|s| s.starts_with("more")), "{p:?}");
        assert!(p.iter().any(|s| s.contains("overlap")), "{p:?}");
    }

    #[test]
    fn validation_survives_hostile_values() {
        // Margins this large once wrapped around to a plausible body.
        let f = PageFormat {
            margin_top: Pt(i32::MAX),
            margin_bottom: Pt(i32::MAX),
            ..PageFormat::default()
        };
        let p = f.problems();
        assert!(p.iter().any(|s| s.contains("three lines")), "{p:?}");
        assert!(p.iter().any(|s| s.starts_with("margin_top")), "{p:?}");
        assert_eq!(f.body_height(), Pt(i32::MIN));

        let mut f = PageFormat {
            scene_number_left: Pt(-1),
            scene_number_right: Pt(i32::MAX),
            more: Some("(MORE)".repeat(10)),
            ..PageFormat::default()
        };
        f.page_numbers.right = Pt(i32::MIN);
        f.page_numbers.suffix = "x".repeat(MAX_PAGE_SUFFIX + 1);
        let p = f.problems();
        for name in [
            "scene_number_left",
            "scene_number_right",
            "page_numbers.right",
            "page_numbers.suffix",
            "more",
        ] {
            assert!(p.iter().any(|s| s.starts_with(name)), "{name}: {p:?}");
        }
        f.page_numbers.suffix = ".\n".into();
        assert!(
            f.problems()
                .iter()
                .any(|s| s.starts_with("page_numbers.suffix"))
        );
        assert!(PageFormat::default().problems().is_empty());
    }
}
