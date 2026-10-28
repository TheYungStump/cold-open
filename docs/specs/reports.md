# Reports, and how names are read

- **Date:** 2026-10-22
- **Status:** phase 5b, being implemented in `crates/screenplay` (`names`: speakers and scene headings), `crates/core` (the `report.*` commands), `crates/cli`, `crates/mcp` and `crates/app`.
- **Sources:**
  - `docs/spec.md`: "Output" (reports: scenes, characters with lines and words per character, and locations), and the `report.characters` row of "Command interface".
  - The phase 5b brief and the decisions taken with the project's owner on 2026-10-22 (`docs/progress/phase-5b.md`).
  - The rules already in the code, which this spec writes down and gathers into one place: the character name before any `(` (`Element::character_name`), the continued-name check (`docs/specs/layout.md` §4.6 and §7.5), the scene prefixes of the Fountain reader (`docs/specs/fountain.md` §6.2 rule 5), and the editor's location and time completion (`docs/specs/editor.md` §6).
  - RFC 4180, for the CSV form.
- No Final Draft material. A choice where the sources are silent is marked **Resolution**.

## 1. Goals

1. **One reading of names.** Layout, the editor's completion, the navigator and the reports read character names and scene headings with the same rules, from one module, so a report never disagrees with the page.
2. **Counts anyone can check by hand.** Every number is defined below, and our own fixtures hold hand counts.
3. **The same output everywhere.** Rows come in a fixed, documented order, sorted, never in hash-map order.

## 2. Speakers

A **speaker** is read from a character block's current text (`docs/specs/changes.md` §2.1):

1. **The name** is the text before the first `(`, trimmed. A name with nothing before its `(` is empty, and such a block has no speaker.
2. **The extensions** are the text inside each pair of brackets after the name, trimmed and upper-cased, in order: `MAYA (V.O.) (CONT'D)` has the extensions `V.O.` and `CONT'D`. An unclosed bracket runs to the end of the text.
3. **The key** compares speakers. It is the name in upper case (Unicode mapping), with runs of whitespace as one space and `’` read as `'`. `Maya`, `MAYA` and `MAYA (O.S.)` are one speaker.
4. **The continued marker** is an extension that reads, in upper case with `’` as `'`, as `CONT'D`, `CONTD` or the page format's continued string (`docs/specs/layout.md` §7.5). Layout uses this to decide whether to add `(CONT'D)`; reports ignore it.
5. **A dual-dialogue character** (`dual`) is a speaker like any other.

## 3. Scene headings

A heading's text, current text, read in upper case:

1. **The prefix.** The text up to the first space is matched against the scene prefixes, in this order, each followed by `.` or a space: `INT./EXT`, `INT/EXT`, `EXT./INT`, `EXT/INT`, `I/E`, `E/I`, `INT`, `EXT`, `EST` (the Fountain reader's list, `docs/specs/fountain.md` §6.2, with its reversed forms). A trailing `.` belongs to the prefix.
2. **The setting** the prefix names:

   | Prefix | Setting |
   | --- | --- |
   | `INT` | `int` |
   | `EXT` | `ext` |
   | `INT./EXT`, `INT/EXT`, `EXT./INT`, `EXT/INT`, `I/E`, `E/I` | `int_ext` |
   | `EST` | `est` |
   | none | `none` |

3. **The time** is the text after the last ` - ` (space, hyphen, space), trimmed, when there is one. **Resolution:** `--` and `—` are not separators, since some locations hold them.
4. **The location** is what is left between the prefix and the time, trimmed, with a trailing ` -` removed. A heading with no prefix is all location.
5. **The location's key** is the location with runs of whitespace as one space. `INT. LAB - NIGHT` and `EXT. LAB - DAY` are one location in two settings.

The editor's completion (`docs/specs/editor.md` §6) offers locations and times read by these rules.

## 4. The reports

Each report is a query, with no params, laid out as `scene.list` lays the document out (its print form with pending changes marked, `docs/specs/changes.md` §9), so its pages are the pages on screen.

### 4.1 `report.scenes`

One row per scene heading, in script order:

| Field | Meaning |
| --- | --- |
| `id`, `index`, `number`, `heading`, `page`, `eighths` | As in `scene.list` (`docs/specs/commands.md`). |
| `setting`, `location`, `time` | The heading read by §3; `time` may be null. |
| `characters` | The speakers' names in the scene, in order of first speech, each as it was first written (its case and spacing, without extensions). |
| `words` | Words in the scene's printing blocks, as `scene.list` counts them. |

### 4.2 `report.characters`

One row per speaker key:

| Field | Meaning |
| --- | --- |
| `name` | The name as it is first written in the script. |
| `speeches` | Character blocks with this key that have a speech after them: at least one dialogue or lyric block before the next character, scene heading or other printing block. |
| `lines` | Printed lines of those speeches' dialogue and lyric blocks, from layout: what a reader counts on the page, continued lines after a page break included. |
| `words` | Words in those dialogue and lyric blocks, by the word rule `scene.list` uses. Parentheticals don't count. |
| `scenes` | The ids of the scenes the speaker speaks in, in script order. |
| `extensions` | The extensions used with the name, in order of first use, without the continued marker. |
| `first_page` | The page of the first speech. |

- **Order:** most words first, then by key. **Resolution:** words, because lines depend on where the page breaks fall, and speeches count a one-word reply like a monologue.
- A character block with no speech after it still adds its scene and extensions, and counts no speech.
- Dual dialogue counts for both speakers.

### 4.3 `report.locations`

One row per location key, sorted by key:

| Field | Meaning |
| --- | --- |
| `location` | The location as first written. |
| `settings` | One entry per setting it appears in, in the order `int`, `ext`, `int_ext`, `est`, `none`: `{setting, scenes, times}`. |
| `times` | Within a setting, one entry per time key (its text in upper case, whitespace as one space), sorted by key, with no time last: `{time, scenes}`. |
| `scenes` | Scene ids, in script order. |

## 5. Front ends

- **Commands:** `report.scenes`, `report.characters`, `report.locations` (`docs/specs/commands.md`).
- **CLI:** `coldopen report scenes|characters|locations FILE [--format text|json|csv]`. The text form is a table for people, with the counts right-aligned. JSON is the command's result. CSV has one header row, and lists inside a field are joined with `; `.
  - **CSV:** RFC 4180, with CRLF line ends. Fields holding a comma, a quote, CR or LF are quoted, with quotes doubled. Written by our own code, with no dependency.
  - In the CSV form of `report.locations`, there is one row per location, setting and time.
- **MCP:** the tools `report_scenes`, `report_characters` and `report_locations`, and the resources `coldopen://report/scenes`, `coldopen://report/characters` and `coldopen://report/locations` (`docs/specs/mcp.md` §4).
- **The app:** "Reports…" (palette) opens a view with three tabs (Scenes, Characters, Locations), in the same order as the commands. Up and Down choose a row; Enter goes to its scene, or a character's first speech; Ctrl+Tab changes tabs; Esc closes. The view refreshes when the navigator does.

## 6. Testing

- **Unit tests** of `names` for every prefix, the reversed forms, extensions, unclosed brackets, curly apostrophes and headings without a time or a prefix.
- **Fixtures** (`tests/fixtures/reports/`): two short scripts we write ourselves, with hand counts in a checked-in file beside each. One has dual dialogue, (V.O.) and (CONT'D), a speech across a page break, and INT./EXT. and EST. headings.
- **Determinism:** each report of the synthetic 119-page script is the same on every OS, checked against a golden file.
