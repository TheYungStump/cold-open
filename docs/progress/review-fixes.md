# Review fixes: progress

Branch `review-fixes`, started 2026-10-16 from `main` (ce171cb). It fixes the findings of four reviews of phases 1–3: Claude Code's (F1–F21), Codex's (C1–C8), Grok's (two notes) and Antigravity's (A1–A12, added 2026-10-16; see "The Antigravity review" below). The review reports themselves stay out of the repository. This file is updated with every commit, so where the work stands never lives only in one session's notes.

**The rule for group 1**, now in `docs/specs/journal.md`: nothing that holds a writer's unsaved work is deleted, truncated or overwritten unless it is copied aside first, and nothing is written before the checks that could refuse it.

Each finding gets a test made from the reviewer's reproduction. The test is run before the fix, to see it fail for the reason the reviewer gives, and again after.

## Findings

| Group | Finding | Outcome | Test | Commit |
| --- | --- | --- | --- | --- |
| 1 | F4: an agent's `doc_close {discard}` deletes recovered work | fixed. Before: the agent's discard returned `{}` and the journal was gone | `core/tests/no_lost_work.rs` `f4_*`; `cli/tests/agent_gate.rs` `an_agent_cannot_discard_the_writers_recovered_work` (real `coldopen mcp`) | "Keep a copy of discarded work; agents can't discard the writer's" |
| 1 | F1: `coldopen run --doc` deletes a recovered journal | fixed. Before: `run` printed "unsaved changes were discarded" and the journal was gone (after F4, it was only copied aside) | `cli/tests/cli.rs` `run_never_discards_recovered_work`, `run_keeps_a_copy_when_it_edits_recovered_work` | "coldopen run: never throw away recovered work" |
| 1 | F5 / C3: `doc.new` truncates an existing journal | fixed. Before: the second `doc.new` succeeded with no warning and the journal was truncated | `core/tests/no_lost_work.rs` `f5_*` | "doc.new: never start over on a crash journal" |
| 1 | F2 / C2: save-as writes before it locks the target | fixed. Before: `doc_locked` came back after the target was already overwritten, and a crashed target's journal was truncated | `core/tests/no_lost_work.rs` `f2_*` | "Save as: lock and judge the target before writing it" |
| 1 | F14: an entry-less journal that disagrees with the file is thrown away | fixed. Before: `recovered: null`, no warning, and the header holding "SAVED WORK." was overwritten | `core/tests/no_lost_work.rs` `f14_*`; `app/tests/file_flows.rs` `a_save_lost_in_a_power_cut_is_offered_back` | "Recover a journal header the file disagrees with" |
| 1 | A2: after F14, Discard (`doc.revert`) of a recovered header kept no copy, and the start-up scan missed such journals | fixed with F14, before it was committed. Before: no `journal_discarded` warning and no copy; the scan returned `[]` | `core/tests/no_lost_work.rs` `f14_discarding_a_recovered_header_keeps_a_copy`; `app/src/files.rs` `a_recent_script_with_a_lost_save_is_leftover_work` | same commit as F14 |
| 1 | F10: a failed journal write leaves a fragment | in progress | | |
| 2 | F3 / C4: a synchronous save races the background save | to do | | |
| 2 | A6.2: a background save commits into the document opened after it | to do (reproduced 2026-10-16, see below); fixed by the same save generation as F3 | | |
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
| — | F21: whitespace normalisation not reported as a loss | noted in `fountain.md` for phase 5, with A12's case | | |
| — | A4: D or N on the lossy-save question discards the script | rejected: both keys only cancel | `app/tests/file_flows.rs` `d_or_n_on_the_lossy_save_question_cancels` (passes on unchanged code) | |
| — | Grok: header size; flusher's cloned handle | flusher assumption documented in `journal.md` | | |

## Decisions

