//! The phase 2 reference comparison (docs/specs/layout.md §13).
//!
//! For every `corpus/**/<stem>.pdf` with a sibling `<stem>.fdx` and a
//! `<stem>.ref.json` written by tools/refpdf, this lays out the .fdx with
//! its own page format and compares the pages with the PDF. Pairs under a
//! folder named `fd11` are report-only; all others are the gate.
//!
//! The corpus is private and never committed, so the test skips when it is
//! missing, and skips a pair whose `.ref.json` is missing or out of date,
//! naming the refpdf command that writes it. `COLDOPEN_REQUIRE_CORPUS=1`
//! turns those skips into failures. Reports go
//! to `target/tmp/coldopen-reference/` and hold no script text unless
//! `COLDOPEN_REFERENCE_EXCERPTS=1` is set.
#![allow(clippy::disallowed_methods)]

mod align;
mod refdata;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use coldopen_layout::{Cursor, Engine, Layout, Options, Rules, ValidFormat};
use coldopen_screenplay::{ElementKind, PageFormat, Pt};
use sha2::{Digest, Sha256};

use align::{Ch, align, fold_str};
use refdata::{Context, RefFile};

/// Format values the reference PDFs pin down, fitted on Big Fish and
/// Brick & Steel and frozen before the gate runs (layout.md §13, §14).
const GATE_PROFILE: &str = "measure-7.0-strict+margin+overhang";

/// A calibration candidate: a change to the format read from the .fdx.
struct Profile {
    name: &'static str,
    apply: fn(&mut PageFormat),
}

/// Sets every kind's cells to the largest n with `n * pitch < width`
/// (`strict`) or `<= width`.
fn measure(f: &mut PageFormat, pitch: i32, strict: bool) {
    for k in ElementKind::ALL {
        if let Some(e) = f.elements.get_mut(k) {
            let w = e.right.0 - e.left.0;
            let n = if strict { (w - 1) / pitch } else { w / pitch };
            e.cells = u16::try_from(n.max(1)).ok();
        }
    }
}

fn cells_at(f: &mut PageFormat, pitch: i32) {
    for k in ElementKind::ALL {
        if let Some(e) = f.elements.get_mut(k) {
            let w = (e.right.0 - e.left.0).max(pitch);
            e.cells = u16::try_from(w / pitch).ok();
        }
    }
}

const PROFILES: &[Profile] = &[
    Profile {
        name: "as-read",
        apply: |_| {},
    },
    Profile {
        name: "measure-7.0-strict",
        apply: |f| measure(f, 700, true),
    },
    Profile {
        name: "measure-7.2-strict",
        apply: |f| measure(f, 720, true),
    },
    Profile {
        name: "measure-7.0-strict+inserts-in-margin",
        apply: |f| {
            measure(f, 700, true);
            f.more_contd_in_margin = true;
        },
    },
    Profile {
        name: "measure-7.0-strict+margin+overhang",
        apply: |f| {
            measure(f, 700, true);
            f.more_contd_in_margin = true;
            f.hyphen_overhang = true;
        },
    },
    Profile {
        name: "measure-7.0-strict+margin+line-ends",
        apply: |f| {
            measure(f, 700, true);
            f.more_contd_in_margin = true;
            f.split_mode = coldopen_screenplay::SplitMode::LineEnd;
        },
    },
    Profile {
        name: "cells-at-7.0",
        apply: |f| cells_at(f, 700),
    },
    Profile {
        name: "inserts-in-margin",
        apply: |f| f.more_contd_in_margin = true,
    },
    Profile {
        name: "split-at-line-ends",
        apply: |f| f.split_mode = coldopen_screenplay::SplitMode::LineEnd,
    },
    Profile {
        name: "no-hyphen-breaks",
        apply: |f| f.hyphen_breaks = false,
    },
    Profile {
        name: "cells-at-7.0+inserts-in-margin",
        apply: |f| {
            cells_at(f, 700);
            f.more_contd_in_margin = true;
        },
    },
    Profile {
        name: "cells-at-7.0+inserts-in-margin+line-ends",
        apply: |f| {
            cells_at(f, 700);
            f.more_contd_in_margin = true;
            f.split_mode = coldopen_screenplay::SplitMode::LineEnd;
        },
    },
];

fn profile(name: &str) -> &'static Profile {
    PROFILES
        .iter()
        .find(|p| p.name == name)
        .unwrap_or(&PROFILES[0])
}

fn corpus_dir() -> PathBuf {
    match std::env::var_os("COLDOPEN_CORPUS") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
    }
}

fn excerpts() -> bool {
    std::env::var("COLDOPEN_REFERENCE_EXCERPTS").as_deref() == Ok("1")
}

#[derive(Clone, Debug)]
struct Pair {
    stem: String,
    set: &'static str,
    pdf: PathBuf,
    fdx: PathBuf,
    refjson: PathBuf,
}

fn discover(dir: &Path, out: &mut Vec<Pair>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            discover(&p, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            let fdx = p.with_extension("fdx");
            if !fdx.exists() {
                continue;
            }
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            let set = if p.components().any(|c| c.as_os_str() == "fd11") {
                "fd11"
            } else {
                "reference"
            };
            out.push(Pair {
                refjson: p.with_file_name(format!("{stem}.ref.json")),
                stem,
                set,
                pdf: p,
                fdx,
            });
        }
    }
}

type FlowAt = (usize, u32);

/// An unmatched stretch: our element, its kind, our characters, reference
/// characters, and excerpts when they are allowed.
type Gap = (
    Option<usize>,
    String,
    usize,
    usize,
    Option<(String, String)>,
);

/// An element left out of M1 because reference lines of it are off the grid
/// (layout.md §13). Its text is still checked: every character on those
/// lines must be matched in our text, in order, on the same body page.
#[derive(Debug, Clone)]
struct OffGrid {
    element: Option<usize>,
    kind: String,
    lines: usize,
    ref_pages: BTreeSet<usize>,
    our_pages: BTreeSet<usize>,
    /// Narrowest and widest glyph advance on its off-grid lines, 1/100 pt.
    advance: (i64, i64),
    text_ok: bool,
    order_ok: bool,
    page_ok: bool,
    /// It landed on the page after the reference's, which is allowed only
    /// where the reference page's line spacing is itself off the grid.
    next_page_allowed: bool,
}

impl OffGrid {
    fn ok(&self) -> bool {
        self.text_ok && self.order_ok && self.page_ok
    }
}

/// A break mismatch under resync.
#[derive(Debug, Clone)]
struct BreakMiss {
    ref_page: usize,
    delta: i64,
    cause: String,
    cut_kind: String,
    excerpt: Option<String>,
}

