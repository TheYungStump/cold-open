# Fountain: how Cold Open reads and writes it

- **Date:** 2026-10-13
- **Updated:** 2026-10-13 (phase 2): reversed scene heading prefixes (§4, §6.2, §9). 2026-10-15 (phase 3): the normal form's source map (§10). 2026-10-16: hidden-only lines inside an element (§5); time in proportion to the input (§1); a document read from .fdx (§11). 2026-10-17: an opening paragraph that starts with U+FEFF, and a dual-dialogue name that reads as a scene heading (§9); title values brought to normal form after emphasis (§4); the ordered emphasis arrangement, styles that emphasis cannot spell and the round-trip check (§9, §10). 2026-10-18 (phase 5a): normalisation reported as a `normalized` loss (§10, F21); tracked changes (§12).
- **Status:** implemented in `crates/fountain` (phase 1)
- **Sources:**
  - The public Fountain syntax page, <https://fountain.io/syntax>, read 2026-10-13.
  - The CommonMark specification 0.31.2, <https://spec.commonmark.org/0.31.2/>, for the emphasis and backslash-escape rules. The Fountain page defers to the Markdown conventions for both.
  - The phase 1 review of real scripts in our private corpus, 2026-10-13, for the reversed scene heading prefixes (§6.2). Only the prefix spellings were noted; no script text was copied.
- No other material was used. This document is written in our own words. Where the syntax page leaves a case open, the rule we chose is marked **Resolution**.

## 1. Goals

1. **Never fail.** Any input string produces a script. Anything the parser cannot classify becomes Action, and problems are reported as diagnostics, never as panics.
2. **Round trip by meaning.** `parse(write(parse(x))) == parse(x)` for every input `x`. Writing produces one canonical text, so writing twice gives the same bytes.
3. **Report loss.** When the model holds something Fountain cannot express, the writer says so (§9) instead of dropping it silently.
4. **Time in proportion to the input** (2026-10-16, review finding F15). Reading and writing take time roughly proportional to the text's length, even for hostile input, because the app opens files on its interface thread and a slow file looks like a hang. Two places were quadratic: a speech's parentheticals (each line starting with `(` searched the rest of the speech for a `)`), and emphasis (the delimiter stack shifted on every removal, and each matched pair restyled every token inside it). `crates/fountain/tests/hostile_time.rs` checks that four times the input takes about four times as long, and the fuzz runs fail any input over two seconds.

## 2. Model produced

The parser produces a `Script`: an optional title page and a flat list of elements. Each element has:

- a kind
- rich text, as a list of runs: plain or styled text, inline notes and inline boneyard
- a few kind-specific attributes: scene number, dual flag, centered flag, section depth, and `space_before`

Line breaks inside an element are `\n` in its text.

The element kinds fall into two groups:

- **Printing:** scene heading, action, character, parenthetical, dialogue, transition, lyric, shot, general.
- **Transparent:** section, synopsis, note, boneyard and page break. They occupy no lines in the printed script, and they are skipped when measuring the spacing between printing elements.

## 3. Reading the input

- A leading byte-order mark (U+FEFF) is removed.
- Lines end at CRLF, LF or a lone CR, in any mix.
- A final line terminator does not create an extra empty line.
- Every line falls into exactly one of these classes:

| Class | Definition |
| --- | --- |
| **Blank** | Empty, or whitespace only and shorter than two characters. |
| **Hold** | Whitespace only (spaces or tabs), two or more characters. This is the syntax page's "two spaces" line. Inside dialogue and inside notes it keeps the block together as an empty line. Everywhere else it behaves exactly like a blank line. |
| **Content** | Anything else. |

**Resolution:** A hold line holds only inside dialogue and notes, where the syntax page describes it. Elsewhere it is a blank line. A stray indented "empty" line from a code editor therefore can't turn a scene heading into Action.

## 4. Title page

The title page is looked for in the first paragraph of the file, after skipping any leading blank lines. A paragraph here means the lines up to the first blank or hold line. The paragraph is a title page only if all of these hold:

