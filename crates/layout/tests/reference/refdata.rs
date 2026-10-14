//! Reference data written by tools/refpdf, and how its segments are
//! classified (docs/specs/layout.md §13).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RefFile {
    pub schema: u32,
    pub tool: String,
    pub pdf_sha256: String,
    pub meta: Meta,
    pub pitch_x: Option<i64>,
    pub pages: Vec<RefPage>,
}

#[derive(Debug, Deserialize)]
pub struct Meta {
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub pages: u32,
}

#[derive(Debug, Deserialize)]
pub struct RefPage {
    pub index: u32,
    #[allow(dead_code)]
    pub media: Option<[i64; 4]>,
    pub segments: Vec<Segment>,
    #[allow(dead_code)]
    pub odd: u32,
    #[allow(dead_code)]
    pub overprint: u32,
    pub flags: Vec<String>,
    pub error: Option<String>,
}

/// A run of text on one baseline, in 1/100 pt from the page's top-left.
#[derive(Clone, Debug, Deserialize)]
pub struct Segment {
    pub y: i64,
    pub x0: i64,
    pub x1: i64,
    pub text: String,
    pub cells: i64,
    #[allow(dead_code)]
    pub trail: i64,
    #[allow(dead_code)]
    pub multi_run: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Content,
    PageNumber,
    More,
    /// The continued name at the top of a page.
    ContdName,
    SceneNumber,
    Continued,
    Header,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Content => "content",
            Kind::PageNumber => "page_number",
            Kind::More => "more",
            Kind::ContdName => "contd_name",
            Kind::SceneNumber => "scene_number",
            Kind::Continued => "continued",
            Kind::Header => "header",
        }
    }
}

/// A classified segment. `text` has any automatic continued marker removed.
#[derive(Clone, Debug)]
pub struct Line {
    pub page: usize,
    pub y: i64,
    pub x0: i64,
    pub text: String,
    pub kind: Kind,
    /// Printed narrower than the character grid (layout.md §13).
    pub off_grid: bool,
    /// Distance between glyph origins along the line, in 1/100 pt.
    pub advance: Option<i64>,
}

/// The strings and geometry needed to classify segments.
pub struct Context {
    pub more: Option<String>,
    pub contd: Option<String>,
    pub scene_top: Option<String>,
    pub scene_bottom: Option<String>,
    pub page_width: i64,
    pub margin_top: i64,
    pub body_left: i64,
    pub body_right: i64,
    pub pitch: i64,
}