#[derive(Debug, Default)]
struct Eval {
    stem: String,
    set: String,
    profile: String,
    error: Option<String>,
    creator: String,
    pdf_sha: String,
    ms: f64,
    ref_pages: usize,
    title_pages: usize,
    our_pages: usize,
    our_title_pages: usize,
    /// How the reference's title pages were counted.
    title_source: &'static str,
    flow_coverage: f64,
    ref_coverage: f64,
    decorations: BTreeMap<String, usize>,
    wrap_total: usize,
    wrap_same: usize,
    wrap_split: usize,
    /// Elements left out of M1 because a reference line of theirs is off
    /// the grid.
    wrap_off_grid: usize,
    wrap_by_kind: BTreeMap<String, (usize, usize)>,
    wrap_misses: Vec<(usize, String, Vec<usize>, Vec<usize>)>,
    breaks_total: usize,
    breaks_exact: usize,
    breaks_unmeasurable: usize,
    /// Of those, breaks next to a page with off-grid reference lines.
    breaks_off_grid: usize,
    /// Every element with off-grid reference lines.
    off_grid: Vec<OffGrid>,
    /// Body pages whose break was not measured because they hold off-grid
    /// lines.
    off_grid_break_pages: Vec<usize>,
    /// The reference's glyph pitch, 1/100 pt.
    pitch_x: i64,
    /// Body pages whose printed line spacing is off the grid.
    spacing_off_grid_pages: Vec<usize>,
    break_misses: Vec<BreakMiss>,
    scenes_total: usize,
    scenes_exact: usize,
    scenes_within1: usize,
    scene_drift: Vec<(usize, usize, usize)>,
    fdx_scene_pages: (usize, usize),
    gaps: usize,
    /// Unmatched stretches.
    gap_list: Vec<Gap>,
    /// The first body page whose free-run start differs from the
    /// reference: (page, reference start line, our start line, kinds).
    first_divergence: Option<(usize, i64, i64, String)>,
}

impl Eval {
    fn wrap_rate(&self) -> f64 {
        ratio(self.wrap_same, self.wrap_total)
    }
    fn break_rate(&self) -> f64 {
        ratio(self.breaks_exact, self.breaks_total)
    }
    fn max_delta(&self) -> i64 {
        self.break_misses
            .iter()
            .map(|m| m.delta.abs())
            .max()
            .unwrap_or(0)
    }
    fn scene_rate(&self) -> f64 {
        ratio(self.scenes_exact, self.scenes_total)
    }
    fn body_pages_ref(&self) -> usize {
        self.ref_pages.saturating_sub(self.title_pages)
    }
}

fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 { 1.0 } else { a as f64 / b as f64 }
}

fn sha256(bytes: &[u8]) -> String {
    let mut s = String::new();
    for b in Sha256::digest(bytes) {
        let _ = write!(s, "{b:02x}");
    }
    s
}

struct Built {
    engine: Engine,
    layout: Layout,
    ms: f64,
}

fn build(
    doc: &coldopen_fdx::FdxDoc,
    prof: &Profile,
    tweak: impl Fn(&mut PageFormat),
) -> Result<Built, String> {
    let mut f = doc.format.clone();
    (prof.apply)(&mut f);
    tweak(&mut f);
    let vf = ValidFormat::new(f).map_err(|p| format!("invalid format: {p:?}"))?;
    let options = Options {
        overrides: doc.overrides.clone(),
        title_lines: Some(doc.title_lines.clone()),
        ..Options::default()
    };
    let mut best = f64::MAX;
    let mut out = None;
    for _ in 0..3 {
        let t = Instant::now();
        let engine = Engine::new(&doc.script, &vf, &options);
        let layout = engine.run();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        out = Some((engine, layout));
    }
    let (engine, layout) = out.ok_or("no layout")?;
    Ok(Built {
        engine,
        layout,
        ms: best,
    })
}

/// The gate profile's layout with some elements' line starts forced, for
/// the off-grid text check (layout.md §13).
fn build_with_breaks(
    doc: &coldopen_fdx::FdxDoc,
    prof: &Profile,
    line_breaks: BTreeMap<usize, Vec<u32>>,
) -> Result<coldopen_layout::Layout, String> {
    let mut f = doc.format.clone();
    (prof.apply)(&mut f);
    let vf = ValidFormat::new(f).map_err(|p| format!("invalid format: {p:?}"))?;
    let options = Options {
        overrides: doc.overrides.clone(),
        title_lines: Some(doc.title_lines.clone()),
        line_breaks,
        ..Options::default()
    };
    Ok(Engine::new(&doc.script, &vf, &options).run())
}

fn evaluate(pair: &Pair, prof: &Profile) -> Eval {
    let mut ev = Eval {
        stem: pair.stem.clone(),
        set: pair.set.to_string(),
        profile: prof.name.to_string(),
        ..Eval::default()
    };
    if let Err(e) = evaluate_into(pair, prof, &mut ev) {
        ev.error = Some(e);
    }
    ev
}

/// The command that writes a pair's `.ref.json`.
fn extract_hint(pair: &Pair) -> String {
    format!(
        "run `cargo run --release --manifest-path tools/refpdf/Cargo.toml -- extract {}`",
        pair.pdf.display()
    )
}

/// Whether a pair's `.ref.json` is there and was extracted from its PDF as
/// it is now. A missing or stale one is a setup step, not a layout result,
/// so the test skips that pair and says how to write it.
fn ref_status(pair: &Pair) -> Result<(), String> {
    let Ok(rj) = fs::read_to_string(&pair.refjson) else {
        return Err(format!("{} is missing", pair.refjson.display()));
    };
    let pdf = fs::read(&pair.pdf).map_err(|e| format!("{}: {e}", pair.pdf.display()))?;
    let sha = serde_json::from_str::<serde_json::Value>(&rj)
        .ok()
        .and_then(|v| v["pdf_sha256"].as_str().map(str::to_string));
    if sha.as_deref() != Some(sha256(&pdf).as_str()) {
        return Err(format!(
            "{} was not extracted from {} as it is now",
            pair.refjson.display(),
            pair.pdf.display()
        ));
    }
    Ok(())
}