- **Every line is either:**
  - a **key line**: a key, a colon, then an optional inline value. The key starts with a letter or digit, then continues with letters, digits, spaces, or the characters `-` `_` `.` `'` `/` `&`. It does not end in a space.
  - a **continuation line**: three or more spaces, or a tab, then a value. It belongs to the key above it.
- At least one key has a value, either inline or on a continuation line. This keeps a script that opens with `FADE IN:` from being read as a title page.
- The first line is not a scene heading by prefix (§6.2 rule 5). This uses the reader's full prefix list, reversed forms included, so a file that opens with `EXT/INT. CAR: DAY` has no title page.

Each key keeps its spelling and its position. Its values are the inline value (if not empty) followed by each continuation value, each trimmed and parsed for emphasis. Keys are not interpreted. The usual ones (Title, Credit, Author/Authors, Source, Draft date, Contact) are matched without regard to case by code that needs them. Unknown keys are kept.

**Resolution (2026-10-17):** emphasis can take up its markers and leave whitespace at a value's edge. In `z: */**/* *` the `*` before and after an empty boneyard close as an empty italic, leaving " *". So each value is brought to its normal form (§10) after it is parsed, as element text is: trimmed again and cleared of bold or italic on edge whitespace, with hidden runs kept. A value left with nothing is dropped. When that leaves no key with a value, the paragraph is not a title page. Found by the `fountain_roundtrip` fuzz target.

**Resolution:** The title page ends at its first blank line. The script starts after it.

## 5. Notes and boneyard (hidden text)

These are found before any classification, scanning the text after the title page in order. Backslash-escaped characters (§8) are skipped by the scan.

- **Boneyard** runs from `/*` to the next `*/`. It may cross any number of lines, including blank ones, and its content is kept verbatim. If there is no closing `*/`, the `/*` is ordinary text and a diagnostic is reported.
- **Note** runs from `[[` to the next `]]`. It may cross lines, including hold lines, which become empty lines in the note text. A blank line ends the search: the `[[` is then ordinary text and a diagnostic is reported. Note content is kept as plain text.
- Whichever opener comes first wins, and the other construct is just content inside it.

After this pass, every line is a mix of visible text and hidden pieces. A hidden piece can contain line breaks without splitting the visible line it sits in.

A line whose visible text is only whitespace but that holds a hidden piece is a **hidden-only line**. What happens to it:

For this rule, a *run* is the lines between blank lines; hold lines belong to the run but are not content lines.

- If its run has no content lines at all, each of its pieces becomes a **Note** or **Boneyard element** of its own. This matches the syntax page: a note on its own line does not leave empty lines behind.
- Otherwise its pieces join the nearest content line in the same run, as inline runs. They join the end of the previous content line if there is one, otherwise the start of the next. The hidden-only line itself disappears, so no visible empty line is created.
- **Resolution** (2026-10-16): the same holds for a line that becomes hidden-only inside an element after its markers are removed, such as an empty lyric line `~` followed by a note that spans lines. The reader joins it to the line before, as the normal form does (§10), so what it reads is always in normal form. The round-trip property test found this case.

## 6. Classifying paragraphs

A **paragraph** is a run of content lines between blank lines (or hold lines, outside dialogue). Classification looks at the visible text of each line. Leading whitespace is ignored when checking for marker characters (Indenting rule). Lines are then trimmed, with the exceptions stated below.

A check marked *paragraph start* only applies to the first line of a paragraph. The same applies to the first line after a section, synopsis or page-break line (§6.1), which also counts as a paragraph start.

### 6.1 Line-level markers (any line outside a dialogue group)

