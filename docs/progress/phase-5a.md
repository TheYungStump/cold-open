# Phase 5a progress: the project bundle and track changes

Branch `phase-5a-track-changes`, started 2026-10-18 from `main` (cddbb1d). This file is updated and pushed with every step, so where the work stands never lives only in one session's notes.

Phase 5 is split in two. **5a** (this phase) brings the `.coldopen` project bundle, track changes (F18.2), agent edits as tracked changes, and F21. **5b** brings production revision sets, script diff, the title page editor and reports. Nothing from 5b is built here, and the change data leaves room for revision sets.

**Before the phase:** `main` holds the phase 4 final report (TheYungStump/cold-open#5, 3aa6a60). It also holds the fixes for the two fuzz findings of run 39030169983 (#6, bf7c856: the 21-byte `fountain_roundtrip` input, a title value whose emphasis left an edge space, and the `fdx_roundtrip` input whose XML declaration held a forbidden character) and the emphasis-writer fix that followed (#7, cddbb1d).

## The plan, in order

| Step | What | State |
| --- | --- | --- |
| 1 | Branch, and this file | done |
| 2 | Specs before code: `bundle.md` and `changes.md` (new); `journal.md`, `commands.md`, `fountain.md`, `fdx.md`, `layout.md`, `editor.md`, `mcp.md`; the corpus-copies rule in `CLAUDE.md` | next |
| 3 | Model: marks in text runs, block marks, the change table, tracking switches, `doc_id`; new operations; header checks | |
| 4 | Semantics: the `tracked` param, `semantics` 2, change grouping, author rules, `changes.list`, `changes.accept`, `changes.reject`, `changes.track`; replay; property tests | |
| 5 | The bundle behind `Format`: rawzip and miniz_oxide, deterministic bytes, a tolerant reader, `doc_id`, the journal header's reference to the change data, recovery; fuzz targets | |
| 6 | Losses: `tracked_changes` on Fountain and .fdx; F21's `normalized` class | |
| 7 | Layout and PDF: marked and accepted print forms, author colors, `export.pdf` `changes` | |
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

(Numbered as they are made.)

## Measurements

(Step 10.)

## Manual test (Windows CI build)

(Written in step 11. The steps use copies of corpus files in a scratch folder, never the files in `corpus/`.)
