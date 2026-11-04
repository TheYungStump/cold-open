# Command reference

<!-- Made from the MCP tool table and the commands' parameter types by crates/mcp/tests/reference.rs. Don't edit this page: change the code, then run `COLDOPEN_BLESS=1 cargo test -p coldopen-mcp --test reference`. -->

Everything Cold Open does to a script is one of these commands. The same names and parameters work everywhere:

- in a JSON batch for `coldopen run`, and over `coldopen serve --stdio` (`{"v": 1, "id": 1, "command": "block.insert", "params": {…}}`);
- as tools of the MCP server `coldopen mcp`, where the dot becomes an underscore (`block.insert` is the tool `block_insert`);
- in the app's palette, which lists every one.

Blocks are named by their ids (`b_42`), which stay the same as the script changes. Positions in a block's text count characters. The full rules, with every error code, are in the [command spec](https://github.com/TheYungStump/cold-open/blob/main/docs/specs/commands.md).

## `doc.new`

MCP tool `doc_new`. Start a new, empty script. With `path`, it is journaled next to that path from the first change.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `path` | string | optional | Where the new script will be saved; it is journaled next to that path from the first change. |
| `start` | one of `scene_heading`, `action`, `character`, `parenthetical`, `dialogue`, `transition`, `lyric`, `shot`, `general`, `page_break`, `section`, `synopsis`, `note`, `boneyard` | optional | The kind of one empty block the document starts with, so an editor has somewhere to type. Part of the new document, not an edit. |

## `doc.open`

MCP tool `doc_open`. Open a .fountain, .spmd, .txt, .fdx or .coldopen script. If the Cold Open app has it open, attach to the app instead, so edits appear live for the writer (`attach`: "auto" or "never").

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `attach` | one of `auto`, `never` | optional | "auto" (the default) attaches to the Cold Open app when it has the script open; "never" always opens the file here. |
| `path` | string | required | The script to open: .fountain, .spmd, .txt, .fdx or .coldopen. |
| `recover` | one of `auto`, `discard` or `apply` | optional | What to do with a crash journal beside the script: `auto` recovers it, `discard` sets it aside unread, `apply` also recovers one written on another computer. |

## `doc.save`

MCP tool `doc_save`. Save the script to its file, or to `path` (not allowed while attached to the app).

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `allow_lossy` | true or false | optional | Save even though the format can't hold everything in the script; the result lists what was lost. |
| `format` | string | optional | `fountain`, `fdx` or `coldopen`; from the path's extension when absent. |
| `overwrite` | true or false | optional | Replace the target even though it is a file other than the open document, or another program changed it since Cold Open last read or saved it. Writers only. |
| `path` | string | optional | Save to this path instead ("Save as"), converting when its extension is another format. |

## `doc.close`

MCP tool `doc_close`. Close the script, or detach from the app and leave the writer's script alone. Refuses unsaved changes unless `discard` is true.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `discard` | true or false | optional | Close even with unsaved changes; they are kept in a copy of the journal. |

## `doc.revert`

MCP tool `doc_revert`. Throw away unsaved changes and read the file again (not allowed while attached).

No parameters.

## `doc.get`

MCP tool `doc_get`. The title page and every block with its id, type, text and attributes, plus `rev`, a change counter.

No parameters.

## `doc.export`

MCP tool `doc_export`. The script as text without saving it: `fountain` (default), `json`, or `pages` (the plain-text page view).

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `format` | string | optional | `fountain` (the default), `fdx`, `json` or `pages`. |

## `scene.list`

MCP tool `scene_list`. The scenes in order: heading, number, page, length in eighths of a page, and the heading block's id.

No parameters.

## `text.find`

MCP tool `text_find`. Find text in the printed text of blocks; returns each match as a block id and character offsets.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `find` | string | required | The text to find. |
| `match_case` | true or false | optional | Match capitals exactly (the default). |
| `scope` | object | optional | Only this scene; the whole script when absent. |

## `text.complete`

MCP tool `text_complete`. Suggestions for finishing the text before an offset in a block, as the editor's autocomplete gives them.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `at` | whole number | required | The caret's place in it, in characters. |
| `id` | string | required | The block being typed in. |

## `history.list`

MCP tool `history_list`. The undo and redo steps, newest first, with each step's label and author.