| Line, after leading whitespace | Element |
| --- | --- |
| Only `=` characters, three or more | **Page break** |
| Starts with `#` | **Section**. The depth is the number of leading `#`; the text is the trimmed rest. |
| Starts with `=` (and is not a page break) | **Synopsis**. The text is the trimmed rest after one `=`. |
| Starts with `~` | **Lyric**. The text is the trimmed rest after `~`. Consecutive lyric lines form one element. |
| Starts with `>`, ends with an unescaped `<`, and has non-empty text between them | **Centered Action**. The text between the brackets is trimmed. Consecutive centered lines form one element. |

When one of these interrupts a paragraph, the lines before it end their element, and the lines after it continue as their own element.

### 6.2 Paragraph start

The rules are tried in this order:

1. **`!` (forced Action).** The `!` is removed, and the rest of the paragraph is Action (§6.4).
2. **`@` (forced Character).** The `@` is removed, mixed case is kept, and a dialogue group follows (§6.3). A forced character does not need a following line.
3. **`.` followed by a letter or digit (forced Scene Heading).** The `.` is removed. A line starting with `..` is never forced, so action and dialogue can begin with an ellipsis. A forced heading does not need a blank line after it.
4. **`>` not matched by §6.1 (forced Transition).** The `>` is removed.
5. **Scene Heading by prefix.** The line must be the paragraph's only line (a blank line follows). It must start, ignoring case, with `INT`, `EXT`, `EST`, `INT./EXT`, `INT/EXT`, `I/E`, `EXT/INT` or `E/I`, followed by `.` or a space. `EXT./INT` needs no entry of its own: it starts with `EXT` followed by `.`.
6. **Transition by shape.** The line must be the paragraph's only line, contain no lowercase letters, and end in `TO:`. It is checked **before trailing whitespace is removed**, so the syntax page's escape (a space after the colon) makes it Action.
7. **Character by shape.** Another line must follow in the paragraph (a hold line counts). The name is the line with any trailing `^` removed, up to its first `(`. The name must contain at least one uppercase letter and no lowercase letters, so "R2D2" qualifies and "23" does not. The extension in parentheses may be in any case.
8. **Otherwise, Action** (§6.4).

**Resolution (2026-10-13):** The syntax page lists only six prefixes: `INT`, `EXT`, `EST`, `INT./EXT`, `INT/EXT` and `I/E`. Writers also use the reversed forms `EXT/INT` and `E/I`; the phase 1 review of real scripts found them in use. The reader accepts them as well. A reader that follows only the syntax page would not, so the writer always forces a heading that starts with a reversed prefix with `.` (§9). The text stays portable and reads back the same.

**Resolution:** Scripts without letter case have no uppercase letters, so a Chinese name never matches rule 7 by itself. Use `@`, as the syntax page recommends.

**Resolution:** After rules 3 and 4, any remaining lines of the paragraph do not start a new paragraph. They become Action (§6.4) placed directly below, with `space_before` set to 0.

**Scene numbers.** For any scene heading, a trailing `#…#` is the scene number when it holds only ASCII letters, digits, `-` and `.`, and its opening `#` is not escaped. It is removed from the heading text.

**Dual dialogue.** For any character, a final unescaped `^` sets `dual`. It may be preceded by spaces, which are ignored.

### 6.3 Dialogue groups

After a Character line, each following line belongs to the character's group until a blank line ends it. Hold lines do **not** end the group.

| Line in the group | What happens |
| --- | --- |
| Starts with `(` | **Parenthetical**. If it ends with `)`, it is one line. Otherwise the parenthetical continues to the first later line in the group that ends with `)`. If there is no such line, the `(` line is dialogue. |
| Starts with `~` | **Lyric** line. Consecutive lyric lines form one element. |
| Hold line | Adds an empty line to the current dialogue. If the current element is not dialogue, it starts a new dialogue with an empty line. |
| Anything else | **Dialogue**. Consecutive dialogue lines form one element, joined with `\n`. |

**Resolution:** Inside a group, `#`, `=`, `===`, `>` and `!` are ordinary dialogue text. Dialogue such as "#1 fan" stays dialogue.

### 6.4 Action

