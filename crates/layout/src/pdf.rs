//! PDF output (layout.md §12): byte-identical on every OS.
//!
//! Each face used is embedded as a TrueType subset in a CID font with
//! identity encoding. Every glyph is declared 600/1000 em wide (marks 0),
//! so at 12 pt glyphs land on the 7.2 pt grid without spacing adjustments;
//! a run of text ends wherever its glyphs' advance would leave the grid.
//! Nothing in the file depends on the clock, a path, the locale or
//! hash-map order.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use miniz_oxide::deflate::compress_to_vec_zlib;
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Settings, Str, TextStr};
use subsetter::GlyphRemapper;

use coldopen_screenplay::{PageFormat, Pt};

use crate::Layout;
use crate::fonts::Fonts;
use crate::page::{Coverage, Page, resolve_cluster};
use crate::text::Look;

/// zlib level for every stream. Changing it changes every PDF golden.
const LEVEL: u8 = 6;
/// Declared width of every glyph with an advance, in 1/1000 em
/// (layout.md §12).
const GLYPH_WIDTH: i32 = 600;
const SYSTEM_INFO: SystemInfo = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

/// Export choices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfOptions {
    /// Text printed diagonally across every page, under the script.
    pub watermark: Option<String>,
    /// The document title for the PDF's metadata.
    pub title: Option<String>,
    /// The author slot of each tracked change a cell names (layout.md §17):
    /// slot `n` prints in `CHANGE_COLORS[n % 6]`. A change not listed
    /// takes slot 0.
    pub change_slots: BTreeMap<u64, u8>,
    /// Colored revised pages (layout.md §18): the paper color, as RGB, of
    /// each revision set a page's header can name. A page whose set is
    /// not listed, or that names none, stays white.
    pub revision_fills: BTreeMap<u64, (u8, u8, u8)>,
}

/// The print colors of tracked changes' authors, as RGB in 0..=255
/// (layout.md §17).
pub const CHANGE_COLORS: [(u8, u8, u8); 6] = [
    (0x00, 0x72, 0xB2),
    (0xD5, 0x5E, 0x00),
    (0x00, 0x9E, 0x73),
    (0xCC, 0x79, 0xA7),
    (0x3C, 0x3C, 0x99),
    (0x8F, 0x5A, 0x00),
];

/// Something the export could not print as written.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PdfWarning {
    /// `missing_glyph` (printed as a box) or `missing_mark` (left out).
    pub code: &'static str,
    /// The characters concerned, sorted, without repeats.
    pub chars: String,
}

/// A written PDF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfOutput {
    pub bytes: Vec<u8>,
    pub pages: usize,
    pub warnings: Vec<PdfWarning>,
}

/// The PDF could not be written. Only damaged bundled fonts cause this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfError(pub String);

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PdfError {}

/// FNV-1a, 64 bits, from a given offset basis. Used for the document id and
/// subset tags because it is fixed forever, unlike std's hashers.
fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = seed;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

const FNV_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// Points as the f32 pdf-writer takes. Exact: |v| < 2^24 hundredths.
fn f(v: Pt) -> f32 {
    v.0 as f32 / 100.0
}

/// A run of glyphs in one face and one color, starting at a position.
struct Run {
    face: usize,
    x: Pt,
    y: Pt,
    glyphs: Vec<u16>,
    underline: Option<(Pt, Pt)>,
    /// A strikeout over the same cells, for a pending deletion.
    strike: Option<(Pt, Pt)>,
    /// The tracked change whose author's color the run prints in.
    change: Option<u64>,
}

/// What every page draws, and what the fonts must hold.
struct Collected {
    pages: Vec<Vec<Run>>,
    /// Per face: old glyph id -> the character it shows (lowest wins).
    used: [BTreeMap<u16, char>; 4],
    missing: BTreeSet<char>,
    missing_marks: BTreeSet<char>,
}

