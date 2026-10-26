//! Production revision sets (docs/specs/revisions.md): the revision table,
//! its color list, and the rules for names, labels and dates.

use serde::{Deserialize, Serialize};

pub use coldopen_screenplay::RevisionId;

/// The usual order of revision colors (docs/specs/revisions.md §2.1): nine
/// colors, then the same nine again with "Double" in front.
pub const COLORS: [&str; 9] = [
    "White",
    "Blue",
    "Pink",
    "Yellow",
    "Green",
    "Goldenrod",
    "Buff",
    "Salmon",
    "Cherry",
];

/// The most names a color list may hold.
pub const MAX_COLORS: usize = 64;
/// The longest color name, in characters.
pub const MAX_COLOR: usize = 32;
/// The longest label, in characters.
pub const MAX_LABEL: usize = 40;

/// The default color list: two rounds of [`COLORS`].
pub fn default_colors() -> Vec<String> {
    COLORS
        .iter()
        .map(|c| c.to_string())
        .chain(COLORS.iter().map(|c| format!("Double {c}")))
        .collect()
}

/// One revision set (docs/specs/revisions.md §3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionSet {
    pub id: RevisionId,
    pub color: String,
    pub label: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Milliseconds since the Unix epoch.
    pub started: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<u64>,
}

/// The revision table: the sets `r_1`, `r_2`, … with no gaps, the color
/// list, and the active set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revisions {
    pub v: u32,
    pub colors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<RevisionId>,
    #[serde(default)]
    pub sets: Vec<RevisionSet>,
    /// For each block the active set has marked, the block as it was
    /// before the set first marked it. Marks are worked out against it, so
    /// text put back as it was loses the set's mark (docs/specs/
    /// revisions.md §6.4, 2026-10-26). Empty with no set active.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub base: std::collections::BTreeMap<crate::document::BlockId, BaseBlock>,
}

/// A block as the active set found it (§6.4): its kind and printing
/// attributes in the distributed draft, its text with the marks it had,
/// and its block mark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseBlock {
    pub kind: coldopen_screenplay::ElementKind,
    pub element: coldopen_screenplay::Element,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<RevisionId>,
}

/// The version of the table's JSON form.
pub const REVISIONS_V: u32 = 1;

impl Default for Revisions {
    fn default() -> Revisions {
        Revisions {
            v: REVISIONS_V,
            colors: default_colors(),
            active: None,
            sets: Vec::new(),
            base: std::collections::BTreeMap::new(),
        }
    }
}

impl Revisions {
    /// No sets, the default colors and no active set: written exactly as
    /// before phase 5b (docs/specs/revisions.md §3).
    pub fn is_default(&self) -> bool {
        *self == Revisions::default()
    }

    pub fn get(&self, id: RevisionId) -> Option<&RevisionSet> {
        let i = usize::try_from(id.0).ok()?.checked_sub(1)?;
        self.sets.get(i)
    }

    pub fn active_set(&self) -> Option<&RevisionSet> {
        self.active.and_then(|id| self.get(id))
    }

    /// The id the next set will take.
    pub fn next_id(&self) -> RevisionId {
        RevisionId(self.sets.len() as u64 + 1)
    }

    /// The color `revision.start` takes when none is given: the entry
    /// after the newest set's color, or the second entry when there is no
    /// set yet, since the first names the draft before any revision
    /// (docs/specs/revisions.md §2.2).
    pub fn next_color(&self) -> String {
        let n = self.colors.len();
        if n == 0 {
            return COLORS[1].to_string();
        }
        let at = match self.sets.last() {
            None => 1 % n,
            Some(s) => match self
                .colors
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&s.color))
            {
                Some(i) if i + 1 < n => i + 1,
                _ => 0,
            },
        };
        self.colors[at].clone()
    }

    /// The model rules the table must keep, as a journal header or bundle
    /// is checked: ids numbered from 1 with no gaps, valid names and dates,
    /// and an active set that exists and has not ended.
    pub fn problems(&self) -> Vec<String> {
        let mut p = Vec::new();
        if let Err(e) = check_colors(&self.colors) {
            p.push(e);
        }
        for (i, s) in self.sets.iter().enumerate() {
            if s.id != RevisionId(i as u64 + 1) {
                p.push(format!("revision set {} is out of order", s.id));
            }
            if let Err(e) = check_name(&s.color, "color", MAX_COLOR) {
                p.push(format!("{}: {e}", s.id));
            }
            if let Err(e) = check_name(&s.label, "label", MAX_LABEL) {
                p.push(format!("{}: {e}", s.id));
            }
            if let Err(e) = parse_date(&s.date) {
                p.push(format!("{}: {e}", s.id));
            }
        }
        if let Some(a) = self.active {
            match self.get(a) {
                None => p.push(format!("the active set {a} does not exist")),
                Some(s) if s.ended.is_some() => {
                    p.push(format!("the active set {a} has ended"));
                }
                Some(_) => {}
            }
        }
        p
    }

    /// The table read back from its JSON form.
    pub fn from_json(bytes: &[u8]) -> Result<Revisions, String> {
        let t: Revisions = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if t.v != REVISIONS_V {
            return Err(format!(
                "the revision table is version {}; this version reads {REVISIONS_V}",
                t.v
            ));
        }
        let problems = t.problems();
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        Ok(t)
    }

    /// Sets a mark names that the table lacks, as a damaged bundle needs
    /// them (docs/specs/bundle.md §7): each gets the color "Unknown".
    pub fn fill_to(&mut self, highest: u64) {
        while (self.sets.len() as u64) < highest {
            let id = self.next_id();
            self.sets.push(RevisionSet {
                id,
                color: "Unknown".into(),
                label: "Unknown Rev.".into(),
                date: "1900-01-01".into(),
                started: 0,
                ended: Some(0),
            });
        }
    }
}

