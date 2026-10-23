# .fdx: how Cold Open reads and writes it

- **Date:** 2026-10-13, updated 2026-10-14, 2026-10-16 (phase 4: the writer, passthrough, round trip and repair, §12 to §17), 2026-10-17 (the corpus downloads, and how the markup style is read, §13.1) and 2026-10-18 (phase 5a: tracked changes are written as their accepted form, §15)
- **Status:** the phase 2 reader is implemented in `crates/fdx` (package `coldopen-fdx`). Phase 4 (2026-10-16) adds the writer, the passthrough store, resyncing after syntax errors and the repair report; they are being implemented.
- **Sources:**
  - Seven publicly shared .fdx files from the MIT-licensed repository <https://github.com/SimonGZ/jumpcut>, path `tests/fixtures/corpus/public/<name>/source/source.fdx`, downloaded 2026-10-13. They are kept in the gitignored `corpus/` folder and identified by the first 12 hex digits of their sha256:

    | Name | Local copy | sha256 prefix |
    | --- | --- | --- |
    | big-fish | `corpus/fd11/big-fish/big-fish.fdx` | 4b9269b70a43 |
    | big-fish-scene-numbers | `corpus/fd11/big-fish-scene-numbers/big-fish-scene-numbers.fdx` | c2c32fd9094e |
    | little-women | `corpus/fd11/little-women/little-women.fdx` | 823fd47b73a0 |
    | extranormal | `corpus/fdx-samples/extranormal.fdx` | 5e0dc47ebb41 |
    | gumshoe | `corpus/fdx-samples/gumshoe.fdx` | 716fd2465753 |
    | mostly-genius | `corpus/fdx-samples/mostly-genius.fdx` | c3e72174457e |
    | vikings | `corpus/fdx-samples/vikings.fdx` | 7fb46b306560 |

  - The six samples of the AGPL repository rsdoiel/fdx, read locally and never copied into this repository: `https://raw.githubusercontent.com/rsdoiel/fdx/main/testdata/sample-0N.fdx` for N = 1 to 6, downloaded 2026-10-13. sha256 prefixes: 6bda3c386458, 717949f5fb55, e4495f4878c9, f507658c1896, 1834073d2591, 1c5fbaa7f24c.
  - (2026-10-16) The three .fdx files of the reference pairs from fountain.io (`docs/specs/layout.md` §14), kept in `corpus/fountain-io/`: Big Fish (daf2e979d0e8), Brick & Steel (c242f797a33b) and The Last Birthday Card (19f11c08dd91). They were saved by Final Draft in 2012 (`Version="1"`). They were studied for phase 4 the same way, with scripts that print element paths, attribute names in source order, whitespace shapes and counts, never text. Brick & Steel is the only sample with dual dialogue and the only one with a `Revisions` list and `ScriptNoteDefinitions`; no sample has an anchored script note or a run with a revision mark.
  - (2026-10-16) Further public files asked of the project's owner for the round-trip corpus (§16): the seven jumpcut files above (no longer on the build machine), files saved by Fade In and by other libraries (wildwinter/screenplay-tools and vilcans/screenplain, both MIT), and two more rsdoiel files (rsdoiel/scripttool, AGPL, local only).
  - (2026-10-17) Arrived, downloaded on Windows, so every file has CRLF line breaks where the published files have LF. With CRLF turned back into LF, the seven jumpcut corpus files give exactly the sha256 prefixes in the table above. Studied the same way, structure only. sha256 prefixes of the copies on disk:
    - `corpus/jumpcut/` (MIT): the seven corpus files above, and six of jumpcut's own .fdx import fixtures. `corpus_public_big-fish-scene-numbers_source_source.fdx` 92c84d000af1; `corpus_public_big-fish_source_source.fdx` 31ef7ce42793; `corpus_public_extranormal_source_source.fdx` 3d78096e89b1; `corpus_public_gumshoe_source_source.fdx` ac017faae392; `corpus_public_little-women_source_source.fdx` a15864b9d01e; `corpus_public_mostly-genius_source_source.fdx` 7f8e822555d7; `corpus_public_vikings_source_source.fdx` c93a11883f05; `fdx-import_brick-n-steel-basic.fdx` 7eacb5640f30; `fdx-import_dual-dialogue-basic.fdx` ae9a6d2201a1; `fdx-import_paragraph-layout-overrides.fdx` ea5889d528d2; `fdx-import_settings-normalized.fdx` b438ec0e66f3; `fdx-import_title-page-cast-page.fdx` d52d91bed750; `fdx-import_title-pages-multi.fdx` 1196c4873fa1.
    - `corpus/screenplain/` (MIT): the nine test files and three examples (the fountain.io scripts as screenplain writes them, `Version="1"`). `examples_Big-Fish.fdx` 63e892ad89d6; `examples_Brick-and-Steel.fdx` 5a751f3f4b78; `examples_The-Last-Birthday-Card.fdx` ccf22d92d928; `tests_files_dialogue-with-blank-line.fdx` d4ac3a46d784; `tests_files_dialogue.fdx` 609c90ca047a; `tests_files_dual-dialogue.fdx` a47dad43431a; `tests_files_extended-characters.fdx` 8dc289e57754; `tests_files_forced-action.fdx` 381360c5fd01; `tests_files_indentation.fdx` e59c70a79c60; `tests_files_parenthetical.fdx` 58cc33795409; `tests_files_simple.fdx` 5b61aab29417; `tests_files_utf-8-bom.fdx` 5b61aab29417. The `utf-8-bom` and `simple` files are the same bytes: the BOM was in screenplain's Fountain input, not its .fdx output.
    - Not arrived: wildwinter/screenplay-tools and rsdoiel/scripttool.
  - Extensible Markup Language (XML) 1.0, <https://www.w3.org/TR/xml/>, for the five predefined entities, character references and line-end handling.
  - The WHATWG Encoding Standard's windows-1252 index, <https://encoding.spec.whatwg.org/index-windows-1252.txt>, for the fallback decoding in §2.1.
  - `docs/specs/layout.md` and the types in `crates/screenplay`, which define what the reader must produce.
- **Method.** The sample files were studied with small scripts that print element paths, attribute names, value vocabularies and counts, never script text. No script text or sample boilerplate is copied into this repository.
- No Final Draft software, manual, help text or website text was used. Everything here is written in our own words. A choice we made where the samples are silent is marked **Resolution**.

## 1. Goals

The reader:

1. **Read every file.** Any bytes produce either a document or one of two errors: the input is not XML at all, or its root element is not `FinalDraft`. Everything else is recovered and reported. The reader never panics.
2. **Report, never drop silently.** Whatever the reader cannot carry into the model is listed in the report with a stable code (§9).
3. **Same result everywhere.** The result depends only on the input bytes. Lengths are parsed with integer arithmetic, and nothing depends on the OS, locale or hash-map order.
4. **Feed layout.** Phase 2 needs the script, the page format, per-element spacing overrides and the title page, so that `crates/layout` can paginate .fdx files and compare them with reference PDFs (`docs/specs/layout.md` §13).

The writer (phase 4, §12 to §16):

5. **Lossless round trip.** A file that reads with no repair and is written back unedited comes back byte for byte. Everything Cold Open does not model passes through untouched, in place (§13).
6. **Edits stay local.** Editing a script changes only the paragraphs that were edited; the rest of the file, settings and unknown sections included, is written back as it was.
7. **Report what cannot be written.** What the model holds and .fdx cannot is listed as a loss before anything is written (§15), as for Fountain.
8. **Our own words.** A file Cold Open creates from scratch uses a settings template written from our own page format (§14.6), never copied from a sample.

## 2. Input

### 2.1 Decoding

The bytes are turned into text before any XML parsing:

1. A UTF-8 byte-order mark (EF BB BF) is removed, and the rest is read as UTF-8.
2. A UTF-16 byte-order mark (FF FE little-endian, FE FF big-endian) selects UTF-16 and is removed. Without a mark, a file starting with `<` followed by a zero byte is UTF-16 little-endian, and one starting with a zero byte followed by `<` is big-endian. UTF-16 input is reported (`encoding_utf16`).
3. Otherwise the bytes are read as UTF-8 if they are valid UTF-8.
4. Otherwise, if the XML declaration in the first 256 bytes names `windows-1252`, `cp1252`, `iso-8859-1` or `latin1` (any case), the bytes are decoded as windows-1252 (`encoding_windows_1252`). ISO-8859-1 is read as windows-1252, as browsers do.
5. Otherwise invalid sequences are replaced with U+FFFD (`encoding_lossy`). Unpaired UTF-16 surrogates and a stray final byte in UTF-16 are handled the same way.

The declaration's `encoding` attribute plays no other part. All public samples are UTF-8 without a byte-order mark; ten declare `UTF-8` and three `utf-8`.

### 2.2 Tolerant XML

- The text is parsed by quick-xml 0.42 with end-name checks off, unmatched end tags allowed, lone `&` allowed and no trimming. The reader keeps its own stack of open elements.
- **Mismatched end tag.** An end tag that does not match the innermost open element closes every element up to the nearest open element with that name (`mismatched_end_tag`). If no open element has that name, the tag is ignored (`stray_end_tag`).
- **Unclosed elements** at the end of the input are closed and listed (`unclosed_element`).
- **Syntax error.** (2026-10-16) The reader skips to the next `<` that can begin markup (a start tag, an end tag, a comment, a CDATA section or a processing instruction) and carries on with the elements it has open, reporting the bytes it skipped (`skipped_bytes`, with their line, column and length). Before phase 4 it stopped at the first error. After 1,000 such errors it stops and keeps everything built so far (`truncated_xml`), so a file of noise is read in time proportional to its length. If no element has been seen before the first error and none can be found after it, the input is not XML.
- **Depth.** Elements nested more than 64 deep are dropped with their content (`too_deep`). No real file comes close: the deepest path in the samples is 8 elements.
- **After the root.** Elements after the root element closes are ignored (`content_after_root`).
- A self-closing element is treated like a start tag followed by its end tag. Comments, processing instructions and the document type declaration are not read, but they are kept for writing back (§13).
- **Positions** (2026-10-16). The reader records where every element starts and ends in the decoded text, and the line and column (1-based, counting characters) of every issue it reports at the XML level.

### 2.3 Text content

- Text, character references, entity references and CDATA sections inside an element are joined in order.
- The five predefined entities (`lt`, `gt`, `amp`, `apos`, `quot`) and decimal or hexadecimal character references are replaced by their characters. Any other entity is kept literally as `&name;` (`unknown_entity`), and so is a character reference that names no valid character (`bad_char_ref`).
- Line ends are normalized as XML 1.0 requires: CR LF and a lone CR become LF. Nothing else changes. **Text is never trimmed**: leading, trailing and repeated spaces are part of the script.

### 2.4 Attributes

- Attribute values are normalized as XML requires: references are replaced, and tab, CR and LF become spaces.
- A value with a reference that cannot be resolved is used as written (`attribute_value`).
- A malformed attribute is skipped (`attribute_syntax`).
- When an attribute is repeated on one element, the first value is kept (`duplicate_attribute`).

## 3. Document structure

The reader relies on this structure. Names are matched exactly unless stated otherwise. Only the first occurrence of each section below is read; later ones are reported (`duplicate_section`). Other top-level elements are listed once (`sections_ignored`).

- `FinalDraft` (root): attributes `DocumentType`, `Template`, `Version`. The samples use version 3 (rsdoiel) and 4 (jumpcut). The order of the root's children varies between files.
  - `Content`: the script, a list of `Paragraph` elements.
    - `Paragraph`: attribute `Type`; optional `Number` (scene number); optional inline paragraph attributes (§6.4).
      - `Text`: one run of text. Optional attributes `Style`, `Font`, `Size`, `Color`, `Background`, `AdornmentStyle`, `RevisionID`.
      - `SceneProperties`: on scene headings, attributes `Page`, `Length`, `Title`, and children we don't read.
      - `DualDialogue`: holds nested `Paragraph` elements (§6.6).
  - `ElementSettings` (one per paragraph type, attribute `Type`):
    - `ParagraphSpec`: `Alignment`, `FirstIndent`, `Leading`, `LeftIndent`, `RightIndent`, `SpaceBefore`, `Spacing`, `StartsNewPage`.
    - `FontSpec`: `Font`, `Size`, `Style`, plus colors and revision attributes.
    - `Behavior`: `PaginateAs`, plus editor keys we don't read.
  - `PageLayout`: `TopMargin`, `BottomMargin`, `HeaderMargin`, `FooterMargin`, `BreakDialogueAndActionAtSentences`, `DocumentLeading`, `UsesSmartQuotes` and colors. Child `PageSize` with `Width` and `Height`.
  - `MoresAndContinueds`: child `DialogueBreaks` with `AutomaticCharacterContinueds`, `BottomOfPage`, `DialogueBottom`, `DialogueTop`, `TopOfNext`; child `SceneBreaks` with `ContinuedNumber`, `SceneBottom`, `SceneBottomOfPage`, `SceneTop`, `SceneTopOfNext`.
  - `SceneNumberOptions`: `LeftLocation`, `RightLocation`, `ShowNumbersOnLeft`, `ShowNumbersOnRight`, `NumberScheme`.
  - `HeaderAndFooter`: `HeaderVisible`, `HeaderFirstPage`, `FooterVisible`, `FooterFirstPage`, `StartingPage`. Children `Header` and `Footer`, each with one `Paragraph` holding `Text` runs, `DynamicLabel` elements (attribute `Type`) and `Tabstops`.
  - `TitlePage`: child `Content` with `Paragraph` elements (no `Type`, full inline paragraph attributes, `Text` runs), plus `HeaderAndFooter` and `TextState`, which we don't read.

A **paragraph index** in the report is the 0-based position of a `Paragraph` among the children of the first `Content`, in document order. Paragraphs nested in `DualDialogue` share their outer paragraph's index.

## 4. Units and values

| Value | Unit | Parsed by |
| --- | --- | --- |
| `LeftIndent`, `RightIndent`, `FirstIndent` | inches from the paper's left edge (`FirstIndent` is relative to `LeftIndent`) | `Pt::parse_inches` |
| `PageSize` `Width`, `Height` | inches | `Pt::parse_inches` |
| `SceneNumberOptions` `LeftLocation`, `RightLocation` | inches from the paper's left edge | `Pt::parse_inches` |
| `TopMargin`, `BottomMargin`, `HeaderMargin`, `SpaceBefore` | points | `Pt::parse_points` |
| `Spacing` | a plain decimal, 1 = single | `Pt::parse_points`, read as hundredths |
| `Size` | points | `Pt::parse_points` |

- **Integers only.** Both parsers work on the decimal text and never use a binary float (`docs/specs/layout.md` §2).
- **Rounding.** An inch value with more precision than a `Pt` holds (1/7200 in) is rounded half away from zero (`value_rounded`). A points value with more than two decimals is rounded to two the same way, by integer arithmetic on its digits (`value_rounded`).
- **Failures.** A value that does not parse is reported (`invalid_value`), and the field keeps its default.
- **Yes and No** are matched exactly. Anything else is `invalid_value`.
- **Alignment** is `Left`, `Center`, `Right` or `Full`, matched exactly.
- **Colors** are `#` followed by hex digits; the samples use 12 digits, 16 bits per channel. Black is all zeros and white is all `F`, in any case and at any length.
- **Style** is a list of tokens joined by `+`, in any order, such as `Bold+Underline+AllCaps` or `AllCaps+Bold`. The empty string means no style. Tokens are matched ignoring ASCII case, and empty tokens are skipped.

Observed values: indents with two decimals (`1.50`, `2.56`, `3.38`, `-0.10`, `-0.14`); margins as whole points (36, 62, 72, 90); `Spacing` 1 and 2; `Leading` always `Regular`; `DocumentLeading` always `Normal`; `Size` always 12.