fn collect(all: &[&Page], format: &PageFormat, fonts: &Fonts) -> Collected {
    let mut c = Collected {
        pages: Vec::new(),
        used: Default::default(),
        missing: BTreeSet::new(),
        missing_marks: BTreeSet::new(),
    };
    let pitch = format.pitch;
    let no_spacing = char_spacing(format) == 0;
    for page in all {
        let mut runs: Vec<Run> = Vec::new();
        for line in &page.lines {
            let y = Pt(format.page_height.0 - line.y.0);
            let mut cur: Option<Run> = None;
            for (text, cell, cl) in line.cluster_iter() {
                let face = Fonts::index(cl.look);
                let x = Pt(line.x.0 + cell as i32 * pitch.0);
                let mut glyphs: Vec<u16> = Vec::new();
                match resolve_cluster(text, cl.look, fonts) {
                    None => {
                        let lack: Vec<char> = lacking(text, cl.look, fonts).collect();
                        if lack.is_empty() {
                            // Not expected: resolve_cluster gives up only
                            // on one of these characters.
                            c.missing.extend(text.chars().take(1));
                        }
                        c.missing.extend(lack);
                        glyphs.extend(std::iter::repeat_n(0, usize::from(cl.cells)));
                        c.used[face].entry(0).or_insert('\u{FFFD}');
                    }
                    Some(chars) => {
                        for ch in text.chars() {
                            if !chars.iter().any(|(k, _)| *k == ch) {
                                c.missing_marks.insert(ch);
                            }
                        }
                        for (ch, _) in chars {
                            if fonts.draws_nothing(ch, cl.look) {
                                continue;
                            }
                            let gid = fonts.face(cl.look).glyph_index(ch).map_or(0, |g| g.0);
                            let e = c.used[face].entry(gid).or_insert(ch);
                            if ch < *e {
                                *e = ch;
                            }
                            glyphs.push(gid);
                        }
                    }
                }
                let ul = cl.look.underline;
                let st = cl.look.strike;
                let width = Pt(i32::from(cl.cells) * pitch.0);
                let continues = cur.as_ref().is_some_and(|r| {
                    r.face == face
                        && r.underline.is_some() == ul
                        && r.strike.is_some() == st
                        && r.change == cl.look.change
                });
                if !continues {
                    if let Some(r) = cur.take() {
                        runs.push(r);
                    }
                    cur = Some(Run {
                        face,
                        x,
                        y,
                        glyphs: Vec::new(),
                        underline: ul.then_some((x, Pt::ZERO)),
                        strike: st.then_some((x, Pt::ZERO)),
                        change: cl.look.change,
                    });
                }
                if let Some(r) = cur.as_mut() {
                    r.glyphs.extend(glyphs.iter().copied());
                    if let Some((_, w)) = r.underline.as_mut() {
                        *w = Pt(w.0 + width.0);
                    }
                    if let Some((_, w)) = r.strike.as_mut() {
                        *w = Pt(w.0 + width.0);
                    }
                }
                // A glyph with an advance moves one pitch, a zero-width one
                // only by the character spacing. When that is not the
                // cluster's cells, the next cluster starts a new run at its
                // own grid position (layout.md §12).
                let wide = glyphs
                    .iter()
                    .filter(|g| !fonts.zero_advance(face, **g))
                    .count();
                let marks = glyphs.len() - wide;
                if (wide != usize::from(cl.cells) || (marks > 0 && !no_spacing))
                    && let Some(r) = cur.take()
                {
                    runs.push(r);
                }
            }
            if let Some(r) = cur.take() {
                runs.push(r);
            }
        }
        c.pages.push(runs);
    }
    c
}

/// The characters of a cluster that print as boxes because the face lacks
/// them: the first, or a later one with a width (layout.md §12). A missing
/// zero-width character is a mark, which is left out, never the cause.
fn lacking<'a>(text: &'a str, look: Look, fonts: &'a Fonts) -> impl Iterator<Item = char> + 'a {
    text.chars().enumerate().filter_map(move |(i, ch)| {
        let wide = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) > 0;
        ((i == 0 || wide) && !fonts.has_glyph(ch, look)).then_some(ch)
    })
}

