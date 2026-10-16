# The desktop editor

- **Date:** 2026-10-15, updated 2026-10-16
- **Status:** implemented in `crates/app` (phase 3); the 2026-10-16 changes come from the reviews of phases 1–3
- **Sources:**
  - `docs/spec.md`: Writing, Structure and navigation, Comfort, Reliability.
  - The phase 3 brief and its approved decisions (2026-10-15).
  - `docs/specs/commands.md`, `docs/specs/journal.md`, `docs/specs/layout.md`, `docs/specs/mcp.md`.
  - The reviews of phases 1–3 (2026-10-15), listed in `docs/progress/review-fixes.md`.
  - General screenwriting practice as `docs/spec.md` summarizes it: which element usually follows which, and the common extensions, prefixes and times of day.
- No Final Draft software, manual, help text or screenshots were used. The element flow, the shortcuts and the look are our own choices. A choice where the sources are silent is marked **Resolution**.

## 1. Goals

1. **What you see is what prints.** The page view shows the pages that `export.pdf` writes: same lines, same breaks, same (MORE) and (CONT'D), and the same box for a character the bundled font lacks.
2. **Every change is a command.** Each keystroke, paste, drag and menu choice becomes one or more commands from `docs/specs/commands.md`, sent to the dispatcher. The editor never changes the document any other way.
3. **Nothing typed is lost.** A change appears on screen only after it is in the journal (`docs/specs/journal.md`).
4. **Keyboard first.** Every function has a keyboard path, and shortcuts come from a file the writer can edit.
5. **Fast.** Keystroke to screen in under 16 ms on a 120-page script (`docs/spec.md`).

## 2. The window

| Area | What it holds |
| --- | --- |
| **Hint bar** (top) | The file name and an unsaved mark, the current element type, what Enter and Tab will do next (§5), and the palette shortcut. |
| **Navigator** (left) | Sections and scenes (§7). |
| **Page area** (centre) | The pages on a neutral "desk", or the draft view (§3). |
| **Status bar** (bottom) | The caret's page and the page count, the current scene and its length, journal state (§13), agent state (§12), and zoom. |
| **Overlays** | The command palette (§9), the find bar (§8), the autocomplete list (§6), dialogs and banners. |

**Popups and the mouse** (2026-10-15). A click outside the palette, the find bar, the autocomplete list, the export dialog or a sheet closes it, as Esc does, and does nothing else: it never reaches the script, the navigator or a button behind it. The save and recovery dialogs decide what happens to the writer's work, so a click outside them is ignored. While the palette or a dialog is open, the wheel does not scroll the pages behind it.

**Lists.** In every list chosen with Up and Down (the palette, the autocomplete list, the navigator), the selection stops at the first and last item, and the first press the other way moves it back.

**Focus mode** hides the hint bar, navigator and status bar. Each panel can also be shown or hidden on its own. F6 moves keyboard focus between the navigator, the page area and an open find bar.

## 3. Views

- **Page view.**
  - **Pages.** The body pages come from the session's layout (`docs/specs/layout.md`). Lines are drawn on the character grid in Courier Prime. A cluster with no glyph in the bundled face is drawn as the same box the PDF prints.
  - **Title page.** Title pages are shown before the body, read-only in phase 3.
  - **Non-printing content.** Notes and boneyard inside a line, and transparent elements (sections, synopses, notes, boneyard, page breaks), never change the page. Each is shown by a small marker in the left margin at the line it belongs to.
- **Draft view.** One continuous column with the same wrapping as the pages, from layout's draft lines (`docs/specs/layout.md` §15.3). There are no page breaks, (MORE) or (CONT'D). A thin rule and the page number mark where each body page starts. Transparent elements show as grey, non-printing lines, so sections and synopses can be read and edited.
- **Zoom.** The steps are 50, 67, 80, 90, 100, 110, 125, 150, 175, 200, 250 and 300 percent. At 100% a page is drawn at its paper size on a 96-pixels-per-inch logical display; the operating system's scale factor applies on top. Zooming keeps what is in view in place: the caret's line stays at the same height on screen when the caret is in view, and otherwise the middle of the view stays put (2026-10-15).
- **The bars stay clear.** Pages scroll under the hint bar and the status bar, never over them: page text is clipped to the page area.
- **Themes.**

  | Theme | Desk | Page | Text |
  | --- | --- | --- | --- |
  | Light | warm grey | white | near-black |
  | Dark | near-black | dim grey | light grey |
  | High contrast | black | black, with a white border | white, with a yellow caret, selection and focus |

  The default follows the operating system's light or dark setting. Printing never uses the screen's colours.
