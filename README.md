# Cold Open

Cold Open is a free, open-source screenwriting app. It reads and writes Fountain and Final Draft (.fdx) files and never loses your work. Every action is a command, and you can drive it from the desktop app, the command line, JSON over stdio, or an MCP server.

**Status:** phase 3 of 6: the desktop editor and the MCP server, on top of the core, the command-line tool, pagination and PDF export. Full .fdx support comes next. See [`docs/spec.md`](docs/spec.md) for the full plan, and [`docs/progress/phase-3.md`](docs/progress/phase-3.md) for where phase 3 stands.

## Build

You need Rust 1.89 or newer. Install it from <https://rustup.rs>, then build:

```bash
cargo build --release
```

This builds two programs in `target/release`: `coldopen-app`, the desktop app, and `coldopen`, the command-line tool, which also runs the MCP server. Both run on Windows, macOS and Linux (`.exe` on Windows). On Linux the app needs X11 or Wayland, and libxkbcommon, at run time.

Test builds for every OS, unsigned, come from CI: open a run of the CI workflow on the `phase-3-editor-mcp` branch and download the `cold-open-<os>` artifact. Each has a `TESTING.txt` that says how to start it and what to try.

## Use the desktop app

```bash
coldopen-app [script.fountain]
```

- **Writing.** A new script starts on a scene heading. Enter and Tab move between elements as screenwriting expects (heading, action, character, dialogue, parenthetical, transition); the bar at the top says what they will give next. Typing in headings, character names and transitions comes out in capitals. Autocomplete offers character names, `INT.`/`EXT.`, locations, times of day and extensions; Tab or Enter takes a suggestion.
- **Everything from the keyboard.** Ctrl+K (Cmd+K on macOS) opens the command palette, which lists every command, every app action and their shortcuts, and recent files.
- **Pages.** The script is shown on pages exactly as it prints, or in a continuous draft view (Ctrl+Shift+D). The navigator on the left lists scenes; drag one, or press Alt+Up/Down, to move it.
- **Files.** Ctrl+S saves (in the background, without pausing your typing), Ctrl+Shift+E exports a PDF, and closing asks about unsaved changes.
- **Your own keys and element flow.** `keymap.json` and `flow.json` in the settings folder change shortcuts and what Enter and Tab do; see [`docs/specs/editor.md`](docs/specs/editor.md) §5.2 and §10. The app never rewrites them.
- **Drawing.** The app draws with the CPU on Windows and Linux and with the GPU on macOS. The GPU is smoother on 4K and Retina screens; the CPU uses far less memory. The palette switches between them.

The full behaviour is in [`docs/specs/editor.md`](docs/specs/editor.md).

## Let an AI agent edit your script

```bash
claude mcp add coldopen -- coldopen mcp
```

`coldopen mcp` is an MCP server: every command is a tool (`block_insert`, `text_replace`, `scene_list` and so on), and the script, its blocks, pages and scenes are resources. Any MCP client works the same way.

When the script is open in the desktop app, the agent's `doc_open` attaches to the app instead of opening the file: you see each edit appear, a violet bar marks the blocks it changed, and Ctrl+Z undoes it like your own. Each agent step is labelled with the agent's name and the tool it used, and an agent can only undo its own steps. The status bar shows a connected agent, with Disconnect and a switch that turns agent access off.

The link between them is private to your user account on your machine: a socket in a folder only you can open, or a Windows pipe only you can reach, plus a fresh 256-bit token. Without the app, `coldopen mcp` opens the file itself, with the same crash journal. Details: [`docs/specs/mcp.md`](docs/specs/mcp.md).

## Use the command line

### Convert a script

```bash
coldopen convert draft.fountain clean.fountain
```

This rewrites a script as canonical Fountain. The meaning is unchanged; spacing and markup are tidied.

To print the document's blocks and ids as JSON instead:

```bash
coldopen convert draft.fountain - --to json
```

Use `-` as the output to write to standard output. If the target format cannot hold everything in the script, nothing is written unless you add `--allow-lossy`.

### List scenes

```bash
coldopen scenes draft.fountain
```

The output looks like this:

```
   1  1    p. 1     3/8  EXT. LIGHTHOUSE - DUSK  (15 blocks, 39 words)
   2  2    p. 1     2/8  INT. RADIO ROOM - NIGHT  (5 blocks, 18 words)
```

Each line shows the page the scene starts on and its length in eighths of a page. Add `--json` for machine-readable output.

### Export a PDF

```bash
coldopen export draft.fountain draft.pdf
coldopen export draft.fountain review.pdf --watermark "DRAFT" --scene-numbers
```

The PDF uses the bundled Courier Prime font and comes out byte for byte the same on Windows, macOS and Linux. `--no-title-page` leaves out the title page. Characters the font has no glyph for (for example Chinese or emoji) print as a box, and the command says which.

### See the pages as text

```bash
coldopen pages draft.fountain
```

prints every page the way it prints, as plain text on a 10-characters-per-inch grid, with a header line per page saying why it ended. It is handy for checking page breaks.

### Licenses

`coldopen licenses` prints Cold Open's license and the SIL Open Font License of the bundled fonts. The app shows them under About in the palette.

### Run a batch of edits

`edits.json` is a JSON array of commands, or one command per line:

```json
[
  {"command": "text.replace", "params": {"find": "MAYA", "with": "NINA"}},
  {"command": "block.insert", "params": {"type": "transition", "text": "FADE OUT."}}
]
```

Run it against a script:

```bash
coldopen run edits.json --doc draft.fountain --save
```