- Action keeps each line's leading whitespace, with tabs expanded to four spaces.
- Trailing whitespace is removed.
- Lines continue the element until a blank or hold line, or a §6.1 marker line.
- A centered line splits the action: the parts before and after it become separate elements with `space_before` set to 0, so the printed spacing doesn't change.

### 6.5 Trimming

All elements except Action have leading and trailing whitespace removed from each line.

## 7. Spacing (`space_before`)

The **gap** before a printing element is the longest run of consecutive blank lines between it and the previous printing element. A transparent element in between adds no blank lines and ends the current run, so blank lines on both sides of a section count once, not twice. Lines that belong to the same paragraph have a gap of 0.

The **default gap** is:

- 0 for a parenthetical, dialogue or lyric whose previous printing element is a character, parenthetical, dialogue or lyric;
- 1 otherwise.

How the gap is stored:

- If the gap equals the default, `space_before` is left unset.
- Otherwise `space_before` records the gap. This is how Fountain's "vertical whitespace" (extra blank lines between action paragraphs) survives a round trip.
- The first printing element of the script never records a gap.
- Transparent elements never record one.

## 8. Inline text

Inline text is parsed one visible line at a time. Hidden pieces stay in place as runs and do not break emphasis around them.

- **Backslash escapes** follow CommonMark. A backslash before any ASCII punctuation character makes that character literal and is removed. Any other backslash is literal.
  - **Resolution:** This goes beyond the syntax page's examples, which only show `\*`. It follows the Markdown convention the page cites, and it gives the writer a way to escape every special sequence.
- **Emphasis** follows the CommonMark delimiter-run algorithm, using only `*` and `_`:
  - `*x*` is italic.
  - `**x**` is bold.
  - `***x***` is bold italic.
  - `_x_` is underline (never italic).
  - Left- and right-flanking rules apply, and `_` cannot open or close emphasis inside a word. `*69` after a space is therefore literal, while `*69 … 23*` is italic.
  - Delimiters left without a partner are literal text.
  - Emphasis never crosses a line break.

## 9. Writing

The writer produces one canonical text, ending in a single `\n`. An empty script gives an empty string.

**Title page.**

- A key with one value is written `Key: value`.
- A key with several values is written as the key line `Key:` followed by each value on its own line, indented four spaces.
- One blank line separates the title page from the script.

**Spacing.** Each printing element is preceded by its gap, taken from `space_before` or the default.

- When transparent elements sit between two printing elements and the gap is 0, the transparent elements are written on consecutive lines.
- Otherwise the gap's blank lines go first, and each transparent element is followed by one blank line.
- Note and boneyard elements always get a blank line on both sides.

**Element syntax.**

| Element | Written as |
| --- | --- |
| Scene number | `␠#12A#` after the heading |
| Dual dialogue | `␠^` after the name |
| Centered line | `>text<` |
| Section | `#` × depth, a space, then the text |
| Synopsis | `=`, a space, then the text |
| Page break | `===` |
| Note element | `[[…]]` |
| Boneyard element | `/*…*/` |

Empty lines inside dialogue or a note are written as two spaces.

**Forcing and escaping.** The writer adds a marker only where the canonical text would otherwise be read back differently. It checks this with the parser's own classifier.

- **Forcing markers** at paragraph start: `!` for Action, `@` for Character, `.` for Scene Heading, `>` for Transition.
  - A scene heading is left unforced only when it starts with one of the syntax page's six prefixes. One that starts with a reversed prefix (`EXT/INT`, `E/I`) is always forced, as in `.E/I CAR - NIGHT`, even though our own reader would not need it.
  - Every other check uses the reader's full prefix list. Action such as "E/I is a label." on its own line therefore gets a `!`.
  - **Resolution (2026-10-17):** a character is checked as its line is written, with the ` ^` of dual dialogue. A dual-dialogue character named `EST` or `INT`, followed by a hold line, is written `@EST ^`: `EST ^` alone reads as a scene heading. Found by the round-trip property test.
