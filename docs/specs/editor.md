# The desktop editor

- **Date:** 2026-10-15, updated 2026-10-16
- **Status:** implemented in `crates/app` (phase 3); the 2026-10-16 changes come from the reviews of phases 1–3; phase 4 (2026-10-16) added .fdx files, the repair report, Recovered versions and one command router (§16). Phase 5a (2026-10-18) added bundles and track changes (§17). Phase 5b (2026-10-22) adds production revisions (§18), the title page editor (§19), reports (§20), comparing drafts (§21) and missing recent files (§11); being implemented.
- **Sources:**
  - `docs/spec.md`: Writing, Structure and navigation, Comfort, Reliability.
  - The phase 3 brief and its approved decisions (2026-10-15).
  - `docs/specs/commands.md`, `docs/specs/journal.md`, `docs/specs/layout.md`, `docs/specs/mcp.md`.
  - The reviews of phases 1–3 (2026-10-15), listed in `docs/progress/review-fixes.md`.
  - The phase 4 brief and the decisions taken with the project's owner (2026-10-16, `docs/progress/phase-4.md`), and `docs/specs/fdx.md`.
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
  - **Title page.** Title pages are shown before the body, read-only in phase 3. Since phase 5b (2026-10-22) they are edited with "Edit title page…" (§19).
  - **Non-printing content.** Notes and boneyard inside a line, and transparent elements (sections, synopses, notes, boneyard, page breaks), never change the page. Each is shown by a small marker in the left margin at the line it belongs to.
- **Draft view.** One continuous column with the same wrapping as the pages, from layout's draft lines (`docs/specs/layout.md` §15.3). There are no page breaks, (MORE) or (CONT'D). A thin rule and the page number mark where each body page starts. Transparent elements show as grey, non-printing lines, so sections and synopses can be read and edited.
- **Zoom.** The steps are 50, 67, 80, 90, 100, 110, 125, 150, 175, 200, 250 and 300 percent. At 100% a page is drawn at its paper size on a 96-pixels-per-inch logical display; the operating system's scale factor applies on top. Zooming keeps what is in view in place: the caret's line stays at the same height on screen when the caret is in view, and otherwise the middle of the view stays put (2026-10-15).
  - **A page always fits the page area's width** (2026-10-21). The page area never scrolls sideways, so a page wider than the area at the chosen zoom is drawn at the largest zoom that fits it, with its margins, and never below 50%. The status bar then shows the zoom drawn with "(fit)", such as "83% (fit)". Zoom out steps down from the zoom drawn. Zoom in can't go past the fit until the area is wider: a wider window, or a side panel closed. The chosen zoom stays in the settings and comes back as soon as it fits. Before, a page at 100% in a window 1280 pixels wide, with both the navigator and the changes panel open, was cut off on the right (phase 5a decision 41). The area's width is the one it reports when redrawn, so the fit follows a resize or a panel opening on the next frame.
- **The bars stay clear.** Pages scroll under the hint bar and the status bar, never over them: page text is clipped to the page area.
- **Themes.**

  | Theme | Desk | Page | Text |
  | --- | --- | --- | --- |
  | Light | warm grey | white | near-black |
  | Dark | near-black | dim grey | light grey |
  | High contrast | black | black, with a white border | white, with a yellow caret, selection and focus |

  The default follows the operating system's light or dark setting. Printing never uses the screen's colours.
- **Fonts.** The page is always Courier Prime (`crates/fonts`). Menus, the navigator and dialogs use Atkinson Hyperlegible Next (`crates/fonts`, `ui` feature). Both are bundled, so the app looks the same on every OS.
- **Tracked changes** (2026-10-18) show on the pages as they print (§17): the page area always lays out the marked form (`docs/specs/changes.md` §9), so the pages on screen are the pages a marked PDF prints.

## 4. Caret and selection

- **The caret** is a position `(block id, offset)`, with the offset as in `docs/specs/commands.md` ("Positions in text"). It is drawn at the grid cell the layout gives for that position, mapped through the normal form's source map (`docs/specs/fountain.md` §10).
  - A position in text the normal form trims, such as a trailing space just typed, is drawn one cell further right for each trimmed space, on the same line. The same holds for spaces layout leaves off the end of a line: a .fdx script or a project is laid out unnormalized (`docs/specs/layout.md` §4.8), so a space just typed there reaches the page and wrapping leaves it off its line (2026-10-21; before, the caret stayed put until the next letter came).
  - In a block layout leaves out, because it is empty or holds only spaces, the caret is drawn where the block's first line will go: below the block before it, after the space layout would put between them (none inside a speech, so a new dialogue or parenthetical sits right under the line above), at its kind's column, after any spaces typed, or at the right edge for a right-aligned kind such as a transition (2026-10-15).
