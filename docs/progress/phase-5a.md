# Phase 5a progress: the project bundle and track changes

Branch `phase-5a-track-changes`, started 2026-10-18 from `main` (cddbb1d). This file is updated and pushed with every step, so where the work stands never lives only in one session's notes.

Phase 5 is split in two. **5a** (this phase) brings the `.coldopen` project bundle, track changes (F18.2), agent edits as tracked changes, and F21. **5b** brings production revision sets, script diff, the title page editor and reports. Nothing from 5b is built here, and the change data leaves room for revision sets.

**Before the phase:** `main` holds the phase 4 final report (TheYungStump/cold-open#5, 3aa6a60). It also holds the fixes for the two fuzz findings of run 39030169983 (#6, bf7c856: the 21-byte `fountain_roundtrip` input, a title value whose emphasis left an edge space, and the `fdx_roundtrip` input whose XML declaration held a forbidden character) and the emphasis-writer fix that followed (#7, cddbb1d).

## Final report (2026-10-21)

Phase 5a is done: the `.coldopen` project bundle, track changes (F18.2), agents' edits as tracked changes, and F21. It is squash-merged from TheYungStump/cold-open#8 once CI is green on the last commit. Every step of the plan below is done, and the project owner's manual test and retest passed. The phase changed 109 files: about 11,500 lines in the crates, fuzz targets and fixtures, and 660 lines of specs.

### What was built

- **The change model** (`changes.md`, new).
  - **Marks in text runs.** A run carries `ins` and `del`, so insertions and deletions live in the text itself.
  - **Marks on blocks.** Inserted, deleted, retyped, split, joined and moved blocks stay visible, marked, until settled.
  - **An append-only change table** with authors and times, with room for 5b's revision sets.
  - **Pending deletions take no offsets**, so `text`, every offset, find and replace work on the current text. Agents never meet struck text unless they read the runs.
- **Commands.** `changes.list`, `changes.accept`, `changes.reject` and `changes.track`, all undoable and journaled.
  - Ten edit commands take `tracked`.
  - `text.insert` and `block.split` take `after_deleted` (from the manual test).
  - Journals write `semantics: 2`, and older journals replay unchanged.
- **Who may do what**, decided in core by author on every front end:
  - only the writer accepts;
  - an agent rejects only its own changes;
  - an agent's untracked edit can't touch another author's marks;
  - agents' edits are tracked by default.
- **The bundle** (`bundle.md`, new).
  - A deterministic zip, with no times, deflated at level 2. It holds `meta.json`, `document.json` with ids and marks, `changes.json`, and a readable `script.fountain`.
  - A tolerant reader that falls back to `script.fountain`.
  - A `doc_id` that follows the project, not its path.
  - The journal header names the change table by count and fingerprint instead of copying it.
- **Losses.** Pending changes are a `tracked_changes` loss in Fountain and .fdx. F21 is option (a): normalisations are a `normalized` loss class that never blocks a save.
- **Save anyway accepts.** Saving to Fountain or .fdx with pending changes keeps the marked version as a Recovered version, then accepts every change as one undoable step, so the script on screen and the file agree (decision 51).
- **Layout and PDF.** A marked and an accepted print form, and author colors on screen and in print. `export.pdf` prints marks by default; "Print the current text" prints the accepted form.
- **MCP.** The tools `changes_list`, `changes_accept`, `changes_reject` and `changes_track`, and the resource `coldopen://doc/changes`. `coldopen mcp --untracked` turns tracking off for people who use an agent without the app.
- **The app.**
  - **On the page:** marks in the author's color, deletions struck, and a bar in the margin.
  - **The caret** steps over struck text one side at a time.
  - **Keys:** accept and reject at the caret (Ctrl+Shift+Enter, Ctrl+Shift+Backspace), and F7 and Shift+F7 to move between changes.
  - **The changes panel** (Ctrl+Shift+R): rows, keys, a filter by author, and who is editing with Change my name….
  - **The writer's name** is asked for once.
  - **Switches** for my tracking and for agents' tracking.
  - **The lossy question** offers "Save as Cold Open project…".
- **Saving off the UI thread.** The UI thread only takes a snapshot; the worker renders and writes. Three save budgets replace "autosave under 10 ms" in `docs/spec.md` (decisions 44–47).

### Checks

- **Tests:** 556 pass on the last commit, and `cargo fmt` and `cargo clippy` are clean on Windows and for the Linux and macOS targets. CI is green on Windows, macOS and Linux (21 checks).
- **Crash gate:**
  - It kills the app during tracked typing, accepting and rejecting, undo and redo across a save, and saves of a project: 22 kill points, two new scenarios.
  - It passed five runs in a row after every change to the save path. The last runs were after the retest fix.
- **Typing gate:** the scene is typed into Fountain, .fdx and a project, with the caret checked after every key in all three.
- **Old journals:** journals written by the phase 4 build are fixtures now, and recover in full with undo.
- **Corpus and reference:**
  - All 34 corpus .fdx files still write back byte for byte.
  - The reference gate gives phase 4's numbers: Big Fish's wraps 100.0%, resynced breaks 97.4%, pages 120/120, scene starts 99.5%.
- **Fuzzing:**
  - Two new targets, `bundle_read` and `changes`, so the weekly workflow runs ten.
  - The run by hand on the branch (39182703613, ten minutes per target) passed.
  - The accept and reject property test passes at 100,000 cases.
- **Measurements:**
  - Big Fish as a project opens in about 31 ms, against 46 ms as .fdx and the 200 ms budget.
  - A save holds the UI thread for 1.3–1.5 ms and finishes in 7–25 ms, against the new 4 ms and 50 ms budgets. CI asserts the budgets on Linux.
  - With 141 pending changes the journal header grows by 22 KB.
- **Manual test** (Windows CI builds, by the project owner, 2026-10-21):
  - The first test found four things: the space in .fdx and project scripts, the caret over struck text, the name in the panel, and Save anyway (decisions 48–52).
  - The retest passed all four and found one leftover, the marks staying on screen after Save anyway (decision 53).
  - The table under "Manual test results" lists each with its tests.

### Deferred

- **Follow-ups after the merge**, each its own PR:
  - deleting a whole word also takes one adjacent space;
  - the palette's long paths and tall empty rows;
  - the notices list and "Dismiss all";
  - the page at 1280 pixels with both side panels open (decision 41);
  - the question asked again after recovering a journal from somewhere else (decision 43).
- **To 5b:** production revision sets (the change table has room for them), script diff, the title page editor and reports.
- **Not tracked in 5a:** styles (`text.style`) and block attributes (`block.set_attrs`).
- **Marks in .fdx files.** The clean room gives us no public source to write them from (`fdx.md`), so .fdx files hold the accepted text and a project is the way to keep changes. Phase 5's gate asks that tracked changes survive .fdx export. In 5a they don't survive it as marks: the export asks first, then writes them accepted, and a project keeps them.
- **The journal header's restart could move to the save worker**, saving about 0.5 ms; the budget doesn't need it (decision 47).
- **After the phase: the 2026-10-19 fuzz finding** (2026-10-22, before phase 5b).
  - **The finding.** The weekly run 39260012138, on `cddbb1d`, failed `fountain_roundtrip` with "parser output is not in normal form". Its 43-byte input still failed on `main` (23eae79).
  - **The input** minimises to `~\n~/**/\n~\n~`: a lyric of an empty line, a line holding only a boneyard, and two more empty lines.
  - **The cause.** A hidden-only line joined the empty lyric line before it. That line then held only hidden runs itself, so normalising again joined it once more, and each pass lost an empty line. Speeches lost hold lines the same way. So the normal form changed what prints, not only what the check sees.
  - **The fix** (`norm.rs`, `merge_hidden_only`, which the reader and the normal form share). Hidden-only lines join the nearest line with visible text, and never an empty line. In an element with no visible text, every hidden run goes to the first line. One pass is now final (`fountain.md` §5, 2026-10-22).
  - **Tests.** `hidden_only_lines_never_eat_empty_lines` (the inputs, and the line counts they must keep), and the property test `hidden_only_lines_normalise_once` (normalising is idempotent, writes back as itself, and keeps every printed line). Both failed before the fix.
  - **A second finding, from the first 30-minute run** (39876198784, after about eight minutes). It failed on `main` too: "unexpected losses: unexpressible", minimised to `*/**/*\r*/**/*`.
    - **The cause.** An action of two lines, each holding only a boneyard, was written with `!` on the first line. A lone `!` hands its hidden runs to the next line, so the two lines read back as one.
    - **The fix.** The writer now anchors that first line with an empty emphasis pair, as it already did every later one, whenever more lines follow (`write.rs`, `fountain.md` §9, 2026-10-22).
    - **Tests.** `a_first_action_line_of_hidden_runs_stays_a_line` fails without the fix. The property test now also makes actions and centered actions, and it failed without the fix too.
  - **The 30-minute run** of `fountain_roundtrip` on the fix's branch, with both fixes: run 39890293108 passed, 1,118,862 inputs in 1,801 seconds with no failure (coverage 4,496, corpus 5,373 inputs). Before it, the Fountain property tests passed at 100,000 cases, and all 568 workspace tests passed with the corpus required.

### Decisions

- **The project owner's, before the phase:**
  - the writer's name asked for once;
  - agents reject only their own changes and accept nothing;
  - PDFs print marks by default;
  - rawzip with miniz_oxide for the bundle.
- **During the phase:** 53 decisions, listed below. The ones that shape later work:
  - pending structure stays visible (2);
  - pending deletions take no offsets (3);
  - the append-only table (6);
  - the writer's name is part of the author (21);
  - who makes `doc_id` (25);
  - a cell's look carries the change number, not a color (31);
  - the save worker and the save budgets (45, 46);
  - the caret's two sides at struck text (49);
  - Save anyway accepts the changes (51).

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
| 10 | Gates: crash gate (tracked edits, accept and reject, bundle saves), phase 4 journal fixtures, corpus round trip, reference gate, CI on three OSes, fuzzing, measurements | done (below). PR TheYungStump/cold-open#8: CI green on three OSes (21 checks, on 78bdd08 after the save rework); the fuzz run 39182703613 (ten targets, ten minutes each) green |
| 11 | Manual test steps for a Windows CI build; the project owner's test; the final report | done: the project owner's test found four things (decisions 48–52); the retest passed and found one leftover (decision 53); final report above |

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
22. **After "Save anyway" to Fountain or .fdx, the open script keeps its marks**, but the file is the record: reopening it, after a close or a crash, gives the file's text, as for any loss the writer accepted. Rewriting the open document into its accepted form would change blocks under the undo history (`changes.md` §9). **Replaced on 2026-10-21 by decision 51**: the manual test showed the script on screen and the file disagreeing. Accepting as an undoable step, rather than rewriting the document under the history, avoids the problem this decision guarded against.
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
44. **Project saves deflate at level 2, and the readable copy uses the Fountain writer's quick path** (2026-10-19, at the project owner's request). Level 2 deflates about four times as fast as level 6, for a file 14% larger. `script.fountain` is never read back, so it skips the writer's read-back check; a test shows its bytes are a Fountain save's on every fixture. Big Fish's project save went from 26–34 ms to 15–17 ms.
45. **A save is rendered on the save worker** (2026-10-19, at the project owner's request). The UI thread takes a snapshot (a copy of the document, about 0.5 ms on Big Fish). The worker writes the file out, checks for losses and for another program's changes, and keeps a damaged original. The UI thread appends the marker, the worker writes the file, and the UI thread commits (`journal.md`, "Saving off the UI thread").
   - **Why the marker can come later.** Edits typed while the worker renders are journaled before the marker. That is safe because replay always starts from the header, and markers only name file versions; a new test crashes after such a save and gets every edit back.
   - **A save rendered after its script was closed is still written.** The router test caught this. It saves, quits and doesn't save the rest, and the file must still hold the save. Before, the bytes were ready before the close; now they come after it, so the marker step lets them through without a marker.
   - **A worker that dies** ends the save with `save_failed` through `RenderedSave::failed`, so the save token is never stuck.
46. **The save budgets in `docs/spec.md`** (2026-10-19, at the project owner's request). "Autosave under 10 ms" is replaced by three budgets:
   - the journal write per edit, under 1 ms (measured at about 0.01 ms; it is the autosave);
   - a save's time on the UI thread for a 120-page script, under 4 ms;
   - the whole save in the background, under 50 ms.
   The CI budgets job measures all three on every OS, for the synthetic 119-page script saved as Fountain, as .fdx and as a project with tracked changes. Linux asserts them, as it does the other headless budgets.
47. **The journal header's restart stays on the UI thread, and keeps its fsync.** In the app's mode the fsync was already done by the background flusher, as for every entry. So on the UI thread the restart costs only putting the document into the header as JSON and handing about 500 KB to the operating system: about 0.5–0.8 ms of the 1.3–1.5 ms a Big Fish save takes there. **Proposal:** it could move to the worker as well, which would save about 0.5 ms. The worker would make the header's JSON from the snapshot, and the commit would write those bytes only if nothing was edited since. The budget doesn't need it, so it is left as it is.

48. **A space typed into an .fdx script or a project now moves the caret** (2026-10-21, from the manual test). The cause: Fountain lays out its normal form, which trims a trailing space, and the caret's code added a cell for each trimmed space. An .fdx script or a project is laid out as its blocks stand, so the space reaches layout, and wrapping leaves trailing spaces off every line. The source map didn't know about those, so the caret stayed before the space until the next letter came. The caret's position now also counts spaces that layout left off the end of its line (`editor.md` §4). The typing gate now checks the caret after every key, in Fountain, .fdx and a project. Each typed character must leave it one cell right of where that character starts, or on a later line where the text wrapped, and it must show in the same place in all three formats. Without the fix, the .fdx and project runs fail on the first space.
49. **The caret has two places at struck text** (2026-10-21, from the manual test). Struck text takes no offsets, so one offset stood for both sides of it, and one Right jumped the struck text and the next character. The caret now records which side it is on (`past`). Right steps past the struck text, then over the next character; Left comes back the same way; Shift extends the selection one side at a time. Typing and Enter go on the side the caret is drawn. That needed `after_deleted` on `text.insert` and `block.split`: without it, text typed after struck text would jump in front of it. The param is journaled only when true, so older journals replay as before. Backspace that takes out the writer's own insertion leaves the caret where it stood; Backspace that strikes text leaves it before the struck text, as before, so repeated Backspace goes on leftward. A selection of struck text alone deletes nothing.
50. **The changes panel says who is editing** (2026-10-21, at the project owner's request): "Editing as: <name>" under its buttons, with **Change my name…** (also N, and "Change my name for changes…" in the palette). Enter on a cleared name removes the name, so changes go back to "Writer". Before, a cleared name was ignored and the old one kept. A cleared name is the same as never naming yourself: the question comes back once in a later session when tracking is on, as after Esc. Storing "Writer" as a name instead would have given the writer a second author color, because colors follow the author recorded in the change table.
51. **Save anyway accepts the pending changes** (2026-10-21, at the project owner's request, following phase 4 decision 21). The save that writes Fountain or .fdx with `allow_lossy` now proceeds in three steps (`commands.md`, `doc.save`):
    1. It copies the journal aside as `.marked-<ms>`, a new Recovered versions kind that replays to the version with the marks.
    2. It accepts every pending change as one undoable step.
    3. It saves.

    The script on screen and the file then agree, and the next save asks nothing. A `changes_accepted` warning gives the count and the copy. The app shows "Saving accepted 3 pending changes. The version with them marked is in Recovered versions." with the Recovered versions button. The question now says so before the save: "Saving accepts all 3 pending changes in the script. The version with them marked is kept in Recovered versions." This is done in core, not the app, so the CLI and JSON clients get the same result. Undo brings the marks back, unsaved. Exports (`export.fdx`, `export.fountain`, "Save a copy as") don't touch the script, so they accept nothing, and their question says the script keeps its changes.
52. **The acceptance happens before the save is written, not after.** The writers already write the accepted form, so the bytes are the same either way. Accepting first means the save itself finds nothing to lose, and the document is clean once it is written, with no extra edit after the save. The cost: if the save then fails (another program changed the file, a full disk), the changes stay accepted, and undo takes that back. The copy with the marks is kept in either case.

53. **The page shows the accepted script as soon as Save anyway accepts it** (2026-10-21, from the retest). The save in place takes its snapshot through `save_prepare`, which now accepts the changes first. Unlike every edit, that path never asked the app to refresh its view, so the page, the navigator and the panel showed the marks until the next key or click. The app now refreshes them when taking the snapshot changed the script. Save as already did. `save_anyway_shows_the_accepted_script_at_once` checks the drawn frame, the panel, the navigator, the status bar count and the laid-out page right after the save, before any other input; without the fix it fails on the frame.

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

## Saves (2026-10-19)

After decisions 44–46, release build on the development machine (Windows 11), 21 saves each in the app's durability mode. The figures come from `cargo run --release -p coldopen-app --example budgets -- --saves-only [--script FILE]`. "UI thread" is the snapshot, the marker and the commit. "Whole save" adds the worker's rendering and write. Each script is saved as Fountain, as .fdx written by Cold Open, and as a project with tracked changes on every fourth scene.

| Script, saved as | UI thread, median (p95) | Whole save, median (p95) |
| --- | --- | --- |
| Synthetic 119 pages, Fountain | 1.19 ms (1.35) | 20.3 ms (21.4) |
| Synthetic 119 pages, .fdx | 1.13 ms (1.22) | 8.2 ms (10.0) |
| Synthetic 119 pages, project | 1.10 ms (1.41) | 11.4 ms (11.8) |
| Big Fish, Fountain | 1.50 ms (1.84) | 24.5 ms (26.0) |
| Big Fish, .fdx | 1.31 ms (1.50) | 7.1 ms (10.5) |
| Big Fish, project | 1.40 ms (1.65) | 15.4 ms (16.0) |

- **Budgets** (`docs/spec.md`): under 4 ms on the UI thread, under 50 ms for the whole save. All six pass with room to spare.
- **Before:** a Big Fish save held the UI thread for 20–35 ms, the whole render included.
- **Projects with `bundle_times`** (one-step `doc.save`, level 2): Big Fish with 141 pending changes saves in 16.6 ms (26.8 ms at level 6), at 181 KB. The Fountain-born Big Fish saves in 14.5 ms (34.2 ms), at 152 KB. Opening and the journal header are as in the table above.
- **Crash gate:** five runs in a row, all eight scenarios pass. Kills now also land between the snapshot, the rendering, the marker and the write.

## Gates (step 10, 2026-10-18)

- **Crash gate** (`crates/app/tests/crash_gate.rs`): eight scenarios, run five times in a row, all pass. Two are new. One kills the app during tracked typing, accepting and rejecting (at the caret, by scene, all), undo and redo across a save, and saves of a Cold Open project every seven keys: 22 kill points, including both sides of every accept and reject. The other does the same on a Fountain script that is never saved, where the journal alone holds the change table. What comes back is compared block by block, ids, runs and marks included. A check on the check makes sure the compared states hold both inserted and deleted text. The driver gains `action NAME` and `modshift X` lines.
- **Old journals** (`crates/core/tests/old_journals.rs`): journals written by the phase 4 build (cddbb1d, built in a temporary worktree) on the Fountain fixture and on an .fdx copy of it are now fixtures, next to phases 1 to 3. All five recover in full, give the old build's blocks and Fountain text, and still undo. A phase 4 journal from this computer, upgraded, takes tracked edits of semantics 2, crashes again and recovers marks and all.
- **Corpus round trip** (`COLDOPEN_REQUIRE_CORPUS=1`): all 34 files write back byte for byte, with no repair needed.
- **After the manual test's fixes** (2026-10-21): the crash gate again five times in a row, all eight scenarios each time; the typing gate in Fountain, .fdx and a project, with the caret checked after every key.
- **Reference gate** (`COLDOPEN_REQUIRE_CORPUS=1`): passes on all three scripts. Big Fish: wraps 100.0%, resynced breaks 97.4% (largest gap 2 lines), pages 120/120, scene starts 99.5%. Brick & Steel and The Last Birthday Card: 100% on every measure. These are phase 4's numbers: layout is unchanged for scripts without marks. The extracted reference data (`*.ref.json`) was missing from this checkout's `corpus/`, so it was made again with `tools/refpdf` as the test says. Those are new files beside the PDFs; no file in `corpus/` was changed.

## Manual test results (2026-10-21)

The project owner ran the steps below on the Windows CI build from 78bdd08 and reported four things to fix in #8 before the final report. Nothing else was reported.

| # | Found | Fix | Tests |
| --- | --- | --- | --- |
| 1 | In .fdx and project scripts, pressing space didn't move the caret or show the space until the next character was typed | decision 48 | the typing gate, now in all three formats with the caret checked after every key |
| 2 | One Right from before struck text skipped the next character too; the same for Left and Shift-selection | decision 49 | `the_caret_steps_over_struck_text`, `shift_selection_steps_over_struck_text_one_side_at_a_time`, `typing_and_enter_go_on_the_side_of_the_struck_text_the_caret_is` (app); `text_and_splits_can_go_after_struck_text` (core); `text_can_go_after_the_struck_text_at_its_offset` (screenplay) |
| 3 | No way to see or change your name from the changes panel; clearing the name didn't go back to "Writer" | decision 50 | `the_panel_shows_and_changes_the_writers_name` |
| 4 | "Save anyway" to .fdx or Fountain left the changes pending on screen while the file held them accepted | decisions 51, 52 | `save_anyway_accepts_the_changes_and_keeps_the_marked_version` (app); `saving_pending_changes_to_fountain_asks_first`, extended to .fdx, a project, undo and a crash (core) |
| 5 | Retest (on the build from bde8fa7): all four fixes work. One leftover: after Save anyway to .fdx, the marks stayed on screen until the next click or key | decision 53 | `save_anyway_shows_the_accepted_script_at_once` |

**After the fixes** (2026-10-21): 555 tests pass; `cargo fmt` and `cargo clippy` (Windows, and the Linux and macOS targets) are clean; the crash gate passed five times in a row (eight scenarios each); the typing gate passes in all three formats. Screenshots of the panel with its new line were checked in three themes.

**Follow-ups after the merge**, each its own PR, as the project owner asked:

- Deleting a whole word also deletes one space next to it (tracked and untracked), so accepting doesn't leave a double space.
- The command palette shortens long paths with "…" in the middle (the file name first, the full path on hover), and its tall empty rows are fixed.
- Notices: "+N more" opens the full list, Dismiss closes one and shows the next, and "Dismiss all" is added.
- The page at 1280 pixels with both side panels open (decision 41).
- The question asked again after recovering a journal from somewhere else (decision 43).

### Retest (the four fixes)

About 5 minutes, on the new Windows build from the latest CI run of #8, with fresh copies in `C:\ColdOpenTest\scripts` as before.

1. **Space.** Open the copy of `Big-Fish.fdx`, click at the end of an action and type a few words: after each space the caret moves one cell right before the next letter is typed. Press Ctrl+K, run "Save as Cold Open project" and save it in the scratch folder, then type a few more words in the project: the same.
2. **Caret over struck text.** Turn on "Track my changes". Select a word in the middle of a line and press Backspace: it stays, struck through. Put the caret just before it and press Right: the caret lands just after the struck word, before the next letter. Right again steps over that letter. Left twice comes back the same way. Shift+Right from before the struck word selects only it; once more takes the next letter too. With the caret just after the struck word, type a letter: it appears after the struck word, not before it.
3. **Your name.** Open the changes panel (Ctrl+Shift+R). Its header says "Editing as: <your name>". Click **Change my name…**, type another name and press Enter: the header shows it. Open it again, clear the field and press Enter: the header says "Writer", and the next change you make says "Writer".
4. **Save anyway.** In the `.fdx` copy with a pending change or two, press Ctrl+S. The question says saving accepts the pending changes and that the marked version is kept in Recovered versions. Press S. The marks disappear from the page, nothing is left to review, and a notice says the changes were accepted, with a button to open Recovered versions. Open it: the newest copy says "With its tracked changes, before a save accepted them", and its preview shows the changed text. Back on the page, Ctrl+Z brings the marks back.

## Manual test (Windows CI build)

About 20 minutes. Every step works on **copies** of corpus files in a scratch folder, never on the files in `corpus/` (CLAUDE.md, Test data). Keys are the Windows ones.

**Set up**

1. On [TheYungStump/cold-open#8](https://github.com/TheYungStump/cold-open/pull/8), open the latest CI run, then Summary, then Artifacts. Download `cold-open-windows-x86_64` and unzip it, for example to `C:\ColdOpenTest\app`. Windows may warn that the build is unsigned; TESTING.txt in the archive explains.
2. Make a scratch folder such as `C:\ColdOpenTest\scripts`. Copy (don't move) these two files into it from the repo's `corpus\fountain-io\` folder: `Brick-&-Steel.fountain` and `Big-Fish.fdx`.

**Tracked changes in a Fountain script**

3. Start `Cold Open.exe` and open the scratch copy of `Brick-&-Steel.fountain`.
4. Press Ctrl+K and run "Track my changes (on or off)". A question asks what name your changes should carry, with "Writer" selected. Type your name and press Enter. The top bar now says "Tracking your changes".
5. Type a sentence at the end of an action: it shows in color, with a bar in the left margin. Select a few words of the script's own text and press Backspace: they stay on the page, struck through. Left and Right step over the struck words in one move. The status bar says how many changes wait for review.
6. Put the caret just after your sentence and press Ctrl+Shift+Enter: the change is accepted and loses its color. Press Ctrl+Z: it comes back. Press Ctrl+Shift+Backspace: it is rejected and the sentence goes. Press Ctrl+Z again.
7. Press F7 and Shift+F7: the caret jumps from change to change, and goes round from the last to the first.
8. Press Ctrl+Shift+R: the changes panel opens on the right, listing each change with your name, when you made it, what it does, its scene and a short excerpt. Press F6 until the panel is outlined. Up and Down choose a row, Enter takes the caret there, A accepts, R rejects, and Esc returns to the page.

**Saving as a project**

9. Make one more tracked change, then press Ctrl+S. A question says Fountain can't hold tracked changes, and "Save as Cold Open project…" is the default. Press Enter. The save dialog suggests `Brick-&-Steel.coldopen` in the scratch folder; save it. The window title now shows the `.coldopen` name, and the Fountain copy is untouched.
10. Close the script (Ctrl+W) and open the `.coldopen` file from the start screen's recent files. Every change is still there, with your name and color, and "Tracking your changes" is still on.

**A crash**

11. Make two or three more tracked edits and don't save. End Cold Open from Task Manager (End task).
12. Start Cold Open and open the `.coldopen` file. The recovery dialog appears; choose Keep. The edits are back, still marked as changes.

**Settling everything, and the PDF**

13. Press Ctrl+Shift+E. The export form has a "Print the current text" switch. Export once as it is (to the scratch folder). Then export again with Ctrl+3 (the switch on) under another name. Open both PDFs: the first shows the changes in color with deletions struck, the second shows clean text.
14. Press Ctrl+K and run "Accept all changes". A question asks to accept all N changes; press Enter. The marks are gone. Ctrl+Z brings them all back.

**An .fdx script**

15. Open the scratch copy of `Big-Fish.fdx`. It opens about as fast as before. Turn on "Track my changes" (no name question this time), change a line, and press Ctrl+S. The question offers the project, Save anyway and Cancel. Press S (Save anyway).
16. Close it and open the `.fdx` again. The line has your change in it, with no marks, because .fdx can't hold them. That is expected, and it is what the question said.

**Tell me**

- Anything that looked wrong or confusing, with a screenshot if you can.
- Whether the colors and the struck text are easy to read in your theme (Ctrl+K, "Theme: …" switches themes).
- When you are done, you can delete `C:\ColdOpenTest`. It holds only copies.