1. Each finding is its own commit: spec, test and fix together. The branch is pushed after each group.
2. **F4.** `Dispatcher::discard` keeps a copy of the journal (`<journal>.discarded-<ms>`) whenever the session holds unsaved work, and `doc.close` reports the copy in a `journal_discarded` warning. As a result the app's "Don't save", and Discard on a never-saved script, now leave a copy too, and the dialog says so.
3. **F4.** An agent may not set aside a writer's unsaved work by any route: `doc.close {discard: true}`, `doc.revert` and `doc.open {recover: "discard"}` all fail with `not_your_step` when the work at stake includes a writer's entries. Each of these keeps a copy, but the writer would never be shown that copy.
4. **F1.** `coldopen run` keeps recovered work in the journal when the batch did not touch it. If the batch edited on top of recovered work, it keeps a copy and names it. A batch's own edits, which can be made again by rerunning it, are still deleted with no copy (`Dispatcher::discard_rerunnable`).
5. **F5.** `doc.new` refuses any non-empty journal at its path with `already_exists`, without analysing it. `doc.open` is the way to recover it or set it aside. `doc.new` also runs all its checks before it closes the open document, so a refusal leaves that document open.
6. **F2.** Save-as judges the target's journal exactly as opening the target would. Where the open would recover, the save fails with `unsaved_changes` (the existing code, with `details.path` naming the target) and writes nothing.
7. **F14.** With no entries, a file that matches the header's (or a marker's) fingerprint, or holds the header's document, is nothing to recover; anything else recovers the header's document with `commands: 0` and `external_change: true`. The fingerprint is checked first, so a file not in Cold Open's normal form is not mistaken for a change. The app's recovery dialog has its own sentence for this case.

8. **A2.** `doc.revert` keeps a copy whenever the document has unsaved changes, not only when the journal has entries. The app's start-up scan judges a recent script's journal as opening it would (`coldopen_core::dispatcher::journal_holds_work`). It still counts lines in `Unsaved/`, where a journal without entries is a script nobody typed in.

## The Antigravity review (added 2026-10-16)

The user added `REVIEW-Antigravity.md` (Gemini, reviewing `a729250` and the then uncommitted F14 work) on 2026-10-16. Many of its code quotes and line numbers name functions this repository does not have (`Session::from_doc`, `Dispatcher::open_file`, `recover: false`, `Target::Local`, `doc.subscribe`, `check_lossy`), so each claim was checked against the real code, and reproduced where it was new.

| # | Claim | Verdict |
| --- | --- | --- |
| A1 | Save, undo, type, crash loses everything after the save | Same as C1 (group 3). |
| A2 | Discarding a header-only recovery keeps no copy; the start-up scan ignores header-only journals | **New and real**, caused by F14. The real `doc.revert` guard was `entries() > 0`. `doc.open {recover: "discard"}` was already fine: it copies any non-empty journal. Reproduced and fixed with F14 (table above). |
| A3 | A partial journal write leaves a fragment; a failed sync leaves a rolled-back entry | Same as F10 (group 1, next). |
| A4 | D or N on the lossy-save question closes without saving | **Rejected.** In `App::reply`, `(Prompt::Lossy, Reply::DontSave)` only clears `after_save`; the quoted match arm does not exist. A headless test (Shot, Mod+W, Save, then D or N) leaves the script open, unsaved, its file and journal untouched. |
| A5 | `coldopen mcp` drops the open document on a failed `doc_open` | Same as C5 (group 4). |
| A6 | Background save races: (1) agent `doc_save`, (2) close or switch during a save, (3) palette session commands | (1) is F3/C4, (3) is F7. **(2) is a new trigger, and real.** In core: `save_prepare` at rev 1, `doc.close {discard}`, `doc.open` of the same file, one insert (rev 1 again), then the worker's write and `save_commit`: the commit returns `dirty: false`, the file holds the old prepared text ("X."), and the document holds "Y.", which is now in neither file nor journal after a clean close. In the app this needs the save to still be running when the script is reopened, so the window is short. Added to group 2 as A6.2. |
| A7 | Saving overwrites another program's changes | Same as F11 (group 2). |
| A8 | An attached agent can overwrite any `.pdf` with `export_pdf`, and save with `allow_lossy` | F13 and F19 (group 2). One new angle, now a group 2 decision: the `overwrite: true` that F13 adds must be refused from agents (over the link, and in `coldopen mcp`), like `allow_lossy`, or an agent would just pass it. |
| A9 | `doc.revert` reuses block ids | Same as F12 (group 4). |
| A10 | Unbounded link channels: (1) events to a client that stops reading, (2) requests into the app | Both are C8 (the per-link outbound channel is `std::sync::mpsc::channel()`, `crates/app/src/link.rs:341`). Group 4's test will cover a client that stops reading `changed` events, not only one that pipelines requests. |
| A11 | UTF-16 Fountain files decode as Windows-1252 | Same as F6 (group 5). |
| A12 | The last action's trailing blank lines are dropped with no loss reported | Real (reproduced: `block.insert` of `"End.\n\n"` as the last block exports as `End.` with `losses: []`), and the same class as F21: normalisation not reported. Added to the F21 note for phase 5. |

## Not now: F18, architecture before phase 4

To be written up.
