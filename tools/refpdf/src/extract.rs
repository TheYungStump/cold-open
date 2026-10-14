//! Drives pdf-extract one page at a time and turns its callbacks into
//! positioned glyphs.
//!
//! pdf-extract 0.12.1 unwraps and panics in well over a hundred places, so
//! every call into it (and into lopdf) runs under `catch_unwind` with a quiet
//! panic hook, and a failed page is recorded instead of ending the run.
//! A panic cannot catch a stack overflow, so a preflight pass first rejects
//! the two kinds of input that make pdf-extract recurse forever (a cyclic
//! /Parent chain and a Form XObject that draws itself), and the work runs on
//! a thread with a large stack. The preflight also flags the constructs
//! pdf-extract is known to mishandle, so the output says when a page's
//! glyphs cannot be trusted.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;

use pdf_extract::content::{Content, Operation};
use pdf_extract::{
    Dictionary, Document, MediaBox, Object, ObjectId, OutputDev, OutputError, Transform,
};

use crate::geom::Glyph;
use crate::units::to_hpt;

/// Stack for the extraction thread. pdf-extract recurses into Form
/// XObjects, so deep but finite nesting needs room.
const WORKER_STACK: usize = 256 << 20;
/// Longest Parent chain and deepest Form XObject nesting we accept.
const MAX_PARENT_DEPTH: usize = 64;
const MAX_FORM_DEPTH: usize = 32;
/// Error messages are cut to this many characters.
const MAX_ERROR_CHARS: usize = 300;

/// Flags for constructs pdf-extract mishandles.
pub const FLAG_QUOTE: &str = "text-op-quote";
pub const FLAG_DOUBLE_QUOTE: &str = "text-op-double-quote";
pub const FLAG_FORM: &str = "form-xobject";
pub const FLAG_IMAGE: &str = "image-xobject";
pub const FLAG_XOBJECT_OTHER: &str = "xobject-other";
pub const FLAG_XOBJECT_MISSING: &str = "xobject-missing";
pub const FLAG_MEDIA_ORIGIN: &str = "mediabox-origin";
pub const FLAG_NOT_STRICT: &str = "content-not-strict";

/// Everything extracted from one PDF.
#[derive(Debug, Clone, Default)]
pub struct DocExtract {
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub pages: Vec<PageExtract>,
}

/// One physical page: its glyphs, or why there are none.
#[derive(Debug, Clone, Default)]
pub struct PageExtract {
    /// 1-based page index.
    pub index: u32,
    /// MediaBox [x0, y0, x1, y1] in hpt, as stored in the PDF.
    pub media: Option<[i64; 4]>,
    /// Every glyph pdf-extract reported, in drawing order.
    pub glyphs: Vec<Glyph>,
    /// Sorted preflight flags.
    pub flags: Vec<String>,
    /// Set when the page could not be extracted.
    pub error: Option<String>,
}

thread_local! {
    static QUIET: Cell<bool> = const { Cell::new(false) };
    static LAST_PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn install_quiet_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let default = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if QUIET.with(Cell::get) {
                let mut msg = payload_text(info.payload());
                if let Some(l) = info.location() {
                    msg.push_str(&format!(" at {}:{}", short_path(l.file()), l.line()));
                }
                LAST_PANIC.with(|p| *p.borrow_mut() = Some(msg));
            } else {
                default(info);
            }
        }));
    });
}

fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic with a non-text payload".to_string()
    }
}

/// Drops the machine-specific prefix of a source path, so messages are the
/// same on every machine: `.../registry/src/<index>/pdf-extract-0.12.1/src/lib.rs`
/// becomes `pdf-extract-0.12.1/src/lib.rs`.
fn short_path(file: &str) -> String {
    let file = file.replace('\\', "/");
    if let Some(i) = file.find("registry/src/") {
        let rest = &file[i + "registry/src/".len()..];
        if let Some(j) = rest.find('/') {
            return rest[j + 1..].to_string();
        }
    }
    if let Some(i) = file.find("library/") {
        return file[i..].to_string();
    }
    file
}

