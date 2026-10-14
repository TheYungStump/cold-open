//! The pure geometry: overprint removal, baseline clustering, pitch
//! detection, segment splitting, space reconstruction and the page-view
//! grid. Everything works on integer hundredths of a point (hpt) and on
//! synthetic glyph lists, so it is unit-tested without any PDF.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::units::{div_round, div_round_half_up};

/// Smallest and largest glyph size, in hpt, that counts as script text.
pub const MIN_SIZE: i64 = 1000;
pub const MAX_SIZE: i64 = 1400;
/// Overprint tolerance: the same text within this distance is a duplicate.
pub const OVERPRINT_DX: i64 = 100;
pub const OVERPRINT_DY: i64 = 50;
/// A new baseline starts when a glyph lies more than this below the first
/// glyph of the current one.
pub const BASELINE_SPAN: i64 = 200;
/// Origin-to-origin distances that can be a character pitch.
pub const PITCH_MIN: i64 = 500;
pub const PITCH_MAX: i64 = 900;
/// Pitch used for splitting when a document has no measurable pitch.
pub const FALLBACK_PITCH: i64 = 720;
/// Default page-view grid: 10 characters per inch, 6 lines per inch.
pub const GRID_PITCH: i64 = 720;
pub const GRID_LINE: i64 = 1200;

/// One drawn glyph, in page coordinates measured from the top-left corner
/// of the MediaBox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    /// Unicode text pdf-extract decoded for the glyph (usually one char).
    pub text: String,
    /// Origin x from the page's left edge, hpt.
    pub x: i64,
    /// Baseline y from the page's top edge, hpt.
    pub y: i64,
    /// Rendered size, hpt.
    pub size: i64,
    /// True when the glyph is drawn without rotation, skew or mirroring.
    pub upright: bool,
    /// Advance width in page space, hpt.
    pub adv: i64,
    /// Text run: one string handed to a text-showing operator.
    pub run: u32,
    /// Drawing order within the page.
    pub seq: u32,
}

impl Glyph {
    /// Upright and 10–14 pt: script text rather than a watermark or a stamp.
    pub fn is_regular(&self) -> bool {
        self.upright && (MIN_SIZE..=MAX_SIZE).contains(&self.size)
    }

    pub fn is_blank(&self) -> bool {
        self.text.chars().all(char::is_whitespace)
    }
}

/// The glyphs of a page that count, plus what was set aside.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageGlyphs {
    /// Regular glyphs in drawing order, overprints removed.
    pub regular: Vec<Glyph>,
    /// Glyphs that are rotated, skewed or outside 10–14 pt.
    pub odd: u32,
    /// Regular glyphs dropped as duplicates of an earlier glyph.
    pub overprint: u32,
}

/// Splits off odd glyphs and removes overprinted duplicates: a glyph whose
/// text matches an earlier kept glyph within ±1 pt horizontally and
/// ±0.5 pt vertically. The earlier glyph (in drawing order) is kept.
pub fn classify(glyphs: &[Glyph]) -> PageGlyphs {
    let mut order: Vec<&Glyph> = glyphs.iter().collect();
    order.sort_by_key(|g| g.seq);
    let mut out = PageGlyphs::default();
    // Kept glyphs bucketed by (text, y / OVERPRINT_DY); a duplicate can only
    // sit in the same bucket or a neighbouring one.
    let mut kept: BTreeMap<(&str, i64), Vec<(i64, i64)>> = BTreeMap::new();
    for g in order {
        if !g.is_regular() {
            out.odd += 1;
            continue;
        }
        let bucket = g.y.div_euclid(OVERPRINT_DY);
        let dup = (bucket - 1..=bucket + 1).any(|b| {
            kept.get(&(g.text.as_str(), b)).is_some_and(|v| {
                v.iter().any(|&(x, y)| {
                    (g.x - x).abs() <= OVERPRINT_DX && (g.y - y).abs() <= OVERPRINT_DY
                })
            })
        });
        if dup {
            out.overprint += 1;
            continue;
        }
        kept.entry((g.text.as_str(), bucket))
            .or_default()
            .push((g.x, g.y));
        out.regular.push(g.clone());
    }
    out
}

/// Glyphs sharing one baseline, sorted left to right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    /// Median baseline y of the member glyphs, hpt.
    pub y: i64,
    pub glyphs: Vec<Glyph>,
}