## 5. The page format

The reader starts from `PageFormat::default()` (`docs/specs/layout.md` §3.2) and changes only the fields below. A missing section or attribute keeps the default. Missing `PageLayout`, `MoresAndContinueds`, `SceneNumberOptions` and `HeaderAndFooter` sections are listed once (`section_missing`). Three of the seven jumpcut files have no `PageLayout`, and only one of them has `SceneNumberOptions`.

### 5.1 Page settings

| Source | Field | Rule |
| --- | --- | --- |
| `PageLayout` `TopMargin` | `margin_top` | points |
| `PageLayout` `BottomMargin` | `margin_bottom` | points |
| `PageLayout` `HeaderMargin` | `header_top` | points |
| `PageSize` `Width`, `Height` | `page_width`, `page_height` | inches |
| `BreakDialogueAndActionAtSentences` | `break_at_sentences` | Yes/No |
| `DocumentLeading` | none | anything but `Normal` is reported (`document_leading`) |
| `DialogueBreaks` `BottomOfPage`, `DialogueBottom` | `more` | `Some(DialogueBottom)` when `BottomOfPage` is Yes, else `None` |
| `DialogueBreaks` `TopOfNext`, `DialogueTop` | `contd` | `Some(DialogueTop)` when `TopOfNext` is Yes, else `None` |
| `DialogueBreaks` `AutomaticCharacterContinueds` | `contd_after_action` | Yes/No |
| `SceneBreaks` `SceneBottomOfPage`, `SceneTopOfNext` | `scene_continueds` | see below |
| `SceneNumberOptions` `LeftLocation`, `RightLocation` | `scene_number_left`, `scene_number_right` | inches |
| `HeaderAndFooter` | `page_numbers.visible`, `.first`, `.suffix` | §5.3 |

- **Insert strings** are kept exactly as written, including case: the rsdoiel samples use a lower-case continued marker. A string that is empty or only spaces keeps the default string (`invalid_value`), and a missing one keeps it silently, so a format never fails validation for that reason alone.
- **Resolution (2026-10-14):** the format allows an insert string of one line and at most 32 characters (`MAX_INSERT` in `crates/screenplay`, enforced by `PageFormat::problems()`). A longer string is cut down to fit: first to the text before its first line break (LF or CR, which can only arrive as a character reference), then to its first 32 characters, and the cut is reported (`invalid_value`). If nothing but spaces is left, the default string is kept, as for an empty one. Either way the format stays valid, so a long marker never costs the file its other settings. Every marker in the samples kept in `corpus/` has 6 or 8 characters.
- **Scene continueds.** When `SceneBottomOfPage` or `SceneTopOfNext` is Yes, `scene_continueds` is set, with `bottom` = `SceneBottom` when `SceneBottomOfPage` is Yes and `top` = `SceneTop` when `SceneTopOfNext` is Yes, and the reader reports `scene_continueds_unsupported`: layout does not print them yet (`docs/specs/layout.md` §16). `ContinuedNumber` is mentioned in that report when Yes.
- `ShowNumbersOnLeft` or `ShowNumbersOnRight` = No is reported (`scene_number_side`); our scene numbers print on both sides when they print at all.
- `FooterMargin`, `UsesSmartQuotes`, `NumberScheme`, colors and the cast-list settings are not read.

### 5.2 Element settings

Each `ElementSettings` whose `Type` names one of our nine printing kinds (§6.1, ignoring ASCII case) sets that kind's `ElementFormat`:

| Source | Field | Rule |
| --- | --- | --- |
| `LeftIndent` | `left` | inches |
| `RightIndent` | `right` | inches |
| `FirstIndent` | `first_indent` | inches |
| `Alignment` | `align` | `Full` is read as `left` (`alignment_full`) |
| `SpaceBefore` × `Spacing` | `space_before` | points; a `Spacing` other than 1 multiplies the space and is reported (`line_spacing`) |
| `StartsNewPage` | `starts_new_page` | Yes/No |
| `FontSpec` `Style` | `caps`, `bold`, `italic`, `underline` | tokens `AllCaps`, `Bold`, `Italic`, `Underline`; other tokens are `unknown_style` |
| `Behavior` `PaginateAs` | `paginate_as` | a type name from §6.1; anything else keeps the kind itself (`unknown_paginate_as`) |

- **Resolution:** `Spacing` most likely sets the spacing between lines of a paragraph, which our model has no field for. Multiplying the space above is the closest we can do; it is reported so nobody relies on it.
- **Resolution (2026-10-14):** the product is worked out exactly, in integers wide enough that no pair of parsed values can overflow on the way, and rounded half away from zero. When the result does not fit a length (more than about 21 million points either way), `Spacing` is not applied: the space above stays as written, and the `Spacing` is reported as out of range (`invalid_value`) instead of as `line_spacing`. A product that fits but lies below 0 or above the body height fails validation like any other bad spacing (§5.4).
- `cells` is never set; layout computes it from the column.
- A `FontSpec` whose `Font` does not contain `Courier`, or whose `Size` is not 12, is listed once (`settings_font_ignored`). We always set Courier Prime at the format's size.
- Settings for other types (`Cast List`, `New Act`, `End of Act`, `End Of Act`, `Cold Opening`, `Show/Ep. Title` in the samples) are ignored and listed once (`settings_for_unmodelled_type`). They are still used as the baseline for inline attributes (§6.4).
- A repeated `Type` keeps its first settings (`duplicate_settings`).
- A kind with no settings keeps its default, and all such kinds are listed once (`settings_missing`). The rsdoiel samples have no `Lyric` settings, for example.

Observed: scene headings use 24 pt above in all but one file, and dialogue, parentheticals and general text 0 pt; lyrics paginate as dialogue and shots as scene headings; one file sets `Spacing` 2 on dialogue, with `SpaceBefore` 0.

### 5.3 Header and page numbers

- `page_numbers.visible` is true when `HeaderVisible` is Yes and the header paragraph holds a `DynamicLabel` whose `Type` is `Page #`.
- `page_numbers.first` is 1 when `HeaderFirstPage` is Yes, else 2.
- **Resolution:** `page_numbers.suffix` is the text that directly follows the page-number label in the header paragraph, up to the next tab, line break, label or the end of the paragraph. Every sample has `.` there, which is also our default.
- **Resolution (2026-10-14):** the format allows a suffix of one line and at most 16 characters (`MAX_PAGE_SUFFIX` in `crates/screenplay`, enforced by `PageFormat::problems()`). A longer suffix is cut to the text before any CR it holds, then to its first 16 characters, and the cut is reported (`invalid_value`). The format stays valid; an empty suffix is allowed.
- `StartingPage` other than 1 is reported (`starting_page`) and not applied.
- Other labels and other text in the header are reported once (`header_content_ignored`). The samples hold tabs and a revision label before the page number.
- A visible footer with a label or text is reported (`footer_ignored`).
- **UNVERIFIED:** header geometry. The header paragraph carries indents (`LeftIndent` 1.25, `RightIndent` −1.00 or −1.25 in the samples), alignments and tab stops whose meaning we have not confirmed against a PDF. The reader does not interpret them, and `page_numbers.right` keeps its default of 7.5 in.

### 5.4 Validation

If the finished format fails `PageFormat::problems()` (`docs/specs/layout.md` §3.3), it is replaced by `PageFormat::default()` and the problem list is reported (`invalid_format`). The rest of the document is still read.

- Validation covers the whole range a file can write. Margins or a page size at the limits of a length (about ±21 million points, or ±298,000 inches) fail it like any other bad value; they never add up to a body that wraps around to a plausible height.
- Values the reader can deal with on its own are handled where they are read, and reported (`invalid_value`), so they never cost the file its other settings: insert strings (§5.1), the page-number suffix (§5.3), paragraph space overrides (§6.4) and title-page spacing (§7).

## 6. The script

### 6.1 Paragraph types