fn clip(mut s: String) -> String {
    if let Some((i, _)) = s.char_indices().nth(MAX_ERROR_CHARS) {
        s.truncate(i);
        s.push('…');
    }
    s
}

/// Runs `f`, turning a panic into `Err(message)` without printing anything.
pub fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    install_quiet_hook();
    let was = QUIET.with(|q| q.replace(true));
    LAST_PANIC.with(|p| p.borrow_mut().take());
    let r = panic::catch_unwind(AssertUnwindSafe(f));
    QUIET.with(|q| q.set(was));
    r.map_err(|payload| {
        let msg = LAST_PANIC
            .with(|p| p.borrow_mut().take())
            .unwrap_or_else(|| payload_text(&*payload));
        clip(msg)
    })
}

/// Collects glyphs from pdf-extract's callbacks.
struct Collector {
    media: Option<MediaBox>,
    run: u32,
    seq: u32,
    glyphs: Vec<Glyph>,
}

impl OutputDev for Collector {
    fn begin_page(
        &mut self,
        _: u32,
        media_box: &MediaBox,
        _: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), OutputError> {
        self.media = Some(*media_box);
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        text: &str,
    ) -> Result<(), OutputError> {
        let corner = self.media.map_or((0.0, 0.0), |m| (m.llx, m.ury));
        self.glyphs.push(make_glyph(
            trm, width, font_size, corner, text, self.run, self.seq,
        ));
        self.seq += 1;
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), OutputError> {
        // pdf-extract calls this once per string shown, which is our run.
        self.run += 1;
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), OutputError> {
        Ok(())
    }
}

/// Builds a glyph from pdf-extract's text rendering matrix. The matrix maps
/// glyph space (before the font size) to PDF user space, whose y axis points
/// up from the bottom of the page. `(llx, ury)` is the MediaBox's top-left
/// corner, which glyph positions are measured from.
fn make_glyph(
    trm: &Transform,
    width: f64,
    font_size: f64,
    (llx, ury): (f64, f64),
    text: &str,
    run: u32,
    seq: u32,
) -> Glyph {
    let (a, b, c, d) = (
        trm.m11 * font_size,
        trm.m12 * font_size,
        trm.m21 * font_size,
        trm.m22 * font_size,
    );
    let scale = a.abs().max(d.abs());
    let upright = a > 0.0 && d > 0.0 && b.abs() <= 1e-3 * scale && c.abs() <= 1e-3 * scale;
    let size = if upright {
        d
    } else {
        (a * d - b * c).abs().sqrt()
    };
    let x = to_hpt(trm.m31 - llx);
    let y = to_hpt(ury - trm.m32);
    match (x, y, to_hpt(size)) {
        (Some(x), Some(y), Some(size)) => Glyph {
            text: text.to_string(),
            x,
            y,
            size,
            upright,
            adv: to_hpt(width * a).unwrap_or(0),
            run,
            seq,
        },
        // Not finite: keep it so it is counted, but never as script text.
        _ => Glyph {
            text: text.to_string(),
            x: 0,
            y: 0,
            size: 0,
            upright: false,
            adv: 0,
            run,
            seq,
        },
    }
}

/// Result of checking a page before pdf-extract sees it.
#[derive(Debug, Default)]
struct Preflight {
    flags: BTreeSet<String>,
    media: Option<[i64; 4]>,
    fatal: Option<String>,
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> Option<(Option<ObjectId>, &'a Object)> {
    doc.dereference(o).ok()
}

fn number(doc: &Document, o: &Object) -> Option<f64> {
    match deref(doc, o)?.1 {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(f64::from(*r)),
        _ => None,
    }
}

/// Looks `key` up on the page and then up its /Parent chain, like
/// pdf-extract's `get_inherited` but with a depth limit.
fn inherited<'a>(doc: &'a Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut dict = page;
    for _ in 0..=MAX_PARENT_DEPTH {
        if let Ok(v) = dict.get(key) {
            return deref(doc, v).map(|(_, o)| o);
        }
        let parent = dict.get(b"Parent").ok()?.as_reference().ok()?;
        dict = doc.get_dictionary(parent).ok()?;
    }
    None
}