No parameters.

## `block.insert`

MCP tool `block_insert`. Insert a block of a given type after or before a block id, or at the end. Returns the new block's id.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `after` | string | optional | The block to insert after. |
| `before` | string | optional | The block to insert before. With neither, the block goes at the end. |
| `centered` | true or false | optional | Centered action. |
| `depth` | whole number | optional | A section's depth, 1 or more. |
| `dual` | true or false | optional | A character who speaks at the same time as the one before (dual dialogue). |
| `runs` | list of object | optional | The block's text with styles, as runs, instead of `text`. |
| `scene_number` | string | optional | A scene heading's number, such as "12A". |
| `space_before` | whole number | optional | Blank lines above the block, when not the usual. |
| `text` | string | optional | The block's text, plain. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |
| `type` | one of `scene_heading`, `action`, `character`, `parenthetical`, `dialogue`, `transition`, `lyric`, `shot`, `general`, `page_break`, `section`, `synopsis`, `note`, `boneyard` | required | The element: `scene_heading`, `action`, `character`, `dialogue` and so on. |

## `block.set_type`

MCP tool `block_set_type`. Change a block's element type.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `id` | string | required | The block to change. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |
| `type` | one of `scene_heading`, `action`, `character`, `parenthetical`, `dialogue`, `transition`, `lyric`, `shot`, `general`, `page_break`, `section`, `synopsis`, `note`, `boneyard` | required | Its new element. |

## `block.set_text`

MCP tool `block_set_text`. Replace a block's whole text.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `id` | string | required | The block whose text to replace. |
| `runs` | list of object | optional | The new text with styles, as runs, instead of `text`. |
| `text` | string | optional | The new text, plain. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |

## `block.set_attrs`

MCP tool `block_set_attrs`. Set a block's attributes: scene number, dual dialogue, centered, section depth, space before.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `centered` | true or false | optional | Centered action. |
| `depth` | whole number | optional | A section's depth, 1 or more. |
| `dual` | true or false | optional | Dual dialogue, on a character. |
| `id` | string | required | The block to change. Attributes not given stay as they are. |
| `scene_number` | string | optional | A scene heading's number; `null` removes it. |
| `space_before` | whole number | optional | Blank lines above the block; `null` gives the usual. |

## `block.delete`

MCP tool `block_delete`. Delete a block.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `id` | string | required | The block to delete. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |

## `block.split`

MCP tool `block_split`. Split a block at a character offset; the second part becomes a new block of the given type (or the same type).

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `after_deleted` | true or false | optional | Keep a pending deletion at `at` in the first block rather than starting the new one with it. |
| `at` | whole number | required | Where, in characters; the end of the block starts a new one. |
| `id` | string | required | The block to split. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |
| `type` | one of `scene_heading`, `action`, `character`, `parenthetical`, `dialogue`, `transition`, `lyric`, `shot`, `general`, `page_break`, `section`, `synopsis`, `note`, `boneyard` | optional | The new block's element; the same as the first when absent. |

## `block.join`

MCP tool `block_join`. Join a block onto the end of the block before it, keeping the earlier block's type.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `id` | string | required | The block to join to the one before it. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |

## `text.insert`

MCP tool `text_insert`. Insert text at a character offset in a block, optionally bold, italic or underlined.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `after_deleted` | true or false | optional | Put the text after a pending deletion at `at` rather than before it, as typed by a caret drawn after the struck text. |
| `at` | whole number | required | Where, in characters from the block's start. |
| `id` | string | required | The block to type into. |
| `style` | object | optional | The inserted text's style; the style before it when absent. |
| `text` | string | required | The text to insert. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |

## `text.delete`

MCP tool `text_delete`. Delete the characters between two offsets in a block. Notes and boneyard are never deleted.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `from` | whole number | required | The first character to delete. |
| `id` | string | required | The block to delete from. |
| `to` | whole number | required | The character after the last one to delete. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |

## `text.style`

MCP tool `text_style`. Turn bold, italic or underline on or off between two offsets in a block.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `bold` | true or false | optional | Bold on or off; unchanged when absent. |
| `from` | whole number | required | The first character to style. |
| `id` | string | required | The block to style. |
| `italic` | true or false | optional | Italic on or off; unchanged when absent. |
| `to` | whole number | required | The character after the last one. |
| `underline` | true or false | optional | Underline on or off; unchanged when absent. |