/// A color name or label: trimmed, 1 to `max` characters, no line breaks.
pub fn check_name(s: &str, what: &str, max: usize) -> Result<(), String> {
    let n = s.chars().count();
    if s.trim() != s {
        Err(format!("a {what} may not start or end with spaces"))
    } else if n == 0 || n > max {
        Err(format!("a {what} needs 1 to {max} characters"))
    } else if s.chars().any(char::is_control) {
        Err(format!(
            "a {what} may not hold line breaks or control characters"
        ))
    } else {
        Ok(())
    }
}

/// A color list: 1 to 64 valid names, no two the same ignoring case.
pub fn check_colors(colors: &[String]) -> Result<(), String> {
    if colors.is_empty() || colors.len() > MAX_COLORS {
        return Err(format!("the color list needs 1 to {MAX_COLORS} names"));
    }
    let mut seen: Vec<String> = Vec::with_capacity(colors.len());
    for c in colors {
        check_name(c, "color", MAX_COLOR)?;
        let key = c.to_lowercase();
        if seen.contains(&key) {
            return Err(format!("the color \"{c}\" is in the list twice"));
        }
        seen.push(key);
    }
    Ok(())
}

/// A date as `(year, month, day)`, from `YYYY-MM-DD`: a real calendar
/// date from 1900 to 9999.
pub fn parse_date(s: &str) -> Result<(u32, u32, u32), String> {
    let bad = || format!("\"{s}\" is not a date written YYYY-MM-DD from 1900 to 9999");
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(bad());
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = s.get(r)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (Some(y), Some(m), Some(d)) = (num(0..4), num(5..7), num(8..10)) else {
        return Err(bad());
    };
    if !(1900..=9999).contains(&y) || !(1..=12).contains(&m) || d == 0 || d > days_in(y, m) {
        return Err(bad());
    }
    Ok((y, m, d))
}