/// Fails when following /Parent from the page loops or runs too deep;
/// pdf-extract would recurse until the stack overflows.
fn check_parents(doc: &Document, page_id: ObjectId, page: &Dictionary) -> Result<(), String> {
    let mut seen = BTreeSet::from([page_id]);
    let mut dict = page;
    for _ in 0..MAX_PARENT_DEPTH {
        let Some(parent) = dict.get(b"Parent").ok().and_then(|p| p.as_reference().ok()) else {
            return Ok(());
        };
        if !seen.insert(parent) {
            return Err("cyclic /Parent chain".into());
        }
        let Ok(d) = doc.get_dictionary(parent) else {
            return Ok(());
        };
        dict = d;
    }
    Err("/Parent chain too deep".into())
}

fn decode_ops(data: &[u8], flags: &mut BTreeSet<String>) -> Result<Vec<Operation>, String> {
    match Content::decode_strict(data) {
        Ok(c) => Ok(c.operations),
        Err(_) => {
            flags.insert(FLAG_NOT_STRICT.into());
            Content::decode(data)
                .map(|c| c.operations)
                .map_err(|_| "content stream does not parse".to_string())
        }
    }
}

/// Walks the content operators, recursing into Form XObjects the way
/// pdf-extract does (a form without /Resources uses its caller's).
fn scan_ops(
    doc: &Document,
    ops: &[Operation],
    resources: Option<&Dictionary>,
    stack: &mut Vec<ObjectId>,
    flags: &mut BTreeSet<String>,
) -> Result<(), String> {
    for op in ops {
        match op.operator.as_str() {
            "'" => {
                flags.insert(FLAG_QUOTE.into());
            }
            "\"" => {
                flags.insert(FLAG_DOUBLE_QUOTE.into());
            }
            "Do" => scan_xobject(doc, op, resources, stack, flags)?,
            _ => {}
        }
    }
    Ok(())
}

fn scan_xobject(
    doc: &Document,
    op: &Operation,
    resources: Option<&Dictionary>,
    stack: &mut Vec<ObjectId>,
    flags: &mut BTreeSet<String>,
) -> Result<(), String> {
    let target = (|| {
        let name = op.operands.first()?.as_name().ok()?;
        let xobjects = deref(doc, resources?.get(b"XObject").ok()?)?
            .1
            .as_dict()
            .ok()?;
        let (id, obj) = deref(doc, xobjects.get(name).ok()?)?;
        Some((id, obj.as_stream().ok()?))
    })();
    let Some((id, stream)) = target else {
        flags.insert(FLAG_XOBJECT_MISSING.into());
        return Ok(());
    };
    match stream.dict.get(b"Subtype").and_then(Object::as_name) {
        Ok(b"Form") => {}
        Ok(b"Image") => {
            flags.insert(FLAG_IMAGE.into());
            return Ok(());
        }
        _ => {
            flags.insert(FLAG_XOBJECT_OTHER.into());
            return Ok(());
        }
    }
    flags.insert(FLAG_FORM.into());
    if stack.len() >= MAX_FORM_DEPTH {
        return Err("Form XObjects nested too deep".into());
    }
    if id.is_some_and(|id| stack.contains(&id)) {
        return Err("Form XObject draws itself".into());
    }
    let data = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let ops = decode_ops(&data, flags)?;
    let own = stream
        .dict
        .get(b"Resources")
        .ok()
        .and_then(|r| deref(doc, r))
        .and_then(|(_, r)| r.as_dict().ok());
    stack.push(id.unwrap_or((u32::MAX, u16::MAX)));
    let r = scan_ops(doc, &ops, own.or(resources), stack, flags);
    stack.pop();
    r
}

