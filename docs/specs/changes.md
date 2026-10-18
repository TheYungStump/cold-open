# Track changes

- **Date:** 2026-10-18
- **Status:** phase 5a, being implemented in `crates/screenplay` (marks in text runs), `crates/core` (block marks, the change table, tracked edits, settling), `crates/layout` (marked pages) and `crates/app`.
- **Sources:**
  - `docs/spec.md`: "Revising", "Reliability features" item 6 (track changes), "Agents edit in the open", and the pain point it answers (added text colored, deleted text not tracked).
  - The review of phases 1–3 (2026-10-15), finding F18.2, as written up in `docs/progress/review-fixes.md` ("F18") and `docs/specs/journal.md` ("Decisions for phase 5").
  - The phase 5a brief and the decisions taken with the project's owner on 2026-10-18 (`docs/progress/phase-5a.md`).
  - `docs/specs/commands.md`, `docs/specs/journal.md`, `docs/specs/bundle.md`.
- No Final Draft material, and no other application's track-changes behaviour, was used. Everything here is our own design, written in our own words. A choice where the sources are silent is marked **Resolution**.

Track changes records what each author inserted and deleted, with the time, and lets the writer accept or reject each change. It is separate from production revision sets (phase 5b), which mark what changed between distributed drafts. An agent's edits are tracked by default, so the writer reviews them like notes from a collaborator.

## 1. Goals

1. **Nothing is lost while it is pending.** A tracked deletion keeps the deleted text and shows it struck through until someone settles it. A tracked insertion can be taken back in one step.
2. **One place.** Every front end gets the same tracked edits, because tracking is decided in the dispatcher, from the document's switches and the command's author. No front end marks text itself.
3. **Replay means what it meant.** Whether an edit was tracked is journaled with it, explicitly (§7), so a later change to a default never changes what an old journal replays to.
4. **The writer decides.** Only the writer accepts changes. An agent can withdraw its own and touch nobody else's (§6).
5. **Room for revision sets.** The change table is versioned, and each change can later belong to a revision set (§3), without moving marks.

## 2. Marks

A **change** is one author's proposal: some insertions, some deletions, and some changes to blocks. Its marks sit where the text and blocks are, and each names the change by its id, `c_<n>`.

### 2.1 In the text

A text run (`docs/specs/commands.md`, "Blocks in JSON") can carry two marks:

| Mark | JSON | Meaning |
| --- | --- | --- |
| `ins` | `"ins": "c_3"` | The run is a pending insertion by change `c_3`. |
| `del` | `"del": "c_4"` | The run is a pending deletion by change `c_4`. |

- A run may carry both: one author proposed deleting text that another author's change is proposing to insert.
- Notes and boneyard runs never carry marks (§10).
- Runs merge only when their style and both marks are equal, so a change's text stays a run of its own.

**The current text** is the printed text with every pending deletion left out. Pending deletions take no offsets, as notes and boneyard already don't (`docs/specs/commands.md`, "Positions in text"). So everything that works on a block's text works on its current text:

- `text` in a block's JSON;
- offsets in `text.insert`, `text.delete`, `text.style`, `block.split` and `text.complete`;
- `text.find` and `text.replace`;
- word counts, autocomplete and scene headings in `scene.list`.

An agent therefore never meets struck text unless it reads `runs`. **Resolution:** text inserted at an offset where a pending deletion sits goes before the deletion, as it goes before a note there, since the deleted text is not part of the current text.

### 2.2 On blocks

A block can carry one mark of each of these kinds (any number of `retyped` and `moved` entries), written as `"change"` in its JSON:

| Mark | JSON | What it records |
| --- | --- | --- |
| `inserted` | `{"inserted": "c_5"}` | The block is a pending insertion. |
| `deleted` | `{"deleted": "c_6"}` | The block is a pending deletion. It stays in the document, its text all marked `del`, until the change is settled. |
| `retyped` | `{"retyped": [{"change": "c_7", "from": "action", "scene_number": "4"}]}` | The block's type was changed from `from`, once for each entry, oldest first. The attributes a retype cleared (`scene_number`, `dual`, `centered`, `depth`) are kept in its entry, so rejecting it gives them back. |
| `split` | `{"split": "c_8"}` | The block was split off the end of the block before it. |
| `joined` | `{"joined": "c_9"}` | The block is to be joined to the block before it. It stays a block of its own until the change is accepted. |
| `moved` | `{"moved": [{"change": "c_10", "before": "b_40", "last": "b_33"}]}` | On a scene heading: the scene was moved, once for each entry. Before the move it stood before `b_40`, or at the end when `before` is null, and the move took the blocks up to `last`. |

A block's marks are written together, as in `{"change": {"inserted": "c_5", "retyped": {...}}}`, and the field is absent when the block has none.