| `Type` | Element kind |
| --- | --- |
| `Scene Heading` | `scene_heading` |
| `Action` | `action` |
| `Character` | `character` |
| `Parenthetical` | `parenthetical` |
| `Dialogue` | `dialogue` |
| `Transition` | `transition` |
| `Shot` | `shot` |
| `General` | `general` |
| `Lyric` | `lyric` |
| anything else, or no `Type` | `general`, reported once per type name (`unknown_paragraph_type`) |

Types are matched ignoring ASCII case and surrounding spaces. In the samples, `Content` paragraphs also use `New Act`, `End of Act` and `End Of Act`. Every `Content` paragraph in the samples has a `Type`; paragraphs without one appear only on the title page, in the header and footer, and inside scene properties.

### 6.2 Text runs

- A paragraph's text is its `Text` children in order, joined with `RichText::new`, which merges neighbouring runs of equal style and drops empty ones.
- **Style tokens.** `Bold`, `Italic` and `Underline` set the run's style. `AllCaps` upper-cases that run's text at read time, using the Unicode mapping, and is reported once with a count (`run_all_caps`). Any other token is reported once per token (`unknown_style`).
- **Other run attributes** are ignored. Each of these is reported once per code, with a count: a `Font` that does not contain `Courier` (`run_font`), a `Size` other than 12 (`run_size`), a `Color` that is not black (`run_color`), a `Background` that is not white (`run_background`), a non-zero `AdornmentStyle` (`run_adornment`) and a non-zero `RevisionID` (`run_revision`).
- **Line breaks** inside a paragraph are literal LF characters inside a `Text` run, and stay `\n` in our text. **Tabs** are kept as tab characters and reported once (`tab_character`); layout removes them before measuring (`docs/specs/layout.md` §2).
- Unknown children of a paragraph or a run are reported once per name (`element_ignored`). `Tabstops` children are ignored without a report: tab stops do nothing without tabs.

### 6.3 Scene numbers and scene properties

- `Number` on a scene heading becomes `scene_number`, trimmed of surrounding spaces. If the result breaks `Element::problems()`, for instance because it is empty, it is dropped (`invalid_scene_number`). `Number` on any other kind is dropped (`scene_number_ignored`).
- The first `SceneProperties` of a scene heading becomes a `SceneProps` entry pointing at the heading's element index, with `page`, `length` and `title` kept as the raw attribute strings. The samples store `Page` as a page number and `Length` as eighths of a page, such as `3/8`. `SceneProperties` anywhere else is reported (`scene_properties_ignored`).

### 6.4 Inline paragraph attributes

A `Paragraph` may carry its own `Alignment`, `FirstIndent`, `Leading`, `LeftIndent`, `RightIndent`, `SpaceBefore`, `Spacing` and `StartsNewPage`. In the samples a paragraph carries either all eight, as a full copy of its type's `ParagraphSpec` with some values changed, or only one or two of them. Each attribute present is compared with the **baseline**: the `ParagraphSpec` of the paragraph's `Type` in this file (matched exactly, then ignoring ASCII case). Where the file has no such settings, the baseline is the default format of the paragraph's kind, with `Leading` Regular and `Spacing` 1. Values are compared after parsing, so `1.5` equals `1.50`, and `Full` equals `Left`.

| Difference | Effect |
| --- | --- |
| `Alignment` `Center` on an action paragraph whose baseline is not centered | `Element.centered` |
| `SpaceBefore` from 0 to the body height | `overrides[element index]` = the paragraph's space, which layout uses before its usual spacing (`docs/specs/layout.md` §6) |
| `SpaceBefore` below 0 or above the body height | no override; the paragraph keeps its type's spacing (`invalid_value`, with the paragraph index) |
| `StartsNewPage` | §6.5 |
| any other difference (indents, `Leading`, `Spacing`, other alignments) | reported once per paragraph, listing the attributes (`paragraph_format_ignored`) |

- **Resolution (2026-10-14):** an override is held to the same bound as a type's own `space_before` (`docs/specs/layout.md` §3.3): from 0 to the body height of the format the reader returns, both ends included. The body height is the page height less the top and bottom margins, taken after §5.4, so a file whose format was replaced is checked against the default body. A negative space would pile paragraphs onto the same line and a huge one would push a paragraph past every page, so neither becomes an override; the paragraph is spaced like the rest of its type and the value is reported with its paragraph index. Only a value that differs from the baseline is checked, since an equal one makes no override.

`Element.space_before` is never set by this reader: its blank-line count belongs to Fountain, and .fdx spacing goes through `space_before` in the format and the overrides.

Observed differences: centered action, `StartsNewPage` Yes on action and scene headings, and 12 pt above the outer paragraph of dual dialogue (§6.6).

### 6.5 Page breaks

- `StartsNewPage="Yes"` on a paragraph inserts a `page_break` element before it, unless nothing has been emitted yet. Layout ignores breaks before the first printing element anyway (`docs/specs/layout.md` §4.5).
- **Resolution:** for a paragraph whose type is not one of our nine kinds, a missing inline `StartsNewPage` falls back to that type's own settings. Act breaks, which start a new page in the samples' settings, therefore keep their page breaks even though they are read as general text.
- For our nine kinds, the type's own `StartsNewPage` is in the format (§5.2). An inline `No` against a baseline of Yes cannot be expressed and is `paragraph_format_ignored`.

### 6.6 Dual dialogue

Observed: the outer paragraph has `Type` `General`, all eight inline attributes with 12 pt above, the `DualDialogue` element as its first child and one empty `Text` run after it. The nested paragraphs carry only a `Type`. They are always two speakers in a row: `Character`, then `Dialogue`, optionally with one `Parenthetical` before the dialogue.

- The nested paragraphs are emitted in order, as ordinary paragraphs (§6.1 to §6.4), and the second group's character gets `dual = true`. A group is a `Character` followed by at least one parenthetical, dialogue or lyric (2026-10-16: before, any number, including none; layout never paired a group with no lines, `docs/specs/layout.md` §4.4, so this changes nothing that prints, and the writer can always write back what the reader flags).
- The outer paragraph's own type, spacing and other inline attributes are ignored. **Resolution:** its `StartsNewPage="Yes"` still inserts a page break before the block.
- If the nested paragraphs are not exactly two such groups, they are all emitted with no `dual` flag (`dual_dialogue_shape`).
- **Resolution:** text in the outer paragraph's own runs is emitted as a `general` element after the block, so nothing is lost (`dual_dialogue_text`). Text made only of whitespace is dropped, with the same report.
- `StartsNewPage` on a nested paragraph is `paragraph_format_ignored`.

### 6.7 Empty paragraphs

- A paragraph with no text becomes an element of its kind with empty text. Layout prints it as one blank line (`docs/specs/layout.md` §4.7).
- Empty `general` elements at the end of the script are dropped, with any page break left in front of them, and counted once (`trailing_empty_dropped`). Every rsdoiel sample ends with one.
- A missing `Content` gives an empty script (`no_content`).

## 7. The title page

- Each `TitlePage/Content/Paragraph` becomes a `TitleLine`, in order: `text` from its runs (§6.2), `align` from `Alignment` (`Full` is read as `left`, reported once per document as `alignment_full`), `left` from `LeftIndent`, `right` from `RightIndent`, and `space_before` from `SpaceBefore` in points.
- Empty paragraphs become empty title lines; the samples position text by stacking 40 to 50 of them, with 1 to 7 paragraphs of text in between. Line breaks inside a run stay `\n`.
- **Resolution:** a missing `LeftIndent` or `RightIndent`, or a pair that does not form a column on the page, uses the format's action column (`invalid_value` for the bad pair). A missing `Alignment` is `left` and a missing `SpaceBefore` is 0.
- **Resolution (2026-10-14):** `SpaceBefore` is clamped to the range from 0 to the body height of the format the reader returns. A negative value becomes 0 and a larger one becomes the body height, and either change is reported (`invalid_value`). No title paragraph asks for more space above it than a whole page body holds; layout drops a line that no longer fits on the title page (`docs/specs/layout.md` §9.3).
- `FirstIndent` other than 0, `Spacing` other than 1, `Leading` other than Regular, and `StartsNewPage` Yes are counted once per document (`title_format_ignored`).
- The title page's own `HeaderAndFooter` and `TextState` are not read. The title page is never numbered (`docs/specs/layout.md` §9.1).

