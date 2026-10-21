# Layout and pagination

- **Date:** 2026-10-13
- **Status:** implemented in `crates/layout` (phase 2; §15 in phase 3; phase 4, 2026-10-16, lays .fdx documents out with their own format and overrides as they are edited, §4.8, §15.2). Phase 5a (2026-10-18) adds tracked-change marks (§17), being implemented.
- **Sources:**
  - `docs/spec.md`, "Formatting and pagination rules": the page defaults table and the seven page-break rules. That section summarizes two public format guides, Highland's "Standard Screenplay Format" and the NFI screenplay-format page. Neither guide could be fetched from this build environment, so this document relies only on the spec's summary of them.
  - `docs/specs/fountain.md`: the element model, `space_before`, transparent elements, and dual dialogue.
  - The public Fountain syntax page, <https://fountain.io/syntax>, as restated in `docs/specs/fountain.md`.
  - Our observations of publicly shared PDF and .fdx pairs printed by Final Draft 11 (2026), from the MIT-licensed repository SimonGZ/jumpcut, `tests/fixtures/corpus/public/{big-fish,big-fish-scene-numbers,little-women}/source/`. We used them as report-only data: positions, counts and settings values, never text. The files are kept in the gitignored `corpus/fd11/` and identified by sha256 in §14.
  - Our observations of the three reference pairs from fountain.io, once they are available (§14).
- No Final Draft software, manual or help text was used. Everything here is written in our own words. A choice we made where the sources are silent is marked **Resolution**.

## 1. Goals

1. **Same pages everywhere.** A script and a page format always produce the same pages, byte for byte, on every OS and every run. Pagination uses integer arithmetic only.
2. **Never fail.** Any script and any valid page format produce pages. Layout never panics, and every page holds at least one line of the script, so pagination always ends. A rule that cannot be kept is broken in a recorded, predictable way (§7.6).
3. **Fast.** A full repagination of a 120-page script takes well under 50 ms in a release build.
4. **Inspectable.** Every page records why it ended (§7.7), and a plain-text page view (§11) shows exactly what prints.

## 2. Units and the grid

- **Units.** All lengths are `Pt` values: whole hundredths of a point, which is 1/7200 inch. An inch is 7200 units and a point is 100. A length written with at most two decimals in points (`184.32`), or in inches (`2.56` in = 184.32 pt), converts exactly. Parsing never goes through a binary float.
- **The grid.** Screenplays are set in a 12-point monospaced face at 10 characters per inch and 6 lines per inch. The page format gives the cell width (`pitch`, 7.2 pt) and the line height (`line_height`, 12 pt). Text is measured in **cells**, not glyph widths, so fonts never affect line breaks.
- **Cells.** A **cluster** is a character of non-zero display width, followed by every zero-width character after it, with any characters joined to it by U+200D (zero-width joiner). A cluster's width is its display width under Unicode 17.0, as `unicode-width` 0.2.2 computes it without the East Asian "ambiguous is wide" option: 1 for Latin text and smart punctuation, 2 for CJK and emoji presentation. A zero-width character with no base before it counts as a 1-cell cluster, unless a character with a width is joined to it, in which case the cluster has that character's width and the joined character starts at the cluster's first cell (2026-10-14).
- **Control characters.** Tabs in action are expanded to four spaces by the Fountain reader; a tab left anywhere else counts as one space. Any other C0 or C1 control character left in the text is removed before measuring, and a `control_char` diagnostic is recorded.

## 3. The page format

A `PageFormat` holds every number layout uses. Nothing is a constant in code. Fountain documents use the default below. An .fdx file supplies its own (`docs/specs/fdx.md`).

### 3.1 Fields

| Field | Meaning |
| --- | --- |
| `page_width`, `page_height` | Paper size. |
| `margin_top`, `margin_bottom` | The body area runs from `margin_top` down to `page_height − margin_bottom`. |
| `header_top` | Top of the header line, which holds the page number. |
| `font_size`, `pitch`, `line_height` | Type size, cell width, line height. |
| `baseline` | Distance from the top of a line to its baseline. |
| `elements` | One `ElementFormat` per printing kind: scene heading, action, character, parenthetical, dialogue, transition, lyric, shot, general. |
| `break_at_sentences` | Action and dialogue split only at sentence ends (§8). When false, they may split at any line end. |
| `split_mode` | `mid_line`: a split at a sentence end in the middle of a line moves the rest of that line to the next page, and that text is wrapped again. `line_end`: a split happens only where a sentence end falls at a line end. |
| `ellipsis_ends_sentence` | Whether `...` or `…` can end a sentence (§8). |
| `hyphen_breaks` | Whether a line may break after a hyphen (§5.2). |
| `hyphen_overhang` | Whether a hyphen ending a line may sit one cell past the line's last cell (§5.2). |
| `more`, `contd` | The strings printed below the last line of dialogue before a split, and after the name at the top of the next page. Either may be absent, which turns that insert off. |
| `contd_after_action` | Add `contd` to a character name when the same speaker talks again after only action in between, within one scene (§4.6). |
| `more_contd_in_margin` | When true, the (MORE) line and the continued name sit in the line slot just outside the body (below it and above it) instead of using body lines. |
| `page_numbers` | Whether numbers print, the first body page that shows one, the suffix, and the right edge. |
| `scene_number_left`, `scene_number_right` | Where the left scene number starts and where the right scene number starts. |
| `dual` | The two columns used for dual dialogue (§4.4). |

An `ElementFormat` has:

