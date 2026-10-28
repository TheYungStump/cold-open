# Phase 5b progress: production revisions, script diff, the title page editor and reports

Branch `phase-5b-revisions`, started 2026-10-22 from `main`. This file is updated and pushed with every step, so where the work stands never lives only in one session's notes.

Phase 5a brought the `.coldopen` bundle and track changes. 5b brings:

- production revision sets;
- script diff, and bringing another draft in as tracked changes;
- the title page editor;
- the scenes, characters and locations reports;
- recent files that no longer exist.

Locked pages (A and B pages) and locked scene numbers are not in 5b.

**Before the phase.** The brief asked for the weekly fuzz finding of 2026-10-19 (run 39260012138, on `cddbb1d`) to be checked against `main` first. It still failed, so it was fixed as its own PR, TheYungStump/cold-open#16. The 30-minute run on that fix found a second, older bug of the same kind, fixed in the same PR. Both are recorded in `docs/progress/phase-5a.md`, under Deferred, "After the phase".

## Final report (2026-10-26)

Phase 5b is done: production revision sets, script diff, bringing a draft in as tracked changes, the title page editor, the three reports, and recent files that no longer exist. It is squash-merged from TheYungStump/cold-open#17 once CI and the third fuzz run are green on the last commit. Every step of the plan below is done, and the project owner's manual test and retest passed. The phase changed about 120 files: about 16,800 lines in the crates, fuzz targets, fixtures and CI, and 740 lines of specs.

### What was built

- **Revision sets** (`revisions.md`, new).
  - **The table.** Each set has a color, a label, a date, and when it started and ended. The color list is editable per script and starts from the usual sequence; the first set is Blue, since White names the draft before any revision. One set is active at a time.
  - **Marks.** They sit on text runs and on whole blocks, and a gap marks its neighbour, so the line where text went gets its asterisk. One marking pass covers every edit, so undo takes marks back with the edit, tracked edits mark only once accepted, and rejects mark nothing. The active set compares each block with how it found it, so text put back loses its marks (from the manual test).
  - **Commands.** `revision.start`, `revision.end`, `revision.list`, `revision.colors` and `revision.clear`, writer-only and journaled.
  - **On the page.** An asterisk beside each revised line, and the set's header ("Blue Rev. 10/22/26") on each revised page. On screen, text shows in the set's ink in each theme. PDFs print white pages by default, with colored pages as an option (`revision_pages`, and a CLI flag).
  - **Files.** A bundle is version 2 only when it holds revision data, so a 5a build refuses it rather than losing the marks. Journals write `semantics: 3`. Fountain and .fdx report a `revision_marks` loss, and Save anyway clears the marks as one undoable step. An .fdx file's own `Revisions` list passes through untouched.
- **Script diff** (`diff.md`, new; the `coldopen-diff` crate, MIT OR Apache-2.0, screenplay types only).
  - **What prints is compared:** accepted text in Fountain normal form, with capitals, quotes, runs of spaces and CONT'D folded, so the Big Fish variants show only real differences.
  - **Matching.** Scenes match by content, and the longest run in order finds the moves. Blocks align, and the rest pair by similarity in order. Words compare by the word rule track changes uses.
  - **Output.** JSON, a readable summary, or a PDF with the changes marked, from the `diff` command, `coldopen diff a b`, an MCP tool, and Compare with… in the app.
- **Bringing a draft in** (`changes.import`, "Review a draft as changes…", and an MCP tool). The draft's differences become one change by a writer of the draft's name, run and journaled as one labelled batch, so replay never reads the draft again. It is refused while changes are pending. What tracking can't hold (styles, notes and boneyard, attributes including blank lines and centering, the title page) is left as it was and listed as skipped.
- **The title page editor.** `title.get`, `title.set` and `title.set_lines`: undoable, never tracked or revision-marked, and refused to an agent while its edits are tracked. The sheet (Ctrl+Shift+T) edits Fountain keys or .fdx lines beside the title page as it prints. An edited .fdx title line keeps what its paragraph held, and lines left alone are written as the file has them.
- **Reports** (`reports.md`, new). The scenes, characters and locations reports, as commands, CLI output (text, JSON, CSV), MCP tools and resources. The report view (Ctrl+Shift+I) takes the page area's place. Autocomplete, layout and the reports now share one parser for speakers and headings.
- **Recent files that are gone** are checked on a worker thread, shown dimmed with "Missing", and stay until the writer removes them.
- **No new dependencies.** CSV, the date arithmetic and the diff algorithms are written in the repo.

### Checks