- **Fonts.** The page is always Courier Prime (`crates/fonts`). Menus, the navigator and dialogs use Atkinson Hyperlegible Next (`crates/fonts`, `ui` feature). Both are bundled, so the app looks the same on every OS.

## 4. Caret and selection

- **The caret** is a position `(block id, offset)`, with the offset as in `docs/specs/commands.md` ("Positions in text"). It is drawn at the grid cell the layout gives for that position, mapped through the normal form's source map (`docs/specs/fountain.md` §10).
  - A position in text the normal form trims, such as a trailing space just typed, is drawn one cell further right for each trimmed space, on the same line.
  - In a block layout leaves out, because it is empty or holds only spaces, the caret is drawn where the block's first line will go: below the block before it, after the space layout would put between them (none inside a speech, so a new dialogue or parenthetical sits right under the line above), at its kind's column, after any spaces typed, or at the right edge for a right-aligned kind such as a transition (2026-10-15).
- **The selection** runs from an anchor position to the caret, and may cross blocks.
- **When an edit moves text**, its own or an agent's, the caret and anchor move with the text they sit in. If their block is deleted, they move to the nearest remaining block.

Movement ("Mod" is Ctrl on Windows and Linux, Cmd on macOS; "Word" is Ctrl on Windows and Linux, Option on macOS):

| Keys | Moves to |
| --- | --- |
| Left, Right | the previous or next cluster (`docs/specs/layout.md` §2), across block edges |
| Word+Left, Word+Right | the previous or next word start or end. A word is a run of letters, digits and apostrophes. |
| Up, Down | the same column on the line above or below, on screen. The column is remembered across a run of Up and Down presses. |
| Home, End (macOS: Mod+Left, Mod+Right) | the start or end of the line on screen |
| Mod+Home, Mod+End (macOS: Mod+Up, Mod+Down) | the start or end of the script |
| Page Up, Page Down | one screen up or down |
| Alt+Page Up, Alt+Page Down | the previous or next scene heading |
| Mod+Alt+Up, Mod+Alt+Down | the start of the previous or next block |

Shift with any of these extends the selection. Mod+A selects the whole script. A mouse click places the caret, a drag selects, a double click selects a word and a triple click selects a block.

## 5. Typing and smart element flow

### 5.1 What each key sends

| Input | Command(s) |
| --- | --- |
| Text: any printable character, including accented letters typed directly, with AltGr, with a dead key or committed by an input method | `text.insert` at the caret. With a selection, the selection is deleted and the text inserted, as one `batch`. |
| Backspace, Delete | `text.delete` of one cluster before or after the caret, or of the selection. |
| Word+Backspace, Word+Delete | `text.delete` to the previous or next word boundary. |
| Backspace at the start of a block | An empty block: `block.delete`, and the caret goes to the end of the previous block. A page break before the block: `block.delete` of the page break. Otherwise: `block.join`, when both blocks are printing kinds. Otherwise the caret just moves to the end of the previous block. |
| Delete at the end of a block | The same, applied to the next block. |
| Enter | §5.2 |
| Shift+Enter | `text.insert` of `\n` in action, dialogue, lyric and general blocks. In other kinds it acts as Enter. |
| Tab, Shift+Tab | §5.2, unless the autocomplete list is open (§6). |
| Mod+B, Mod+I, Mod+U | `text.style` on the selection. With no selection, they set the style for the next text typed. |
| Mod+1 … Mod+8 | `block.set_type` for the caret's block: 1 scene heading, 2 action, 3 character, 4 parenthetical, 5 dialogue, 6 transition, 7 shot, 8 general. |
| Mod+X, Mod+C, Mod+V | Cut (copy, then `text.delete`), copy, and paste (§5.4). |