| Field | Meaning |
| --- | --- |
| `left`, `right` | The text column's edges, measured from the paper's left edge. |
| `first_indent` | Added to `left` for the first line of each paragraph. It may be negative, as for a parenthetical whose opening bracket hangs. |
| `align` | `left`, `center` or `right` within the column. |
| `space_before` | The usual space above this kind (§6). |
| `caps` | The text prints in capitals. |
| `bold`, `italic`, `underline` | A style added to every run of this kind. |
| `paginate_as` | Which kind's keep rules apply. For example, a shot keeps with what follows it, like a heading. |
| `starts_new_page` | Every element of this kind starts a new page. |
| `cells` | An explicit number of cells per line. When absent, it is computed (§5.1). |

### 3.2 Defaults (US Letter, feature film)

**Resolution:** these follow the spec's table. Where the table gives a range or says "about", or names no value, we chose the number shown.

| Setting | Default |
| --- | --- |
| Paper | 8.5 × 11 in |
| Margins | top 1 in, bottom 1 in; header line at 0.5 in |
| Type | 12 pt, pitch 7.2 pt (10 per inch), line height 12 pt (6 per inch), baseline 9 pt below the line top |
| Body | 648 pt, which is 54 lines. The spec says "about 55"; with 1 in margins at 6 lines per inch the exact figure is 54. |

| Kind | Left | Right | First indent | Cells | Align | Other |
| --- | --- | --- | --- | --- | --- | --- |
| Scene heading | 1.5 in | 7.5 in | 0 | 60 | left | caps |
| Action, general | 1.5 in | 7.5 in | 0 | 60 | left | Centered action uses `center` (§4.1) |
| Character | 3.7 in | 7.5 in | 0 | 38 | left | caps |
| Parenthetical | 3.2 in | 5.2 in | −0.1 in | 20 | left | The first line starts at 3.1 in, so the bracket hangs |
| Dialogue | 2.5 in | 6.0 in | 0 | 35 | left | |
| Transition | 5.5 in | 7.5 in | 0 | 20 | right | caps; ends at the right margin |
| Lyric | 2.5 in | 6.0 in | 0 | 35 | left | italic |
| Shot | 1.5 in | 7.5 in | 0 | 60 | left | caps; paginates as a scene heading |

- **Spacing.** Every kind has `space_before` = 12 pt (one line). A Fountain file therefore prints with the blank lines its text shows. Inside a dialogue group, members get no space (§6).
- **Breaks.** `break_at_sentences` is on, `split_mode` is `mid_line`, `ellipsis_ends_sentence` is off and `hyphen_breaks` is on. `more` = `(MORE)`, `contd` = `(CONT'D)`; `contd_after_action` and `more_contd_in_margin` are off.
- **Page numbers.** They print. Their right edge is at 7.5 in and the suffix is `.`. The first body page has no number, so the first number printed is `2.` (§9.1).
- **Scene numbers.** The left number starts at 0.75 in and the right one at 7.6 in.
- **Dual dialogue.** The left column runs from 1.5 in to 4.4 in and the right one from 4.6 in to 7.5 in. Inside each column, the character name starts 0.9 in in, a parenthetical runs from 0.4 in to 0.4 in short of the column's right edge (hanging bracket), and dialogue fills the column.

A4 (210 × 297 mm) keeps every left and right position and adjusts the top and bottom margins (`docs/spec.md`). It is not selectable until templates arrive.

### 3.3 Validation

Layout accepts only a `ValidFormat`, made by `PageFormat::validate`. Validation fails, listing every problem, when any of these hold:
- the page is smaller than 1 in or larger than 200 in;
- `font_size`, `pitch` or `line_height` is 0 or more than 72 pt;
- `baseline` falls outside 0 to `line_height`;
- a margin is negative;
- the body holds fewer than 3 lines (enough for a continued name, one line and (MORE));
- the header line falls off the page;
- for any kind, `left < 0`, `right > page_width`, `left + first_indent < 0`, or the column has fewer than 1 cell;
- `space_before` is negative or taller than the body;
- an insert string is empty or holds a line break;
- a dual column is empty, or the two columns overlap.

The .fdx reader replaces a format that fails validation with the default and reports why.

## 4. From script to flow

Layout first turns a `Script` into a **flow**: a list of blocks, each made of members, each member holding printable text and its format.

1. **Transparent elements** (sections, synopses, notes, boneyard) are skipped. Page breaks are kept (item 5).
2. **Hidden runs** (inline notes and boneyard) are removed from the text.
3. **Blocks.**
   - A scene heading, shot, action, general element, transition or standalone lyric is a block of one member.
   - A **dialogue group** is a character followed by every parenthetical, dialogue and lyric element up to the next element of any other kind. The character is the group's **name** and the others are its members.
   - A parenthetical, dialogue or lyric with no character before it forms a group with no name.
   - A character with nothing after it is a group with a name and no members.
4. **Dual dialogue.**
   - A group whose character has `dual` set pairs with the group just before it, if that group has a name, is not already paired, both groups have at least one member after the name, and nothing printing or a page break lies between them. The pair is one **dual block**: the earlier group goes in the left column and the dual one in the right.
   - A dual character with no partner forms a normal group and is reported as `dual_without_partner`.
   - A dual block whose taller column, plus a continued name and a (MORE) line, would not fit in the body is laid out as two groups one after the other, and reported as `dual_too_tall`. This depends only on the script and format, never on where the block falls on a page.
5. **Page breaks.** A page-break element, or an element whose format has `starts_new_page`, marks an **explicit break** before the next printing element. Consecutive breaks count as one. Breaks before the first printing element or after the last are ignored, so they never produce blank pages.
6. **Capitals and continued names.** Text whose format has `caps` is printed in capitals, using the Unicode upper-case mapping. Sentence ends (§8) are found on the original text first. When `contd_after_action` is on, a character name gets ` ` + `contd` when all of these hold:
   - the previous named group in the same scene had the same name (compared in capitals, ignoring an extension in brackets);
   - only action, general or centered elements came between them;
   - the name does not already contain the marker (§7.5).

   A scene heading resets this.