**Resolution: pending structure stays visible.** A tracked delete or join does not remove the block; it marks it. Its id stays valid, nothing has to be rebuilt when the change is rejected, and the writer sees exactly what is proposed. A block marked `deleted` cannot be edited (`change_pending`, §5.4) until its change is settled.

## 3. The change table

The document holds one entry per change, in the order they were made:

```json
{"v": 1, "changes": [
  {"id": "c_1", "by": {"kind": "writer", "name": "Maya"}, "t": 1760356801123},
  {"id": "c_2", "by": {"kind": "agent", "name": "claude-code"}, "t": 1760356905410}
]}
```

| Field | Meaning |
| --- | --- |
| `id` | `c_1`, `c_2`, …: the table is numbered from 1 with no gaps, so the next change is always `c_<len+1>`. |
| `by` | The author: `kind` (`writer` or `agent`) and `name`. A writer with no name is shown as "Writer". The MCP tool name is not kept: a change can span several tools. |
| `t` | When the change was made, in milliseconds since the Unix epoch. |

- **Append-only.** An entry is never removed or rewritten while the document lives, in memory or in a bundle (`docs/specs/bundle.md`). Accepting, rejecting or undoing a change removes its marks, not its entry. A change whose marks are all gone is **settled**, and `changes.list` leaves it out. This keeps every undo and redo, including the steps a journal carries across a save, pointing at an entry that exists, and it lets the journal name the table by its length and fingerprint (`docs/specs/journal.md`, "The change data").
- **Room for phase 5b.** Each entry will be able to carry the revision set it belongs to. That comes with a new `v` for the table, so a 5a build never misreads a 5b table.

## 4. The tracking switches

The document has two switches, kept with it in a bundle and in the crash journal:

| Switch | Default | Tracks |
| --- | --- | --- |
| `tracking.writer` | off | The writer's edits. |
| `tracking.agents` | on | Every agent's edits. |

- `changes.track` changes them (§8). It is an edit: undoable and journaled. Only the writer may send it.
- **Resolution:** a Fountain or .fdx file has nowhere to keep the switches, so it opens with the defaults. Saving it is not a loss: the switches change nothing that prints, and the brief's rule is about the script's content.

## 5. Tracked edits

An edit is **tracked** when its `tracked` param is true (§7). This section says what each edit does then. An untracked edit works exactly as before, on the current text.

Here *the author* is the edit's author (`docs/specs/commands.md`, "Authors and undo steps"). Two authors are the same when they are both writers with the same name, or both agents with the same name.

### 5.1 Text

- **`text.insert`.** The new text is marked `ins` with the edit's change (§5.3).
- **`text.delete`.** For each character in the range:
  - a character the author's own pending change inserted is removed outright, since it was never in the accepted text;
  - any other character is marked `del` with the edit's change.

  Pending deletions already in the range stay as they are. Notes and boneyard stay, as they do in an untracked delete.
- **`text.replace`.** Each match is deleted as `text.delete` deletes, and the replacement is inserted right after the deleted text, marked `ins`. It takes the style of the first matched character, as before. **Resolution:** after, not before, so the old words read first and the new ones after them.
- **`block.set_text`.** The block's current text is compared with the new text by words, where a word is a run of letters, digits and apostrophes, and every other character is a word of its own. Then:
  - words in both keep their marks and take the new text's styles;
  - words only in the new text are marked `ins`;
  - words only in the old text are deleted as `text.delete` deletes, and placed where they stood, before any new words at the same place;
  - pending deletions already in the block stay where they were;
  - notes and boneyard are taken from the new text as given.

  The comparison finds the common start and end, then the longest common sequence of words in between. When the two texts between those have more than 1,000,000 pairs of words to compare, it stops there: the old middle is deleted and the new middle inserted, as one deletion and one insertion. The result depends only on the two texts.
- **`text.style` is not tracked** (§10). It restyles the current text, marked or not.

### 5.2 Blocks

- **`block.insert`.** The block is marked `inserted`, and its text `ins`.
- **`block.delete`.**
  - The author's own pending inserted block, whose only mark is that insertion and whose text is all the author's own pending insertion, is removed outright.
  - Any other block is marked `deleted`. Its text is deleted as `text.delete` deletes it: the author's own pending insertions go, and the rest is marked `del`.
- **`block.set_type`.**
  - The author's own pending inserted block simply changes type.
  - Otherwise the block is marked `retyped`, with the type it had and the attributes the change clears. Every retype is kept as its own entry, oldest first, so rejecting undoes them in turn, even when the block is changed back to its first type.
- **`block.split`.**
  - At the end of the text, with nothing moving to the new block, the new block is an insertion, marked `inserted`. This is what Enter at the end of a line sends.
  - Anywhere else, the new block is marked `split`, and the text it takes keeps its marks.
  - A split of the author's own pending inserted block makes another inserted block.