- **Input methods.** Text an input method is still composing (a dead key waiting for its letter, for example) is drawn underlined at the caret but is not sent until it is committed. The journal never holds half-composed text.
- **Capitals.** **Resolution:** in blocks whose kind prints in capitals (scene heading, character, transition, shot), typed letters are inserted as capitals (Unicode upper case). The document then holds what prints, and the Fountain file reads naturally.
- **Typing over a block kind's limits.** A newline is never inserted into a scene heading, character, transition or shot by typing.

### 5.2 Enter and Tab: the element flow table

**Enter**:
- In the middle of a block, Enter splits it (`block.split`), and both parts keep the block's kind.
- At the end of a non-empty block, it starts a new block of the kind in the table's *Enter* column (`block.split` at the end, with `type`).
- On an empty block, it changes the block to the *Empty Enter* kind (`block.set_type`).
- At offset 0 of a non-empty block, it splits there, which leaves an empty block of the same kind before it.
- In a parenthetical whose text ends with `)`, the position just before that `)` counts as the end.

**Tab and Shift+Tab** change the current block's kind (`block.set_type`) to the *Tab* or *Shift+Tab* kind.

| Kind | Enter → new block | Empty Enter → | Tab → | Shift+Tab → |
| --- | --- | --- | --- | --- |
| Scene heading | action | action | action | transition |
| Action | action | scene heading | character | scene heading |
| Character | dialogue | action | transition | action |
| Parenthetical | dialogue | dialogue | dialogue | character |
| Dialogue | character | action | parenthetical | character |
| Transition | scene heading | scene heading | scene heading | character |
| Shot | action | action | action | scene heading |
| Lyric | lyric | dialogue | dialogue | character |
| General | general | action | action | action |
| Section, synopsis, note, boneyard | action | action | — | — |

- **Parentheses** (2026-10-15). When a block becomes a parenthetical by any route (Tab, Shift+Tab, an element shortcut, the palette, the flow table), its text is wrapped in `(` and `)` in the same `batch` as the type change, unless the text, spaces aside, already starts with `(` and ends with `)`. An empty block gets `()` with the caret between them. The brackets are inserted unstyled.
  - **Leaving.** When the block becomes another kind, the brackets the editor added are taken off in the same `batch`, but only while the text between them is still exactly the text that was wrapped. Brackets the writer typed, brackets around text that has changed, and brackets that were already there stay.
  - **Empty brackets** the editor added also go when the writer leaves them: Enter on them acts as on an empty block, and moving the caret to another block removes them as their own undo step. So cycling through types never leaves `()` behind.
  - **The caret** stays on the same character through each conversion, and through undo and redo of it.
- **Auto-convert.** These run as their own commands, so one undo reverses the conversion and keeps the text:
  - In an action block that was empty when typing began, a text equal to a scene prefix (`INT`, `EXT`, `EST`, `INT./EXT`, `INT/EXT`, `I/E`, `EXT/INT`, `E/I`, any case) followed by `.` or a space changes the block to a scene heading.
  - In an empty dialogue block, typing `(` changes the block to a parenthetical.
  - **Resolution:** pressing Enter at the end of an action block whose whole text has no lower-case letters and ends in `TO:` first changes it to a transition, matching the Fountain reader (`docs/specs/fountain.md` §6.2 rule 6).