- **Tests:** All 667 workspace tests pass with the corpus required, and `cargo fmt` and `cargo clippy` are clean. CI was green on Windows, macOS and Linux on f3c7306; the last commit's run is the one to merge on.
- **Crash gate.** A new scenario kills the app around revision sets: a set started from the form, marked typing with saves every seven keys, a set ended and another started, and an undo and a redo across a save. All 21 kills recovered every acknowledged mark and set.
- **Old journals.** A journal written by a phase 5a build, with tracked changes pending, is a fixture now. It recovers in full, goes on with revision marks, and recovers again after a second crash.
- **Corpus and reference.**
  - All 34 corpus .fdx files still write back byte for byte, Brick & Steel's `Revisions` included.
  - The reference gate gives phase 4's numbers: Big Fish's wraps 100.0%, resynced breaks 97.4%, pages 120/120, scene starts 99.5%.
- **Diff.**
  - It finds the moved and edited scenes of our fixtures.
  - The Big Fish differences that remain are real, pinned and explained (decision 44).
  - Budget: 15 ms in memory and 52 ms for `coldopen diff` end to end (release, the test machine), against 100 ms and 500 ms. Both are asserted on Linux in CI.
- **Reports** match hand counts of two fixtures and a golden of the synthetic script, in a fixed order.
- **Property tests.** They cover marking, reports, title pages through Fountain, and the diff (a draft against itself, swapped drafts, accepting an import gives B, rejecting gives A). The diff's ran at 10,000 cases by hand.
- **Fuzzing.** Twelve targets now, with `revisions` and `diff` new.
  - The first ten-minute run (40294830086) found two bugs (decision 55).
  - The second (40310454075) found one (decision 56).
  - A local run of the `diff` check over 5,000,000 pairs of drafts found ten more, all fixed (decision 56).
  - The third run, on those fixes, starts with this push.
- **Measurements** (CI, Linux). A save holds the UI thread for 1.29 ms at p95 (Fountain), 0.81 ms (.fdx) and 0.92 ms (a project with tracked changes), against 4 ms. Whole saves take 10.9, 2.9 and 7.9 ms at the median, against 50 ms.
- **Manual test** (a Windows CI build, by the project owner, 2026-10-26). It found four things: a restored space stayed marked, the title page beside the sheet, .fdx title lines on Tab, and the report view's scrolling. All four were fixed, and the retest passed.

### Deferred

- **The report view on CI's machines.** It scrolls in 16 ms a step on the test machine, but CI's windowed benchmark measures 50 ms a frame on Linux and 81 ms on Windows with the CPU renderer (14 ms on macOS), where the pages take 11 and 18 ms. The windowed numbers are reported, not asserted. Phase 6's gate asks for every budget on all three OSes, so it needs another look before then.
- **The report view as a popup.** The project owner preferred the popup, but the scrolling was only made fast by giving the view the page area.
- **Locked pages and locked scene numbers** (A and B pages) were never in 5b.
- **Marks in .fdx files.** As in 5a, no public source shows how to write them. An .fdx export asks first, then writes the text without revision marks or tracked changes, and a project keeps them.
- **What an import can't bring.** Styles, notes and boneyard, attributes (blank lines and centering among them) and the title page can't be tracked. They're listed as skipped for the writer to apply by hand.

### Decisions

- **The project owner's, before the phase:** title page edits are journaled and undoable on every front end, never tracked or revision-marked; an agent's title page edit is refused while its edits are tracked.
- **During the phase:** 56 decisions, listed below. The ones that shape later work:
  - one marking pass for every edit (16);
  - the active set marks against where it started (51);
  - bundles stay version 1 without revision data (9);
  - diff compares what prints (43);
  - an import is one labelled batch by a named writer (45);
  - a tracked retype that would drop text or styles is refused (55, 56);
  - the report view's rows are lines, for the CPU renderer (54).

## The plan, in order