- **Line-start escapes:**
  - `\#`, `\=`, `\~` and `\>` on action lines that would read as markers;
  - `\(` and `\~` on dialogue lines;
  - `\^` on a character name that ends in a caret.
- **Inline escapes:**
  - `\*` and `\_` where a literal asterisk or underscore would otherwise become emphasis;
  - `\\` before punctuation;
  - `\[` before `[[`;
  - `\/` before `*`, so a literal `/*` never opens boneyard (escaping the `/` works even when the `*` is an emphasis marker).
- **Emphasis around notes and boneyard.** Markers are placed in the first of several arrangements that reads back exactly. A style that carries on past a note stays open across it (`**a [[n]] b**`). **Resolution (2026-10-14):** when the style changes right after a note or boneyard, closing the old style there can put two delimiter runs side by side (`**bold[[n]]***it*`), which reads back differently. The writer then also tries closing the old style before the hidden span (`**bold**[[n]]*it*`). It tries that last, so lines that already read back keep their written form. Found by the round-trip property test with a multi-line note.
- **Resolution (2026-10-17): the ordered arrangement.**
  - Which arrangement reads back depends on the characters next to the markers. The flanking rules (§8) treat punctuation differently from letters, and U+FFFD, the character invalid UTF-8 is read as, counts as punctuation. So the same styles, underline, then italic and underline, then italic, read back from `_a*b*_*c*` between letters but need `_é_*_�_�*` between punctuation.
  - The writer therefore also tries an **ordered** arrangement. At every run the longest-lasting style is outermost, and a shorter style already open is closed and reopened inside it. It comes after the other arrangements, so a line that already read back keeps its written form.
  - Found by the `fountain_roundtrip` fuzz target.
- **Resolution (2026-10-17): styles emphasis cannot spell.**
  - Emphasis must nest, so some styling an editor can apply has no spelling that reads back:
    - **Crossing phrases.** With bold on `a b c` and italic on `b c d`, the space after `c` would have to be italic without being bold.
    - **A style edge between two punctuation marks inside another style**, such as a bold `--` inside an italic `"--"`. A marker between two punctuation marks can both open and close, so it ends the outer style early.
    - **Two styles that cross inside a word**, such as italic on `"déjà` and bold on `déjà?`.
  - When no arrangement reads the line back exactly, the writer rewrites the line in fixed steps, trying the usual arrangements after each, before it writes the line plain. There is no search.
    1. Bold and italic come off every whitespace character of the line. They do not print there, so this is not a loss and is not reported. It extends the normal form's rule for edge whitespace (§10). It goes first because it loses nothing.
    2. Underline comes off the whitespace at the ends of each underlined stretch (reported).
    3. Underline comes off every whitespace character as well (reported, since underline prints).
    4. Where two styles cross inside a word, the one that starts later comes off that whole word (reported). The rest of the line keeps its styles.
    5. The line is written as plain text (reported).
  - The writer's own read-back check (§1, goal 3) and the round-trip check set aside bold and italic on whitespace for the same reason.
  - **The round-trip check** (the `fountain_roundtrip` fuzz target and the property tests) passes a style loss when it was reported for that element, or for the title page, and the text reads back otherwise identical. It fails on any other difference that was not reported, and on any change to the text itself. Before 2026-10-17 it allowed a style loss only when the input held four or more `*` and `_` side by side, and it then read nothing back. Only a lossless write must come out the same when written again: after a reported loss, the next write may spell the read-back text differently (an escape it no longer needs).
  - A property test of ordinary prose, with styles on whole words and phrases, holds the writer to this. What emphasis can spell writes with no loss and reads back exactly. Crossing styles lose only what the steps above say. The rates are recorded in `docs/progress/phase-4.md` under "Deferred".
  - Found by that property test, written after three emphasis findings by the `fountain_roundtrip` fuzz target in two days.
