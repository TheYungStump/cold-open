# Script diff

- **Date:** 2026-10-22, updated 2026-10-25 (as built in step 9: §3.3, §4) and 2026-10-26 (fuzzing: §2, §3.3, §3.5, §6)
- **Status:** phase 5b, being implemented in `crates/diff` (package `coldopen-diff`, MIT OR Apache-2.0: the comparison) and `crates/core` (reading the drafts, the `diff` and `changes.import` commands, the marked PDF), with front ends in `crates/cli`, `crates/mcp` and `crates/app`.
- **Sources:**
  - `docs/spec.md`: "Revising" (script diff between any two drafts) and "Reliability features" item 7 (scene by scene, moved scenes detected; a readable summary, JSON, or a PDF with changes marked; `coldopen diff draft2.fdx draft3.fdx`).
  - The phase 5b brief and the decisions taken with the project's owner on 2026-10-22 (`docs/progress/phase-5b.md`).
  - `docs/specs/fountain.md` §10, the normal form and finding F21; `docs/specs/changes.md` §5.1, the word rule, and §9, the marked print form.
  - The longest common subsequence and the longest increasing subsequence, as found in any algorithms textbook.
- No Final Draft material, and no other application's comparison behaviour, was used. A choice where the sources are silent is marked **Resolution**.

A script diff compares two drafts, in any of the three formats, and says what changed. It works scene by scene, so a scene that moved is reported as a move, not as a deletion and an insertion. Within each scene it works block by block, then word by word.

## 1. Goals

1. **Only real differences.** Formatting nobody changed never shows: both drafts are compared in a normal form (§2), so a trailing space, a blank line or two dialogue blocks written as one are not changes.
2. **Moves are moves.** A scene that moved is found as the same scene in another place, even when it was edited too.
3. **The same answer everywhere.** The result depends only on the two drafts. It never depends on the OS, the locale or hash-map order, and every choice between equals is broken by position.
4. **Fast.** Comparing two 120-page scripts in memory takes under 100 ms (release), and `coldopen diff a b` under 500 ms end to end, file reading included (decided 2026-10-22).

## 2. What is compared

Each draft is read as `doc.open` reads it, without a session or a journal, then put in its **comparison form**:

