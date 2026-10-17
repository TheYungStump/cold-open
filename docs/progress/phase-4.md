# Phase 4 progress: .fdx exchange

Branch `phase-4-fdx`, started 2026-10-16 from `main` (7707861). This file is updated and pushed with every step, so where the work stands never lives only in one session's notes.

Phase 4 makes Cold Open read and write .fdx. A file that came from another app opens, is edited and is saved back with nothing lost. Whatever Cold Open doesn't model passes through untouched, and a damaged file opens with a report of what was repaired. The phase also settles three things from the review of phases 1–3 (`docs/progress/review-fixes.md`, "F18"): the format seam (F18.1), one app-level command router (F18.5), and set-aside recovery copies that writers never have to manage.

## The plan, in order

| Step | What | State |
| --- | --- | --- |
| 1 | Branch, and this file | done |
| 2 | Format seam (F18.1): a `Format` trait in core with Fountain behind it, no change in behaviour; the journal header records the format. Its own reviewed commit | done |
| 3 | Specs before code: `fdx.md` (writer, passthrough, round trip, repair report), `commands.md`, `journal.md` (F18.2–F18.4, hidden files, Recovered versions, cleanup), `editor.md`, `mcp.md`, `layout.md`, `fountain.md` | done |
| 4 | Model and layout: the document's page format, title lines, paragraph spacing and passthrough store; per-document layout | done, except the store, which comes with the reader (step 5) and core integration (step 8) |
| 5 | .fdx reader: a lossless XML layer, the passthrough store, resyncing after syntax errors, report classes; our own fixtures, broken ones included | done |
| 6 | .fdx writer: write back what was read byte for byte, generate what was edited; property tests, the hostile-input timing test, fuzz targets for the writer | done |
| 7 | The round-trip gate on the corpus, with its report | done on the nine corpus files here; to run again when the new downloads arrive |
| 8 | Core: .fdx open, save and conversion, `export.fdx`, `export.fountain`, `file.repair`, damaged originals; the CLI's `repair` and `convert`; MCP tools | done |
| 9 | Recovery copies: hidden on Windows, `recovered.*` commands, clean-up | done: core in this step (hidden files, commands, the clean-up rule); the app's clean-up and screen in step 11 |
| 10 | App: one command router, `App::dispatch(request, origin)` (F18.5) | done: the refactor (117f45e, app tests unchanged), then restore, delete and waiting for saves in flight (ccf4b0b) |
| 11 | App: .fdx dialogs, Save As conversion, exports, the repair dialog, the Recovered versions screen, notices without paths | done: all of these, plus the start-up clean-up and leftover work for restored scripts; screenshots reviewed in three themes |
| 12 | Gates: crash gate (kills during .fdx saves and Save As conversion), typing gate on .fdx, agent gate; CI on three OSes; measurements; your manual test of the Windows build; the final report | gates done: the crash gate kills during .fdx saves and around a Save as to .fdx (five clean runs in a row); the typing gate types the scene into an .fdx script and reads it back; the agent gate saves an .fdx in place and writes copies. CI, measurements, your manual test and the report are next |

## Decisions made with the user before the phase (2026-10-16)

1. **"Lossless round trip" means byte-identical.** An unedited, well-formed corpus file comes back byte for byte. Only two kinds of file may differ, each difference listed in the gate's report: input that was not UTF-8 or UTF-16 and is re-encoded as UTF-8 (asked as a `reencoded` loss), and files the reader repaired (they differ exactly as the repair report says). Also gated: reading our output back gives the same model and passthrough, writing it again gives the same bytes, and an edited file changes only the paragraphs that were edited.
2. **Repair is automatic and reported.** Every fix happens when the file is read; nothing is asked item by item. The app shows a short report (with Details and "Save repaired copy…"), nothing is written until the writer saves, and the first save over the damaged file keeps a hidden copy of the original, listed in Recovered versions. The report is plain text for people and JSON for tools (`code`, `class`: fixed, dropped, kept or info, `paragraph`/`line`/`column`, `message`), and never quotes script text. `coldopen repair in out` prints the text report; `--json` prints JSON and `--report PATH` saves it.
3. **Notes, boneyard, sections and synopses are losses on .fdx save.** No publicly shared .fdx we have shows how anchored script notes are stored, so the clean room gives us nothing to write them from. They are listed as losses when saving or exporting .fdx, as a Shot is for Fountain. If a public file with anchored script notes turns up, notes get mapped to it in this phase.
4. **Recovered versions has no menu.** The app has no menu bar, so the screen is reached from a button in the notice, the palette and the start screen. The notice reads: "Your unsaved changes were set aside. Open Recovered versions to restore them."

