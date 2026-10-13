# Cold Open

Cold Open is a free, open-source screenwriting app. It reads and writes Fountain and Final Draft (.fdx) files and never loses your work. Every action is a command, and you can drive it from the desktop app, the command line, JSON over stdio, or an MCP server.

**Status:** phase 1 of 6, the core and the command-line tool. The desktop app, PDF output and .fdx support come in later phases. See [`docs/spec.md`](docs/spec.md) for the full plan.

## Build

You need Rust 1.89 or newer. Install it from <https://rustup.rs>, then build:

```bash
cargo build --release
```

The binary is `target/release/coldopen` (`coldopen.exe` on Windows). It runs on Windows, macOS and Linux.

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
   1  1  EXT. LIGHTHOUSE - DUSK  (15 blocks, 39 words)
   2  2  INT. RADIO ROOM - NIGHT  (5 blocks, 18 words)
```

Add `--json` for machine-readable output.

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

- If Cold Open crashes, or the machine loses power, the next `doc.open` replays the journal and returns `"recovered": {"commands": N}`. Your undo history comes back too.
- Saving resets the journal, and closing a saved document deletes it.
- If the script is in a git repository, add `.*.coldopen-journal` to your `.gitignore`.

**Atomic saves.** Saves are atomic: a crash mid-save leaves the old file or the new one, never half of each.

The details are in [`docs/specs/journal.md`](docs/specs/journal.md).

## Fountain support

Cold Open supports everything on the [Fountain syntax page](https://fountain.io/syntax):

- the title page
- scene headings, including forced headings and scene numbers
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

### Private corpus

Real scripts are copyrighted, so they are never committed. Put any `.fountain` files under `corpus/` (which is gitignored), or point `COLDOPEN_CORPUS` at another folder. The corpus test then checks that each script round-trips without loss. Without a corpus, that test is skipped.

### Fuzzing

The `fuzz/` folder holds cargo-fuzz targets. They need nightly Rust and run on Linux or macOS:

```bash
cd fuzz
cargo +nightly fuzz run fountain_roundtrip
```

The other targets are `envelope` and `journal_replay`. A scheduled workflow runs all three every Sunday; you can also start it by hand from the Actions tab.

## Layout

| Crate | License | What it holds |
| --- | --- | --- |
| `crates/screenplay` | MIT OR Apache-2.0 | Screenplay types: elements, rich text, title page |
| `crates/fountain` | MIT OR Apache-2.0 | The Fountain parser and writer |
| `crates/core` | GPL-3.0-or-later | Document, commands, dispatcher, undo, journal, atomic save |
| `crates/cli` | GPL-3.0-or-later | The `coldopen` binary |

The format libraries are permissively licensed so other apps can use them. The app itself is GPL-3.0-or-later; see [LICENSE](LICENSE).
