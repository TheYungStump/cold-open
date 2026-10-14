//! End to end: build a small PDF with the standard Courier font through
//! pdf-extract's re-exported lopdf types, then check what `extract`,
//! `grid`, `self-check` and `stats` make of it, including pages built to
//! make pdf-extract panic or recurse forever.

use std::path::{Path, PathBuf};
use std::process::Command;

use pdf_extract::content::{Content, Operation};
use pdf_extract::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use refpdf::extract::extract_pdf;
use refpdf::geom::{GRID_LINE, GRID_PITCH, Segment};
use refpdf::reffile::{self, RefFile};

fn op(o: &str, args: Vec<Object>) -> Operation {
    Operation::new(o, args)
}

fn s(text: &str) -> Object {
    Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
}

fn n(v: f32) -> Object {
    Object::Real(v)
}

/// `BT /F1 size Tf x y Td (text) Tj ET`
fn text_at(size: i64, x: f32, y: f32, text: &str) -> Vec<Operation> {
    vec![
        op("BT", vec![]),
        op("Tf", vec!["F1".into(), size.into()]),
        op("Td", vec![n(x), n(y)]),
        op("Tj", vec![s(text)]),
        op("ET", vec![]),
    ]
}

fn stream(ops: Vec<Operation>, dict: Dictionary) -> Stream {
    Stream::new(dict, Content { operations: ops }.encode().unwrap())
}

struct Builder {
    doc: Document,
    pages_id: ObjectId,
    font_res: ObjectId,
    kids: Vec<Object>,
}

impl Builder {
    fn new() -> Self {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let font = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
            "Encoding" => "WinAnsiEncoding",
        });
        let font_res = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => font } });
        Builder {
            doc,
            pages_id,
            font_res,
            kids: Vec::new(),
        }
    }

    fn page(&mut self, ops: Vec<Operation>, mut extra: Dictionary) -> ObjectId {
        let content = self.doc.add_object(stream(ops, Dictionary::new()));
        extra.set("Type", "Page");
        extra.set("Contents", content);
        if !extra.has(b"Parent") {
            extra.set("Parent", self.pages_id);
        }
        let id = self.doc.add_object(extra);
        self.kids.push(id.into());
        id
    }

    fn finish(mut self) -> Vec<u8> {
        let count = self.kids.len() as i64;
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => self.kids,
            "Count" => count,
            "Resources" => self.font_res,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        self.doc
            .objects
            .insert(self.pages_id, Object::Dictionary(pages));
        let catalog = self
            .doc
            .add_object(dictionary! { "Type" => "Catalog", "Pages" => self.pages_id });
        let info = self
            .doc
            .add_object(dictionary! { "Producer" => s("refpdf e2e test") });
        self.doc.trailer.set("Root", catalog);
        self.doc.trailer.set("Info", info);
        let mut out = Vec::new();
        self.doc.save_to(&mut out).unwrap();
        out
    }
}