fn evaluate_into(pair: &Pair, prof: &Profile, ev: &mut Eval) -> Result<(), String> {
    let pdf = fs::read(&pair.pdf).map_err(|e| e.to_string())?;
    let rj = fs::read_to_string(&pair.refjson).map_err(|_| {
        format!(
            "{} is missing; {}",
            pair.refjson.display(),
            extract_hint(pair)
        )
    })?;
    let r: RefFile = serde_json::from_str(&rj).map_err(|e| format!("ref.json: {e}"))?;
    if r.schema != 1 {
        return Err(format!(
            "ref.json schema {} (tool {}) is not 1",
            r.schema, r.tool
        ));
    }
    ev.pdf_sha = sha256(&pdf);
    if ev.pdf_sha != r.pdf_sha256 {
        return Err(
            "ref.json is stale: its pdf_sha256 does not match the PDF; extract again".into(),
        );
    }
    if r.meta.pages as usize != r.pages.len() {
        return Err("ref.json page count does not match its pages".into());
    }
    if let Some(p) = r.pages.iter().find(|p| p.error.is_some()) {
        return Err(format!("refpdf could not read page {}", p.index));
    }
    ev.creator = format!(
        "{} / {}",
        r.meta.creator.clone().unwrap_or_default(),
        r.meta.producer.clone().unwrap_or_default()
    );
    let flagged: BTreeSet<&String> = r.pages.iter().flat_map(|p| p.flags.iter()).collect();
    if !flagged.is_empty() {
        ev.decorations.insert(format!("flags {flagged:?}"), 0);
    }
    let doc = coldopen_fdx::read(&fs::read(&pair.fdx).map_err(|e| e.to_string())?)
        .map_err(|e| format!("fdx: {e}"))?;
    let b = build(&doc, prof, |_| {})?;
    ev.ms = b.ms;
    let fmt = b.engine.format().clone();
    let lh = i64::from(fmt.line_height.0);

    // Reference side.
    let has_title = doc
        .title_lines
        .iter()
        .any(|l| !l.text.plain_text().trim().is_empty());
    ev.ref_pages = r.pages.len();
    // Printed page numbers say where the body starts; a title that ran on
    // to further pages is then counted correctly (layout.md §9.3).
    let labelled = refdata::title_pages_from_labels(
        &r.pages,
        i64::from(fmt.page_width.0),
        i64::from(fmt.margin_top.0),
    );
    (ev.title_pages, ev.title_source) = match labelled {
        Some(n) => (n.min(r.pages.len()), "page labels"),
        None => (usize::from(has_title).min(r.pages.len()), "title text"),
    };
    ev.our_pages = b.layout.pages.len();
    ev.our_title_pages = b.layout.title.len();
    let body: Vec<&refdata::RefPage> = r.pages.iter().skip(ev.title_pages).collect();
    let ctx = Context {
        more: doc.format.more.clone(),
        contd: doc.format.contd.clone(),
        scene_top: doc
            .format
            .scene_continueds
            .as_ref()
            .and_then(|s| s.top.clone()),
        scene_bottom: doc
            .format
            .scene_continueds
            .as_ref()
            .and_then(|s| s.bottom.clone()),
        page_width: i64::from(fmt.page_width.0),
        margin_top: i64::from(fmt.margin_top.0),
        body_left: i64::from(fmt.elements.action.left.0),
        body_right: i64::from(fmt.elements.action.right.0),
        pitch: r.pitch_x.unwrap_or(720),
    };
    ev.pitch_x = r.pitch_x.unwrap_or(720);
    let classified = refdata::classify(&body, &ctx);
    for l in &classified {
        *ev.decorations.entry(l.kind.name().to_string()).or_insert(0) += 1;
    }
    let lines = refdata::reading_order(&classified, lh);

    // Flow side: every printing element's text.
    let mut flow: Vec<Ch<FlowAt>> = Vec::new();
    for (e, el) in doc.script.elements.iter().enumerate() {
        if el.kind.is_printing() {
            let text = el.text.plain_text();
            let mut chars = Vec::new();
            fold_str(&text, |i| (e, i as u32), &mut chars);
            // The reference side drops a continued marker from names (the
            // automatic ones are not in our text), so a marker typed into
            // the name is dropped here too. Folding skips spaces and case,
            // so the kept part is the folded length of what remains.
            if el.kind == coldopen_screenplay::ElementKind::Character
                && let Some(rest) = refdata::strip_contd(&text, doc.format.contd.as_deref())
            {
                let mut kept = Vec::new();
                fold_str(&rest, |_| (e, 0), &mut kept);
                chars.truncate(kept.len());
            }
            flow.extend(chars);
        }
    }
    let mut refs: Vec<Ch<usize>> = Vec::new();
    for (li, l) in lines.iter().enumerate() {
        fold_str(&l.text, |_| li, &mut refs);
    }
    let a = align(&flow, &refs);
    ev.flow_coverage = ratio(a.flow_matched, flow.len());
    ev.ref_coverage = ratio(a.ref_matched, refs.len());
    ev.gaps = a.gaps.len();
    for &(fi, fl, rj, rl) in &a.gaps {
        let el = flow.get(fi).map(|c| c.at.0);
        let kind = el
            .and_then(|e| doc.script.elements.get(e))
            .map_or_else(|| "-".to_string(), |e| format!("{:?}", e.kind));
        let ex = excerpts().then(|| {
            let take = |v: &mut dyn Iterator<Item = char>| v.take(30).collect::<String>();
            (
                take(&mut flow.iter().skip(fi).take(fl).map(|c| c.c)),
                take(&mut refs.iter().skip(rj).take(rl).map(|c| c.c)),
            )
        });
        ev.gap_list.push((el, kind, fl, rl, ex));
    }
    let mut flow_to_ref = vec![None; flow.len()];
    for (j, m) in a.ref_to_flow.iter().enumerate() {
        if let Some(i) = m {
            flow_to_ref[*i] = Some(j);
        }
    }
    // Each reference line's first matched flow character.
    let mut line_start: Vec<Option<usize>> = vec![None; lines.len()];
    for (j, m) in a.ref_to_flow.iter().enumerate() {
        if let Some(i) = m {
            let li = refs[j].at;
            if line_start[li].is_none_or(|s| *i < s) {
                line_start[li] = Some(*i);
            }
        }
    }
    let flow_idx = |at: FlowAt| flow.partition_point(|c| c.at < at);
    let pages = body.len();
    let mut page_start: Vec<Option<usize>> = vec![None; pages];
    for (li, l) in lines.iter().enumerate() {
        if let Some(s) = line_start[li]
            && page_start[l.page].is_none_or(|p| s < p)
        {
            page_start[l.page] = Some(s);
        }
    }

    // Reference lines printed off the grid (layout.md §13): their elements
    // and pages cannot be measured on a fixed-pitch grid, so wrapping and
    // the breaks from those pages are skipped. Their text is still checked:
    // every character matched in our text, in order, on the same body page
    // of our free run.
    let mut off_grid_pages: BTreeSet<usize> = BTreeSet::new();
    let mut off_grid_elems: BTreeSet<usize> = BTreeSet::new();
    for (li, l) in lines.iter().enumerate() {
        if l.off_grid {
            off_grid_pages.insert(l.page);
            if let Some(s) = line_start[li] {
                off_grid_elems.insert(flow[s].at.0);
            }
        }
    }
    // Wrapping is the one thing not compared, so the check lays those
    // elements out with the line breaks the reference printed (decided
    // 2026-10-14); everything else keeps our own wrapping.
    let mut printed: BTreeMap<usize, Vec<u32>> =
        off_grid_elems.iter().map(|&e| (e, Vec::new())).collect();
    for s in line_start.iter().flatten() {
        let (e, byte) = flow[*s].at;
        if let Some(v) = printed.get_mut(&e) {
            v.push(byte);
        }
    }
    for v in printed.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    let printed_layout = if printed.is_empty() {
        None
    } else {
        Some(build_with_breaks(&doc, prof, printed)?)
    };
    let check_layout = printed_layout.as_ref().unwrap_or(&b.layout);
    let mut our_page = vec![None; flow.len()];
    for (n, page) in check_layout.pages.iter().enumerate() {
        for l in &page.lines {
            if let Some((e, s0, s1)) = l.source {
                for slot in our_page
                    .iter_mut()
                    .take(flow_idx((e, s1)))
                    .skip(flow_idx((e, s0)))
                {
                    *slot = Some(n + 1);
                }
            }
        }
    }
    // Reference pages whose content baselines are not a whole number of
    // lines apart (within half a point): their line spacing is off the grid.
    // Segments under four characters are left out, so a raised superscript
    // does not count.
    let mut baselines: BTreeMap<usize, BTreeSet<i64>> = BTreeMap::new();
    for l in lines.iter().filter(|l| l.text.trim().chars().count() >= 4) {
        baselines.entry(l.page).or_default().insert(l.y);
    }
    let spacing_off_grid: BTreeSet<usize> = baselines
        .iter()
        .filter(|(_, ys)| {
            ys.iter().zip(ys.iter().skip(1)).any(|(a, b)| {
                let r = (b - a).rem_euclid(lh.max(1));
                r > 50 && r < lh - 50
            })
        })
        .map(|(p, _)| *p)
        .collect();
    ev.spacing_off_grid_pages = spacing_off_grid.iter().map(|p| p + 1).collect();
    let mut ref_chars_of_line: Vec<Vec<usize>> = vec![Vec::new(); lines.len()];
    for (j, c) in refs.iter().enumerate() {
        ref_chars_of_line[c.at].push(j);
    }
    let mut by_elem: BTreeMap<Option<usize>, OffGrid> = BTreeMap::new();
    let mut last_flow: Option<usize> = None;
    for (li, l) in lines.iter().enumerate() {
        if !l.off_grid {
            continue;
        }
        let element = line_start[li].map(|s| flow[s].at.0);
        let og = by_elem.entry(element).or_insert_with(|| OffGrid {
            element,
            kind: element
                .and_then(|e| doc.script.elements.get(e))
                .map_or_else(|| "-".to_string(), |e| format!("{:?}", e.kind)),
            lines: 0,
            ref_pages: BTreeSet::new(),
            our_pages: BTreeSet::new(),
            advance: (i64::MAX, i64::MIN),
            text_ok: true,
            order_ok: true,
            page_ok: true,
            next_page_allowed: false,
        });
        og.lines += 1;
        og.ref_pages.insert(l.page + 1);
        if let Some(adv) = l.advance {
            og.advance = (og.advance.0.min(adv), og.advance.1.max(adv));
        }
        for &j in &ref_chars_of_line[li] {
            let Some(i) = a.ref_to_flow[j] else {
                og.text_ok = false;
                continue;
            };
            if last_flow.is_some_and(|p| i <= p) {
                og.order_ok = false;
            }
            last_flow = Some(i);
            match our_page[i] {
                Some(n) => {
                    og.our_pages.insert(n);
                    let (ok, allowed) =
                        off_grid_page_ok(n, l.page + 1, spacing_off_grid.contains(&l.page));
                    og.page_ok &= ok;
                    og.next_page_allowed |= allowed;
                }
                None => og.page_ok = false,
            }
        }
    }
    ev.off_grid = by_elem.into_values().collect();

    // M1: wraps.
    let mut ref_lines_of: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for (li, l) in lines.iter().enumerate() {
        if let Some(s) = line_start[li] {
            ref_lines_of
                .entry(flow[s].at.0)
                .or_default()
                .push((s, l.page));
        }
    }
    // Our wrap of each element whole, independent of where pages break.
    let mut our_lines_of: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for e in ref_lines_of.keys() {
        let v = our_lines_of.entry(*e).or_default();
        for a0 in b.engine.line_starts(*e) {
            let i = flow_idx((*e, a0));
            if i < flow.len() && flow[i].at.0 == *e {
                v.push(i);
            }
        }
    }
    let mut fully_matched: BTreeMap<usize, bool> = BTreeMap::new();
    for (i, c) in flow.iter().enumerate() {
        let ok = flow_to_ref[i].is_some();
        let v = fully_matched.entry(c.at.0).or_insert(true);
        *v &= ok;
    }
    let mut wrap_bad: BTreeSet<usize> = BTreeSet::new();
    for (e, rl) in &ref_lines_of {
        let pages_of: BTreeSet<usize> = rl.iter().map(|x| x.1).collect();
        if pages_of.len() > 1 {
            ev.wrap_split += 1;
            continue;
        }
        if off_grid_elems.contains(e) {
            ev.wrap_off_grid += 1;
            continue;
        }
        if !fully_matched.get(e).copied().unwrap_or(false) {
            continue;
        }
        let mut rs: Vec<usize> = rl.iter().map(|x| x.0).collect();
        rs.sort_unstable();
        rs.dedup();
        let mut os = our_lines_of.get(e).cloned().unwrap_or_default();
        os.sort_unstable();
        os.dedup();
        let kind = format!("{:?}", doc.script.elements[*e].kind);
        let entry = ev.wrap_by_kind.entry(kind.clone()).or_insert((0, 0));
        entry.1 += 1;
        ev.wrap_total += 1;
        if rs.get(1..) == os.get(1..) {
            ev.wrap_same += 1;
            entry.0 += 1;
        } else {
            wrap_bad.insert(*e);
            let lens = |v: &[usize], end: usize| {
                let mut out = Vec::new();
                for w in v.windows(2) {
                    out.push(w[1] - w[0]);
                }
                if let Some(&last) = v.last() {
                    out.push(end.saturating_sub(last));
                }
                out
            };
            let end = flow.partition_point(|c| c.at.0 <= *e);
            ev.wrap_misses
                .push((*e, kind, lens(&rs, end), lens(&os, end)));
        }
    }

    // Local debugging only: print one element's lines on both sides.
    if excerpts()
        && std::env::var("COLDOPEN_REFERENCE_DEBUG_STEM").is_ok_and(|s| s == pair.stem)
        && let Ok(Ok(dbg)) = std::env::var("COLDOPEN_REFERENCE_DEBUG").map(|v| v.parse::<usize>())
        && let Some(el) = doc.script.elements.get(dbg)
    {
        eprintln!("element {dbg} {:?}: {:?}", el.kind, el.text.plain_text());
        for (li, l) in lines.iter().enumerate() {
            if line_start[li].is_some_and(|s| flow[s].at.0 == dbg) {
                let seg = body[l.page]
                    .segments
                    .iter()
                    .find(|s| s.y == l.y && s.x0 == l.x0);
                eprintln!(
                    "  ref  p{} x0={} cells={:?} trail={:?} {:?}",
                    l.page + 1,
                    l.x0,
                    seg.map(|s| s.cells),
                    seg.map(|s| s.trail),
                    l.text
                );
            }
        }
        for page in &b.layout.pages {
            for l in &page.lines {
                if l.source.is_some_and(|s| s.0 == dbg) {
                    eprintln!("  ours p{:?} cells={} {:?}", page.number, l.cells(), l.text);
                }
            }
        }
    }

    // M2: resynced breaks.
    let mut starts: Vec<usize> = line_start.iter().flatten().copied().collect();
    starts.sort_unstable();
    // Index of the reference line containing flow character i.
    let line_of = |i: usize| -> usize { starts.partition_point(|&s| s <= i).saturating_sub(1) };
    type Variant<'a> = (&'static str, Box<dyn Fn() -> Result<Built, String> + 'a>);
    let variants: Vec<Variant> = vec![
        (
            "sentence rule",
            Box::new(|| {
                build(&doc, prof, |_| {}).map(|b| Built {
                    engine: b.engine.with_rules(Rules {
                        sentences: false,
                        ..Rules::default()
                    }),
                    ..b
                })
            }),
        ),
        (
            "inserts in margin",
            Box::new(|| {
                build(&doc, prof, |f| {
                    f.more_contd_in_margin = !f.more_contd_in_margin
                })
            }),
        ),
        (
            "one more line per page",
            Box::new(|| {
                build(&doc, prof, |f| {
                    f.margin_bottom = Pt(f.margin_bottom.0 - f.line_height.0)
                })
            }),
        ),
        (
            "one less line per page",
            Box::new(|| {
                build(&doc, prof, |f| {
                    f.margin_bottom = Pt(f.margin_bottom.0 + f.line_height.0)
                })
            }),
        ),
        (
            "split mode",
            Box::new(|| {
                build(&doc, prof, |f| {
                    f.split_mode = match f.split_mode {
                        coldopen_screenplay::SplitMode::MidLine => {
                            coldopen_screenplay::SplitMode::LineEnd
                        }
                        coldopen_screenplay::SplitMode::LineEnd => {
                            coldopen_screenplay::SplitMode::MidLine
                        }
                    }
                })
            }),
        ),
        (
            "heading keep",
            Box::new(|| {
                build(&doc, prof, |_| {}).map(|b| Built {
                    engine: b.engine.with_rules(Rules {
                        heading: false,
                        ..Rules::default()
                    }),
                    ..b
                })
            }),
        ),
        (
            "name keep",
            Box::new(|| {
                build(&doc, prof, |_| {}).map(|b| Built {
                    engine: b.engine.with_rules(Rules {
                        name: false,
                        ..Rules::default()
                    }),
                    ..b
                })
            }),
        ),
        (
            "min lines 1",
            Box::new(|| {
                build(&doc, prof, |_| {}).map(|b| Built {
                    engine: b.engine.with_rules(Rules {
                        min_lines: 1,
                        ..Rules::default()
                    }),
                    ..b
                })
            }),
        ),
        (
            "min lines 3",
            Box::new(|| {
                build(&doc, prof, |_| {}).map(|b| Built {
                    engine: b.engine.with_rules(Rules {
                        min_lines: 3,
                        ..Rules::default()
                    }),
                    ..b
                })
            }),
        ),
        (
            "parenthetical rule",
            Box::new(|| {
                build(&doc, prof, |_| {}).map(|b| Built {
                    engine: b.engine.with_rules(Rules {
                        parenthetical: false,
                        ..Rules::default()
                    }),
                    ..b
                })
            }),
        ),
        (
            "transition rule",
            Box::new(|| {
                build(&doc, prof, |_| {}).map(|b| Built {
                    engine: b.engine.with_rules(Rules {
                        transition: false,
                        ..Rules::default()
                    }),
                    ..b
                })
            }),
        ),
    ];
    let mut built_variants: Vec<(&str, Option<Built>)> = Vec::new();
    for p in 0..pages.saturating_sub(1) {
        let (Some(s), Some(t)) = (page_start[p], page_start[p + 1]) else {
            ev.breaks_unmeasurable += 1;
            continue;
        };
        // Where a page ends depends on the lines on it; a break from a page
        // printed partly off the grid is not a fixed-pitch measurement.
        if off_grid_pages.contains(&p) {
            ev.breaks_unmeasurable += 1;
            ev.breaks_off_grid += 1;
            ev.off_grid_break_pages.push(p + 1);
            continue;
        }
        ev.breaks_total += 1;
        let cur = |eng: &Engine, i: usize| -> Option<Cursor> {
            eng.cursor_at(flow[i].at.0, flow[i].at.1)
        };
        let (Some(c), Some(want)) = (cur(&b.engine, s), cur(&b.engine, t)) else {
            ev.breaks_unmeasurable += 1;
            ev.breaks_total -= 1;
            continue;
        };
        let got = b.engine.page_from(c).next;
        if got == want {
            ev.breaks_exact += 1;
            continue;
        }
        let got_flow = b.engine.source_of(got).map_or(flow.len(), flow_idx);
        let delta = line_of(got_flow) as i64 - line_of(t) as i64;
        // Likely cause: a wrap difference on this page, else the first
        // single change that makes the break match.
        let touched: BTreeSet<usize> = (s..t.min(flow.len())).map(|i| flow[i].at.0).collect();
        let mut cause = if touched.iter().any(|e| wrap_bad.contains(e)) {
            "wrap".to_string()
        } else {
            String::new()
        };
        if cause.is_empty() {
            if built_variants.is_empty() {
                for (name, f) in &variants {
                    built_variants.push((name, f().ok()));
                }
            }
            for (name, v) in &built_variants {
                if let Some(v) = v
                    && let (Some(c2), Some(w2)) = (cur(&v.engine, s), cur(&v.engine, t))
                    && v.engine.page_from(c2).next == w2
                {
                    cause = (*name).to_string();
                    break;
                }
            }
        }
        if cause.is_empty() {
            cause = "unknown".into();
        }
        let cut_kind = format!("{:?}", doc.script.elements[flow[t].at.0].kind);
        let excerpt = excerpts().then(|| {
            let e = flow[t].at.0;
            doc.script.elements[e]
                .text
                .plain_text()
                .chars()
                .take(30)
                .collect()
        });
        ev.break_misses.push(BreakMiss {
            ref_page: p + 1,
            delta,
            cause,
            cut_kind,
            excerpt,
        });
    }

    // Where the free run first starts a page somewhere else.
    for (k, page) in b.layout.pages.iter().enumerate() {
        let Some(Some(theirs)) = page_start.get(k).copied() else {
            break;
        };
        let ours = b.engine.source_of(page.start).map_or(flow.len(), flow_idx);
        if line_of(ours) != line_of(theirs) {
            let kind = |i: usize| {
                flow.get(i).map_or("end".to_string(), |c| {
                    format!("{:?}", doc.script.elements[c.at.0].kind)
                })
            };
            ev.first_divergence = Some((
                k + 1,
                line_of(theirs) as i64,
                line_of(ours) as i64,
                format!(
                    "reference starts at {}, ours at {}",
                    kind(theirs),
                    kind(ours)
                ),
            ));
            break;
        }
    }

    // M4: scene starts.
    let mut line_page = vec![0usize; lines.len()];
    for (li, l) in lines.iter().enumerate() {
        line_page[li] = l.page;
    }
    let mut ordinal = 0usize;
    for (e, el) in doc.script.elements.iter().enumerate() {
        if el.kind != ElementKind::SceneHeading {
            continue;
        }
        ordinal += 1;
        let i = flow_idx((e, 0));
        if i >= flow.len() || flow[i].at.0 != e {
            continue;
        }
        let (Some(j), Some(ours)) = (flow_to_ref[i], b.layout.first_page(e)) else {
            continue;
        };
        let theirs = line_page[refs[j].at] + 1;
        ev.scenes_total += 1;
        let ours = ours as usize;
        if ours == theirs {
            ev.scenes_exact += 1;
        }
        if ours.abs_diff(theirs) <= 1 {
            ev.scenes_within1 += 1;
        }
        if ours != theirs {
            ev.scene_drift.push((ordinal, theirs, ours));
        }
        if let Some(sp) = doc.scenes.iter().find(|s| s.element == e)
            && let Some(label) = &sp.page
        {
            let n: String = label.chars().take_while(char::is_ascii_digit).collect();
            ev.fdx_scene_pages.1 += 1;
            if n.parse::<usize>().ok() == Some(theirs) {
                ev.fdx_scene_pages.0 += 1;
            }
        }
    }
    Ok(())
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
}

