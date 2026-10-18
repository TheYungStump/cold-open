# Phase 5a progress: the project bundle and track changes

Branch `phase-5a-track-changes`, started 2026-10-18 from `main` (cddbb1d). This file is updated and pushed with every step, so where the work stands never lives only in one session's notes.

Phase 5 is split in two. **5a** (this phase) brings the `.coldopen` project bundle, track changes (F18.2), agent edits as tracked changes, and F21. **5b** brings production revision sets, script diff, the title page editor and reports. Nothing from 5b is built here, and the change data leaves room for revision sets.

**Before the phase:** `main` holds the phase 4 final report (TheYungStump/cold-open#5, 3aa6a60). It also holds the fixes for the two fuzz findings of run 39030169983 (#6, bf7c856: the 21-byte `fountain_roundtrip` input, a title value whose emphasis left an edge space, and the `fdx_roundtrip` input whose XML declaration held a forbidden character) and the emphasis-writer fix that followed (#7, cddbb1d).

## The plan, in order

| Step | What | State |
| --- | --- | --- |
| 1 | Branch, and this file | done |
| 2 | Specs before code: `bundle.md` and `changes.md` (new); `journal.md`, `commands.md`, `fountain.md`, `fdx.md`, `layout.md`, `editor.md`, `mcp.md`; the corpus-copies rule in `CLAUDE.md` | done |
| 3 | Model: marks in text runs, block marks, the change table, tracking switches, `doc_id`; new operations; header checks | done |
| 4 | Semantics: the `tracked` param, `semantics` 2, change grouping, author rules, `changes.list`, `changes.accept`, `changes.reject`, `changes.track`; replay; property tests | done |
| 5 | The bundle behind `Format`: rawzip and miniz_oxide, deterministic bytes, a tolerant reader, `doc_id`, the journal header's reference to the change data, recovery; fuzz targets | done |
| 6 | Losses: `tracked_changes` on Fountain and .fdx; F21's `normalized` class | done, with step 4 (the agent tests needed it) |
| 7 | Layout and PDF: marked and accepted print forms, author colors, `export.pdf` `changes` | next |
| 8 | MCP tools and resource; the session file's `documents`; agent tests through the real link and `coldopen mcp` | |
| 9 | App: marks on the page, caret over them, keys and palette, the changes panel, the writer's name, the per-script agent switch, "Save as Cold Open project" | |
| 10 | Gates: crash gate (tracked edits, accept and reject, bundle saves), phase 4 journal fixtures, corpus round trip, reference gate, CI on three OSes, fuzzing, measurements | |
| 11 | Manual test steps for a Windows CI build; the project owner's test; the final report | |

## Decisions made with the project's owner before the phase (2026-10-18)

1. **The writer's name is asked for once.** Before the app's settings name the writer, the app asks for a name the first time it would record a tracked change of the writer's, prefilled with "Writer". It asks when the writer turns their own tracking on, or opens a script whose writer tracking is on, so the question never interrupts a keystroke. The CLI and `serve --stdio` take `--author NAME`, default "Writer".
2. **Agents may reject their own changes, and accept nothing.** Accepting is the writer's review. An agent can withdraw its own suggestion; it can never accept or reject anyone else's. Enforced in core by author, as F19 is.
3. **PDF export prints marks by default.** Insertions print in their author's color and deletions struck through, as the page view shows them, so an agent's unreviewed text never prints as if it were accepted. Printing the current text, with every change applied, is an option.
4. **The bundle uses rawzip with miniz_oxide.** rawzip 0.5.2 is MIT, pure Rust, has no unsafe code and no runtime dependencies, and brings its own CRC32 (MSRV 1.85; ours is 1.89). Deflate comes from miniz_oxide, already used for PDF. The alternatives were the `zip` crate (five required dependencies besides deflate) and a zip reader and writer of our own.

## Proposals in the approved plan (2026-10-18)

- **Element-level changes** stay visible as marked blocks until they are settled: an inserted, deleted, retyped, split, joined or moved block carries a mark that accepting or rejecting settles. Styles and block attributes are not tracked in 5a.
- **The changes list** is a side panel on the right, not a dialog, so the writer reviews while reading the page.
- **F21 takes option (a)**, a `normalized` loss class that never blocks a save. Option (b), commands that normalise as they apply, would eat the space a writer has just typed before the next word, and would make the edit commands mean different things in different formats.
- **The journal header names the change table** by the count and fingerprint of its entries. The table is append-only, so a crash between writing a bundle and restarting its journal still finds the header's table in the newer bundle.

## Decisions made during the phase

1. **Specs first** (step 2, 2026-10-18): `docs/specs/changes.md` and `docs/specs/bundle.md` are new. `journal.md`, `commands.md`, `fountain.md`, `fdx.md`, `layout.md`, `editor.md` and `mcp.md` gain the phase 5a parts, and `CLAUDE.md` the rule that manual tests use copies of corpus files.
2. **Pending structure stays visible.** A tracked block deletion or join marks the block instead of removing it, so ids stay valid and rejecting rebuilds nothing. A block that is a pending deletion cannot be edited until its change is settled (`change_pending`).
3. **Pending deletions take no offsets**, like notes. `text` and every offset count the current text, so agents and the find and replace commands never meet struck text. Text typed where a deletion sits goes before it.
4. **Marks are made only by tracked edits.** In input, a run marked `del` is left out and an `ins` is ignored, so an agent can send back the runs `doc.get` gave it and they stand for the current text.
5. **A tracked replacement goes after the deleted text**, so the old words read first.
6. **The change table is append-only**, numbered `c_1`… with no gaps. Settling or undoing a change removes its marks, not its entry. This is what lets the journal name a bundle's table by count and fingerprint.
7. **No `doc.saved` markers for synchronous saves.** The plan added them so a crash between a bundle save and the journal's restart could find the old table; the append-only table makes that unnecessary, because the newer bundle starts with the old table.
8. **The session file mirrors `doc_id` as `doc_ids`**, a list in the same order as `docs`, rather than a list of objects, so readers from earlier phases keep reading `docs` as before.
9. **The tracking switches are not a loss** when a script is saved as Fountain or .fdx: they change nothing that prints. Those files open with the defaults (the writer's off, the agents' on).
10. **Writers with different names are different authors for tracked changes** (a change joins only its own author's marks) and the same author for undo grouping and permissions.
11. **A bundle is laid out as its blocks stand**, as .fdx is. A script read from Fountain is already in normal form, so its pages do not change when it becomes a bundle.
12. **The lossy-save question defaults to Save as Cold Open project** when tracked changes are among the losses.
13. **Marks are fields of text runs** (`Run::Text { text, style, marks }` in `crates/screenplay`), with a third, print-only flag, `show`, that the marked print form sets so a deletion counts as printed text for the Fountain normaliser, layout and the source map. It is never stored or read from JSON, and a document holding it fails its checks.
14. **Note and boneyard blocks may carry marks.** Their rule was "plain text only"; it is now "no styles and no hidden runs", so a tracked edit of a note block is tracked like any other (`changes.md` §10).
15. **A change is made only when a mark needs it.** A tracked delete that only takes back the author's own insertion leaves no mark, so it makes no change and its result names none; the table entry it would have added is never written.
16. **Every move and every retype is kept as its own entry** (`retyped` and `moved` are lists), and a move records the last block it took. Found by the property test at 20,000 cases: a scene moved twice, or a retype that changed a scene's extent between two moves, could not be put back. Rejecting now undoes structure change by change, newest first, as undo would; accepting removes deleted blocks before it joins joined ones.
17. **A block is deleted or joined outright only when its only mark is the author's own insertion.** Joining an own split block outright, after its scene had moved, lost its marks (property test).
18. **A tracked replacement leaves what sits inside the match in place.** An untracked replace moves notes to the end of a match; a tracked one keeps the old text struck where it was, so moving a pending deletion past it scrambled the text on reject (property test).
19. **Agent tracking is decided in core, by author, on every front end.** With it on by default, an agent alone on a .fountain or .fdx file cannot save its tracked edits there (a lossy save is the writer's call). The three phase 3 and 4 agent-gate tests that were about saving, not tracking, now run `coldopen mcp --untracked`, or have the writer turn agent tracking off first.
20. **`coldopen mcp --untracked`**: for people who use an agent on a Fountain file without the app. It is set in the MCP client's configuration by the person, never by the agent, and overrides the switch only in scripts the server opens itself. It is a dispatcher setting, not document state; every edit journals the `tracked` it took (`mcp.md` §6.1).
21. **The writer's name is part of the author**: `Author::Writer { name }`. The journal writes `by` for a named writer; the plain writer is still left out, so old journals are unchanged. `coldopen run` and `serve --stdio` take `--author NAME`.
22. **After "Save anyway" to Fountain or .fdx, the open script keeps its marks**, but the file is the record: reopening it, after a close or a crash, gives the file's text, as for any loss the writer accepted. Rewriting the open document into its accepted form would change blocks under the undo history (`changes.md` §9).
23. **F21's normalisations are a separate list from the Fountain crate** (`Written::normalized`), so the crate's `losses` keep their meaning for its own property tests and fuzz checks; core gives them the class `normalized`. Each names its block and the case. The recovery check counts only `lost` losses, exactly as it counted none of these before.
24. **The bundle records no times.** rawzip writes a timestamp extra field whenever an entry is given a time, so entries are given none: their date and time fields are zero and there are no extra fields (`bundle.md` §2, corrected from "1980-01-01").
25. **Who makes `doc_id`.** The dispatcher, never the writer of the bytes, so writing stays deterministic: `doc.new` at a `.coldopen` path, and a save that first writes a bundle at a path (Save as, a conversion), make a new one; a save in place keeps it; a conversion to Fountain or .fdx drops it (`format::adopt`). A bundle whose `meta.json` is unreadable gets a new one when read.
26. **Recovery of a bundle's journal** reads the bundle on disk for the table its header names (`ReadBack::use_table`). Until then the header's table stands as that many entries of an unknown author, so replay hands out the same change ids. A missing table (`change_data_missing`), another `doc_id`, or an unreadable bundle all count as a change by another program.
27. **`doc.export` refuses `coldopen`**: a bundle is a zip, not text; `json` gives its blocks.
28. **A join that is no longer possible keeps both blocks** (found by the `changes` fuzz check): a join is checked when it is proposed, but the block before can become a note afterwards. Accepting it, or rejecting a split whose join back is no longer possible, takes the mark off, keeps the blocks, and lists the change in `skipped` (`change_conflict` by `id`). Joins run from the first block to the last, as the accepted form gathers them (the fuzz check found the two disagreeing on a run of joined blocks).
29. **`Dispatcher::journal_bytes`**, hidden from the docs: the journal is locked while a session is open, and on Windows a locked file cannot be read from outside, so tests read it through the lock.
30. **The fuzz targets** `bundle_read` and `changes` join the weekly workflow (ten targets). Their checks also run on every build as property tests, with real bundles cut short or with a byte changed.


## Measurements

(Step 10.)

## Manual test (Windows CI build)

(Written in step 11. The steps use copies of corpus files in a scratch folder, never the files in `corpus/`.)
