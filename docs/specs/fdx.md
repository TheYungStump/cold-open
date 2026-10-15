# .fdx: how Cold Open reads it

- **Date:** 2026-10-13, updated 2026-10-14
- **Status:** phase 2 read-only loader, being implemented in `crates/fdx` (package `coldopen-fdx`). Phase 4 extends it into a tolerant reader and writer (§12).
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
  - Extensible Markup Language (XML) 1.0, <https://www.w3.org/TR/xml/>, for the five predefined entities, character references and line-end handling.
  - The WHATWG Encoding Standard's windows-1252 index, <https://encoding.spec.whatwg.org/index-windows-1252.txt>, for the fallback decoding in §2.1.
  - `docs/specs/layout.md` and the types in `crates/screenplay`, which define what the reader must produce.
- **Method.** The sample files were studied with small scripts that print element paths, attribute names, value vocabularies and counts, never script text. No script text or sample boilerplate is copied into this repository.
- No Final Draft software, manual, help text or website text was used. Everything here is written in our own words. A choice we made where the samples are silent is marked **Resolution**.

## 1. Goals

1. **Read every file.** Any bytes produce either a document or one of two errors: the input is not XML at all, or its root element is not `FinalDraft`. Everything else is recovered and reported. The reader never panics.
2. **Report, never drop silently.** Whatever the reader cannot carry into the model is listed in the report with a stable code (§9).
3. **Same result everywhere.** The result depends only on the input bytes. Lengths are parsed with integer arithmetic, and nothing depends on the OS, locale or hash-map order.
4. **Feed layout.** Phase 2 needs the script, the page format, per-element spacing overrides and the title page, so that `crates/layout` can paginate .fdx files and compare them with reference PDFs (`docs/specs/layout.md` §13).

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
- **Syntax error.** The reader stops and keeps everything built so far (`truncated_xml`). If no element has been seen yet, the input is not XML.
- **Depth.** Elements nested more than 64 deep are dropped with their content (`too_deep`). No real file comes close: the deepest path in the samples is 8 elements.
- **After the root.** Elements after the root element closes are ignored (`content_after_root`).
- A self-closing element is treated like a start tag followed by its end tag. Comments, processing instructions and the document type declaration are ignored.

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

- The nested paragraphs are emitted in order, as ordinary paragraphs (§6.1 to §6.4), and the second group's character gets `dual = true`. A group is a `Character` followed by any number of parentheticals, dialogue and lyrics.
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

- **Line breaks.** Inside `Content`, line breaks occur only as LF characters in the middle of a `Text` run, in action and dialogue paragraphs (including dialogue nested in dual dialogue), never two in a row. The rsdoiel title pages have LF inside title runs. No sample has a CR byte.
- **Tabs** occur only in header paragraphs, before `DynamicLabel` elements. No script or title text holds a tab.
- **Whitespace.** Trailing spaces at the end of a paragraph are common (over 200 paragraphs in one script), and a few paragraphs start with a space. They are kept.
- **Characters.** Curly quotes, ellipses, accented Latin letters and the degree sign appear; quotes in the XML are written as `&#x27;` and `&quot;` in the fd11 files and as raw characters elsewhere.
- **Runs.** Almost every paragraph has a single run; styled runs carry only a `Style` attribute. Run-level `AllCaps` occurs only on title pages.
- **Settings.** Margins: 72/72 pt (jumpcut, where present) and 90/62 pt (rsdoiel), header at 36 pt. Parentheticals hang their first line by 0.10 or 0.14 in.
- **Act breaks** (`New Act`, `End of Act`) are centered and capitalized by their own settings, and `New Act` starts a new page; we read them as general text (§6.1, §6.5).

## 9. Report

Each `Issue` has a stable `code`, a message in English that never quotes script text, and the paragraph index (§3) when it concerns one `Content` paragraph. Issues come in this order: decoding, XML, settings, script, title page, then the counted codes (marked ‡), each once in order of first occurrence.