/// Groups glyphs into baselines. Glyphs are sorted by y; a new cluster
/// starts when a glyph is more than 2 pt below the first glyph of the
/// current cluster. Each cluster's y is the median of its glyphs' y.
pub fn cluster_baselines(glyphs: &[Glyph]) -> Vec<Baseline> {
    let mut sorted: Vec<&Glyph> = glyphs.iter().collect();
    sorted.sort_by_key(|g| (g.y, g.x, g.seq));
    let mut out = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i].y;
        let mut j = i + 1;
        while j < sorted.len() && sorted[j].y - start <= BASELINE_SPAN {
            j += 1;
        }
        let members = &sorted[i..j];
        let n = members.len();
        // Members are sorted by y, so the median is read off directly.
        let y = if n % 2 == 1 {
            members[n / 2].y
        } else {
            div_round(members[n / 2 - 1].y + members[n / 2].y, 2)
        };
        let mut gl: Vec<Glyph> = members.iter().map(|g| (*g).clone()).collect();
        gl.sort_by_key(|g| (g.x, g.seq));
        out.push(Baseline { y, glyphs: gl });
        i = j;
    }
    out
}

/// Adds the origin-to-origin distances between neighbouring glyphs of a
/// baseline that fall in the plausible pitch range to `hist`.
pub fn pitch_histogram(baseline: &Baseline, hist: &mut BTreeMap<i64, u64>) {
    for w in baseline.glyphs.windows(2) {
        let dx = w[1].x - w[0].x;
        if (PITCH_MIN..=PITCH_MAX).contains(&dx) {
            *hist.entry(dx).or_default() += 1;
        }
    }
}

/// The mode of a histogram; ties go to the smaller value.
pub fn mode(hist: &BTreeMap<i64, u64>) -> Option<i64> {
    let mut best: Option<(i64, u64)> = None;
    for (&v, &n) in hist {
        if best.is_none_or(|(_, bn)| n > bn) {
            best = Some((v, n));
        }
    }
    best.map(|(v, _)| v)
}

/// The document pitch: the mode of neighbouring-glyph dx in 5–9 pt.
pub fn detect_pitch<'a>(baselines: impl IntoIterator<Item = &'a Baseline>) -> Option<i64> {
    let mut hist = BTreeMap::new();
    for b in baselines {
        pitch_histogram(b, &mut hist);
    }
    mode(&hist)
}

/// Number of spaces implied by an origin gap: `max(0, round(dx / p) - 1)`.
pub fn spaces_for_gap(dx: i64, pitch: i64) -> usize {
    (div_round(dx, pitch) - 1).max(0) as usize
}

/// True when `dx` is a whole number of pitches within 0.15 pitch.
pub fn on_pitch(dx: i64, pitch: i64) -> bool {
    let off = (dx - div_round(dx, pitch) * pitch).abs();
    off * 100 <= 15 * pitch
}

/// True when the gap between two neighbouring glyphs ends a segment: the
/// origin gap exceeds 4 pitches (with the same 0.15 pitch tolerance), or a
/// run boundary falls on a gap that is not a whole number of pitches.
pub fn splits(a: &Glyph, b: &Glyph, pitch: i64) -> bool {
    let dx = b.x - a.x;
    if dx * 100 > 415 * pitch {
        return true;
    }
    a.run != b.run && !on_pitch(dx, pitch)
}

/// A horizontal run of text on one baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// Baseline y from the top edge, hpt.
    pub y: i64,
    /// Origin x of the first non-blank glyph, hpt.
    pub x0: i64,
    /// Origin plus advance of the last non-blank glyph, hpt.
    pub x1: i64,
    /// Text with spaces rebuilt from the gaps; leading and trailing blanks
    /// removed.
    pub text: String,
    /// Character cells from the first to the last non-blank glyph,
    /// inclusive: `round((x_last - x0) / p) + 1`.
    pub cells: i64,
    /// Spaces after the last non-blank glyph, up to the last space the PDF
    /// drew there (drawn spaces plus rebuilt gaps between them).
    pub trail: i64,
    /// True when the glyphs came from more than one text run.
    pub multi_run: bool,
}

/// Splits a baseline into segments and rebuilds their text.
pub fn split_segments(baseline: &Baseline, pitch: i64) -> Vec<Segment> {
    let g = &baseline.glyphs;
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=g.len() {
        if i == g.len() || splits(&g[i - 1], &g[i], pitch) {
            if let Some(s) = build_segment(baseline.y, &g[start..i], pitch) {
                out.push(s);
            }
            start = i;
        }
    }
    out
}

