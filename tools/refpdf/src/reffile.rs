//! The `<stem>.ref.json` format and the checks printed by `extract`.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::extract::DocExtract;
use crate::geom::{self, Baseline, PageGlyphs, Segment};
use crate::units::fmt_pt;

pub const SCHEMA: u32 = 1;
pub const TOOL: &str = "refpdf 0.1";

/// US Letter in hpt, and how far a MediaBox may stray from it.
const LETTER: (i64, i64) = (61200, 79200);
const MEDIA_TOLERANCE: i64 = 50;
/// Accepted document pitch, hpt.
const PITCH_OK: std::ops::RangeInclusive<i64> = 650..=750;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefFile {
    pub schema: u32,
    pub tool: String,
    pub pdf_sha256: String,
    pub meta: Meta,
    /// Detected character pitch, hpt.
    pub pitch_x: Option<i64>,
    pub pages: Vec<RefPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub pages: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefPage {
    pub index: u32,
    pub media: Option<[i64; 4]>,
    pub segments: Vec<Segment>,
    pub odd: u32,
    pub overprint: u32,
    pub flags: Vec<String>,
    pub error: Option<String>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The regular glyphs of every page plus the document pitch.
pub struct Analysis {
    pub pages: Vec<PageGlyphs>,
    pub baselines: Vec<Vec<Baseline>>,
    pub pitch: Option<i64>,
}

pub fn analyze(doc: &DocExtract) -> Analysis {
    let pages: Vec<PageGlyphs> = doc
        .pages
        .iter()
        .map(|p| geom::classify(&p.glyphs))
        .collect();
    let baselines: Vec<Vec<Baseline>> = pages
        .iter()
        .map(|p| geom::cluster_baselines(&p.regular))
        .collect();
    let pitch = geom::detect_pitch(baselines.iter().flatten());
    Analysis {
        pages,
        baselines,
        pitch,
    }
}

pub fn build(sha256: String, doc: &DocExtract, an: &Analysis) -> RefFile {
    let p = an.pitch.unwrap_or(geom::FALLBACK_PITCH);
    let pages = doc
        .pages
        .iter()
        .zip(&an.pages)
        .zip(&an.baselines)
        .map(|((pe, pg), bl)| RefPage {
            index: pe.index,
            media: pe.media,
            segments: bl.iter().flat_map(|b| geom::split_segments(b, p)).collect(),
            odd: pg.odd,
            overprint: pg.overprint,
            flags: pe.flags.clone(),
            error: pe.error.clone(),
        })
        .collect();
    RefFile {
        schema: SCHEMA,
        tool: TOOL.into(),
        pdf_sha256: sha256,
        meta: Meta {
            creator: doc.creator.clone(),
            producer: doc.producer.clone(),
            pages: doc.pages.len() as u32,
        },
        pitch_x: an.pitch,
        pages,
    }
}

/// Pretty JSON with every object's keys sorted, ending in a newline.
pub fn to_json(r: &RefFile) -> String {
    // serde_json's Map is a BTreeMap unless `preserve_order` is enabled,
    // which this workspace never does, so going through Value sorts keys.
    let v = serde_json::to_value(r).expect("RefFile serializes");
    let mut s = serde_json::to_string_pretty(&v).expect("Value serializes");
    s.push('\n');
    s
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub msg: String,
}

/// True for printable Latin-1 and General Punctuation.
pub fn is_expected_char(c: char) -> bool {
    matches!(c, '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' | '\u{2000}'..='\u{206f}')
}

/// The checks `extract` prints. None of them prints script text.
pub fn checks(r: &RefFile, an: &Analysis) -> Vec<Check> {
    let mut out = Vec::new();
    let mut push = |level, msg: String| out.push(Check { level, msg });

    let fd = [&r.meta.creator, &r.meta.producer]
        .iter()
        .any(|s| s.as_deref().is_some_and(|s| s.contains("Final Draft")));
    push(
        if fd { Level::Ok } else { Level::Warn },
        if fd {
            "Creator or Producer names Final Draft".into()
        } else {
            "neither Creator nor Producer names Final Draft".into()
        },
    );

    // A page without a readable MediaBox has already failed, and the
    // failed-pages check reports it.
    let known: Vec<[i64; 4]> = r.pages.iter().filter_map(|p| p.media).collect();
    let bad_media = known
        .iter()
        .filter(|[x0, y0, x1, y1]| {
            ((x1 - x0) - LETTER.0).abs() > MEDIA_TOLERANCE
                || ((y1 - y0) - LETTER.1).abs() > MEDIA_TOLERANCE
        })
        .count();
    let unknown = r.pages.len() - known.len();
    push(
        if bad_media == 0 {
            Level::Ok
        } else {
            Level::Fail
        },
        format!(
            "{} of {} MediaBoxes are 612x792 within 0.5 pt{}",
            known.len() - bad_media,
            known.len(),
            if unknown > 0 {
                format!(" ({unknown} pages have none)")
            } else {
                String::new()
            }
        ),
    );

    match r.pitch_x {
        Some(p) if PITCH_OK.contains(&p) => push(
            Level::Ok,
            format!("pitch {} pt is within [6.5, 7.5]", fmt_pt(p)),
        ),
        Some(p) => push(
            Level::Fail,
            format!("pitch {} pt is outside [6.5, 7.5]", fmt_pt(p)),
        ),
        None => push(Level::Fail, "no pitch detected".into()),
    }

    let (mut total, mut outside) = (0u64, 0u64);
    for c in an
        .pages
        .iter()
        .flat_map(|p| &p.regular)
        .flat_map(|g| g.text.chars())
    {
        total += 1;
        if !is_expected_char(c) {
            outside += 1;
        }
    }
    // Share in thousandths of a percent, so it prints without floats.
    let share = (outside * 100_000).checked_div(total).unwrap_or(0);
    push(
        if outside * 1000 > total {
            Level::Warn
        } else {
            Level::Ok
        },
        format!(
            "{}.{:03}% of {total} glyph chars are outside printable Latin-1 and General Punctuation ({outside})",
            share / 1000,
            share % 1000
        ),
    );

    let failed = r.pages.iter().filter(|p| p.error.is_some()).count();
    push(
        if failed == 0 { Level::Ok } else { Level::Fail },
        format!("{failed} failed pages"),
    );

    let mut flags = std::collections::BTreeMap::<&str, usize>::new();
    for f in r.pages.iter().flat_map(|p| &p.flags) {
        *flags.entry(f.as_str()).or_default() += 1;
    }
    if !flags.is_empty() {
        let list: Vec<String> = flags
            .iter()
            .map(|(f, n)| format!("{f} on {n} pages"))
            .collect();
        push(
            Level::Warn,
            format!("flagged constructs: {}", list.join(", ")),
        );
    }

    let odd: u32 = r.pages.iter().map(|p| p.odd).sum();
    let over: u32 = r.pages.iter().map(|p| p.overprint).sum();
    push(
        Level::Ok,
        format!("{over} overprinted duplicates removed, {odd} odd glyphs set aside"),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::PageExtract;
    use crate::geom::tests::word;

    fn doc() -> DocExtract {
        let mut glyphs = word("HELLO", 10800, 7200, 700, 1, 0);
        glyphs.extend(word("2.", 50400, 3600, 700, 2, 10));
        glyphs.extend(word("2.", 50400, 3600, 700, 3, 20));
        DocExtract {
            creator: Some("Final Draft 11".into()),
            producer: None,
            pages: vec![PageExtract {
                index: 1,
                media: Some([0, 0, 61200, 79200]),
                glyphs,
                flags: vec![],
                error: None,
            }],
        }
    }

    #[test]
    fn json_keys_are_sorted_and_output_is_stable() {
        let d = doc();
        let an = analyze(&d);
        let r = build(sha256_hex(b"x"), &d, &an);
        let a = to_json(&r);
        assert_eq!(a, to_json(&build(sha256_hex(b"x"), &d, &analyze(&d))));
        assert!(a.ends_with("}\n"));
        let top: Vec<usize> = [
            "\"meta\"",
            "\"pages\"",
            "\"pdf_sha256\"",
            "\"pitch_x\"",
            "\"schema\"",
            "\"tool\"",
        ]
        .iter()
        .map(|k| a.find(k).unwrap())
        .collect();
        assert!(top.windows(2).all(|w| w[0] < w[1]), "{a}");
        let seg: Vec<usize> = [
            "\"cells\"",
            "\"multi_run\"",
            "\"text\"",
            "\"trail\"",
            "\"x0\"",
            "\"x1\"",
            "\"y\"",
        ]
        .iter()
        .map(|k| a.find(k).unwrap())
        .collect();
        assert!(seg.windows(2).all(|w| w[0] < w[1]), "{a}");
        let back: RefFile = serde_json::from_str(&a).unwrap();
        assert_eq!(back, r);
        assert_eq!(r.pitch_x, Some(700));
        assert_eq!(r.pages[0].overprint, 2);
        assert_eq!(r.pages[0].segments.len(), 2);
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn checks_pass_on_a_letter_page_and_flag_problems() {
        let d = doc();
        let an = analyze(&d);
        let r = build(String::new(), &d, &an);
        let c = checks(&r, &an);
        assert!(c.iter().all(|c| c.level == Level::Ok), "{c:?}");
        let mut bad = d.clone();
        bad.creator = None;
        bad.pages[0].media = Some([0, 0, 59500, 84200]);
        bad.pages[0]
            .glyphs
            .push(crate::geom::tests::g("\u{4e00}", 0, 0, 9, 99));
        bad.pages[0].flags = vec!["form-xobject".into()];
        let an = analyze(&bad);
        let r = build(String::new(), &bad, &an);
        let c = checks(&r, &an);
        let levels: Vec<Level> = c.iter().map(|c| c.level).collect();
        assert_eq!(
            levels,
            [
                Level::Warn,
                Level::Fail,
                Level::Ok,
                Level::Warn,
                Level::Ok,
                Level::Warn,
                Level::Ok
            ]
        );
        assert!(
            is_expected_char('é') && is_expected_char('\u{2014}') && !is_expected_char('\u{301}')
        );
    }
}
