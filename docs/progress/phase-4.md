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
| 5 | .fdx reader: a lossless XML layer, the passthrough store, resyncing after syntax errors, report classes; our own fixtures, broken ones included | |
| 6 | .fdx writer: write back what was read byte for byte, generate what was edited; property tests, the hostile-input timing test, fuzz targets for the writer | |
| 7 | The round-trip gate on the corpus, with its report | |
| 8 | Core: .fdx open, save and conversion, `export.fdx`, `export.fountain`, `file.repair`, damaged originals; the CLI's `repair` and `convert`; MCP tools | |
| 9 | Recovery copies: hidden on Windows, `recovered.*` commands, clean-up | |
| 10 | App: one command router, `App::dispatch(request, origin)` (F18.5) | |
| 11 | App: .fdx dialogs, Save As conversion, exports, the repair dialog, the Recovered versions screen, notices without paths | |
| 12 | Gates: crash gate (kills during .fdx saves and Save As conversion), typing gate on .fdx, agent gate; CI on three OSes; measurements; your manual test of the Windows build; the final report | |

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