- **Struck text** (2026-10-18; reworked 2026-10-21). The caret's offset counts the current text, so it never stands inside a pending deletion. Struck text therefore gives the caret two places at one offset: just before it, or just after it, before the next character of the current text. The caret records which (`past`).
  - **Right** from before struck text lands just after it; the next Right steps over the next character. **Left** from just after that character lands just after the struck text, and the next Left just before it. Before 2026-10-21 one move jumped the struck text and the character after it.
  - **Shift** with Left or Right extends the selection the same way, one side of the struck text at a time. A selection of struck text alone selects no current text: Delete and Backspace only drop the selection.
  - **Word moves** and moves to the end of a block land after the struck text there, as Left does; moves to the start of a block land before it. A click lands after it when it falls on or after the struck text's last cell, and End lands after it at the end of a line.
  - **Typing and Enter** go where the caret is drawn: before the struck text, or after it (`text.insert` and `block.split` with `after_deleted`, `docs/specs/commands.md`). Backspace that takes text out (untracked, or the writer's own pending insertion) leaves the caret where it was drawn; Backspace that strikes text leaves it before the struck text, ready for the next Backspace, as before.
  - **An edit by someone else** at the caret's offset keeps the caret on its side of the struck text.
  - In a block that is a pending deletion the caret can only stand at its start; typing there is refused (`change_pending`), and the banner says to accept or reject the deletion first.
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
| A whole word deleted | (2026-10-21) Word+Backspace, Word+Delete, and Backspace, Delete or Mod+X on a selection within one block take one space with the words they delete, so no double space is left, tracked or not. That happens when the deleted text is whole words and would otherwise leave two spaces together, a space at the start or end of the block, or a space before closing punctuation (`. , ; : ! ? )` or `…`). The space after the words goes when there is one, else the space before. "the quick brown" without "quick" is "the brown"; "on the roof." without "roof" is "on the.". Typing over a selection, part of a word and a selection across blocks delete exactly what is selected. Agents' `text.delete` always deletes exactly its range. |
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
- **Files** (recent scripts and leftover work, §11) are listed by name first ("Open recent: low-tide.fdx"), then their folder (2026-10-21). Every row is one line:
  - a name too long for the row is shortened in the middle, so its extension shows;
  - a long folder keeps its first part (the drive, or `/`) and as many of its last folders as fit, around "…": `C:\…\Drafts\Act two`. A last folder too long even alone is itself shortened in the middle.
  - The whole path shows in a tooltip when the mouse rests on the row.
  - Typing still matches the whole folder.
  - Before, a long folder took the row's width, and the title, squeezed to nothing, made a tall empty row.
- **Commands that use the caret or selection** run directly: element types, styles, `scene.move` for the current scene, undo and redo.
- **Any other command** opens a small form where its params can be written as JSON, and shows the response. Every command is reachable from the keyboard, including queries such as `doc.get`. What the form sends goes through the app's command router like everything else (§16): the commands that open, create, close, revert or save the script, export it, repair a file or restore a copy run the app's own flows, so the form never skips a question the screens ask. `doc.close` with `discard` still asks.

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
| `app.export_fdx`, `app.export_fountain` (save a copy as .fdx or Fountain, through a dialog) | — (palette) |
| `app.repair` (repair an .fdx file into a new one, §11) | — (palette) |
| `app.recovered` (the Recovered versions screen, §13) | — (palette, start screen, notices) |
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
| `changes.accept_here`, `changes.reject_here` (the change at the caret, §17) | Mod+Shift+Enter, Mod+Shift+Backspace |
| `changes.next`, `changes.previous` | F7, Shift+F7 |
| `view.changes` (toggle the changes panel) | Mod+Shift+R |
| `changes.accept_scene`, `changes.reject_scene`, `changes.accept_all`, `changes.reject_all` | — (palette, panel) |
| `changes.track_mine`, `changes.track_agents` (toggle) | — (palette, status bar) |
| `app.save_as_project` (Save as a `.coldopen` bundle) | — (palette, the lossy-save question) |
| `app.revision_start`, `revision.end`, `app.revision_colors`, `app.revision_clear` (2026-10-22, §18) | — (palette, the hint bar's revision menu) |
| `app.title_page` (the title page editor, §19) | Mod+Shift+T |
| `app.reports` (§20) | Mod+Shift+I |
| `app.compare`, `app.import_draft` (§21) | — (palette) |
| `app.forget_recent` (remove a missing recent file, §11) | — (palette, Delete on the start screen) |

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

`settings.json` holds the theme, zoom, view, panel visibility, agent access (§12), the writer's name for tracked changes (`author_name`, 2026-10-18, §17) and up to 10 recent files. Like `keymap.json`, an unreadable file is ignored with a banner and never stops the app.

- **New.** **Resolution:** a new script is created with `doc.new` at `Unsaved/Untitled-<n>.fountain` in the data folder, with `start: "scene_heading"`, so it has a crash journal from its first keystroke and begins clean, on an empty scene heading. A script nobody typed in is never offered as lost work and never asks to be saved. The file itself is written only by "Save as". Mod+S on such a script opens "Save as".
- **No script open.** After Close, the window shows New, Open and the recent files. Mod+N, Mod+O and the palette work there too.
- **Open.** A dialog for `.fountain`, `.spmd`, `.txt`, (2026-10-16) `.fdx` and (2026-10-18) `.coldopen` files, then `doc.open`. If the result reports a recovery, the recovery dialog opens (§13). If it reports a repair, the repair dialog does (below).
- **A repaired file** (2026-10-16). When `doc.open` reports `repaired`, a dialog says "Cold Open repaired this file", gives the counts (fixed, dropped) and the first few issues in plain words, with no script text. **OK** (Enter, Esc or O) continues with the repaired script; nothing has been written yet. **Details** (D) shows the whole report. **Save repaired copy…** (S) runs Save as. The first save over the damaged file keeps a hidden copy of it, listed in Recovered versions, and says so in a banner.
- **Repair a file…** (`app.repair`, 2026-10-16) asks for an .fdx file, then for where to write the repaired copy, runs `file.repair`, and shows the report in the same dialog. The open script is not touched.
- **Recent files** are listed in the palette and the open dialog, .fdx files included. A script opened, saved as or restored is added.
  - **A recent file that is gone** (2026-10-22, decided with the project's owner) stays in the list, dimmed, with "Missing", on the start screen and in the palette. It is never dropped on its own: a drive that was unplugged, or a synced folder that hasn't synced yet, comes back. "Remove from recent" (Delete on the start screen's row, and a palette entry for each missing file) takes it out of `settings.json`. Choosing a missing file says it can't be found and offers to remove it.
  - The files are checked off the UI thread, once each time the list is shown, so a slow network path never holds a frame. Until a check answers, a file shows as present.
- **Save.** `doc.save` runs off the UI thread (`docs/specs/journal.md`, "Saving off the UI thread"): the UI thread only takes a snapshot of the script and appends the journal's marker and header (2026-10-19). The status bar shows "Saving…" until it is done. Mod+S during a save saves again when it finishes. If Fountain can't hold something in the script, the app says what and asks "Save anyway" (`allow_lossy`) or Cancel. Only losses of class `lost` are asked about (2026-10-18; `docs/specs/commands.md`, "Losses"). For a .fountain or .fdx script the question also offers **Save as Cold Open project…** (P), which runs Save as with the `.coldopen` type chosen and the script's name, so nothing is dropped (`docs/spec.md`: never silently drop data). Pending tracked changes are such a loss (`tracked_changes`).
- **Changed by another program** (2026-10-16). If the save fails with `file_changed`, the app says the file was changed by another program and asks: **Save as…** (Enter or A, the default, which keeps both versions), **Overwrite** (O: saves again with `overwrite`), **Reload** (R: `doc.revert`, which keeps a copy of the unsaved changes, and a banner names it), or Cancel (Esc or C). A pending close or quit waits on the answer, as for "Save anyway".
- **Save as.** A dialog, then `doc.save` with `path`, on the UI thread. PDF export passes `overwrite` the same way. The dialog offers Fountain (`.fountain`), FDX (`.fdx`) and (2026-10-18) Cold Open project (`.coldopen`); the extension of the name chosen decides the format, so Save as converts (`docs/specs/commands.md`), and what the new format cannot hold is asked about first, as "Save anyway" does. A name typed without an extension gets the extension of the script's own format (2026-10-16; before, always `.fountain`).
- **Export .fdx and Export Fountain** (2026-10-16) save a copy in that format through a dialog (`export.fdx`, `export.fountain`); the open script keeps its own file and format. Losses are asked about as for a save. Moving an untitled script out of `Unsaved/` also removes its old journal. The system's save dialog asks before replacing a file, so the app passes `overwrite` (2026-10-16), except when it added the extension itself: the dialog asked about the name without it.
- **Close and quit.** With unsaved changes, the app asks: Save, Don't save, or Cancel. Keys: Enter or S saves, D or N doesn't, Esc or C cancels; nothing else runs while it asks. Don't save sends `doc.close` with `discard`. Save finishes the save, including a "Save as" for an untitled script, before closing. The window's close button asks the same question.
- **Export PDF.** First a small form: scene numbers (Mod+1, off by default), title page (Mod+2, on by default), (2026-10-18) "Print the current text" (Mod+3, off by default: tracked changes print marked, as on screen; shown only when the script has pending changes) and a watermark field, which has the keyboard. Enter opens the dialog for the path; then `export.pdf`. A banner gives the page count and the first warning, if any.
- **Dialogs.** Open, Save as and Export use the operating system's own file dialogs (the XDG desktop portal on Linux, with no GTK). The app's own questions (unsaved changes, recovery, export options) are drawn in the window, and every button in them has a key.
- **Leftover work at start-up.** The app checks `Unsaved/` for journals that hold entries, and the recent files for journals that opening them would recover (2026-10-16: this includes a journal holding only the header of a save that never reached the disk). Either means a session ended without saving. It offers to reopen each one, so a crash is recovered even if the writer never reopens the file by hand.

## 12. Agents

When an agent attaches to the open script through `coldopen mcp` (`docs/specs/mcp.md`):

- **Status bar.** It shows "<name> connected · N edits" for as long as the link is open. Clicking it offers Disconnect, and Turn off agent access.
- **Turning access off.** The "agent access" setting (on by default) closes the app's endpoint and removes its session file, so `coldopen mcp` can't find or attach to the script.
- **Tracked by default** (2026-10-18). While the script's agent switch is on (the default), an agent's edits arrive as tracked changes under its name (`docs/specs/mcp.md` §6.1), in its color, and the writer settles them as §17 says. "Track agent edits" in the palette and in the status bar's agent menu turns the switch off or on for this script (`changes.track`, an undoable step). The status bar's agent text adds "· N to review" while the agent has pending changes.
- **Margin marks.** Each block an agent changed without tracking gets a violet bar in the left margin. The bar stays until the writer edits that block, or the "Dismiss agent marks" action clears all of them. Tracked edits have their own marks (§17) and get no violet bar.
- **Undo.** Agent steps appear in the undo history with their label, `<name> · <tool>`. Mod+Z undoes the newest step whoever made it. `app.undo_agent` undoes all the agent steps on top of the history, one `undo` each.
- **Caret.** Agent edits never move the writer's caret except to keep it in the same text (§4).

## 13. Reliability on screen

- **Journal state** in the status bar:
  - "Protected": every journal entry is flushed.
  - "Saving…": a save is running.
  - "Protecting…": entries have waited more than 250 ms for the disk.
  - "Not protected": the journal failed. The reason is shown, and edits are refused until it is fixed: a flush succeeds again, or, for a journal a failed write left broken, a save starts it again (`docs/specs/journal.md`, "Writing").
- **Recovery dialog.** When `doc.open` recovers unsaved work, a dialog shows:
  - the number of changes recovered or, when only the journal's header was (`commands` is 0; `docs/specs/journal.md`, recovery step 4), that the file on disk is not the version Cold Open last saved or opened, and that version is back in the script;
  - the time of the last one;
  - the scenes they touched, found by comparing blocks by id with the file on disk;
  - when `external_change` is true, a warning that the file was changed by another program and that saving will replace that version.

  **Keep** (Enter, K or Esc) continues with the recovered script, unsaved. **Discard** (D) sends `doc.revert`, which keeps a copy of the journal. For a script that was never saved there is no file to go back to: Discard closes it with `discard` and starts a new script. That too keeps a copy of the journal (2026-10-16), and the dialog says so.
  The number of changes is the number of recovered commands, so a burst of typing counts each key.
- **A journal from somewhere else** (2026-10-16). When `doc.open` fails with `journal_foreign`, the app asks before anything is recovered: "Unsaved changes from somewhere else", naming the computer and path the journal was written for. **Recover them** (Enter or R) opens with `recover: "apply"`, and the recovery dialog follows as usual, so Discard is still there. **Set them aside** (S) opens with `recover: "discard"`, which keeps a copy. Cancel (Esc or C) leaves the script closed. Once recovered, the journal is this computer's (`docs/specs/journal.md`, "Adopting it", 2026-10-21): a later crash is recovered with the usual dialog, without this question again.
- **Errors.** A command that fails shows a banner with the error's message, and the editor stays usable. Nothing fails silently.
- **Notices** (2026-10-21). Banners queue, and the bar under the hint bar shows the newest.
  - **Dismiss** closes the one shown, and the next newest takes its place.
  - With more than one, the bar adds **+N more**, which opens the full list, and **Dismiss all**.
  - **The list** shows every notice, newest first, each with its own Dismiss (and Recovered versions where it applies), under "N notices" with **Show fewer** and **Dismiss all**. It closes by itself when one notice is left.
  - A notice shows the Recovered versions button only when it is about a set-aside copy.
  - Before, Dismiss closed every notice at once, and "+N more" was text that opened nothing.
- **Work set aside** (2026-10-16). When work is set aside (Don't save, Discard in the recovery dialog, Reload, a journal kept aside on open, a damaged original kept on save), the banner says "Your unsaved changes were set aside. Open Recovered versions to restore them." (or, for a damaged original, "The file as it was before the repair was kept. Open Recovered versions to see it.") with a **Recovered versions** button. Banners never show the copy's path; before phase 4 some named it, and Don't save and Discard said nothing.
- **Recovered versions** (2026-10-16; `app.recovered`, also on the start screen and in those banners). A screen listing the set-aside copies (`docs/specs/journal.md`, "Recovered versions") of the open script, the recent scripts and the untitled scripts in `Unsaved/`, newest first. Each row shows the script's name, what the copy is ("Set aside when you chose Don't save", "Changes that could not all be replayed", "An unreadable crash journal", "The file before Cold Open repaired it") and when it was made. Up and Down choose a row, stopping at the ends; the selected copy is previewed read-only beside the list. **Restore** (R or Enter) opens it as a new untitled script (`recovered.restore` into `Unsaved/`, named "<script> (restored)" with the script's extension, or "(restored 2)" and so on when that name is taken), after the usual question about the open script's unsaved changes; nothing is overwritten. **Delete** (Delete) asks "Delete this copy? It can't be undone." (Enter deletes, Esc cancels), then sends `recovered.delete`. Esc closes the screen. A copy that will be removed at the next start says so ("Removed at next start: older than 30 days").
- **Clean-up at start-up** (2026-10-16): the app deletes the copies the rule in `docs/specs/journal.md` allows, and records in `recovered.json` the first time each copy was shown in the list.
- **Esc** closes the nearest thing first: the autocomplete list, the find bar or palette, a selection, then the full list of notices (2026-10-21), then all the notices, then the leftover-work offer. "Dismiss messages" in the palette does the last three.

## 14. Testing the app

**Resolution:** the app's own tests run it headless (`crates/app/src/headless.rs`): real keyboard, mouse and input-method events go through the real view and widgets, the messages they publish go through `App::update`, and the tasks `update` returns are run, including widget operations such as focus and scrolling, the clipboard, and the save worker. Only the window is missing; frames can still be drawn with tiny-skia. File dialogs are answered by a script (`Dialogs::scripted`).

- **Typing gate** (`crates/app/tests/typing_gate.rs`). A short scene is typed key by key: smart flow, two autocomplete picks, accented letters from the keyboard and from an input method, a pause and an undo, then Mod+S and its "Save as" dialog. The saved file must equal `tests/fixtures/app/gate-scene.fountain` byte for byte. Since 2026-10-21 the caret is checked after every key, and the keys are typed into a Fountain script, an .fdx script and a project. A typed character must leave the caret one cell right of where that character starts, or on a later line where the text wrapped. The caret must also show in the same place after every key in all three formats.
- **File flows** (`crates/app/tests/file_flows.rs`): saving in place, close and quit prompts, save as from `Unsaved/`, the recovery dialog after a crash (Keep and Discard), leftover work at start-up, and PDF export options.
- **The .fdx gates** (2026-10-16): the typing gate's key sequence is also typed into an .fdx script, which must save and read back with nothing lost; the crash gate also kills the app during .fdx saves and during Save as from Fountain to .fdx.
- **Crash gate** (`crates/app/tests/crash_gate.rs`). The app runs as its own process (`coldopen-driver`), keys go in one at a time, and the process is killed (TerminateProcess or SIGKILL) at points spread over the sequence. Reopening must give back every acknowledged key. Since 2026-10-16 (review finding F17.4) it also saves every seven keys, so kills land before, during and right after saves, and it kills the app while an agent attached through the real link is inserting lines: every edit the agent was told had succeeded must come back. Since 2026-10-18 it also kills the app while the writer types with tracking on, while changes are accepted and rejected, and during saves of a bundle; reopening must give back every acknowledged change with its marks, authors and times.

## 15. Not in phase 3

- Editing notes and boneyard inside a line: they are kept, and marked in the margin.
- A dual-dialogue editing UI: the `dual` flag is set through the palette.
- The title page editor, spellcheck, and templates other than feature film.
- A settings screen for shortcuts.
- Multiple windows, and a native macOS menu bar.
- Screen readers, and input methods for Chinese, Japanese and Korean (v1.x in `docs/spec.md`).

## 16. One command router

Added 2026-10-16 (phase 4; review finding F18.5). Keys, the palette and its params form, buttons and agents all send what they want done through one function, `App::dispatch(request, origin)`. A request is an app action (§10) or a core command. The origin says who sent it, so it is decided in one place what each source may do and which questions come first:

| Request | From the writer (keys, buttons, palette, params form) | From an agent over the link |
| --- | --- | --- |
| Edits, `undo`, `redo` | The dispatcher, as the writer. | The dispatcher, as the agent (`docs/specs/mcp.md` §5.4). |
| Queries | The dispatcher. | The dispatcher. |
| `doc.open`, `doc.new`, `doc.close`, `doc.revert`, `recovered.restore` | The app's flows: unsaved changes are asked about, agents are disconnected when the path changes, and recent files are updated. | Refused, `not_allowed_attached`. |
| `doc.save` | Save in place runs off the UI thread; with a path, Save as. Losses and changed files are asked about. | In place only, as the agent; with a path, refused. |
| `export.pdf`, `export.fdx`, `export.fountain`, `file.repair` | Through a dialog for the path. With params from the form, as given: a file that exists is refused unless `overwrite` is set, and losses unless `allow_lossy` is. | As given, as the agent. `overwrite` and `allow_lossy` are refused (`writer_only`). |
| `recovered.delete` | Asks first. | Refused, `not_allowed_attached`. |
| `changes.*` (2026-10-18) | The dispatcher, as the writer. | The dispatcher, as the agent, which refuses what agents may not do (`writer_only`, `not_your_change`). |

Before running anything that opens, closes, reverts or replaces the script, the router waits for a save in flight to finish (before phase 4, opening another script during a background save did not wait). Don't save during that save sets the newer changes aside at once and leaves when the save is written; revert goes back to what that save wrote. Whatever the save's outcome, the step that waited then runs, after any banner about the save. An agent's `doc.save` during the writer's save gets core's `save_in_progress` and tries again. While one of the app's questions is open, only that question's keys reach it; an agent's request that would need a question is refused with `not_allowed_attached` and a message saying the writer is being asked something.

## 17. Track changes

Added 2026-10-18 (phase 5a). Sources: `docs/spec.md` ("Revising", "Agents edit in the open"), `docs/specs/changes.md`, `docs/specs/bundle.md`, the phase 5a brief and the decisions taken with the project's owner (2026-10-18, `docs/progress/phase-5a.md`). The keys, the panel and the look are our own choices.

### 17.1 On the page

- **Insertions** are drawn in their author's color. **Deletions** are drawn struck through, in their author's color. Authors take colors in the order they first appear in the script's change table, the same order the PDF uses (`docs/specs/layout.md` §17):

  | Theme | Colors, in order |
  | --- | --- |
  | Light | `#0072B2`, `#D55E00`, `#009E73`, `#CC79A7`, `#3C3C99`, `#8F5A00` (the print colors) |
  | Dark | `#56B4E9`, `#E69F00`, `#3DD6A4`, `#E8A0C8`, `#A8A8FF`, `#D9B26A` |
  | High contrast | `#00E5FF`, `#FF9E40`, `#7CFF7C`, `#FF8CFF`, `#B8B8FF`, `#FFD9A0` |

- **A change bar.** Every line holding a mark gets a thin bar in the left margin, in its author's color: the line's first mark when there are several.
- **Block marks** get a small label in the left margin, in the author's color: "New" for an inserted block, "Deleted" for a deleted one (its text is all struck), "Was Action" (the old type) for a retyped one, "Split" and "Joined" (with a struck ¶ at the end of the line before) for a split or joined block, and "Moved" on a moved scene heading. A deleted empty block, such as a page break, shows its label on an empty line.
- **The navigator** strikes through a scene whose heading is a pending deletion, and marks a scene holding pending changes with a dot in its first author's color.
- **The hint bar** shows "Tracking your changes" while the writer's switch is on.

### 17.2 Settling changes

| Action | Keys | Sends |
| --- | --- | --- |
| Accept the change at the caret | Mod+Shift+Enter | `changes.accept` with that change's `id` |
| Reject the change at the caret | Mod+Shift+Backspace | `changes.reject` with that change's `id` |
| Next or previous change | F7, Shift+F7 | Moves the caret to the start of the next or previous change in document order, round to the other end as find does |
| Accept or reject in this scene | palette, panel | `changes.accept` or `changes.reject` with `scene` |
| Accept or reject all | palette, panel | the same, with `all`, after the question "Accept all 12 changes?" (Enter confirms, Esc cancels) |

- **The change at the caret** is the change of the mark the caret touches: the run just before it, else the one just after it, else the block's own mark. With nothing there, the keys do nothing and say so in a banner. With a selection, the change of its first mark is taken.
- **Each is one undo step**, so Mod+Z gives the change back.
- **A move that cannot be undone** (`change_conflict`) shows a banner saying the scene's old place is gone; the change stays pending.

### 17.3 The changes panel

`view.changes` (Mod+Shift+R, and the palette) shows a panel on the right of the page area, beside the pages, so the writer reviews while reading them.

- **Rows:** one per pending change, in document order (`changes.list`), with a dot in its author's color, the author's name, when it was made, as the recovery dialog says it ("5 minutes ago", "2 days ago": a relative time needs no time zone, so the app takes no date crate), what it does ("Inserted", "Deleted", "Replaced", "New block", "Changed element", "Split", "Joined", "Moved scene"), the scene's number and heading, and a short excerpt: inserted text as is, deleted text struck.
- **The chosen row** is outlined in the focus color rather than filled, so the authors' colors keep their contrast in every theme, and it shows small Accept and Reject buttons for the mouse. A click on a row chooses it, moves the caret to the change and gives the panel the keyboard.
- **Keys** while the panel has the keyboard (F6 moves focus to it):
  - Up and Down choose a row, stopping at the ends;
  - Enter moves the caret to the change, scrolling the page to it;
  - A accepts the change and R rejects it, then the next row is chosen;
  - Esc gives the keyboard back to the page.
- **Buttons** at the top: "Accept all" and "Reject all" (asking first, as above), and a filter with "All authors" and each author.
- **Who is editing** (2026-10-21): under the buttons, "Editing as: Dana", the name the writer's changes carry now ("Writer" with no name set), and a **Change my name…** button (N while the panel has the keyboard) that opens the name question (§17.4).
- **An empty panel** says "No changes to review."
- **The status bar** says "N to review" while changes are pending; a click shows or hides the panel.
- The panel's visibility is kept in `settings.json` (`changes_panel`), as the navigator's is.

### 17.4 The switches and the writer's name

- **Track my changes** (`changes.track_mine`, in the palette and from a click on the hint bar's text) turns the writer's switch on or off (`changes.track` with `writer`). **Track agent edits** (`changes.track_agents`) turns the agents' switch for this script on or off (§12). Each is an undoable step, and a bundle keeps it. A .fountain or .fdx script opens with the defaults: the writer's off and the agents' on.
- **The writer's name.** The writer's changes carry `author_name` from `settings.json`. It is asked for once (decided with the project's owner, 2026-10-18): when the writer turns on Track my changes, or opens a script whose writer switch is on, and no name is set, a small question asks "What name should your changes carry?", with a field prefilled with "Writer". Enter saves the name in the settings and goes on; Esc goes on without saving, and the changes say "Writer" until a name is set. After Esc it is not asked again until the app restarts. Asking at that point, before anything is typed, means a keystroke never waits on the question. The name can be changed later with **Change my name…** in the changes panel (§17.3) or "Change my name for changes…" in the palette (`app.author_name`; before 2026-10-21 "Set my name for changes", in the palette only). Changes already made keep the name they were made with. Enter with the field cleared (2026-10-21) removes the name from the settings, so the writer's changes carry no name again and show as "Writer"; before, it kept the old name.
- **Agents' changes** carry the agent's name (§12).

### 17.5 Saving

- **A bundle** (`.coldopen`) keeps every change. **Save as Cold Open project…** (`app.save_as_project`, in the palette and in the lossy-save question, §11) saves the script as one.
- **A .fountain or .fdx script with pending changes** asks before saving, as for any loss. Under the heading "Fountain can't hold tracked changes" it says (2026-10-21) "Saving accepts all 3 pending changes in the script. The version with them marked is kept in Recovered versions." It offers **Save as Cold Open project…** (P; Enter chooses it whenever tracked changes are among the losses), **Save anyway** (S) and **Cancel** (Esc or C). The project choice is offered for every lossy save of the script itself, since a bundle loses nothing; Enter takes Save anyway when no tracked changes are at stake, as before. "Save a copy as" keeps its two choices: the copy is asked for in that format, and its question says the script keeps its changes.
- **Save anyway accepts the changes** (2026-10-21, decided with the project's owner after the phase 5a manual test). Before, the file held the script with every change accepted while the script on screen kept them pending, so the two differed and every later save asked again. Now the script takes what the file holds, as a converting save does (phase 4 decision 21). `doc.save` with `allow_lossy` sets the version with the marks aside as a Recovered version, then accepts every pending change in front of the writer as one undo step, then saves (`docs/specs/commands.md`, `doc.save`). The page, the navigator, the changes panel and the status bar show the accepted script as soon as the changes are accepted, while the file is still being written, not at the next key or click (2026-10-21). A notice says "Saving accepted 3 pending changes. The version with them marked is in Recovered versions.", with a button that opens the list. Undo brings the marks back, unsaved; restoring the copy opens the marked version as a new script.
- **PDF export** prints the marks by default (`docs/specs/layout.md` §17); "Print the current text" (Mod+3 in the export form, shown while changes are pending) prints the script with every change accepted.
- **The link's session file** lists each open script's `doc_id` when it is a bundle (`docs/specs/mcp.md` §5.1).

### 17.6 Testing

- **Headless tests** (`crates/app/tests/changes.rs`):
  - tracked typing in each theme's colors;
  - the caret over struck text: Left, Right, Shift-selection, typing, Backspace and Enter on each side of it (2026-10-21);
  - accept and reject at the caret and by scene, with undo;
  - the panel's keys;
  - the name question: Enter, Esc, a cleared name, and the panel's Change my name…;
  - the lossy question's Save as Cold Open project;
  - Save anyway accepting the changes, with the marked version kept;
  - a bundle saved and reopened with its changes.

  Screenshots of the panel and the marks in all three themes are reviewed.
- **The agent tests** (`crates/app/tests/agent_link.rs`) attach a real link client, make edits, and check that they arrive tracked under the agent's name, that the writer can accept and reject them, and that the agent cannot settle the writer's.

## 18. Production revisions

Added 2026-10-22 (phase 5b). Sources: `docs/specs/revisions.md`, the phase 5b brief and the decisions taken with the project's owner (2026-10-22, `docs/progress/phase-5b.md`). The form, the keys and the look are our own choices.

- **On the page.** Revision-marked text is drawn in its set's ink for the theme (`docs/specs/revisions.md` §2.3). A tracked-change mark on the same cell wins, in its author's color. Every marked line shows `*` in the page's right margin, and every revised page its revision header, exactly as the PDF prints them.
- **The hint bar** shows the active set, "Blue Rev. 10/22/26", while one is active. A click on it opens the revision menu below.
- **The navigator** shows a small `*` after the heading of a scene holding revision marks.
- **Starting a set.** "Start revision…" (palette) opens a small form with three fields: the color (the next one chosen, Up and Down step through the document's list, and any name may be typed), the label ("<color> Rev.", following the color until it is edited) and the date (today's date in UTC, as `YYYY-MM-DD`). The color field has the keyboard. Enter sends `revision.start`; Esc cancels. With a set active, the form first asks "End Blue Rev. first?" (Enter ends it and goes on, Esc cancels).
- **Ending, colors and clearing.**
  - "End revision" sends `revision.end`.
  - "Revision colors…" opens the list, one name per line, to edit and confirm with Mod+Enter.
  - "Clear revision marks…" asks which set (or all) and confirms: "Clear the marks of Blue Rev.? The set stays in the list." Each is one undo step.
- **Saving.** A .fountain or .fdx script with revision data asks before saving, as for tracked changes (§17.5): "Fountain can't hold revision marks." It says that saving takes them out and that the marked version is kept in Recovered versions. **Save as Cold Open project…** (P) is the default (Enter), **Save anyway** (S) runs `doc.save` with `allow_lossy`, and **Cancel** (Esc or C).
- **PDF export.** The export form gains "Colored pages" (Mod+4, off by default, shown only when the script has revision marks): `revision_pages: "colored"`.

## 19. The title page editor

Added 2026-10-22 (phase 5b; decided with the project's owner: edits are journaled and undoable, never tracked or marked).

- **Opening it.** "Edit title page…" (palette, and Mod+Shift+T) opens a sheet on the right of the page area, with the title page as it prints beside it. The page area scrolls to the title page.
- **Keys** (a Fountain script, or any script whose title page is keys). One field per key, in the title page's order, each holding its values one per line: the usual keys Title, Credit, Author, Source, Draft date, Contact, Notes and Copyright are always shown, empty ones too, and the script's other keys after them. "Add a key…" adds one by name. Tab and Shift+Tab move between fields; leaving a field that changed sends `title.set` for that key, one undo step.
- **Lines** (an .fdx script, or any whose title page is positioned lines). A list of the lines, each with its text, its alignment (Left, Center, Right, chosen with Mod+L, Mod+E and Mod+R) and its space above in lines. Enter adds a line after the current one, and Mod+Backspace removes it. Leaving a line that changed sends `title.set_lines` with the whole list, one undo step.
- **Undo** works from the sheet and the page alike: the steps are labelled "Title page".
- **Esc** closes the sheet, and the page area takes the keyboard back.
- **An agent** editing the title page arrives as any other edit, while agent tracking is off. With it on, the agent's request is refused in core (`writer_only`).

## 20. Reports

Added 2026-10-22 (phase 5b; `docs/specs/reports.md`).

- "Reports…" (palette, and Mod+Shift+I) opens a view in place of the page area, with three tabs: Scenes, Characters and Locations. Ctrl+Tab and Ctrl+Shift+Tab change tabs.
- Each tab is a table with the report's fields as columns, and counts right-aligned. Up and Down choose a row, stopping at the ends; Enter goes to the row's scene (a character's first speech, a location's first scene) and closes the view; Esc closes it.
- "Copy as CSV" (Mod+C in the view) puts the tab's CSV (`docs/specs/reports.md` §5) on the clipboard.
- The view refreshes when the navigator does.

## 21. Comparing drafts

Added 2026-10-22 (phase 5b; `docs/specs/diff.md`).

- **Compare with…** (palette) asks for a draft (.fountain, .fdx or .coldopen), then runs `diff` with the open script as A, as it stands now, and the draft as B. It opens a read-only view in place of the page area: B's pages with the changes marked as the diff PDF marks them, and a list of the changed scenes on the left, in place of the navigator. Up and Down choose a scene, Enter scrolls to it, "Save as PDF…" (Mod+Shift+E in the view) writes the diff PDF, and Esc closes the view. The open script is not changed, and nothing can be typed into the view.
- **Review a draft as changes…** (palette) asks for a draft and the name its changes should carry, prefilled with the file's name, then sends `changes.import` and opens the changes panel (§17.3). What could not be brought in as changes (styles, attributes, the title page) is listed in a notice, and the draft itself is never changed.