fn norm(s: &str) -> String {
    s.trim()
        .to_uppercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_page_label(s: &str) -> bool {
    let t = s.trim().trim_end_matches('.');
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && t[digits..].chars().all(|c| c.is_ascii_uppercase())
}

fn is_scene_number(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && t.len() <= 8
        && t.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && t.chars().any(|c| c.is_ascii_digit())
}

/// Strips a trailing continued marker (`(CONT'D)` or the file's string),
/// with or without a space before it. Returns the rest when one was found.
pub fn strip_contd(text: &str, contd: Option<&str>) -> Option<String> {
    let t = norm(text);
    let mut markers = vec!["(CONT'D)".to_string(), "(CONTD)".to_string()];
    if let Some(c) = contd {
        markers.push(norm(c));
    }
    for m in markers {
        if let Some(rest) = t.strip_suffix(&m) {
            let rest = rest.trim_end();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// True when a segment holds clearly more characters than the grid cells it
/// spans: it was printed in a narrower font than the page's grid (for
/// example a face the printing machine had to substitute), so it says
/// nothing about where a fixed-pitch layout wraps or breaks.
pub fn off_grid(s: &Segment) -> bool {
    let n = s.text.trim_end().chars().count() as i64;
    s.cells > 0 && n >= s.cells + 3 && n * 10 > s.cells * 11
}

/// The mean distance between glyph origins in a segment, in 1/100 pt.
pub fn advance(s: &Segment) -> Option<i64> {
    let n = s.text.trim_end().chars().count() as i64;
    (n > 1).then(|| (s.x1 - s.x0) / (n - 1))
}

/// Title pages before the body, from printed page numbers: a page whose
/// top-right label is `N.` is body page `N`. The most common answer wins
/// (the smaller on a tie), so one stray label cannot move the body. `None`
/// when no page carries a label.
pub fn title_pages_from_labels(
    pages: &[RefPage],
    page_width: i64,
    margin_top: i64,
) -> Option<usize> {
    let mut votes: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    for (i, p) in pages.iter().enumerate() {
        for s in &p.segments {
            let t = norm(&s.text);
            if s.y < margin_top && s.x0 > page_width / 2 && is_page_label(&t) {
                let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
                if let Ok(n) = digits.parse::<usize>()
                    && n >= 1
                    && i + 1 >= n
                {
                    *votes.entry(i + 1 - n).or_insert(0) += 1;
                }
            }
        }
    }
    let best = votes.values().copied().max()?;
    votes.into_iter().find(|&(_, v)| v == best).map(|(k, _)| k)
}

/// Classifies the segments of the body pages (title pages excluded).
pub fn classify(pages: &[&RefPage], ctx: &Context) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut prev_had_more = false;
    for (pi, page) in pages.iter().enumerate() {
        let mut segs: Vec<&Segment> = page.segments.iter().collect();
        segs.sort_by_key(|s| (s.y, s.x0));
        let mut lines: Vec<Line> = Vec::new();
        for s in segs {
            let t = norm(&s.text);
            let kind = if s.y < ctx.margin_top && s.x0 > ctx.page_width / 2 && is_page_label(&t) {
                Kind::PageNumber
            } else if ctx.more.as_deref().is_some_and(|m| norm(m) == t) {
                Kind::More
            } else if ctx.scene_top.as_deref().is_some_and(|m| norm(m) == t)
                || ctx.scene_bottom.as_deref().is_some_and(|m| norm(m) == t)
            {
                Kind::Continued
            } else if is_scene_number(&t)
                && (s.x1 <= ctx.body_left - ctx.pitch / 2 || s.x0 >= ctx.body_right)
            {
                Kind::SceneNumber
            } else if s.y < ctx.margin_top - 2 * ctx.pitch
                && lines.iter().all(|l| l.kind != Kind::Content)
                && !is_page_label(&t)
                && t.len() > 30
            {
                Kind::Header
            } else {
                Kind::Content
            };
            lines.push(Line {
                page: pi,
                y: s.y,
                x0: s.x0,
                text: s.text.clone(),
                kind,
                off_grid: off_grid(s),
                advance: advance(s),
            });
        }
        // The first content line of a page that follows a (MORE), if it
        // carries the continued marker, is the continued name, not content.
        if prev_had_more
            && let Some(first) = lines.iter_mut().find(|l| l.kind == Kind::Content)
            && strip_contd(&first.text, ctx.contd.as_deref()).is_some()
        {
            first.kind = Kind::ContdName;
        }
        // Other names carrying the marker (the same speaker after action)
        // keep their name and lose the marker.
        for l in lines.iter_mut().filter(|l| l.kind == Kind::Content) {
            if let Some(rest) = strip_contd(&l.text, ctx.contd.as_deref()) {
                l.text = rest;
            }
        }
        prev_had_more = lines.iter().any(|l| l.kind == Kind::More);
        out.extend(lines);
    }
    out
}

/// Content lines in reading order. Rows where text sits on both sides of a
/// gap are dual dialogue; inside such a region the left column is read
/// before the right one.
pub fn reading_order(lines: &[Line], line_height: i64) -> Vec<Line> {
    let mut out = Vec::new();
    let pages = lines.iter().map(|l| l.page).max().map_or(0, |m| m + 1);
    for p in 0..pages {
        let content: Vec<&Line> = lines
            .iter()
            .filter(|l| l.page == p && l.kind == Kind::Content)
            .collect();
        // Group into rows by baseline.
        let mut rows: Vec<Vec<&Line>> = Vec::new();
        for l in content {
            match rows.last_mut() {
                Some(r) if (r[0].y - l.y).abs() <= 100 => r.push(l),
                _ => rows.push(vec![l]),
            }
        }
        let mut i = 0;
        while i < rows.len() {
            if rows[i].len() < 2 {
                out.extend(rows[i].iter().map(|l| (*l).clone()));
                i += 1;
                continue;
            }
            // A dual region: the two-column row and the rows right below it,
            // one line apart. The element after a dual block has space above
            // it, which ends the region.
            let split = {
                let r = &rows[i];
                (r[0].x0 + r[1].x0) / 2
            };
            let mut j = i + 1;
            let mut last_y = rows[i][0].y;
            while j < rows.len() && rows[j][0].y - last_y <= line_height + 100 {
                last_y = rows[j][0].y;
                j += 1;
            }
            let region: Vec<&Line> = rows[i..j].iter().flatten().copied().collect();
            out.extend(region.iter().filter(|l| l.x0 < split).map(|l| (*l).clone()));
            out.extend(
                region
                    .iter()
                    .filter(|l| l.x0 >= split)
                    .map(|l| (*l).clone()),
            );
            i = j;
        }
    }
    out
}