- **Templates.** The table above is the built-in feature-film template. A user file `flow.json` in the config folder (§11) can override any cell:

  ```json
  {"v": 1, "kinds": {"dialogue": {"enter": "action"}, "action": {"tab": "transition"}}}
  ```

  - **Fields and values.** The fields are `enter`, `empty_enter`, `tab` and `shift_tab`. Kind names are those of `docs/specs/commands.md`.
  - **Bad entries.** An unknown kind, field or value is ignored and listed in a banner when the app starts.

### 5.3 The hint bar

It shows the caret block's kind, and what Enter and Tab would make it, from the table, for example "Character · Enter → Dialogue · Tab → Transition". The flow table is never a hidden rule.

### 5.4 Paste

- **Within one block.** Text with no line breaks, or any paste into a note or boneyard block, is inserted with `text.insert`.
- **Across blocks.** Other text is read as Fountain (`docs/specs/fountain.md`). The resulting elements are inserted at the caret as one `batch`: the caret's block is split, and the elements go between the two halves.
- **Styles.** Text copied out of Cold Open is put on the clipboard as Fountain, so styles and element kinds survive a copy and paste between scripts.

## 6. Autocomplete

`text.complete` (`docs/specs/commands.md`) finds the suggestions. The editor shows them in a list under the caret while the caret's block is one of the contexts below. Up and Down choose, stopping at the first and last item; Tab, Enter or a click on an item accepts; and Esc, or a click anywhere else, closes the list until the caret leaves the block or the text changes. Accepting replaces the range `from`..`to` with the item, as one `batch`.

| Context | When | Suggestions, in order |
| --- | --- | --- |
| `character` | In a character block, before any `(` | Names already used in character blocks (the text before any `(`, trimmed), compared in capitals, prefix-matched against the text typed so far. The block being typed in doesn't count. Order: (1) the speaker before the previous one in the current scene, (2) other speakers in this scene, most recent first, (3) all other names, most lines first. |
| `extension` | In a character block, after `(` | `V.O.`, `O.S.`, `O.C.`, `CONT'D`, and every extension used in the script. Order: most used first, then this list's order. Accepting adds the closing `)`. |
| `scene_prefix` | In a scene heading, before the first space | `INT.`, `EXT.`, `INT./EXT.`, `EST.`, `I/E.`. Accepting adds a space. |
| `location` | In a scene heading, after the first space and before any ` - ` | Locations from other headings: the text after the first space, up to the last ` - `. Order: most used, then most recent. |
| `time` | In a scene heading, after the last ` - ` | `DAY`, `NIGHT`, `MORNING`, `AFTERNOON`, `EVENING`, `DAWN`, `DUSK`, `LATER`, `CONTINUOUS`, `MOMENTS LATER`, `SAME`, and every time used in the script. Order: most used, then this list's order. |
| `transition` | In a transition block | `CUT TO:`, `DISSOLVE TO:`, `SMASH CUT TO:`, `MATCH CUT TO:`, `FADE OUT.`, `FADE TO BLACK.`, and every transition used. Order: most used, then this list's order. |

- **Matching.** Matching ignores case and requires at least one typed character, except for `time` and `extension`, whose list opens as soon as the separator is typed.
- **Shown.** At most 8 items are shown. The list does not open when the only match is exactly what is already typed.
- **Determinism.** Remaining ties are broken by first appearance in the script, so the same script always gives the same list. "In the current scene" means the character blocks between the nearest scene heading before the caret's block and that block.

## 7. Scene navigator

- **Rows.**
  - Each section is a header, indented by its depth.
  - Each scene shows its number (`scene_number`, or its position), heading, start page and length in eighths (`scene.list`).
  - The scene holding the caret is highlighted.
- **Moving around.** A click, or Enter on a selected row, moves the caret to the start of that heading.
- **Reordering.** Dragging a scene and dropping it before another scene or section sends `scene.move` with `before`; dropping after the last scene sends it with no `before`. With the navigator focused, Alt+Up and Alt+Down move the selected scene one place, also through `scene.move`.