Observed: every title paragraph carries all eight inline attributes, with `LeftIndent` 1.00 and `RightIndent` 7.50 (jumpcut) or 1.25 and 7.25 (rsdoiel), `SpaceBefore` 0, and alignment `Center`, `Left` or `Right` on paragraphs with text. `Full` appears only on empty paragraphs. A paragraph has one run, rarely two or three.

## 8. Observations for layout

These facts come from the public samples (structure only) and matter to `crates/layout`:

- **Line breaks.** Inside `Content`, line breaks occur only as LF characters in the middle of a `Text` run, in action and dialogue paragraphs (including dialogue nested in dual dialogue), never two in a row. The rsdoiel title pages have LF inside title runs. No sample as published has a CR byte; the copies downloaded on Windows on 2026-10-17 have CRLF throughout the markup (§16).
- **Tabs** occur only in header paragraphs, before `DynamicLabel` elements. No script or title text holds a tab.
- **Whitespace.** Trailing spaces at the end of a paragraph are common (over 200 paragraphs in one script), and a few paragraphs start with a space. They are kept.
- **Characters.** Curly quotes, ellipses, accented Latin letters and the degree sign appear; quotes in the XML are written as `&#x27;` and `&quot;` in the fd11 files and as raw characters elsewhere.
- **Runs.** Almost every paragraph has a single run; styled runs carry only a `Style` attribute. Run-level `AllCaps` occurs only on title pages.
- **Settings.** Margins: 72/72 pt (jumpcut, where present) and 90/62 pt (rsdoiel), header at 36 pt. Parentheticals hang their first line by 0.10 or 0.14 in.
- **Act breaks** (`New Act`, `End of Act`) are centered and capitalized by their own settings, and `New Act` starts a new page; we read them as general text (§6.1, §6.5).

## 9. Report

Each `Issue` has a stable `code`, a `class`, a message in English that never quotes script text, the paragraph index (§3) when it concerns one `Content` paragraph, and (2026-10-16) the `line` and `column` in the decoded text where the reader met it, for the XML-level codes. Issues come in this order: decoding, XML, settings, script, title page, then the counted codes (marked ‡), each once in order of first occurrence.

**Classes** (2026-10-16). Each code has one class, which says what happens to that part of the file when it is written back (§12):

| Class | Meaning | Written back |
| --- | --- | --- |
| `fixed` | The file's XML or encoding was broken and was put right; nothing the writer wrote is lost. | Corrected. The paragraph or section it is in is written from the repaired tree or generated from the model rather than copied (§13.2), so the repair reaches the file. |
| `dropped` | Part of the file could not be read at all and is not kept. | Left out. |
| `kept` | Cold Open does not use it, and keeps it. | Unchanged, in place (§13). |
| `info` | A fact about the file, or how the model reads something, including values the model cannot use (it takes its default instead). | Unchanged: the file keeps its own text. |

A file is **repaired** when its report holds any `fixed` or `dropped` issue. Only a repaired file can differ from what was read when it is written back unedited (§16).

**Resolution (2026-10-16): values the model rejects stay in the file.** A margin that does not parse, a format that fails our validation, an empty scene number: the model uses its default or leaves the value out, and the file keeps what it says. Our validation is stricter than other apps may be (`docs/specs/layout.md` §3.3), and writing our defaults over a producer's settings would change their formatting without their asking. An edited paragraph is generated from the model, so a rejected value in it is not written again (§14.4).

| Code | Class | When | Effect |
| --- | --- | --- | --- |
| `encoding_utf16` | info | the input is UTF-16 | decoded; written back as UTF-16 |
| `encoding_windows_1252` | fixed | not UTF-8, and the declaration names windows-1252 or ISO-8859-1 | decoded as windows-1252; written as UTF-8 with the declaration changed to match, which a save reports as a `reencoded` loss (§15) |
| `encoding_lossy` | dropped | invalid byte sequences | replaced with U+FFFD |
| `truncated_xml` | dropped | more than 1,000 syntax errors (§2.2) | the rest of the file is ignored |
| `skipped_bytes` | dropped | an XML syntax error, or text outside the root (§2.2) | the bytes up to the next markup are skipped |
| `invalid_char` | dropped | a character XML 1.0 does not allow in text or an attribute value: a control character other than tab, line feed and carriage return (2026-10-16); or in a comment, processing instruction or doctype (2026-10-17) | dropped from text and attribute values; a comment, processing instruction, doctype or CDATA section holding one is left out whole, because taking the character out could end it early (`<?x\0>?>` would become `<?x>?>`); an XML declaration holding one is replaced by `<?xml version="1.0"?>`, which is right for UTF-8 and UTF-16 alike; one inside an element, where no declaration belongs, is left out, and the elements around it are written from the tree (2026-10-17, found by the `fdx_roundtrip` fuzz target); a character reference to one is `bad_char_ref` |
| `mismatched_end_tag` | fixed | an end tag closes other open elements first | those elements are closed |
| `stray_end_tag` | fixed | an end tag with no open element of that name | ignored |
| `unclosed_element` | fixed | elements still open at the end | closed |
| `too_deep` | dropped | nesting deeper than 64 | the deeper elements are dropped |
| `content_after_root` | dropped | elements after the root closes | ignored |
| `unknown_entity` ‡ | kept | an entity other than the predefined five, per name | kept as written |
| `bad_char_ref` ‡ | kept | a character reference to no valid character | kept as written |
| `attribute_syntax` | dropped | a malformed attribute | skipped |
| `attribute_value` | kept | an attribute value with an unresolvable reference | used as written |
| `duplicate_attribute` | dropped | an attribute repeated on one element | first kept |
| `sections_ignored` | kept | top-level elements we don't read | listed once; written back unchanged |
| `duplicate_section` | kept | a second `Content`, `PageLayout` or other section we read | first read; the second is written back unchanged |
| `section_missing` | info | `PageLayout`, `MoresAndContinueds`, `SceneNumberOptions` or `HeaderAndFooter` absent | defaults kept; nothing is added on writing |
| `invalid_value` | info | a value that does not parse or is out of range, or an unusable string or column | default kept, or the value cut or clamped to its range (§5.1, §5.3, §7); the file keeps its text |
| `value_rounded` | info | a value with more precision than a `Pt` | rounded in the model; the file keeps its text |
| `settings_for_unmodelled_type` | kept | settings for types we don't model | listed once |
| `duplicate_settings` | kept | a repeated settings `Type` | first read |
| `settings_missing` | info | kinds with no settings | defaults kept, listed once |
| `alignment_full` | info | `Full` alignment in settings, or on the title page (‡) | read as left; the file keeps `Full` |
| `line_spacing` | info | `Spacing` other than 1 in settings | space above multiplied |
| `unknown_paginate_as` | info | an unknown `PaginateAs` | the kind itself |
| `settings_font_ignored` | kept | a non-Courier font or a size other than 12 in settings | ignored |
| `document_leading` | kept | `DocumentLeading` other than Normal | ignored |
| `scene_continueds_unsupported` | info | scene continueds switched on | stored, not printed |
| `scene_number_side` | kept | scene numbers hidden on one side | ignored |
| `starting_page` | kept | `StartingPage` other than 1 | ignored |
| `header_content_ignored` | kept | header labels or text besides the page number | ignored |
| `footer_ignored` | kept | a visible footer with content | ignored |
| `invalid_format` | info | the format fails validation | default format used for layout; the file keeps its settings |
| `no_content` | info | no `Content` element | empty script |
| `unknown_paragraph_type` ‡ | kept | a type outside §6.1, per type name | read as general; the type name is written back while the block stays general |
| `invalid_scene_number` | info | a scene number that breaks the model rules | dropped from the model; the file keeps it until the paragraph is edited |
| `scene_number_ignored` | kept | `Number` on a kind other than a scene heading | not read; written back |
| `scene_properties_ignored` | kept | `SceneProperties` not on a scene heading | not read; written back |
| `run_all_caps` ‡ | info | runs styled `AllCaps` | upper-cased in the model; the file keeps `AllCaps` and its own case |
| `unknown_style` ‡ | kept | an unknown style token, per token | ignored |
| `run_font` ‡, `run_size` ‡, `run_color` ‡, `run_background` ‡, `run_adornment` ‡, `run_revision` ‡ | kept | run attributes we don't model (§6.2) | ignored; carried onto edited text (§14.4) |
| `tab_character` ‡ | info | tab characters in script or title text | kept |
| `element_ignored` ‡ | kept | an unexpected element inside `Content`, a paragraph, a run or `DualDialogue`, per name | not read; written back in place |
| `paragraph_format_ignored` | kept | inline attributes we cannot express (§6.4) | not read; written back |
| `dual_dialogue_shape` | info | dual dialogue that is not two speaker groups | read without the dual flag |
| `dual_dialogue_text` | info | text in the outer dual-dialogue paragraph | emitted after the block |
| `trailing_empty_dropped` | kept | empty general paragraphs at the end | left out of the model; written back |
| `title_format_ignored` ‡ | kept | title paragraph attributes we cannot express (§7) | not read; written back |

