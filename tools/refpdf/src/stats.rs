//! Numeric calibration statistics over `.ref.json` files. Nothing here
//! ever prints segment text: reference scripts are copyrighted, and these
//! numbers are what goes into reports and specs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::geom::{GRID_LINE, Segment, mode};
use crate::reffile::RefFile;
use crate::units::{div_round_half_up, fmt_pt};

/// Page numbers sit in the top 60 pt of the page.
const PAGE_NUMBER_ZONE: i64 = 6000;
/// How many x0 modes get a cells histogram.
const TOP_X0: usize = 8;

type Hist = BTreeMap<i64, u64>;

/// Matches `^\d+[A-Z]*\.$`.
pub fn looks_like_page_number(text: &str) -> bool {
    let Some(body) = text.strip_suffix('.') else {
        return false;
    };
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && body.bytes().skip(digits).all(|b| b.is_ascii_uppercase())
}

fn is_page_number(s: &Segment) -> bool {
    s.y <= PAGE_NUMBER_ZONE && looks_like_page_number(&s.text)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub pages: usize,
    pub failed: usize,
    pub segments: usize,
    pub odd: u64,
    pub overprint: u64,
    pub pitch_x: Option<i64>,
    /// Distances between consecutive body baselines on a page.
    pub dy: Hist,
    pub pitch_y: Option<i64>,
    /// Line height used for rows: `pitch_y`, or 12 pt without one.
    pub line: i64,
    /// First body baseline per page.
    pub first_y: Hist,
    /// Row (`floor(y / line)`) of the first and last body baseline per page.
    pub first_row: Hist,
    pub last_row: Hist,
    pub x0: Hist,
    pub cells_by_x0: BTreeMap<i64, Hist>,
    pub page_number_y: Hist,
    pub page_number_x1: Hist,
}

pub fn compute(r: &RefFile) -> Stats {
    let mut st = Stats {
        pages: r.pages.len(),
        pitch_x: r.pitch_x,
        ..Stats::default()
    };
    let mut body_ys: Vec<Vec<i64>> = Vec::new();
    for p in &r.pages {
        st.odd += u64::from(p.odd);
        st.overprint += u64::from(p.overprint);
        if p.error.is_some() {
            st.failed += 1;
            continue;
        }
        st.segments += p.segments.len();
        let mut ys = Vec::new();
        for s in &p.segments {
            if is_page_number(s) {
                *st.page_number_y.entry(s.y).or_default() += 1;
                *st.page_number_x1.entry(s.x1).or_default() += 1;
                continue;
            }
            ys.push(s.y);
            *st.x0.entry(s.x0).or_default() += 1;
            *st.cells_by_x0
                .entry(s.x0)
                .or_default()
                .entry(s.cells)
                .or_default() += 1;
        }
        ys.sort_unstable();
        ys.dedup();
        for w in ys.windows(2) {
            *st.dy.entry(w[1] - w[0]).or_default() += 1;
        }
        body_ys.push(ys);
    }
    st.pitch_y = mode(&st.dy);
    st.line = st.pitch_y.filter(|&l| l > 0).unwrap_or(GRID_LINE);
    for ys in body_ys {
        if let (Some(&first), Some(&last)) = (ys.first(), ys.last()) {
            *st.first_y.entry(first).or_default() += 1;
            *st.first_row.entry(first.div_euclid(st.line)).or_default() += 1;
            *st.last_row.entry(last.div_euclid(st.line)).or_default() += 1;
        }
    }
    st
}