fn test_pdf() -> Vec<u8> {
    let mut b = Builder::new();

    // Page 1: ordinary text at Courier's own 7.2 pt pitch.
    let mut p1 = Vec::new();
    // The page number, drawn twice the way some printers do.
    p1.extend(text_at(12, 504.0, 748.0, "2."));
    p1.extend(text_at(12, 504.0, 748.0, "2."));
    p1.extend(text_at(12, 108.0, 720.0, "INT. HOUSE - DAY"));
    // Two runs one space apart, then a far-right run.
    p1.extend(text_at(12, 108.0, 696.0, "Hello"));
    p1.extend(text_at(12, 151.2, 696.0, "world."));
    p1.extend(text_at(12, 500.0, 696.0, "12A."));
    // A run boundary 1.44 pitches away: two segments.
    p1.extend(text_at(12, 108.0, 672.0, "AB"));
    p1.extend(text_at(12, 125.6, 672.0, "CD"));
    // Drawn trailing spaces.
    p1.extend(text_at(12, 108.0, 648.0, "End.  "));
    // A 60 pt watermark rotated by about 37 degrees (a 3-4-5 rotation).
    p1.extend([
        op("BT", vec![]),
        op("Tf", vec!["F1".into(), 60.into()]),
        op(
            "Tm",
            vec![n(0.8), n(0.6), n(-0.6), n(0.8), n(150.0), n(300.0)],
        ),
        op("Tj", vec![s("DRAFT")]),
        op("ET", vec![]),
    ]);
    b.page(p1, Dictionary::new());

    // Page 2: constructs pdf-extract mishandles, on a rotated page whose
    // MediaBox does not start at the origin.
    let form = b.doc.add_object(stream(
        text_at(12, 118.0, 610.0, "FORM"),
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => b.font_res,
        },
    ));
    let font = b
        .doc
        .get_dictionary(b.font_res)
        .unwrap()
        .get(b"Font")
        .unwrap()
        .clone();
    let res2 = b
        .doc
        .add_object(dictionary! { "Font" => font, "XObject" => dictionary! { "Fm1" => form } });
    let p2 = vec![
        op("BT", vec![]),
        op("Tf", vec!["F1".into(), 12.into()]),
        op("TL", vec![14.into()]),
        op("Td", vec![n(118.0), n(730.0)]),
        op("Tj", vec![s("first")]),
        op("'", vec![s("second")]),
        op("\"", vec![0.into(), 0.into(), s("third")]),
        op("ET", vec![]),
        op("Do", vec!["Fm1".into()]),
    ];
    b.page(
        p2,
        dictionary! {
            "Rotate" => 90,
            "MediaBox" => vec![10.into(), 10.into(), 622.into(), 802.into()],
            "Resources" => res2,
        },
    );

    // Page 3: Tj before any Tf makes pdf-extract unwrap a None.
    b.page(
        vec![
            op("BT", vec![]),
            op("Td", vec![n(108.0), n(720.0)]),
            op("Tj", vec![s("x")]),
            op("ET", vec![]),
        ],
        Dictionary::new(),
    );

    // Page 4: a Form XObject that draws itself; pdf-extract would recurse
    // until the stack overflows.
    let loop_id = b.doc.new_object_id();
    let loop_form = stream(
        vec![op("Do", vec!["Me".into()])],
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            "Resources" => dictionary! { "XObject" => dictionary! { "Me" => loop_id } },
        },
    );
    b.doc.objects.insert(loop_id, Object::Stream(loop_form));
    let res4 = b
        .doc
        .add_object(dictionary! { "XObject" => dictionary! { "Me" => loop_id } });
    b.page(
        vec![op("Do", vec!["Me".into()])],
        dictionary! { "Resources" => res4 },
    );

    // Page 5: a /Parent chain that loops before reaching /Resources.
    let knot = b.doc.new_object_id();
    b.doc.objects.insert(
        knot,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Parent" => knot }),
    );
    b.page(
        text_at(12, 108.0, 720.0, "lost"),
        dictionary! { "Parent" => knot },
    );

    b.finish()
}

fn seg(y: i64, x0: i64, x1: i64, text: &str, cells: i64, trail: i64, multi_run: bool) -> Segment {
    Segment {
        y,
        x0,
        x1,
        text: text.into(),
        cells,
        trail,
        multi_run,
    }
}

fn expected_grid_page1() -> String {
    let pad = |n: usize| " ".repeat(n);
    let mut rows = vec![String::new(); 13];
    rows[3] = format!("{}2.", pad(70));
    rows[6] = format!("{}INT. HOUSE - DAY", pad(15));
    rows[8] = format!("{}Hello world.{}12A.", pad(15), pad(42));
    rows[10] = format!("{}ABCD", pad(15));
    rows[12] = format!("{}End.", pad(15));
    let mut out = String::from("=== page 1 ===\n");
    for r in rows {
        out.push_str(&r);
        out.push('\n');
    }
    out
}

