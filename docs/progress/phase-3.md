# Phase 3 progress: desktop editor and MCP server

Branch `phase-3-editor-mcp`, started 2026-10-15 from `main`. This file is
kept up to date as the phase goes, so nothing about where it stands lives
only in one person's (or one agent's) notes.

## The plan, in order

| Step | What | State |
| --- | --- | --- |
| 1 | Specs: `editor.md`, `mcp.md`, and updates to `commands.md`, `journal.md`, `layout.md`, `fountain.md` | done |
| 2 | Core: keystroke commands, authors, grouped undo, journal fields | done |
| 3 | Core: `batch`, `text.find`, `text.complete`, `text.replace` with `at`, `doc.revert` | done |
| 4 | Core: deferred journal flushing (250 ms) and saves off the UI thread | done |
| 5 | Fountain: byte-level source map for the normal form (and normalization 10× faster) | done |
| 6 | Layout: incremental repagination, its property test, the draft view, the synthetic 120-page script | done |
| 7 | Core: the session's cached, incrementally laid-out view | done |
| 8 | App: first build (page widget, editor model, navigator, palette, find, themes, keymap); screenshots reviewed | done |
| 9 | Editor typing through the real UI, and the typing gate | done |
| 10 | Autocomplete, find and replace, palette, zoom, draft view, keymap: finish and test | next |
| 11 | Files (dialogs, recent files, PDF export), recovery dialog, save worker | done |
| 12 | Agent link server in the app, `crates/mcp`, `coldopen mcp`, the agent gate | |
| 13 | The headless driver and the crash gate | |
| 14 | CI: app builds and artifacts on three OSes, budgets job, fuzz targets | |
| 15 | Measurements: Windows locally (Big Fish, both renderers at 4K) and all three OSes in CI | |
| 16 | README, the PR, the final report | |

## Measured so far (Windows 11, release build)

| What | Number |
| --- | --- |
| Keystroke in the core on Big Fish: command plus journal write | 0.019 ms median, 1.0 ms p99 |
| Bringing the view up to date after a keystroke on Big Fish | 3.6 ms median |
| Full layout of the synthetic script (119 pages) | 16 ms (budget 50 ms) |
| Incremental repagination after an edit on page 90 | 0.83 ms median, 1.37 ms worst, at most 3 pages laid out (GitHub Linux runner: 2.01 ms median, full layout 27 ms) |
| Fountain normalization of Big Fish | 17.4 ms before, 1.7 ms after |
| Interface font (two subset faces) | 73,740 bytes raw, about 34.5 KB compressed |

Drawing time, cold start, memory and binary size are measured in steps 14
and 15.

## Decisions made during the phase

The four up-front choices (private local socket with agent access on, one
undo step per tool call, write now and fsync within 250 ms, tiny-skia with
both renderers measured) and the reviewer's additions to them are in the
plan and the specs. These are the decisions made along the way without
stopping to ask:

1. Text offsets in the new commands count Unicode scalar values (characters) of the block's printed text, not bytes or UTF-16 units.
2. Text inserted at an offset goes right after the previous printed character, ahead of any notes or boneyard there, so a note after a word stays after it while typing continues.
3. Typing groups into one undo step while it's contiguous, in one block, by the same writer, with gaps under 1.5 s. Backspace runs and forward-delete runs group the same way. Agent commands never group (one step per tool call).
4. Authors: `writer` (app, CLI, stdio) or `agent` (name and tool). The JSON envelope can't claim an author; only the MCP server and the app's agent link set agent authors.
5. `block.split` keeps attributes on the first half, and the new block gets none. Its type is the given `type`, or the same type. Hidden runs at the split point go with the second half.
6. `block.join` keeps the earlier block's type and attributes, and returns `{id, at}` for placing the caret.
7. `batch` takes edits only, is all-or-nothing, and makes one undo step and one journal entry.
8. `doc.revert` is new (Discard in the recovery dialog, and Revert to Saved). The journal is copied aside, never deleted.
9. `doc.open` of a missing file recovers from its journal if one exists, so never-saved documents survive crashes.
10. The app saves off the UI thread with a `doc.saved` journal marker written before the file is replaced; edits made during a save stay in the journal. "Save as" stays synchronous.
11. "Typing goes idle" means 100 ms with no new journal entry; the flusher flushes then. The hard limit is 250 ms after each write.
12. `doc.get` gains `rev` (edits, undos and redos since open), used for live-update events and MCP resource freshness.
13. `history.list` is a new query; `undo` and `redo` results gain `label`; an agent's undo or redo of someone else's step fails with `not_your_step`.
14. A `block.split` at the very end leaves hidden runs (notes) with the speech rather than moving them to the new block.
15. Normalization took 17 ms on Big Fish and the editor normalizes on every keystroke, so it was made 10× faster (same output) in its own commit.
16. Incremental layout memoizes per-element measurement, reassembles the flow and diffs it, rather than rebuilding scene by scene. It's simpler and equal to a full layout by construction; `layout.md` §15.2 says so.
17. Elements and rich text derive `Ord` (layout's lint forbids `HashMap`), so the measurement memo is a `BTreeMap`.
18. Layout clusters gain `src`, their source byte offset, so the editor can map clicks and carets inside lines printed in capitals.
19. Typing in kinds that print in capitals inserts capitals; word-backspace deletes the word with its punctuation.
20. A new script starts with one empty scene heading, in `Unsaved/Untitled-<n>.fountain`, so it is journaled from the first keystroke.
21. iced runs on its smol executor (timers need smol or tokio; smol is lighter).
22. The interface font has no arrows or ⌘ symbol, so the hint bar spells keys and flow out as text ("Enter: Dialogue", "Ctrl+K").
23. The palette also lists core commands; a command that needs params opens a JSON params form and shows the response.
24. Undo history is shown through `history.list`; agent steps use the label `<name> · <tool>`.
25. The gates run the app with our own headless runner (`crates/app/src/headless.rs`) rather than iced_test's Simulator, because the Simulator drops the tasks `update` returns: focus, scrolling, the clipboard and the save worker would go untested.
26. `doc.new` gains `start`, one empty block that is part of the new document rather than an edit, so a fresh script is clean: it never asks to be saved and is never offered as lost work.
27. `doc.open`'s `recovered` gains `last`, the time of the newest recovered change, shown as "5 minutes ago" (no time zone code needed).
28. rfd is used without default features: XDG desktop portal on Linux through a libdbus loaded at run time (zenity as its fallback), no GTK.
29. The recovery dialog counts recovered commands, so a burst of typing counts each key.
30. Discard in the recovery dialog for a script never saved deletes the recovered work (there is no file to go back to), and the dialog says so first.
31. CI fails the incremental repagination test above 4 ms, not 2 ms: the shared runner measured 2.01 ms on noise. The aim stays 2 ms.
32. Keys in the app's own dialogs: Enter or S saves, D or N doesn't, Esc or C cancels; recovery Enter, K or Esc keeps, D discards; export Mod+1 and Mod+2 toggle scene numbers and the title page.
33. Mod+S while a save is running saves again when it finishes, rather than being refused.