7. **Empty elements.** A printing element with no visible text takes one blank line. Empty elements at the end of the script are dropped.
8. **Normal form.** Core lays out a Fountain document as `normalize(script)` (`docs/specs/fountain.md` §10), so its pages never change when it is saved and read back. Layout itself accepts any script. (2026-10-16) An .fdx document is laid out as its blocks stand, with the file's page format, title lines and spacing overrides. (2026-10-18) So is a bundle (`docs/specs/bundle.md` §4). The .fdx reader keeps text as it is (`docs/specs/fdx.md` §10), the reference comparison of §13 measured exactly that, and a Fountain normalization would merge and renumber elements under the overrides. Writing and reading back .fdx changes nothing that prints (`docs/specs/fdx.md` §16.2).

## 5. Measure and wrapping

### 5.1 Cells per line

The first line of a paragraph has `floor((right − left − first_indent) / pitch)` cells. Every later line has `floor((right − left) / pitch)` cells. An explicit `cells` value replaces the second figure; the first line then gets `cells − round(first_indent / pitch)`. A line always has at least one cell.

### 5.2 Wrapping

Wrapping is greedy: each line takes as many words as fit.
- **Break points.** A line may break after any run of spaces; the spaces at the break are dropped and do not count. When `hyphen_breaks` is on, it may also break after a hyphen (`-`) that follows a letter or digit and is followed by a letter. **Resolution:** on by default, to be checked against the corpus (§14).
- **Hanging hyphens.** When `hyphen_overhang` is on, a hyphen that would be the first cell past the measure still goes on the line if the line ends right after it: at the end of the text, before a space, or at a hyphen break. Off by default.
- **Line breaks in the text** (`\n`) always end a line.
- **Words longer than a line** break at the last whole cluster that fits.
- **Leading spaces** at the start of the text or after a `\n` are kept and count as cells; this is action's indentation.
- **Alignment.** Left-aligned lines start at the column's left edge (plus `first_indent` on the first line). Centered lines are placed so that the cells left over on the left are `floor(spare / 2)`. Right-aligned lines end at the column's right edge.
- **Source map.** Each line keeps the byte range of the source text it shows, so the editor and the reference comparison can map lines back to the script.

## 6. Spacing