1. **Accepted.** A bundle's pending tracked changes are taken as accepted (`docs/specs/changes.md` §9), since that is the text another reader would see. Revision marks are left out.
2. **As Fountain holds it, when either draft is Fountain.** A Shot becomes the scene heading Fountain writes it as, and General text becomes action, on both sides. Without this, a Fountain draft and an .fdx draft of the same script would differ in every Shot. **Resolution (2026-10-25):** these kind changes are made directly, not by writing and reading the script, so each element still knows the block it came from; the normal form of step 3 does the rest of what a write and a read would.
3. **In Fountain normal form** (`docs/specs/fountain.md` §10). This is finding F21's point: commands can leave text the normal form changes, and none of that should show as a change.
4. **As it prints** (2026-10-25). Text is compared, and blocks found equal or similar, on what prints, while the result gives each draft's text as it is and `changes.import` brings in B's text as it is:
   - **Capitals.** An element whose kind the draft's page format prints in capitals is compared in capitals. One .fdx of the corpus stores headings and names in any case (`int.`, mixed-case names) and prints them in capitals through its element settings.
   - **Quotes.** Curly quotes and apostrophes compare as straight ones. Converters change them, and they never change what a script says.
   - **Spaces.** A run of spaces, tabs or no-break spaces compares as one space. **Resolution (2026-10-26):** however runs split it: a style change or a note inside the run does not make it two (found by a local run of the `diff` check).
   - **CONT'D.** A CONT'D marker in a character's extensions (`(CONT'D)`, `(CONTD)`, any case) compares as absent. Layout prints the marker wherever a speaker continues (`docs/specs/layout.md`), so drafts differ only in whether they type it: in the corpus, one Big Fish .fountain types it a hundred times, and the .fdx of the same script never does.

   Why: the Big Fish variants (§7) differed in almost every block without these, and in nothing a reader would see.

**What is left out:**

- **Transparent elements** (sections, synopses, notes and boneyard, as blocks or inside a line) are compared only when neither draft is .fdx, since .fdx cannot hold them. When they are left out, the result says so (`compared.transparent: false`).
- **The page format, paragraph spacing and the .fdx store** are not script text, and are never compared. **Resolution:** the diff answers "what does the script say", not "how is it laid out".
- **The title page** is compared on its own (§3.5).

## 3. Matching

### 3.1 Units

Each draft's blocks are cut into **units**: one per scene heading, holding the heading and the blocks after it up to the next heading, plus an opening unit for the blocks before the first heading (when there are any). Sections and synopses, when compared, belong to the unit they fall in.

**Resolution (2026-10-25):** only a heading with a setting (`docs/specs/reports.md`'s prefixes: INT., EXT., EST., INT./EXT. and the rest) starts a unit. A heading without one, a forced heading or a Shot as Fountain writes it, is a block of the unit before it. Converters disagree on what such a line is (one Big Fish .fdx has Shots where its .fountain has action), and a unit split there turned one retyped line into scenes added and deleted.

**Resolution:** this differs from `scene.list`, where a section ends a scene, because a section moved with its scene should not split it in two.

### 3.2 Scenes

1. **Equal units.** Units whose blocks are equal (kind, text, styles and printing attributes) are paired first: the k-th unit of a given content in A with the k-th of the same content in B.
2. **Similar units.** For the units left, every pair is scored by the similarity of their words: the share of their three-word sequences that they have in common (Jaccard), or of their words when either has fewer than three. A pair scoring at least 0.5 is a candidate. Candidates are taken greedily: highest score first, then the smaller distance between their positions, then A's position, then B's. A unit is in at most one pair.
3. **Opening units** pair only with each other.
4. **In order, or moved.** Among the pairs, the longest run that is in increasing order in both drafts is **in order**. Ties are broken towards the pairs that come first in A. Any other pair is **moved**.
5. **Unpaired units** are **added** (only in B) or **deleted** (only in A).

### 3.3 Blocks

Within a pair of units:

1. Blocks are aligned with the longest common subsequence on (kind, text). Aligned blocks whose styles or printing attributes differ are **restyled** or have **changed attributes**.
2. **The gaps.** Between aligned blocks, the unaligned blocks of A and B are paired by similarity, at least 0.5, preferring the same kind. **Resolution (2026-10-25):** a block's similarity is the share of its words in common with the other's, in order: twice the length of their longest common run of words over the sum of their lengths (past the pair bound of step 3, the words they share in any order). §3.2's three-word sequences are too coarse for short blocks: one changed word in a four-word speech leaves only a third of its sequences in common, so the change showed as a speech deleted and another added. Scenes keep §3.2's measure, and pairs are still taken greedily as §3.2 says. A paired block whose text is equal and kind differs is **retyped**, and one whose text differs is **changed** (and **retyped** too when its kind differs). The rest are **added** or **deleted**. **Resolution (2026-10-26):** pairs in a gap keep their order: a pair that would cross one already taken is not made, since a block cannot move within a scene, and crossed pairs showed blocks in A's order as B's. A page break pairs only with a page break, and a note or boneyard block only with a note or boneyard block, since nothing else can become one without losing its text or styles (`docs/specs/changes.md` §5.2). Found by the `diff` fuzz target and a local run of its check.
3. **A bound.** When the two units have more than 1,000,000 pairs of blocks to compare, the alignment no longer takes the longest common subsequence of the middle. **Resolution (2026-10-28, review findings GR11, CL7; decided with the project's owner):** it anchors on the blocks whose kind and text occur exactly once in each draft's middle, keeps those in the longest run increasing in both, and aligns again between each two anchors, by these same rules, at most 32 levels deep. A gap with more than 250,000 pairs of blocks is not paired by similarity: its blocks are deleted and added. Before, the whole middle was one gap, paired with every pair scored: a 4,800-block unit took 14 s and about 740 MB, and a script written with forced headings is one unit (§3.1). Without anchors a long unit would show as deleted and added whole, and importing it would make thousands of tracked edits.

### 3.4 Words

A changed block's text is compared word by word, with the word rule of `docs/specs/changes.md` §5.1: a word is a run of letters, digits and apostrophes, and every other character is a word of its own. The comparison finds the common start and end, then the longest common sequence of words between, with the same 1,000,000-pair bound. The word splitter lives in `crates/screenplay`, so tracked changes and the diff share one rule.

### 3.5 The title page

- **Fountain title keys** are compared key by key, matched by name ignoring case. **Resolution (2026-10-26):** a key given more than once pairs by occurrence, the second with the second, so a draft with a repeated key is not different from itself. Each is **added**, **deleted** or **changed**, with its values.
- **Positioned title lines** (.fdx) are compared as a list of lines, with the block rule of §3.3, on their text, alignment and space above.
- A draft with keys compared with a draft with lines turns the lines into keys first, as a Fountain save does (`docs/specs/fountain.md` §11).

## 4. The result

### 4.1 JSON

```json
{"v": 1,
 "a": {"path": "draft2.fdx", "format": "fdx"},
 "b": {"path": "draft3.fountain", "format": "fountain"},
 "compared": {"transparent": false},
 "summary": {"scenes": {"same": 180, "changed": 14, "moved": 2, "added": 3, "deleted": 1},
             "blocks": {"changed": 31, "added": 40, "deleted": 22, "retyped": 2, "restyled": 1, "attributes": 0},
             "title_page": 1},
 "title_page": [{"op": "changed", "key": "Draft date", "a": ["October 1"], "b": ["October 22"]}],
 "scenes": [
   {"op": "moved", "a": {"index": 12, "id": "b_210", "heading": "INT. LAB - NIGHT"},
    "b": {"index": 15, "id": "b_260", "heading": "INT. LAB - NIGHT"}, "after": 14,
    "blocks": [{"op": "changed", "a": {"id": "b_214", "kind": "dialogue"}, "b": {"id": "b_265", "kind": "dialogue"},
                "words": [{"op": "same", "text": "I told you "}, {"op": "del", "text": "twice"}, {"op": "ins", "text": "already"}]}]}
 ]}