fn preflight(doc: &Document, page_id: ObjectId) -> Preflight {
    let mut pf = Preflight::default();
    let Ok(page) = doc.get_dictionary(page_id) else {
        pf.fatal = Some("page object is not a dictionary".into());
        return pf;
    };
    if let Err(e) = check_parents(doc, page_id, page) {
        pf.fatal = Some(e);
        return pf;
    }
    let media = inherited(doc, page, b"MediaBox")
        .and_then(|m| m.as_array().ok())
        .filter(|a| a.len() == 4)
        .and_then(|a| {
            let v: Option<Vec<i64>> = a.iter().map(|o| number(doc, o).and_then(to_hpt)).collect();
            v.map(|v| [v[0], v[1], v[2], v[3]])
        });
    let Some(media) = media else {
        pf.fatal = Some("missing or malformed /MediaBox".into());
        return pf;
    };
    pf.media = Some(media);
    if media[0] != 0 || media[1] != 0 {
        pf.flags.insert(FLAG_MEDIA_ORIGIN.into());
    }
    if let Some(rot) = inherited(doc, page, b"Rotate").and_then(|r| number(doc, r)) {
        let rot = (rot.round() as i64).rem_euclid(360);
        if rot != 0 {
            pf.flags.insert(format!("rotate-{rot}"));
        }
    }
    let resources = inherited(doc, page, b"Resources").and_then(|r| r.as_dict().ok());
    let data = match doc.get_page_content(page_id) {
        Ok(d) => d,
        Err(e) => {
            pf.fatal = Some(format!("cannot read page content: {e}"));
            return pf;
        }
    };
    let result = decode_ops(&data, &mut pf.flags)
        .and_then(|ops| scan_ops(doc, &ops, resources, &mut vec![page_id], &mut pf.flags));
    if let Err(e) = result {
        pf.fatal = Some(e);
    }
    pf
}

fn info_string(doc: &Document, key: &[u8]) -> Option<String> {
    let info = deref(doc, doc.trailer.get(b"Info").ok()?)?
        .1
        .as_dict()
        .ok()?;
    let v = deref(doc, info.get(key).ok()?)?.1;
    pdf_extract::decode_text_string(v).ok()
}

fn extract_page(doc: &Document, index: u32, page_id: ObjectId) -> PageExtract {
    let mut page = PageExtract {
        index,
        ..PageExtract::default()
    };
    let pf = guarded(|| preflight(doc, page_id)).unwrap_or_else(|p| Preflight {
        fatal: Some(format!("preflight panicked: {p}")),
        ..Preflight::default()
    });
    page.media = pf.media;
    page.flags = pf.flags.into_iter().collect();
    if let Some(e) = pf.fatal {
        page.error = Some(clip(e));
        return page;
    }
    let mut col = Collector {
        media: None,
        run: 0,
        seq: 0,
        glyphs: Vec::new(),
    };
    match guarded(|| pdf_extract::output_doc_page(doc, &mut col, index)) {
        Ok(Ok(())) => page.glyphs = col.glyphs,
        Ok(Err(e)) => page.error = Some(clip(format!("pdf-extract error: {e}"))),
        Err(p) => page.error = Some(format!("pdf-extract panicked: {p}")),
    }
    page
}

