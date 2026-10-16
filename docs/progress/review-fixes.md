# Review fixes: progress

Branch `review-fixes`, started 2026-10-16 from `main` (ce171cb). It fixes the findings of three reviews of phases 1–3: Claude Code's (F1–F21), Codex's (C1–C8) and Grok's (two notes). The review reports themselves stay out of the repository. This file is updated with every commit, so where the work stands never lives only in one session's notes.

**The rule for group 1**, now in `docs/specs/journal.md`: nothing that holds a writer's unsaved work is deleted, truncated or overwritten unless it is copied aside first, and nothing is written before the checks that could refuse it.

Each finding gets a test made from the reviewer's reproduction. The test is run before the fix, to see it fail for the reason the reviewer gives, and again after.

## Findings

| Group | Finding | Outcome | Test | Commit |
| --- | --- | --- | --- | --- |
| 1 | F4: an agent's `doc_close {discard}` deletes recovered work | fixed. Before: the agent's discard returned `{}` and the journal was gone | `core/tests/no_lost_work.rs` `f4_*`; `cli/tests/agent_gate.rs` `an_agent_cannot_discard_the_writers_recovered_work` (real `coldopen mcp`) | "Keep a copy of discarded work; agents can't discard the writer's" |
| 1 | F1: `coldopen run --doc` deletes a recovered journal | in progress | | |
| 1 | F5 / C3: `doc.new` truncates an existing journal | to do | | |
| 1 | F2 / C2: save-as writes before it locks the target | to do | | |
| 1 | F14: an entry-less journal that disagrees with the file is thrown away | to do | | |
| 1 | F10: a failed journal write leaves a fragment | to do | | |
| 2 | F3 / C4: a synchronous save races the background save | to do | | |
| 2 | F11: saving overwrites another program's changes | to do | | |
| 2 | F13: `doc.save {path}` and `export.pdf` overwrite any file | to do | | |
| 2 | F19: an agent saves with `allow_lossy` | to do | | |
| 3 | C1: save, undo, type, crash: recovery fails with `nothing_to_undo` | to do | | |
| 3 | F8 / C6: replay demands byte-equal results | to do | | |
| 3 | F9: a diverged entry stays in the recovered document | to do | | |
| 3 | C7: journal headers are not validated | to do | | |
| 3 | F16: undo targets, resolved params, `semantics`, foreign journals | to do | | |
| 3 | F17.3: journals from earlier phases as fixtures | to do | | |
| 4 | F7: the palette runs session commands past the app's flows | to do | | |
| 4 | C5: a failed `doc_open` in `coldopen mcp` drops the open document | to do | | |
| 4 | F12: `doc.revert` reuses block ids | to do | | |
| 4 | F20 / C8: unbounded link queues | to do | | |
| 5 | F6: UTF-16 Fountain files | to do | | |
| 5 | F15: quadratic time in the Fountain reader and writer | to do | | |
| 6 | F17.1: the reference gate passes while comparing nothing | to do | | |
| 6 | F17.2: the journal fuzz target fuzzes a copy of replay | to do | | |
| 6 | F17.4: the crash gate kills only while typing | to do | | |
| 6 | F17.5: a cheap fuzz schedule with a per-input timeout | to do | | |
| — | F18: architecture before phase 4 | written up below, not implemented | | |
| — | F21: whitespace normalisation not reported as a loss | noted in `fountain.md` for phase 5 | | |
| — | Grok: header size; flusher's cloned handle | flusher assumption documented in `journal.md` | | |

## Decisions

1. Each finding is its own commit: spec, test and fix together. The branch is pushed after each group.
2. **F4.** `Dispatcher::discard` keeps a copy of the journal (`<journal>.discarded-<ms>`) whenever the session holds unsaved work, and `doc.close` reports the copy in a `journal_discarded` warning. As a result the app's "Don't save", and Discard on a never-saved script, now leave a copy too, and the dialog says so.
3. **F4.** An agent may not set aside a writer's unsaved work by any route: `doc.close {discard: true}`, `doc.revert` and `doc.open {recover: "discard"}` all fail with `not_your_step` when the work at stake includes a writer's entries. Each of these keeps a copy, but the writer would never be shown that copy.
4. **F1.** `coldopen run` keeps recovered work in the journal when the batch did not touch it. If the batch edited on top of recovered work, it keeps a copy and names it. A batch's own edits, which can be made again by rerunning it, are still deleted with no copy (`Dispatcher::discard_rerunnable`).
5. **F5.** `doc.new` refuses any non-empty journal at its path with `already_exists`, without analysing it. `doc.open` is the way to recover it or set it aside.
6. **F2.** Save-as judges the target's journal exactly as opening the target would. Where the open would recover, the save fails with `unsaved_changes` (the existing code, with `details.path` naming the target) and writes nothing.

## Not now: F18, architecture before phase 4

To be written up.