The space above an element is the first of these that applies:
1. **Top of page.** 0 when the element's first line is the first content on the page. This includes the rest of a split element and every continued group.
2. **Override.** The per-element override, when the caller supplies one. Only the .fdx path does, for a paragraph whose own spacing differs from its type's.
3. **Explicit gap.** `n × line_height`, when `space_before == Some(n)` and `n` differs from the usual gap. The usual gap is 0 for a parenthetical, dialogue or lyric right after a character, parenthetical, dialogue or lyric, and 1 otherwise (the Fountain reader's rule, `ElementKind::default_gap`).
4. **Inside a group.** 0 when the usual gap is 0.
5. **Otherwise** the format's `space_before` for the element's kind.

Because rule 3 treats `Some(usual)` like `None`, a document prints the same before and after a Fountain save and reload.

A dual block uses the space of its left group; the right group's own space is ignored. Space is only placed when content follows it on the same page.

## 7. Pagination

### 7.1 Cursor

A **cursor** names a position in the flow: a block, a member within it, and a byte offset into that member's text. A cursor is **canonical** when its offset skips leading spaces and points into the text, or the member and block have moved on when the text is used up. A canonical cursor past the last block is the end.

A page starting at a cursor inside a named group is a **continuation**. This is derived from the cursor alone, never stored, so a page can be laid out from any cursor without knowing earlier pages.

`page_from(flow, format, cursor)` lays out one page and returns it with the cursor for the next page. Paginating a whole script is a loop from the start. The reference comparison (§13) calls `page_from` from the cursor where a reference page starts.

### 7.2 Filling a page

Lines are placed top-down. A line fits when the y of its bottom, measured from the top of the body, is no more than the body height, counting:
- the space above it;
- every line above it;
- any insert the break would need at the foot (§7.5), unless inserts go in the margin.

After each line is placed, every **candidate** break that follows it is checked. A candidate is a cursor where the next page could start:
- **(a)** between two blocks;
- **(b)** between two members of a group;
- **(c)** inside a member's text, at a split point (§7.4).

Filling stops at the first line that does not fit. The page then ends at the latest candidate of the lowest **level** that has one:
- **level 0**: every rule in §7.3 holds;
- **level 1**: the same, except that action and dialogue may split at any line end instead of only at sentence ends;
- **level 2**: any candidate that fits.

There is always a level-2 candidate after the page's first line, so every page holds at least one line (§7.6).

### 7.3 Rules

These restate `docs/spec.md`'s seven rules precisely. "Lines" means content lines; inserted lines and space don't count.

1. **Headings.** A candidate is illegal if the last scene heading on the page (or anything with `paginate_as` heading, such as a shot) has fewer than two lines after it on this page. If fewer than two lines exist after it in the whole script, all of them must be on the page.
2. **Character names.** A named group whose name is on the page must have at least two dialogue or lyric lines on the page, or all of them if it has fewer. The name is never the last line of a page. Parentheticals don't count toward the two lines.
3. **Dialogue splits.**
   - A dialogue or lyric member splits only at a split point (§7.4), with at least two of its lines on each side, counting the rest as it will be wrapped again.
   - A break between two members of a group needs at least two dialogue or lyric lines of the group on each side.
   - A lyric splits only at a `\n`.
4. **Parentheticals.**
   - A page never ends right after a parenthetical.
   - A break is illegal before a parenthetical that has no dialogue or lyric after it in its group.
   - A break before any other parenthetical is allowed: the next page starts with the continued name, so the parenthetical is not alone.
   - A parenthetical never splits.
5. **Action splits.** Action, centered action, general text and standalone lyrics split only at a split point, with at least two lines on each side. They have no inserts.
6. **Transitions.** A break is illegal right before a transition.
7. **Dual dialogue.** A dual block has no candidates inside it.

Scene headings, shots, character names and transitions never split. An explicit break (§4.5) ends the page there, whatever the rules say. Inside a group it acts as a member break, with inserts.

### 7.4 Split points

- **Sentence ends.** With `break_at_sentences` on, a split point is the start of a sentence after a sentence end (§8).
  - `mid_line`: any sentence end inside the last line placed is a split point. The page keeps the text up to the sentence end, and the rest is wrapped again from the start of the next page.
  - `line_end`: only a sentence end that falls exactly at a line end is a split point.
- **Any line end.** With `break_at_sentences` off, or at level 1, every line end is a split point.

### 7.5 (MORE) and continued names

- When a page ends inside a named group, at a member break or a split, the page gets `more` on a line below the last group line, at the character's left edge.
- The next page starts with the **continued name**: the name as printed, then a space and `contd`, unless the name already contains the marker. The marker is matched by comparing in capitals with `’` read as `'`, accepting `contd`, `CONT'D` or `CONTD`.
- The marker goes after any extension, so the result reads `MAYA (V.O.) (CONT'D)`.
- An unnamed group gets neither insert, and an absent string turns that insert off.
- Inserts take body lines unless `more_contd_in_margin` is set. A candidate that only fits without its (MORE) line is not a candidate, so inserts never overflow a page.

### 7.6 Fallback and termination

- If no candidate exists at level 0, the page ends at the latest level-1 candidate. Failing that, it ends at the latest level-2 candidate, which is any line end that fits.
- If even the first line plus the inserts does not fit, the inserts are dropped for that page (`inserts_dropped`).
- A page that ends at level 1 or 2 records which rules it broke.

Every page takes at least one line, so the next cursor is always past the current one, and pagination ends after at most one page per line of the script.

### 7.7 Why a page ended

Each page records its **break cause**:
- `natural`: level 0;
- `explicit`: a page break or `starts_new_page`;
- `fallback(level, rules broken)`;
- `end`: the script ended.

## 8. Sentence ends

Sentence ends are found on the source text, before capitals are applied. A sentence end is a run of terminators (`.` `!` `?` `…`) followed by any closing characters (`"` `'` `)` `]` `”` `’`). It counts when all of these hold:
1. Whitespace follows it.
2. The next non-space character, after any opening characters (`"` `'` `(` `[` `“` `‘` `¿` `¡`), is not a lowercase letter. Scripts without letter case always pass.
3. For a single `.`, the word before it is not:
   - a single letter of a script with case, as in `J.`;
   - a word containing a `.`, as in `U.S.`;
   - one of the titles `MR MRS MS DR ST JR SR PROF REV GEN COL LT SGT CAPT CMDR GOV SEN VS`, compared in capitals;
   - one of `NO VOL PG PP CH FIG APT RM` when a digit follows.
4. Two or more dots, or a `…` with no `?` or `!` after it, count only when `ellipsis_ends_sentence` is on.

`--`, `—`, `:` and `;` never end a sentence. The split point is the first character of the next sentence.

**Resolution:** these rules are ours and are checked against the reference splits (§14).

## 9. Page furniture

### 9.1 Page numbers

- The title page is unnumbered and not counted. The first page after it is body page 1.
- Body pages from `page_numbers.first` onward (default 2) show their number plus the suffix, right-aligned at `page_numbers.right` on the header line.

### 9.2 Scene numbers

When the export option `scene_numbers` is on, every scene heading prints a number on its first line, at both `scene_number_left` and `scene_number_right`:
- a heading with a `scene_number` prints it as written;
- a heading without one prints its 1-based position among the scene headings.

When the option is off, no numbers print.

### 9.3 Title page

The title page comes from either source:
- **Fountain keys.**
  - `Title` is centered, in capitals, starting a third of the way down the body.
  - Then `Credit`, then `Author`/`Authors`, then `Source`, each centered with one blank line before it. A key with several values prints one value per line.
  - `Draft date` and `Contact` print at the bottom left, ending at the bottom margin, in source order.
  - `Notes` and `Copyright` print at the bottom right.
  - Other keys are not printed.
- **.fdx paragraphs.** Each paragraph prints on its own lines with its alignment and space before, one line per `\n`, wrapped to its column.

There are title pages only when the source has a title page. Title pages are unnumbered and not counted: the first body page is page 1 however many title pages come before it.

**Resolution (2026-10-14): a title that does not fit continues on the next title page.** Title rows are counted from the top of the first title page; row `r` prints on title page `r / rows` at row `r % rows`, where `rows` is the number of body rows. So:
- an empty line takes its row but prints nothing, and never starts a page by itself;
- a paragraph's space before moves it at most one page, so a huge value cannot make blank pages;
- a title on more than one page carries a `title_continued` diagnostic on its first page;
- lines that would print above the first row (only the bottom-anchored Fountain blocks can, when they are taller than the page) are dropped, with a `title_overflow` diagnostic.

Source: the Big Fish reference printed by Final Draft in 2012 (§13) has an unnumbered second title page holding the end of its title paragraphs; its first numbered body page is labelled 2 on the fourth sheet. See §14 for where its continued lines differ from ours.

## 10. Placement, scene pages and length

- For every source element, layout reports its first body page and the rows it uses on each page.
- A scene, which `docs/specs/commands.md` defines as running from its heading to the next heading or section, starts on the page of its heading.
- A scene's **rows** on a page run from the row of its first line there, including the space above it unless that space is dropped at the top of the page, through its last line.
- Its **length** is the sum, over its pages, of `rows × 8 / body_lines`, rounded up to the next whole eighth. The minimum length is 1/8.
- **Resolution:** our rule. It is compared with the lengths an .fdx file records, as information only.

## 11. Page view

The page view is the plain-text picture of the pages, used by `coldopen pages`, by `doc.export` with format `pages`, by the golden files and by `tools/refpdf`. It is defined exactly, so a PDF can be checked against it.

- **Header lines.** Each page starts with a line beginning `=== page N`, where N counts physical pages from 1. For our own output, the line continues with ` · title` or ` · body K`, then the break cause, then any diagnostics, and ends with ` ===`.
- **Rows.** Rows follow, from 0 to the last non-empty row.
  - A glyph with baseline `y` (from the page top) and origin `x` (from the left edge) goes in row `floor(y / line_height)` and column `round_half_up(x / pitch)`.
  - Each cell holds one cluster. A cluster with no glyph in the font prints as one U+FFFD per cell it covers.
  - Empty cells are spaces, trailing spaces are dropped, and empty rows inside the range stay as empty lines.
  - The watermark, underlines and other drawing are not shown.
- **End.** The text ends with a single `\n`.

## 12. PDF output

- **Fonts.**
  - Courier Prime Regular, Bold, Italic and Bold Italic are bundled (`crates/fonts`, SIL Open Font License 1.1). Each face used is embedded as a subset of its TrueType glyphs, as a CID font with identity encoding.
  - All glyphs get the width 600/1000 em (`/DW 600`). At 12 pt this is exactly 7.2 pt, so glyphs land on the grid without spacing adjustments; character spacing is used only when a format's pitch is not 0.6 em.
  - A combining mark the font has gets width 0 and is drawn at its base's position.
  - A mark with no base is placed at its own cell like any other glyph. Courier Prime's marks hang left of their origin, so its ink prints over the cell before; the page view shows it in its own cell. Text after it stays on the grid. (2026-10-14: kept this way, because a mark with no base is malformed text and moving it would make our own PDF reader disagree with the page view.)
  - At a pitch other than 0.6 em, the character spacing is added after every glyph, so a mark after its base is drawn that much to the right of where the font places it (1.2 pt at 10 pt type on the 7.2 pt grid). Text after it stays on the grid. Every front end today uses 12 pt type, where the spacing is zero.
  - **Resolution (2026-10-14):** a character of zero display width whose glyph has an advance anyway draws nothing, in the script and in the watermark. In Courier Prime this is only the soft hyphen U+00AD, a default-ignorable format character. Inside a cluster it takes no cell (§2), so drawing it would print a stray hyphen and push the rest of its line one cell right. A soft hyphen with no base before it (at the start of an element or after a space) keeps the one cell §2 gives it, and that cell prints blank.
- **Glyph positions.** Each line is drawn as runs of glyphs in one face. A glyph with an advance moves the pen one pitch (0.6 em plus the character spacing), and a zero-width glyph moves it by the character spacing alone. **Resolution (2026-10-14):** a run ends after any cluster whose glyphs do not move the pen by exactly its cells: the number of its glyphs with an advance is not its cell count, or it has a zero-width glyph and the character spacing is not zero. The next cluster starts a new run at its own grid position, so an error never carries along the line. This happens for an emoji-presentation sequence whose variation selector the font lacks (two cells, one glyph), for a keycap, and for a mark with no base (one cell, one zero-width glyph). The check uses whole numbers only.
- **Missing glyphs.** A cluster with no glyph in the font draws the font's missing-glyph box once per cell it covers. It maps to U+FFFD for text extraction. The export's `missing_glyph` warning names the characters the face lacks: the cluster's first character, or a later one with a display width. A missing zero-width character after the first never causes the boxes; in a cluster that prints, it is a mark that is left out and named in `missing_mark`. **Resolution (2026-10-14):** the watermark follows the same rules: a character the face lacks prints as a box and joins `missing_glyph`, unless it is a zero-width mark after the first character, which is left out and joins `missing_mark`.
- **Styles.** Bold and italic use their own faces. Underline is a filled rectangle at the font's underline position and thickness, covering the underlined cells.
- **Watermark.** The text is centered on the page, rotated 45°, in 50% gray at 20% opacity, under the script text. Its size is the largest whole point size up to 96 pt at which the text fits 80% of the page's shorter side, and at least 1 pt. The size is worked out in 64-bit integers, so a watermark of any length is safe. The rotation uses the constant 1/√2, never a trigonometric function.
- **Pages.** The title page, when there is one, then the body pages. **Resolution (2026-10-14):** a PDF must have at least one page. When there is nothing to print (an empty script, one with only notes, sections, synopses or boneyard, or a title page that is left out), the PDF has one blank page at the format's size, still with the watermark when there is one, and the export counts 1 page. The page view shows no page in that case (§11).
- **Byte-identical output.**
  - The file has no dates and no random data.
  - The document ID is two FNV-1a 64-bit hashes (different seeds) of the bytes written before the trailer. The subset tag comes from the same hash over the face name and the sorted glyph ids.
  - Glyph ids, widths and the Unicode map are written in sorted order.
  - Streams are compressed with zlib at a fixed level.
  - Producer is "Cold Open", with no version.
  - Nothing in the file depends on a path, the clock, the locale or the environment.
- **Metadata.** The title is the title page's `Title`, when there is one.

## 13. Reference comparison

The gate for phase 2 compares our pagination of the three reference .fdx files with the PDFs Final Draft printed from them in 2012. The test lives in `crates/layout/tests/reference/` and runs with `cargo test -p coldopen-layout --test reference`. It skips when `corpus/` is missing, and skips any pair whose `.ref.json` is missing or was extracted from a different PDF, printing the `refpdf extract` command that writes it; with `COLDOPEN_REQUIRE_CORPUS=1` each of those skips is a failure instead. Whenever it compares fewer than the three reference scripts, it writes a "REFERENCE GATE NOT RUN" banner straight to standard error, which a plain `cargo test` shows even for a passing test, and in CI also a warning on the run's page (2026-10-16, review finding F17.1), so a skipped gate never passes looking like a real one.

- **Inputs.** `tools/refpdf` extracts each reference PDF to `<stem>.ref.json`: positioned text segments per baseline, with no interpretation. The test reads the .fdx with `crates/fdx`, which gives the script, page format, overrides and title page.
- **Alignment.**
  - Reference segments are classified as page numbers, (MORE), continued names, scene numbers, other decoration, or content.
  - Content is aligned to our flow on its characters, with all whitespace removed and quotes, dashes and case folded.
  - A continued marker at the end of a name is dropped on both sides, so a marker the writer typed counts the same as one layout adds.
  - Coverage below 99.5% on either side is reported as a pair mismatch and no metrics are computed. The report lists each unmatched stretch by element and length.
  - **Off-grid lines (Resolution, 2026-10-14).** A reference segment with at least 3 more characters than the grid cells it spans, and over 10% more, was printed in a narrower font than the page's grid, for example a face the printing machine substituted. A fixed-pitch layout cannot be measured against it, so its elements are left out of M1, and breaks from the pages holding it count as unmeasurable. Three conditions apply (accepted 2026-10-14):
    - the report lists every excluded element with its script, reference and our pages, and the reason (its measured glyph advance against the grid), and every unmeasured break;
    - a script with more than 2% of its breaks unmeasurable fails the gate;
    - only wrapping is skipped: every character on an off-grid line must still be matched in our text, in the same order, on the same body page of our free run, or the script fails. For this check the off-grid elements are laid out with the line breaks the reference printed (decided 2026-10-14); everything else keeps our wrapping, and M1 to M4 use our wrapping throughout.
    - **Pages printed off the line grid (decided 2026-10-14).** A reference page whose content baselines (from segments of four or more characters, so a raised superscript does not count) are not a whole number of lines apart, within half a point, has its line spacing off the grid. Only there may an off-grid element land on the page after the reference's; text and order must still match, and the report marks each element that used this.
- **Metrics.**

  | Metric | Definition | Pass |
  | --- | --- | --- |
  | M1 wraps | The share of elements, not split across a reference page, whose line breaks fall at the same characters as ours | ≥ 99% |
  | M2 resynced breaks | For each reference page except the last, `page_from` the cursor where that page starts, and compare our next cursor with where the next reference page starts | ≥ 95% exact per script, and every difference within 2 lines |
  | M3 page count | Body pages from a free run. Title pages are counted separately and must match; the reference's are the sheets before body page 1, found from its printed page numbers when it has any | exact for Brick & Steel and The Last Birthday Card, ±1 for Big Fish |
  | M4 scene starts | The body page of each scene heading in a free run, compared with the reference PDF's | ≥ 90% exact, all within 1 page |

  The .fdx files' recorded scene pages are checked against the PDF first, and count only if they agree completely. Recorded scene lengths are reported, never gated.
- **Causes.** Each break mismatch is rerun with one change at a time: the reference's wraps, the reference's spacing, the reference's line count, inserts in the margin, sentence rule off, then each keep rule relaxed. The first change that fixes it is reported as the likely cause.
- **Report.** The report goes to `target/tmp/coldopen-reference/`, inside the `CARGO_TARGET_TMPDIR` that Cargo gives integration tests. It never contains script text unless `COLDOPEN_REFERENCE_EXCERPTS=1` is set, and then at most 30 characters per excerpt.

**Calibration.** In the event, the profile was fitted on the Final Draft 11 report-only set (§14) and frozen before the 2012 references could be downloaded, then applied to all three unchanged; nothing was fitted on the 2012 files. The plan, kept for any future refit: format values that the reference PDFs pin down are fitted on Big Fish and Brick & Steel only. The Last Birthday Card, with its own margins and sentence breaking off, is the check that the fitted values predict a script they were not fitted on. Values are fitted from counts and positions, never text, and recorded in §14 with the evidence. Those values are:
- line pitch;
- baseline offset;
- how the line count follows from the margins;
- cells per line for each kind;
- hyphen breaks;
- where inserts sit;
- `split_mode`;
- the sentence policy.

## 14. Observed

**Final Draft 11 report-only set** (`corpus/fd11/`, from SimonGZ/jumpcut `tests/fixtures/corpus/public/`, downloaded 2026-10-13; sha256 prefixes):

| Pair | PDF | .fdx | Pages |
| --- | --- | --- | --- |
| big-fish | 3d262d0f5f48 | 4b9269b70a43 | 128 |
| big-fish-scene-numbers | 9adab341cde2 | c2c32fd9094e | 9 |
| little-women | 6ad19538b9fa | 823fd47b73a0 | 16 |

All three PDFs say Final Draft 11 created them through macOS Quartz on US Letter. Their .fdx files use Courier Prime at 12 pt with 72 pt margins.

Measured with `tools/refpdf` and `crates/layout/tests/reference` (numbers only, 2026-10-13):

| Quantity | Observation |
| --- | --- |
| Glyph pitch | 7.00 pt between glyph origins, although each glyph is declared 7.2 pt wide. |
| Line pitch | 12.00 pt. |
| First body baseline | `TopMargin` + 8 pt (80 pt for a 72 pt margin), on 118 of 127 Big Fish body pages; the other 9 start with a continued name one line higher. |
| Page number | Baseline at `HeaderMargin` + 8 pt (44 pt); right edge at 522.2 pt (7.25 in). |
| Body capacity | 54 lines for 72/72 pt margins, the same as `floor(body / 12 pt)`. |
| Cells per line | The largest n with n × 7.0 pt < the column width: action and headings 61, dialogue 35, parenthetical 25 (26 on its hanging first line), character 38. |
| Hanging hyphen | A hyphen ending a line may sit one cell past those counts (lines of 62 cells end in `-`). |
| (MORE) and continued names | In the line slot just outside the body: the continued name one line above the first body line, (MORE) one line below the last. |
| Split mode | Splits at mid-line sentence ends match better than splits only at line ends. |

With those values as a calibration profile (`measure-7.0-strict+margin+overhang` in the harness), the report-only comparison gives:

| Pair | M1 wraps | M2 breaks exact (max Δ) | M3 body pages, ours / theirs | M4 scene starts exact |
| --- | --- | --- | --- | --- |
| big-fish | 100.0% | 96.8% (2 lines) | 127 / 127 | 95.8% |
| big-fish-scene-numbers | 100.0% | 100.0% (0) | 8 / 8 | 100.0% |
| little-women | 99.4% | 100.0% (0) | 16 / 15 | 100.0% |

The little-women PDF is a partial print: its .fdx holds about one more page of script after the PDF's last page.

These are observations of a newer Final Draft. They set the calibration profile; they are not the gate.

**Final Draft 2012 gate set** (`corpus/reference/`, from fountain.io `/_downloads/`, downloaded 2026-10-14; sha256 prefixes):

| Script | PDF | .fdx | Sheets |
| --- | --- | --- | --- |
| big-fish | ac12996fe9e1 | daf2e979d0e8 | 122 |
| brick-and-steel | aed488598484 | c242f797a33b | 5 |
| the-last-birthday-card | 26658d8d7b0f | 19f11c08dd91 | 21 |

All three PDFs say Final Draft created them through Mac OS X 10.7 Quartz on US Letter. Big Fish and Brick & Steel have 72 pt margins and break at sentences; The Last Birthday Card has an 81 pt top and 54 pt bottom margin and does not break at sentences.

Measured the same way (numbers only, 2026-10-14):

| Quantity | Observation |
| --- | --- |
| Glyph pitch | 6.99–7.00 pt, as in the newer version. |
| Line pitch | 12.00 pt. |
| First body baseline | `TopMargin` + 9 pt (81 pt and 90 pt), one point lower than the newer version; on 8 Big Fish pages a continued name sits one line higher. |
| Page number | Baseline 45 pt; right edge 522.0 pt for Brick & Steel and The Last Birthday Card, 533.0 pt for Big Fish. |
| Title pages | Big Fish has two: its title paragraphs need more rows than one page holds, and the rest continue on an unnumbered second sheet (§9.3). The first numbered page, labelled 2, is the fourth sheet. |
| Off-grid lines | 16 Big Fish lines on body pages 72 and 73 print at 6.14–6.40 pt per character, against a median of 7.19 pt elsewhere. They are the 17 italic runs set in "Courier Final Draft"; no other line in the three PDFs is narrower than 6.8 pt per character, and Brick & Steel prints italic runs in the same font at full pitch, so this comes from that machine's fonts, not a layout rule. |

With the frozen profile (`measure-7.0-strict+margin+overhang`), the gate gives:

| Script | Coverage | M1 wraps | M2 breaks exact (max Δ) | M3 body pages, ours / theirs (title) | M4 scene starts exact | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| big-fish | 100.0% / 100.0% | 100.0% (2743/2744; 11 off-grid left out) | 97.4% (114/117, 2 lines; 2 unmeasurable, both off-grid) | 120 / 120 (2 / 2) | 99.5% (201/202) | pass; 10 off-grid elements on the reference's page, 1 on the next page under the line-spacing allowance |
| brick-and-steel | 100.0% / 100.0% | 100.0% | 100.0% (0) | 4 / 4 (1 / 1) | 100.0% | pass |
| the-last-birthday-card | 100.0% / 100.0% | 100.0% | 100.0% (0) | 20 / 20 (1 / 1) | 100.0% | pass |

The Last Birthday Card is the script nothing was fitted on. Without the off-grid rule, Big Fish's break from body page 72 is 3 lines off and fails "all within 2"; its likely cause is the narrow lines on that page. With the rule, 10 of the 11 off-grid elements pass the text check. The eleventh, a one-line speech at the foot of the reference's page 72, opens our page 73 even with the printed line breaks: the substitute font is also shorter, and the reference sets those lines 10–11 pt apart instead of 12 (12 such gaps on body page 72, 4 on page 73, none on any other page of the three references), so it fits about three more rows on that page than a 12 pt grid can. The printed breaks recover one of those rows. Body pages 72 and 73 are the only pages in the three references whose line spacing is off the grid, so the next-page allowance applies there and nowhere else; the eleventh element is the one that uses it.

Known differences that the gate does not measure:
- On Big Fish's second title sheet, the continued lines print two rows below the top; ours print twelve rows down, because we carry over every empty title paragraph and Final Draft carries over fewer. One sample is not enough to infer its rule.
- A Big Fish title paragraph that starts with about 35 tab characters prints its text one row lower, at the left margin; we keep it on its row.
- One Big Fish speech breaks between the two hyphens of a double hyphen; we break before the pair (M1 counts it).

## 15. Incremental layout and draft lines

Added 2026-10-15 for the desktop editor (phase 3). Sources: `docs/spec.md` ("Full repagination, 120 pages: under 50 ms; typical edits repaginate incrementally") and the phase 3 brief. No external material.

**Status:** implemented in `crates/layout` (`Incremental`, `Engine::draft`).

### 15.1 The rule

An incremental layout keeps a laid-out script and accepts edits to it. After every edit its pages, lines, diagnostics, spans and break causes are **exactly** those of laying out the edited script from scratch. Incremental layout is only a faster way to get the same answer. A property test checks the rule against random edit sequences, and the editor's test driver checks it after every keystroke in debug builds.

### 15.2 How an edit is applied

The caller gives the whole edited script, and (2026-10-16) the spacing overrides as they now stand, since an edit before an overridden element changes its index. The flow is assembled again on every update anyway, so new overrides cost nothing extra. The engine keeps the flow and pages of the last layout, and reuses them:

1. **Measurement is memoized.** An element's measure (cells, wrapped lines and sentence starts: §2, §5, §8) depends only on the element and the page format. It is kept between layouts, keyed by the whole element, so an edit measures only the elements it changed. Measures no longer used are dropped after each layout.
2. **The flow is reassembled and compared.** The flow (§4) is assembled again from the measured elements: grouping, spacing, page breaks, continued names and dual pairs, all of which are cheap once measuring is done. The new flow is compared with the old one from the front and from the back, allowing for the shift in element numbers after the edit. This finds the blocks that really changed, including effects that reach past the edited element, such as a scene number or a dual pair.
3. **Pagination restarts one page early.** Pages up to the one before the first page that could see a changed block are kept. A page can look at most three blocks past its last line: the line that does not fit, a heading's two following lines, and a transition after a candidate break. From there, pages are laid out again with `page_from` (§7.1).
4. **Pagination stops where it agrees.** When a new page ends past the changed blocks at a cursor where an old page started (allowing for the change in block count), every later page is the same as before. They are kept, with their element indices and block cursors shifted, their page numbers rewritten, and the spans and diagnostics of §10 gathered again.

**Measured (2026-10-15).** On the synthetic 119-page script of `layout::synthetic`, release build: a full layout takes 16 ms, and an edit on page 90 takes a median of 0.8 ms and lays out at most 3 pages. On GitHub's Linux runner the same test gives 27 ms and 2.0 ms. The aim for an edit is 2 ms; CI fails above 4 ms, a quarter of a 60 Hz frame.

### 15.3 Draft lines

The editor's continuous "draft" view uses the same wrapping as the pages, without pagination:
- each printing element's lines, wrapped whole, at the x positions of §5.2;
- the space above each element as §6 places it when the element is not at the top of a page;
- no (MORE), no continued names, no page numbers.

The editor also marks where each body page starts, using the spans of §10.

## 16. Not yet supported

- Scene "CONTINUED" headers and footers. The format can hold them, the .fdx reader reports them, and they don't print.
- Production revision marks and colors (phase 5b), and locked or A-pages.

## 17. Tracked changes

Added 2026-10-18 (phase 5a). Sources: `docs/specs/changes.md`, the phase 5a brief and its decisions (2026-10-18), and the Okabe–Ito palette (Masataka Okabe and Kei Ito, "Color Universal Design", 2008, <https://jfly.uni-koeln.de/color/>) for print colors that stay distinct for most kinds of color blindness. Status: being implemented.

Layout knows nothing about authors or changes. Core hands it one of the document's two print forms (`docs/specs/changes.md` §9):

- **Accepted:** a script with no marks, laid out exactly as any other.
- **Marked:** text runs keep their `ins` and `del` marks, and core gives `Options` the author slot of each change: a small number, assigned by the order in which authors first appear in the change table, the same in the editor and the PDF.

**What marks change.**

- **Nothing about measuring.** A pending deletion is text that prints, struck through, so it takes its cells, wraps and paginates like any other text. Insertions are ordinary text. The marked form of a document therefore paginates exactly like the same text with no marks, which is what makes the editor's pages and the marked PDF the same pages.
- **Each cell's `Look`** gains `strike` (the cell is in a pending deletion) and `mark`, the author slot of the change that inserted or deleted it (the deletion's, when a cell has both). Runs, page view and source map are otherwise unchanged.
- **Sentence ends** (§8) are found in the text as printed, struck text included.
- **The page view** (§11) shows struck text as its characters, as it shows underlined text, with no marks.

**In the PDF** (§12):

- A cell with a `mark` is drawn in its author's color. Slots are taken in turn from this list, starting again after the last: `#0072B2`, `#D55E00`, `#009E73`, `#CC79A7`, `#3C3C99`, `#8F5A00`. The first four are Okabe–Ito colors; the last two are darker blends of its blue and orange, since its lighter colors are too faint for text on white.
- A struck cell gets a filled rectangle at the face's strikeout position and thickness (from the font's `OS/2` table; 0.3 em and 0.05 em when the face has none), covering its cells, in the same color, drawn with the text, as underline is.
- Bytes stay deterministic: the colors are fixed numbers, and the drawing order follows the cells.
- Fallback fonts, right-to-left text and complex scripts.
- Selectable templates (A4, television).