/// The character spacing `Tc` that makes each glyph advance one pitch, in
/// 1/100000 pt: the pitch minus 0.6 em. Zero at 12 pt with a 7.2 pt pitch.
fn char_spacing(format: &PageFormat) -> i64 {
    i64::from(format.pitch.0) * 1000 - i64::from(format.font_size.0) * i64::from(GLYPH_WIDTH)
}

/// Six capital letters from a hash, for a subset font's name.
fn subset_tag(name: &str, gids: &[u16]) -> String {
    let mut bytes = name.as_bytes().to_vec();
    for g in gids {
        bytes.extend_from_slice(&g.to_be_bytes());
    }
    let mut h = fnv1a64(FNV_BASIS, &bytes);
    let mut s = String::with_capacity(6);
    for _ in 0..6 {
        s.push(char::from(b'A' + (h % 26) as u8));
        h /= 26;
    }
    s
}

struct Refs(i32);

impl Refs {
    fn next(&mut self) -> Ref {
        self.0 += 1;
        Ref::new(self.0)
    }
}

fn z(b: &[u8]) -> Vec<u8> {
    compress_to_vec_zlib(b, LEVEL)
}

/// Writes the title page (if any) and body pages of `layout` as a PDF.
pub fn write(layout: &Layout, fonts: &Fonts, options: &PdfOptions) -> Result<PdfOutput, PdfError> {
    let format = layout.format();
    let all: Vec<&Page> = layout.title.iter().chain(layout.pages.iter()).collect();
    let mut collected = collect(&all, format, fonts);
    // A PDF needs at least one page: with nothing to print, it is one blank
    // page, still with the watermark (layout.md §12).
    if collected.pages.is_empty() {
        collected.pages.push(Vec::new());
    }
    let watermark = options
        .watermark
        .as_deref()
        .map(str::trim)
        .filter(|w| !w.is_empty());
    // The watermark draws in the regular face, so its glyphs join that
    // face's subset, and what it lacks joins the warning.
    let wm = watermark.map(|w| watermark_text(w, format, fonts));
    if let Some(wm) = &wm {
        for (gid, ch) in &wm.glyphs {
            let e = collected.used[0].entry(*gid).or_insert(*ch);
            if *ch < *e {
                *e = *ch;
            }
        }
        collected.missing.extend(wm.missing.iter().copied());
        collected
            .missing_marks
            .extend(wm.missing_marks.iter().copied());
    }

    let mut refs = Refs(0);
    let catalog = refs.next();
    let tree = refs.next();
    let info = refs.next();
    let mut pdf = Pdf::with_settings(Settings { pretty: false });
    pdf.set_version(1, 7);
    pdf.catalog(catalog).pages(tree);

    // Fonts: one Type0 font per face used.
    let size = f(format.font_size);
    let mut font_refs: [Option<Ref>; 4] = [None; 4];
    let mut remaps: [Option<GlyphRemapper>; 4] = Default::default();
    for (i, used) in collected.used.iter().enumerate() {
        if used.is_empty() {
            continue;
        }
        let src = coldopen_fonts::FACES[i];
        let face = &fonts.faces()[i];
        let gids: Vec<u16> = used.keys().copied().collect();
        let remap = GlyphRemapper::new_from_glyphs_sorted(&gids);
        let sub = subsetter::subset(src.data, 0, &remap)
            .map_err(|e| PdfError(format!("{}: could not subset: {e:?}", src.postscript_name)))?;
        let base = format!(
            "{}+{}",
            subset_tag(src.postscript_name, &gids),
            src.postscript_name
        );
        let (t0, cid, fd, file, tu) = (
            refs.next(),
            refs.next(),
            refs.next(),
            refs.next(),
            refs.next(),
        );
        let upem = f32::from(face.units_per_em());
        let em = |v: i16| f32::from(v) * 1000.0 / upem;

        pdf.type0_font(t0)
            .base_font(Name(base.as_bytes()))
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid)
            .to_unicode(tu);
        let mut cf = pdf.cid_font(cid);
        cf.subtype(CidFontType::Type2)
            .base_font(Name(base.as_bytes()))
            .system_info(SYSTEM_INFO)
            .font_descriptor(fd)
            .default_width(GLYPH_WIDTH as f32)
            .cid_to_gid_map_predefined(Name(b"Identity"));
        let zero_width: Vec<u16> = used
            .keys()
            .filter(|old| fonts.zero_advance(i, **old))
            .filter_map(|old| remap.get(*old))
            .collect();
        if !zero_width.is_empty() {
            let mut w = cf.widths();
            for new in zero_width {
                w.consecutive(new, [0.0]);
            }
        }
        cf.finish();
        let bb = face.global_bounding_box();
        let mut flags = FontFlags::FIXED_PITCH | FontFlags::SERIF | FontFlags::NON_SYMBOLIC;
        if src.italic {
            flags |= FontFlags::ITALIC;
        }
        pdf.font_descriptor(fd)
            .name(Name(base.as_bytes()))
            .flags(flags)
            .bbox(Rect::new(
                em(bb.x_min),
                em(bb.y_min),
                em(bb.x_max),
                em(bb.y_max),
            ))
            .italic_angle(face.italic_angle())
            .ascent(em(face.ascender()))
            .descent(em(face.descender()))
            .cap_height(em(face.capital_height().unwrap_or(face.ascender())))
            .stem_v(if src.bold { 120.0 } else { 80.0 })
            .font_file2(file);
        pdf.stream(file, &z(&sub))
            .filter(Filter::FlateDecode)
            .pair(Name(b"Length1"), sub.len() as i32);
        let mut cmap = UnicodeCmap::new(Name(b"Adobe-Identity-UCS"), SYSTEM_INFO);
        for (old, ch) in used {
            if let Some(new) = remap.get(*old) {
                cmap.pair(new, *ch);
            }
        }
        pdf.cmap(tu, &z(&cmap.finish())).filter(Filter::FlateDecode);
        font_refs[i] = Some(t0);
        remaps[i] = Some(remap);
    }

    let gs = wm.as_ref().map(|_| refs.next());
    if let Some(gs) = gs {
        pdf.ext_graphics(gs).non_stroking_alpha(0.2);
    }

    let mut page_refs = Vec::new();
    for (i, runs) in collected.pages.iter().enumerate() {
        let page = refs.next();
        let content = refs.next();
        page_refs.push(page);
        let mut c = Content::new();
        // A colored revised page: the fill goes under everything else.
        if let Some(&(r, g, b)) = all
            .get(i)
            .and_then(|p| p.revision)
            .and_then(|set| options.revision_fills.get(&set))
        {
            c.save_state();
            c.set_fill_rgb(
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            );
            c.rect(0.0, 0.0, f(format.page_width), f(format.page_height));
            c.fill_nonzero();
            c.restore_state();
        }
        if let (Some(_), Some(wm)) = (gs, wm.as_ref())
            && let Some(remap) = remaps[0].as_ref()
        {
            let k = std::f32::consts::FRAC_1_SQRT_2;
            c.save_state();
            c.set_parameters(Name(b"GS1"));
            c.set_fill_gray(0.5);
            c.begin_text();
            c.set_font(Name(b"F0"), wm.size);
            c.set_text_matrix([k, k, -k, k, wm.origin.0, wm.origin.1]);
            let codes: Vec<u8> = wm
                .glyphs
                .iter()
                .flat_map(|(g, _)| remap.get(*g).unwrap_or(0).to_be_bytes())
                .collect();
            c.show(Str(&codes));
            c.end_text();
            c.restore_state();
        }
        let spacing = char_spacing(format);
        for r in runs {
            // An author's color, for a tracked change; black otherwise.
            let color = r.change.map(|c| {
                let slot = options.change_slots.get(&c).copied().unwrap_or(0);
                let (cr, cg, cb) = CHANGE_COLORS[usize::from(slot) % CHANGE_COLORS.len()];
                [cr, cg, cb].map(|v| f32::from(v) / 255.0)
            });
            if let Some([cr, cg, cb]) = color {
                c.save_state();
                c.set_fill_rgb(cr, cg, cb);
            }
            // A run can hold no glyph (a lone soft hyphen) but still be
            // underlined.
            if !r.glyphs.is_empty() {
                let Some(remap) = remaps[r.face].as_ref() else {
                    continue;
                };
                let codes: Vec<u8> = r
                    .glyphs
                    .iter()
                    .flat_map(|g| remap.get(*g).unwrap_or(0).to_be_bytes())
                    .collect();
                c.begin_text();
                c.set_font(Name(face_name(r.face)), size);
                if spacing != 0 {
                    c.set_char_spacing(spacing as f32 / 100_000.0);
                }
                c.set_text_matrix([1.0, 0.0, 0.0, 1.0, f(r.x), f(r.y)]);
                c.show(Str(&codes));
                c.end_text();
            }
            if let Some((x, w)) = r.underline {
                let face = &fonts.faces()[r.face];
                let upem = f32::from(face.units_per_em());
                let (pos, thick) = face.underline_metrics().map_or((-100.0, 50.0), |m| {
                    (f32::from(m.position), f32::from(m.thickness))
                });
                let top = f(r.y) + size * pos / upem;
                let t = size * thick / upem;
                c.rect(f(x), top - t, f(w), t);
                c.fill_nonzero();
            }
            if let Some((x, w)) = r.strike {
                let face = &fonts.faces()[r.face];
                let upem = f32::from(face.units_per_em());
                let em = f32::from(face.units_per_em());
                let (pos, thick) = face.strikeout_metrics().map_or((0.3 * em, 0.05 * em), |m| {
                    (f32::from(m.position), f32::from(m.thickness))
                });
                let t = size * thick / upem;
                let bottom = f(r.y) + size * pos / upem;
                c.rect(f(x), bottom, f(w), t);
                c.fill_nonzero();
            }
            if color.is_some() {
                c.restore_state();
            }
        }
        pdf.stream(content, &z(&c.finish()))
            .filter(Filter::FlateDecode);
        let mut p = pdf.page(page);
        p.media_box(Rect::new(
            0.0,
            0.0,
            f(format.page_width),
            f(format.page_height),
        ))
        .parent(tree)
        .contents(content);
        {
            let mut res = p.resources();
            {
                let mut fonts_dict = res.fonts();
                for (i, r) in font_refs.iter().enumerate() {
                    if let Some(r) = r {
                        fonts_dict.pair(Name(face_name(i)), *r);
                    }
                }
            }
            if let Some(gs) = gs {
                res.ext_g_states().pair(Name(b"GS1"), gs);
            }
        }
        p.finish();
    }
    pdf.pages(tree)
        .kids(page_refs.iter().copied())
        .count(page_refs.len() as i32);
    {
        let mut di = pdf.document_info(info);
        di.producer(TextStr("Cold Open"));
        if let Some(t) = options.title.as_deref().filter(|t| !t.trim().is_empty()) {
            di.title(TextStr(t));
        }
    }
    let body = pdf.as_bytes();
    let mut id = fnv1a64(FNV_BASIS, body).to_be_bytes().to_vec();
    id.extend_from_slice(&fnv1a64(FNV_BASIS ^ 0x5eed, body).to_be_bytes());
    pdf.set_file_id((id.clone(), id));

    let mut warnings = Vec::new();
    for (code, set) in [
        ("missing_glyph", &collected.missing),
        ("missing_mark", &collected.missing_marks),
    ] {
        if !set.is_empty() {
            warnings.push(PdfWarning {
                code,
                chars: set.iter().collect(),
            });
        }
    }
    Ok(PdfOutput {
        bytes: pdf.finish(),
        pages: collected.pages.len(),
        warnings,
    })
}

