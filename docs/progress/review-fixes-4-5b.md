# Review fixes for phases 4–5b: progress

Branch `review-fixes-4-5b`, started 2026-10-28 from `main` (75bf19c). It fixes the findings of four reviews of `7707861..75bf19c`, phases 4, 5a and 5b: Grok's (GR1–GR12), Claude's (CL1–CL8), Astra's (GP1–GP14) and Antigravity's (AR1–AR12). The reports stay out of the repository. This file is updated and pushed after every group, so where the work stands never lives only in one session's notes.

**The rule for group 1**, now in `docs/specs/changes.md` §6: accepting or rejecting a change never removes text or blocks that aren't part of that change. A mark that can't settle cleanly fails with `change_conflict` when settled by id, and otherwise stays pending and is listed in `skipped`.

Each finding gets a test made from the reviewer's reproduction. It is run before the fix, to see it fail for the reason the reviewer gives, and again after. Same-bug findings share one commit, holding the spec, the test and the fix.

## Findings

| Group | Finding | Outcome | Test | Commit |
| --- | --- | --- | --- | --- |
| 0 | Journals of semantics 3, as phase 5b's build wrote them | done: `phase5b.journal`, written by 75bf19c's `serve --stdio` before any fix, holds every settle this round changes; today's code replays it to that build's blocks | `core/tests/old_journals.rs` (all four tests now include it) | "Journal fixture from the phase 5b build" |
| 1 | GR1 = GP1 = AR1 (part 2): a reject of an inserted block deletes untracked text in it | pending | | |
| 1 | AR1 (part 1): a split of one's own inserted block is marked `inserted` | pending | | |
| 1 | GR4: rejecting a retype back to a page break deletes text typed after it | pending | | |
| 1 | GR3: accepting a join after the block before it was deleted drops the joined text | pending | | |
| 1 | AR4: a conflicted split or join is stripped, and listed in both `changes` and `skipped` | pending | | |
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

## Decisions

Decided with the project's owner before any code (2026-10-28):

1. **The common rule** for settling (group 1), above.
2. **"Save anyway"** (CL6, AR3) runs every check that can refuse the save first, so a refused save changes nothing. Accepting and clearing then become one undo step.
3. **Moves of different scenes in one change** record their order, so rejecting undoes them newest first. A project holding such an order is bundle version 3.
4. **The diff past its pair bound** anchors on blocks whose text occurs once in each draft, rather than showing the unit as deleted and added.
5. **Unknown bundle entries** (GP3) are kept, not reported as a loss.
