# Screenwriting App — Product Spec v0.1

Oct 13, 2026 · @Brandon WIlde

## Overview

We are building a free, open-source screenwriting app that reads and writes Final Draft (.fdx) files, never loses work, and can be driven by people, scripts or AI agents through one set of commands. Name: **Cold Open** (command-line name coldopen; trademark search still pending).

It is a clean-room reimplementation: we match what screenwriters need from Final Draft, built from public documentation and our own specs, not from Final Draft's code or assets.

Goals that shape every decision:

- **Small:** an installer under 15 MB, against roughly 116 MB for Final Draft's Mac download.
- **Fast:** opens instantly, types without lag, saves without pauses.
- **Everywhere:** native on Windows, macOS and Linux from the first release.
- **Command-driven:** every action is a command, reachable from the GUI, a CLI, JSON over stdin/stdout, and an MCP server.
- **Interoperable:** opens and saves .fdx and PDF the way producers expect, with Fountain as the native format.
- **Yours:** no account, no activation, unlimited machines, works offline.

Non-goal: a pixel-for-pixel copy of Final Draft's interface. We copy the job it does, not its look.

## Problems we're solving

Beyond price, the loudest complaints about Final Draft are crashes, broken files, weak track changes and license limits. Each one maps to a feature in this spec.