/// Histogram entries by count, most frequent first, ties by value.
fn by_count(h: &Hist) -> Vec<(i64, u64)> {
    let mut v: Vec<(i64, u64)> = h.iter().map(|(&k, &n)| (k, n)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v
}

fn pt_hist(h: &Hist, limit: usize) -> String {
    let v = by_count(h);
    let mut s: Vec<String> = v
        .iter()
        .take(limit)
        .map(|(k, n)| format!("{}x{n}", fmt_pt(*k)))
        .collect();
    if v.len() > limit {
        s.push(format!("(+{} more)", v.len() - limit));
    }
    if s.is_empty() {
        "none".into()
    } else {
        s.join("  ")
    }
}

fn int_hist(h: &Hist) -> String {
    let s: Vec<String> = h.iter().map(|(k, n)| format!("{k}:{n}")).collect();
    if s.is_empty() {
        "none".into()
    } else {
        s.join(" ")
    }
}

pub fn render(name: &str, st: &Stats) -> String {
    let mut o = String::new();
    let pt = |v: Option<i64>| v.map_or("none".into(), |v| format!("{} pt", fmt_pt(v)));
    let _ = writeln!(o, "== {name} ==");
    let _ = writeln!(
        o,
        "pages {}  failed {}  segments {}  overprints {}  odd glyphs {}",
        st.pages, st.failed, st.segments, st.overprint, st.odd
    );
    let _ = writeln!(o, "pitch_x {}", pt(st.pitch_x));
    let _ = writeln!(
        o,
        "pitch_y {}   baseline dy: {}",
        pt(st.pitch_y),
        pt_hist(&st.dy, 6)
    );
    let _ = writeln!(
        o,
        "first body baseline y per page: {}",
        pt_hist(&st.first_y, 8)
    );
    let _ = writeln!(
        o,
        "rows use line {} pt; row = floor(y / line)",
        fmt_pt(st.line)
    );
    let _ = writeln!(o, "first body row per page: {}", int_hist(&st.first_row));
    let _ = writeln!(o, "last body row per page:  {}", int_hist(&st.last_row));
    match st.last_row.iter().next_back() {
        Some((r, n)) => {
            let _ = writeln!(
                o,
                "max content row {r}, reached by {n} of {} pages",
                st.pages - st.failed
            );
        }
        None => {
            let _ = writeln!(o, "max content row none");
        }
    }
    let top = by_count(&st.x0);
    let _ = writeln!(o, "x0 modes (pt, column at pitch_x, segments):");
    for (x0, n) in top.iter().take(TOP_X0 * 2) {
        let col = st
            .pitch_x
            .filter(|&p| p > 0)
            .map_or("-".into(), |p| div_round_half_up(*x0, p).to_string());
        let _ = writeln!(o, "  {:>7}  col {:>3}  x{n}", fmt_pt(*x0), col);
    }
    if top.len() > TOP_X0 * 2 {
        let _ = writeln!(o, "  (+{} more x0 values)", top.len() - TOP_X0 * 2);
    }
    let _ = writeln!(o, "segment cells per x0 (cells:count):");
    for (x0, _) in top.iter().take(TOP_X0) {
        let h = &st.cells_by_x0[x0];
        let max = h.keys().next_back().copied().unwrap_or(0);
        let _ = writeln!(o, "  {:>7}  max {max:>3}  {}", fmt_pt(*x0), int_hist(h));
    }
    let n: u64 = st.page_number_y.values().sum();
    let _ = writeln!(
        o,
        "page-number segments {n}: y {}  x1 {}",
        pt_hist(&st.page_number_y, 4),
        pt_hist(&st.page_number_x1, 4)
    );
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reffile::{Meta, RefPage};

    fn seg(y: i64, x0: i64, cells: i64, text: &str) -> Segment {
        Segment {
            y,
            x0,
            x1: x0 + cells * 700,
            text: text.into(),
            cells,
            trail: 0,
            multi_run: false,
        }
    }

    #[test]
    fn page_numbers_match_the_pattern() {
        for t in ["1.", "12.", "12A.", "3AB."] {
            assert!(looks_like_page_number(t), "{t}");
        }
        for t in ["", ".", "A.", "12", "12a.", "1.2.", " 1.", "12. "] {
            assert!(!looks_like_page_number(t), "{t}");
        }
    }

    #[test]
    fn statistics_are_computed_without_text() {
        let page = |i: u32, segs: Vec<Segment>| RefPage {
            index: i,
            media: Some([0, 0, 61200, 79200]),
            segments: segs,
            odd: 1,
            overprint: 2,
            flags: vec![],
            error: None,
        };
        let r = RefFile {
            schema: 1,
            tool: "t".into(),
            pdf_sha256: String::new(),
            meta: Meta {
                creator: None,
                producer: None,
                pages: 3,
            },
            pitch_x: Some(700),
            pages: vec![
                page(
                    1,
                    vec![
                        seg(7400, 10800, 10, "SECRET WORDS"),
                        seg(8600, 10800, 50, "x"),
                        seg(9800, 25200, 8, "y"),
                    ],
                ),
                page(
                    2,
                    vec![
                        seg(5000, 50400, 2, "2."),
                        seg(8600, 10800, 50, "z"),
                        seg(9800, 17500, 35, "w"),
                        seg(12200, 17500, 30, "v"),
                    ],
                ),
                RefPage {
                    error: Some("boom".into()),
                    segments: vec![],
                    ..page(3, vec![])
                },
            ],
        };
        let st = compute(&r);
        assert_eq!(
            (st.pages, st.failed, st.segments, st.odd, st.overprint),
            (3, 1, 7, 3, 6)
        );
        assert_eq!(st.pitch_y, Some(1200));
        assert_eq!(st.dy.get(&2400), Some(&1));
        assert_eq!(st.first_y, BTreeMap::from([(7400, 1), (8600, 1)]));
        assert_eq!(st.first_row, BTreeMap::from([(6, 1), (7, 1)]));
        assert_eq!(st.last_row, BTreeMap::from([(8, 1), (10, 1)]));
        assert_eq!(st.x0, BTreeMap::from([(10800, 3), (17500, 2), (25200, 1)]));
        assert_eq!(st.cells_by_x0[&10800], BTreeMap::from([(10, 1), (50, 2)]));
        assert_eq!(st.page_number_y, BTreeMap::from([(5000, 1)]));
        let text = render("f", &st);
        for secret in ["SECRET", "WORDS", "2.\n"] {
            assert!(!text.contains(secret), "{text}");
        }
        assert!(
            text.contains("max content row 10, reached by 1 of 2 pages"),
            "{text}"
        );
        assert!(text.contains("pitch_y 12.00 pt"), "{text}");
    }
}
