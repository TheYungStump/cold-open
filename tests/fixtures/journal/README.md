# Crash journals from earlier phases

Review finding F17.3 (2026-10-16): nothing checked that a journal written by an earlier version of Cold Open still recovers. These fixtures were written by the real builds of earlier phases, so `crates/core/tests/old_journals.rs` can replay them with today's code.

`lighthouse.fountain` is a short scene we wrote for these tests.

| Fixture | Written by | Requests |
| --- | --- | --- |
| `phase1.journal` | `coldopen serve --stdio` built from 0654f37 (phase 1) | `phase1-2.requests.ndjson` |
| `phase2.journal` | the same, built from f26f59c (phase 2) | `phase1-2.requests.ndjson` |
| `phase3.journal` | the same, built from ce171cb (phase 3) | `phase3.requests.ndjson` |

Each was made the same way: copy `lighthouse.fountain` into an empty folder, pipe the requests into that build's `coldopen serve --stdio` there, and let input end with the document unsaved, which keeps the journal exactly as a crash would. The journal is `.lighthouse.fountain.coldopen-journal`, renamed here because that name is gitignored. The last two requests are `doc.export` and `doc.get`; their results, from the same build, are `<phase>.expected.fountain` and `<phase>.expected-blocks.json`.

Never regenerate these with a newer build: their value is that an older build wrote them.