```

- **`scenes`** lists every unit that is not `same`, in B's order. A deleted unit goes after the B partner of the nearest in-order unit before it in A, or first when there is none.
- **`op`** is `changed`, `moved` (with `after`: the B index of the unit it now follows, or null at the start), `added` or `deleted`. A moved unit that was also edited is `moved`, with its `blocks`.
- **`index`** counts units from 1, the opening unit as 0. `heading` is the heading's text, empty for the opening unit. `id` is the id of the unit's first block in that draft, as its file numbers them: the heading's, or the opening's first block's.
- **`blocks`** lists each block that is not the same, in B's order, with `op`: `changed`, `added`, `deleted`, `retyped`, `restyled` or `attributes`. A block has `a`, `b` or both, each with `id` and `kind`. `words` is given for `changed`, as runs of equal ops with their text joined.
- **No differences** gives empty `scenes` and `title_page`, with every count but `same` at 0.

### 4.2 The readable summary

Plain text, one line per item, with no script text beyond headings and the first 60 characters of a changed block:

```
draft2.fdx → draft3.fountain: 14 scenes changed, 2 moved, 3 added, 1 deleted; the title page changed.
Title page: Draft date changed.
Scene 15 (was 12) INT. LAB - NIGHT: moved after scene 14; 1 speech changed.
Scene 20 EXT. ROOF - DAY: added.
…
```

The words "speech", "action", "heading" and so on name each block's kind. Counts come from `summary`.

### 4.3 The PDF

A PDF of B with the changes marked, laid out through the marked print form of `docs/specs/changes.md` §9:

- **Inserted** words and added blocks print in the first author color, as a tracked insertion does.
- **Deleted** words and blocks, A's text placed where it stood, print struck through in the second.
- **A moved unit** gets a line above its heading, in the first color: "Moved from after scene 11." (the A index of the unit it followed).
- **The title page** prints B's, unmarked, with the summary's title-page line after the last page.

The document printed is built only for the PDF. It is never saved or journaled, and its pages are not B's pages: struck text takes room.

**Resolution (2026-10-25):** the PDF is A with B brought in as §6 brings a draft in, printed in the marked print form. The changes are one author's, named after B's file, so inserted text prints in that author's color and deleted text struck through in the same color, rather than in two colors; and a moved scene prints as `docs/specs/changes.md` §9 prints a tracked move, without a "Moved from" line. One way of marking a draft serves both, so the PDF shows exactly what "Review a draft as changes…" would.

## 5. Where it runs

| Front end | How |
| --- | --- |
| Command | `diff` (class file): `a?`, `b`, `pdf?`, `overwrite?`. Without `a`, A is the open document, its current state, in its comparison form. Result: the JSON of §4.1 with `text`, the summary of §4.2, and `pdf` (the path written) when asked. |
| CLI | `coldopen diff A B` prints the summary; `--json` prints the JSON; `--pdf OUT.pdf` also writes the PDF. Exit code 0 whether or not the drafts differ, as `diff -q` is not the point. |
| MCP | The tool `diff`, with the same params. |
| App | **Compare with…** (palette, and the File menu's list) asks for a draft, then opens a read-only view: B's pages as the PDF marks them, with a list of the changed scenes beside them. Up and Down choose a row, Enter goes to it, and Esc closes the view. |

`pdf` follows `export.pdf`'s rules: it must end in `.pdf`, may not be either draft or the open document, and replaces a file only with `overwrite`, which only a writer may pass.

## 6. Bringing a draft in as tracked changes

Decided with the project's owner, 2026-10-22: in phase 5b. A co-writer's .fdx draft cannot hold tracked changes, so bringing it in this way is how a writer reviews it.

- **`changes.import`** (edit): `path`, `name?`. It compares the open document's current text with the draft at `path`, as `diff` does, and applies the differences to the open document as **tracked** edits by a writer named `name` (by default the draft's file name, without its extension):

  | Difference | Tracked edit |
  | --- | --- |
  | added block | `block.insert` |
  | deleted block | `block.delete` |
  | changed block | `block.set_text`, which marks word by word (`docs/specs/changes.md` §5.1) |
  | retyped block | `block.set_type` |
  | moved unit | `scene.move`, with `last` the unit's last block (2026-10-28): the unit moves whole, its sections and headings without a setting with it, where `scene.move` alone stops at either (review findings GR6, GP10, AR2) |

- **Not imported.** Restyled blocks, changed attributes and the title page cannot be tracked (`docs/specs/changes.md` §10), so they are left out and listed in the result's `skipped`, each with its block or key. The writer can apply them by hand.
  - **Resolution (2026-10-26):** found by the `diff` fuzz target and a local run of its check.
    - A retexted block keeps A's style on the words both drafts have, and A's notes and boneyard where they stood. A tracked `block.set_text` takes styles, notes and boneyard from the new text, and no change records them, so rejecting could not give them back. What differs is listed as `restyled` or `notes`.
    - A changed block whose attributes differ too is listed as `attributes`, as an unchanged one is.
    - An inserted block brings its blank lines above, and a section its depth. Blank lines on blocks A keeps are attributes, so they are not brought in: where they join or split paragraphs, the result can still differ from B there.
    - A retyped block whose text prints differently in its new kind (a name typed in lowercase prints in capitals only as a character) is retexted too.
- **One undo step**, labelled "Changes from <name>", and one change in the table, by that writer. Accepting all of it gives the open document B's text, apart from what was skipped. Rejecting all of it gives back the document as it was.
- **The journal** records the edits it resolved, not the path, so replay never reads the draft again. **Resolution (2026-10-25):** the edits are run as a `batch` with that label, by a writer of that name, and the batch is what the journal records; `changes.import` itself is never journaled, so no entry can carry edits of its own.
- **Pending changes.** A document with tracked changes waiting is refused with `change_pending` (2026-10-25): its current text and its accepted text differ, and edits planned on one would land wrongly on the other. Accept or reject them first.
- **Planning the edits.** They are planned on A's comparison form when each of its elements is one of its blocks, unchanged, as a script read from a file is; otherwise on A's blocks as they stand, so every edit names a real block, and the normalisation differences become edits too. (2026-10-28, review finding AR7) A Shot or General block taken as Fountain holds it (§2 step 2) still counts as unchanged: before, it made the form inexact, and the import retyped every such block though the diff showed nothing.
- **Writer-only.** An agent gets `writer_only`: the edits carry a writer's name, which an agent may not claim.
- **In three steps** (2026-10-28, review finding CL7). `changes.import` checks and copies what it needs from the session, plans the edits from that copy (which reads the draft and compares, and can run on any thread), then applies them as the batch above. The desktop app plans off the UI thread. Applying fails with `import_stale`, changing nothing, when the script changed after the copy was taken; the app plans again.
- **Result:** `{change, edits, skipped}`: the change's id, how many edits it holds, and what was skipped. With no differences, `change` is null, nothing changes and nothing is journaled.
- **The app:** "Review a draft as changes…" (palette) asks for the draft and the name to show, prefilled from the file name, then opens the changes panel.

## 7. Testing

- **Fixtures** (`crates/diff/tests/fixtures/`): short scenes we write ourselves, with a scene moved, a scene moved and edited, a scene added and one deleted, a speech changed by one word, a retype, and a title page change. The expected JSON is checked in.
- **Property tests:**
  - a script diffed with itself is empty;
  - A against B and B against A swap added and deleted, and report the same moves;
  - a script against itself written as .fdx by Cold Open is empty, apart from transparent elements, which are left out;
  - importing B into A, then accepting all, gives B's comparison form; rejecting all gives A.
  - (2026-10-28) after accepting all of an import, comparing the document with B again finds no scene moved, added or deleted, only what `skipped` explains; the drafts the property tests build hold sections, synopses and headings without a setting.
- **The Big Fish variants** in `corpus/`: fountain.io's .fountain and .fdx, jumpcut's .fdx and screenplain's .fdx, each pair compared, with every reported difference explained in `docs/progress/phase-5b.md`.
- **The budget** is measured on the synthetic 119-page script against an edited copy, and on the Big Fish variants, and asserted on Linux by the budgets job.
- **Fuzzing.** The `diff` fuzz target parses two inputs as Fountain, compares them and checks that nothing panics, that each diffed with itself is empty, and that the counts match the lists. It also brings B into A: the import must apply, accepting all of it must leave only what cannot be tracked (styles, attributes among them blank lines and centering, and the title page), and rejecting all of it must give A back. The property tests in `crates/core/tests/diff.rs` run the same check on random bytes and on drafts built from words.