/// Allowed page-count difference for a gate script.
fn page_tolerance(stem: &str) -> usize {
    let s = stem.to_lowercase();
    usize::from(s.contains("big") && s.contains("fish"))
}

/// The gate's verdict for one evaluation (layout.md §13).
fn verdict(ev: &Eval) -> Vec<String> {
    let mut fails = Vec::new();
    if let Some(e) = &ev.error {
        fails.push(format!("error: {e}"));
        return fails;
    }
    if ev.flow_coverage < 0.995 || ev.ref_coverage < 0.995 {
        fails.push(format!(
            "PAIR MISMATCH: alignment coverage {} / {} is under 99.5%",
            pct(ev.flow_coverage),
            pct(ev.ref_coverage)
        ));
        return fails;
    }
    if ev.wrap_rate() < 0.99 {
        fails.push(format!("M1 wraps {} < 99%", pct(ev.wrap_rate())));
    }
    if ev.break_rate() < 0.95 {
        fails.push(format!("M2 resynced breaks {} < 95%", pct(ev.break_rate())));
    }
    if ev.max_delta() > 2 {
        fails.push(format!("M2 a break is off by {} lines", ev.max_delta()));
    }
    let diff = ev.our_pages.abs_diff(ev.body_pages_ref());
    if diff > page_tolerance(&ev.stem) {
        fails.push(format!(
            "M3 body pages {} vs {}",
            ev.our_pages,
            ev.body_pages_ref()
        ));
    }
    if ev.our_title_pages != ev.title_pages {
        fails.push(format!(
            "M3 title pages {} vs {}",
            ev.our_title_pages, ev.title_pages
        ));
    }
    if ev.scene_rate() < 0.90 {
        fails.push(format!("M4 scene starts {} < 90%", pct(ev.scene_rate())));
    }
    if ev.scenes_within1 < ev.scenes_total {
        fails.push("M4 a scene starts more than one page away".into());
    }
    // The cap is part of the gate, not a note in the report.
    if ev.breaks_unmeasurable * 50 > ev.breaks_total.max(1) {
        fails.push(format!(
            "{} breaks could not be measured (over 2%)",
            ev.breaks_unmeasurable
        ));
    }
    for og in ev.off_grid.iter().filter(|og| !og.ok()) {
        fails.push(format!(
            "off-grid element {}: {}",
            og.element
                .map_or_else(|| "?".to_string(), |e| e.to_string()),
            off_grid_check(og)
        ));
    }
    fails
}