Sections are not dragged in phase 3.

## 8. Find and replace

- **Opening it.** Mod+F opens the find bar. Mod+H (macOS: Mod+Alt+F) opens it with a replace field.
- **Highlights and stepping.** Matches come from `text.find` and are highlighted on the pages. Enter, F3 or Mod+G goes to the next match, and Shift with any of them to the previous one.
- **Options.** "Match case" (`match_case`), and "This scene only" (`scope`).
- **Replacing.** Replace sends `text.replace` with `at` for the current match. Replace all sends `text.replace` for the scope and is one undo step.

## 9. Command palette

Mod+K or Mod+Shift+P opens the palette.

- **What it lists.** Every core command, by its dotted name and a short title, and every app action (§10), with its shortcut. Typing filters the list by subsequence, and the best matches come first.
- **Commands that use the caret or selection** run directly: element types, styles, `scene.move` for the current scene, undo and redo.
- **Any other command** opens a small form where its params can be written as JSON, and shows the response. Every command is reachable from the keyboard, including queries such as `doc.get`.

## 10. Keyboard map

Shortcuts are looked up by **action** name: a core command (`undo`, `doc.save`, …), or one of the app actions below, which change only the view or the app's own state and never the document.

| Action | Default keys |
| --- | --- |
| `app.palette` | Mod+K, Mod+Shift+P |
| `app.new` (`doc.new`) | Mod+N |
| `app.open` (`doc.open` through a dialog) | Mod+O |
| `doc.save` | Mod+S |
| `app.save_as` | Mod+Shift+S |
| `app.close` (`doc.close` with a prompt) | Mod+W |
| `app.export_pdf` (`export.pdf` through a dialog) | Mod+Shift+E |
| `app.quit` | Mod+Q |
| `undo` | Mod+Z |
| `redo` | Mod+Shift+Z, and Mod+Y on Windows and Linux |
| `app.undo_agent` (undo the agent steps on top) | Mod+Alt+Z |
| `edit.cut`, `edit.copy`, `edit.paste`, `edit.select_all` | Mod+X, Mod+C, Mod+V, Mod+A |
| `edit.bold`, `edit.italic`, `edit.underline` | Mod+B, Mod+I, Mod+U |
| `element.scene_heading` … `element.general` | Mod+1 … Mod+8 (§5.1) |
| `edit.find`, `edit.replace` | Mod+F; Mod+H (macOS: Mod+Alt+F) |
| `edit.find_next`, `edit.find_previous` | F3, Mod+G; Shift+F3, Mod+Shift+G |
| `view.zoom_in`, `view.zoom_out`, `view.zoom_reset` | Mod+=, Mod+-, Mod+0 |
| `view.draft` (toggle the draft view) | Mod+Shift+D |
| `view.navigator` (toggle) | Mod+\ |
| `view.focus` (focus mode) | Mod+Shift+F |
| `view.next_pane` | F6 |
| `nav.previous_scene`, `nav.next_scene` | Alt+Page Up, Alt+Page Down |
| `app.history` (the undo history list) | Mod+Shift+H |
| `app.agent_disconnect` | — (status bar, palette) |
| `app.agent_access` (toggle) | — (palette) |
| `view.theme_system`, `view.theme_light`, `view.theme_dark`, `view.theme_high_contrast` | — (palette) |

Esc closes the topmost overlay, or the autocomplete list. Movement and editing keys (§4, §5.1) are built in and not listed in the map.

### 10.1 The keymap file

`keymap.json` in the config folder (§11) holds the writer's changes only:

```json
{"v": 1, "bindings": [
  {"keys": "Mod+Shift+K", "action": "app.palette"},
  {"keys": "Mod+1", "action": null}
]}
```

