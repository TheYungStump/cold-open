# Review fixes for phases 4–5b: progress

Branch `review-fixes-4-5b`, started 2026-10-28 from `main` (75bf19c). It fixes the findings of four reviews of `7707861..75bf19c`, phases 4, 5a and 5b: Grok's (GR1–GR12), Claude's (CL1–CL8), Astra's (GP1–GP14) and Antigravity's (AR1–AR12). The reports stay out of the repository. This file is updated and pushed after every group, so where the work stands never lives only in one session's notes.

**The rule for group 1**, now in `docs/specs/changes.md` §6: accepting or rejecting a change never removes text or blocks that aren't part of that change. A mark that can't settle cleanly fails with `change_conflict` when settled by id, and otherwise stays pending and is listed in `skipped`.

Each finding gets a test made from the reviewer's reproduction. It is run before the fix, to see it fail for the reason the reviewer gives, and again after. Same-bug findings share one commit, holding the spec, the test and the fix.

## Findings

| Group | Finding | Outcome | Test | Commit |
| --- | --- | --- | --- | --- |
| 0 | Journals of semantics 3, as phase 5b's build wrote them | done: `phase5b.journal`, written by 75bf19c's `serve --stdio` before any fix, holds every settle this round changes; today's code replays it to that build's blocks | `core/tests/old_journals.rs` (all four tests now include it) | "Journal fixture from the phase 5b build" (6a387b1) |
| 0 | Journal semantics 4, with a marker when an older journal goes on | done: `SEMANTICS` is 4; a recovered journal of 1–3 gets a `journal.semantics` marker, and replay switches rules there; `legacy.rs` keeps the rules semantics 4 changed | `core/tests/old_journals.rs` `a_phase_5b_journal_goes_on_under_semantics_4_and_recovers_again`, `a_semantics_marker_this_version_cannot_follow_stops_replay` | "Journal semantics 4, and a marker when an older journal goes on" (4623995) |
| 1 | GR1 = GP1 = AR1 (part 2): a reject of an inserted block deletes untracked text in it | fixed (semantics 4). Before: the agent's reject of its own block returned `{"changes":["c_1"],"count":1,"skipped":[]}` and the writer's " by the door" went with the block; the writer's own reject did the same to their untracked sentence. Grok and Astra expected `not_your_change`; by the common rule it is `change_conflict` for everyone, since nothing is removed | `core/tests/changes.rs` `an_agents_reject_keeps_the_writers_words_in_its_block`, `rejecting_a_block_keeps_untracked_words_typed_into_it` | "Settling never removes text typed into an inserted block" (6fdd031) |
| 1 | AR1 (part 1): a split of one's own inserted block is marked `inserted` | fixed (semantics 4). Before: the new block was `{"inserted":"c_2"}`, and rejecting `c_2` deleted "Second sentence." | `core/tests/changes.rs` `rejecting_a_split_of_ones_own_block_joins_it_back` | "A later split of one's own inserted block is a split" (bec4892) |
| 1 | GR4: rejecting a retype back to a page break deletes text typed after it | fixed (semantics 4). Before: the reject returned `{"changes":["c_1"],...}`, and the block was a page break with "Hello" gone | `core/tests/changes.rs` `rejecting_a_retype_never_empties_text_typed_since` | "Rejecting a retype never empties text typed since" (8be0e7d) |
| 1 | GR3: accepting a join after the block before it was deleted drops the joined text | fixed (semantics 4). Before: accepting the join succeeded onto the pending deletion, and accepting that deletion took the bedroom heading's text with it | `core/tests/changes.rs` `a_join_onto_a_block_waiting_to_be_deleted_waits_for_it` | "A join onto a block waiting to be deleted waits for it" (56113fb) |
| 1 | AR4: a conflicted split or join is stripped, and listed in both `changes` and `skipped` | fixed (semantics 4). Before: `{"changes":["c_1"],"count":1,"skipped":["c_1"]}`, and `changes.list` was empty | `core/tests/changes.rs` `a_split_that_cannot_join_back_stays_pending`; the accept-all property and the `changes` fuzz check now allow the skipped changes' marks | "A split or join that can't be settled stays pending" (e44836c) |
| 1 | P1 (found by the new property): a deleted block can hold text no deletion marks, when an earlier deletion inside it is withdrawn; accepting the block's deletion removed it | fixed (semantics 4): such a block is held, and the accepted form keeps it. Before: the accepted form and accept-all dropped the word | `core/tests/changes.rs` `accepting_a_block_deletion_keeps_words_no_deletion_marks`; the property `settling_never_removes_text_outside_the_change` (`fuzzing::settle_keeps`, also in the `changes` fuzz target) | "Accepting a block deletion keeps words no deletion marks" (e738de2) |
| 2 | CL1 = GP6, AR12 (part 1): `--untracked` agent edits and title edits don't replay; revert drops the override | pending | | |
| 2 | CL2: restoring a copy that replays only in part succeeds silently | pending | | |
| 2 | GP4: agent-only replay stops protecting a restored draft | pending | | |
| 2 | GP5: a never-saved project can't recover its journal | pending | | |
| 2 | CL6 = AR3: "Save anyway" accepts and clears before the checks that refuse the save | pending | | |
| 2 | GR12: a tracking switch in a Fountain or .fdx script is lost on reopen | pending | | |
| 2 | GP9: revert renumbers blocks but not move anchors or revision baselines | pending | | |
| 2 | AR11 (part 1): replay doesn't compare `change` ids | pending | | |
| 3 | GR8 = GP2 (A): a page break that starts a group holds the group's kept paragraph | pending | | |
| 3 | GP2 (B): un-dualling a dual-dialogue group throws away its carry | pending | | |
| 3 | CL8: editing a dual speech drops the outer `StartsNewPage="No"` | pending | | |
| 3 | GP3: unknown bundle entries are dropped on save | pending | | |
| 4 | GR5 = GP7 + GP8: rejecting a change that retyped or moved a block twice restores only part of it | pending | | |
| 4 | GR9: an untracked agent retype or move over another author's is allowed | pending | | |
| 4 | GR10: a revision base stores attributes a retype already cleared | pending | | |
| 4 | GP11: accept with no active set gives every change the highest revision | pending | | |
| 5 | GR6 = GP10 = AR2: import moves `Document::scene`, not the diff unit | pending | | |
| 5 | AR7: Shot or General make import add retypes diff doesn't show | pending | | |
| 5 | GR11 = CL7: gap pairing is quadratic and uncapped, on the UI thread | pending | | |
| 6 | GR2 = CL4 = GP12: `fill_to` fills the tables up to any id a mark names | pending | | |
| 6 | GR7 = GP13: an empty kept paragraph makes the .fdx writer loop forever | pending | | |
| 6 | CL3: .fdx repair is quadratic in nesting depth | pending | | |
| 6 | CL5: a 33 KB bundle takes 15 s to open | pending | | |
| 6 | GP14: an extra field in `meta.json` gets around the version refusal | pending | | |
| 7 | AR5: converting to Fountain doesn't adopt the .fdx spacing | pending | | |
| 7 | AR6: pending deletions count in `scene.list` and the reports | pending | | |
| 7 | AR8: the MCP scene resource returns text whose deletion is pending | pending | | |
| 7 | AR9: a custom continued marker isn't recognized | pending | | |
| 7 | AR10: section rows break "This scene", the status bar and the dots | pending | | |
| 7 | AR11 (part 2): the caret doesn't follow an agent's batch | pending | | |
| 7 | AR12 (part 2): the `unsupported_format` message names only Fountain | pending | | |

## Checks per group

- **Step 0 and group 1** (2026-10-28, Windows 11): `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (no failures, the crash gate included), and clippy of core for `x86_64-unknown-linux-gnu` and `aarch64-apple-darwin`. The changes properties ran at 1,024 cases and the new settle property at 8,192, all passing. `old_journals.rs` replays the phase 5b fixture to that build's blocks after every commit.

## Decisions

Decided with the project's owner before any code (2026-10-28):

1. **The common rule** for settling (group 1), above.
2. **"Save anyway"** (CL6, AR3) runs every check that can refuse the save first, so a refused save changes nothing. Accepting and clearing then become one undo step.
3. **Moves of different scenes in one change** record their order, so rejecting undoes them newest first. A project holding such an order is bundle version 3.
4. **The diff past its pair bound** anchors on blocks whose text occurs once in each draft, rather than showing the unit as deleted and added.
5. **Unknown bundle entries** (GP3) are kept, not reported as a loss.
