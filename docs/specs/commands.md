# Command schema, version 1

- **Date:** 2026-10-13, updated 2026-10-15, 2026-10-16 and 2026-10-18
- **Status:** implemented in `crates/core` (phase 1; `export.pdf`, the `pages` export and scene pages added in phase 2, 2026-10-13; phase 3's keystroke editing, undo grouping, authors, `batch`, find, completion, history and `doc.revert`, 2026-10-15; phase 4's .fdx support, `export.fdx`, `export.fountain`, `file.repair` and the `recovered.*` commands, 2026-10-16). Phase 5a (2026-10-18) added bundles, tracked changes (the `tracked` param, `changes.list`, `changes.accept`, `changes.reject`, `changes.track`), `export.pdf`'s `changes`, and loss classes. Phase 5b (2026-10-22) adds revision sets (`revision.*`), the title page commands (`title.*`), the reports (`report.*`), `diff` and `changes.import`, and `export.pdf`'s `revision_pages`; they are being implemented.
- **Sources:** `docs/spec.md` (Command interface, File formats), `CLAUDE.md` (Commands), the phase 3 brief (2026-10-15), the reviews of phases 1–3 (2026-10-15, listed in `docs/progress/review-fixes.md`), the phase 4 brief and its decisions (2026-10-16, `docs/progress/phase-4.md`), the phase 5a brief and its decisions (2026-10-18, `docs/progress/phase-5a.md`), `docs/specs/fdx.md`, `docs/specs/bundle.md`, `docs/specs/changes.md`. No external material.

Every action in Cold Open is a command. The desktop app, the CLI, JSON over stdio and the MCP server all send the same commands to one dispatcher.

## Envelope

Requests and responses are single JSON objects. Over stdio they are one per line (NDJSON). A byte-order mark at the start of a line is ignored (2026-10-15: Windows PowerShell puts one before what it pipes).

