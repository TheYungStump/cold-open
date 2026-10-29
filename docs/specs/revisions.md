# Production revision sets

- **Date:** 2026-10-22, updated 2026-10-26 (§6.4, text put back) and 2026-10-28 (§9, "Save anyway" as one step after its checks)
- **Status:** phase 5b, being implemented in `crates/screenplay` (marks in text runs), `crates/core` (the revision table, marking, the commands, files), `crates/layout` (asterisks, page headers, colored pages) and `crates/app`.
- **Sources:**
  - `docs/spec.md`: "Revising" (colored revision sets with asterisks in the margin and revised-page headers) and "Output" (PDF export with revision colors).
  - The phase 5b brief and the decisions taken with the project's owner on 2026-10-22 (`docs/progress/phase-5b.md`).
  - Wikipedia, "Shooting script", <https://en.wikipedia.org/wiki/Shooting_script>, read 2026-10-22: a typical order of revision page colors, the repeat round named "double white", asterisks in the right margin, and the revision's date and color at the top of each revised page.
  - Scriptation, "What Are Blue Pages? Script Revision Colors and Order", <https://scriptation.com/blog/what-are-blue-pages_script-revision-colors/>, read 2026-10-22: the same order through goldenrod, the second round, and the color named in a PDF page's header; also that productions vary the order (UK productions often swap pink and blue).
  - `docs/specs/changes.md`, `docs/specs/bundle.md`, `docs/specs/journal.md`, `docs/specs/layout.md`.
- No Final Draft material, and no other application's revision behaviour, was used. The color order is an industry convention restated from the two public pages above. Everything else is our own design, in our own words. A choice where the sources are silent is marked **Resolution**.

A production revision set is a permanent record of what changed in one distributed draft. While a set is active, every change to what prints marks the text it touches with that set. The marks stay after the set ends, and they print as an asterisk in the right margin and as the set's name and date at the top of the page.

Revision sets are separate from track changes (`docs/specs/changes.md`). A tracked change is a pending decision. A revision mark records a decision already made.

## 1. Goals

1. **A record, not a proposal.** Marks never change the text. They say which distributed draft last changed a line.
2. **One place.** Marking is done in the dispatcher, from the document's active set, so every front end gets the same marks. No front end marks text itself.
3. **Undo means undo.** A change's marks are part of its undo step: undoing the change takes them away, and redoing it puts them back.
4. **Replay means what it meant.** The active set is document state, journaled with every command that changes it, so replay marks exactly what the edits marked (`semantics` 3, `docs/specs/journal.md`).
5. **The page tells the truth.** Every line whose printed form changed while a set was active carries an asterisk, including lines where text was removed.

## 2. The colors

### 2.1 The convention

Productions print each round of revised pages on paper of a new color, in an order agreed before shooting. The usual order, which both sources give, is:

**White, Blue, Pink, Yellow, Green, Goldenrod**, then, if the production keeps going, **Buff, Salmon, Cherry**.

When the colors run out, the order starts again, with each name doubled: **Double White, Double Blue, Double Pink**, and so on. Productions vary it: some add Tan or Ivory after Cherry, and UK productions often swap Blue and Pink. White is the draft before any revision.

### 2.2 The document's list

Each document keeps its own list of color names, which the writer can edit (`revision.colors`, §5).

- **The default list** is the nine colors above, then the same nine with "Double " in front: 18 names.
- **Names** are free text, trimmed, 1 to 32 characters, with no line breaks. Two entries may not have the same name, ignoring case.
- **The next color.** `revision.start` without a color takes the entry after the newest set's color, or the second entry when there is no set yet, since the first names the draft before any revision. A color not in the list, or the last entry, is followed by the first entry. **Resolution:** the writer can always name any color, in or out of the list.

### 2.3 Paper and ink

Each color has a paper fill (colored PDF pages, §7.3) and an ink for each theme (the editor, §8). Names are matched ignoring case, after taking off a leading "Double ", "2nd " or "Triple ". A name that matches nothing gets white paper and the theme's text color, so it shows by its asterisks only.

