//! The PDF draws what the layout placed (layout.md §11, §12). Each case is
//! read back from the PDF's own content streams, with the widths the file
//! declares, and compared glyph by glyph with the layout.

mod common;

use std::collections::BTreeSet;

use coldopen_layout::pdf::{self, PdfOptions, PdfOutput, PdfWarning};
use coldopen_layout::{Fonts, Layout, Options, ValidFormat, layout};
use coldopen_screenplay::{PageFormat, Pt};
use common::pdf_dump;
use common::pdf_read::{Pdf, assert_draws_layout, watermark_glyphs};

fn lay(fountain: &str, format: &ValidFormat, options: &Options) -> Layout {
    layout(&coldopen_fountain::parse(fountain).script, format, options)
}

/// Writes the PDF twice (it must not change) and checks it against the
/// layout.
fn write(layout: &Layout, watermark: Option<&str>) -> PdfOutput {
    let fonts = Fonts::new().unwrap();
    let options = PdfOptions {
        watermark: watermark.map(str::to_string),
        title: None,
    };
    let out = pdf::write(layout, &fonts, &options).unwrap();
    assert_eq!(pdf::write(layout, &fonts, &options).unwrap(), out);
    assert_draws_layout(&out.bytes, layout, &fonts);
    out
}

fn warning(code: &'static str, chars: &str) -> PdfWarning {
    PdfWarning {
        code,
        chars: chars.to_string(),
    }
}

/// Every glyph's text, in drawing order, watermark included.
fn texts(bytes: &[u8]) -> String {
    Pdf::parse(bytes)
        .glyphs()
        .iter()
        .flatten()
        .map(|g| g.text.as_str())
        .collect()
}

/// The text the first page's watermark draws.
fn watermark(bytes: &[u8]) -> String {
    watermark_glyphs(bytes)[0]
        .iter()
        .map(|g| g.text.as_str())
        .collect()
}

/// A soft hyphen takes no cell (layout.md §2). The font has a glyph for it
/// with an advance, which used to print as a hyphen and push the rest of
/// the line one cell right, past the right margin on a full line.
#[test]
fn soft_hyphens_draw_nothing() {
    let full = format!(
        "{}\u{AD}{}\u{AD}{} w",
        "x".repeat(20),
        "y".repeat(19),
        "z".repeat(19)
    );
    let src = format!(
        "INT. ROOM - DAY\n\nCCC\u{AD}DDD end\n\n{full}\n\nOne \u{AD}two\n\n_UN\u{AD}DER_ line\n"
    );
    let format = ValidFormat::default();
    let l = lay(&src, &format, &Options::default());
    let view = l.page_view(&Fonts::new().unwrap());
    assert!(view.contains("CCC\u{AD}DDD end"), "{view}");
    let out = write(&l, None);
    assert!(!texts(&out.bytes).contains('\u{AD}'));
    assert_eq!(out.warnings, vec![]);
    // The full line's last glyph ends at the right margin, 7.5 in.
    let page = &Pdf::parse(&out.bytes).glyphs()[0];
    let w = page.iter().find(|g| g.text == "w").unwrap();
    assert_eq!(w.x + 7_200_000, 540_000_000);

    // Nor in the watermark.
    let out = write(&l, Some("DR\u{AD}AFT"));
    assert_eq!(watermark(&out.bytes), "DRAFT");
    assert!(!texts(&out.bytes).contains('\u{AD}'));
}

/// A cluster whose glyphs do not fill its cells: an emoji-presentation
/// sequence whose selector the font lacks (two cells, one glyph), a keycap,
/// and a combining mark with no base (one cell, a zero-width glyph). The
/// next cluster used to follow the glyphs' advance and drift off the grid.
#[test]
fn clusters_that_glyphs_do_not_fill_keep_the_grid() {
    let src = "INT. ROOM - DAY\n\n\
        AAA\u{A9}\u{FE0F}BBB end\n\n\
        Step 1\u{FE0F}\u{20E3} then end\n\n\
        \u{326}abc def\n\n\
        Key #\u{FE0F}\u{20E3} and \u{2122}\u{FE0F} and \u{326} mark\n\n\
        _AAA\u{A9}\u{FE0F}BBB_ and **\u{326}bold** end\n\n\
        Tail mark \u{326}\n";
    let format = ValidFormat::default();
    let l = lay(src, &format, &Options::default());
    let view = l.page_view(&Fonts::new().unwrap());
    assert!(view.contains("AAA\u{A9} BBB end"), "{view}");
    let out = write(&l, None);
    assert_eq!(
        out.warnings,
        vec![warning("missing_mark", "\u{20E3}\u{FE0F}")]
    );
}

/// With a pitch that is not 0.6 em, character spacing makes every glyph
/// advance one pitch, but it is added to zero-width marks too. Each mark
/// used to push the rest of its run right by the spacing.
#[test]
fn character_spacing_does_not_move_text_after_marks() {
    let src = "INT. ROOM - DAY\n\n\
        s\u{326}s\u{326}s\u{326}s\u{326} abc end\n\n\
        \u{326}abc def and s\u{326} t\u{326}\n";
    for size in [10, 11, 13] {
        let format = ValidFormat::new(PageFormat {
            font_size: Pt::points(size),
            ..PageFormat::default()
        })
        .unwrap();
        let l = lay(src, &format, &Options::default());
        let out = write(&l, None);
        assert!(pdf_dump(&out.bytes).contains(" Tc\n"), "{size} pt");
    }
}