fn face_name(i: usize) -> &'static [u8] {
    match i {
        0 => b"F0",
        1 => b"F1",
        2 => b"F2",
        _ => b"F3",
    }
}

/// The watermark as drawn (layout.md §12).
struct Watermark {
    /// Glyph ids in the regular face, with the character each shows.
    glyphs: Vec<(u16, char)>,
    /// Font size in points.
    size: f32,
    /// Text origin, so that its centre sits at the page centre after a 45°
    /// rotation.
    origin: (f32, f32),
    /// Characters the face lacks; they print as boxes.
    missing: BTreeSet<char>,
    /// Marks the face lacks after the first character; they are left out.
    missing_marks: BTreeSet<char>,
}

/// Lays out the watermark text: a glyph per character the face has, a box
/// for one it lacks, and nothing for a control character or a character
/// that draws nothing in the script either (the soft hyphen).
fn watermark_text(text: &str, format: &PageFormat, fonts: &Fonts) -> Watermark {
    let look = Look::default();
    let face = fonts.face(look);
    let mut glyphs = Vec::new();
    let mut missing = BTreeSet::new();
    let mut missing_marks = BTreeSet::new();
    for ch in text.chars() {
        if ch.is_control() || fonts.draws_nothing(ch, look) {
            continue;
        }
        let zero_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) == 0;
        if fonts.has_glyph(ch, look) {
            glyphs.push((face.glyph_index(ch).map_or(0, |g| g.0), ch));
        } else if zero_width && !glyphs.is_empty() {
            // As in the script: a mark the face lacks is left out.
            missing_marks.insert(ch);
        } else {
            glyphs.push((0, '\u{FFFD}'));
            missing.insert(ch);
        }
    }
    let shorter = format.page_width.0.min(format.page_height.0);
    let size = watermark_size(glyphs.len(), shorter) as f32;
    let k = std::f32::consts::FRAC_1_SQRT_2;
    let w = glyphs.len().max(1) as f32 * size * GLYPH_WIDTH as f32 / 1000.0;
    let h = size * 0.6;
    let cx = f(format.page_width) / 2.0;
    let cy = f(format.page_height) / 2.0;
    let origin = (
        cx - k * (w / 2.0) + k * (h / 2.0),
        cy - k * (w / 2.0) - k * (h / 2.0),
    );
    Watermark {
        glyphs,
        size,
        origin,
        missing,
        missing_marks,
    }
}

