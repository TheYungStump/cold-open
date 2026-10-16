# Command schema, version 1

- **Date:** 2026-10-13, updated 2026-10-15 and 2026-10-16
- **Status:** implemented in `crates/core` (phase 1; `export.pdf`, the `pages` export and scene pages added in phase 2, 2026-10-13). Phase 3 (2026-10-15) adds keystroke editing, undo grouping, authors, `batch`, find, completion, history and `doc.revert`; they are being implemented.
- **Sources:** `docs/spec.md` (Command interface), `CLAUDE.md` (Commands), the phase 3 brief (2026-10-15), the reviews of phases 1–3 (2026-10-15, listed in `docs/progress/review-fixes.md`). No external material.

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

New commands, new optional params and new result fields are added without changing `v`. Clients must ignore result fields they do not know. A breaking change needs `v: 2`. A command name never changes meaning, because crash journals replay commands by name.

## Ids

- **Block ids** are opaque strings, currently `b_<n>`.
- Within an open document they are never reused. A deleted id stays invalid, and undoing the delete brings the same id back.
- Opening a Fountain file numbers its blocks in order, because Fountain has no place to store ids.
- **A scene's id** is the id of its scene-heading block. A scene runs from its heading up to the next scene heading or section.

## Blocks in JSON

A block is its id plus the element fields from the screenplay model:

```json
{"id": "b_12", "type": "scene_heading", "text": "INT. LAB - NIGHT", "scene_number": "4"}
```

- **`type`** is one of `scene_heading`, `action`, `character`, `parenthetical`, `dialogue`, `transition`, `lyric`, `shot`, `general`, `page_break`, `section`, `synopsis`, `note` or `boneyard`.
- **`text`** is always the printed text as a plain string, with `\n` for line breaks.
- **`runs`** appears only when the text has styles or hidden runs. It is a list of `{"text", "bold"?, "italic"?, "underline"?}`, `{"note"}` or `{"boneyard"}`. Inputs may give `text` (a literal plain string) or `runs`.
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
- An offset past the end, or a `from` greater than `to`, fails with `invalid_range`.

**Resolution:** text inserted at an offset goes immediately after the printed character before it, ahead of any hidden runs there. A note written right after a word therefore stays after the word while the writer keeps typing it.

## Authors and undo steps

Every applied edit, `undo` and `redo` has an **author** (added 2026-10-15):

| `by` | Who |
| --- | --- |
| `{"kind": "writer"}` | A person: the desktop app, the CLI, a `serve --stdio` client. This is the default. |
| `{"kind": "agent", "name": "...", "tool": "..."}` | An agent through MCP. `name` is the client's name from the MCP handshake; `tool` is the MCP tool it called (`docs/specs/mcp.md`). |

The JSON envelope has no way to claim an author: a stdio client is always a writer, and agent authors are set only by the MCP server and the app's agent link.

Each undo step has a **label** for menus and history:

- For a writer, a short description of the command, such as "Typing", "Delete", "Insert block", "Change element", "Split block", "Join blocks", "Replace", "Move scene", "Style", or the `label` given to `batch`.
- For an agent, `<name> · <tool>`, such as `claude-code · block_insert`.

**Grouping.** Typing makes one undo step per burst, not per keystroke. A `text.insert` joins the step on top of the undo stack when all of these hold:

1. That step holds only `text.insert` commands by the same writer, in the same block.
2. The new text starts exactly where the step's last insert ended.
3. No other command has been applied, undone or redone in between.
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

An edit that changes nothing, such as `text.replace` with no matches, succeeds without being journaled or added to the undo history.

### Session commands

| Command | Params | Result |
| --- | --- | --- |
| `doc.new` | `path?`, `start?` | `{path, warnings}` |
| `doc.open` | `path`, `recover?` | `{path, format, blocks, scenes, warnings, recovered}` |
| `doc.save` | `path?`, `format?`, `allow_lossy?` | `{path, format, bytes, losses}` |
| `doc.close` | `discard?` | `{warnings}` |
| `doc.revert` | — | `{path, blocks, scenes, warnings}` |

Notes on each command:

- **`doc.new`**
  - Starts an empty document.
  - With `path`, the document is journaled next to that path. The file must not already exist (`already_exists`).
  - Nor may a crash journal for that path hold anything (2026-10-16): a document made with `doc.new` and never saved exists only in its journal, so `doc.new` fails with `already_exists`, with `details.journal` naming the journal, and leaves it untouched. `doc.open` recovers it, or sets it aside with `recover: "discard"`.
  - Every check runs before the open document is closed (2026-10-16), so a refused `doc.new` leaves it open.
  - Without a path, the document is held in memory only, and a warning says so.
  - `start` (2026-10-15) is an element type, such as `"scene_heading"`. The document then begins with one empty block of that type, so an editor has a line to type on. The block is part of the new document, not an edit: the document starts clean, with nothing to undo and an empty journal.