- If the script has no title page but its first paragraph would read as one, that paragraph is forced.
- **Resolution (2026-10-17):** the same holds when, with no title page, the first paragraph starts with U+FEFF. A reader removes that character as a byte-order mark (§3), so the paragraph is forced and the character stays its text (`!` before an action). Only a U+FEFF that is not at the start of the input reaches the model, as in a blank line followed by the character. Found by the `fountain_roundtrip` fuzz target.
- A line inside an element that holds only notes or boneyard would join the line next to it when read back (§5). The writer wraps it in an empty emphasis pair, `*[[note]]*`, which prints nothing but keeps it a line of its own.

**Losses.** These are reported, never silently dropped:

| Content | What the writer does |
| --- | --- |
| Shot elements | Written as forced scene headings |
| General elements | Written as Action |
| Line breaks in single-line elements (scene heading, character, transition, section, synopsis, title-page values) | Become spaces |
| A scene heading whose text doesn't start with a letter or digit, and that has no prefix | Written anyway; reads back as Action |
| A parenthetical not wrapped in brackets | Written as is |
| `]]` inside a note, or `*/` inside a boneyard | Written as is |
| Underline on whitespace that emphasis cannot keep: at the ends of an underlined stretch, or where two styles cross at a space (2026-10-17: the second) | Underline taken off that whitespace |
| Two styles that cross inside a word (2026-10-17) | The style that starts later taken off that word |
| Other styles emphasis syntax cannot express: mid-word underline, and emphasis that only packed or mixed `*` and `_` can spell (2026-10-17; before, "emphasis that came from four or more delimiter characters packed together") | Written with the closest styles the steps above find, or as plain text |
| Spacing the reader cannot reproduce (for example, a gap of 0 before a scene heading) | Written with the nearest spacing the reader can reproduce |

## 10. Normal form

The writer's output for a script `s` reads back as `s` exactly when `s` is in **Fountain normal form**:

- No element on the loss list above.
- Text runs merged where their styles match.
- No leading or trailing whitespace except on Action lines.
- No bold or italic on whitespace at the ends of a bold or italic stretch. It cannot be written and does not print. Underline, which prints, stays.
  - **Extended by the writer (2026-10-17):** on a line it cannot otherwise write, the writer takes bold and italic off all of the line's whitespace (§9). That line reads back without them, and nothing is reported, since nothing that prints changed. The read-back checks compare with bold and italic on whitespace set aside.
- No empty lines inside Action.
- No two adjacent elements that the reader would merge, such as two dialogue elements with no gap, or two non-centered action elements with gap 0.

Every script the parser produces is in normal form. The property tests generate only normal-form scripts.