## `text.replace`

MCP tool `text_replace`. Replace text everywhere, in one scene, or at one match (`at`), as one undo step.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `at` | object | optional | Replace only the match starting here. |
| `find` | string | required | The text to find. |
| `match_case` | true or false | optional | Match capitals exactly (the default). |
| `scope` | object | optional | Only this scene; the whole script when absent. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |
| `with` | string | required | What to put in its place. |

## `scene.move`

MCP tool `scene_move`. Move a scene, heading and all, before another scene's heading, or to the end.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `before` | string | optional | The scene or section to move it before; the end when absent. |
| `id` | string | required | The scene heading of the scene to move. |
| `last` | string | optional | The last block to move, at or after the scene's own last block; by default the scene's last block (2026-10-28). `changes.import` moves a whole diff unit with it. |
| `tracked` | true or false | optional | Record the edit as a tracked change. Absent: the document's tracking switch for the author decides. |

## `batch`

MCP tool `batch`. Run several edits as one undo step: all of them apply, or none do.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `commands` | list of object | required | The commands, run in order. One that fails undoes the whole batch. |
| `label` | string | optional | The undo history's name for the batch. |

## `changes.list`

MCP tool `changes_list`. The pending tracked changes, in document order: id, author, time, kinds, blocks, scene, and the inserted and deleted text.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `scene` | string | optional | Only changes with a mark in this scene. |

## `changes.reject`

MCP tool `changes_reject`. Withdraw this agent's own tracked changes, by `id`, `scene` or `all`. Another author's changes fail with not_your_change.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `all` | true or false | optional | Every pending change. |
| `id` | string | optional | One change, such as `c_3`. |
| `scene` | string | optional | The marks inside this scene. |

## `changes.accept`

MCP tool `changes_accept`. Accept tracked changes. Only the writer may; an agent gets writer_only.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `all` | true or false | optional | Every pending change. |
| `id` | string | optional | One change, such as `c_3`. |
| `scene` | string | optional | The marks inside this scene. |

## `changes.track`

MCP tool `changes_track`. Choose which edits are tracked. Only the writer may; an agent gets writer_only.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `agents` | true or false | optional | Track agents' edits. |
| `writer` | true or false | optional | Track the writer's edits. |

## `revision.list`

MCP tool `revision_list`. The production revision sets, oldest first: color, label, date, whether each is active, and the blocks and pages its marks are on.

No parameters.

## `revision.start`

MCP tool `revision_start`. Start a production revision set. Only the writer may; an agent gets writer_only. While a set is active, this agent's edits are marked with it like anyone's.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `color` | string | optional | The set's color; the next one in the document's list by default. |
| `date` | string | optional | `YYYY-MM-DD`; today's date in UTC by default. |
| `label` | string | optional | What revised pages' headers print before the date; "\<color> Rev." by default. |

## `revision.end`

MCP tool `revision_end`. End the active revision set. Only the writer may; an agent gets writer_only.

No parameters.

## `revision.colors`

MCP tool `revision_colors`. Set the list of revision colors. Only the writer may; an agent gets writer_only.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `colors` | list of string | required | The color names, in the order sets take them. |

## `revision.clear`

MCP tool `revision_clear`. Remove revision marks, of one set or all. Only the writer may; an agent gets writer_only.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `all` | true or false | optional | Clear every set's marks. |
| `set` | string | optional | The set whose marks to clear. |
| `sets` | true or false | optional | With `all`: the sets go too, and the table is the default again, as Save anyway to Fountain or .fdx leaves it. |

## `report.scenes`

MCP tool `report_scenes`. Every scene: heading, setting (int, ext, int_ext, est), location, time, page, length in eighths, words, and the characters who speak.

No parameters.

## `report.characters`

MCP tool `report_characters`. Every speaker, most words first: speeches, printed dialogue lines, words, scenes, extensions and first page.

No parameters.

## `report.locations`

MCP tool `report_locations`. Every location by name, split by setting and time of day, with its scenes.

No parameters.

## `title.get`

MCP tool `title_get`. The title page: its form (keys, lines or none), its Fountain keys with their values, and its positioned lines.