- `run` prints one JSON response per command.
- It stops at the first failure unless you add `--keep-going`.
- It exits with status 1 if any command failed.
- Without `--save` (or a `doc.save` in the batch), changes are discarded when the batch ends.
- Unsaved work recovered from a crash journal is never thrown away. If the batch leaves it untouched, it stays in the journal for the next open; if the batch changed it, a copy of the journal is kept, and `run` says where.

### Serve commands over stdio

```bash
coldopen serve --stdio
```

The server reads one JSON request per line and answers each with one JSON response line. Any language can drive it this way:

```
→ {"v":1,"id":1,"command":"doc.open","params":{"path":"draft.fountain"}}
← {"v":1,"id":1,"ok":true,"result":{"blocks":42,"format":"fountain","path":"draft.fountain","recovered":null,"scenes":6,"warnings":[]}}
→ {"v":1,"id":2,"command":"block.insert","params":{"after":"b_3","type":"character","text":"MAYA"}}
← {"v":1,"id":2,"ok":true,"result":{"id":"b_43"}}
→ {"v":1,"id":3,"command":"undo"}
← {"v":1,"id":3,"ok":true,"result":{"undone":"block.insert"}}
```

The full command list, parameters and error codes are in [`docs/specs/commands.md`](docs/specs/commands.md).

## Your work is safe

**The crash journal.** While a script is open, every change is written to a crash journal before the command returns. For `draft.fountain`, the journal is the hidden file `.draft.fountain.coldopen-journal` next to it.

- If Cold Open crashes, or the machine loses power, the next `doc.open` replays the journal and returns `"recovered": {"commands": N}`. Your undo history comes back too. The desktop app shows what came back and lets you keep it or go back to the saved file.
- In the desktop app, every key is written to the journal before it shows, and flushed to disk within 250 ms. Ending the app at any moment loses nothing that was on screen; a power cut loses at most the last quarter of a second. A script you never saved has a journal too, and the app offers it at the next start.
- Saving resets the journal, and closing a saved document deletes it.
- If the script is in a git repository, add `.*.coldopen-journal` to your `.gitignore`.

**Atomic saves.** Saves are atomic: a crash mid-save leaves the old file or the new one, never half of each.

The details are in [`docs/specs/journal.md`](docs/specs/journal.md).

## Fountain support

Cold Open supports everything on the [Fountain syntax page](https://fountain.io/syntax):

- the title page
- scene headings, including forced headings and scene numbers, plus the reversed `EXT/INT` and `E/I` prefixes that some writers use
- action, characters, dialogue, parentheticals and dual dialogue
- lyrics, transitions and centered text
- emphasis
- notes, boneyard, sections and synopses
- page breaks

Every rule, and how we settle the cases the syntax page leaves open, is written down in [`docs/specs/fountain.md`](docs/specs/fountain.md).

## Test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The test suite includes:

- **Fountain conformance cases.** One case per rule, in [`tests/fixtures/fountain`](tests/fixtures/fountain), with expected results written by hand.
- **Property tests.** Random Fountain and random edit sequences check round trips, undo, redo and journal replay.
- **A crash test.** It kills `coldopen serve --stdio` in the middle of a stream of edits, then checks that a new process recovers every edit that had been acknowledged.
- **The app's gates.** The app's own tests run it headless, with real key presses through the real interface. The typing gate types a short scene with smart flow, autocomplete, accents, an undo and a save, and compares the file byte for byte; the crash gate kills the app as a separate process at random points mid-typing and checks the recovery; the agent gate drives a real `coldopen mcp` process against the app.

### Private corpus

Real scripts are copyrighted, so they are never committed. Put any `.fountain` files under `corpus/` (which is gitignored), or point `COLDOPEN_CORPUS` at another folder. The corpus test then checks that each script round-trips without loss. Without a corpus, that test is skipped.

### Fuzzing

The `fuzz/` folder holds cargo-fuzz targets. They need nightly Rust and run on Linux or macOS:

```bash
cd fuzz
cargo +nightly fuzz run fountain_roundtrip
```

The other targets are `envelope`, `journal_replay`, `fdx_read`, `link_line` and `app_config`. A scheduled workflow runs all six every Sunday; you can also start it by hand from the Actions tab.

## Layout

| Crate | License | What it holds |
| --- | --- | --- |
| `crates/screenplay` | MIT OR Apache-2.0 | Screenplay types: elements, rich text, title page |
| `crates/fountain` | MIT OR Apache-2.0 | The Fountain parser and writer |
| `crates/fonts` | OFL-1.1 (fonts), MIT OR Apache-2.0 (code) | The bundled fonts: Courier Prime for the page, Atkinson Hyperlegible Next for the app's interface |
| `crates/layout` | MIT OR Apache-2.0 | Layout on a character grid, pagination, page view and PDF export |
| `crates/fdx` | MIT OR Apache-2.0 | A read-only .fdx reader (phase 2); writing and repair come in phase 4 |
| `crates/core` | GPL-3.0-or-later | Document, commands, dispatcher, undo, journal, atomic save |
| `crates/cli` | GPL-3.0-or-later | The `coldopen` binary |
| `crates/app` | GPL-3.0-or-later | The desktop app, `coldopen-app` |
| `crates/mcp` | GPL-3.0-or-later | The MCP server behind `coldopen mcp` |
| `crates/link` | GPL-3.0-or-later | The private link between the app and `coldopen mcp` |

The format libraries are permissively licensed so other apps can use them. The app itself is GPL-3.0-or-later; see [LICENSE](LICENSE). The Courier Prime and Atkinson Hyperlegible Next fonts are under the SIL Open Font License 1.1 (`crates/fonts/fonts/`); PDFs you export are yours and are not covered by it.

`tools/refpdf` is a development tool that measures reference PDFs for the layout tests; it is never shipped.
