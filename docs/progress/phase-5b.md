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
| 12 | Gates: crash gate, corpus round trip, reference gate, save budgets, CI on three OSes, fuzzing, measurements | |
| 13 | Manual test steps for a Windows CI build; the project owner's test; the final report | |

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