No parameters.

## `title.set`

MCP tool `title_set`. Set one title-page key's values (none removes the key). Refused with writer_only while this script tracks agents' edits.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `at` | whole number | optional | Where a new key goes, 0-based; the end by default. |
| `key` | string | required | The key, such as "Title" or "Draft date"; matched ignoring case. |
| `values` | list of string or list of object | optional | One value per line; none removes the key. |

## `title.set_lines`

MCP tool `title_set_lines`. Replace the title page's positioned lines. Refused with writer_only while this script tracks agents' edits.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `lines` | list of object | required | Every positioned title line, in order. |

## `diff`

MCP tool `diff`. Compare two drafts (.fountain, .fdx or .coldopen) scene by scene: moved, changed, added and deleted scenes, then blocks and words, with a readable summary in text. Without a, the open script is the first draft. With pdf, also writes the second draft with the changes marked.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `a` | string | optional | The first draft's path; the open document when absent. |
| `b` | string | required | The second draft's path. |
| `overwrite` | true or false | optional | Replace the PDF if it exists. |
| `pdf` | string | optional | Also write B with the changes marked to this .pdf path. |

## `changes.import`

MCP tool `changes_import`. Bring a draft into the open script as one writer's tracked changes. Writer-only: an agent gets writer_only.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `name` | string | optional | The name its changes carry; the draft's file name by default. |
| `path` | string | required | The draft to bring in. |

## `undo`

MCP tool `undo`. Undo this agent's newest step. Fails with not_your_step when the newest step is someone else's.

No parameters.

## `redo`

MCP tool `redo`. Redo this agent's step that was undone last.

No parameters.

## `export.pdf`

MCP tool `export_pdf`. Write the script as a PDF to `path`, with optional scene numbers and watermark. The script is not changed.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `changes` | `marked` or `accepted` | optional | Print pending tracked changes marked (the default) or accepted. |
| `overwrite` | true or false | optional | Replace a file already at `path`. Writers only. |
| `path` | string | required | The PDF to write. |
| `revision_pages` | one of `white`, `colored` | optional | Revised pages on white (the default) or on their set's color. |
| `scene_numbers` | true or false | optional | Print scene numbers beside the headings. |
| `title_page` | true or false | optional | Print the title page (the default). |
| `watermark` | string | optional | Text printed diagonally across every page. |

## `export.fdx`

MCP tool `export_fdx`. Write a copy of the script as an .fdx file to a new `path`. The open script keeps its own file and format.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `allow_lossy` | true or false | optional | Write even though the format cannot hold everything. Writers only. |
| `overwrite` | true or false | optional | Replace a file already at `path`. Writers only. |
| `path` | string | required | The copy to write. |

## `export.fountain`

MCP tool `export_fountain`. Write a copy of the script as a Fountain file to a new `path`. The open script keeps its own file and format.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `allow_lossy` | true or false | optional | Write even though the format cannot hold everything. Writers only. |
| `overwrite` | true or false | optional | Replace a file already at `path`. Writers only. |
| `path` | string | required | The copy to write. |

## `recovered.list`

MCP tool `recovered_list`. List the copies Cold Open set aside for a script (unsaved changes kept when they were discarded, journals that could not be replayed, damaged originals).

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `path` | string | optional | The script whose copies to list; the open document's without it. |

## `recovered.restore`

MCP tool `recovered_restore`. Restore a set-aside copy as a new script. Only the writer may; an agent gets writer_only.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `copy` | string | required | The set-aside copy, as `recovered.list` names it. |
| `path` | string | required | Where the restored script is journaled: a path with no file yet. |

## `recovered.delete`

MCP tool `recovered_delete`. Delete a set-aside copy. Only the writer may; an agent gets writer_only.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `copy` | string | required | The set-aside copy, as `recovered.list` names it. |

## `file.repair`

MCP tool `file_repair`. Read the .fdx file at `path`, repairing what it can, and write the repaired copy to a new path `out`; returns the repair report. `path` is never written.

| Parameter | Type | | What it is |
| --- | --- | --- | --- |
| `out` | string | required | Where the repaired copy goes. |
| `overwrite` | true or false | optional | Replace a file already at `out`. Writers only. |
| `path` | string | required | The .fdx file to read. It is never written. |