| Pain point | What users report | Our answer |
| --- | --- | --- |
| Crashes and freezes | A popular r/Screenwriting thread describes constant crashes on Windows despite reinstalls ([thread](https://cal1.lr.ggtyler.dev/r/Screenwriting/comments/1jcpclv/i_hate_final_draft)); reviewers report freezes when leaving revision mode or using dual dialogue ([GetApp](https://www.getapp.com.au/software/2050325/final-draft)) | Command journal and atomic saves, so a crash loses at most the last second; fuzzed parsers |
| Slow saves and loads | One reviewer measured auto-saves of up to 20 seconds, 30-second loads and crashes as often as daily ([AWN](https://awn.com/blog/whats-best-screenwriting-software)) | Autosave under 10 ms off the UI thread; a 120-page script opens in under 200 ms |
| Corrupted files | The official fix is to export plain text and re-import, losing formatting ([Final Draft KB](https://kb.finaldraft.com/hc/en-us/articles/15575131452692)); users suspect shared .fdx files trigger crashes | Tolerant .fdx import with a repair report, plus a `repair` CLI command |
| Weak track changes | Added text is colored but deleted text is not tracked ([Writing Codidact](https://writing.codidact.com/posts/5771)) | Full track changes with deletions, authors and accept/reject; scene-aware script diff |
| Shortcuts changed between versions | An upgrade broke a reviewer's shortcut keys ([AWN](https://awn.com/blog/whats-best-screenwriting-software)) | User-editable shortcuts that carry forward across upgrades |
| Two-machine limit | One license runs on two computers at once; others open read-only, and signing in or out needs internet ([Final Draft KB](https://kb.finaldraft.com/hc/en-us/articles/15575351417620-On-how-many-computers-can-I-install-Final-Draft)) | No activation or account; install anywhere |
| Old versions expire | Discontinued activation means old copies die on an OS upgrade or new drive ([Final Draft KB](https://kb.finaldraft.com/hc/en-us/articles/28991324279444-I-m-getting-an-error-that-says-The-Customer-Number-You-ve-Entered-Is-Out-Of-Activations)) | Open source: any version keeps working forever |
| No Linux, no Android | Linux users say it does not run at all; others want to write on phones ([thread](https://cal1.lr.ggtyler.dev/r/Screenwriting/comments/1jcpclv/i_hate_final_draft)) | Linux in v1; the Rust core is built to compile to mobile and web later |
| Large download | Roughly 116 MB on Mac ([FileHorse](https://mac.filehorse.com/es/download-final-draft/26287/)) | Under 15 MB |

## V1 feature set

Version 1 covers everything a writer needs to draft, revise and deliver a script; production-only tools come in v1.1.

**Writing**

- Element types: Scene Heading, Action, Character, Parenthetical, Dialogue, Transition, Shot, General, centered text and page breaks.
- Smart element flow: Enter and Tab move to the likely next element (Scene Heading to Action, Character to Dialogue, and so on), editable per template.
- Autocomplete for character names, locations, INT./EXT., times of day and extensions such as (V.O.), (O.S.) and (CONT'D).
- Automatic (MORE) and (CONT'D) at page breaks and for a character speaking again after action.
- Dual dialogue.
- Inline notes anchored to text, and an omit-from-output "boneyard" for cut material.
- Unlimited undo and redo, find and replace, and a script-aware spellchecker that knows character and location names.

**Structure and navigation**

- Scene navigator with headings, page numbers, lengths and a one-line synopsis per scene.
- Drag scenes to reorder them in the navigator.
- Optional sections (acts, sequences) that never print.
- Scene numbering, with locked numbers (12A, 12B) once a script is in production.

**Revising**

- Track changes: insertions and deletions, author and time, accept or reject (details under Reliability).
- Production revision marks: colored revision sets (Blue, Pink, Yellow and so on) with asterisks in the margin and revised-page headers.
- Script diff between any two drafts.

**Output**

- Title page editor.
- PDF export with optional watermark, revision colors and scene numbers.
- Export to .fdx, Fountain and plain text.
- Reports: scenes, characters (lines and words per character) and locations.

**Comfort**

- Page view and a continuous "draft" view, zoom, dark mode and a distraction-free mode.
- Command palette that lists every command.
- Editable keyboard shortcuts that survive upgrades; every function reachable from the keyboard; a high-contrast theme.

**Templates:** feature film, one-hour TV drama and multi-camera sitcom.

## Formatting and pagination rules

The layout engine must produce the same pages on every OS, so a script is the same length for everyone who opens it. We get there by bundling our own fonts and doing our own text layout instead of relying on the system.

**Page defaults** (US Letter, feature template; all values configurable per template). Sources: [Highland's format guide](https://blog.quoteunquoteapps.com/standard-screenplay-format-the-writers-guide), [NFI](https://www.nfi.edu/screenplay-format).

| Setting | Default |
| --- | --- |
| Font | 12 pt Courier-style monospace; we bundle Courier Prime (SIL Open Font License) |
| Lines per page | About 55, excluding the page number |
| Margins | 1.5 in left, 1 in right (ragged), 1 in top and bottom |
| Scene heading, action | Start 1.5 in from the left edge, full width |
| Character name | 3.7 in from the left edge, all caps |
| Parenthetical | 3.1 in from the left edge, about 2 in wide |
| Dialogue | 2.5 in from the left edge, about 3.5 in wide |
| Transition | Right side, ending at the right margin (about 6 in from the left edge) |
| Page number | Top right, 0.5 in from the top, followed by a period; title page unnumbered, first numbered page is 2 |

A4 uses the same left and right positions with adjusted top and bottom margins, so line counts stay comparable.

**Page-break rules**

1. A scene heading never ends a page; it moves to the next page with at least two lines of what follows.
2. A character name never ends a page; it stays with at least two lines of its dialogue.
3. Dialogue splits only at a sentence boundary, with at least two lines on each side, (MORE) at the foot and the name plus (CONT'D) at the top of the next page.
4. A parenthetical never ends a page or starts one alone.
5. Action splits only at a sentence boundary, with at least two lines on each side.
6. A transition stays with the block before it.
7. Dual dialogue never splits across pages.

**Determinism tests:** a corpus of reference scripts with expected page breaks runs in CI on Windows, macOS and Linux; any difference fails the build. Exact parity with Final Draft's page counts is a goal measured against our own reference PDFs (see Open questions).

## Reliability features

The promise is simple: a crash or a bad file never costs a writer more than the last second of typing.

1. **Command journal.** Every edit is a command, appended to a journal file as it happens. After a crash, the app replays the journal on top of the last save, so nothing typed before the crash is lost.
2. **Atomic saves.** Saving writes a temporary file in the same folder, flushes it to disk, then swaps it in with a single rename. A crash mid-save leaves the previous version intact, never a half-written file.
3. **Fast, invisible autosave.** Autosave runs off the UI thread and finishes in under 10 ms, so it never interrupts typing.
4. **Version history.** Timestamped snapshots of the script, browsable and restorable from the app, with automatic pruning of old ones.
5. **Tolerant .fdx import.** The importer recovers what it can from malformed XML, keeps any elements it does not understand so they survive a round trip, and shows a short report of what it fixed. The same logic runs as `coldopen repair broken.fdx`, useful even for people who stay on Final Draft.
6. **Track changes.** Records insertions and deletions with author and time; deletions show as strikethrough. Changes can be accepted or rejected one at a time, by scene or all at once. This is separate from production revision colors, which mark what changed between distributed drafts.
7. **Script diff.** Compares any two drafts scene by scene, detects moved scenes, and outputs a readable summary, JSON, or a PDF with changes marked. Available in the app and as `coldopen diff draft2.fdx draft3.fdx`.
8. **Hardened parsers.** Fountain and .fdx parsers are fuzz-tested continuously, and pagination has property tests, so malformed input fails gracefully instead of crashing.
9. **Opt-in crash reports** that never include script text.

## File formats

Fountain is the native script format, a project bundle holds everything Fountain cannot, and .fdx plus PDF handle exchange with the industry.

| Format | Role | v1 support | Notes |
| --- | --- | --- | --- |
| Fountain (.fountain) | Native script text | Read and write | Open plain-text spec ([fountain.io](https://fountain.io)); readable without the app, diffs cleanly in git, easy for AI agents to read. Covers notes, boneyard, sections, synopses, dual dialogue and title pages. |
| Project bundle (.coldopen, name TBD) | Full project | Read and write | A zip holding the Fountain script plus JSON for track changes, revision sets, notes, settings and the command journal. Open format, documented in the repo. |
| Final Draft (.fdx) | Exchange with producers and other apps | Read and write | XML. Goal: lossless round trip for every feature we support; unknown elements are preserved untouched. |
| PDF | Reading, printing, submission | Export | Matches our pagination exactly; optional watermark, scene numbers and revision colors. Import comes later. |
| Plain text | Fallback | Read and write | For pasting from other tools. |

Rules:

- Opening a .fountain file saves back to .fountain. If the writer uses a feature Fountain cannot store (such as track changes), the app offers to save a bundle instead, never silently dropping data.
- Opening an .fdx file saves back to .fdx by default, so producers get the format they sent.
- Format adapters live in separate libraries (see License), so other apps can reuse our .fdx and Fountain code.

## Architecture

The app is a set of Rust libraries around one command dispatcher; the GUI, CLI, JSON server and MCP server are thin layers on top.

&#91;embedded content: architecture · 4 front ends, 1 dispatcher, 4 core libraries\]

Because every change passes through the dispatcher, undo, the crash journal, track changes and agent access all come from one place instead of four.

**Stack candidates** (confirmed during phase 1 prototypes)

| Concern | Candidate |
| --- | --- |
| Language | Rust (stable) |
| GUI | iced (chosen), with our own page-view editor widget; Slint as the fallback |
| Text layout | Our own character grid (10 characters per inch, 6 lines per inch, from the page format) with bundled fonts. Chosen Oct 13 over cosmic-text and parley: pagination stays integer-exact and fast, and the bundled Courier Prime has no glyphs that shaping would help with. See `docs/specs/layout.md`. |
| PDF output | pdf-writer with subsetter, ttf-parser and miniz_oxide (chosen Oct 13). krilla 0.8 needs a newer Rust than our minimum (1.89); printpdf 0.12 pulls in random ids and clocks, which threaten byte-identical output. |
| XML for .fdx | quick-xml |
| CLI | clap |
| JSON and command schema | serde and serde\_json |
| MCP | rmcp, the official Rust MCP SDK |
| Accessibility | AccessKit |

## Command interface

One versioned command schema serves all four surfaces, so anything a person can do in the app, a script or an agent can do too, with the same names and parameters.

**Sample commands**

| Command | Parameters | Does |
| --- | --- | --- |
| `doc.open` | path | Opens a .fountain, .fdx or bundle file |
| `doc.save` | path?, format? | Saves atomically, optionally converting format |
| `block.insert` | after, type, text | Adds a script element |
| `block.set_type` | id, type | Changes an element, for example Action to Character |
| `text.replace` | find, with, scope? | Find and replace in a scene or the whole script |
| `scene.list` | none | Returns scenes with ids, headings, pages and lengths |
| `scene.move` | id, before | Reorders a scene |
| `changes.accept` | id or all | Accepts tracked changes |
| `revision.start` | color, label | Starts a production revision set |
| `export.pdf` | path, watermark? | Writes a PDF |
| `report.characters` | none | Returns lines and words per character |
| `diff` | a, b | Compares two drafts |

**Surfaces**

- **GUI:** menus, shortcuts and the command palette all emit these commands; the app has no hidden actions.
- **CLI:** batch work without opening the app, for example `coldopen export script.fountain out.pdf`, `coldopen diff a.fdx b.fdx`, `coldopen repair broken.fdx`, or `coldopen run edits.json`.
- **JSON:** `coldopen serve --stdio` reads newline-delimited JSON requests and writes responses, for any language to drive.
- **MCP:** `coldopen mcp` exposes commands as tools and the script, scenes and reports as resources.

Example JSON request:

```json
{"id": 7, "command": "block.insert", "params": {"after": "b_0142", "type": "character", "text": "MAYA"}}
```

**Agents edit in the open.** An agent can attach to a script that is open in the app, and the writer watches its edits appear live. By default, agent edits land as tracked changes under the agent's name, so the writer accepts or rejects them like notes from a collaborator.

## Platforms, size and performance

These are release targets, checked in CI on all three operating systems; missing one blocks a release.

| Metric | Target |
| --- | --- |
| Windows | Windows 10 and 11, x86-64 and ARM64 |
| macOS | macOS 12 and later, Apple Silicon and Intel |
| Linux | x86-64 and ARM64, Wayland and X11; AppImage, Flatpak and .deb |
| Installer size | Under 15 MB, fonts included |
| Cold start | Under 1 s |
| Open a 120-page script | Under 200 ms |
| Keystroke to screen | Under 16 ms (one frame at 60 Hz) |
| Full repagination, 120 pages | Under 50 ms; typical edits repaginate incrementally |
| Autosave and journal write | Under 10 ms, off the UI thread |
| Memory with a feature script open | Under 150 MB |
| Page breaks across OSes | Identical |

Web (WebAssembly) and mobile builds are not in v1, but the core libraries must stay free of desktop-only dependencies so those builds remain possible.

## Clean-room process and legal guardrails

We rebuild Final Draft's behavior from public information and our own specs, and keep a written trail that shows it. This is not legal advice; we get a lawyer's review before a public launch.

**Two roles.** The spec side studies public material (Final Draft's published manuals and help articles, industry format guides, .fdx files and PDFs that others have shared publicly) and writes behavior specs in our own words. The build side, including AI agents, works only from those specs and the public Fountain spec.

**Never copied:** Final Draft code, decompiled binaries, templates, fonts (no "Courier Final Draft"), icons, artwork or help text. Our interface gets its own design rather than mimicking Final Draft's layout.

**Final Draft's terms.** Its Terms of Service forbid using the software, or non-public information learned from it, to create similar services, and forbid reverse engineering or recreating it ([section 4](https://www.finaldraft.com/company/legal/terms-of-service)). So nobody on the project installs or uses Final Draft, including the trial and Reader Mode, and nobody scripts it to generate test files. Reading Final Draft's public web pages is fine; copying their text is not.

**Naming.** "Final Draft" is the trademark of Final Draft, Inc., a Cast & Crew company. We may say the app "opens and saves .fdx files" or is "Final Draft compatible"; we never use their name or logo in our branding.

**Paper trail.** Every spec is dated, lists its sources, and lives in the repo. Prompts given to AI agents reference specs, never pasted Final Draft material.

**AI caveat.** Models may have seen related material during training, which makes an AI-built clean room murkier than a human one. Spec-only prompts and code review reduce this risk but do not remove it.

## License

Decision: the app is GPL-3.0-or-later, and the file-format libraries are MIT or Apache-2.0, with a contributor license agreement (CLA) for outside contributions.

| Part | License | Why |
| --- | --- | --- |
| Desktop app, CLI, MCP server | GPL-3.0-or-later | Anyone may fork or sell it, but they must keep their version open, so nobody can turn our work into a closed competitor. |
| Fountain and .fdx libraries, pagination engine | MIT or Apache-2.0 | We want other apps to adopt them. More tools using reliable .fdx code spreads our name and makes .fdx exchange better for everyone. |
| Contributions | CLA | Gives the project the right to publish its own builds in the Mac App Store and Microsoft Store, whose terms conflict with GPL distribution by third parties, and keeps a future dual-licensing option open. |

Trade-off: a CLA puts off some contributors. If that becomes a problem, we can switch to a lighter Developer Certificate of Origin and give up app-store distribution of outside contributions.

## Monetization

The app stays free and complete; we charge for convenience and for services that cost us money to run.

| Stream | What people pay for | When | Price |
| --- | --- | --- | --- |
| Pay-what-you-want downloads | The same builds from our website and GitHub, with a suggested price; $0 is allowed | At v1 | Name your price |
| Store builds | Signed, auto-updating builds in the Mac App Store, Microsoft Store and Steam | Once early income covers store and signing fees | One-time, TBD |
| Sync and collaboration | Encrypted cloud sync, real-time co-writing, shared revision history, and review links that let a producer comment without installing anything | v2 | Monthly subscription, TBD; benchmark against WriterDuet and Arc Studio |
| AI credits | Hosted AI for people who don't want their own API key | When in-app AI features ship | Usage-based, small margin |
| Sponsorships | GitHub Sponsors or Open Collective | From launch | Voluntary |

Ground rules:

- Writing, formatting, export and file access are never behind a paywall.
- A lapsed subscription never locks anyone out of their scripts; files stay on their machine in open formats.
- Bringing your own AI key, or connecting any agent through MCP, is always free.

## Out of scope for v1

These wait so v1 can ship small and solid; the architecture leaves room for each.

| Feature | Target |
| --- | --- |
| Locked pages and A-pages, omitted scenes, production breakdown tags | v1.1 |
| Screen-reader support; input methods for Chinese, Japanese and Korean | v1.x |
| Index cards, beat board, story map | v1.x |
| Stage play, comics and other templates | v1.x |
| PDF import | v1.x |
| Read-aloud with voices per character | v1.x |
| Cloud sync and real-time collaboration | v2 (paid service) |
| Web, iOS and Android apps | v2 |
| Built-in AI writing features | Later; agents work through MCP from v1 |
| Interface languages beyond English | Later |

## Milestones

We build in six phases, and none starts until the previous phase passes its test gate. Dates come once the phase 1 prototypes show our real pace.

&#91;embedded content: milestones · 6 phases, each with a test gate\]

The command core comes first because everything else, including the editor and agent access, is built on it; .fdx support follows the editor so it can be tested against real use.

## Open questions and risks

- [ ] **Name.** Chosen: Cold Open (repo cold-open), with no conflicting software found so far. Draftsmith is already an AI writing assistant for Word, and Smashcut is a film-school platform with screenwriting tools, so both are out. Still to check: USPTO, domains and a GitHub organization.
- [x] **Final Draft's terms (reviewed Oct 13).** The Terms of Service bar using Final Draft, or non-public information gained from it, to create similar services, and bar reverse engineering or recreating its software ([section 4](https://www.finaldraft.com/company/legal/terms-of-service)). Decision: nobody on the project installs or uses Final Draft, including the free trial and Reader Mode.
- [ ] **.fdx test corpus.** Start with the six small samples in rsdoiel/fdx on GitHub (one to three scenes each, all sharing one settings template; four have title-page content; none has dual dialogue, scene numbers or inline styles; the repo is AGPL, so they are used locally, never copied in). Then collect publicly shared .fdx files covering title pages, dual dialogue, revisions, notes and production features, plus files saved by Fade In, WriterDuet and Highland to test tolerance. Scripts are copyrighted, so this corpus stays private; the public repo uses scripts we write ourselves.
- [ ] **Page-count reference pairs.** Three confirmed (Oct 13): Big Fish (122 pages), Brick & Steel (5) and The Last Birthday Card (21) from fountain.io. All three PDFs were made by Final Draft on US Letter, though in 2012, so newer versions may paginate slightly differently. Each .fdx also records the page every scene starts on, a second check beyond total length. Find more scripts with both the .fdx and a PDF of the same draft. Check that the PDF's Creator or Producer metadata names Final Draft and that page size matches, then use each pair as an expected-output test.
- [x] **GUI toolkit: iced.** MIT-licensed, its message-and-update model maps directly onto our commands, and System76's COSMIC desktop apps, including a text editor, are built on it. Slint is the fallback.
- [x] **Text input and accessibility.** v1 ships accented characters, full keyboard operation, zoom and a high-contrast theme. Input methods for Chinese, Japanese and Korean, and screen-reader support, move to v1.x; iced's screen-reader work currently lives in a fork, so we track it.
- [x] **Code signing decided.** Ship unsigned builds with clear instructions for the macOS and Windows warnings. Apply to SignPath Foundation for free Windows signing ([signpath.org](https://signpath.org/)); pay Apple's fee once pay-what-you-want income covers it.
- [ ] **First users.** Writers through r/Screenwriting (check its self-promotion rules first), Linux communities, film-school students, Fountain users and screenwriting forums; contributors through r/rust and Show HN.

## Sources

- [r/Screenwriting: "I. HATE. FINAL. DRAFT."](https://cal1.lr.ggtyler.dev/r/Screenwriting/comments/1jcpclv/i_hate_final_draft)
- [AWN: Fade In may be a better choice than Final Draft](https://awn.com/blog/whats-best-screenwriting-software)
- [GetApp: Final Draft reviews](https://www.getapp.com.au/software/2050325/final-draft)
- [Writing Codidact: revisions in Final Draft](https://writing.codidact.com/posts/5771)
- [Final Draft KB: install and activation limits](https://kb.finaldraft.com/hc/en-us/articles/15575351417620-On-how-many-computers-can-I-install-Final-Draft)
- [Final Draft KB: out of activations](https://kb.finaldraft.com/hc/en-us/articles/28991324279444-I-m-getting-an-error-that-says-The-Customer-Number-You-ve-Entered-Is-Out-Of-Activations)
- [Final Draft KB: rebuilding a corrupted script](https://kb.finaldraft.com/hc/en-us/articles/15575131452692)
- [FileHorse: Final Draft for Mac download](https://mac.filehorse.com/es/download-final-draft/26287/)
- [Highland: standard screenplay format](https://blog.quoteunquoteapps.com/standard-screenplay-format-the-writers-guide)
- [NFI: screenplay format](https://www.nfi.edu/screenplay-format)
- [Fountain syntax](https://fountain.io)