fn extract_inner(bytes: &[u8]) -> Result<DocExtract, String> {
    let mut doc = guarded(|| Document::load_mem(bytes))
        .map_err(|p| format!("PDF parser panicked: {p}"))?
        .map_err(|e| clip(format!("cannot parse PDF: {e}")))?;
    if doc.is_encrypted() {
        guarded(|| doc.decrypt(""))
            .map_err(|p| format!("decryption panicked: {p}"))?
            .map_err(|e| clip(format!("encrypted PDF: {e}")))?;
    }
    let pages = guarded(|| doc.get_pages()).map_err(|p| format!("page tree walk panicked: {p}"))?;
    Ok(DocExtract {
        creator: guarded(|| info_string(&doc, b"Creator")).ok().flatten(),
        producer: guarded(|| info_string(&doc, b"Producer")).ok().flatten(),
        pages: pages
            .into_iter()
            .map(|(i, id)| extract_page(&doc, i, id))
            .collect(),
    })
}

/// Extracts every page of a PDF held in memory. Never panics: a document
/// that cannot be opened is an `Err`, and a page that cannot be read carries
/// an `error` and no glyphs.
pub fn extract_pdf(bytes: &[u8]) -> Result<DocExtract, String> {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("refpdf-extract".into())
            .stack_size(WORKER_STACK)
            .spawn_scoped(s, || extract_inner(bytes))
            .map_err(|e| format!("cannot start the extraction thread: {e}"))?
            .join()
            .map_err(|_| "the extraction thread panicked".to_string())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panics_become_errors_quietly() {
        let r: Result<(), String> = guarded(|| panic!("boom {}", 7));
        let e = r.unwrap_err();
        assert!(e.starts_with("boom 7 at "), "{e}");
        assert_eq!(guarded(|| 5), Ok(5));
    }

    #[test]
    fn source_paths_are_shortened() {
        assert_eq!(
            short_path(
                "/root/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/pdf-extract-0.12.1/src/lib.rs"
            ),
            "pdf-extract-0.12.1/src/lib.rs"
        );
        assert_eq!(
            short_path("C:\\x\\registry\\src\\idx\\lopdf-0.42.0\\src\\a.rs"),
            "lopdf-0.42.0/src/a.rs"
        );
        assert_eq!(
            short_path("/rustc/abc/library/core/src/option.rs"),
            "library/core/src/option.rs"
        );
        assert_eq!(short_path("src/extract.rs"), "src/extract.rs");
    }

    #[test]
    fn garbage_is_an_error_not_a_crash() {
        assert!(extract_pdf(b"").is_err());
        assert!(extract_pdf(b"%PDF-1.4\n%%EOF\n").is_err());
        assert!(extract_pdf(&[0xff; 4096]).is_err());
    }

    #[test]
    fn glyph_geometry_from_the_text_matrix() {
        // 12 pt Courier at (72, 720) on a Letter page.
        let trm = Transform::row_major(1.0, 0.0, 0.0, 1.0, 72.0, 720.0);
        let g = make_glyph(&trm, 0.6, 12.0, (0.0, 792.0), "A", 3, 9);
        assert_eq!(
            (g.x, g.y, g.size, g.adv, g.upright),
            (7200, 7200, 1200, 720, true)
        );
        assert_eq!((g.run, g.seq), (3, 9));
        // Rotated 45 degrees: not upright, size from the determinant.
        let r = std::f64::consts::FRAC_1_SQRT_2;
        let trm = Transform::row_major(r, r, -r, r, 200.0, 300.0);
        let g = make_glyph(&trm, 0.6, 40.0, (0.0, 792.0), "W", 1, 1);
        assert!(!g.upright);
        assert_eq!(g.size, 4000);
        // Mirrored text is not upright either.
        let trm = Transform::row_major(-1.0, 0.0, 0.0, 1.0, 72.0, 720.0);
        assert!(!make_glyph(&trm, 0.6, 12.0, (0.0, 792.0), "A", 1, 1).upright);
        // A NaN size never counts as script text.
        let trm = Transform::row_major(1.0, 0.0, 0.0, 1.0, 72.0, 720.0);
        let g = make_glyph(&trm, 0.6, f64::NAN, (0.0, 792.0), "A", 1, 1);
        assert!(!g.is_regular());
    }
}
