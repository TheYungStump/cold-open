# refpdf

A dev-only tool that reads reference PDFs printed by Final Draft and
extracts the position of every glyph. The phase 2 layout gate compares our
pagination against what it finds.

**It is never shipped.** It is its own Cargo workspace (like `fuzz/`), it is
excluded from the root workspace, and no Cold Open crate depends on it.

It only reads PDF files that people have published. It does not run or
script Final Draft, and nothing in it comes from Final Draft's software,
website or manuals.

## Where the data lives

Real scripts are copyrighted, so the PDFs and everything extracted from them
stay in `corpus/`, which is gitignored:

- `extract` writes `<stem>.ref.json` next to each PDF by default, which keeps
  the output in `corpus/`. Use `--out DIR` to write somewhere else, such as a
  scratch folder under `target/`.
- Never commit `.ref.json` files, `grid` output or `self-check` output made
  from real scripts. All of them contain script text.
- `stats` prints numbers only, never text. Its output is what goes into
  specs, PR descriptions and reports.

## Running it

```sh
cd tools/refpdf
cargo run --release -- extract ../../corpus/fd11
cargo run --release -- extract --out ../../target/refpdf-out ../../corpus/fd11
cargo run --release -- grid ../../corpus/fd11/little-women/little-women.pdf
cargo run --release -- grid --pitch 7 --line 12 some.pdf
cargo run --release -- self-check some.pdf pages.txt
cargo run --release -- stats ../../corpus/fd11/*/*.ref.json
```

| Command | What it does | Exit code |
| --- | --- | --- |
| `extract [--out DIR] <pdf-or-dir>...` | Writes one `.ref.json` per PDF. Directories are scanned recursively in sorted order (symbolic links are skipped). Prints the checks below. | 0, or 1 if a PDF could not be opened, a page failed or a check failed |
| `grid [--pitch PT] [--line PT] <pdf>` | Prints the page-view grid (defined below). | 0, or 1 if a page failed |
| `self-check [--pitch PT] [--line PT] <pdf> <pages.txt>` | Compares the PDF's grid with a page-view file and prints the first differing page and row, with both texts. | 0 when identical, otherwise 1 |
| `stats <ref.json>...` | Calibration numbers: line pitch, first baseline, last row, x0 modes, cells per x0, page-number position. | 0 |

A usage error exits with 2. `PT` is a number of points with at most two
decimals, such as `7.2`.

The checks `extract` prints:

- whether Creator or Producer names Final Draft (a warning, not an error);
- whether every MediaBox is 612 x 792 pt within 0.5 pt;
- whether the detected pitch is within 6.5–7.5 pt;
- the share of glyphs outside printable Latin-1 and General Punctuation
  (a warning above 0.1%);
- the number of failed pages, the flagged constructs, and the overprint and
  odd-glyph counts.

## How glyphs become segments

Every coordinate is an integer number of hundredths of a point (1/100 pt),
rounded half away from zero, so output does not depend on float formatting.
Coordinates are measured from the top-left corner of the MediaBox:
`x = trm.m31 - llx` (the glyph origin) and `y = ury - trm.m32` (the
baseline).

1. **Odd glyphs.** Glyphs that are rotated, skewed or mirrored, or whose
   size is outside 10–14 pt, are counted in `odd` and set aside (watermarks,
   stamps).
2. **Overprints.** A glyph with the same text as an earlier glyph within
   ±1 pt across and ±0.5 pt down is a duplicate. The first one drawn is kept
   and the rest are counted in `overprint`. Final Draft's Quartz PDFs draw
   page numbers, and some header and footer lines, twice.
3. **Baselines.** Glyphs are sorted by y; a new baseline starts when a glyph
   is more than 2 pt below the first glyph of the current one. Its y is the
   median.
4. **Pitch.** The mode of origin-to-origin dx between neighbouring glyphs
   on a baseline, counting only 5–9 pt, over the whole document (ties go to
   the smaller value).
5. **Segments.** A baseline splits where the origin gap exceeds 4 pitches
   (more than 4.15, to absorb rounding), or where two neighbouring glyphs
   come from different text runs and their gap is not a whole number of
   pitches within 0.15 pitch. A run is one string passed to a text-showing
   operator. Final Draft's PDFs place almost every glyph with its own run,
   so in practice the second rule tests every gap.
6. **Spaces.** Between neighbouring glyphs, `max(0, round(dx / p) - 1)`
   spaces are inserted. Spaces the PDF actually draws are kept as glyphs.

## `.ref.json`

Keys are sorted, the output is pretty-printed, and there are no timestamps,
so the same PDF always gives the same bytes.

```json
{
  "meta": { "creator": "...", "pages": 16, "producer": "..." },
  "pages": [
    {
      "error": null,
      "flags": [],
      "index": 1,
      "media": [0, 0, 61200, 79200],
      "odd": 0,
      "overprint": 2,
      "segments": [
        { "cells": 16, "multi_run": true, "text": "...", "trail": 0,
          "x0": 10800, "x1": 22320, "y": 8000 }
      ]
    }
  ],
  "pdf_sha256": "...",
  "pitch_x": 700,
  "schema": 1,
  "tool": "refpdf 0.1"
}
```

