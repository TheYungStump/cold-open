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
| 7 | Layout and PDF: marked and accepted print forms, author colors, `export.pdf` `changes` | done |
| 8 | MCP tools and resource; the session file's `doc_ids`; agent tests through the real link and `coldopen mcp` | done |
| 9 | App: marks on the page, caret over them, keys and palette, the changes panel, the writer's name, the per-script agent switch, "Save as Cold Open project" | done: headless tests in `crates/app/tests/changes.rs` (8) and the agent tests; screenshots of the marks and the panel reviewed in three themes; 541 tests pass |
| 10 | Gates: crash gate (tracked edits, accept and reject, bundle saves), phase 4 journal fixtures, corpus round trip, reference gate, CI on three OSes, fuzzing, measurements | crash gate, old journals, corpus, reference gate and measurements done (below); the PR, CI on three OSes and the fuzz run next |
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
31. **A cell's `Look` carries the change's number, not its author's slot.** Layout memoizes measures by element, so authors are mapped to colors where cells are drawn: `PdfOptions::change_slots` for the PDF, the app for the screen. Slots follow the order authors first appear in the change table (`Document::change_slots`).
32. **The marked print form is a light copy**: only the title page and the blocks, with pending deletions set to print, so typing in a script with pending changes never copies its .fdx store. The blocks keep their order, so the view's indices still point into the document. The accepted form, used only by an accepted PDF, is laid out from the whole accepted document.
33. **Long runs (2026-10-18)**: the accept and reject property test at 100,000 cases, the `changes` and `bundle_read` checks at 20,000, and the crash property test into a bundle at 5,000 all pass.
34. **The panel says when a change was made as a relative time** ("5 minutes ago", "2 days ago"), as the recovery dialog does, not as a clock time such as "today 14:05". A local clock time needs the system's time zone, which means a date crate; a relative time needs nothing new. `editor.md` §17.3 now says so.
35. **The name question after Esc** is not asked again until the app restarts, so turning tracking off and on doesn't nag. When it is asked on opening a script, it sits on top of any recovery or repair dialog and its field has the keyboard from the first frame (`App::boot`).
36. **"Save as Cold Open project…" is offered for every lossy save of the script itself**, since a bundle loses nothing. Enter chooses it only when tracked changes are at stake; otherwise Enter still means Save anyway, as before. D and N still cancel. "Save a copy as" keeps its two choices, because the copy was asked for in that format.
37. **The panel's chosen row is outlined in the focus color, not filled.** On the blue selection fill, the dark and high-contrast author colors lost their contrast (seen in the screenshots). The chosen row also shows Accept and Reject buttons for the mouse, and a click on a row chooses it and moves the caret there.
38. **F7 and Shift+F7 go round the ends**, as find does. A change that spans several blocks is one stop.
39. **Counts.** The status bar says "N to review", and a click shows or hides the panel. The agent text adds "· N to review" for the agents' changes, and the agent box gains "Track agent edits" or "Stop tracking agent edits". The counts refresh when the navigator does, so while the writer types body text they catch up at the next tick (half a second), which keeps typing fast.
40. **The violet bar is for an agent's untracked edits only.** A tracked edit carries its own marks. There is a test for each case (`agent_link.rs`).
41. **Known limit, proposed as a follow-up.** At 1280 pixels wide, with both the navigator and the changes panel open, a page at 100% is wider than the space between them. Its right edge is cut off, because the page area doesn't scroll sideways. Zooming out or hiding the navigator avoids it. The fix is horizontal scrolling or a fit-width zoom, and it touches the page view's layout, which is outside this phase.
42. **The phase 4 fixture journals have a neutral `host` and `path`.** A phase 4 header records the computer's name and the script's full path. Here those held the development machine's name and a path under the user's home folder, which a public repo shouldn't carry. Both values were replaced in the fixtures; every entry is exactly as the old build wrote it. On any other computer such a journal counts as one from somewhere else, so the test opens it with `recover: "apply"`. To test the upgrade on the same computer, the test stamps the header with this computer's identity, taken from a real journal.
43. **Found while testing, not changed (from phase 4):** after `recover: "apply"` on a journal from somewhere else, the journal goes on under the old header. If the app then crashes again on this computer, reopening asks the same question once more. No work is lost, since Recover still applies everything, but the question is wrong the second time. A fix would restart the journal under this computer's header once the writer chooses Recover. That changes recovery, so it is proposed as a follow-up rather than done in this phase's gate step.