| Color | Paper | Light ink | Dark ink | High-contrast ink |
| --- | --- | --- | --- | --- |
| White | `#FFFFFF` | (text) | (text) | (text) |
| Blue | `#CFE3FF` | `#1F5FBF` | `#7FB2FF` | `#66CCFF` |
| Pink | `#FFD6E7` | `#B8336A` | `#FF8FBF` | `#FF99CC` |
| Yellow | `#FFF6B3` | `#8A6D00` | `#F2D15C` | `#FFFF66` |
| Green | `#D4F2CF` | `#2E7D32` | `#7FD98A` | `#66FF66` |
| Goldenrod | `#F5DFA0` | `#9A6A00` | `#E6B84D` | `#FFCC33` |
| Buff | `#F2E3C6` | `#7A5C3A` | `#D9B98C` | `#FFD9A0` |
| Salmon | `#FFD1C2` | `#B5462A` | `#FF9C80` | `#FF9966` |
| Cherry | `#F7C1C8` | `#A3122E` | `#FF7A8F` | `#FF6680` |
| Tan | `#E8D9C0` | `#7D6440` | `#CCB088` | `#FFE0B0` |
| Ivory | `#FFFBEA` | (text) | (text) | (text) |

**Resolution:** the inks are our own choices, dark enough on white (light theme), light enough on the dark page, and bright on the high-contrast page. White and Ivory, whose paper is almost white, show only by their asterisks.

## 3. The revision table

The document holds one entry per set, in the order they were started:

```json
{"v": 1, "colors": ["White", "Blue", "Pink"], "active": "r_2", "sets": [
  {"id": "r_1", "color": "Blue", "label": "Blue Rev.", "date": "2026-10-22", "started": 1761130000000, "ended": 1761200000000},
  {"id": "r_2", "color": "Pink", "label": "Pink Rev.", "date": "2026-10-29", "started": 1761735000000}
]}
```

| Field | Meaning |
| --- | --- |
| `id` | `r_1`, `r_2`, …: numbered from 1 with no gaps, so the next set is always `r_<len+1>`. |
| `color` | The set's color name. |
| `label` | What the page header prints before the date (§7.2). By default the color followed by " Rev.", as in "Blue Rev.". Free text, 1 to 40 characters, no line breaks. |
| `date` | The revision's date, `YYYY-MM-DD`: a real calendar date from 1900 to 9999. It prints in the page header. |
| `started`, `ended` | When the set was started and ended, in milliseconds since the Unix epoch. `ended` is absent while the set is active. |

- `colors` is the editable list (§2.2), and `active` the active set, or absent when none is.
- **Never renumbered.** Sets are only ever added at the end, and `revision.clear` removes marks, never sets. The one way a set goes is undoing the `revision.start` that made it (and Save anyway to Fountain or .fdx, §9, which resets the table as an undoable step). By then every later step has been undone first, so no mark names it. Redo brings it back with the same id.
- **The default table** has no sets, the default colors and no active set. A document with the default table is written exactly as before phase 5b (§9).

## 4. Marks

### 4.1 In the text

A text run (`docs/specs/commands.md`, "Blocks in JSON") can carry a third mark, beside `ins` and `del`:

| Mark | JSON | Meaning |
| --- | --- | --- |
| `rev` | `"rev": "r_2"` | Set `r_2` was the newest set to change this text. |

- A run carries at most one `rev`: a later set's mark replaces an earlier one.
- Runs merge only when their style and all three marks are equal.
- Notes and boneyard never carry `rev`, and neither do the runs of note, boneyard, section and synopsis blocks: they never print.
- **In input**, a `rev` is ignored, as `ins` is (`docs/specs/changes.md` §2.1): marks are made only by the dispatcher, so runs read from `doc.get` can be sent back as they are.

### 4.2 On blocks