fn build_segment(y: i64, glyphs: &[Glyph], pitch: i64) -> Option<Segment> {
    let first = glyphs.iter().position(|g| !g.is_blank())?;
    let last = glyphs.iter().rposition(|g| !g.is_blank())?;
    let mut text = String::new();
    let mut prev: Option<&Glyph> = None;
    for g in &glyphs[first..] {
        if let Some(p) = prev {
            text.extend(std::iter::repeat_n(' ', spaces_for_gap(g.x - p.x, pitch)));
        }
        text.push_str(&g.text);
        prev = Some(g);
    }
    let trimmed_len = text.trim_end().len();
    let trail = text[trimmed_len..].chars().count() as i64;
    text.truncate(trimmed_len);
    let body = &glyphs[first..=last];
    let x0 = body[0].x;
    let lg = &body[body.len() - 1];
    let mut runs: Vec<u32> = body.iter().map(|g| g.run).collect();
    runs.sort_unstable();
    runs.dedup();
    Some(Segment {
        y,
        x0,
        x1: lg.x + lg.adv,
        text,
        cells: div_round(lg.x - x0, pitch) + 1,
        trail,
        multi_run: runs.len() > 1,
    })
}

/// Builds the page-view rows of one page from its regular glyphs.
///
/// A glyph goes in row `floor(y / line)` and column
/// `round_half_up(x / pitch)`. Glyphs sharing a cell are concatenated in
/// drawing order; blank glyphs leave their cell empty; empty cells render
/// as one space; trailing spaces are trimmed. Rows run from 0 to the last
/// non-empty row.
pub fn grid_rows(glyphs: &[Glyph], pitch: i64, line: i64) -> Vec<String> {
    let mut order: Vec<&Glyph> = glyphs
        .iter()
        .filter(|g| g.is_regular() && !g.is_blank())
        .collect();
    order.sort_by_key(|g| g.seq);
    let mut cells: BTreeMap<i64, BTreeMap<i64, String>> = BTreeMap::new();
    for g in order {
        let row = g.y.div_euclid(line);
        let col = div_round_half_up(g.x, pitch);
        if row < 0 || col < 0 {
            continue;
        }
        cells
            .entry(row)
            .or_default()
            .entry(col)
            .or_default()
            .push_str(&g.text);
    }
    let Some(&last) = cells.keys().next_back() else {
        return Vec::new();
    };
    (0..=last)
        .map(|r| {
            let mut s = String::new();
            if let Some(row) = cells.get(&r) {
                let mut next = 0;
                for (&c, t) in row {
                    s.extend(std::iter::repeat_n(' ', (c - next) as usize));
                    s.push_str(t);
                    next = c + 1;
                }
            }
            s.truncate(s.trim_end_matches(' ').len());
            s
        })
        .collect()
}

