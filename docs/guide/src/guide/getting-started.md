# Getting started

## The window

| Part | What it shows |
| --- | --- |
| **The hint bar** (top) | The script's name, the element the caret is in, and what Enter and Tab will make next |
| **The navigator** (left) | The scenes and sections, with each scene's page and length |
| **The pages** (middle) | The script exactly as it prints |
| **The status bar** (bottom) | The page you're on and the page count, the scene and its length, whether your work is protected, and the zoom |

Cold Open starts on a new script, with the caret on an empty scene heading. Everything you type is saved to a crash journal as you type it, so an untitled script is safe even before you save it.

## A first scene

Type this, pressing the keys shown in brackets:

1. `int. lighthouse - night` **[Enter]**. Scene headings come out in capitals. As you type, Cold Open offers `INT.`, places you've used and times of day: Tab or Enter takes a suggestion.
2. `The lamp turns. Waves break below.` **[Enter]** **[Tab]**. Enter after action gives more action; Tab turns it into a character.
3. `nora` **[Enter]**. Character names come out in capitals, and Enter goes to dialogue.
4. `Still turning. Still here.` **[Enter]**. Enter after dialogue goes to the next character.
5. `eli` **[Enter]** **[Tab]** `from below` **[Enter]**. Tab in dialogue makes a parenthetical, with its brackets added for you. Enter goes back to dialogue.
6. `Come down, Nora.` **[Enter]** **[Enter]**. Enter on an empty character goes to action.

You've written a scene. Press **Mod+S** to save it: an untitled script asks where, and the name's extension picks the format (`.fountain` unless you choose another).

## The element flow

Enter and Tab always do what the hint bar says. Here is the whole table:

| You're in | Enter makes | Enter on an empty one makes | Tab makes | Shift+Tab makes |
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

A few things happen by themselves, and one undo (Mod+Z) takes each back:

- Typing `INT.`, `EXT.`, `I/E` and the like at the start of an empty action makes it a scene heading.
- Typing `(` in an empty dialogue makes it a parenthetical.
- An action in capitals that ends in `TO:` becomes a transition when you press Enter.

**Mod+1 to Mod+8** set the element directly: 1 scene heading, 2 action, 3 character, 4 parenthetical, 5 dialogue, 6 transition, 7 shot, 8 general.

**Shift+Enter** starts a new line inside action or dialogue, without starting a new element.

## Autocomplete

While you type, a short list offers what fits:

- **character names** you've used, the ones in the current scene first;
- **extensions** after a `(` in a character: `V.O.`, `O.S.`, `O.C.`, `CONT'D` and any you've used;
- **scene prefixes** (`INT.`, `EXT.`, `INT./EXT.`, `EST.`, `I/E.`), **places** you've used, and **times of day** after ` - `;
- **transitions**: `CUT TO:`, `DISSOLVE TO:` and the ones you've used.

Up and Down choose, Tab or Enter takes it, and Esc closes the list.

## Moving around

| Keys | Move to |
| --- | --- |
| Left, Right | the previous or next character |
| Ctrl+Left, Ctrl+Right (Mac: Option+Left, Option+Right) | the previous or next word |
| Up, Down | the line above or below |
| Home, End (Mac: Cmd+Left, Cmd+Right) | the start or end of the line |
| Mod+Home, Mod+End (Mac: Cmd+Up, Cmd+Down) | the start or end of the script |
| Page Up, Page Down | a screen up or down |
| Alt+Page Up, Alt+Page Down | the previous or next scene |
| Mod+Alt+Up, Mod+Alt+Down | the previous or next element |

Shift with any of these selects. A double click selects a word, a triple click an element.

**The navigator** lists every scene with its number, page and length in eighths of a page. Click a scene to go to it. Drag a scene to move it, or select it and press **Alt+Up** or **Alt+Down**. Mod+\ shows or hides the navigator, and F6 moves the keyboard between the navigator, the page and any open panel.

**Find and replace:** Mod+F finds, and Mod+H (Mac: Cmd+Option+F) finds and replaces. Enter, F3 or Mod+G goes to the next match. "This scene only" limits it to one scene. Replace all is one undo step.

## The palette, and everything else

**Mod+K** opens the command palette. It lists every action, with its keys, and your recent files. Type a few letters of what you want: "pdf", "reports", "dark". The [keyboard reference](../reference/keyboard.md) lists everything it holds.

## Views

- **Page view** shows the pages as they print. **Draft view** (Mod+Shift+D) is one long column with no page breaks, which also shows sections and synopses as grey lines.
- **Zoom:** Mod+= and Mod+- (Mod+0 for 100%). A page always fits the window's width.
- **Focus mode** (Mod+Shift+F) hides everything but the page.
- **Themes:** light, dark and high contrast, in the palette. The default follows your system.
- **The renderer:** Cold Open draws with the CPU on Windows and Linux, and with the GPU on macOS. The GPU is smoother on 4K and Retina screens; the CPU uses less memory. "Draw with the GPU" and "Draw with the CPU" in the palette switch, from the next start.

## Your own keys

`keymap.json` in the settings folder (see [Install](install.md#where-cold-open-keeps-things)) changes any shortcut. It holds only your changes, and Cold Open never rewrites it, so an upgrade never undoes them:

```json
{"v": 1, "bindings": [
  {"keys": "Mod+Shift+K", "action": "app.palette"},
  {"keys": "Mod+1", "action": null}
]}
```

`null` removes a shortcut. The action names are in the [keyboard reference](../reference/keyboard.md). Keys are written with `Mod`, `Ctrl`, `Alt`, `Shift` and `Super` (Cmd on a Mac), then a letter, digit, `F1`–`F24`, a named key such as `Enter` or `PageUp`, or one punctuation character. A mistake in the file is listed in a notice when Cold Open starts, and never stops it starting.

`flow.json` changes the element flow the same way:

```json
{"v": 1, "kinds": {"dialogue": {"enter": "action"}, "action": {"tab": "transition"}}}
```

The fields are `enter`, `empty_enter`, `tab` and `shift_tab`.

## Saving

- **Mod+S** saves in the background, so typing never waits for it.
- **Save as** (Mod+Shift+S) saves under another name, and converts when the extension is another format.
- A script saves back to the format it came in. If you used something that format can't hold (tracked changes in a Fountain file, say), Cold Open asks before saving, and offers a Cold Open project, which keeps everything. See [Files](files.md).
- **Closing** with unsaved changes asks first.