## The corpus

Corpus files stay in the gitignored `corpus/`. On 2026-10-16 it held `fountain-io/` (Big Fish, Brick & Steel, The Last Birthday Card, each with its PDF) and `rsdoiel-fdx/` (six samples). The seven jumpcut files that `docs/specs/fdx.md` lists were not on this machine. Asked of the user, to download:

1. The seven jumpcut files `fdx.md` cites (MIT): `https://raw.githubusercontent.com/SimonGZ/jumpcut/main/tests/fixtures/corpus/public/<name>/source/source.fdx`, into `corpus/fd11/<name>/<name>.fdx` for big-fish, big-fish-scene-numbers and little-women, and `corpus/fdx-samples/<name>.fdx` for extranormal, gumshoe, mostly-genius and vikings.
2. wildwinter/screenplay-tools (MIT), `https://raw.githubusercontent.com/wildwinter/screenplay-tools/main/tests/<file>`, into `corpus/screenplay-tools/`: `TestFDX-FD.fdx` (saved by Final Draft), `TestFDX-FI.fdx` (saved by Fade In) and `TestFDX-FD-Write-{cpp,csharp,js,py}.fdx` (written by that library).
3. vilcans/screenplain (MIT), the nine files at `https://raw.githubusercontent.com/vilcans/screenplain/master/tests/files/<name>.fdx` (dialogue-with-blank-line, dialogue, dual-dialogue, extended-characters, forced-action, indentation, parenthetical, simple, utf-8-bom), into `corpus/screenplain/`.
4. rsdoiel/scripttool (AGPL, local only): `testdata/testplay-01a.fdx` and `testplay-01b.fdx`, into `corpus/rsdoiel-scripttool/`.
5. Wanted, not found yet: any public .fdx with anchored script notes, with revision-marked runs, or saved by Highland or WriterDuet.

The files are studied only with scripts that print structure (element paths, attribute names, counts), never script text, and each is recorded by sha256 in `fdx.md`.

**2026-10-17:** `corpus/jumpcut/` (the seven corpus files and six import fixtures) and `corpus/screenplain/` (nine test files, three examples) arrived: 25 more files, 34 in all. They were downloaded on Windows and have CRLF line breaks; converted back to LF, the jumpcut files match the hashes `fdx.md` recorded on 2026-10-13. screenplay-tools and scripttool have not arrived.