/// A script with nothing printable still gives a valid PDF: one blank page
/// at the format's size, with the watermark.
#[test]
fn nothing_to_print_gives_one_blank_page() {
    let format = ValidFormat::default();
    let no_title = Options {
        no_title_page: true,
        ..Options::default()
    };
    for (src, options) in [
        ("", &Options::default()),
        (
            "[[A note.]]\n\n# Act One\n\n/* Cut. */\n\n= A synopsis.\n",
            &Options::default(),
        ),
        ("Title: Only a Title\nAuthor: Someone\n\n", &no_title),
    ] {
        let l = lay(src, &format, options);
        assert!(l.pages.is_empty() && l.title.is_empty(), "{src:?}");
        let out = write(&l, None);
        assert_eq!(out.pages, 1, "{src:?}");
        let pdf = Pdf::parse(&out.bytes);
        assert_eq!(pdf.page_count(), 1);
        assert!(pdf.glyphs()[0].is_empty());
        assert!(
            pdf_dump(&out.bytes).contains("/MediaBox[0 0 612 792]"),
            "{src:?}"
        );

        let out = write(&l, Some("DRAFT"));
        assert_eq!(out.pages, 1);
        assert_eq!(watermark(&out.bytes), "DRAFT");
    }
}

/// Watermark characters the font lacks print as boxes, so the export lists
/// them with the body's missing characters.
#[test]
fn watermark_characters_the_font_lacks_are_reported() {
    let format = ValidFormat::default();
    let l = lay(
        "INT. ROOM - DAY\n\nA sign reads \u{65E5}.\n",
        &format,
        &Options::default(),
    );
    let mark = "\u{427}\u{415}\u{420}\u{41D}\u{41E}\u{412}\u{418}\u{41A} \u{8349}\u{7A3F}";
    let out = write(&l, Some(mark));
    let lacking: BTreeSet<char> = mark
        .chars()
        .filter(|c| *c != ' ')
        .chain(['\u{65E5}'])
        .collect();
    assert_eq!(
        out.warnings,
        vec![warning(
            "missing_glyph",
            &lacking.iter().collect::<String>()
        )]
    );
    assert_eq!(
        watermark(&out.bytes),
        "\u{FFFD}".repeat(8) + " " + &"\u{FFFD}".repeat(2)
    );

    // On a page with nothing else, the watermark alone is reported.
    let empty = lay("", &format, &Options::default());
    let out = write(&empty, Some("\u{65E5}\u{672C} DRAFT"));
    assert_eq!(
        out.warnings,
        vec![warning("missing_glyph", "\u{65E5}\u{672C}")]
    );
}

/// A cluster prints as boxes when any of its characters with a width has no
/// glyph; the warning names that character, not the cluster's first.
#[test]
fn missing_glyphs_name_the_characters_the_font_lacks() {
    let format = ValidFormat::default();
    for (src, lacking) in [
        ("ab\u{200D}\u{65E5} end", "\u{65E5}"),
        ("x\u{200D}y\u{200D}\u{672C} end", "\u{672C}"),
        ("\u{65E5}\u{200D}b end", "\u{65E5}"),
        ("a\u{200D}\u{65E5}\u{200D}\u{672C}", "\u{65E5}\u{672C}"),
        // The same missing character in a second cluster.
        ("ab\u{200D}\u{65E5} and cd\u{200D}\u{65E5}", "\u{65E5}"),
    ] {
        let l = lay(
            &format!("INT. ROOM - DAY\n\n{src}\n"),
            &format,
            &Options::default(),
        );
        let out = write(&l, None);
        assert_eq!(
            out.warnings,
            vec![warning("missing_glyph", lacking)],
            "{src:?}"
        );
    }
}

/// A mark with no base takes a cell of its own (layout.md §2), and its
/// glyph is placed at that cell like any other; its ink, which hangs left
/// of its origin, prints over the cell before (§12). The text after it
/// stays on the grid.
#[test]
fn a_mark_with_no_base_is_placed_at_its_own_cell() {
    let format = ValidFormat::default();
    let l = lay("\u{326}abc def\n", &format, &Options::default());
    let out = write(&l, None);
    let glyphs = Pdf::parse(&out.bytes).glyphs();
    let at = |t: &str| {
        glyphs[0]
            .iter()
            .find(|g| g.text == t)
            .map(|g| g.x)
            .unwrap_or_else(|| panic!("no {t:?}"))
    };
    // Action starts at 1.5 in (108 pt); the mark has cell 0, `a` cell 1.
    assert_eq!(at("\u{326}"), 108_000_000);
    assert_eq!(at("a"), 115_200_000);
}

/// A combining mark the font lacks is left out of the watermark, as in the
/// script, instead of printing a box.
#[test]
fn watermark_marks_the_font_lacks_are_left_out() {
    let format = ValidFormat::default();
    let l = lay("A line.\n", &format, &Options::default());
    let out = write(&l, Some("X\u{301}"));
    assert_eq!(out.warnings, vec![warning("missing_mark", "\u{301}")]);
    assert_eq!(watermark(&out.bytes), "X");
}