/// Renders a whole page view: a `=== page {i} ===` header per page, then
/// its rows. The result ends with a single newline (or is empty).
pub fn render_grid(pages: &[Vec<String>]) -> String {
    let mut out = String::new();
    for (i, rows) in pages.iter().enumerate() {
        out.push_str(&format!("=== page {} ===\n", i + 1));
        for r in rows {
            out.push_str(r);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 12 pt upright glyph at (x, y) hpt in run `run`.
    pub fn g(text: &str, x: i64, y: i64, run: u32, seq: u32) -> Glyph {
        Glyph {
            text: text.into(),
            x,
            y,
            size: 1200,
            upright: true,
            adv: 720,
            run,
            seq,
        }
    }

    /// Glyphs for `s` starting at x, one per char at pitch p, all in one run.
    pub fn word(s: &str, x: i64, y: i64, p: i64, run: u32, seq0: u32) -> Vec<Glyph> {
        s.chars()
            .enumerate()
            .map(|(i, c)| g(&c.to_string(), x + p * i as i64, y, run, seq0 + i as u32))
            .collect()
    }

    #[test]
    fn overprints_and_odd_glyphs_are_set_aside() {
        let mut v = word("12.", 50000, 5000, 700, 1, 0);
        // The same page number drawn again, 0.4 pt to the right.
        v.extend(word("12.", 50040, 5010, 700, 2, 10));
        // Same position but a different character is not an overprint.
        v.push(g("X", 50000, 5000, 3, 20));
        // Just outside the tolerance.
        v.push(g("1", 50101, 5000, 4, 21));
        v.push(g("1", 50000, 5051, 5, 22));
        // A rotated watermark glyph and a tiny glyph.
        v.push(Glyph {
            upright: false,
            ..g("W", 20000, 40000, 6, 23)
        });
        v.push(Glyph {
            size: 999,
            ..g("t", 20000, 40000, 7, 24)
        });
        v.push(Glyph {
            size: 1401,
            ..g("t", 20000, 40000, 7, 25)
        });
        let pg = classify(&v);
        assert_eq!(pg.overprint, 3);
        assert_eq!(pg.odd, 3);
        assert_eq!(pg.regular.len(), 6);
        // The first-drawn copy wins.
        assert_eq!(pg.regular[0].x, 50000);
    }

    #[test]
    fn baselines_cluster_within_two_points_and_take_the_median() {
        let v = vec![
            g("a", 0, 10000, 1, 0),
            g("b", 700, 10150, 1, 1),
            g("c", 1400, 10200, 1, 2),
            // 2.01 pt below the first glyph: a new baseline.
            g("d", 0, 10201, 2, 3),
            g("e", 700, 11400, 3, 4),
            g("f", 1400, 11401, 3, 5),
        ];
        let b = cluster_baselines(&v);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].y, 10150);
        assert_eq!(b[0].glyphs.len(), 3);
        assert_eq!(b[1].y, 10201);
        // Even count: the mean of the two middle values, rounded half away.
        assert_eq!(b[2].y, 11401);
        // Glyphs come out left to right even when drawn right to left.
        let w = cluster_baselines(&[g("z", 1400, 0, 1, 0), g("y", 700, 0, 1, 1)]);
        assert_eq!(w[0].glyphs[0].text, "y");
    }

    #[test]
    fn pitch_is_the_mode_within_five_to_nine_points() {
        let mut v = word("HELLO", 10000, 10000, 700, 1, 0);
        v.extend(word("WORLD", 20000, 10000, 700, 2, 10));
        v.extend(word("ab", 10000, 20000, 720, 3, 20));
        // Gaps outside 5–9 pt never count.
        v.extend(word("xyz", 10000, 30000, 400, 4, 30));
        let b = cluster_baselines(&v);
        assert_eq!(detect_pitch(&b), Some(700));
        assert_eq!(
            detect_pitch(&cluster_baselines(&word("xyz", 0, 0, 400, 1, 0))),
            None
        );
        let mut tie = BTreeMap::new();
        tie.insert(720, 3);
        tie.insert(700, 3);
        assert_eq!(mode(&tie), Some(700));
    }

    #[test]
    fn spaces_come_from_origin_gaps() {
        assert_eq!(spaces_for_gap(700, 700), 0);
        assert_eq!(spaces_for_gap(1400, 700), 1);
        assert_eq!(spaces_for_gap(2100, 700), 2);
        assert_eq!(spaces_for_gap(1049, 700), 0);
        assert_eq!(spaces_for_gap(1050, 700), 1);
        assert_eq!(spaces_for_gap(0, 700), 0);
        assert_eq!(spaces_for_gap(-700, 700), 0);
        // Two runs with a one-space gap and a two-space gap, plus a drawn
        // space that must be kept rather than doubled.
        let mut v = word("INT.", 10800, 7200, 700, 1, 0);
        v.extend(word("HOUSE", 10800 + 700 * 5, 7200, 700, 2, 10));
        v.extend(word(" -", 10800 + 700 * 10, 7200, 700, 3, 20));
        v.extend(word("DAY", 10800 + 700 * 14, 7200, 700, 4, 30));
        let b = cluster_baselines(&v);
        let s = split_segments(&b[0], 700);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].text, "INT. HOUSE -  DAY");
        assert_eq!(s[0].cells, 17);
        assert_eq!(s[0].x0, 10800);
        assert_eq!(s[0].x1, 10800 + 700 * 16 + 720);
        assert!(s[0].multi_run);
        assert_eq!(s[0].trail, 0);
    }

    #[test]
    fn segments_split_on_wide_gaps_and_off_pitch_run_boundaries() {
        let p = 700;
        // Three blank cells (gap of 4 pitches) stays together.
        let mut v = word("AB", 0, 0, p, 1, 0);
        v.extend(word("CD", p * 5, 0, p, 2, 10));
        let s = split_segments(&cluster_baselines(&v)[0], p);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].text, "AB   CD");
        // Four blank cells (gap of 5 pitches) splits, even within one run.
        let mut v = word("AB", 0, 0, p, 1, 0);
        v.extend(word("CD", p * 6, 0, p, 1, 10));
        let s = split_segments(&cluster_baselines(&v)[0], p);
        assert_eq!(
            s.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            ["AB", "CD"]
        );
        assert!(!s[0].multi_run);
        // A run boundary 1.5 pitches away splits; within one run it does not.
        let mut v = word("AB", 0, 0, p, 1, 0);
        v.extend(word("CD", p * 2 + p / 2, 0, p, 2, 10));
        assert_eq!(split_segments(&cluster_baselines(&v)[0], p).len(), 2);
        let mut v = word("AB", 0, 0, p, 1, 0);
        v.extend(word("CD", p * 2 + p / 2, 0, p, 1, 10));
        assert_eq!(split_segments(&cluster_baselines(&v)[0], p).len(), 1);
        // A run boundary 0.15 pitch off still counts as on the grid.
        let mut v = word("AB", 0, 0, p, 1, 0);
        v.extend(word("CD", p * 3 + 105, 0, p, 2, 10));
        assert_eq!(split_segments(&cluster_baselines(&v)[0], p).len(), 1);
        let mut v = word("AB", 0, 0, p, 1, 0);
        v.extend(word("CD", p * 3 + 106, 0, p, 2, 10));
        assert_eq!(split_segments(&cluster_baselines(&v)[0], p).len(), 2);
    }

    #[test]
    fn drawn_blanks_are_trimmed_and_trailing_ones_counted() {
        let p = 700;
        let v = word("  HI  ", 1000, 0, p, 1, 0);
        let s = split_segments(&cluster_baselines(&v)[0], p);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].text, "HI");
        assert_eq!(s[0].x0, 1000 + 2 * p);
        assert_eq!(s[0].cells, 2);
        assert_eq!(s[0].trail, 2);
        // A baseline of only blanks yields no segment.
        assert!(split_segments(&cluster_baselines(&word("   ", 0, 0, p, 1, 0))[0], p).is_empty());
        // A combining mark at its base's x adds text but not a cell.
        let v = vec![
            g("e", 0, 0, 1, 0),
            g("\u{301}", 0, 0, 1, 1),
            g("t", p, 0, 1, 2),
        ];
        let s = split_segments(&cluster_baselines(&v)[0], p);
        assert_eq!(s[0].text, "e\u{301}t");
        assert_eq!(s[0].cells, 2);
    }

    #[test]
    fn grid_places_glyphs_by_row_and_rounded_column() {
        let p = GRID_PITCH;
        let l = GRID_LINE;
        let mut v = word("FADE IN:", 10800, 7400, p, 1, 0);
        // A glyph 0.5 cell right of column 2 rounds up; one just below rounds down.
        v.push(g("a", 2 * p + p / 2, 1199, 2, 100));
        v.push(g("b", 5 * p + p / 2 - 1, 1199, 2, 101));
        // A combining mark drawn at its base's x shares the cell.
        v.push(g("e", 10800, 3 * l, 3, 102));
        v.push(g("\u{301}", 10800, 3 * l + 50, 3, 103));
        // Odd glyphs are excluded.
        v.push(Glyph {
            upright: false,
            ..g("W", 0, 2 * l, 4, 104)
        });
        let rows = grid_rows(&v, p, l);
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0], "   a b");
        assert_eq!(rows[1], "");
        assert_eq!(rows[2], "");
        assert_eq!(rows[3], format!("{}e\u{301}", " ".repeat(15)));
        assert_eq!(rows[6], format!("{}FADE IN:", " ".repeat(15)));
        let text = render_grid(&[rows, vec![], vec!["x".into()]]);
        assert!(text.starts_with("=== page 1 ===\n   a b\n\n\n"));
        assert!(text.ends_with("=== page 2 ===\n=== page 3 ===\nx\n"));
        assert!(!text.ends_with("\n\n"));
    }

    #[test]
    fn grid_concatenates_shared_cells_in_drawing_order() {
        let v = vec![g("2", 50, 0, 2, 5), g("1", 0, 0, 1, 1)];
        assert_eq!(grid_rows(&v, 720, 1200), ["12"]);
        assert!(grid_rows(&[], 720, 1200).is_empty());
        // Glyphs above or left of the page are dropped.
        assert!(grid_rows(&[g("x", -400, 100, 1, 0), g("y", 100, -1, 1, 1)], 720, 1200).is_empty());
    }
}