| Code | When | Effect |
| --- | --- | --- |
| `encoding_utf16` | the input is UTF-16 | decoded |
| `encoding_windows_1252` | not UTF-8, and the declaration names windows-1252 or ISO-8859-1 | decoded as windows-1252 |
| `encoding_lossy` | invalid byte sequences | replaced with U+FFFD |
| `truncated_xml` | an XML syntax error | the rest of the file is ignored |
| `mismatched_end_tag` | an end tag closes other open elements first | those elements are closed |
| `stray_end_tag` | an end tag with no open element of that name | ignored |
| `unclosed_element` | elements still open at the end | closed |
| `too_deep` | nesting deeper than 64 | the deeper elements are dropped |
| `content_after_root` | elements after the root closes | ignored |
| `unknown_entity` ‡ | an entity other than the predefined five, per name | kept as written |
| `bad_char_ref` ‡ | a character reference to no valid character | kept as written |
| `attribute_syntax` | a malformed attribute | skipped |
| `attribute_value` | an attribute value with an unresolvable reference | used as written |
| `duplicate_attribute` | an attribute repeated on one element | first kept |
| `sections_ignored` | top-level elements we don't read | listed once |
| `duplicate_section` | a second `Content`, `PageLayout` or other section we read | first kept |
| `section_missing` | `PageLayout`, `MoresAndContinueds`, `SceneNumberOptions` or `HeaderAndFooter` absent | defaults kept |
| `invalid_value` | a value that does not parse or is out of range, or an unusable string or column | default kept, or the value cut or clamped to its range (§5.1, §5.3, §7) |
| `value_rounded` | a value with more precision than a `Pt` | rounded |
| `settings_for_unmodelled_type` | settings for types we don't model | listed once |
| `duplicate_settings` | a repeated settings `Type` | first kept |
| `settings_missing` | kinds with no settings | defaults kept, listed once |
| `alignment_full` | `Full` alignment in settings, or on the title page (‡) | read as left |
| `line_spacing` | `Spacing` other than 1 in settings | space above multiplied |
| `unknown_paginate_as` | an unknown `PaginateAs` | the kind itself |
| `settings_font_ignored` | a non-Courier font or a size other than 12 in settings | ignored |
| `document_leading` | `DocumentLeading` other than Normal | ignored |
| `scene_continueds_unsupported` | scene continueds switched on | stored, not printed |
| `scene_number_side` | scene numbers hidden on one side | ignored |
| `starting_page` | `StartingPage` other than 1 | ignored |
| `header_content_ignored` | header labels or text besides the page number | ignored |
| `footer_ignored` | a visible footer with content | ignored |
| `invalid_format` | the format fails validation | default format used |
| `no_content` | no `Content` element | empty script |
| `unknown_paragraph_type` ‡ | a type outside §6.1, per type name | read as general |
| `invalid_scene_number` | a scene number that breaks the model rules | dropped |
| `scene_number_ignored` | `Number` on a kind other than a scene heading | dropped |
| `scene_properties_ignored` | `SceneProperties` not on a scene heading | ignored |
| `run_all_caps` ‡ | runs styled `AllCaps` | upper-cased |
| `unknown_style` ‡ | an unknown style token, per token | ignored |
| `run_font` ‡, `run_size` ‡, `run_color` ‡, `run_background` ‡, `run_adornment` ‡, `run_revision` ‡ | run attributes we don't model (§6.2) | ignored |
| `tab_character` ‡ | tab characters in script or title text | kept |
| `element_ignored` ‡ | an unexpected element inside `Content`, a paragraph, a run or `DualDialogue`, per name | ignored |
| `paragraph_format_ignored` | inline attributes we cannot express (§6.4) | ignored |
| `dual_dialogue_shape` | dual dialogue that is not two speaker groups | read without the dual flag |
| `dual_dialogue_text` | text in the outer dual-dialogue paragraph | emitted after the block |
| `trailing_empty_dropped` | empty general paragraphs at the end | dropped |
| `title_format_ignored` ‡ | title paragraph attributes we cannot express (§7) | ignored |

## 10. Tolerance rules

1. Only "not XML at all" (no element before the end or before the first syntax error) and "the first element is not `FinalDraft`" are errors.
2. Bad bytes are decoded with a report (§2.1); bad XML is repaired or cut off with a report (§2.2).
3. A missing section, element or attribute keeps the default. A value that does not parse keeps the default and is reported.
4. Unknown elements and attributes never stop reading. What they would have changed is reported where we know it matters, and silently ignored otherwise (editor state such as `SmartType`, `WindowState` or `Macros` is listed under `sections_ignored`).
5. Text is never trimmed, re-cased (except `AllCaps`) or re-wrapped.
6. Every element the reader produces satisfies `Element::problems()`, and the format satisfies `PageFormat::problems()`. Every override and every title line's space above lies between 0 and that format's body height. The fuzz checks (`coldopen_fdx::fuzzing::read`) test all of these.
7. **Time.** Reading takes O(n log n) time in the size of the input. Every lookup by name, such as a counted code's key, a section, a settings type, a header label or an attribute, goes through an ordered map or set, never a scan of everything seen so far. A file of tens of thousands of distinct names reads about as fast as one of the same size that repeats a single name.

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

## 12. Phase 4

Phase 4 turns this loader into a tolerant reader and writer:

- **Writer** for the model, with the settings blocks written in our own words and values.
- **Unknown-element passthrough:** keep every element and attribute we don't model, in place, and write it back unchanged, so a file survives a round trip through Cold Open.
- **Repair report and `coldopen repair`:** the report above, extended with what was fixed, written as a corrected file.
- Act breaks and other custom types as model elements, revisions and their colors, script notes, header and footer content and geometry (once verified), scene numbering options, locked pages, A4 and other page sizes, and title pages written back as positioned paragraphs.
- Fuzzing of the writer and a lossless round-trip gate on the .fdx corpus.
