# Crash journals from earlier phases

Review finding F17.3 (2026-10-16): nothing checked that a journal written by an earlier version of Cold Open still recovers. These fixtures were written by the real builds of earlier phases, so `crates/core/tests/old_journals.rs` can replay them with today's code.

`lighthouse.fountain` is a short scene we wrote for these tests. `lighthouse.fdx` is the same scene as .fdx, written by the phase 4 build with `export.fdx` (2026-10-18).

| Fixture | Written by | Requests |
| --- | --- | --- |
| `phase1.journal` | `coldopen serve --stdio` built from 0654f37 (phase 1) | `phase1-2.requests.ndjson` |
| `phase2.journal` | the same, built from f26f59c (phase 2) | `phase1-2.requests.ndjson` |
| `phase3.journal` | the same, built from ce171cb (phase 3) | `phase3.requests.ndjson` |
| `phase4.journal` | the same, built from cddbb1d (phase 4, with the fixes after it), 2026-10-18 | `phase3.requests.ndjson` |
| `phase4-fdx.journal` | the same build, on `lighthouse.fdx` | `phase4-fdx.requests.ndjson` (the phase 3 requests, opening the .fdx file) |
| `phase5a.journal` | the same, built from 67e43d9 (phase 5a, with #16), 2026-10-26: tracked edits by the writer, an accept, a reject, two changes left pending, then untracked edits with an undo and a redo (semantics 2) | `phase5a.requests.ndjson` |
| `phase5b.journal` | the same, built from 75bf19c (phase 5b), 2026-10-28, before the fixes of the reviews of phases 4–5b: every settle the review fixes change, as that build did them (a reject of an inserted block holding untracked text, a split of one's own inserted block, a retype back to a page break rejected, a join accepted after the block before it was deleted, a conflicted split, two retypes and two moves rejected in one change), and revision sets marking an accept and accepts with no set active (semantics 3) | `phase5b.requests.ndjson` |

Each was made the same way: copy `lighthouse.fountain` into an empty folder, pipe the requests into that build's `coldopen serve --stdio` there, and let input end with the document unsaved, which keeps the journal exactly as a crash would. The journal is `.lighthouse.fountain.coldopen-journal` (`.lighthouse.fdx.coldopen-journal` for the .fdx one), renamed here because that name is gitignored. The last two requests are `doc.export` and `doc.get`; their results, from the same build, are `<phase>.expected.fountain` and `<phase>.expected-blocks.json`.

Since phase 4 the header records the computer's name and the script's path (`host`, `path`). The repo is public, so in `phase4.journal`, `phase4-fdx.journal`, `phase5a.journal` and `phase5b.journal` those two values were replaced with `fixture-host` and `/fixtures/<script>`; nothing else was touched. On any computer but the one that wrote it, such a journal is one "from somewhere else" anyway, so the test opens these with `recover: "apply"`.

Never regenerate these with a newer build: their value is that an older build wrote them.