- **`block.join`.**
  - The author's own pending inserted block, whose only mark is that insertion, is joined outright, its text keeping its marks.
  - Any other block is marked `joined` and stays a block of its own. The result's `id` and `at` name the end of the block before it, where an editor puts the caret.
- **`scene.move`.** The scene heading is marked `moved`, with the block the scene stood before and the last block the move took. Every move is kept as its own entry, oldest first, so rejecting undoes them in turn.

### 5.3 Which change an edit records into

Each tracked edit records into one change, named in its result as `change` (§7). It is the first of these:

1. **The same undo step.** A keystroke that joins the typing step before it (`docs/specs/commands.md`, "Grouping") records into that step's change. Every edit inside one `batch` records into the batch's change.
2. **Next to the author's own pending mark.** A `text.insert` whose new text touches a run the author's own pending change inserted, in the same block, records into that change. So does a `text.delete` whose range touches a run the author's own pending change deleted. A word typed in two bursts is therefore one change.
3. **Otherwise a new change**, added to the table with the author and the edit's time.

One undo step creates at most one change, and an agent's tool call is one undo step, so each tool call is at most one change.

### 5.4 Edits that are refused

| Error | When |
| --- | --- |
| `change_pending` | Any edit of a block marked `deleted`, except settling it, or an untracked `block.delete` by the writer (which removes it outright). `details` give `id` and `change`. |
| `not_your_change` | An agent's edit, tracked or not, that would remove or change another author's mark, such as an untracked delete over the writer's pending insertion, or a retype of a block the writer has retyped. Tracked edits never remove another author's marks; they add their own beside them, so they are refused only where a block holds one mark of a kind (`retyped`, `moved`). |

Writers are never refused for whose marks an edit touches: the writer may edit anything.

## 6. Settling changes

`changes.accept` and `changes.reject` settle changes by `id`, by `scene`, or `all` (§8).