- **Key syntax.** Modifiers `Mod`, `Ctrl`, `Alt` (Option), `Shift` and `Super` (Cmd on macOS, the Windows key elsewhere), then one key, joined by `+`:
  - letters `A`–`Z`, digits `0`–`9`, `F1`–`F24`;
  - named keys: `Enter`, `Tab`, `Escape`, `Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `Up`, `Down`, `Left`, `Right`, `Space`;
  - or a single punctuation character.

  Names ignore case.
- **Applying the file.** The built-in map is applied first, then the file's entries in order. An entry replaces whatever was bound to its keys; `"action": null` removes the binding.
- **Errors.** A bad version, unknown action, malformed keys or a file over 1 MiB is ignored, entry by entry where possible, and listed in a banner at start-up. The file never stops the app from starting.
- **Upgrades.** The app never writes this file. An upgrade's new default shortcuts apply to keys the writer hasn't rebound, and nothing the writer bound is ever changed.

## 11. Files and folders

| Folder | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Config: `settings.json`, `keymap.json`, `flow.json` | `%APPDATA%\Cold Open` | `~/Library/Application Support/Cold Open` | `$XDG_CONFIG_HOME/coldopen` (default `~/.config/coldopen`) |
| Data: `Unsaved/` | `%LOCALAPPDATA%\Cold Open` | `~/Library/Application Support/Cold Open` | `$XDG_DATA_HOME/coldopen` (default `~/.local/share/coldopen`) |
| Run: agent link session files (`docs/specs/mcp.md`) | `%LOCALAPPDATA%\Cold Open\run` | `~/Library/Application Support/Cold Open/run` | `$XDG_RUNTIME_DIR/coldopen`, or `run/` in the data folder |

`settings.json` holds the theme, zoom, view, panel visibility, agent access (§12) and up to 10 recent files. Like `keymap.json`, an unreadable file is ignored with a banner and never stops the app.

- **New.** **Resolution:** a new script is created with `doc.new` at `Unsaved/Untitled-<n>.fountain` in the data folder, with `start: "scene_heading"`, so it has a crash journal from its first keystroke and begins clean, on an empty scene heading. A script nobody typed in is never offered as lost work and never asks to be saved. The file itself is written only by "Save as". Mod+S on such a script opens "Save as".
- **No script open.** After Close, the window shows New, Open and the recent files. Mod+N, Mod+O and the palette work there too.
- **Open.** A dialog for `.fountain`, `.spmd` and `.txt` files, then `doc.open`. If the result reports a recovery, the recovery dialog opens (§13).
- **Recent files** are listed in the palette and the open dialog.
- **Save.** `doc.save` runs off the UI thread (`docs/specs/journal.md`, "Saving off the UI thread"). The status bar shows "Saving…" until it is done. Mod+S during a save saves again when it finishes. If Fountain can't hold something in the script, the app says what and asks "Save anyway" (`allow_lossy`) or Cancel.
- **Save as.** A dialog, then `doc.save` with `path`, on the UI thread. A name typed without an extension gets `.fountain`. Moving an untitled script out of `Unsaved/` also removes its old journal.
- **Close and quit.** With unsaved changes, the app asks: Save, Don't save, or Cancel. Keys: Enter or S saves, D or N doesn't, Esc or C cancels; nothing else runs while it asks. Don't save sends `doc.close` with `discard`. Save finishes the save, including a "Save as" for an untitled script, before closing. The window's close button asks the same question.
- **Export PDF.** First a small form: scene numbers (Mod+1, off by default), title page (Mod+2, on by default) and a watermark field, which has the keyboard. Enter opens the dialog for the path; then `export.pdf`. A banner gives the page count and the first warning, if any.
- **Dialogs.** Open, Save as and Export use the operating system's own file dialogs (the XDG desktop portal on Linux, with no GTK). The app's own questions (unsaved changes, recovery, export options) are drawn in the window, and every button in them has a key.
- **Leftover work at start-up.** The app checks `Unsaved/` for journals that hold entries, and the recent files for journals that opening them would recover (2026-10-16: this includes a journal holding only the header of a save that never reached the disk). Either means a session ended without saving. It offers to reopen each one, so a crash is recovered even if the writer never reopens the file by hand.

## 12. Agents

When an agent attaches to the open script through `coldopen mcp` (`docs/specs/mcp.md`):

- **Status bar.** It shows "<name> connected · N edits" for as long as the link is open. Clicking it offers Disconnect, and Turn off agent access.
- **Turning access off.** The "agent access" setting (on by default) closes the app's endpoint and removes its session file, so `coldopen mcp` can't find or attach to the script.
- **Margin marks.** Each block an agent changed gets a violet bar in the left margin. The bar stays until the writer edits that block, or the "Dismiss agent marks" action clears all of them.
- **Undo.** Agent steps appear in the undo history with their label, `<name> · <tool>`. Mod+Z undoes the newest step whoever made it. `app.undo_agent` undoes all the agent steps on top of the history, one `undo` each.
- **Caret.** Agent edits never move the writer's caret except to keep it in the same text (§4).

## 13. Reliability on screen

- **Journal state** in the status bar:
  - "Protected": every journal entry is flushed.
  - "Saving…": a save is running.
  - "Protecting…": entries have waited more than 250 ms for the disk.
  - "Not protected": the journal failed. The reason is shown, and edits are refused until it is fixed.
- **Recovery dialog.** When `doc.open` recovers unsaved work, a dialog shows:
  - the number of changes recovered or, when only the journal's header was (`commands` is 0; `docs/specs/journal.md`, recovery step 4), that the file on disk is not the version Cold Open last saved or opened, and that version is back in the script;
  - the time of the last one;
  - the scenes they touched, found by comparing blocks by id with the file on disk;
  - when `external_change` is true, a warning that the file was changed by another program and that saving will replace that version.

  **Keep** (Enter, K or Esc) continues with the recovered script, unsaved. **Discard** (D) sends `doc.revert`, which keeps a copy of the journal. For a script that was never saved there is no file to go back to: Discard closes it with `discard` and starts a new script. That too keeps a copy of the journal (2026-10-16), and the dialog says so.
  The number of changes is the number of recovered commands, so a burst of typing counts each key.
- **Errors.** A command that fails shows a banner with the error's message, and the editor stays usable. Nothing fails silently.
- **Esc** closes the nearest thing first: the autocomplete list, the find bar or palette, a selection, then the banners, then the leftover-work offer. "Dismiss messages" in the palette does the last two.

## 14. Testing the app

**Resolution:** the app's own tests run it headless (`crates/app/src/headless.rs`): real keyboard, mouse and input-method events go through the real view and widgets, the messages they publish go through `App::update`, and the tasks `update` returns are run, including widget operations such as focus and scrolling, the clipboard, and the save worker. Only the window is missing; frames can still be drawn with tiny-skia. File dialogs are answered by a script (`Dialogs::scripted`).

- **Typing gate** (`crates/app/tests/typing_gate.rs`). A short scene is typed key by key: smart flow, two autocomplete picks, accented letters from the keyboard and from an input method, a pause and an undo, then Mod+S and its "Save as" dialog. The saved file must equal `tests/fixtures/app/gate-scene.fountain` byte for byte.
- **File flows** (`crates/app/tests/file_flows.rs`): saving in place, close and quit prompts, save as from `Unsaved/`, the recovery dialog after a crash (Keep and Discard), leftover work at start-up, and PDF export options.

## 15. Not in phase 3

- Editing notes and boneyard inside a line: they are kept, and marked in the margin.
- A dual-dialogue editing UI: the `dual` flag is set through the palette.
- The title page editor, spellcheck, and templates other than feature film.
- A settings screen for shortcuts.
- Multiple windows, and a native macOS menu bar.
- Screen readers, and input methods for Chinese, Japanese and Korean (v1.x in `docs/spec.md`).
