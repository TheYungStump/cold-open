# Cold Open — notes for AI agents

> The product name is **Cold Open**. The binary and crate prefix are `coldopen`.

A free, open-source screenwriting app in Rust. It reads and writes Fountain and Final Draft (.fdx) files, never loses work, and every action is a command reachable from the GUI, a CLI, JSON over stdio and an MCP server.

**Source of truth:** `docs/spec.md`. Read it before starting any phase. The two diagrams in the spec do not survive Markdown export, so they are written out below.

## Architecture

Four front ends (desktop app, CLI, JSON over stdio, MCP server) send the same commands to one dispatcher. The dispatcher validates each command, applies it to the document model, records undo and appends it to the journal. Below it sit four core pieces: the document model, the layout engine, the format adapters and storage.

Front ends never mutate the document directly. If something can be done in the GUI, it exists as a `Command` first.

### Workspace layout

| Crate | License | Phase | Contents |
| --- | --- | --- | --- |
| `crates/screenplay` | MIT OR Apache-2.0 | 1 | Shared screenplay types: element kinds, title page, a plain document AST. No app logic. |
| `crates/fountain` | MIT OR Apache-2.0 | 1 | Fountain parser and writer over `screenplay` types. |
| `crates/core` | GPL-3.0-or-later | 1 | Document with stable element ids, `Command` enum, dispatcher, undo/redo, command journal, atomic save, version snapshots. |
| `crates/cli` | GPL-3.0-or-later | 1 | The `coldopen` binary: subcommands plus `serve --stdio`. |
| `crates/fonts` | OFL-1.1 AND (MIT OR Apache-2.0) | 2, 3 | The bundled fonts and their licenses, nothing else: Courier Prime for the page, and (phase 3, `ui` feature) Atkinson Hyperlegible Next subset to Latin for the app's interface. |
| `crates/layout` | MIT OR Apache-2.0 | 2 | Character-grid layout, pagination rules, page view, PDF export. |
| `crates/fdx` | MIT OR Apache-2.0 | 2, 4 | .fdx reader (phase 2: read-only, for the layout gate), then writer, unknown-element passthrough, repair report (phase 4). |
| `crates/app` | GPL-3.0-or-later | 3 | iced desktop app with a custom page-view editor widget. |
| `crates/mcp` | GPL-3.0-or-later | 3 | MCP server built on `rmcp`, run as `coldopen mcp`. |
| `crates/link` | GPL-3.0-or-later | 3 | The private agent link between the app and `coldopen mcp`: session files, handshake, same-user socket or pipe. Keeps rmcp and tokio out of the app. |

The MIT/Apache crates must never depend on GPL crates.

## Milestones

Each phase starts only after the previous gate passes.

1. **Core and CLI** — dispatcher, document model, Fountain read/write, CLI, JSON over stdio. Gate: passes our own Fountain test cases, written from the public syntax page at fountain.io.
2. **Layout and PDF** — layout engine with bundled fonts, page-break rules, PDF export, cross-platform determinism tests, a minimal read-only .fdx loader for the reference tests, and the `EXT/INT` and `E/I` scene-heading fix. Gate: layout and PDF goldens identical on Windows, macOS and Linux, and the reference comparison against the Final Draft PDFs meets the thresholds in `docs/specs/layout.md` (wraps ≥ 99%, resynced breaks ≥ 95% exact and all within 2 lines, page counts exact or ±1 on Big Fish, scene starts ≥ 90% exact and all within 1 page).
3. **Desktop editor and MCP** — editor window, smart element flow, autocomplete, scene navigator, atomic saves, crash journal, MCP server. Gate: a full short script written with no crash and no lost text.
4. **.fdx exchange** — import/export, unknown-element passthrough, repair report and `repair` command. Gate: lossless round trip on the .fdx test corpus.
5. **Revisions and output** — track changes, revision colors, script diff, title page, reports. Gate: track changes survive save, reload and .fdx export.
6. **Alpha release** — builds for all three OSes, user docs, website, tester group. Gate: every size and speed budget in the spec met on all three OSes.

## Rules

### Files you didn't create (non-negotiable)

- Never delete, move or overwrite a file or folder you didn't create in this session; ask first.
- This includes untracked files in the repo. A test build downloaded from CI into the repo root may be the user's, and may be running.

### Clean room (non-negotiable)

- Never download, install, run or script Final Draft, including its trial and Reader Mode.
- Never copy text from Final Draft's website, manuals or help articles into code, comments, docs or prompts.
- Knowledge of .fdx comes only from publicly shared sample files and from our own behavior specs in `docs/specs/`.
- Before implementing a format behavior, write or update a spec in `docs/specs/` in our own words, dated, listing its sources. Code cites the spec, never Final Draft material.
- No Final Draft fonts, templates, icons or artwork. We bundle Courier Prime (SIL OFL).

### Test data

- Real scripts are copyrighted. They live only in `corpus/`, which is gitignored. Tests that need them skip cleanly when it is missing.
- Public fixtures in `tests/fixtures/` are short scenes we write ourselves.
- Manual tests, yours and the project owner's, use copies of corpus files in a scratch folder, never the files in `corpus/`. Phase 4's manual test saved over `corpus/fountain-io/Brick-&-Steel.fdx`.

### Commands

- Every command is a `serde`-serializable variant with a stable, dotted name matching the spec (`block.insert`, `scene.move`, `doc.save` and so on).
- The JSON envelope is versioned: `{"v": 1, "id": ..., "command": ..., "params": {...}}`. Responses echo `id`.
- Commands are undoable unless documented otherwise, and every applied command is journaled before the UI sees the result.

### Reliability

- Saves are atomic: write a temp file in the same directory, flush and fsync, then rename over the original.
- The journal must let a crashed session be replayed on top of the last save.
- Parsers never panic on bad input; they return errors or recover with a report. Add a `cargo-fuzz` target for each parser.

### Determinism and performance

- Output must not depend on OS, locale, system fonts or hash-map iteration order. Use `BTreeMap` or sorted output wherever order is visible.
- Respect the size and speed budgets in the spec. Justify every new dependency in the PR description, and prefer small crates with permissive licenses.

### Quality

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` must pass before a commit.
- CI runs on `windows-latest`, `macos-latest` and `ubuntu-latest`.
- Keep commits small and focused, with messages that say why.
