# Phase 6c progress: distribution and discovery

Branch `phase-6c-distribution`, started 2026-11-05 from `main` (4573d48). This file is updated and pushed with every step.

Alpha.2 is out. SignPath Foundation turned the signing application down until the project shows public adoption: stars, forks, outside articles and discussions. 6c makes Cold Open easy to find and to install, and gives two ad campaigns somewhere to land: one for writers, one for people who use AI agents. It ends with `v0.1.0-alpha.3` rehearsed, and tagged only when the project owner says so.

The owner's constraints: no cost; no personal details anywhere public; the app makes no network connections of its own; the site uses Cloudflare Web Analytics only.

## Plan and status

| # | Step | Status |
| --- | --- | --- |
| 0 | Branch, this record, specs first: `usage-summary.md` (new), `site.md`, `release.md` §2, §7, §11, §12, `editor.md` §23 and §24, `bench.md` §5 | done |
| 1 | The Mac save: confirm #27 on CI's Mac (asserted), explain the 44 ms Fountain save, widen the headroom if cheap, the crash gate five times, a bench build for the Mac tester | CI assertion and the analysis done (below). The crash gate and the bench build follow step 2, so they cover the release profile that ships. The headroom waits for the Mac tester's worker rows (decision 6). |
| 2 | The universal Mac build: fat LTO, sizes and budgets before and after | done here (below); the universal .dmg's size comes from CI's packaging job |
| 3 | The brand stand-in (Opening Frame): the icon, the mark, the image templates | icon, mark and templates done; images rendered once the sample screenshots are made |
| 4 | The usage summary, the Settings sheet, Share feedback… | done; tests pass here (below) |
| 5 | The website: two landing pages, the download page and its recommendation, the started pages, analytics behind a switch, the search pages and tags, the site root | |
| 6 | Store packages: Flathub, winget, Homebrew, Microsoft Store (MSIX), itch.io | |
| 7 | Listings and outreach drafts: AlternativeTo, MCP directories, list authors, ads, Show HN, the GitHub repository's details | |
| 8 | alpha.3: version, notes, changelog, the rehearsal | |
| 9 | Final checks, the manual test list, this report | |

## Decisions from the project owner (2026-11-05, before the phase)

1. **Download counts by path.** Cloudflare Web Analytics doesn't record query strings (its FAQ), so `/download/started/?build=…&from=…` would count every download as one page. The build and the landing page go in the path: `/download/started/<build>/` and `/download/started/<build>/<from>/`. `from` is a plain word (`writers`, `ai`), never an identifier.
2. **The Mac universal build.** Try safe size settings only: `lto = "fat"`, `codegen-units = 1` and `strip`, with `opt-level` changed only if a budget improves, and never `panic = "abort"` (it would bypass the crash journal and crash reports). Rerun every budget and gate. If the universal .dmg still won't fit under 15 MB, ship it anyway and change the spec (dated): the 15 MB budget applies per architecture, and the universal .dmg, which holds two, is exempt. Recommend it for all Macs.
3. **The site's root.** I prepare the files of a user-site repository, `TheYungStump/TheYungStump.github.io` (robots.txt naming the sitemap, llms.txt, a root page linking to `/cold-open/`). The owner creates the repository and pushes them. Copies also stay under `/cold-open/`.
4. **Homebrew:** our own tap, `TheYungStump/homebrew-tap`, which the owner creates.
5. **Brand art:** stand in with **01 / Opening Frame** (`design/brand-explorations/2026-11-05/01-opening-frame.png`), the current frontrunner, still subject to change.
6. **The usage summary is on by default,** and disclosed up front, not only at the first Share feedback:
   - one line in the privacy statement: it counts which features you use, on this computer only, and is never sent unless you include it in feedback;
   - a visible "Usage summary" entry in Settings and in About, with View, Reset and the off switch.
7. **A new small Settings sheet.**
   - It opens with the standard shortcut: Ctrl+, on Windows and Linux, Cmd+, on macOS, where it's also "Settings…" in the app menu.
   - It's fully keyboard-navigable.
   - It holds existing settings only: no new options in this phase, beyond the usage summary's switch that item 6 of the brief requires.