**Normalisation is reported as a `normalized` loss** (noted 2026-10-16, review finding F21 and Antigravity's 12; settled 2026-10-18, phase 5a). A document held in memory need not be in normal form: commands can put text in it that the normal form changes. Writing it then changes the document in ways that do not change what prints:
- leading and trailing spaces on dialogue and other non-action lines, and trailing spaces on action lines;
- an action holding only spaces, which becomes empty;
- an action beginning with blank lines, which lose them, and one with several blank lines inside, which splits into elements with `space_before`;
- two adjacent dialogue elements, which join into one;
- a dialogue line `=== ` that keeps only `===`;
- a note at the start of an action line, followed by a line break, which loses the break;
- trailing blank lines in the last action of the script, which are dropped (in any other action they become the next element's `space_before`).

It matters in phase 5, where script diff compares documents block by block: "one block became two" or "a trailing space vanished" would show as a change nobody made. The 2026-10-16 note left two options: report these as a `normalized` loss class that never blocks a save, or have the `block.*` and `text.*` commands normalise as they apply, so the document never holds what the file cannot.

**Decision (2026-10-18): the `normalized` class.**

- **Why not normalise in the commands.** The editor sends one `text.insert` per key, and the document must hold a trailing space the writer has just typed before the next word arrives (`docs/specs/editor.md` §4). A command that trimmed it would eat the space. Normalising commands would also mean different things in different formats, since .fdx and bundles keep text exactly.
- **Why tracked changes need neither.** They record marks as edits are made, and never compare documents (`docs/specs/changes.md`).
- **What the writer does.** It lists one `normalized` loss for each element the normal form changes, with a message naming the case: spaces at the edges of lines, an action of spaces only, blank lines at the start or inside of an action, dialogue joined to the dialogue before it, a `=== ` line in dialogue, a line break after a note at the start of an action line, and blank lines at the end of the script's last action (A12). The loss names the first element involved. A `normalized` loss never blocks a save (`docs/specs/commands.md`, "Losses"), and the app never asks about it.
- **Comparisons.** Anything that compares a document with a Fountain file read back (the recovery dialog's changed scenes, phase 5b's script diff) compares the normal form of each, so a normalisation never shows as a change, and the loss list says what the normal form did.
- **The recovery check.** "Nothing to recover?" in `docs/specs/journal.md` counts only `lost` losses, as it counted none of these before, and relies on the file's fingerprint first, so a file outside normal form is not taken for a change.

**Source map** (2026-10-15, for the desktop editor). Layout runs on the normal form (`docs/specs/layout.md` §4.8), but the editor places its caret in the document's own elements, so it needs to know where each piece of normal-form text came from. `normalize_with_source_map` returns the normal form and, for each of its elements:
- the index of the source element it starts in;
- a list of **segments**. Each segment is a byte range of the element's printed text, together with the source element and byte offset in that element's printed text that the range was copied from.

Text the normal form adds has no segment: the `\n` that joins two merged elements. Text it removes appears in no segment: trimmed edge whitespace, and the empty lines that splitting an action turns into spacing. Segments are in order and never overlap. The plain `normalize_with_map` gives the same element indices.

## 11. Writing a document read from .fdx

Added 2026-10-16 (phase 4). Sources: `docs/specs/fdx.md`, `docs/specs/layout.md` §9.3, and the phase 4 decisions (`docs/progress/phase-4.md`).

A document read from an .fdx file can hold what Fountain cannot, besides the element kinds of §9. Saving it as Fountain ("Save as" converts, `docs/specs/commands.md`) reports these losses too, before anything is written:

| Code | Content | What the writer does |
| --- | --- | --- |
| `page_format` | A page format other than the default: page size, margins, element settings | Writes the script; a Fountain file always prints with the default format |
| `paragraph_spacing` | A paragraph's space above, in points, that is not a whole number of lines | Writes the nearest whole number of blank lines. An override that is a whole number of lines is written as that many blank lines (`space_before`) and is not a loss |
| `title_page_layout` | Title lines with their own positions | Writes them as title-page keys, as below; their positions are not kept |
| `fdx_data` | The .fdx passthrough store: settings, smart-type lists, revision sets and everything else Cold Open keeps but does not use (`docs/specs/fdx.md` §13), listed by section name | Leaves it out |

**Title lines as keys.** Lines are taken in order and grouped into blocks separated by empty lines. The first centered block is `Title`, one value per line. A centered block of one line ending in "by" (ignoring case and spaces) is `Credit`. The centered block after a credit is `Author`, and any other centered blocks are `Source`. Left-aligned blocks after the title are `Contact`, and right-aligned blocks are `Notes`. Styles in the lines are kept as emphasis (§8). This is a rule of thumb for English title pages, which is why `title_page_layout` is always reported.

## 12. Writing a document with tracked changes

Added 2026-10-18 (phase 5a). Sources: `docs/specs/changes.md`, the phase 5a brief. Fountain has no syntax for tracked changes, and an inline note is not one: it would change the script text another app reads. So a document with pending changes is written from its accepted form (`docs/specs/changes.md` §9), the script as accepting every change would leave it, and a save or export reports one loss for the document:

| Code | Content | What the writer does |
| --- | --- | --- |
| `tracked_changes` | Pending tracked changes, counted in the message | Writes the current text with every change applied |

It is of class `lost`, so the writer is asked first. The desktop app's question also offers "Save as Cold Open project…", which keeps everything (`docs/specs/editor.md` §11).