**The report's two forms** (2026-10-16; decided with the project's owner). For people, plain text, one issue per line: its class, where it is (`line 120, column 7`, or `paragraph 42`), the message and the code in brackets, `fixed` and `dropped` first, then a one-line summary of the `kept` and `info` issues by count. For tools, JSON:

```json
{"repaired": true, "issues": [{"code": "mismatched_end_tag", "class": "fixed", "line": 120, "column": 7, "message": "an end tag </Text> closed 2 elements that were left open"}]}
```

`paragraph`, `line` and `column` appear only when known. Neither form ever quotes script text: messages name elements, attributes and positions only.

## 10. Tolerance rules

1. Only "not XML at all" (no element anywhere in the input) and "the first element is not `FinalDraft`" are errors.
2. Bad bytes are decoded with a report (§2.1); bad XML is repaired, or skipped up to the next markup, with a report (§2.2).
3. A missing section, element or attribute keeps the default. A value that does not parse keeps the default and is reported.
4. Unknown elements and attributes never stop reading. What they would have changed is reported where we know it matters, and silently ignored otherwise (editor state such as `SmartType`, `WindowState` or `Macros` is listed under `sections_ignored`). Since phase 4 every one of them is kept and written back (§13).
5. Text is never trimmed, re-cased (except `AllCaps`) or re-wrapped.
6. Every element the reader produces satisfies `Element::problems()`, and the format satisfies `PageFormat::problems()`. Every override and every title line's space above lies between 0 and that format's body height. The fuzz checks (`coldopen_fdx::fuzzing::read`) test all of these.
7. **Time.** Reading takes O(n log n) time in the size of the input. Every lookup by name, such as a counted code's key, a section, a settings type, a header label or an attribute, goes through an ordered map or set, never a scan of everything seen so far. A file of tens of thousands of distinct names reads about as fast as one of the same size that repeats a single name.
8. **Writing takes O(n log n) time** too (2026-10-16), whatever the file holds and however it was edited: matching blocks to kept paragraphs goes through ordered maps keyed by block id, and carrying run attributes onto edited text (§14.4) compares the old and new text once from each end. `crates/fdx/tests/hostile_time.rs` checks that four times the input takes under nine times as long, for reading and writing, and the fuzz runs fail any input over two seconds.

## 11. Not observed in the public samples

Implemented from our own reasoning, and to be checked when a sample shows them:

- `BreakDialogueAndActionAtSentences` = No, `DocumentLeading` other than Normal, `StartingPage` other than 1, `HeaderFirstPage` = Yes, a visible footer.
- `SceneBottomOfPage`, `SceneTopOfNext` or `ContinuedNumber` = Yes.
- `ShowNumbersOnLeft` or `ShowNumbersOnRight` = No.
- Non-zero `RevisionID` on a run, run fonts other than Courier, sizes other than 12, colored text or backgrounds. (`AdornmentStyle` −1 occurs on a few title-page runs.)
- `AllCaps` on a run inside `Content`; style tokens other than the four we know.
- `Shot` or `Lyric` paragraphs in `Content` (both have settings).
- Inline indent, `Leading` or `Spacing` differences; inline `Full` alignment.
- Tabs in script text, CDATA sections, entities other than `amp`, `quot` and `#x27`, a byte-order mark, UTF-16 or any non-UTF-8 encoding.
- `Content` paragraphs without a `Type`, a `Number` with spaces around it, or `Number` on anything but a scene heading.
- Dual dialogue with lyrics, more than one parenthetical, or text in the outer paragraph.
- Script notes, tags or revision marks inside `Content`.

## 12. The writer

Added 2026-10-16 (phase 4). Sources: §1 to §11 above, `docs/specs/layout.md`, the phase 4 brief and the decisions taken with the project's owner on 2026-10-16 (`docs/progress/phase-4.md`): a byte-identical round trip, automatic repair with a report, and notes reported as losses.

The writer takes a document and gives bytes and a list of losses (§15). A document is:

- the blocks in order, each with its kind, rich text and attributes (`docs/specs/commands.md`, "Blocks in JSON");
- the page format, the title lines and the paragraph spacing overrides, keyed by block id (`docs/specs/journal.md`, "The document in the header");
- the **passthrough store** (§13), present when the document was read from an .fdx file;
- the text style: the encoding and whether there was a byte-order mark.

There are two cases:

- **A document read from .fdx** writes back everything that was not edited from its store, and generates only what was (§13.3).
- **A document without a store** (read from Fountain, or new) is generated whole, with a settings template in our own values (§14.6).

The output depends only on the document, never on the OS, the locale or hash-map order.

## 13. Passthrough

### 13.1 What the store keeps

| Part | Kept as |
| --- | --- |
| **Prolog** | Everything before the root's start tag: the declaration, a doctype, comments and processing instructions, and the whitespace between them, as text. |
| **Root** | Its start tag as text, and everything after its end tag that is not dropped (whitespace and comments). |
| **Sections** | Each child of the root other than `Content`, in order, as text, with the whitespace before it. A section in which the reader made a structural repair (a `fixed` or `dropped` issue at the XML level) is kept as the tree the reader built and written in the file's style (§13.5), so the repair reaches the file. |
| **`Content`'s frame** | Its start tag, and the whitespace before its end tag. |
| **Kept paragraphs** | For each paragraph group the generator does not reproduce exactly (§13.2): its text in the file, with the whitespace before it, and a copy of the blocks it was read into. Stored under the id of the group's first block. |
| **Loose children** | Anything in `Content` that is not a `Paragraph` (unknown elements, comments, text that is not whitespace), as text, anchored to the paragraph group before it (§13.4). |
| **Trailing paragraphs** | The empty general paragraphs the reader leaves out of the model at the end (§6.7), as text. |
| **Style** | The newline used in markup (LF or CRLF), the indentation, and whether apostrophes and quotation marks in text were written as references. |

**Reading the style** (2026-10-17). The style comes from the first `Content` paragraph whose start tag begins a line:

- The newline is CRLF when the line break before that paragraph is CRLF, otherwise LF.
- The indent unit is what the paragraph's first child element (its `Text`) is indented by beyond the paragraph itself.
- The paragraph is two levels below the root, so whatever its indentation has before two units is an offset added at every level below the root. The jumpcut files indent `Content` by four spaces and step by two after that, which is a two-space unit with a two-space offset.
- If the child is not on its own line, the paragraph's indentation is taken as two units, with no offset.

The 2026-10-16 reader looked past the whitespace before the first paragraph, so it saw no line break and kept the defaults (LF, two spaces). Every file whose markup differs from those defaults then kept nearly all its paragraphs as raw text: the Windows (CRLF) copies of the jumpcut and screenplain files, and the jumpcut files with their offset. They still wrote back byte for byte, but their stores were about three times the size of the file.