**Round trip on all 34 files (2026-10-17):** every file writes back byte for byte, unedited (G1); re-reading the output gives the same model and store, and writing again gives the same bytes (G2); the scripted edits change only the edited paragraphs (G3). The core test opens, edits, saves and reopens every file through the dispatcher. No file needed repair. Before the style fix below, the CRLF copies and the jumpcut files kept nearly all their paragraphs as raw text (2,754 of jumpcut's Big Fish, 827 KB of store); after it, jumpcut's Big Fish keeps 6 (35 KB) and screenplain's keeps none.

## Measurements (2026-10-16)

Release build on the development machine (Windows 11), median of 11 runs, with `cargo run --release -p coldopen-core --example open_times -- FILE`. "Open" is `doc.open` plus the first layout, which is what the app waits for before drawing. The budget is a 120-page script open in under 200 ms.

| File | Size | Pages | Open + first layout | Save in place | Journal header |
| --- | --- | --- | --- | --- | --- |
| Big Fish (.fdx) | 435 KB | 122 | 46 ms (31 + 15) | 6 ms | 491 KB |
| Big Fish (.fountain) | 146 KB | 125 | 34 ms (17 + 17) | 20 ms | 264 KB |
| The Last Birthday Card (.fdx) | 109 KB | 20 | 23 ms | 2 ms | 148 KB |
| Brick & Steel (.fdx) | 55 KB | 4 | 15 ms | 2 ms | 74 KB |
| rsdoiel samples 1–6 (.fdx) | 25–43 KB | 1–2 | 10–16 ms | 2 ms | 29–58 KB |

- Every open is well inside the 200 ms budget. The .fdx reader costs more than the Fountain one (the lossless XML layer and the generate-and-compare pass), and the layout costs less (the identity layout form skips the normal form).
- The .fdx journal header is about 1.1 times the file, because it carries the passthrough store. It is written once per save, inside the save's time above; the first edit after a save takes 0.3 ms.
- Saving .fdx is faster than saving Fountain: kept paragraphs are copied as they are.
- The new corpus files (2026-10-17, after the style fix): jumpcut's Big Fish (387 KB, 129 pages) opens in 36 ms, saves in 5 ms, header 304 KB; jumpcut's Vikings (356 KB, 114 pages) 32 ms, 5 ms, 284 KB; screenplain's Big Fish (342 KB, 124 pages) 35 ms, 5 ms, 265 KB.

## Decisions made during the phase

1. **The seam** (`crates/core/src/format.rs`). A `Format` trait (`read`, `style`, `write`, `layout_form`, `extensions`) and a `FormatId` enum, which the session and the journal header record. Every place core named the Fountain crate for a file now asks the session's format: open, new, revert, save and save-as, recovery's "already in the file" check, the save-as target check, the start-up journal check, `doc.export`, the views and PDF layout. So does the app's recovery dialog. Fountain as clipboard text (the editor) and as an MCP resource is not a file format and stays as it was.
2. **The header's `format` is left out for Fountain** (absent means Fountain, like `semantics`), so Fountain journals are byte for byte what they were. A format this version doesn't know makes the header unreadable, so the journal is kept aside rather than replayed as the wrong format.
3. **The `reencoded` loss stays in the save path**, not in Fountain's `write`, because it is about the file's bytes; `doc.export` returns text and never reported it.
4. **Evidence of no change in behaviour:** all 392 tests pass, and no existing test was changed; the only test edits are new tests for the seam. The review of the seam commit (`/code-review`, high) found seven things. Fixed:
   - an explicit `format` in `doc.save` that disagrees with the target's extension is now refused;
   - `Session::new` takes the format, so no constructor can forget it;
   - `doc.export` looks the format up once.

   Left for the .fdx steps, where they can be tested:
   - the recovery dialog's document when a file can't be read;
   - PDF layout building a source map it doesn't need;
   - a test of a non-Fountain header through `doc.save`.

   Not a bug: a recovered journal whose header names another format than its path's. A save then converts, and its loss check reports what the conversion drops.
5. **Specs written before code** (step 3):
   - `fdx.md` §12 to §18: the writer, passthrough, generating, losses, round trip and repair. Every report code gains a class (fixed, dropped, kept or info). A file is "repaired" when it has a fixed or dropped issue, and only a repaired file may differ when written back unedited.
   - `commands.md`: .fdx in open, save and export; conversion on Save as; `export.fdx`, `export.fountain`, `file.repair`, `recovered.list`, `recovered.restore` and `recovered.delete`; a new `file` class.
   - `journal.md`: the document's new header fields, damaged originals, hidden files, Recovered versions and clean-up, and the decisions for phase 5 (F18.2–F18.4).
   - `editor.md`: the .fdx flows, the repair dialog, the Recovered versions screen, and the router table (§16).
   - `mcp.md`, `layout.md` and `fountain.md`: the matching additions.
6. **Generate and compare** (`fdx.md` §13.2): at read time each paragraph is generated from its blocks and compared with its bytes. Only the paragraphs that differ are kept as text, which makes the unedited round trip byte-identical by construction and keeps the store, and so the journal header, small.
7. **`Version="1"` for new files**, the lowest version in the samples, and only elements every sample version has. New files use Courier Prime and our own `PageFormat` values (`fdx.md` §14.6).
8. **No `File ›` wording.** The notices say "Open Recovered versions to restore them." and carry a button (decision 4). A damaged original has its own sentence.
9. **Fountain from .fdx** (`fountain.md` §11): new losses for a non-default page format, spacing that isn't whole lines, title-line positions (with a written rule for turning lines into keys) and the .fdx store.
10. **Spacing overrides are keyed by block id in the document** (`Document.spacing`), not by element index as the reader gives them. Layout gets them as indices recomputed on every view refresh (`Document::overrides`), through `Incremental::update_with`, so an edit before an overridden paragraph never moves its spacing to another one. The flow is assembled again on every update anyway, so this adds no layout work.
11. **The document's page format, title lines and spacing are checked with the rest of a journal header** (`Document::check`): a format layout refuses, a spacing on an id at or above `next_id`, or a space outside the page body makes the header untrusted, as a bad id already did.
12. **Round-trip results** (2026-10-16, `cargo test -p coldopen-fdx --test roundtrip`): all nine corpus files come back byte for byte (G1), read back as the same document and store (G2), and keep every paragraph but the edited ones under scripted edits (G3). Our own fixtures pass too, the repaired ones as G2.

    | File | Kept paragraphs | Store as JSON | Read and write |
    | --- | --- | --- | --- |
    | Big Fish (435 KB) | 361 of 2,770 | 211 KB | 52 ms (debug build) |
    | Brick & Steel (55 KB) | 20 of 83 | 61 KB | 5 ms |
    | The Last Birthday Card (109 KB) | 111 of 486 | 100 KB | 12 ms |
    | rsdoiel samples 1–6 (25–43 KB) | 1 to 4 each | 27–50 KB | 2–3 ms |

    The store is larger than planned. Scene headings with `SceneProperties` and runs with font attributes are kept whole, and settings sections like `Macros` are kept as text. JSON escaping roughly doubles it. A journal header for Big Fish therefore grows by about half. It is written once per save, as the header always was; header sizes are measured again in step 13.
13. **Values the model rejects stay in the file** (`fdx.md` §9). `invalid_value`, `invalid_format` and `invalid_scene_number` became `info`: the model uses its default, and the file keeps its text. Only XML and encoding repairs are `fixed` or `dropped`. This keeps our stricter validation from overwriting a producer's settings.
14. **Two new report codes**: `skipped_bytes` (a syntax error the reader skipped past, or text outside the root) and `invalid_char` (control characters XML does not allow, in text or attribute values). Both are `dropped`. The fuzz checks found both cases, along with a leftover second byte-order mark that the parser skipped without counting it in its positions.
15. **Dual dialogue needs a line on each side** (`fdx.md` §6.6). A speaker with no lines is no longer flagged `dual`, matching layout, which never paired one. Nothing prints differently, and the writer can always write back what the reader flags.
16. **What an edited paragraph carries** (`fdx.md` §14.4): only between paragraphs of the same shape (a dual-dialogue wrapper's attributes never move onto a speech), and inline layout attributes only when the `Type` is unchanged. A dual block's speeches carry like ordinary paragraphs when the block still has the same shape.
17. **Spacing outside the page body** is a loss (`spacing_out_of_range`), not written for the reader to drop.
18. **The fuzz workflow** gains `fdx_roundtrip` and `fdx_write` (eight targets). libFuzzer does not build on this Windows machine, and the targets call the same checks the property tests run on every build. A probe of 200,000 random multi-edit mutations of our fixtures found nothing after the fixes above.
19. **The .fdx format in core** (`format.rs`, `Fdx`). A document read from .fdx carries its page format (when not the default), title lines, spacing and store, with block ids `b_1` onwards, as the store numbers them. It is laid out unnormalized. The reference-gate layout and core's view give identical pages on all nine corpus files (`core/tests/fdx.rs`). Windows-1252 and lossy input map to our `Windows1252` style, so a save reports `reencoded` and writes UTF-8 with the declaration fixed.
20. **Open warnings for .fdx are the repairs alone** (plus `invalid_format`). What the file keeps for other apps is not a warning; the whole report is in `repaired.issues` and from `coldopen repair --json`. Otherwise every Final Draft file would open with banners about sections Cold Open keeps but does not use.
21. **A converting save leaves the document in the new format's form** (`format::adopt`, `commands.md` "Converting"). Saved as Fountain, it drops the store and page format, and its title lines become keys. Saved as .fdx, its keys become title lines. This happens before the journal restarts, so a crash right after a conversion recovers nothing, and later saves don't ask again about what the conversion already reported. None of these parts is touched by an undoable command.
22. **Fountain keys become .fdx title lines** through a new `coldopen_layout::title_lines`, which places them on the rows layout prints them on (a test checks the two lay out the same). Stacked paragraphs cannot share a row, so with both bottom blocks the right one goes just above the left (`fdx.md` §14.3).
23. **Damaged originals** are copied with a new `io::write_new` (create-new, fsync) before the first save in place, for both `doc.save` and the background save. The save's result carries an `original_kept` warning.
24. **New commands**: `export.fdx`, `export.fountain` (class output) and `file.repair` (a new class, `file`). Each has an MCP tool and a palette title. The CLI gets `coldopen repair` (text report, `--json`, `--report`), and `convert` reads and writes .fdx.
25. **Hidden files on Windows** use the existing hand-declared kernel32 FFI in `io::win` (`GetFileAttributesW`/`SetFileAttributesW`), so there is no new dependency. Journals are hidden whenever they are opened, which includes those from earlier phases; copies and damaged originals are hidden when written. Hiding is best effort: a file that cannot be hidden still works. A test checks the attribute on Windows.
26. **Copies are never written over**: `copy_aside` now creates a new file (it used to overwrite), and a copy made in the same millisecond takes the next free one, so its name still gives the time.
27. **`unsaved` is a journal header field**, not a document field as first written in `journal.md`. It describes the journal (a restored script with no file), and the first header written for a saved file clears it.
28. **`recovered.list` and `recovered.delete` are in the new `file` class**: they need no open document. `recovered.restore` is a session command. Restore and delete are writer-only in core and refused over the agent link. A copy is known only by its name pattern, so `recovered.delete` can never delete anything else.
29. **Set-aside warnings carry `details.copy`** (`journal_discarded`, `journal_unreadable`, `journal_partial`, `original_kept`), so the app can say "set aside" without showing a path. The messages keep the path for the CLI and JSON clients.
30. **The router lands in two commits.** The first moves keys, buttons, the palette, the params form and agent requests onto `App::dispatch` with no test changed. The second adds the new session commands and the waiting.
31. **Agents get core's `save_in_progress`**: the router adds no check of its own for an agent's `doc.save` during the writer's save, because core already refuses a second save.
32. **Revert waits for a save in flight too**, as open, close, quit and restore do. Otherwise it read the file from before the save. Don't save during a save sets the newer changes aside at once, and the step runs when the save is written, whatever its outcome.
33. **Where a restored copy goes**: Recovered versions puts it in `Unsaved/` as "<script> (restored).<ext>", numbered when taken, so it is untitled and Save as names it. A `path` given in the params form is used as given, as the command defines it. (Revised in step 11; step 10 ignored the form's path.)
34. **Delete asks with Enter as Delete**, since the writer already chose Delete to get there; D also deletes and Esc or C cancels. S does nothing there, unlike the other questions.
35. **Banners are a small type** with an optional Recovered versions button. It reads as text, so existing code and tests that look at banners did not change.
36. **Repair issues are not banners**: on open, warnings of class fixed or dropped go to the repair dialog only. Other warnings are still banners, and a set-aside copy is announced only by the path-free notice.
37. **Save a copy** writes the other format through `export.fdx` or `export.fountain`. What the format can't hold is asked about with the same question as Save as, worded for the target format ("Fountain can't hold…", "An .fdx file can't hold…").
38. **Recovered versions is a wide dialog**, not a separate window or a page: a list, a read-only preview of up to 400 paragraphs, and Restore, Delete and Close. Opening it marks every listed copy as shown, in `recovered.json`.
39. **The clean-up runs at start-up before anything opens** and looks only where the list looks (next to the recent scripts, and `Unsaved/`). It deletes through `recovered.delete`, so only names of set-aside copies can go.
40. **Closing a script no longer shows "no document is open"**: the navigator asked core for scenes after the script closed. Found by the new Don't save test; it predates phase 4.
41. **Restored scripts are leftover work**: an untitled script whose journal header says unsaved is offered at the next start even with no entries in its journal.
42. **The crash gate's Save as run accepts either file until the Save as is acknowledged.** A kill can land after the conversion wrote the .fdx file and moved the journal but before the acknowledgement, so the work is then in the .fdx file. After the acknowledgement, the .fdx file must give it back.
43. **The .fdx typing gate saves a new script as .fdx before the first key**, so every key goes through the .fdx layout. The saved file reads back as the same blocks, and written as Fountain it equals the phase 3 gate's file.
44. **An agent can't accept losses on export**: `export_fountain` of the .fdx gate script reports `lossy_format`, and `allow_lossy` from an agent is `writer_only`, as for `doc.save`.
45. **The markup style is read properly** (2026-10-17, found by the new corpus files): the reader looked past the whitespace before the first paragraph and so always kept the default style (LF, two-space indentation). It now reads the newline from that whitespace, takes the indent unit from the paragraph's `Text`, and keeps any extra indentation below the root as an offset (jumpcut indents `Content` by four). Files were byte-identical before and after; what changed is how much is kept as raw text: jumpcut's Big Fish journal header went from 1.1 MB to 304 KB, and its open from 43 ms to 36 ms.