```json
{"v": 1, "id": 7, "command": "block.insert", "params": {"after": "b_142", "type": "character", "text": "MAYA"}}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `v` | no, defaults to 1 | Envelope version. Any other value is answered with `unsupported_version`. |
| `id` | no, defaults to null | A number, string or null. It is echoed in the response. |
| `command` | yes | The dotted command name. |
| `params` | no, defaults to `{}` | An object. Unknown fields are rejected with `invalid_params`, so a typo is never silently ignored. |

Unknown top-level fields are rejected with `invalid_request`.

Responses echo `v` and `id`:

```json
{"v": 1, "id": 7, "ok": true, "result": {"id": "b_143"}}
{"v": 1, "id": 7, "ok": false, "error": {"code": "block_not_found", "message": "no block b_142", "details": {"id": "b_142"}}}
```

### How the schema evolves

New commands, new optional params and new result fields are added without changing `v`. Clients must ignore result fields they do not know. A breaking change needs `v: 2`. A command name never changes meaning, because crash journals replay commands by name. Result fields are never removed or renamed. Crash journal replay compares only the ids a result assigned and the step an undo or redo reversed (2026-10-16; `docs/specs/journal.md`, "What replay compares"), so new result fields never stop recovery.

## Ids

- **Block ids** are opaque strings, currently `b_<n>`.
- Within an open document they are never reused. A deleted id stays invalid, and undoing the delete brings the same id back.
- Opening a Fountain file numbers its blocks in order, because Fountain has no place to store ids.
- `doc.revert` reads the file into the same open document, so its blocks get fresh ids, continuing from the document's next id (2026-10-16). An id from before the revert then fails with `block_not_found` instead of naming another block, which matters to agents that remember ids between calls.
- **A scene's id** is the id of its scene-heading block. A scene runs from its heading up to the next scene heading or section.

## Blocks in JSON

A block is its id plus the element fields from the screenplay model:

```json
{"id": "b_12", "type": "scene_heading", "text": "INT. LAB - NIGHT", "scene_number": "4"}
```

- **`type`** is one of `scene_heading`, `action`, `character`, `parenthetical`, `dialogue`, `transition`, `lyric`, `shot`, `general`, `page_break`, `section`, `synopsis`, `note` or `boneyard`.
- **`text`** is always the printed text as a plain string, with `\n` for line breaks. With tracked changes it is the **current text**: pending deletions are left out (2026-10-18, `docs/specs/changes.md` §2.1).
- **`runs`** appears only when the text has styles, hidden runs or tracked-change marks. It is a list of `{"text", "bold"?, "italic"?, "underline"?, "ins"?, "del"?}`, `{"note"}` or `{"boneyard"}`. Inputs may give `text` (a literal plain string) or `runs`. `ins` and `del` name a pending change (2026-10-18). Marks are only ever made by tracked edits, so in input a run with `del` is left out and an `ins` is ignored: runs read from `doc.get` can be sent back as they are, and stand for the current text.
- **`change`** (2026-10-18) appears only on a block with pending structural marks (`inserted`, `deleted`, `retyped`, `split`, `joined`, `moved`; `docs/specs/changes.md` §2.2). It is read-only: commands that take a block's fields do not accept it.
- **Revision marks** (2026-10-22). A text run may also carry `"rev": "r_2"`, and a block `"rev": "r_2"` beside `change` (`docs/specs/revisions.md` §4). Both are read-only, like `change`: a run's `rev` in input is ignored, as `ins` is, and commands that take a block's fields do not accept the block's.
- **Optional attributes** appear only when set:

  | Attribute | Applies to | Value |
  | --- | --- | --- |
  | `scene_number` | scene headings | string |
  | `dual` | characters | bool |
  | `centered` | action | bool |
  | `depth` | sections | integer, 1 or more |
  | `space_before` | printing blocks | blank lines above, when not the usual |

## Positions in text

Commands that work inside a block's text (added 2026-10-15) take **offsets**:

- An offset counts Unicode scalar values (characters) in the block's printed text, the `text` string of its JSON form. It runs from 0 to the text's length.
- Counting characters, not bytes or UTF-16 units, gives the same numbers in every language a client is written in, except for characters outside the Basic Multilingual Plane, which JavaScript counts twice.
- Hidden runs (notes and boneyard) take no offsets. An offset names the point just after the printed character before it, so hidden runs that sit there belong to the text that follows.
- Pending deletions (2026-10-18) take no offsets either, and are treated like hidden runs for placing text (`docs/specs/changes.md` §2.1).
- An offset past the end, or a `from` greater than `to`, fails with `invalid_range`.

**Resolution:** text inserted at an offset goes immediately after the printed character before it, ahead of any hidden runs there. A note written right after a word therefore stays after the word while the writer keeps typing it.

## Authors and undo steps

Every applied edit, `undo` and `redo` has an **author** (added 2026-10-15):

| `by` | Who |
| --- | --- |
| `{"kind": "writer", "name"?: "..."}` | A person: the desktop app, the CLI, a `serve --stdio` client. This is the default. `name` (2026-10-18) is the writer's name for tracked changes: the app's setting (`docs/specs/editor.md` §11), or `--author NAME` for `coldopen run` and `coldopen serve --stdio`. Without one, changes show "Writer". |
| `{"kind": "agent", "name": "...", "tool": "..."}` | An agent through MCP. `name` is the client's name from the MCP handshake; `tool` is the MCP tool it called (`docs/specs/mcp.md`). |

The JSON envelope has no way to claim an author: a stdio client is always a writer, and agent authors are set only by the MCP server and the app's agent link. Writers with different names are different authors for tracked changes (`docs/specs/changes.md` §5) and the same author for everything else.

Each undo step has a **label** for menus and history:

- For a writer, a short description of the command, such as "Typing", "Delete", "Insert block", "Change element", "Split block", "Join blocks", "Replace", "Move scene", "Style", or the `label` given to `batch`.
- For an agent, `<name> · <tool>`, such as `claude-code · block_insert`.

**Grouping.** Typing makes one undo step per burst, not per keystroke. A `text.insert` joins the step on top of the undo stack when all of these hold:

1. That step holds only `text.insert` commands by the same writer, in the same block.
2. The new text starts exactly where the step's last insert ended.
3. No other command has been applied, undone or redone in between, and the document has not been saved in between (2026-10-16).
4. Less than 1.5 s passed since that last insert, by the dispatcher's monotonic clock.

A `text.delete` joins a step of `text.delete` commands by the same writer in the same block in the same way, when it continues a backspace run (its `to` is the last delete's `from`) or a forward-delete run (its `from` is the last delete's `from`).

Agent commands never group: each MCP tool call is its own step, and an agent groups several changes with `batch`.

A grouped command is still journaled on its own, with `"merge": true`, so recovery rebuilds exactly the same steps without looking at the clock (`docs/specs/journal.md`).

## Commands

Commands fall into these classes:

| Class | Undo | Journal |
| --- | --- | --- |
| **edit** | Undoable | Journaled before the result is returned |
| **history** (`undo`, `redo`) | — | Journaled |
| **query** | — | Not journaled |
| **output** | — | Not journaled; writes one file from the document without changing it |
| **session** | — | See `docs/specs/journal.md` |
| **file** (2026-10-16) | — | Not journaled; works on files other than the open document, and needs no open document |

An edit that changes nothing, such as `text.replace` with no matches, succeeds without being journaled or added to the undo history.

### Session commands

| Command | Params | Result |
| --- | --- | --- |
| `doc.new` | `path?`, `start?` | `{path, warnings}` |
| `doc.open` | `path`, `recover?` | `{path, format, blocks, scenes, warnings, recovered}` |
| `doc.save` | `path?`, `format?`, `allow_lossy?`, `overwrite?` | `{path, format, bytes, losses}` |
| `doc.close` | `discard?` | `{warnings}` |
| `doc.revert` | — | `{path, blocks, scenes, warnings}` |
| `recovered.restore` | `copy`, `path` | `{path, format, blocks, scenes, warnings}` |

Notes on each command:

- **`doc.new`**
  - Starts an empty document.
  - With `path`, the document is journaled next to that path. The file must not already exist (`already_exists`).
  - Nor may a crash journal for that path hold anything (2026-10-16): a document made with `doc.new` and never saved exists only in its journal, so `doc.new` fails with `already_exists`, with `details.journal` naming the journal, and leaves it untouched. `doc.open` recovers it, or sets it aside with `recover: "discard"`.
  - Every check runs before the open document is closed (2026-10-16), so a refused `doc.new` leaves it open.
  - Without a path, the document is held in memory only, and a warning says so.
  - `start` (2026-10-15) is an element type, such as `"scene_heading"`. The document then begins with one empty block of that type, so an editor has a line to type on. The block is part of the new document, not an edit: the document starts clean, with nothing to undo and an empty journal.
- **`doc.open`**
  - Reads a `.fountain`, `.spmd` or `.txt` file as Fountain, (2026-10-16) an `.fdx` file (`docs/specs/fdx.md`), or (2026-10-18) a `.coldopen` bundle (`docs/specs/bundle.md`). The extension decides, ignoring case. `format` in the result says which: `"fountain"`, `"fdx"` or `"coldopen"`. A bundle's blocks keep the ids it was saved with.
  - `repaired` (2026-10-16) is null, or, for an .fdx file the reader had to repair (`docs/specs/fdx.md` §9), `{fixed, dropped, issues}`: how many issues of each class, and the report's issues. Repair is automatic; the open goes ahead.
  - `recover` is `"auto"` (the default), `"discard"` or `"apply"`. If a crash journal exists, `auto` recovers it and `discard` sets it aside unread; see the journal spec. A journal written on another computer or for another path (synced or copied with the script) is only offered: `auto` fails with `journal_foreign` and changes nothing, and `apply` recovers it anyway (2026-10-16).
  - `recovered` is null, or `{commands, external_change, last}`. `last` (added 2026-10-15) is when the newest recovered change was made, in milliseconds since the Unix epoch, for the app's recovery dialog.
- **`doc.save`**
  - Saves atomically.
  - `path` is "save as". A path where another file already is fails with `already_exists` unless `overwrite` is true (2026-10-16): "save as" never replaces a file by accident, and every `.txt` counts as a script. The target's lock is taken, and its crash journal judged, before anything is written (2026-10-16; `docs/specs/journal.md`, "Saving"): a target open in another process fails with `doc_locked`, and one whose journal holds unsaved work fails with `unsaved_changes`. A refused save writes nothing.
  - `format` is `"fountain"`, (2026-10-16) `"fdx"` or (2026-10-18) `"coldopen"`. It is inferred from the path's extension when omitted. A save in place writes the format the document was opened in or last saved as, which the session remembers, and `doc.open` reports it as `format`. An .fdx file therefore saves back to .fdx. Saving as a bundle loses nothing, so it is the way to keep tracked changes, ids and everything else a script holds.
  - **Converting** (2026-10-16). "Save as" to a path of another format converts: the document is written in the target's format, and from then on the session is in that format. What the target format cannot hold is listed as losses before anything is written, as below. Once written, the document takes the form the new format holds, so later saves do not ask again: saved as Fountain, it drops the .fdx store and page format and its title lines become title-page keys (`docs/specs/fountain.md` §11); saved as .fdx, its title-page keys become title lines (`docs/specs/fdx.md` §14.3). None of these is changed by an undoable command, so undo is unaffected. An explicit `format` that disagrees with a path whose extension names another format fails with `unsupported_format`.
  - **A repaired .fdx file** (2026-10-16). The first save in place of a document whose `doc.open` reported `repaired` first keeps a copy of the damaged file as it was (`docs/specs/journal.md`, "Damaged originals"), and an `original_kept` warning in the result names it. If the copy cannot be made, the save fails with `io_error` and writes nothing.
  - If the format cannot hold something in the document (`docs/specs/fountain.md` §9, `docs/specs/fdx.md` §15), the save fails with `lossy_format` and lists the losses, unless `allow_lossy` is true. A file read as Windows-1252 and about to be written as UTF-8 counts as a loss too (`reencoded`, 2026-10-16; `docs/specs/journal.md`, "Encoding"). Only a writer may pass `allow_lossy` (2026-10-16); from an agent it fails with `writer_only`. Only losses of class `lost` refuse a save ("Losses" below, 2026-10-18).
  - **Saving pending changes away** (2026-10-21, decided with the project's owner after the phase 5a manual test). A save with `allow_lossy` to Fountain or .fdx of a document with pending tracked changes (`tracked_changes`) leaves the document in the form the file holds, as converting does above. The steps run in this order:
    1. The version with its changes marked is set aside as a Recovered version: the journal is copied to `<journal>.marked-<ms>` (`docs/specs/journal.md`, "Recovered versions").
    2. Every pending change is accepted, as `changes.accept` with `all` does: one undoable step by the writer, journaled like any other.
    3. The save goes ahead with the accepted document, so its result lists no `tracked_changes` loss.

    A `changes_accepted` warning says so: `details` gives `count`, the changes accepted, and `copy`, the copy's path (left out when the session keeps no journal). Undo brings the marks back, unsaved. Should the save then fail, as with `file_changed`, the acceptance stays; undo takes it back. A save as a bundle keeps every mark, so it accepts nothing.
  - **Saving revision marks away** (2026-10-22). The same holds for revision data (`revision_marks`, `docs/specs/revisions.md` §9). After step 2, the revision table and every revision mark are reset to the default, as one more undoable step by the writer ("Clear revisions"). A `revisions_cleared` warning gives `sets` and `blocks`, and the copy of step 1, which is made once for both.
  - Saving in place over a file that another program changed since this session last read or wrote it fails with `file_changed` (2026-10-16; `docs/specs/journal.md`, "Saving"). `overwrite: true` replaces it anyway. Only a writer may pass `overwrite`; from an agent it fails with `writer_only`.
  - While a save off the UI thread is being written, `doc.save` fails with `save_in_progress` (2026-10-16; `docs/specs/journal.md`, "One save at a time"). Try again when it has finished.
- **`doc.close`**
  - Refuses with `unsaved_changes` unless `discard` is true.
  - With `discard`, unsaved changes are kept in a copy of the journal, which a `journal_discarded` warning names (2026-10-16; `docs/specs/journal.md`, "Closing").
  - An agent's `discard` fails with `not_your_step` when the document holds a writer's recovered work (2026-10-16; `docs/specs/mcp.md` §6). The same holds for an agent's `doc.revert`, and for its `doc.open` with `recover: "discard"`.
- **`doc.revert`** (2026-10-15)
  - Throws away unsaved changes and the undo history, and reads the document's file again. The blocks read get fresh ids ("Ids" above).
  - The journal is never deleted by a revert: if the document has unsaved changes or the journal holds entries, it is copied aside to `<journal>.discarded-<ms>` first, then started again from the file. (2026-10-16: "unsaved changes" covers a recovered journal header with no entries, `docs/specs/journal.md` recovery step 4.)
  - Fails with `no_path` for an untitled document, and with `io_error` if the file cannot be read; the open document is unchanged in both cases.
  - The desktop app's recovery dialog uses it for Discard.

- **`recovered.restore`** (2026-10-16)
  - Opens the document a set-aside copy holds (`recovered.list`) as a new, unsaved script, journaled at `path` like `doc.new`, which must be a path with no file and no journal (`already_exists` otherwise). The copy, the script it came from and that script's journal are never touched.
  - The new journal's header is marked as holding unsaved work, so a crash before the first save recovers it and the app offers it at start-up (`docs/specs/journal.md`, "Recovered versions").
  - Writer-only: an agent gets `writer_only`, and over the app's agent link `not_allowed_attached`.
  - A copy that cannot be read fails with `copy_unreadable`.

Opening or creating a document while another is open first closes the open one. If that one has unsaved changes, the request fails with `unsaved_changes`.

`doc.open` of a file that does not exist fails with `io_error`, unless a crash journal for that path exists. Then the document is recovered from the journal alone (`docs/specs/journal.md`, "Recovery on open"). This is how a document created with `doc.new` and never saved survives a crash.

### Queries

| Command | Params | Result |
| --- | --- | --- |
| `doc.get` | — | `{path, dirty, rev, title_page, blocks}` |
| `doc.export` | `format?` | `{format, text, losses}` |
| `scene.list` | — | `{scenes: [{id, index, number, heading, synopsis, blocks, words, page, eighths}]}` |
| `text.find` | `find`, `scope?`, `match_case?` | `{matches: [{id, from, to}]}` |
| `text.complete` | `id`, `at` | `{context, from, to, items: [{text, count}]}` |
| `history.list` | — | `{undo: [{command, label, by}], redo: [...]}` |
| `changes.list` | `scene?` | `{changes: [{id, by, t, kinds, blocks, scene, inserted, deleted}]}` |
| `recovered.list` | `path?` | `{copies: [{copy, script, kind, at, unread, readable}]}` |
| `revision.list` | — | `{active, colors, sets: [{id, color, label, date, started, ended, blocks, pages}]}` (2026-10-22) |
| `title.get` | — | `{form, keys, lines}` (2026-10-22) |
| `report.scenes` | — | `{scenes: [...]}` (2026-10-22, `docs/specs/reports.md` §4.1) |
| `report.characters` | — | `{characters: [...]}` (2026-10-22, §4.2 there) |
| `report.locations` | — | `{locations: [...]}` (2026-10-22, §4.3 there) |

`rev` (2026-10-15) counts the edits, undos and redos applied since the document was opened, created or reverted. It never goes down while the document stays open, so a client can tell whether anything changed since it last looked.

- **`text.find`** (2026-10-15)
  - Matches exactly as `text.replace` does: in printed text, never in notes, boneyard or the title page, never across blocks, non-overlapping, from the start.
  - Matches are listed in document order. `from` and `to` are offsets in the block's text.
  - `find` must not be empty.
- **`text.complete`** (2026-10-15)
  - Suggests how to finish the text before offset `at` in a block, as the desktop editor's autocomplete does. `docs/specs/editor.md` §6 defines the contexts (`character`, `extension`, `scene_prefix`, `location`, `time`, `transition`), their sources and their ranking.
  - Choosing an item means replacing the text from `from` to `to` with the item's `text`.
  - Where nothing applies, `context` is null and `items` is empty.
- **`history.list`** (2026-10-15)
  - Lists the undo and redo stacks, most recent step first: each step's command name (`batch` or `typing` for a group), label and author.

`doc.export` returns the document as text without saving it. `format` is `"fountain"` (the default), `"fdx"` (2026-10-16; UTF-8, LF), `"json"` (the title page and blocks, pretty-printed) or `"pages"` (the plain-text page view of `docs/specs/layout.md` §11). Losses are listed as for `doc.save`; nothing is refused, since nothing is written. A bundle is binary and is not a `doc.export` format (`unsupported_format`); `json` gives the same blocks, marks included.

### Losses

Added 2026-10-18 (phase 5a). Every save, export and `doc.export` lists what the target format did with the document, as `{id, code, class, message}`:

| `class` | Meaning | Refuses a save without `allow_lossy` |
| --- | --- | --- |
| `lost` | The format cannot hold it, so it is changed or left out: a Shot in Fountain, a note in .fdx, pending tracked changes in either (`tracked_changes`, `docs/specs/changes.md` §9), and (2026-10-22) revision sets and marks in either (`revision_marks`, `docs/specs/revisions.md` §9). | yes |
| `normalized` | Written in the format's normal form, which prints the same and reads back differently (`docs/specs/fountain.md` §10, finding F21): a trailing space, two dialogue blocks written as one. | no |

Clients that do not know `class` can treat every loss as before. A `normalized` loss is never asked about in the desktop app.

Pages come from laying out the document in its format's form with its page format: for Fountain the normal form with the default page format (US Letter, feature film; `docs/specs/layout.md` §3.2); for .fdx (2026-10-16) the blocks as they are, with the file's page format, title lines and paragraph spacing. Either way they are the same after a save and reload.

- **`recovered.list`** (2026-10-16)
  - Lists the set-aside copies (`docs/specs/journal.md`, "Recovered versions") of the script at `path`, or of the open document without it, newest first.
  - `copy` is the copy's path; `script` the script it belongs to; `kind` is `discarded`, `corrupt`, `diverged`, `original` (a damaged .fdx file kept before a repaired save) or (2026-10-21) `marked` (the version with its tracked changes marked, kept before a save accepted them, `doc.save`); `at` is when it was made, in milliseconds since the Unix epoch, from its name.
  - `readable` says whether the copy can be restored. `unread` is reserved for front ends that track which copies a writer has been shown, and is false here.

- **`changes.list`** (2026-10-18) lists the pending tracked changes, in document order, as `docs/specs/changes.md` §8 describes. With `scene`, only those with a mark in that scene.

For `scene.list`:

- `index` counts from 1.
- `number` and `synopsis` may be null.
- `synopsis` is the first synopsis inside the scene.
- `blocks` counts the scene's blocks, including its heading.
- `words` counts words in its printing blocks.
- `page` is the body page the scene's heading prints on, counting from 1 after the title page; null if nothing prints.
- `eighths` is the scene's length in eighths of a page (`docs/specs/layout.md` §10); 9 means one and one-eighth pages.

### Edits

| Command | Params | Result |
| --- | --- | --- |
| `block.insert` | `after?` or `before?`, `type`, `text` or `runs`, attributes | `{id}` |
| `block.set_type` | `id`, `type` | `{}` |
| `block.set_text` | `id`, `text` or `runs` | `{}` |
| `block.set_attrs` | `id`, `scene_number?`, `dual?`, `centered?`, `depth?`, `space_before?` | `{}` |
| `block.delete` | `id` | `{}` |
| `text.replace` | `find`, `with`, `scope?` or `at?`, `match_case?` | `{count, blocks}` |
| `scene.move` | `id`, `before?` | `{}` |
| `text.insert` | `id`, `at`, `text`, `style?`, `after_deleted?` | `{}` |
| `text.delete` | `id`, `from`, `to` | `{}` |
| `text.style` | `id`, `from`, `to`, `bold?`, `italic?`, `underline?` | `{}` |
| `block.split` | `id`, `at`, `type?`, `after_deleted?` | `{id}` |
| `block.join` | `id` | `{id, at}` |
| `batch` | `commands`, `label?` | `{results}` |
| `changes.accept` | `id`, `scene` or `all` | `{changes, count, skipped}` |
| `changes.reject` | `id`, `scene` or `all` | `{changes, count, skipped}` |
| `changes.track` | `writer?`, `agents?` | `{writer, agents}` |
| `changes.import` | `path`, `name?` | `{change, edits, skipped}` (2026-10-22) |
| `revision.start` | `color?`, `label?`, `date?` | `{set}` (2026-10-22) |
| `revision.end` | — | `{set}` (2026-10-22) |
| `revision.colors` | `colors` | `{colors}` (2026-10-22) |
| `revision.clear` | `set` or `all` | `{count}` (2026-10-22) |
| `title.set` | `key`, `values`, `at?` | `{}` (2026-10-22) |
| `title.set_lines` | `lines` | `{}` (2026-10-22) |

**Tracked edits** (2026-10-18). `block.insert`, `block.set_type`, `block.set_text`, `block.delete`, `block.split`, `block.join`, `text.insert`, `text.delete`, `text.replace` and `scene.move` also take `tracked?`, a boolean. When it is absent, the document's tracking switches decide, for the command's author; the value used is journaled. A tracked edit records a change and names it in its result as `change`: `{"id": "b_9", "change": "c_4"}`, `{"change": "c_4"}` where the result is otherwise `{}`, and in a `batch`'s result and in each of its `results`. What each edit does when tracked, and which edits are refused (`change_pending`, `not_your_change`), is in `docs/specs/changes.md` §5. An agent's `tracked: false` while its edits are tracked fails with `writer_only`.

Notes on each command:

- **`block.insert`**
  - With neither `after` nor `before`, the block is appended at the end.
  - The new block must satisfy the model rules, otherwise the request fails with `invalid_attr`.
- **`block.set_type`**
  - Clears attributes that do not apply to the new type. Undo restores them.
- **`block.set_attrs`**
  - Changes only the attributes given.
  - `null` clears `scene_number` or `space_before`.
- **`text.replace`**
  - Searches the printed text of every block except notes and boneyard. The title page and hidden runs are skipped.
  - `scope` is `{"scene": id}`; without it, the whole document is searched.
  - `match_case` defaults to true.
  - Matches never cross blocks.
  - The replacement takes the style of the first matched character, and hidden runs inside a match move to its end.
  - All replacements form one undo step.
  - The result lists the ids of the changed blocks.
  - `at` (2026-10-15) is `{"id", "from"}`: only the match starting at offset `from` in that block is replaced. If no match starts there, nothing changes and `count` is 0. `at` and `scope` together fail with `invalid_params`.
- **`scene.move`**
  - Moves a scene (its heading and body) before the scene or section given in `before`, or to the end when `before` is omitted.

The keystroke commands below (2026-10-15) are what the desktop editor sends for every key. Any client may use them. Offsets follow "Positions in text" above.

- **`text.insert`**
  - Inserts `text`, which may hold `\n`, at offset `at`.
  - The new text takes `style` when it is given, as `{"bold"?, "italic"?, "underline"?}`. Otherwise it takes the style of the printed character before `at`, or the one at `at` when `at` is 0, or plain style in an empty block.
  - Notes, boneyard and other raw-text blocks take plain text only: a `style` there fails with `invalid_attr`.
  - Fails with `invalid_attr` on a page break, which has no text.
  - Empty `text` changes nothing.
  - Text inserted at an offset where a pending deletion sits goes before the deletion (`docs/specs/changes.md` §2.1). With `after_deleted: true` (2026-10-21) it goes after it, and after any note among the struck text: the desktop app sends this when the caret is drawn after the struck text (`docs/specs/editor.md` §4). Where no deletion sits at `at`, it changes nothing. Absent means false, so journals from before replay as they did.
- **`text.delete`**
  - Deletes the printed characters from `from` up to `to`.
  - Hidden runs inside the range are kept and move to `from`, so a backspace never deletes a note.
  - `from == to` changes nothing.
- **`text.style`**
  - Sets (true) or clears (false) each style given, on the characters from `from` up to `to`. Styles not given are left alone.
  - Fails with `invalid_attr` on raw-text blocks.
- **`block.split`**
  - Splits a block at offset `at`. The block keeps the text before `at` and all its attributes.
  - A new block after it gets the rest of the text and no attributes. Its type is `type`, or the block's own type when `type` is omitted.
  - Hidden runs at the split point go to the new block with the text after them. **Resolution:** at the very end of the block they stay where they are, so a note at the end of a speech stays with the speech when Enter starts the next element.
  - A pending deletion at the split point goes to the new block too, as hidden runs do. With `after_deleted: true` (2026-10-21) it stays in the block, with any note among it, as Enter after struck text in the desktop app asks.
  - Fails with `invalid_attr` when either part would break a model rule, such as text left in a page break.
  - The result is the new block's id. Splitting at the end of a block makes an empty new block, which is how the editor starts the next element.
- **`block.join`**
  - Appends the block's text, hidden runs included, to the block before it, then removes the block.
  - The earlier block keeps its type and attributes.
  - The result gives the earlier block's id, and `at`: the offset in it where the joined text starts.
  - Fails with `invalid_params` for the first block, and with `invalid_attr` when the result would break a model rule, such as styled text joined into a note.
- **`batch`**
  - Runs a list of `{"command", "params"}` edits in order, as one change.
  - Only edits may appear: queries, session, output and history commands, and nested batches, fail with `invalid_params` before anything runs.
  - If any edit fails, none of them is kept. The error is that edit's, with `details.index` naming its position.
  - On success it is one undo step, labelled with `label` (or "Batch"), and one journal entry. The result lists each edit's result in order.
  - An empty list changes nothing.
  - (2026-10-18) Its tracked edits all record into one change. A `tracked` given to the batch itself is not a param; each edit has its own.

The track-changes commands (2026-10-18; `docs/specs/changes.md` §6 and §8):

- **`changes.accept`, `changes.reject`**
  - Give exactly one of `id` (a change, `"c_4"`), `scene` (a scene's id: the marks inside that scene) or `all: true`; anything else fails with `invalid_params`. An unknown or settled change by `id` fails with `change_not_found`.
  - The result lists the changes settled (`changes`), how many (`count`), and moves that could not be undone because their place is gone (`skipped`, `docs/specs/changes.md` §6). A reject of such a move by `id` fails with `change_conflict`.
  - One undo step ("Accept change", "Reject changes" and so on), journaled. Nothing to settle changes nothing and is not journaled.
  - Only a writer may accept (`writer_only` for an agent). An agent may reject only its own changes (`not_your_change`); with `scene` or `all` it settles only its own.
- **`changes.track`**
  - Turns tracking of the writer's edits (`writer`) or of every agent's edits (`agents`) on or off for this document. Switches not given stay as they are. The result gives both as they now are.
  - An edit: one undo step ("Track changes"), journaled, and the document is unsaved after it. A bundle keeps the switches; Fountain and .fdx files open with the defaults (writer off, agents on).
  - Writer-only: an agent gets `writer_only`.
- **`changes.import`** (2026-10-22; `docs/specs/diff.md` §6)
  - Compares the open document's current text with the draft at `path`, and applies the differences as tracked edits by a writer named `name` (by default the draft's file name without its extension), as one undo step and one change.
  - Restyled blocks, changed attributes and the title page cannot be tracked, so they are listed in `skipped` and not applied.
  - Journaled with the edits it resolved (`edits`), so replay never reads the draft. A request that gives `edits` fails with `invalid_params`.
  - Writer-only: an agent gets `writer_only`.

The revision commands (2026-10-22; `docs/specs/revisions.md` §5):

- **`revision.start`** adds a set and makes it active. Without `color` it takes the next color in the document's list, without `label` "<color> Rev.", and without `date` today's date in UTC (`YYYY-MM-DD`). The values used are journaled. It fails with `revision_active` while a set is active (`details.active`), and with `invalid_params` for a date that is not a real calendar date from 1900 to 9999, or a color or label that breaks the naming rules.
- **`revision.end`** ends the active set: `revision_not_active` when there is none.
- **`revision.colors`** replaces the document's list of color names (1 to 64 names, each 1 to 32 characters, no line breaks, no two the same ignoring case).
- **`revision.clear`** removes the marks of one set (`set`) or of every set (`all: true`); the sets stay. An unknown set fails with `revision_not_found`.
- **While a set is active**, every edit that changes what prints marks what it changed, and accepting a tracked change marks what it did (`docs/specs/revisions.md` §6). The marks are part of the edit's undo step.
- All four are one undo step each and journaled. They are writer-only: an agent gets `writer_only`. `revision.list` is a query anyone may send.

The title page commands (2026-10-22, decided with the project's owner):

- **`title.set`** sets one Fountain title-page key: `key` (1 to 64 characters, as `docs/specs/fountain.md` §4 allows a key to be written), and `values`, each a plain string or a list of runs, as a block's text is given. Each value is one line of text with no space at either end, since Fountain trims values when it reads them (`docs/specs/fountain.md` §4); an empty value, or one with a line break or space at an end, fails with `invalid_params` (2026-10-25). An empty `values` removes the key. A key not yet there is added at `at` (a 0-based position) or at the end. Keys match ignoring case, so `title.set` with `"title"` changes `Title`.
- **`title.set_lines`** replaces the positioned title lines (`docs/specs/fdx.md` §7): each `{text or runs, align, left, right, space_before}`, with `align` `left`, `center` or `right`, `left` and `right` in points from the paper's left edge, forming a column of at least one cell on the page, and `space_before` in points from 0 to the page body's height (`invalid_attr` otherwise). An empty list removes the title page.
- **The form.** A document holds its title page as lines when it has any, and as keys otherwise (`title.get`'s `form`: `"keys"` or `"lines"`). `title.set` on a document with lines, or `title.set_lines` on one with keys, fails with `title_form`, naming the form it has. Converting between them is a Save as (`doc.save`).
- **Never tracked and never revision-marked.** Each is one undo step ("Title page"), journaled.
- **Agents.** While the document's `tracking.agents` switch is on, an agent's `title.set` or `title.set_lines` fails with `writer_only`, so every agent edit stays reviewable. With it off, an agent may send them like the writer. Enforced in core by author.

The reports (2026-10-22): `report.scenes`, `report.characters` and `report.locations` are queries with no params, defined in `docs/specs/reports.md` §4.


### Output

| Command | Params | Result |
| --- | --- | --- |
| `export.pdf` | `path`, `watermark?`, `scene_numbers?`, `title_page?`, `changes?`, `revision_pages?`, `overwrite?` | `{path, pages, bytes, warnings}` |
| `export.fdx` | `path`, `allow_lossy?`, `overwrite?` | `{path, format, bytes, losses}` |
| `export.fountain` | `path`, `allow_lossy?`, `overwrite?` | `{path, format, bytes, losses}` |

- **`export.pdf`**
  - Lays out the open document (as `scene.list` does) and writes it as a PDF, atomically, as `docs/specs/layout.md` §12 describes. The same document and options always give the same bytes.
  - `path` must end in `.pdf`, otherwise `unsupported_format`. It may never be the open document or its crash journal (`would_overwrite_document`). A file already there is replaced only with `overwrite: true`, otherwise `already_exists` (2026-10-16). Only a writer may pass `overwrite` (`writer_only`). The CLI's `export` subcommand passes it, since its output is named on the command line, as `convert` replaces its output.
  - `watermark` is text printed diagonally under every page. `scene_numbers` (default false) prints scene numbers beside headings. `title_page` (default true) includes the title page when the script has one.
  - `changes` (2026-10-18) is `"marked"` (the default) or `"accepted"`. Marked prints pending tracked changes as the editor shows them: deletions struck through, insertions in their author's color. Accepted prints the script as accepting every change would leave it (`docs/specs/changes.md` §9). A script with no pending changes prints the same either way.
  - `revision_pages` (2026-10-22) is `"white"` (the default, decided with the project's owner) or `"colored"`: each body page with a revision header filled with its set's paper color (`docs/specs/revisions.md` §7.3). Asterisks and revision headers print either way.
  - `pages` counts the PDF's pages, title page included. It is never 0: a document with nothing to print gives one blank page, with the watermark if one is given (2026-10-14).
  - `warnings` lists `{code, message, chars?, id?}`: characters the bundled font cannot print, in the script or the watermark (`missing_glyph`, `missing_mark`), and layout diagnostics such as `dual_too_tall`, with the block `id` when there is one.
  - It is not undoable, not journaled, and leaves `dirty` as it was. Journal replay never runs it.
- **`export.fdx`, `export.fountain`** (2026-10-16)
  - Write a copy of the open document in that format, atomically, without changing the session: its path, format, `dirty` and journal stay as they were ("Save a copy").
  - `path` must have that format's extension (`unsupported_format`), and may not be the open document or its crash journal (`would_overwrite_document`). A file already there is replaced only with `overwrite` (`already_exists`).
  - Losses refuse the export with `lossy_format` unless `allow_lossy` is given, as for `doc.save`. Only a writer may pass `overwrite` or `allow_lossy` (`writer_only`).
  - The copy is written in UTF-8 with LF line ends.

### Files

| Command | Params | Result |
| --- | --- | --- |
| `file.repair` | `path`, `out`, `overwrite?` | `{out, bytes, repaired, issues}` |
| `recovered.delete` | `copy` | `{}` |
| `diff` | `a?`, `b`, `pdf?`, `overwrite?` | `{v, a, b, compared, summary, title_page, scenes, text, pdf?}` (2026-10-22) |

- **`diff`** (2026-10-22; `docs/specs/diff.md`)
  - Compares two drafts, each a `.fountain`, `.fdx` or `.coldopen` path. Without `a`, the open document, as it stands, is the first draft.
  - The result is the JSON of `docs/specs/diff.md` §4.1, plus `text`, the readable summary.
  - With `pdf`, it also writes B with the changes marked, as `export.pdf` writes a file: it must end in `.pdf`, may not be either draft, the open document or its journal, and replaces a file only with `overwrite`, which only a writer may pass.
  - It needs no open document unless `a` is left out, and never changes the open one.

- **`file.repair`** (2026-10-16)
  - Reads the .fdx file at `path`, repairing what it can (`docs/specs/fdx.md` §17), and writes the result to `out`, atomically. `path` is never written.
  - `out` must be an `.fdx` path. It may not be `path` itself, however it is spelled (`would_overwrite_input`), nor the open document or its journal (`would_overwrite_document`). A file already at `out` is replaced only with `overwrite`, which only a writer may pass.
  - `repaired` and `issues` are the report (`docs/specs/fdx.md` §9). An input that is not XML, or not a Final Draft document, fails with `unsupported_format`.
  - It needs no open document and leaves the open one alone.
- **`recovered.delete`** (2026-10-16)
  - Deletes one set-aside copy. `copy` must be a path that `recovered.list` would list (`invalid_params` otherwise), so no other file can be deleted this way.
  - Writer-only, as for `recovered.restore`. It cannot be undone; the desktop app asks first.

### History

| Command | Params | Result |
| --- | --- | --- |
| `undo` | — | `{undone: "<command>", label}` |
| `redo` | — | `{redone: "<command>", label}` |

Undo is unlimited. A new edit clears the redo history.

- `undone` and `redone` name the step's command, `batch` for a batch and `typing` for a group of `text.insert` or `text.delete` commands. `label` was added 2026-10-15.
- History is one list for everyone: a writer's `undo` reverses the newest step, whoever made it.
- An agent may only undo or redo its own steps: when the step on top was made by anyone else, its `undo` or `redo` fails with `not_your_step` and nothing changes (2026-10-15).

## Error codes

| Code | Meaning |
| --- | --- |
| `invalid_request` | Not JSON, not an object, a missing `command`, an unknown top-level field, or a bad `id` type. |
| `unsupported_version` | `v` is not 1. |
| `unknown_command` | No command with that name. |
| `invalid_params` | Params do not match the command. |
| `no_document` | The command needs an open document. |
| `block_not_found` | No block with that id. |
| `scene_not_found` | The id is not a scene heading (or section, for `before`). |
| `invalid_attr` | The block would break a model rule; `details.problems` lists them. |
| `nothing_to_undo`, `nothing_to_redo` | The history is empty in that direction. |
| `unsaved_changes` | The document has unsaved changes; or a "save as" target has unsaved work in its crash journal (`details.path`). |
| `already_exists` | `doc.new` was given an existing file, the open document's path, or a path whose crash journal holds anything (`details.journal`); or "save as" or `export.pdf` was given an existing file without `overwrite`. |
| `no_path` | `doc.save` has no path to save to. |
| `unsupported_format` | The file type is not supported in this version. |
| `lossy_format` | The format cannot hold the document; `details.losses` lists why. |
| `doc_locked` | Another process has the document open. |
| `journal_foreign` | `doc.open` found unsaved work in a crash journal written on another computer or for another path; `details` gives `host` and `path`. Open with `recover` `"apply"` or `"discard"`. |
| `io_error` | Reading or writing a file failed. |
| `journal_error` | The crash journal could not be written; the edit was rolled back. |
| `would_overwrite_document` | An output command was asked to write over the open document or its journal. |
| `save_in_progress` | Another save of the document is being written; nothing changed. Retryable. |
| `file_changed` | Another program changed the file since the document last read or wrote it; nothing was saved. `overwrite` replaces it. |
| `writer_only` | An agent passed a choice only the writer may make (`overwrite`, `allow_lossy`, or since 2026-10-18 `tracked: false` while its edits are tracked), or sent a command only the writer may send (`recovered.restore`, `recovered.delete`, `changes.accept`, `changes.track`; since 2026-10-22 `changes.import`, `revision.start`, `revision.end`, `revision.colors`, `revision.clear`, and `title.set` or `title.set_lines` while its edits are tracked). |
| `would_overwrite_input` | `file.repair` was asked to write over the file it reads (2026-10-16). |
| `copy_unreadable` | `recovered.restore` was given a copy whose header cannot be read (2026-10-16). |
| `invalid_range` | An offset is past the end of the block's text, or `from` is after `to`. |
| `not_your_step` | An agent tried to undo or redo a step made by someone else, or to set aside a writer's recovered work (`details.recovered`). |
| `not_allowed_attached` | A command an agent may not send over the app's agent link, such as `doc.open` (`docs/specs/mcp.md`). |
| `change_not_found` | (2026-10-18) No pending change with that id. |
| `change_pending` | (2026-10-18) The block is a pending deletion; settle its change first (`details`: `id`, `change`). |
| `change_conflict` | (2026-10-18) A change cannot be carried out as it stands: a moved scene cannot be moved back, because the block it stood before is gone, or blocks it would join can no longer be joined. |
| `not_your_change` | (2026-10-18) An agent tried to reject another author's change, or an edit would remove or change another author's mark (`docs/specs/changes.md` §5.4, §6). |
| `revision_active` | (2026-10-22) `revision.start` while a set is active; `details.active` names it. |
| `revision_not_active` | (2026-10-22) `revision.end` with no active set. |
| `revision_not_found` | (2026-10-22) No revision set with that id. |
| `title_form` | (2026-10-22) `title.set` on a document whose title page is positioned lines, or `title.set_lines` on one whose title page is keys; `details.form` names the form it has. |
| `internal_error` | Something that should never happen, such as damaged bundled fonts. |