A **paragraph group** is one `Content` paragraph and the blocks it was read into: a page break before it (§6.5), and its element, or the elements of its dual-dialogue block (§6.6).

### 13.2 Generate and compare

At read time each paragraph group is generated again from its blocks (§14), in the file's style, and compared with its text in the file, including the whitespace before it.

- **Equal:** nothing is stored. In the corpus about nine paragraphs in ten are like this.
- **Different:** the group is kept (§13.1). This is how scene properties, run fonts and colors, a full set of inline attributes, `AllCaps` runs, unknown children and any spelling of the markup our generator does not use survive.
- **A paragraph with a structural repair** (§9) is kept with its text marked *not verbatim*. It is never written as it was; it is always generated, carrying what can be carried (§14.4), so the repair reaches the file.

A file written back unedited is therefore the concatenation of pieces each equal to the file's own text, which is why the round trip is byte-identical (§16).

### 13.3 Writing a document with a store

The writer emits, in order:

1. the prolog and the root's start tag;
2. each section, as kept (§13.5 for the settings and the title page), with `Content` at its place:
   - `Content`'s start tag;
   - for each paragraph group in the model, in block order: if a kept paragraph is stored under its first block's id, its blocks still equal the stored copy (kind, text, attributes and spacing override), they are contiguous and in the same order, and its text is verbatim, the kept text; otherwise the group generated (§14), carrying the kept paragraph's attributes and children when there is one (§14.4);
   - loose children, after the group they are anchored to (§13.4);
   - the trailing paragraphs, then the whitespace and `</Content>`;
3. the root's end tag and what follows it.

A group that was moved (by `scene.move`) but not changed is still written as kept, since its text carries its own indentation.

### 13.4 Anchors

A loose child is anchored to the last block of the paragraph group before it in the file, or to the start of `Content`. If that block has been deleted, it goes after the nearest block before it in the file's original order that still exists: ids are handed out in file order when a document is opened, so this is the surviving original block with the largest id below the deleted one. Loose children are never dropped; with no surviving block before them, they go at the start.

### 13.5 Settings, the title page and the header

- **Settings sections** (`ElementSettings`, `PageLayout`, `MoresAndContinueds`, `SceneNumberOptions`, `HeaderAndFooter`) are written back as kept. No phase 4 command changes the page format, and values the model rejects stay as the file has them (§9). A command that edits the page format (a later phase) will rewrite only the attributes whose value it changed.
- **The title page** is written back as kept while the document's title lines equal the lines read from it. Otherwise its `Content` is generated from the title lines (§14.3), and the rest of the `TitlePage` section (its `HeaderAndFooter` and `TextState`) is kept.
- **A section kept as a tree** (a `fixed` issue inside it) is written with the file's newline and indent, one element per line, attributes in their source order and quoted with `"`, text escaped as §14.2 says, and empty elements as `<X/>`.

### 13.6 Where the store lives

The store belongs to the document: core keeps it in the document, so undo, the crash journal's header and recovery carry it with everything else (`docs/specs/journal.md`), and a document recovered after a crash writes back the same file. Kept paragraphs are keyed by block id, outside the elements, so an edit never moves them and undoing a delete brings its block's kept paragraph back with the block. Nothing in the store is ever removed while the document is open: after a save, a kept paragraph whose blocks were edited still describes the file as it was, and an undo can make its blocks equal to the copy again.

## 14. Generating

### 14.1 Paragraphs

A generated paragraph is written on its own line at the file's indent (two spaces per level when there is no file):

```xml
    <Paragraph Number="12" Type="Scene Heading">
      <Text>INT. LAB - NIGHT</Text>
    </Paragraph>
```

- **Attributes**, in alphabetical order, as every sample orders them:
  - `Alignment="Center"` on a centered action;
  - `Number`, the scene number of a scene heading;
  - `SpaceBefore`, the block's spacing override, in points (§14.5);
  - `StartsNewPage="Yes"`, when a page break comes before it (§14.7);
  - `Type`, from the kind: `Scene Heading`, `Action`, `Character`, `Parenthetical`, `Dialogue`, `Transition`, `Shot`, `General` or `Lyric`.
- Only these attributes are written. A paragraph never carries the full set of eight a sample sometimes does: the reader treats missing attributes as the type's own (§6.4).
- **Children:** one `Text` element per run (§14.2), each on its own line one level deeper.
- **Resolution:** a `Number` on a block that is no longer a scene heading is not written; the model rule already forbids it (`Element::problems()`).

### 14.2 Runs

- A run is `<Text>…</Text>`, with `Style` when it is styled: the tokens `Bold`, `Italic` and `Underline`, joined by `+` in that order, as the samples write them. Neighbouring runs of the same style are one run (`RichText::new`).
- An empty paragraph has one empty run, `<Text></Text>`.
- **Escaping:** `&` as `&amp;`, `<` as `&lt;` and `>` as `&gt;`. Apostrophes and quotation marks are written as `&#x27;` and `&quot;` when the file wrote them as references (§13.1), and as themselves otherwise and in new files. Line breaks are literal LF characters, as in the samples (§8); tabs are literal too.
- A character XML 1.0 cannot hold (a control character other than tab and LF, an unpaired surrogate, U+FFFE, U+FFFF) is left out, and so listed as a loss (`invalid_xml_char`).
- Notes and boneyard inside the text are left out (§15).

### 14.3 Title lines

- Each title line is a `TitlePage/Content` paragraph with `Alignment` (`Left`, `Center` or `Right`), `LeftIndent`, `RightIndent` and `SpaceBefore`, in alphabetical order, and its runs. Empty lines are paragraphs with one empty run.
- **A document with Fountain title keys and no title lines** (read from Fountain, or new) gets title lines made from the keys exactly as layout prints them (`docs/specs/layout.md` §9.3): one line per printed line, with its block's alignment, the action column, and as space before the rows above it times the line height. The page therefore looks the same, with one exception: stacked paragraphs cannot share rows, so when both bottom blocks are present (`Draft date` and `Contact` on the left, `Notes` and `Copyright` on the right) the right one goes just above the left. The key names themselves (`Title:`, `Author:`…) have no place in .fdx, so they are a loss (`title_page_keys`); the text is kept.

### 14.4 Editing a kept paragraph

When a group with a kept paragraph has changed, it is generated as in §14.1, and carries from the kept text:

- **Paragraph attributes** other than the five §14.1 writes, in their source position. Of those five, `Type` is carried when the block's kind is still the one it was read as and the type name was not one of ours (§6.1), so an act break edited in Cold Open stays an act break. `Alignment`, `SpaceBefore` and `StartsNewPage` are carried unless the model's centered flag, spacing or page break changed. In that case the model's value is written, or the attribute left out. Inline layout attributes (the eight of §6.4) carry only when the paragraph keeps the `Type` it had, since they are measured against that type's settings.
- **Children** other than `Text` (`SceneProperties`, unknown elements), in the same position relative to the runs: before them, between two of them by run index, or after them. `SceneProperties` stays only while the block is a scene heading, and a nested `DualDialogue` only while the group is still that dual block.
- **Run attributes** other than `Style` (fonts, sizes, colors, revision marks, unknown attributes), per character. The kept and the new printed text are compared once from the start and once from the end. Characters in the common start and end keep the attributes they had, and text in between takes the attributes of the character before it, or the one after it at the very start, as `text.insert` takes style (`docs/specs/commands.md`). `Style` is always the model's.

What cannot be carried this way is reported as a loss (§15).

### 14.5 Values

- **Inches** (indents, columns, page size) are written with two decimals, as the samples write them: `1.50`, `-0.10`, `8.50`. A value that needs more digits is written with as many as it needs, up to five.
- **Points** (margins, spacing) are written as `Pt` displays them: no trailing zeros, `12`, `12.5`.
- **Yes and No** as `Yes` and `No`; alignments as `Left`, `Center` and `Right`.