A block can carry a block-level mark, written as `"rev": "r_2"` in its JSON beside `change`. It records a change to the whole block rather than to some of its text:

- its element type changed;
- a printing attribute changed (§6.1);
- it was inserted empty, or emptied;
- it is part of a moved scene.

Every line of a block with a block mark is marked (§7.1). An empty block prints one blank line, which carries the mark. Like the text mark, the block's `rev` is read-only: commands that take a block's fields don't accept it.

## 5. Commands

Defined in `docs/specs/commands.md`:

| Command | Class | Params | Result |
| --- | --- | --- | --- |
| `revision.start` | edit | `color?`, `label?`, `date?` | `{set}`: the new set's entry |
| `revision.end` | edit | — | `{set}` |
| `revision.list` | query | — | `{active, colors, sets: [{…entry, blocks, pages}]}` |
| `revision.colors` | edit | `colors` | `{colors}` |
| `revision.clear` | edit | `set` or `all` | `{count}`: the blocks whose marks were removed |

- **`revision.start`** adds a set and makes it active.
  - It fails with `revision_active` while another set is active (`details.active` names it): ending a set is a decision of its own. The desktop app offers to end it first.
  - Without `color`, it takes the next color (§2.2); without `label`, "<color> Rev."; without `date`, today's date in UTC.
  - **Resolution:** the date defaults to UTC because a local date needs the system's time zone, which needs a date crate. The resolved values are journaled, so replay never reads the clock, and the desktop app shows the date in its form so the writer can change it.
- **`revision.end`** ends the active set and records `ended`. It fails with `revision_not_active` when none is.
- **`revision.list`** lists every set, oldest first. `blocks` counts the blocks holding its marks, and `pages` the body pages whose header names it (§7.2).
- **`revision.colors`** replaces the color list. It must hold 1 to 64 names, by §2.2's rules (`invalid_params` otherwise). Sets already made keep their colors.
- **`revision.clear`** removes the marks of one set (`set: "r_1"`) or of all sets (`all: true`), in text and on blocks. The sets stay in the table. It is for a production that starts its pages over, or for a writer who started a set by mistake. An unknown set fails with `revision_not_found`.
- **All five are writer-only**, except `revision.list`. An agent gets `writer_only`, enforced in core by author as the rules for `changes.accept` are. Agents' edits are marked like anyone's while a set is active (§6).
- **Each edit is one undo step** ("Start revision", "End revision", "Revision colors", "Clear revision marks"), journaled. Starting or ending a set changes no text. Undoing one gives back the table as it was.

## 6. What marks

### 6.1 Which edits

While a set is active, an edit that changes what prints marks it with the active set. This was decided with the project's owner (2026-10-22, proposal 2): **anything that prints** marks a line.

| Edit | What it marks |
| --- | --- |
| `text.insert` | The inserted text. |
| `text.delete` | The gap (§6.2). |
| `text.replace`, `block.set_text` | The new text, and the gap where text went without new text in its place. `block.set_text` compares old and new by words, as a tracked one does (`docs/specs/changes.md` §5.1), so only the words that changed are marked. |
| `text.style` | The restyled text. |
| `block.insert` | Its text, or a block mark when it is empty. |
| `block.delete` | The gap it leaves (§6.2). |
| `block.set_type` | A block mark. |
| `block.set_attrs` | A block mark, when `scene_number`, `dual`, `centered` or `space_before` changes. `depth` belongs to sections, which never print, so it marks nothing. |
| `block.split` | The last character before the split, or a block mark on the new block when it starts empty. |
| `block.join` | The last character before the joined text. |
| `scene.move` | A block mark on every block of the moved scene. |
| `changes.accept` | What the accepted change did, as the edit itself would have (§6.3). |