#[test]
fn extract_reports_segments_flags_and_failed_pages() {
    let bytes = test_pdf();
    let doc = extract_pdf(&bytes).expect("the test PDF opens");
    let an = reffile::analyze(&doc);
    let r = reffile::build(reffile::sha256_hex(&bytes), &doc, &an);

    assert_eq!(r.meta.pages, 5);
    assert_eq!(r.meta.producer.as_deref(), Some("refpdf e2e test"));
    assert_eq!(r.pitch_x, Some(720));

    let p1 = &r.pages[0];
    assert_eq!(p1.media, Some([0, 0, 61200, 79200]));
    assert_eq!((p1.odd, p1.overprint), (5, 2));
    assert_eq!(p1.error, None);
    assert!(p1.flags.is_empty());
    assert_eq!(
        p1.segments,
        vec![
            seg(4400, 50400, 51840, "2.", 2, 0, false),
            seg(7200, 10800, 22320, "INT. HOUSE - DAY", 16, 0, false),
            seg(9600, 10800, 19440, "Hello world.", 12, 0, true),
            seg(9600, 50000, 52880, "12A.", 4, 0, false),
            seg(12000, 10800, 12240, "AB", 2, 0, false),
            seg(12000, 12560, 14000, "CD", 2, 0, false),
            seg(14400, 10800, 13680, "End.", 4, 2, false),
        ]
    );

    // pdf-extract silently drops text shown with ' and ", which is why
    // those operators are flagged. Coordinates are measured from the
    // MediaBox corner.
    let p2 = &r.pages[1];
    assert_eq!(
        p2.flags,
        [
            "form-xobject",
            "mediabox-origin",
            "rotate-90",
            "text-op-double-quote",
            "text-op-quote"
        ]
    );
    assert_eq!(p2.media, Some([1000, 1000, 62200, 80200]));
    let texts: Vec<(&str, i64, i64)> = p2
        .segments
        .iter()
        .map(|s| (s.text.as_str(), s.x0, s.y))
        .collect();
    assert_eq!(texts, [("first", 10800, 7200), ("FORM", 10800, 19200)]);
    assert_eq!(p2.error, None);

    let e3 = r.pages[2].error.as_deref().unwrap();
    assert!(e3.starts_with("pdf-extract panicked: "), "{e3}");
    assert!(r.pages[2].segments.is_empty());
    assert_eq!(
        r.pages[3].error.as_deref(),
        Some("Form XObject draws itself")
    );
    assert_eq!(r.pages[3].flags, ["form-xobject"]);
    assert_eq!(r.pages[4].error.as_deref(), Some("cyclic /Parent chain"));

    let checks = reffile::checks(&r, &an);
    let failed = checks
        .iter()
        .filter(|c| c.level == reffile::Level::Fail)
        .count();
    assert_eq!(failed, 1, "only the failed-pages check fails: {checks:?}");
}

#[test]
fn grid_matches_the_page_view_definition() {
    let doc = extract_pdf(&test_pdf()).unwrap();
    let grid = refpdf::grid_text(&doc, GRID_PITCH, GRID_LINE);
    let pad = " ".repeat(15);
    // Page 2: "first" on row 6, the form's text on row 16.
    let mut expected = expected_grid_page1();
    expected.push_str("=== page 2 ===\n");
    expected.push_str(&"\n".repeat(6));
    expected.push_str(&format!("{pad}first\n"));
    expected.push_str(&"\n".repeat(9));
    expected.push_str(&format!("{pad}FORM\n"));
    // Failed pages keep their headers so page numbers stay aligned.
    expected.push_str("=== page 3 ===\n=== page 4 ===\n=== page 5 ===\n");
    assert_eq!(grid, expected);
}

#[test]
fn damaged_pdfs_never_crash() {
    // Deterministic byte flips and truncations. Every outcome is fine
    // except taking the process down.
    let bytes = test_pdf();
    let mut seed: u64 = 0x5eed;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for i in 0..200 {
        let mut b = bytes.clone();
        if i % 4 == 0 {
            b.truncate(next() % b.len());
        } else {
            for _ in 0..1 + i % 16 {
                let p = next() % b.len();
                b[p] = next() as u8;
            }
        }
        if let Ok(doc) = extract_pdf(&b) {
            let an = reffile::analyze(&doc);
            let r = reffile::build(String::new(), &doc, &an);
            let _ = reffile::to_json(&r);
            let _ = refpdf::grid_text(&doc, GRID_PITCH, GRID_LINE);
        }
    }
}

