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
| 1 | Branch, and this file | done (branch rebased onto `main` once #16 is merged) |
| 2 | Specs before code: `revisions.md`, `diff.md` and `reports.md` (new); `changes.md`, `bundle.md`, `journal.md`, `commands.md`, `layout.md`, `editor.md`, `mcp.md`, `fdx.md` and `fountain.md`; the crate table in `CLAUDE.md` | done |
| 3 | The revision model: marks, the revision table, the commands, marking in the dispatcher and on accept, change table v2, `semantics` 3; property tests | |
| 4 | Files: bundle v2 and `revisions.json`, the journal header, the `revision_marks` loss and Save anyway, the `revisions` fuzz target | |
| 5 | Layout and PDF: asterisks, revised-page headers, colored pages; page view and PDF goldens | |
| 6 | The app's revisions: start form, palette, inks, asterisks, the active set in the hint bar | |
| 7 | Shared names and the three reports: core, CLI (text, JSON, CSV), MCP tools and resources, the report view | |
| 8 | The title page editor: commands, op, the agent rule, the app sheet | |
| 9 | Script diff: the `coldopen-diff` crate, the `diff` command, CLI, MCP, PDF, Compare with…; the Big Fish variants; the budget; property tests and a fuzz target | |
| 10 | Bringing a draft in as tracked changes (`changes.import`) | |
| 11 | Recent files that no longer exist | |
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