fn days_in(y: u32, m: u32) -> u32 {
    match m {
        2 if (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The UTC date of a time in milliseconds since the Unix epoch, as
/// `YYYY-MM-DD`: the default for `revision.start` (docs/specs/revisions.md
/// §5). Days to a civil date by the usual era arithmetic, in integers.
pub fn utc_date(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The color a name stands for, for its paper and inks: the name without a
/// leading "Double ", "2nd " or "Triple ", ignoring case
/// (docs/specs/revisions.md §2.3).
fn base(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut s = lower.as_str();
    for prefix in ["double ", "2nd ", "triple "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
        }
    }
    s.trim().to_string()
}

/// A color's paper and its ink for the light, dark and high-contrast
/// themes, as RGB; `None` for an ink means the theme's text color
/// (docs/specs/revisions.md §2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shades {
    pub paper: (u8, u8, u8),
    pub inks: [Option<(u8, u8, u8)>; 3],
}

const fn rgb(v: u32) -> (u8, u8, u8) {
    ((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// The paper and inks of a color name. A name that matches nothing gets
/// white paper and the text color.
pub fn shades(name: &str) -> Shades {
    let s = |paper: u32, light: u32, dark: u32, high: u32| Shades {
        paper: rgb(paper),
        inks: [Some(rgb(light)), Some(rgb(dark)), Some(rgb(high))],
    };
    let plain = |paper: u32| Shades {
        paper: rgb(paper),
        inks: [None; 3],
    };
    match base(name).as_str() {
        "blue" => s(0xCFE3FF, 0x1F5FBF, 0x7FB2FF, 0x66CCFF),
        "pink" => s(0xFFD6E7, 0xB8336A, 0xFF8FBF, 0xFF99CC),
        "yellow" => s(0xFFF6B3, 0x8A6D00, 0xF2D15C, 0xFFFF66),
        "green" => s(0xD4F2CF, 0x2E7D32, 0x7FD98A, 0x66FF66),
        "goldenrod" => s(0xF5DFA0, 0x9A6A00, 0xE6B84D, 0xFFCC33),
        "buff" => s(0xF2E3C6, 0x7A5C3A, 0xD9B98C, 0xFFD9A0),
        "salmon" => s(0xFFD1C2, 0xB5462A, 0xFF9C80, 0xFF9966),
        "cherry" => s(0xF7C1C8, 0xA3122E, 0xFF7A8F, 0xFF6680),
        "tan" => s(0xE8D9C0, 0x7D6440, 0xCCB088, 0xFFE0B0),
        "ivory" => plain(0xFFFBEA),
        _ => plain(0xFFFFFF),
    }
}

/// What a revised page's header prints: the label, a space and the date
/// as `MM/DD/YY` (docs/specs/revisions.md §7.2).
pub fn header_text(set: &RevisionSet) -> String {
    match parse_date(&set.date) {
        Ok((y, m, d)) => format!("{} {m:02}/{d:02}/{:02}", set.label, y % 100),
        Err(_) => set.label.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(n: u64, color: &str) -> RevisionSet {
        RevisionSet {
            id: RevisionId(n),
            color: color.into(),
            label: format!("{color} Rev."),
            date: "2026-10-22".into(),
            started: 1,
            ended: Some(2),
        }
    }

    #[test]
    fn the_default_list_has_two_rounds() {
        let c = default_colors();
        assert_eq!(c.len(), 18);
        assert_eq!(c[0], "White");
        assert_eq!(c[8], "Cherry");
        assert_eq!(c[9], "Double White");
        assert_eq!(c[17], "Double Cherry");
    }

    #[test]
    fn the_next_color_follows_the_newest_set() {
        let mut t = Revisions::default();
        assert_eq!(t.next_color(), "Blue");
        t.sets.push(set(1, "Blue"));
        assert_eq!(t.next_color(), "Pink");
        t.sets.push(set(2, "double cherry"));
        assert_eq!(t.next_color(), "White");
        t.sets.push(set(3, "Lavender"));
        assert_eq!(t.next_color(), "White");
    }

    #[test]
    fn dates_are_checked_and_formatted() {
        assert_eq!(parse_date("2026-10-22"), Ok((2026, 10, 22)));
        assert_eq!(parse_date("2024-02-29"), Ok((2024, 2, 29)));
        for bad in [
            "2023-02-29",
            "1899-12-31",
            "2026-13-01",
            "2026-1-01",
            "２026-10-22",
            "",
        ] {
            assert!(parse_date(bad).is_err(), "{bad}");
        }
        assert_eq!(header_text(&set(1, "Blue")), "Blue Rev. 10/22/26");
    }

    #[test]
    fn shades_follow_the_base_color() {
        assert_eq!(shades("Blue").paper, (0xCF, 0xE3, 0xFF));
        assert_eq!(shades("Double BLUE"), shades("blue"));
        assert_eq!(shades("2nd Pink"), shades("Pink"));
        assert_eq!(shades("White").inks, [None; 3]);
        assert_eq!(shades("Lavender").paper, (0xFF, 0xFF, 0xFF));
    }

    #[test]
    fn utc_dates_from_epoch_milliseconds() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400_000), "2000-02-29");
        assert_eq!(utc_date(1_761_091_200_000), "2025-10-22");
        assert_eq!(utc_date(1_761_091_199_999), "2025-10-21");
    }

    #[test]
    fn the_table_checks_its_rules() {
        let mut t = Revisions::default();
        assert!(t.problems().is_empty());
        t.sets.push(set(2, "Blue"));
        assert!(!t.problems().is_empty());
        t.sets[0].id = RevisionId(1);
        t.active = Some(RevisionId(1));
        assert!(t.problems().iter().any(|p| p.contains("has ended")));
        t.sets[0].ended = None;
        assert!(t.problems().is_empty());
        assert!(check_colors(&["Blue".into(), "blue".into()]).is_err());
        assert!(check_name(" Blue", "color", MAX_COLOR).is_err());
    }
}