- **`doc.open`**
  - Reads a `.fountain`, `.spmd` or `.txt` file.
  - `recover` is `"auto"` (the default) or `"discard"`. If a crash journal exists, `auto` recovers it and `discard` sets it aside unread; see the journal spec.
  - `recovered` is null, or `{commands, external_change, last}`. `last` (added 2026-10-15) is when the newest recovered change was made, in milliseconds since the Unix epoch, for the app's recovery dialog.
- **`doc.save`**
  - Saves atomically.
  - `path` is "save as". The target's lock is taken, and its crash journal judged, before anything is written (2026-10-16; `docs/specs/journal.md`, "Saving"): a target open in another process fails with `doc_locked`, and one whose journal holds unsaved work fails with `unsaved_changes`. A refused save writes nothing.
  - `format` is `"fountain"`. It is inferred from the path when omitted.
  - If Fountain cannot hold something in the document, the save fails with `lossy_format` and lists the losses, unless `allow_lossy` is true.
- **`doc.close`**
  - Refuses with `unsaved_changes` unless `discard` is true.
  - With `discard`, unsaved changes are kept in a copy of the journal, which a `journal_discarded` warning names (2026-10-16; `docs/specs/journal.md`, "Closing").
  - An agent's `discard` fails with `not_your_step` when the document holds a writer's recovered work (2026-10-16; `docs/specs/mcp.md` §6). The same holds for an agent's `doc.revert`, and for its `doc.open` with `recover: "discard"`.
- **`doc.revert`** (2026-10-15)
  - Throws away unsaved changes and the undo history, and reads the document's file again.
  - The journal is never deleted by a revert: if it holds entries, it is copied aside to `<journal>.discarded-<ms>` first, then started again from the file.
  - Fails with `no_path` for an untitled document, and with `io_error` if the file cannot be read; the open document is unchanged in both cases.
  - The desktop app's recovery dialog uses it for Discard.

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

`doc.export` returns the document as text without saving it. `format` is `"fountain"` (the default), `"json"` (the title page and blocks, pretty-printed) or `"pages"` (the plain-text page view of `docs/specs/layout.md` §11). Losses are listed as for `doc.save`; nothing is refused, since nothing is written.

Pages come from laying out the document's Fountain normal form with the default page format (US Letter, feature film; `docs/specs/layout.md` §3.2), so they are the same after a save and reload.

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
| `text.insert` | `id`, `at`, `text`, `style?` | `{}` |
| `text.delete` | `id`, `from`, `to` | `{}` |
| `text.style` | `id`, `from`, `to`, `bold?`, `italic?`, `underline?` | `{}` |
| `block.split` | `id`, `at`, `type?` | `{id}` |
| `block.join` | `id` | `{id, at}` |
| `batch` | `commands`, `label?` | `{results}` |

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

### Output

| Command | Params | Result |
| --- | --- | --- |
| `export.pdf` | `path`, `watermark?`, `scene_numbers?`, `title_page?` | `{path, pages, bytes, warnings}` |

- **`export.pdf`**
  - Lays out the open document (as `scene.list` does) and writes it as a PDF, atomically, as `docs/specs/layout.md` §12 describes. The same document and options always give the same bytes.
  - `path` must end in `.pdf`, otherwise `unsupported_format`. It may never be the open document or its crash journal (`would_overwrite_document`).
  - `watermark` is text printed diagonally under every page. `scene_numbers` (default false) prints scene numbers beside headings. `title_page` (default true) includes the title page when the script has one.
  - `pages` counts the PDF's pages, title page included. It is never 0: a document with nothing to print gives one blank page, with the watermark if one is given (2026-10-14).
  - `warnings` lists `{code, message, chars?, id?}`: characters the bundled font cannot print, in the script or the watermark (`missing_glyph`, `missing_mark`), and layout diagnostics such as `dual_too_tall`, with the block `id` when there is one.
  - It is not undoable, not journaled, and leaves `dirty` as it was. Journal replay never runs it.

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
| `already_exists` | `doc.new` was given an existing file, the open document's path, or a path whose crash journal holds anything (`details.journal`). |
| `no_path` | `doc.save` has no path to save to. |
| `unsupported_format` | The file type is not supported in this version. |
| `lossy_format` | The format cannot hold the document; `details.losses` lists why. |
| `doc_locked` | Another process has the document open. |
| `io_error` | Reading or writing a file failed. |
| `journal_error` | The crash journal could not be written; the edit was rolled back. |
| `would_overwrite_document` | An output command was asked to write over the open document or its journal. |
| `invalid_range` | An offset is past the end of the block's text, or `from` is after `to`. |
| `not_your_step` | An agent tried to undo or redo a step made by someone else, or to set aside a writer's recovered work (`details.recovered`). |
| `not_allowed_attached` | A command an agent may not send over the app's agent link, such as `doc.open` (`docs/specs/mcp.md`). |
| `internal_error` | Something that should never happen, such as damaged bundled fonts. |
