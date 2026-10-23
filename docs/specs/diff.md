# Script diff

- **Date:** 2026-10-22
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
2. **As Fountain holds it, when either draft is Fountain.** Both scripts go through the Fountain writer and reader (`parse(write(s))`). A Shot becomes the scene heading Fountain writes it as, and General text becomes action, on both sides. Without this, a Fountain draft and an .fdx draft of the same script would differ in every Shot.
3. **In Fountain normal form** (`docs/specs/fountain.md` §10). This is finding F21's point: commands can leave text the normal form changes, and none of that should show as a change.

**What is left out:**

- **Transparent elements** (sections, synopses, notes and boneyard, as blocks or inside a line) are compared only when neither draft is .fdx, since .fdx cannot hold them. When they are left out, the result says so (`compared.transparent: false`).
- **The page format, paragraph spacing and the .fdx store** are not script text, and are never compared. **Resolution:** the diff answers "what does the script say", not "how is it laid out".
- **The title page** is compared on its own (§3.5).

## 3. Matching

### 3.1 Units

Each draft's blocks are cut into **units**: one per scene heading, holding the heading and the blocks after it up to the next heading, plus an opening unit for the blocks before the first heading (when there are any). Sections and synopses, when compared, belong to the unit they fall in.

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
2. **The gaps.** Between aligned blocks, the unaligned blocks of A and B are paired by the similarity of §3.2, at least 0.5, preferring the same kind. A paired block whose text is equal and kind differs is **retyped**, and one whose text differs is **changed** (and **retyped** too when its kind differs). The rest are **added** or **deleted**.
3. **A bound.** When the two units have more than 1,000,000 pairs of blocks to compare, the alignment stops at their common start and end, and the middle is compared as one gap.

### 3.4 Words

A changed block's text is compared word by word, with the word rule of `docs/specs/changes.md` §5.1: a word is a run of letters, digits and apostrophes, and every other character is a word of its own. The comparison finds the common start and end, then the longest common sequence of words between, with the same 1,000,000-pair bound. The word splitter lives in `crates/screenplay`, so tracked changes and the diff share one rule.

### 3.5 The title page

- **Fountain title keys** are compared key by key, matched by name ignoring case. Each is **added**, **deleted** or **changed**, with its values.
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
- **`index`** counts units from 1, the opening unit as 0. `heading` is the heading's text, empty for the opening unit. `id` is the heading's block id in that draft, as its file numbers them.
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
  | moved unit | `scene.move` |

- **Not imported.** Restyled blocks, changed attributes and the title page cannot be tracked (`docs/specs/changes.md` §10), so they are left out and listed in the result's `skipped`, each with its block or key. The writer can apply them by hand.
- **One undo step**, labelled "Changes from <name>", and one change in the table, by that writer. Accepting all of it gives the open document B's text, apart from what was skipped. Rejecting all of it gives back the document as it was.
- **The journal** records the edits it resolved, not the path, so replay never reads the draft again: the entry's params gain `edits`, the list of tracked edits applied. A request that gives `edits` itself fails with `invalid_params`.
- **Writer-only.** An agent gets `writer_only`: the edits carry a writer's name, which an agent may not claim.
- **Result:** `{change, edits, skipped}`. With no differences, nothing changes and nothing is journaled.
- **The app:** "Review a draft as changes…" (palette) asks for the draft and the name to show, prefilled from the file name, then opens the changes panel.

## 7. Testing

- **Fixtures** (`crates/diff/tests/fixtures/`): short scenes we write ourselves, with a scene moved, a scene moved and edited, a scene added and one deleted, a speech changed by one word, a retype, and a title page change. The expected JSON is checked in.
- **Property tests:**
  - a script diffed with itself is empty;
  - A against B and B against A swap added and deleted, and report the same moves;
  - a script against itself written as .fdx by Cold Open is empty, apart from transparent elements, which are left out;
  - importing B into A, then accepting all, gives B's comparison form; rejecting all gives A.
- **The Big Fish variants** in `corpus/`: fountain.io's .fountain and .fdx, jumpcut's .fdx and screenplain's .fdx, each pair compared, with every reported difference explained in `docs/progress/phase-5b.md`.
- **The budget** is measured on the synthetic 119-page script against an edited copy, and on the Big Fish variants, and asserted on Linux by the budgets job.
- **Fuzzing.** The `diff` fuzz target parses two inputs as Fountain, compares them and checks that nothing panics, that each diffed with itself is empty, and that the counts match the lists.