/// Whether an off-grid character on our body page `ours` counts as on the
/// reference's page `theirs` (layout.md §13): the same page, or the next one
/// when the reference page's line spacing is off the grid. The second value
/// says the allowance was used.
fn off_grid_page_ok(ours: usize, theirs: usize, spacing_off_grid: bool) -> (bool, bool) {
    if ours == theirs {
        (true, false)
    } else if spacing_off_grid && ours == theirs + 1 {
        (true, true)
    } else {
        (false, false)
    }
}

/// What the text check found for an off-grid element.
fn off_grid_check(og: &OffGrid) -> String {
    let mut bad = Vec::new();
    if !og.text_ok {
        bad.push("text missing on our side");
    }
    if !og.order_ok {
        bad.push("text out of order");
    }
    if !og.page_ok {
        bad.push("text on another page");
    }
    if bad.is_empty() && og.next_page_allowed {
        "same text, same order; next page, allowed because the reference page's line spacing is off the grid".to_string()
    } else if bad.is_empty() {
        "same text, same order, same page".to_string()
    } else {
        bad.join(", ")
    }
}

fn pages_list(s: &BTreeSet<usize>) -> String {
    s.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Hundredths of a point as points with two decimals.
fn pt2(v: i64) -> String {
    format!("{}.{:02}", v / 100, (v % 100).abs())
}

/// The off-grid table for a report (layout.md §13): every excluded element
/// with its script, pages and reason, and every unmeasured break.
fn off_grid_md(ev: &Eval, s: &mut String) {
    let _ = writeln!(
        s,
        "## Off-grid exclusions ({} elements, {} breaks)\n\n| Script | Element | Kind | Reference page | Our page | Off-grid lines | Reason | Text check |\n| --- | --- | --- | --- | --- | --- | --- | --- |",
        ev.off_grid.len(),
        ev.off_grid_break_pages.len()
    );
    for og in &ev.off_grid {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} | printed at {}–{} pt per character on a {} pt grid; wrapping not compared, text checked with the printed line breaks | {} |",
            ev.stem,
            og.element
                .map_or_else(|| "?".to_string(), |e| e.to_string()),
            og.kind,
            pages_list(&og.ref_pages),
            pages_list(&og.our_pages),
            og.lines,
            pt2(og.advance.0),
            pt2(og.advance.1),
            pt2(ev.pitch_x),
            off_grid_check(og)
        );
    }
    if !ev.spacing_off_grid_pages.is_empty() {
        let _ = writeln!(
            s,
            "\nBody pages of {} whose printed line spacing is off the grid: {}.",
            ev.stem,
            ev.spacing_off_grid_pages
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let _ = writeln!(
        s,
        "\n| Script | Break from body page | Reason |\n| --- | --- | --- |"
    );
    for p in &ev.off_grid_break_pages {
        let _ = writeln!(
            s,
            "| {} | {p} | the page holds off-grid lines; counted as unmeasurable |",
            ev.stem
        );
    }
    let _ = writeln!(s);
}

fn report_md(ev: &Eval) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# {} ({}, profile `{}`)\n", ev.stem, ev.set, ev.profile);
    if let Some(e) = &ev.error {
        let _ = writeln!(s, "**Error:** {e}\n");
        return s;
    }
    let v = verdict(ev);
    let _ = writeln!(
        s,
        "- PDF sha256 `{}…`, made by {}",
        &ev.pdf_sha[..12.min(ev.pdf_sha.len())],
        ev.creator
    );
    let _ = writeln!(s, "- Layout time (best of 3): {:.2} ms", ev.ms);
    let _ = writeln!(
        s,
        "- Verdict: {}\n",
        if v.is_empty() {
            "PASS".to_string()
        } else {
            format!("FAIL — {}", v.join("; "))
        }
    );
    let _ = writeln!(s, "| Metric | Value | Threshold |\n| --- | --- | --- |");
    let _ = writeln!(
        s,
        "| Alignment coverage (ours / reference) | {} / {} | ≥ 99.5% |",
        pct(ev.flow_coverage),
        pct(ev.ref_coverage)
    );
    let _ = writeln!(
        s,
        "| M1 wraps identical | {} ({}/{}; {} split elements scored in M2, {} off the grid left out) | ≥ 99% |",
        pct(ev.wrap_rate()),
        ev.wrap_same,
        ev.wrap_total,
        ev.wrap_split,
        ev.wrap_off_grid
    );
    let _ = writeln!(
        s,
        "| M2 resynced breaks exact | {} ({}/{}, max Δ {} lines, {} unmeasurable, {} of them beside off-grid lines) | ≥ 95%, all within 2; ≤ 2% unmeasurable |",
        pct(ev.break_rate()),
        ev.breaks_exact,
        ev.breaks_total,
        ev.max_delta(),
        ev.breaks_unmeasurable,
        ev.breaks_off_grid
    );
    let _ = writeln!(
        s,
        "| M3 body pages (ours / reference) | {} / {} (title pages: {} / {}, from {}) | {} |",
        ev.our_pages,
        ev.body_pages_ref(),
        ev.our_title_pages,
        ev.title_pages,
        ev.title_source,
        if page_tolerance(&ev.stem) == 0 {
            "exact"
        } else {
            "±1"
        }
    );
    let _ = writeln!(
        s,
        "| M4 scene starts exact | {} ({}/{}, {} within 1) | ≥ 90%, all within 1 |",
        pct(ev.scene_rate()),
        ev.scenes_exact,
        ev.scenes_total,
        ev.scenes_within1
    );
    let _ = writeln!(
        s,
        "| .fdx recorded scene pages agreeing with the PDF | {}/{} | report only |\n",
        ev.fdx_scene_pages.0, ev.fdx_scene_pages.1
    );
    let _ = writeln!(
        s,
        "Reference lines by kind: {:?}; alignment gaps: {}\n",
        ev.decorations, ev.gaps
    );
    if let Some((p, r, o, k)) = &ev.first_divergence {
        let _ = writeln!(
            s,
            "First free-run divergence: body page {p} (reference line {r}, ours {o}; {k})\n"
        );
    }
    off_grid_md(ev, &mut s);
    let _ = writeln!(
        s,
        "## Alignment gaps ({}, first 50)\n\n| Our element | Kind | Our chars | Reference chars | Excerpts (ours / reference) |\n| --- | --- | --- | --- | --- |",
        ev.gap_list.len()
    );
    for (el, kind, fl, rl, ex) in ev.gap_list.iter().take(50) {
        let el = el.map_or_else(|| "-".to_string(), |e| e.to_string());
        let ex = ex
            .as_ref()
            .map_or_else(String::new, |(a, b)| format!("`{a}` / `{b}`"));
        let _ = writeln!(s, "| {el} | {kind} | {fl} | {rl} | {ex} |");
    }
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "## Wraps by kind\n\n| Kind | Identical | Total |\n| --- | --- | --- |"
    );
    for (k, (a, b)) in &ev.wrap_by_kind {
        let _ = writeln!(s, "| {k} | {a} | {b} |");
    }
    let mut causes: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &ev.break_misses {
        *causes.entry(m.cause.as_str()).or_insert(0) += 1;
    }
    let _ = writeln!(
        s,
        "\n## Break mismatches ({})\n\nCauses: {causes:?}\n",
        ev.break_misses.len()
    );
    let _ = writeln!(
        s,
        "| Ref page → next | Δ lines (+ = we fit more) | Next page starts with | Likely cause | Excerpt |\n| --- | --- | --- | --- | --- |"
    );
    for m in &ev.break_misses {
        let _ = writeln!(
            s,
            "| {} → {} | {:+} | {} | {} | {} |",
            m.ref_page,
            m.ref_page + 1,
            m.delta,
            m.cut_kind,
            m.cause,
            m.excerpt.clone().unwrap_or_default()
        );
    }
    let _ = writeln!(
        s,
        "\n## Wrap mismatches ({}, first 200)\n\n| Element | Kind | Reference line lengths | Our line lengths |\n| --- | --- | --- | --- |",
        ev.wrap_misses.len()
    );
    for (e, k, r, o) in ev.wrap_misses.iter().take(200) {
        let _ = writeln!(s, "| {e} | {k} | {r:?} | {o:?} |");
    }
    let _ = writeln!(
        s,
        "\n## Scene drift ({})\n\n| Scene | Reference page | Our page |\n| --- | --- | --- |",
        ev.scene_drift.len()
    );
    for (n, r, o) in &ev.scene_drift {
        let _ = writeln!(s, "| {n} | {r} | {o} |");
    }
    s
}