fn tmp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("refpdf-e2e")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn refpdf(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_refpdf"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn the_command_line_round_trip() {
    let dir = tmp("cli");
    let src = dir.join("in");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    let pdf = src.join("sub").join("scene.pdf");
    std::fs::write(&pdf, test_pdf()).unwrap();
    std::fs::write(src.join("notes.txt"), "not a pdf").unwrap();
    let out = dir.join("out");
    let (pdf_s, src_s, out_s) = (
        pdf.to_str().unwrap(),
        src.to_str().unwrap(),
        out.to_str().unwrap(),
    );

    // extract walks the directory; failed pages make it exit 1.
    let (code, stdout, _) = refpdf(&["extract", "--out", out_s, src_s]);
    assert_eq!(code, 1, "{stdout}");
    assert!(stdout.contains("FAIL 3 failed pages"), "{stdout}");
    assert!(
        stdout.contains("WARN neither Creator nor Producer names Final Draft"),
        "{stdout}"
    );
    let json = std::fs::read_to_string(out.join("scene.ref.json")).unwrap();
    let r: RefFile = serde_json::from_str(&json).unwrap();
    assert_eq!(
        (r.schema, r.tool.as_str(), r.pages.len()),
        (1, "refpdf 0.1", 5)
    );
    // Deterministic: a second run writes the same bytes.
    refpdf(&["extract", "--out", out_s, pdf_s]);
    assert_eq!(
        std::fs::read_to_string(out.join("scene.ref.json")).unwrap(),
        json
    );

    // grid prints the page view and exits 1 because pages failed.
    let (code, grid, stderr) = refpdf(&["grid", pdf_s]);
    assert_eq!(code, 1);
    assert!(grid.starts_with(&expected_grid_page1()));
    assert!(stderr.contains("page 3 failed"), "{stderr}");
    let pitch7 = refpdf(&["grid", "--pitch", "7", "--line=12", pdf_s]).1;
    assert_ne!(pitch7, grid);

    // self-check: identical, then one changed row.
    let view = dir.join("pages.txt");
    std::fs::write(
        &view,
        grid.replace("=== page 2 ===", "=== page 2 (from the layout engine) ==="),
    )
    .unwrap();
    let (code, stdout, _) = refpdf(&["self-check", pdf_s, view.to_str().unwrap()]);
    assert_eq!(code, 1, "pages failed, so even a match exits 1: {stdout}");
    assert!(stdout.starts_with("identical: 5 pages"), "{stdout}");
    std::fs::write(&view, grid.replace("Hello world.", "Hello  world.")).unwrap();
    let (code, stdout, _) = refpdf(&["self-check", pdf_s, view.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(
        stdout.starts_with("first difference: page 1, row 8\n"),
        "{stdout}"
    );

    // stats prints numbers only.
    let (code, stdout, _) = refpdf(&["stats", out.join("scene.ref.json").to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(stdout.contains("pitch_x 7.20 pt"), "{stdout}");
    assert!(
        stdout.contains("page-number segments 1: y 44.00x1"),
        "{stdout}"
    );
    // ("first" would also match the "first body baseline" label.)
    for word in [
        "INT", "HOUSE", "DAY", "Hello", "world", "End", "FORM", "12A",
    ] {
        assert!(!stdout.contains(word), "stats leaked text: {stdout}");
    }

    // Usage errors exit 2.
    assert_eq!(refpdf(&["grid"]).0, 2);
    assert_eq!(refpdf(&["grid", "--pitch", "abc", pdf_s]).0, 2);
    assert_eq!(refpdf(&["frobnicate"]).0, 2);
}