/// The largest whole size, from 1 to 96 pt, at which `glyphs` cells span at
/// most 80% of the shorter page side (given in hundredths of a point):
/// `cells * size * 0.6 <= 0.8 * shorter`. In 64 bits with a saturating
/// product, so no length overflows.
fn watermark_size(glyphs: usize, shorter: i32) -> i32 {
    let cells = i64::try_from(glyphs).unwrap_or(i64::MAX).max(1);
    let size = 4 * i64::from(shorter) / cells.saturating_mul(300);
    i32::try_from(size.clamp(1, 96)).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watermark_size_fits_and_never_overflows() {
        // US Letter's shorter side is 612 pt: 5 cells fit at 96 pt, and
        // 0.8 * 612 / (0.6 * 20) = 40.8, so 20 cells get 40 pt.
        assert_eq!(watermark_size(5, 61_200), 96);
        assert_eq!(watermark_size(20, 61_200), 40);
        assert_eq!(watermark_size(0, 61_200), 96);
        // 3 * cells * 100 in i32 overflowed from 7 158 279 cells.
        for n in [7_158_279, i32::MAX as usize, usize::MAX] {
            assert_eq!(watermark_size(n, 61_200), 1);
            assert!((1..=96).contains(&watermark_size(n, i32::MAX)));
        }
        assert_eq!(watermark_size(usize::MAX, i32::MAX), 1);
    }
}