/// A passing evaluation to vary in the verdict tests.
fn passing_eval() -> Eval {
    Eval {
        stem: "brick-and-steel".into(),
        flow_coverage: 1.0,
        ref_coverage: 1.0,
        wrap_total: 100,
        wrap_same: 100,
        breaks_total: 100,
        breaks_exact: 100,
        ref_pages: 5,
        title_pages: 1,
        our_pages: 4,
        our_title_pages: 1,
        scenes_total: 10,
        scenes_exact: 10,
        scenes_within1: 10,
        ..Eval::default()
    }
}

#[test]
fn verdict_fails_over_the_unmeasurable_cap() {
    let mut ev = passing_eval();
    assert!(verdict(&ev).is_empty(), "{:?}", verdict(&ev));
    ev.breaks_unmeasurable = 2;
    assert!(verdict(&ev).is_empty(), "2 of 100 is within the cap");
    ev.breaks_unmeasurable = 3;
    let v = verdict(&ev);
    assert!(v.iter().any(|f| f.contains("over 2%")), "{v:?}");
}

#[test]
fn verdict_fails_when_off_grid_text_moves() {
    let og = OffGrid {
        element: Some(7),
        kind: "Dialogue".into(),
        lines: 2,
        ref_pages: BTreeSet::from([3]),
        our_pages: BTreeSet::from([3]),
        advance: (614, 640),
        text_ok: true,
        order_ok: true,
        page_ok: true,
        next_page_allowed: false,
    };
    let mut ev = passing_eval();
    ev.off_grid = vec![og.clone()];
    assert!(verdict(&ev).is_empty(), "{:?}", verdict(&ev));
    for broken in [
        OffGrid {
            text_ok: false,
            ..og.clone()
        },
        OffGrid {
            order_ok: false,
            ..og.clone()
        },
        OffGrid {
            page_ok: false,
            our_pages: BTreeSet::from([4]),
            ..og.clone()
        },
    ] {
        ev.off_grid = vec![broken];
        let v = verdict(&ev);
        assert!(
            v.iter().any(|f| f.starts_with("off-grid element 7")),
            "{v:?}"
        );
    }
}