| Step | What | State |
| --- | --- | --- |
| 1 | Branch, and this file | done: #16 squash-merged as 67e43d9 (2026-10-23), the branch rebased onto it |
| 2 | Specs before code: `revisions.md`, `diff.md` and `reports.md` (new); `changes.md`, `bundle.md`, `journal.md`, `commands.md`, `layout.md`, `editor.md`, `mcp.md`, `fdx.md` and `fountain.md`; the crate table in `CLAUDE.md` | done |
| 3 | The revision model: marks, the revision table, the commands, marking in the dispatcher and on accept, change table v2, `semantics` 3; property tests | done: `crates/core/tests/revisions.rs` (13 tests, a property test among them); `revision.list`'s `pages` comes with layout (step 5) |
| 4 | Files: bundle v2 and `revisions.json`, the journal header, the `revision_marks` loss and Save anyway, the `revisions` fuzz target | done: bundle unit tests, file tests in `revisions.rs`, and the `revisions` check and fuzz target (eleven targets now) |
| 5 | Layout and PDF: asterisks, revised-page headers, colored pages; page view and PDF goldens | done: `Layout::mark_revisions`; page view and colored-PDF tests in `revisions.rs`; existing goldens unchanged |
| 6 | The app's revisions: start form, palette, inks, asterisks, the active set in the hint bar | done: `crates/app/tests/revisions.rs` (6 tests: the form, inks in three themes, ending first, clearing, Save anyway, colored pages); screenshots reviewed in three themes |
| 7 | Shared names and the three reports: core, CLI (text, JSON, CSV), MCP tools and resources, the report view | done: `coldopen_screenplay::names` (3 unit tests); `crates/core/tests/reports.rs` (hand counts of two fixtures, a speech across a page break, the synthetic script's golden); `coldopen report`; MCP tools, and three resources the agent gate reads; `crates/app/tests/reports.rs`; screenshots reviewed in three themes |
| 8 | The title page editor: commands, op, the agent rule, the app sheet | done: `title.get`, `title.set`, `title.set_lines` and `Op::TitlePage`; `crates/core/tests/title.rs` (10 tests, a Fountain property test among them); edited .fdx title lines written line by line (`crates/fdx/tests/title_edit.rs`, and the `fdx_roundtrip` fuzz target now edits title lines); the app sheet (`crates/app/tests/title_page.rs`); screenshots reviewed in three themes |
| 9 | Script diff: the `coldopen-diff` crate, the `diff` command, CLI, MCP, PDF, Compare with…; the Big Fish variants; the budget; property tests and a fuzz target | done, but the Compare view's screenshots aren't reviewed yet: `crates/diff` (fixture goldens, property tests, `perf.rs`), `crates/core/tests/diff.rs`, `diff_corpus.rs` (Big Fish counts pinned), `coldopen diff`, the MCP tool, `crates/app/tests/compare.rs`, the `diff` fuzz target (twelve targets now). Budget: 14.9 ms in memory, 52 ms end to end (release, this machine) |
| 10 | Bringing a draft in as tracked changes (`changes.import`) | done with step 9: core, the MCP tool, "Review a draft as changes…", a property test (accept all gives B, reject all gives A) |
| 11 | Recent files that no longer exist | done: checked on a worker thread when the palette opens or the start screen appears; dimmed with "Missing" and kept; removed from the palette, the start screen or the question choosing one asks (`crates/app/tests/recent.rs`); screenshots reviewed |
| 12 | Gates: crash gate, corpus round trip, reference gate, save budgets, CI on three OSes, fuzzing, measurements | done: the crash gate's revision scenario (21 kills), the phase 5a journal fixture, the corpus and reference gates with the corpus required, CI on three OSes, three fuzz runs and a local check (decisions 49, 55, 56) |
| 13 | Manual test steps for a Windows CI build; the project owner's test; the final report | done: the test found four things, fixed and retested (2026-10-26); the final report is above |

## Decisions made with the project's owner before the phase (2026-10-22)

1. **The title page** is edited with journaled, undoable commands on every front end. Its edits are never tracked and never revision-marked.
2. **Agents and the title page.** An agent's title page edit fails with `writer_only` while agent tracking is on, enforced in core by author, so agents' edits stay reviewable. With agent tracking off, agents may edit it like the writer.

## Proposals in the approved plan (2026-10-22)

The project's owner approved the plan without choosing on these, so they stand as proposed:

1. **Bringing another draft in as tracked changes is in 5b**, as step 10, the last feature step, so it can be cut without touching anything else.
2. **Anything that prints marks a line:** text, style (bold, italic, underline), element type, and printing attributes (scene number, centered, dual). Notes and boneyard never mark.
3. **PDFs print white pages by default**, with asterisks and the revision named in each revised page's header. Colored pages are an export option and a CLI flag.
4. **Recent files that are gone show as missing**, dimmed with "Missing" and "Remove from recent". They're never dropped on their own, since a drive or a synced folder may come back.
5. **The diff budget.** Under 100 ms to compare two 120-page scripts in memory, and under 500 ms for `coldopen diff a b` end to end (release).
6. **No new dependencies.** CSV, the date arithmetic and the diff algorithms are written in the repo.

## Decisions made during the phase

1. **Specs first** (step 2, 2026-10-22). Three are new:
   - `revisions.md`: colors, the table, marks, what marks, printing and files;
   - `diff.md`: the comparison form, matching, the result, and bringing a draft in;
   - `reports.md`: speakers, headings and the three reports.

   `changes.md`, `bundle.md`, `journal.md`, `commands.md`, `layout.md`, `editor.md`, `mcp.md`, `fdx.md` and `fountain.md` gain the phase 5b parts, and `CLAUDE.md` gains the `crates/diff` row.
2. **The color list's public sources** are Wikipedia's "Shooting script" and Scriptation's article on revision colors, both read 2026-10-22. They agree on the order through Goldenrod, then vary. The default list is the brief's nine colors and a second "Double" round, and the writer can edit it.
3. **The first set takes the second color** (Blue). The list's first entry, White, names the draft before any revision.
4. **A gap marks its neighbour.** Deleted text leaves nothing to mark, so the character before it (or after it, at a block's start) carries the mark, and the line where text went gets its asterisk (`revisions.md` §6.2).
5. **Accepting a tracked change marks it with the set active at acceptance**, or the set it was made under when none is. The accepted text first appears in that set's distributed pages (`revisions.md` §6.3).
6. **`revision.start` refuses while a set is active** (`revision_active`). Ending a set is a decision of its own, and the app offers it first.
7. **The default date is today's UTC date**, resolved and journaled, so replay never reads the clock. A local date would need a time-zone crate. The app's form shows it, so the writer can change it.
8. **The asterisk column defaults to 8.1 in**, clear of a right scene number at 7.6 in. The revision header ends three cells before the page number.
9. **Bundles stay version 1 unless they hold revision data.** A 5a build reading a version 2 bundle as version 1 would fall back to `script.fountain` and lose the marks on its next save.
10. **Save anyway clears the revision data**, as it accepts tracked changes since 5a's decision 51. One `.marked` copy covers both.
11. **When either draft is Fountain, diff compares both as Fountain holds them** (`parse(write(s))`), so a Shot in an .fdx draft doesn't show as a retype against the same script in Fountain.
12. **Diff units start at every scene heading**, and sections belong to the unit they fall in, so a section moved with its scene doesn't split it.
13. **`changes.import` is writer-only and journals the edits it resolved**, so replay never rereads the draft and no client can claim a writer's name for edits it sends.
14. **The characters report sorts by words, then name.** Lines depend on page breaks, and speeches count a one-word reply like a monologue.
15. **`layout.md` §16's last two items** had ended up after §17 when 5a added it. They are back in §16.
16. **One marking pass for every edit** (step 3). Rather than marking in each command, core applies an edit's operations one at a time, recording each block they touch as it was. It then compares each touched block's distributed form (pending insertions left out, pending deletions kept) with what it was (`crates/core/src/revise.rs`):
    - what differs takes the active set's mark;
    - a gap marks its neighbour;
    - a change of kind, printing attribute or place marks the block.

    So tracked edits and rejects mark nothing by construction, an accept marks exactly what it brings into the distributed text, and every command, `batch` and `changes.import` included, marks by one rule.
17. **Changed words are marked whole.** The changed middle is widened to word boundaries before the word comparison, so a word that lost or gained a letter is marked whole, not from its first differing letter. Typed text in a new word marks only that word.
18. **Unchanged text keeps its marks, even with no set active.** An untracked `block.set_text` rebuilds the runs, so the pass carries each unchanged character's mark across. It runs whenever the document's revision table is not the default.
19. **An accept with no set active** marks with the newest set the accepted changes were made under.
20. **The table's version.** The change table is written `v: 2` only when an entry carries `rev`, so tables and journal fingerprints from before are unchanged. `semantics` is 3.
21. **Save anyway uses `revision.clear` with `sets: true`** (step 4), so taking the revision data out is journaled as an ordinary command, and replay never meets a step it doesn't know. Without `all`, `sets` is refused.
22. **A version 2 bundle's `document.json`** is written through a borrowing struct with every field of `Document` but the revision table. A version 1 bundle still goes through `Document`'s own serialization, so its bytes can't drift.
23. **Phase 5a's `changes_accepted` message** had a run of stray spaces in the middle, from a line continuation that lost its backslash. The message is now written once, in a helper the revision warning shares.
24. **The asterisk sits 0.4 in from the paper's right edge** (step 5), 8.1 in on Letter, not at a page-format field. A field would have changed every page format's JSON, and a phase 5a build reading a version 1 bundle of an .fdx script would then fail on it.
25. **Revision marks are a pass over finished pages**, not part of pagination (`Layout::mark_revisions`). Marks never move a line, so the incremental engine needs nothing new, and the pass runs again after any change. Dual dialogue gets one asterisk per row, and a right scene number in the way pushes it one cell past.
26. **The app paints revision marks for the pages in view only** (step 6). It calls layout's per-page `revision_lines` while drawing, so typing never copies the layout. A tracked change's author color wins over a revision's ink on the same cell.
27. **The hint bar's badge** ("Blue Rev. 10/22/26") opens the palette showing the revision commands, rather than ending the set on a click, since one stray click would end it.
28. **The color list is edited as one line**, names between commas, rather than one per line: a single field needs no multi-line editor, and Enter saves it as every other question does.
29. **`App::refresh`** (hidden from the docs) shows edits made straight through the dispatcher, as the app's own edits are shown. The screenshot example needed it; tests can use it too.
30. **One parser for names and headings** (step 7, `coldopen_screenplay::names`). Autocomplete's extensions and locations, and layout's check for a continued speaker, now use the parser the reports use, so a speaker or a location means the same thing everywhere. The layout goldens and the reference gate are unchanged, with the corpus required.
31. **The report view is read when it opens.** It covers the page, so nothing changes under it, and an agent's edit shows the next time it opens. Long cells are cut to 36 characters with "…"; Mod+C copies the whole table as CSV (`editor.md` §20).
32. **The reports are MCP resources as well as tools** (`coldopen://report/scenes`, `/characters` and `/locations`), so an agent can read them as it reads the script.
33. **A title-page value is one line of text with no space at either end** (step 8, 2026-10-25). Fountain trims values when it reads them, so `title.set` refuses what a Fountain file could not give back. A property test sets random title pages, markup characters and styles among them, saves them as Fountain and reads them back: 128 cases on every run, and 2,000 once by hand.
34. **Edited .fdx title lines keep what the file held** (`fdx.md` §13.5). Before, changing one title line generated the whole title page again, dropping tab stops, first-line indents, `Full` alignment and run attributes from every line; nothing could edit title lines then, so nothing reached it. Now lines left alone are written as the file has them, and an edited line carries what its paragraph held, as an edited script paragraph does (§14.4). Of the 34 corpus files, 16 have title pages; each is checked with eight sets of edits, and the `fdx_roundtrip` fuzz target edits title lines too.
35. **Only the first `TitlePage` is rewritten.** The reader reads only the first, but the writer rewrote every one from the document's lines, so a file with two title pages would have had its second replaced by a copy of the first.
36. **A title page the store keeps as text reports a `title_page_kept` loss** (a structural repair inside it, or something other than whitespace before its `Content`). Its edits used to be dropped on save without a word; now the save asks first.
37. **The view lays the document out again when its title lines change.** `View::refresh` compared only the script, and .fdx title lines are a layout option, so the page view kept showing the old lines. Found while building the sheet; `the_view_follows_title_lines` covers it.
38. **The sheet stands on the right, and the page area shows the title pages while it is open,** read-only, with the navigator stepped aside so the whole page shows beside the sheet. The editor did not draw title pages before.
39. **Mod+Shift+K removes a title line**, not Mod+Backspace as planned: a text field takes Backspace with every modifier, so the sheet would never see it.
40. **Tab selects the text of the field it reaches**, as form fields do, so typing replaces it. A line break inside an .fdx title line shows in its one-line field as ¶.
41. **Styles survive plain fields.** A changed value keeps the styles of its unchanged start and end, and new text takes the style before it, as typing does, so retyping a word of a bold underlined title keeps it bold and underlined.
42. **The screenshot example deletes old shots under either renderer's name.** iced_test writes a shot only when its file does not exist, and names it after the renderer, so on a machine with a GPU a second run kept the first run's images.
43. **Diff compares what prints** (step 9, `diff.md` §2 item 4, §3.1). The Big Fish variants differed in nearly every block. The causes were capitals an .fdx prints through its element settings, curly against straight quotes, double spaces, typed (CONT'D) markers, and Shots read as scene headings. So text compares as it prints, and only headings with a setting start a scene. A short speech's similarity counts words in common, since three-word runs were too coarse (§3.3).
44. **The Big Fish differences that remain are real**, pinned in `crates/core/tests/diff_corpus.rs`:
    - fountain.io's .fountain and jumpcut's .fdx agree on every scene; their title pages differ in the title's case and in Notes and Copyright, which the .fdx's positioned lines don't yield as keys.
    - fountain.io's .fdx numbers its scenes and the .fountain doesn't (every heading has other attributes). It has Shots where the .fountain has action, and a few speeches with literal asterisks, a colon, a line break and five lines that the other lacks.
    - screenplain's .fdx reads two headings as action, so two scenes join the ones before them. It has no title page, and the .fountain has a page break at the start.
    - jumpcut's scene-numbered file is a 42 KB excerpt of the full script.
45. **`changes.import` runs as one labelled `batch` by a writer of that name**, so the journal holds the edits and replay never reads the draft. The batch is one change. It is refused while changes are pending.
46. **The diff PDF and the Compare view are A with B brought in**, printed in the marked form, so they show exactly what an import would. The marks are one author's color, not the two colors §4.3 first planned.
47. **Removing a missing recent file has no key on the start screen** (step 11). Its rows are buttons, which the keyboard does not select, so the row has a "Remove from recent" button, and the palette is the keyboard's way: a missing file reads "Missing: <file>" there, with "Remove from recent: <file>" beside it. A file with a crash journal is not missing, since opening it recovers the journal.
48. **A phase 5a journal fixture** (step 12, `tests/fixtures/journal/phase5a.journal`). It was written by `coldopen serve --stdio` built from 67e43d9: tracked edits, an accept, a reject, two changes left pending, then untracked edits. It recovers in full with today's build. After the upgrade it goes on with revision marks (semantics 3), and survives a second crash with both. The header's host and path were scrubbed, as for phase 4.
49. **The crash gate kills around revision sets** (step 12): a set started from the app's form, typing marked with saves every seven keys, the set ended and a second started, and an undo and a redo across a save. 21 kills, at random and around each start, end and redo, all recovered the marks and sets the app had acknowledged. The sets' start and end times are the clock's, so the gate compares whether a set ended, not when.
50. **A property test assumed two random drafts always differ** (CI on d543160, minimum Rust). When they were equal, the import brought nothing in and the test's undo failed. The test now expects nothing to bring in for equal drafts; the code was right.
51. **The active set marks against where it started** (the manual test, `revisions.md` §6.4). Kept per block in the revision table (`base`), through an op of its own, so undo takes it back with the edit. Starting, ending or clearing a set resets it.
52. **The title page sheet doesn't shield the page.** Clicks on the page are ignored while the sheet is open, rather than closing it.
53. **The sheet learns which field was clicked by asking for focus** after every mouse press, since iced's text fields don't report a click.
54. **The report view's rows are lines, not cells**, for iced's CPU renderer (the manual test). A per-cell table repainted each cell's region separately: 244 ms a step on Big Fish. The `reports` windowed benchmark measures it, and CI reports it.
55. **Fuzzing, ten minutes a target** (run 40294830086, on 2d547d8): ten targets passed, and two found bugs.
    - **`diff`:** a draft of one stray line against a page break. Importing it retyped the line into a page break, and a tracked retype keeps the old type and attributes but not text, so rejecting it gave back an empty line. This was a phase 5a bug, now reachable through `changes.import`. A tracked retype that would drop text is now refused (`changes.md` §5). In the diff, a page break pairs only with a page break, and blocks with no words pair only when their text is equal.
    - **`fdx_roundtrip`:** a file whose first title page is `<TitlePage/>`, given title lines. The writer added a second `TitlePage`, which a reader never reads when the first comes before `Content`. The store now records that the first title page is kept as text, and such lines are a `title_page_kept` loss (`fdx.md` §13.5).

    Both inputs are regression tests, and the workflow ran again on the fix.
56. **The second run** (40310454075, on f3c7306): eleven targets passed, and `diff` found that importing a draft changed the style of words both drafts have, with "2" against a line with italics around a digit. A tracked `block.set_text` takes the new text's styles, and no change records them, so rejecting the import couldn't give them back. The import now keeps A's styles on the words both drafts have, and A's notes and boneyard where they stood, and lists what it leaves out as skipped (`restyled`, `notes`; `diff.md` §6).

    Rather than wait ten minutes a run for the next finding, a local check built pairs of drafts from Fountain tokens and ran each through the `diff` target's own check: headings, names, parentheticals, emphasis, escapes, notes, boneyard, synopses, title keys, centering, spaces and tabs. It found ten more. Each is fixed, and each input is a case in `local_check_findings` (`crates/core/tests/diff.rs`):
    - boneyard in a retexted line was dropped, and an unchanged note or boneyard moved to where the new text had it. `set_text` now keeps unchanged notes and boneyard where they stood (`changes.md` §5.1);
    - an inserted block lost the blank lines above it, so it read back as part of the block before. Inserts now carry them, and a section its depth;
    - a title key given twice paired both copies with the first, so a draft differed from itself. Keys now pair by occurrence (`diff.md` §3.5);
    - a note inside a run of spaces made two spaces in the comparison. Spaces now fold across runs (§2);
    - a retype into a synopsis cleared the blank lines above the block, and rejecting it didn't put them back. The retype mark now keeps them. A retype into a note or boneyard block, which drops styles, is refused like one that drops text (`changes.md` §5.2);
    - a name typed in lowercase prints in capitals as a character and as typed in action, so a retype alone left the text wrong. The import now retexts it too;
    - blocks in a gap paired across each other, which edits in place can't express. Pairs in a gap now keep their order (§3.3);
    - a changed block's other attributes (centering here) weren't listed as skipped. They are now;
    - blank lines above a kept block, and centering, are attributes, never tracked, so the import can't bring them in, and they change how text reads back. The check now leaves them out after accepting.

    After the fixes, 5,000,000 pairs of drafts passed the local check, on five seeds. The local check was a throwaway test, not committed. The fuzz workflow ran again on the fixes.

## Manual test (Windows CI build)

About 30 minutes. Every step works on **copies** of corpus files in a scratch folder, never on the files in `corpus/` (CLAUDE.md, Test data), and the build is unzipped outside the repo. Keys are the Windows ones.

**Set up**

1. On [TheYungStump/cold-open#17](https://github.com/TheYungStump/cold-open/pull/17), open the latest CI run, then Summary, then Artifacts. Download `cold-open-windows-x86_64` and unzip it to `C:\ColdOpenTest\app`. Windows may warn that the build is unsigned; TESTING.txt in the archive explains.
2. Make `C:\ColdOpenTest\scripts`. Copy (don't move) these three files into it from the repo's `corpus\fountain-io\` folder: `Big-Fish.fountain`, `Big-Fish.fdx` and `Brick-&-Steel.fdx`.

**Revision sets**

3. Start `Cold Open.exe` and open the scratch copy of `Big-Fish.fountain`. Press Ctrl+K and run "Start revision…". The form offers Blue, the label "Blue Rev." and today's date. Press Enter.
4. The hint bar shows "Blue Rev." with the date. Change a word in a speech: the new word shows in the set's ink, and an asterisk appears in the right margin beside that line. Delete a word elsewhere: that line gets an asterisk too.
5. Press Ctrl+Shift+E and export a PDF to the scratch folder. Open it: each revised page has "Blue Rev." and the date at the top, and asterisks beside the changed lines, on white paper. Export again with Ctrl+4 ("Colored pages") under another name: the revised pages are now blue.
6. Run "End revision", then "Start revision…" again: it offers Pink. Press Enter and change another line: it shows in the second set's ink.
7. Press Ctrl+S. A question says Fountain can't hold revision marks, with "Save as Cold Open project…" as the default. Press Enter and save `Big-Fish.coldopen` in the scratch folder.
8. Make one more change and don't save. End Cold Open from Task Manager (End task). Start it, open `Big-Fish.coldopen`, and choose Keep: the change is back, with every mark and both sets.
9. Run "Clear revision marks…", choose one set with Up and Down, and press Enter: its marks go. Ctrl+Z brings them back.

**The title page**

10. Still in the project, press Ctrl+Shift+T. A sheet opens on the right, and the page area shows the title page beside it. Type a date into Draft date and press Tab: the page shows it at once. Press Esc, then Ctrl+Z: the date goes, as one step.
11. Open the scratch copy of `Brick-&-Steel.fdx` and press Ctrl+Shift+T: this time each title line has a row. Tab to a line's text and change it, then press Ctrl+R: it moves to the right. Press Enter for a new line, type something, then Ctrl+Shift+K removes it again. Press Esc and Ctrl+S, close the script, and open it again: the changed line is still there.

**Reports**

12. Open `Big-Fish.coldopen` and press Ctrl+Shift+I. The Scenes tab lists every scene. Ctrl+Tab shows the characters, most words first, and the locations. Choose a row with Down and press Enter: the view closes and the caret is in that scene. Open it again, press Ctrl+C, and paste into Notepad or Excel: the table comes out as CSV.

**Comparing drafts**

13. Open the scratch copy of `Big-Fish.fountain` and save a copy as `Big-Fish-v2.fountain` (Ctrl+Shift+S). In the copy, click a scene in the navigator and press Alt+Down twice to move it, then change a line of dialogue and delete an action line. Save it.
14. Open `Big-Fish.fountain` again, press Ctrl+K and run "Compare with…", and choose `Big-Fish-v2.fountain`. A view lists the changed scenes, the move among them. Up and Down choose a row, and Enter scrolls the pages to it: the changed line is marked, and the deleted line struck through. Ctrl+Shift+E saves the marked draft as a PDF. Esc closes the view; the script itself is unchanged.
15. Run "Review a draft as changes…" and choose `Big-Fish-v2.fountain`. Type a name such as "Sam" and press Enter. The changes panel opens with Sam's changes; F7 steps through them. Run "Accept all changes": the script now reads as v2. Ctrl+Z takes it all back.

**Recent files that are gone**

16. Close the script (Ctrl+W). In Explorer, rename `Big-Fish-v2.fountain` to anything else. The start screen shows it dimmed, with "Missing" and "Remove from recent". Click its name: a question says it can't be found; press K to keep it. Rename it back and press Ctrl+K: the palette lists it as "Open recent" again.
17. Rename it once more, press Ctrl+K, type "remove", and choose "Remove from recent: Big-Fish-v2.fountain". It is gone from the list, and stays gone after Cold Open restarts.

**The command line**

18. In PowerShell, in `C:\ColdOpenTest`, run each of these. The first prints a summary of the two drafts, and the second the characters as CSV.
    - `.\app\coldopen.exe diff scripts\Big-Fish.fountain scripts\Big-Fish-v2.fountain`
    - `.\app\coldopen.exe report characters scripts\Big-Fish.fountain --format csv`

**Tell me**

- Anything that looked wrong or confusing, with a screenshot if you can.
- Whether the revision inks, the asterisks and the compare view's marks are easy to read in your theme (Ctrl+K, "Theme: …" switches themes).
- When you are done, you can delete `C:\ColdOpenTest`. It holds only copies.

## Manual test results (2026-10-26)

The project owner tested the Windows CI build of #17 and reported four things. Each is fixed:

1. **A space deleted and typed again stayed revision-marked.** Each edit was compared only with the text just before it, so the gap's mark on its neighbour stayed after the space came back. Now the active set compares against each block as it found it, kept in the revision table (`revisions.md` §6.4): text put back as it was takes back the marks it had. Tests: `text_put_back_as_the_set_found_it_is_not_marked`, `a_later_set_puts_back_the_earlier_sets_marks`, and the marking property test at 1,000 cases.
2. **The title page beside the sheet couldn't be scrolled, and a click on it closed the sheet.** The page area was shielded like the background of any dialog. Now the sheet leaves it to scroll, and a click on it does nothing (`the_title_page_beside_the_sheet_scrolls_and_ignores_clicks`).
3. **.fdx title lines didn't show on the page when Tab left them**, only after Enter or an alignment change, and clicking into a line didn't make it the current row. Tab now redraws the page after sending the line. After each click the sheet asks which field has the keyboard, so the clicked row's alignment is the one highlighted (`tab_shows_the_line_and_a_click_chooses_the_row`).
4. **The report view scrolled slowly.** It wasn't the test computer. A new windowed benchmark (`COLDOPEN_BENCH=reports`) measured 244 ms a step on Big Fish at 1920×1080. iced's CPU renderer repaints each changed region separately and fills a window-sized clip mask for each, and a table of separate cells made a region per cell. The report view now takes the page area's place, its rows are one line of text each in the page's monospaced font, and only the rows in sight are built. It scrolls in 16 ms a step, against 13 ms for the pages. Up and Down keep the selection in sight, and a Close button stands beside the title. CI's budgets job now reports the benchmark on all three OSes.

### Retest

With the Windows build from #17's latest CI run, and the scratch copies from the steps above:

1. Start a revision set, delete a space between two words, and type it back. The line has no asterisk and no colored text.
2. In a Fountain script, press Ctrl+Shift+T. Scroll the page beside the sheet down to the bottom of the title page, and click on it: the sheet stays open.
3. In `Brick-&-Steel.fdx`, press Ctrl+Shift+T, change a line, and press Tab: the page shows the change at once. Click into another line's text: its alignment button is the highlighted one.
4. In `Big-Fish.fountain`, press Ctrl+Shift+I and scroll the scenes with the wheel and with Up and Down: it should feel as smooth as scrolling the pages. Close it with the Close button.

**Result (2026-10-26).** All four passed. The project owner would have preferred the report view as a popup over the page, but takes the page area if that is what keeps scrolling fast (Deferred, in the final report).
