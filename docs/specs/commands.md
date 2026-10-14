# Command schema, version 1

- **Date:** 2026-10-13
- **Status:** implemented in `crates/core` (phase 1; `export.pdf`, the `pages` export and scene pages added in phase 2, 2026-10-13)
- **Sources:** `docs/spec.md` (Command interface), `CLAUDE.md` (Commands). No external material.

Every action in Cold Open is a command. The desktop app, the CLI, JSON over stdio and the MCP server all send the same commands to one dispatcher.

## Envelope

Requests and responses are single JSON objects. Over stdio they are one per line (NDJSON).

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
| `doc.new` | `path?` | `{path, warnings}` |
| `doc.open` | `path`, `recover?` | `{path, format, blocks, scenes, warnings, recovered}` |
| `doc.save` | `path?`, `format?`, `allow_lossy?` | `{path, format, bytes, losses}` |
| `doc.close` | `discard?` | `{}` |

Notes on each command:

- **`doc.new`**
  - Starts an empty document.
  - With `path`, the document is journaled next to that path. The file must not already exist (`already_exists`).
  - Without a path, the document is held in memory only, and a warning says so.
- **`doc.open`**
  - Reads a `.fountain`, `.spmd` or `.txt` file.
  - `recover` is `"auto"` (the default) or `"discard"`. If a crash journal exists, `auto` recovers it and `discard` sets it aside unread; see the journal spec.
  - `recovered` is null, or `{commands, external_change}`.
- **`doc.save`**
  - Saves atomically.
  - `path` is "save as".
  - `format` is `"fountain"`. It is inferred from the path when omitted.
  - If Fountain cannot hold something in the document, the save fails with `lossy_format` and lists the losses, unless `allow_lossy` is true.
- **`doc.close`**
  - Refuses with `unsaved_changes` unless `discard` is true.

Opening or creating a document while another is open first closes the open one. If that one has unsaved changes, the request fails with `unsaved_changes`.

### Queries

| Command | Params | Result |
| --- | --- | --- |
| `doc.get` | — | `{path, dirty, title_page, blocks}` |
| `doc.export` | `format?` | `{format, text, losses}` |
| `scene.list` | — | `{scenes: [{id, index, number, heading, synopsis, blocks, words, page, eighths}]}` |

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
| `text.replace` | `find`, `with`, `scope?`, `match_case?` | `{count, blocks}` |
| `scene.move` | `id`, `before?` | `{}` |

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
- **`scene.move`**
  - Moves a scene (its heading and body) before the scene or section given in `before`, or to the end when `before` is omitted.

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
| `undo` | — | `{undone: "<command>"}` |
| `redo` | — | `{redone: "<command>"}` |

Undo is unlimited. A new edit clears the redo history.

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
| `unsaved_changes` | The document has unsaved changes. |
| `already_exists` | `doc.new` was given an existing file. |
| `no_path` | `doc.save` has no path to save to. |
| `unsupported_format` | The file type is not supported in this version. |
| `lossy_format` | The format cannot hold the document; `details.losses` lists why. |
| `doc_locked` | Another process has the document open. |
| `io_error` | Reading or writing a file failed. |
| `journal_error` | The crash journal could not be written; the edit was rolled back. |
| `would_overwrite_document` | An output command was asked to write over the open document or its journal. |
| `internal_error` | Something that should never happen, such as damaged bundled fonts. |