### 14.6 A new file

A document without a store is written as:

```xml
<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="1">

  <Content>
    …paragraphs (§14.1)…
  </Content>

  …TitlePage (§14.3), when there are title lines…
  …ElementSettings, one for each of the nine kinds, in §6.1 order…
  …PageLayout, HeaderAndFooter, MoresAndContinueds, SceneNumberOptions…
</FinalDraft>
```

- **Resolution:** `Version="1"`, the lowest version in the samples, since a newer reader also reads older files and the template uses only elements every sample version has.
- Each `ElementSettings` holds:
  - a `FontSpec` with `Font="Courier Prime"`, `Size` and the kind's `Style` tokens (`AllCaps`, `Bold`, `Italic`, `Underline`);
  - a `ParagraphSpec` with all eight attributes from the kind's `ElementFormat` (`Leading="Regular"`, `Spacing="1"`);
  - a `Behavior` with `PaginateAs`.
- The other settings come from the page format, as §5 reads them; the header holds the page-number label with its suffix.
- Every value is written from our own `PageFormat` (`docs/specs/layout.md` §3.2), never copied from a sample.
- The newline is LF, sections are separated by a blank line, and the indent is two spaces.

### 14.7 Page breaks and dual dialogue

- **A page break** is written as `StartsNewPage="Yes"` on the next printing paragraph.
- **Breaks that print the same** are normalized rather than reported (§16.2):
  - several breaks in a row are one;
  - a break before the first printing paragraph, or after the last, changes nothing, as layout ignores it (`docs/specs/layout.md` §4.5).
- **A dual-dialogue block** (a character with `dual` and the group before it, as layout pairs them, `docs/specs/layout.md` §4.4) is written as one outer `General` paragraph with `SpaceBefore` set to the character kind's space before. Its only child is a `DualDialogue` element holding the two groups' paragraphs, each with only its `Type` and runs.
- A `dual` character that layout would not pair is written as an ordinary group, and the flag is a loss (`dual_unpaired`).
- **Resolution (2026-10-17):** groups are found in the document as it is written, without the notes, boneyard, sections and synopses that §15 leaves out. A section between the second speaker's dialogue and a parenthetical no longer ends the dual-dialogue block, since reading the file back would join them and writing it again would give other text. Found by the `fdx_write` fuzz target.

## 15. Losses

Writing .fdx lists these losses, each with the block it concerns. A save refuses with `lossy_format` unless the writer allows them, as for Fountain (`docs/specs/commands.md`).

| Code | Content | What is written |
| --- | --- | --- |
| `note_element` | a note block | nothing |
| `boneyard_element` | a boneyard block | nothing |
| `section_element` | a section block | nothing |
| `synopsis_element` | a synopsis block | nothing |
| `inline_note` | a note inside text | the text without it |
| `inline_boneyard` | boneyard inside text | the text without it |
| `title_page_keys` | Fountain title-page keys | the lines, without their key names (§14.3) |
| `dual_unpaired` | a dual flag layout would not pair | an ordinary speech |
| `invalid_xml_char` | characters XML cannot hold | the text without them |
| `spacing_out_of_range` | space above a paragraph (an override, or blank lines) outside 0 to the page body's height, which the reader would not accept (§6.4) | the paragraph without it |
| `run_attributes` | run attributes on text that an edit split in a way §14.4 cannot follow (none known; the code is reserved so a gap is reported, not hidden) | the model's runs |

**Decision (2026-10-16, with the project's owner).** No public .fdx sample shows how anchored script notes are stored, so the clean room gives nothing to write notes from. Notes, boneyard, sections and synopses are therefore losses, as a Shot is for Fountain. If a public sample with anchored script notes turns up, notes will be mapped to it.

A save also reports `reencoded` when the file was read as Windows-1252 or with bytes replaced, and is about to be written as UTF-8 (`docs/specs/journal.md`, "Encoding").

**Production revisions** (2026-10-22, phase 5b; `docs/specs/revisions.md` §9). A document with revision sets or revision marks is written without them, and the save reports a `revision_marks` loss for the document, as for tracked changes. A file's own `Revisions` section (Brick & Steel has one) is passed through untouched, as every section is (§13). It is never read into the document's revision table, and never written from it. **Decision (2026-10-22, the phase 5b brief):** no public sample shows how revision-marked runs are stored, so marks are not written into .fdx.

**Tracked changes** (2026-10-18, phase 5a; `docs/specs/changes.md` §9). A document with pending tracked changes is written from its accepted form, the script as accepting every change would leave it, and the save reports a `tracked_changes` loss for the document, as a Fountain save does (`docs/specs/fountain.md` §12). Edited paragraphs are then generated as §14.4 says. **Decision (2026-10-18, with the project's owner):** no public .fdx sample shows how marked runs are stored. Some runs carry a `RevisionID` (§6.2), but nothing in the corpus shows what it refers to, or how deleted text would be kept (§11). So the clean room gives us nothing to write tracked changes from, and they are not mapped. If a public file with revision-marked runs turns up, mapping them becomes its own task.

## 16. Round trip

### 16.1 The gate

Decided with the project's owner on 2026-10-16. For every .fdx file in the corpus, `crates/fdx/tests/roundtrip.rs` checks:

| Check | Rule |
| --- | --- |
| **G1. Bytes** | A file that reads with no `fixed` or `dropped` issue, in UTF-8 or UTF-16, written back unedited, is byte for byte the file. A repaired file differs exactly where its report says, and a re-encoded file only in its encoding and the declaration's `encoding`. Every such difference is listed in the gate's report. |
| **G2. Fixpoint** | Reading what was written gives the same document (blocks, page format, title lines, spacing, store) with no `fixed` or `dropped` issue, and writing that again gives the same bytes. |
| **G3. Edits stay local** | After scripted edits (one paragraph's text changed, a paragraph inserted, one deleted, a scene moved), the bytes outside the edited paragraph groups equal the file's, and G2 holds for the edited document. |

The report goes to `target/tmp/coldopen-roundtrip/`. Per file it lists whether the bytes were identical, the differences by category, the number of kept paragraphs, the store's size and the time taken. It never holds script text. Without a corpus the test skips with a "ROUND-TRIP GATE NOT RUN" banner, as the reference gate does (`docs/specs/layout.md` §13); with `COLDOPEN_REQUIRE_CORPUS=1` a skip fails.

### 16.2 Documents not read from .fdx

A document made in Cold Open, or read from Fountain, is written and read back as the **.fdx normal form** of the document. The normal form differs from the document only in ways that print the same, and these are never losses:

- page breaks normalized as in §14.7;
- a block's `space_before` in blank lines becomes a spacing override of that many lines' height (`docs/specs/layout.md` §6 places both the same way);
- trailing empty general blocks are dropped, as §6.7 reads them.

The property tests generate documents, write them, read them back and compare with the normal form, after removing what §15 lists as lost.

## 17. Repair

- **Every fix is automatic** (decided with the project's owner, 2026-10-16). The reader repairs what it can as it reads (§2, §9), and nothing is asked item by item.
- **`coldopen repair <in.fdx> <out.fdx>`** reads the input, writes the document as §12 says, and prints the report as text (§9); `--json` prints the JSON form instead, and `--report PATH` also writes it to a file. It never writes the input, and refuses an output that is the input, whatever the path's spelling. A file with nothing to repair is still written, and the report says so. The same runs as the `file.repair` command (`docs/specs/commands.md`).
- **Opening a repaired file** opens it with the report (`doc.open`'s `repaired`). The desktop app shows it (`docs/specs/editor.md`). Nothing is written until the writer saves. The first save over the damaged file first keeps a copy of it as it was (`docs/specs/journal.md`, "Damaged originals").

## 18. Not yet

Act breaks and other custom types as model elements; reading or writing revision sets, marks and colors in .fdx (2026-10-22: Cold Open keeps them in projects, §15); script notes (§15); header and footer content and geometry beyond the page number (§5.3); scene numbering options; locked pages; A4 and other page sizes as templates.