## Measurements (2026-10-18)

Release build on the development machine (Windows 11), median of 11 runs, with `cargo run --release -p coldopen-core --example bundle_times -- FILE` (new). It converts the script to a bundle, then makes tracked changes on every fourth scene. In each changed scene the writer inserts a block and adds to an action, and an agent deletes the start of another action. "Open" is `doc.open` plus the first layout. "Save" is an edit, then `doc.save` in place. The header is the one the journal starts again with after that save. For comparison, `open_times` on the .fdx file today gives 46.5 ms to open, 6.0 ms to save and a 490,638-byte header, as in phase 4.

| Big Fish | File | Pages | Pending changes | Open + first layout | Save in place | Journal header |
| --- | --- | --- | --- | --- | --- | --- |
| .fdx, as phase 4 measured it | 435 KB | 122 | — | 46.5 ms | 6.0 ms | 491 KB (490,638 bytes) |
| bundle converted from the .fdx | 156 KB | 122 | 0 | 32.0 ms (13.9 + 18.1) | 25.2 ms | 491 KB (490,693 bytes) |
| the same, tracked changes on 51 of 202 scenes | 159 KB | 124 | 141 | 30.6 ms (13.6 + 17.0) | 26.8 ms | 512 KB (512,498 bytes) |
| bundle converted from the .fountain | 131 KB | 125 | 0 | 28.8 ms | 35.6 ms | 264 KB (264,441 bytes) |
| the same, tracked changes on 48 scenes | 134 KB | 127 | 129 | 25.5 ms | 34.2 ms | 284 KB (284,492 bytes) |

- **Opening is faster as a bundle than as .fdx:** 31 ms against 46 ms, well inside the 200 ms budget, because reading the stored model skips the .fdx reader's work. Pending changes cost nothing measurable.
- **The bundle is about a third the size of the .fdx file**, because it is deflated. It still keeps the .fdx file's passthrough store.
- **Saving a bundle takes 25 to 35 ms**, against 6 ms for .fdx. The time goes to deflating about 600 KB of JSON and writing the readable `script.fountain`. Saves run on the save worker, so typing never waits on them.
- **The journal header grows by the marks only.** With 141 pending changes it gains 22 KB (491 KB to 512 KB), all of it marks in the document. The change table stays out of the header, named by fingerprint (decision 7). Saved as .fdx, where the header copies the table, the same document's header is 521 KB (520,688 bytes), so the reference saves 8 KB here. The saving grows with the table.

## Gates (step 10, 2026-10-18)

- **Crash gate** (`crates/app/tests/crash_gate.rs`): eight scenarios, run five times in a row, all pass. Two are new. One kills the app during tracked typing, accepting and rejecting (at the caret, by scene, all), undo and redo across a save, and saves of a Cold Open project every seven keys: 22 kill points, including both sides of every accept and reject. The other does the same on a Fountain script that is never saved, where the journal alone holds the change table. What comes back is compared block by block, ids, runs and marks included. A check on the check makes sure the compared states hold both inserted and deleted text. The driver gains `action NAME` and `modshift X` lines.
- **Old journals** (`crates/core/tests/old_journals.rs`): journals written by the phase 4 build (cddbb1d, built in a temporary worktree) on the Fountain fixture and on an .fdx copy of it are now fixtures, next to phases 1 to 3. All five recover in full, give the old build's blocks and Fountain text, and still undo. A phase 4 journal from this computer, upgraded, takes tracked edits of semantics 2, crashes again and recovers marks and all.
- **Corpus round trip** (`COLDOPEN_REQUIRE_CORPUS=1`): all 34 files write back byte for byte, with no repair needed.
- **Reference gate** (`COLDOPEN_REQUIRE_CORPUS=1`): passes on all three scripts. Big Fish: wraps 100.0%, resynced breaks 97.4% (largest gap 2 lines), pages 120/120, scene starts 99.5%. Brick & Steel and The Last Birthday Card: 100% on every measure. These are phase 4's numbers: layout is unchanged for scripts without marks. The extracted reference data (`*.ref.json`) was missing from this checkout's `corpus/`, so it was made again with `tools/refpdf` as the test says. Those are new files beside the PDFs; no file in `corpus/` was changed.

## Manual test (Windows CI build)

(Written in step 11. The steps use copies of corpus files in a scratch folder, never the files in `corpus/`.)