| Field | Meaning |
| --- | --- |
| `pitch_x` | Detected pitch in 1/100 pt, or `null`. Segments fall back to 7.2 pt without one. |
| `media` | The MediaBox as stored, in 1/100 pt, or `null` if unreadable. |
| `y` | Baseline y from the top of the page. |
| `x0` | Origin of the first non-blank glyph. Leading drawn spaces are dropped. |
| `x1` | Origin plus advance width of the last non-blank glyph. |
| `text` | Text with rebuilt spaces, without leading or trailing blanks. |
| `cells` | `round((x_last - x0) / pitch) + 1`: character cells spanned. It differs from the text length when a cell holds a combining mark. |
| `trail` | Spaces after the last non-blank glyph up to the last space the PDF drew there (drawn spaces plus any rebuilt gaps between them). Final Draft draws trailing spaces on many wrapped lines. |
| `multi_run` | True when the segment's glyphs came from more than one run. |
| `flags` | Constructs pdf-extract mishandles (see below), sorted. |
| `error` | Why the page has no segments, or `null`. |

## The page-view grid

`grid` prints the same "page view" the layout engine prints, so that the two
can be compared byte for byte:

- For each physical page `i`, counting from 1, a line `=== page {i} ===`.
  The layout engine's own headers carry extra text after `=== page {i}`;
  `self-check` treats any line starting with `=== page ` as a page boundary
  and ignores the rest of it.
- Then rows 0 to R, where R is the last non-empty row. A glyph goes in row
  `floor(y / line)` and column `round_half_up(x / pitch)`, using integer
  arithmetic on the 1/100 pt values. The pitch defaults to 7.2 pt and the
  line to 12 pt.
- A cell holds its glyph's text. Glyphs sharing a cell (a combining mark
  drawn at its base's x) are concatenated in drawing order. Empty cells
  render as one space, trailing spaces are trimmed, and empty rows inside
  the range stay as empty lines. Drawn spaces leave their cell empty, which
  renders the same way.
- Only upright glyphs of 10–14 pt count (odd glyphs are excluded), after
  overprints are removed.
- Pages are separated only by their header lines; a failed page is just its
  header. The output ends with a single newline.

Final Draft's PDFs measured so far use a 7.00 pt pitch, so `--pitch 7` puts
their columns where they print; the 7.2 pt default is the page view's own
10-characters-per-inch grid.

## pdf-extract caveats

We use pdf-extract 0.12.1, pinned exactly, and its re-exported lopdf 0.42.
It has well over a hundred `unwrap`, `expect` and `panic!` sites, so:

- Pages are extracted one at a time with `output_doc_page`, each inside
  `catch_unwind` with a quiet panic hook. A panic becomes the page's
  `error`, and the run continues.
- A stack overflow cannot be caught, and pdf-extract recurses without limit
  in two places. A preflight rejects both before pdf-extract sees the page:
  a cyclic `/Parent` chain (`get_inherited`) and a Form XObject that draws
  itself (`Do`). The work also runs on a thread with a 256 MiB stack, for
  deep but finite nesting. Other unbounded recursion inside pdf-extract or
  lopdf would still abort the process.
- The preflight decodes the content strictly (`Content::decode_strict`) and
  flags what pdf-extract mishandles, so you know when a page's glyphs cannot
  be trusted:

| Flag | What pdf-extract does |
| --- | --- |
| `text-op-quote`, `text-op-double-quote` | Ignores the `'` and `"` operators: their text is silently dropped and the line does not advance. |
| `form-xobject` | Draws a form's content with a fresh graphics state, ignoring both the caller's transformation and the form's `/Matrix`, so its text can land in the wrong place. |
| `image-xobject` | Parses the image data as if it were a content stream, which usually panics (the page fails) and can produce junk. |
| `xobject-other`, `xobject-missing` | An XObject that is neither a form nor an image, or a `Do` whose name does not resolve (usually a panic). |
| `rotate-N` | Ignores `/Rotate`; coordinates stay in the unrotated page. |
| `mediabox-origin` | The MediaBox does not start at (0, 0). pdf-extract's own outputs flip y with the box height only; refpdf measures from the box corner, but the page is unusual and worth a look. |
| `content-not-strict` | The content stream only parses leniently; pdf-extract continues with what lenient parsing recovered. |

Other limits: encrypted PDFs are tried with an empty password only, Type 3
fonts and fonts without a ToUnicode map may decode to the wrong characters,
and text clipped or painted over is still extracted.

## Tests

`cargo test` runs unit tests on synthetic glyph lists, an end-to-end test
that builds a small PDF with the standard Courier font (including pages
built to make pdf-extract panic or recurse forever), a test that damages
that PDF 200 ways and checks nothing crashes, and a corpus test. The corpus
test reads `corpus/fd11` (or `REFPDF_CORPUS`) and skips cleanly when it is
missing.

```sh
cargo fmt --manifest-path tools/refpdf/Cargo.toml --all
cargo clippy --manifest-path tools/refpdf/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path tools/refpdf/Cargo.toml --locked
```