| Mark | Accept | Reject |
| --- | --- | --- |
| `ins` | The mark comes off. | The text is removed. |
| `del` | The text is removed. | The mark comes off. |
| `inserted` | The mark comes off. | The block is removed. |
| `deleted` | The block is removed. | The mark comes off (its text's `del` marks of that change come off too). |
| `retyped` | The mark comes off. | The block gets its type and kept attributes back. |
| `split` | The mark comes off. | The block is joined back to the block before it. |
| `joined` | The block is joined to the block before it. | The mark comes off. |
| `moved` | The mark comes off. | The scene moves back before `before`, or to the end when it is null. |

- **By scene** settles the marks inside that scene's blocks only. A change that also has marks elsewhere stays pending there.
- **Order.** Text marks are settled first; their order does not matter.
  - **Accepting** then removes deleted blocks, from the last to the first, and only then joins joined blocks, from the first to the last, each to the block kept before it, as the accepted form does (§9).
  - **Rejecting** then puts structure back change by change, newest first, as undo would: for each change, its moves are undone, its inserted blocks removed, its splits joined back and its retypes restored. A later change that reshaped a scene is therefore undone before an earlier move of that scene.
  - The same document and request always give the same result.
- **A move whose place is gone.** When the block a moved scene stood before no longer exists, or no longer starts a scene or section, the move cannot be undone. Rejecting it by `id` fails with `change_conflict` and changes nothing; by `scene` or `all`, the other changes settle and the move stays pending, listed in the result's `skipped`.
- **A join that is no longer possible** (2026-10-18). A join is checked when it is proposed, as an untracked join is, but the block before can change afterwards: it may become a note, which cannot hold styled text. Accepting such a join, or rejecting a split whose join back is no longer possible, keeps both blocks as they are. By `id` it fails with `change_conflict`; by `scene` or `all` the mark comes off and the change is listed in `skipped`. The accepted form (§9) keeps the blocks apart the same way. Found by the `changes` fuzz check.
- **Undo.** Settling is an edit: one undo step, journaled, and undo gives every mark back.

**Who may settle what** (decided with the project's owner, 2026-10-18):

| Author | Accept | Reject |
| --- | --- | --- |
| Writer | any change | any change |
| Agent | nothing (`writer_only`) | only its own changes. By `id`, another author's change fails with `not_your_change`. By `scene` or `all`, only its own are settled. A reject that would remove text or a block holding another author's work (another author's insertion inside the agent's inserted block, say) fails with `not_your_change`. |

These rules are enforced in core by author, as the rules for `allow_lossy` and `overwrite` are, so they hold over the app's agent link and inside `coldopen mcp` alike.

## 7. The `tracked` param, and replay

`block.insert`, `block.set_type`, `block.set_text`, `block.delete`, `block.split`, `block.join`, `text.insert`, `text.delete`, `text.replace` and `scene.move` take an optional `tracked` (a boolean).

- **Resolved before it runs.** When it is absent, it is filled in from the document's switches: `tracking.writer` for a writer, `tracking.agents` for an agent. The filled-in value is journaled with the command, inside a `batch` too, so every edit in a journal says whether it was tracked (`docs/specs/journal.md`, "Entries"). This is why tracking needed a new `semantics` number: it changes what these commands mean on replay.
- **Agents cannot opt out.** An agent's `tracked: false` while `tracking.agents` is on fails with `writer_only`. An agent may always ask for `tracked: true`. A writer may give either.
- **`text.style`, `block.set_attrs` and `changes.*`** take no `tracked`: they are never tracked.
- **The result** of a tracked edit names its change: `{"id": "b_9", "change": "c_4"}`. A batch names it at the top and in each edit's result. Replay compares it, as it compares ids (`docs/specs/journal.md`, "What replay compares").
- **The change's time** is the journal entry's `t` on replay, which the dispatcher writes from the same clock reading it gave the change.

## 8. Commands

Defined in `docs/specs/commands.md`:

| Command | Class | Params | Result |
| --- | --- | --- | --- |
| `changes.list` | query | `scene?` | `{changes: [...]}` |
| `changes.accept` | edit | `id`, `scene` or `all` | `{changes, count, skipped}` |
| `changes.reject` | edit | `id`, `scene` or `all` | `{changes, count, skipped}` |
| `changes.track` | edit | `writer?`, `agents?` | `{writer, agents}` |

Each `changes.list` row describes one pending change:

```json
{"id": "c_4", "by": {"kind": "agent", "name": "claude-code"}, "t": 1760356905410,
 "kinds": ["delete", "insert"], "blocks": ["b_12", "b_13"], "scene": "b_10",
 "inserted": "she runs", "deleted": "she walks slowly"}
```

- `kinds` lists, in this order, the kinds of marks it has: `insert`, `delete`, `block_insert`, `block_delete`, `retype`, `split`, `join`, `move`.
- `blocks` lists the blocks holding its marks, in document order; `scene` is the scene of the first one, or null before the first scene.
- `inserted` and `deleted` are its text, joined with a space across runs and cut to 200 characters with `…`.
- Rows come in document order of each change's first mark. With `scene`, only changes with a mark in that scene are listed.

## 9. Printing

A document with pending changes has two print forms:

- **Marked.** The blocks as they stand, pending structure included. Pending deletions print struck through, insertions in their author's color (`docs/specs/layout.md` §17). This is what the editor shows and what `export.pdf` prints by default (decided with the project's owner, 2026-10-18), so an agent's unreviewed text never prints as if it were accepted.
- **Accepted.** The document as accepting every change would leave it, with no marks. `export.pdf` prints it with `changes: "accepted"`.

**Writing other formats.** Fountain and .fdx cannot hold marks, so they are written from the accepted form, and a save or export reports a `tracked_changes` loss through the usual question (`docs/specs/commands.md`, "Losses"). A bundle keeps everything (`docs/specs/bundle.md`).

**Resolution: after "Save anyway".** A save that wrote the accepted form leaves the open script as it was, marks included, so the writer can still review and undo; but the file is now the record. Reopening the file, after a close or after a crash, gives the file's text, as for any other loss the writer accepted (`docs/specs/journal.md`, "Nothing to recover?": the file is the version the journal's header names). Rewriting the open document into its accepted form on such a save would change its blocks under the undo history, so it is not done.

**Resolution: no .fdx mapping.** No publicly shared .fdx file we have shows how marked runs are stored (`docs/specs/fdx.md` §11), so the clean room gives us nothing to write them from. If one turns up, mapping them becomes its own task.

## 10. Not tracked in phase 5a

- Styles (`text.style`) and block attributes (`block.set_attrs`): they change how text looks, not what it says.
- Notes and boneyard inside a line: they never print, and a run of them carries no marks. Note and boneyard blocks are tracked like any other block, so their text can carry marks.
- The title page, the page format and paragraph spacing: no command changes them yet.

## 11. Testing

- **Property tests** (`crates/core/tests/changes.rs`). Starting from a document with no marks, random tracked edits by random authors, then:
  - accepting all leaves each block's text equal to its current text before, with no marks;
  - rejecting all gives back the document before the tracked edits, apart from ids handed out;
  - undo gives every mark back.
- **Fuzzing.** The `changes` fuzz target runs random edits, settles and undos on random documents and checks the model rules and that settling never panics (`fuzz/fuzz_targets/changes.rs`).
- **Gates.** Tracked changes survive a save and reopen of a bundle, a crash at any point, and undo and redo across a save (`docs/progress/phase-5a.md`).