#[test]
fn off_grid_pages_may_slip_one_page_only_where_spacing_is_off_grid() {
    assert_eq!(off_grid_page_ok(72, 72, false), (true, false));
    assert_eq!(off_grid_page_ok(73, 72, false), (false, false));
    assert_eq!(off_grid_page_ok(73, 72, true), (true, true));
    assert_eq!(off_grid_page_ok(74, 72, true), (false, false));
    assert_eq!(off_grid_page_ok(71, 72, true), (false, false));
}

#[test]
fn ref_status_says_when_reference_data_is_missing_or_stale() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ref-status");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let pair = Pair {
        stem: "x".into(),
        set: "reference",
        pdf: dir.join("x.pdf"),
        fdx: dir.join("x.fdx"),
        refjson: dir.join("x.ref.json"),
    };
    fs::write(&pair.pdf, b"%PDF-1.7 not really").unwrap();
    let e = ref_status(&pair).unwrap_err();
    assert!(e.contains("is missing"), "{e}");
    assert!(
        extract_hint(&pair).contains("-- extract "),
        "{}",
        extract_hint(&pair)
    );
    fs::write(&pair.refjson, r#"{"pdf_sha256": "0000"}"#).unwrap();
    let e = ref_status(&pair).unwrap_err();
    assert!(e.contains("was not extracted from"), "{e}");
    let sha = sha256(&fs::read(&pair.pdf).unwrap());
    fs::write(&pair.refjson, format!(r#"{{"pdf_sha256": "{sha}"}}"#)).unwrap();
    assert_eq!(ref_status(&pair), Ok(()));
}

#[test]
fn reference_comparison() {
    let dir = corpus_dir();
    let mut found = Vec::new();
    discover(&dir, &mut found);
    let require = std::env::var("COLDOPEN_REQUIRE_CORPUS").as_deref() == Ok("1");
    // Pairs without current reference data are skipped, with the command
    // that writes it (CLAUDE.md: corpus tests skip cleanly).
    let mut pairs = Vec::new();
    let mut not_ready = Vec::new();
    for p in found {
        match ref_status(&p) {
            Ok(()) => pairs.push(p),
            Err(why) => {
                let msg = format!("{}/{}: {why}; {}", p.set, p.stem, extract_hint(&p));
                eprintln!("skipping {msg}");
                if p.set == "reference" {
                    not_ready.push(msg);
                }
            }
        }
    }
    if require {
        assert!(
            not_ready.is_empty(),
            "COLDOPEN_REQUIRE_CORPUS=1 but reference data is not ready:\n{}",
            not_ready.join("\n")
        );
    }
    let gate: Vec<&Pair> = pairs.iter().filter(|p| p.set == "reference").collect();
    if require {
        assert!(
            gate.len() >= 3,
            "COLDOPEN_REQUIRE_CORPUS=1 but only {} reference pairs were found under {}",
            gate.len(),
            dir.display()
        );
    }
    if pairs.is_empty() {
        eprintln!(
            "skipping: no .pdf/.fdx pairs with current reference data under {}",
            dir.display()
        );
        return;
    }
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("coldopen-reference");
    fs::create_dir_all(&out).unwrap();
    let mut index = String::from("# Reference comparison\n\n");
    let mut failures = Vec::new();

    // The gate profile on every pair.
    let gp = profile(GATE_PROFILE);
    let _ = writeln!(
        index,
        "Gate profile: `{}`\n\n| Script | Set | Verdict | M1 wraps | M2 breaks (max Δ) | M3 pages (ours/ref) | M4 scenes | Layout ms |\n| --- | --- | --- | --- | --- | --- | --- | --- |",
        gp.name
    );
    let mut off_grid_index = String::new();
    for p in &pairs {
        let ev = evaluate(p, gp);
        let v = verdict(&ev);
        let md = report_md(&ev);
        if p.set == "reference" && (!ev.off_grid.is_empty() || !ev.off_grid_break_pages.is_empty())
        {
            off_grid_md(&ev, &mut off_grid_index);
        }
        fs::write(out.join(format!("{}-{}.md", p.set, p.stem)), &md).unwrap();
        let _ = writeln!(
            index,
            "| {} | {} | {} | {} | {} ({}) | {}/{} | {} | {:.2} |",
            p.stem,
            p.set,
            if v.is_empty() { "PASS" } else { "FAIL" },
            pct(ev.wrap_rate()),
            pct(ev.break_rate()),
            ev.max_delta(),
            ev.our_pages,
            ev.body_pages_ref(),
            pct(ev.scene_rate()),
            ev.ms
        );
        eprintln!("{}", md.lines().take(14).collect::<Vec<_>>().join("\n"));
        if p.set == "reference" && !v.is_empty() {
            failures.push(format!("{}: {}", p.stem, v.join("; ")));
        }
    }

    if !off_grid_index.is_empty() {
        let _ = writeln!(index, "\n{off_grid_index}");
    }

    // Calibration sweep, report only.
    let _ = writeln!(
        index,
        "\n## Calibration candidates (report only)\n\n| Profile | Script | M1 | M2 (max Δ) | M3 | M4 |\n| --- | --- | --- | --- | --- | --- |"
    );
    for prof in PROFILES {
        for p in &pairs {
            let ev = evaluate(p, prof);
            let _ = writeln!(
                index,
                "| {} | {} | {} | {} ({}) | {}/{} | {} |",
                prof.name,
                p.stem,
                pct(ev.wrap_rate()),
                pct(ev.break_rate()),
                ev.max_delta(),
                ev.our_pages,
                ev.body_pages_ref(),
                pct(ev.scene_rate())
            );
        }
    }
    fs::write(out.join("index.md"), &index).unwrap();
    eprintln!("{index}\nreports in {}", out.display());
    assert!(
        failures.is_empty(),
        "reference gate failed:\n{}",
        failures.join("\n")
    );
}