- **Never marked:** edits of notes, boneyard, sections and synopses, inside a line or as blocks; a tracked edit while it is pending (§6.3); `changes.reject`; the title page (decided with the project's owner, 2026-10-22: its edits are never tracked and never marked); the revision commands themselves.
- **An edit that changes nothing** that prints marks nothing, such as a style set to what it already is.
- **Inside a `batch`,** each edit marks as it would alone, and all of it is one undo step.

### 6.2 Gaps

Removed text leaves nothing to mark, so the gap marks what is next to it, and the line it falls on gets its asterisk:

- **Inside a block:** the character just before the gap, or, at the block's start, the character just after it.
- **A block emptied:** a block mark on it.
- **A block deleted:** the last character of the nearest printing block before it, or the first character of the nearest one after it when there is none before. A neighbour with no text gets a block mark instead.
- **Resolution:** the neighbour's own text is unchanged, so it is a marker for the line, not a claim that the character changed. Removing it later moves the asterisk to its own gap.

### 6.3 Tracked changes

- **A tracked edit marks nothing while it is pending.** It is a proposal, and the distributed draft does not have it yet. Its change records the set active when it was made: the change table's new `rev` field (`docs/specs/changes.md` §3, table `v` 2).
- **Accepting a change** marks what it did: its inserted text, the gaps of its deletions, and block marks for its retypes, moves, splits and joins, as §6.1 says for the edit itself. The set is the one active when the change is accepted, or, with none active, the set the change recorded. With neither, nothing is marked.
- **Resolution:** the set active at acceptance wins, because the accepted text first appears in that set's distributed pages. A change made during the Blue pass but accepted during the Pink one is a Pink revision. The recorded set covers a writer who ends the pass before reviewing its last changes.
- **Rejecting** marks nothing: the text goes back to what was distributed.

### 6.4 Text put back

Added 2026-10-26, from the project owner's manual test of phase 5b: deleting a space and typing it again left the line marked, though it read as it did before the set.

- **The active set marks against where it started.** The first time the active set marks a block, the block as it was then is kept in the revision table (`base`): its kind and printing attributes in the distributed draft, its text with the marks it had, and its block mark. (2026-10-28, journal semantics 4, review finding GR10) The printing attributes are the distributed draft's even under a pending retype, which keeps the ones it cleared in its oldest entry; the block they are compared with is read the same way. Before, the base kept the attributes as the retype had left them. Every later edit to that block, while the set stays active, is compared with that, not with the text just before the edit.
- **So text put back loses the set's mark.** Characters that read as they did when the set started take back the marks they had then, earlier sets' included. A block back to its kind and attributes, with its text as it was, takes back its block mark. Only what still differs from the set's start stays marked. A gap still marks its neighbour (§6.2) while the gap is there.
- **Undo** takes a block's starting point back with the edit that recorded it.
- **Ending or starting a set** clears the starting points. **Clearing** the active set's marks (or all marks) clears them too, so the set starts afresh from the text as it then stands.
- **Files:** the starting points are part of the revision table, so a project bundle (`revisions.json`) and the crash journal's header keep them, and a set still active after a reopen or a crash goes on marking against the same start.
- **An accept** that marks with an ended set (§6.3, no set active) marks against the text just before it, as before: the starting points belong to the active set only.

## 7. Printing

### 7.1 Asterisks

- **A marked line** is a printed line holding any cell with a `rev` mark, or any line of a block with a block mark.
- It prints `*` 0.4 in from the paper's right edge: at 8.1 in on US Letter, so it clears a right scene number at 7.6 in (`docs/specs/layout.md` §18).
- **Continued names and (MORE)** are added by layout, not the writer, and are never marked.
- **Dual dialogue:** a marked line in either column marks the row.
- **The page view** (`docs/specs/layout.md` §11) shows the asterisk in its cell, so the goldens hold it.

### 7.2 The page header

- A body page holding a marked line names the **newest** set among its marks, the one with the highest number, on its header line. The text is the set's label, a space, and its date as `MM/DD/YY`: "Blue Rev. 10/22/26". The date is formatted by our own code, never by the system's locale.
- It is right-aligned to end three cells before the page number's first cell. On a page whose number does not print, it ends at the page number's right edge (`page_numbers.right`).
- Title pages carry no revision header.

### 7.3 PDF export

`export.pdf` gains `revision_pages`:

| Value | Pages |
| --- | --- |
| `"white"` (the default, decided 2026-10-22, proposal 3) | Every page on white, with asterisks and revision headers. |
| `"colored"` | Each body page with a revision header is filled with the paper color of the set it names (§2.3); other pages stay white. |

- Revision marks never color the text in a PDF: the asterisks and headers carry them, as on paper. Tracked-change marks print as they did in phase 5a.
- The page fill is drawn first, under the watermark and the text. Bytes stay deterministic: the fills are fixed numbers.

## 8. The editor

- Marked text is drawn in its set's ink for the theme (§2.3). Where a cell also carries a tracked-change mark, the change's author color wins.
- Marked lines show `*` in the page's right margin, as they print.
- The hint bar shows the active set, "Blue Rev. 10/22/26", while one is active.
- The navigator marks a scene holding revision marks with a small asterisk.
- The commands are in the palette: "Start revision…" opens a form with the color (the next one chosen), label and date, which has the keyboard. "End revision", "Revision colors…" and "Clear revision marks…" ask first, as their effects are wide.
- Details are in `docs/specs/editor.md` §18.

## 9. Files

- **The bundle** (`docs/specs/bundle.md`) keeps the table in a new entry, `revisions.json`, and the marks in `document.json`. A bundle holding a table other than the default writes `meta.json` with `v: 2`, so a phase 5a build refuses it with `unsupported_format`. Without that, a 5a build would find fields it does not know in `document.json`, read the readable copy instead, and lose every mark on its next save. A bundle with the default table is written as `v: 1`, byte for byte as before.
- **The crash journal** (`docs/specs/journal.md`) writes `semantics: 3`. The header's `doc` holds `revisions` when it is not the default. The table is small (one entry per distributed draft), so it is copied, not named by fingerprint as the change table is.
- **Fountain and .fdx** cannot hold marks or sets. A save or export of a document with a table other than the default reports a `revision_marks` loss of class `lost`, counting the marked blocks and the sets. The desktop app asks about it as it asks about tracked changes.
  - **Save anyway** leaves the open script in the form the file holds, as it does for tracked changes (phase 5a decision 51). Every check that can refuse the save runs first, so a refused save changes nothing (2026-10-28). Then it sets the marked version aside as a Recovered version (`.marked-<ms>`), accepts any pending changes and resets the revision table and its marks to the default, together as one undoable step, and saves (`docs/specs/commands.md`, `doc.save`). A `revisions_cleared` warning says so.
  - Exports (`export.fdx`, `export.fountain`, "Save a copy as") don't change the script. Their question says the script keeps its marks.
- **An .fdx file's own `Revisions` list** (`docs/specs/fdx.md` §13) passes through untouched, as before. **Resolution:** no marks are written into .fdx. No public sample shows how revision-marked runs are stored (`docs/specs/fdx.md` §11), so the clean room gives us nothing to write them from.

## 10. Not in phase 5b

- Locked pages: A and B pages, locked page numbers, omitted pages.
- Locked scene numbers (12A, 12B).
- Revision marks on the title page (§6.1).
- Collating: printing only the revised pages of a set.

## 11. Testing

- **Property tests** (`crates/core/tests/revisions.rs`): random edits by random authors, with sets started and ended among them. Then:
  - every mark names a set that exists;
  - a block untouched while a set was active holds none of that set's marks;
  - undo of every step gives back the document with no marks;
  - nothing is marked after `revision.end`.
- **Fuzzing.** The `revisions` fuzz target runs random edits and revision commands on random documents, saves and reads back a bundle, and checks the model rules (`fuzz/fuzz_targets/revisions.rs`).
- **Gates.** Sets and marks survive a save and reopen of a bundle, a crash at any point (marking, `revision.start`, `revision.end`, saving), and undo and redo across a save (`docs/progress/phase-5b.md`).