## Where the brief's item 1 stood at the start

The brief's two candidates for the Mac's slow .fdx save were already checked on 2026-11-04, after alpha.2 (`docs/progress/phase-6a.md`, decision 20 and "The Mac tester's alpha.2 bench"; TheYungStump/cold-open#27, merged as 4573d48):

- **The .fdx passthrough store** was ruled out: the bench's .fdx is written by Cold Open, its store is 5.7 KB, and copying it takes 0.001 ms.
- **The marker waiting behind a flush** was the likely cause, but not the flusher's: the journal tidy's folder flush after each save, which on macOS is `F_FULLFSYNC`, a flush of the whole drive (5 to 8 ms there). Only an .fdx renders fast enough (about 2 ms) for its next marker to arrive during that flush. The flush bought nothing, since recovery copes with the rename either on the disk or not, so #27 removed it. CI's quick Mac bench then measured p95 0.26 ms.

What's left for 6c (step 1): confirm it on CI's Mac with the budget asserted, explain the Fountain save's 44 ms, widen its headroom if that's cheap, run the crash gate five times, and send the Mac tester a bench build.

## Found before the work

- **Cloudflare Web Analytics** (its FAQ, read 2026-11-05: developers.cloudflare.com/web-analytics/faq/): it doesn't record query strings, to avoid collecting sensitive data. Hence decision 1.
- **Homebrew** (Homebrew's discussions and issues, read 2026-11-05): casks whose apps fail Gatekeeper are deprecated, with existing ones disabled from 2026-09-01, and Homebrew's cask policy doesn't accept bypassing Gatekeeper. Cold Open's Mac build is unsigned, so the official repository would refuse it. Hence decision 4.
- **The Microsoft Store** (Microsoft's announcements of 2025-05-19 and 2025-09-10): an individual developer account is free, with an ID and selfie check. Microsoft's Partner Center documentation says customers see the account's **publisher display name**, chosen at sign-up, and that for an individual account Microsoft checks only that no one else uses it. A community answer on Microsoft Q&A, citing policy 10.14, says a name that a customer would read as a business may need a company account, which costs $99. **To confirm in step 6 from Microsoft's own pages, and report before anything is submitted.**
- **Flathub** (its requirements and forum, read 2026-11-05): building from source is expected where the source is open, and reviewers have refused wrapped binaries. Hence a manifest that builds from source (`release.md` §11.1), unlike the release's bundle.
- **robots.txt** is read only at a host's root (RFC 9309), and the site lives at `theyungstump.github.io/cold-open/`. Hence decision 3.
- **The app has no Help menu or Settings screen:** its menu is the palette (`editor.md` §9), and its settings are palette commands. "Share feedback…" goes in the palette, About and the start screen's links. Settings is decision 7.

## Decisions made during the phase

1. **Specs first** (step 0):
   - `usage-summary.md` keeps the summary to fixed tables: command names from `Command::NAMES` and the action table, four session buckets, four formats, and the renderer's kind. Nothing the writer types can become a key or a value.
   - It's written off the UI thread, at most every 5 minutes and at quit.
   - Off means no writes at all. Reset deletes the file.
2. **The website keeps mdBook for the docs, at their addresses,** and builds the landing, download, started and search pages with a small Python script (`site.md` §2). The docs' sidebar lists the AI agents page, and the writers' pages mustn't mention AI, so those pages can't be mdBook chapters.
3. **The download page recommends nothing on Windows without client hints** (Firefox, Safari, old Edge), since those browsers report x64 on ARM computers too, and an x64 build doesn't run on Windows 10 on ARM. The brief says not to guess. The Windows group comes first, with how to tell.
4. **Linux's recommendation is the AppImage,** which needs no install and runs on any distribution.
5. **macOS CI asserts the UI thread's save** (`bench.md` §5), so the gate "under 4 ms on CI's Mac" is checked on every pull request. It's a new step in `ci.yml`'s budgets job, which reads the bench's JSON as the Linux report-scrolling step does.
6. **The Fountain save's headroom waits for the Mac tester's worker rows** (step 1, below). On CI's Mac the save is mostly rendering, and on the tester's Mac it's most likely drive flushes. Neither has a change that is both cheap and safe without the tester's per-step numbers, which #27 added to the bench and which the alpha.3 bench build will report.

## Step 1: the Mac save

### CI's Mac on main after #27 (4573d48, run 42107453249, the full bench)

| Save | UI thread p95 (snapshot / marker / commit, p95) | Whole save, median | Worker: render / write / finish, median |
| --- | --- | --- | --- |
| Fountain | 0.51 ms (0.4 / 0.1 / 0.1) | 27.6 ms | 19.0 / 3.7 / 2.0 ms |
| .fdx | **0.52 ms** (0.4 / 0.1 / 0.1) | 8.6 ms | 2.9 / 3.1 / 1.6 ms |
| Project | 0.42 ms (0.3 / 0.1 / 0.1) | 13.3 ms | 10.6 / 3.0 / 1.7 ms |

- **The .fdx marker no longer waits:** p95 0.1 ms, against 3.03 ms on the Mac tester's alpha.2 bench. The edit typed right after each save takes 0.1 ms (p95 0.2) for every kind.
- **CI's virtual Mac flushes in 0.8 ms** (the bench's fsync row), where the tester's real Mac took 5.0 ms (p95 7.6), since a real drive's `F_FULLFSYNC` empties its cache. So CI can't show the flushes' cost, only the rest.

### Where a Fountain save's time goes

- **The render, 19 ms on CI's Mac and 21 ms on this PC,** is mostly the Fountain writer reading its own text back to check that nothing is lost. Timed here on the synthetic 120-page script (145 KB): `write` 21.5 ms, `write_text` alone 6.5 ms, `parse` of the text 14.5 ms, the comparison 0.1 ms. The parser reads about 10 MB/s, building one atom per character before classifying lines (`crates/fountain/src/scan.rs`).
- **On the tester's Mac,** the same 15 to 20 ms of rendering, plus about five drive flushes at about 5 ms each (`phase-6a.md`, "Where the Mac's 44 ms Fountain save goes"), make the 44 ms.
- **The ways to win it back,** each worth about 5 to 15 ms on that Mac:
  1. Fewer full drive flushes on macOS: `F_BARRIERFSYNC` for order, and one `F_FULLFSYNC` at the end. The biggest lever, but it's invisible on CI's virtual disk, and it changes when a save is durable, so it needs the tester's per-step numbers to justify it.
  2. Write the file while the text is read back, renaming only once the check passes. That saves the write step: about 4 ms on CI, more on a real Mac.
  3. A faster Fountain parser. That's real work on a fuzzed parser, not cheap.
- **Decision 6:** none is done before the tester's bench of this phase's build, which reports `save_worker_*_render`, `_write` and `_finish` on a real Mac. The budget is met there (44 against 50).

7. **The universal .dmg ships beside the two per-architecture ones** (`package.sh macos`), so a writer who knows their Mac can take the smaller file, and the website recommends the universal one. Only the universal .dmg is exempt from the size check, and only if it's over.
8. **The icon is redrawn from Opening Frame's geometry** in `make_icons.py`, not cut from the board's raster: a C-shaped frame (outer edges 0.24 to 0.72 across and 0.24 to 0.76 down, 0.115 thick) and a red cursor bar (0.675 to 0.765, 0.385 to 0.615) on the rounded navy tile, in the stand-in's own navy, paper and red. It also writes `coldopen.svg` and `mark.svg` (the frame in `currentColor`, for both site themes). Every package's icon follows.
9. **Brand images are HTML rendered by Playwright** (`tools/brand/card.html`, `render.mjs`, `images.json`), with the app's own Atkinson Hyperlegible Next, so a layout change is an edit and a rerun. Their screenshots come from a short scene of our own (`tools/brand/the-night-ferry.fountain`), through `screenshots.rs` with `COLDOPEN_SHOT_SCRIPT`, since the synthetic script's filler words would look odd in an ad.
10. **The usage summary counts in three places** (`usage-summary.md` §2.2):
    - the dispatcher's new command hook (`Dispatcher::set_command_hook`, one `Arc` call per command, as the writer or an agent), which covers typing and agents' commands;
    - the router, for the app's own actions;
    - the in-place save, which goes through the save worker rather than `execute`.

    Opens and saves are counted by format where the app finishes them. Tests and benches never count: `Boot::count_usage` is on only in the real app's `main`.
11. **The summary's file holds three formats, not four:** a `.txt` opens as Fountain, so `text` was dropped from the spec.
12. **Share feedback keeps the letter keys** of the other sheets (spec updated): iced's buttons can't take keyboard focus, so a Tab order through them isn't possible. The text box's own key binding sends Mod+Enter to the form, and Esc or Tab let go of it.
13. **Both issue templates gained an optional "Usage summary" field** (`usage`), and the feedback template a "Renderer" field, so the issue addresses carry what the sheet showed. The Google Form needs no change: the summary goes in its existing `report` box.
14. **"Settings…" in the macOS app menu** (`crates/app/src/macos_menu.rs`): an `NSMenuItem` with Cmd+, after About, whose target sends `app.settings` through a channel the app's subscription reads. Three `unsafe` calls (the target's `init`, the item's `init`, `setTarget`), each with its SAFETY note, approved with the plan. It's installed on the first timer tick, once winit has made its menu. Checked here with `cargo clippy --target aarch64-apple-darwin`; it runs on CI's Mac and the Mac tester's.

## New dependencies

| Dependency | Where | Why | License |
| --- | --- | --- | --- |
| `objc2` 0.6, `objc2-app-kit` 0.3, `objc2-foundation` 0.3, named directly (macOS only) | app | "Settings…" in the app menu. All three are already in the tree through `rfd`, at these versions: `Cargo.lock` gains no package. Only the `NSApplication`, `NSMenu`, `NSMenuItem`, `NSResponder`, `NSString` and `NSThread` features are asked for | MIT |

## Step 2: the universal Mac build

Release binaries on this PC (x86_64-pc-windows-msvc, `strip = true`), before and after `lto = "fat"`:

| Binary | Thin LTO | Fat LTO | Change |
| --- | --- | --- | --- |
| `coldopen-app.exe` | 18,684,416 | 17,667,072 | −5.4% |
| `coldopen.exe` | 8,949,760 | 8,465,920 | −5.4% |

- The universal .dmg was 15,218,928 to 15,404,028 bytes in phase 6a. 5.4% off that is about 14.4 to 14.6 MB, so it should fit; CI's packaging job measures it.
- **Every budget passes on the fat build** (the quick bench, this PC, `…20261105-224732`): open 40.7 ms, repagination 22.6 ms, journal write p95 0.10 ms, save on the UI thread p95 0.57 / 0.70 / 0.70 ms, whole save median 27.7 / 5.9 / 13.9 ms, cold start 228 ms, keystroke p95 11.8 ms (median 8.1), report scrolling p95 7.7 ms (median 6.7), memory 55 MiB. 6a's quick runs on this PC were in the same range.
- A fat build takes 3.5 minutes here, against 2.2 for thin.

## Checks

- **Step 4, here (Windows):** `cargo test -p coldopen-app --test usage --test sheet_layout --test reference`, and the `usage` and `feedback` unit tests, pass:
  - the sentinel session: typing, renaming a character and a scene, saving, and Save as under a new name in a folder with a private name. Neither `usage.json` nor the plain words hold any of it, or the home folder, or the user name;
  - counting off: no file is made, and an existing file's bytes and modified time don't change;
  - Reset, and turning counting off mid-session;
  - Share feedback's issue address holds the text and, only when ticked, the summary;
  - Settings by the keyboard alone (Mod+, then the arrows, Space, Tab, Enter, Esc);
  - every sheet, ticks ticked, fits its card.
- `cargo clippy -D warnings` is clean for Windows, and for the app and core on Linux and macOS.
