# Phase 6c progress: distribution and discovery

Branch `phase-6c-distribution`, started 2026-11-05 from `main` (4573d48). This file is updated and pushed with every step.

Alpha.2 is out. SignPath Foundation turned the signing application down until the project shows public adoption: stars, forks, outside articles and discussions. 6c makes Cold Open easy to find and to install, and gives two ad campaigns somewhere to land: one for writers, one for people who use AI agents. It ends with `v0.1.0-alpha.3` rehearsed, and tagged only when the project owner says so.

The owner's constraints: no cost; no personal details anywhere public; the app makes no network connections of its own; the site uses Cloudflare Web Analytics only.

## Final report (2026-11-06)

Phase 6c is done, up to the steps that are the project owner's: Cold Open is easy to find and to install, and both ad campaigns have somewhere to land. `v0.1.0-alpha.3` is rehearsed (below) and not tagged.

### Merged and released (2026-11-07)

- **#28 squash-merged** on the owner's go-ahead as 3c64bb7, after CI passed on its last commit (b69ea2f, the store jobs included).
- **`v0.1.0-alpha.3` tagged** on 3c64bb7, on the owner's go-ahead; release run 42346060036 passed every job. The release is published as a pre-release with 18 files. The checksums match, and `coldopen --version` prints `coldopen 0.1.0-alpha.3 (3c64bb735bfc)`. The universal .dmg is 14,898,829 bytes.
- **The website rebuilt itself:** the release started `pages.yml` (run 42347454911, `workflow_dispatch`), the first release to do so. The live download page names alpha.3, and its started pages point at the alpha.3 files.
- **itch.io:** the release's job found no key and stopped, as designed. The owner then made the page and set the key and the project (`theyungstump/cold-open`). Rerunning that one job, on the owner's go-ahead (attempt 2), pushed all five builds, each to its channel: `windows-x64`, `windows-arm64`, `mac-universal`, `linux-x64`, `linux-arm64`. Butler warns that installers and single files patch poorly. That only makes updates bigger to download, and the itch.io app is the only thing that patches.
- **Microsoft Store:** the owner submitted the `.msixbundle` (identity `TheYungStump.ColdOpen`, version 0.1.3.0, checked before upload). It's in Microsoft's certification.
- **winget:** the owner opened microsoft/winget-pkgs#473898 with the three manifests. The installers' hashes match the release, the product code matches an installed copy, and `winget validate` passes. The CLA is signed; it waits for a reviewer.
- **Homebrew:** on the owner's go-ahead, the tap `TheYungStump/homebrew-tap` was created, public, with a README and the release's `Casks/cold-open.rb`, committed with the noreply address. The copy on GitHub matches the release's cask, whose checksum matches the universal .dmg.
- **The Official MCP Registry:** the owner published alpha.3's `server.json` with `mcp-publisher`, signed in as TheYungStump. The registry lists `io.github.TheYungStump/cold-open` 0.1.0-alpha.3 as active and latest (published 2026-11-07 14:17 UTC). Before that, the bundle's SHA-256 was checked against the release's file, and the file against the registry's schema. Glama reads the registry; its search needs an API key, so whether it has the listing yet is the owner's to look at on glama.ai.
- **`store-files` and `store-msix`** from run 42346060036 are what the owner submits. They are kept until 2027-02-05.

### What was built

- **The website, first** (TheYungStump/cold-open#29, merged as 5245636 and live; `docs/specs/site.md`):
  - **`/cold-open/` for writers,** which never mentions AI (CI checks it), and **`/cold-open/ai/`** for AI users, with MCP setup for four clients.
  - **A download page that recommends the build** for the visitor's computer, in the browser only, and lists every build. It doesn't guess where the browser can't tell.
  - **Download counts by build and landing page:** every download goes through `/download/started/<build>/[writers|ai/]`, since Cloudflare drops query strings. Downloads start with JavaScript off and with ad blockers.
  - **Five search pages,** including a comparison whose every fact cites the other app's own page; titles, descriptions, Open Graph, `SoftwareApplication` data, `sitemap.xml`, `robots.txt` and `llms.txt`; and the host's root in `TheYungStump.github.io`, which the owner created.
  - **Cloudflare Web Analytics** on every page with the owner's token; the privacy statement says so everywhere.
  - **The owner's finalized brand,** the share image and the ad images, rendered in CI.
- **The Mac's .fdx save:** #27's fix (before the phase) confirmed: CI's Mac now asserts the UI thread's save, and measures the .fdx save at p95 0.62 ms (the tester's alpha.2: 4.87). The crash gate passed five times in a row.
- **One download for every Mac:** fat link-time optimization makes the binaries 5.4% smaller, and the universal disk image is 14,892,881 bytes, under the budget. Every budget still passes.
- **In the app** (`docs/specs/usage-summary.md`, `editor.md` §23 and §24):
  - **a local usage summary:** counts only, from fixed tables, never sent, on by default and disclosed, with View, Reset and an off switch;
  - **Settings** (Mod+,, and Settings… in the macOS app menu), from the keyboard alone;
  - **Share feedback…,** and the summary's tick in it and in Report a problem.
- **Store packages,** built and checked in CI: a from-source Flatpak, winget manifests, a Homebrew cask, an MSIX installed and started; an `.msixbundle` for the Store with the owner's Partner Center identity; an itch.io upload job; an MCP bundle and the Official MCP Registry's file on each release. `stores.py` fills them all from a release.
- **Drafts** in `docs/marketing/`: the ads and their claims ledger, AlternativeTo, emails to list authors, Show HN, the GitHub repository's details, itch.io, the Microsoft Store listing, the MCP directories.

### Checks

- **Tests here (Windows):** the app's new tests (usage, sheet layout, reference, and the `usage` and `feedback` unit tests; Checks, below), fmt, and clippy (`-D warnings`), also for Linux and macOS. The whole workspace's tests ran in CI on all three systems.
- **The crash gate,** five times in a row on the app's final code (`crash_gate` and core's `restart`, one thread): 9 of 9 and 3 of 3 each time.
- **CI on #28** (8241210): every job passed on Windows, macOS and Linux, the store jobs included; on #29, the site's checks (links, tags, structured data, the AI words, accessibility, the download page in 11 browsers, JavaScript off, the analytics switch both ways).
- **The live site** (5245636, checked 2026-11-06): all 33 pages in the sitemap answer 200, and the Cloudflare beacon is on.

### The owner's manual test (2026-11-07)

Every step passed, except three findings, all fixed:

1. **"Mod+," read as Ctrl and the plus key,** which zooms in. The shortcut is now written Mod+Comma (Ctrl+Comma, Cmd+Comma) in the test, the guide, the notes, the changelog, the README, the privacy statement and the spec. The keymap's syntax and the generated keyboard reference keep `Ctrl+,`, as the palette shows it.
2. **The "[ ] Include my usage summary" tick didn't look clickable.** Both sheets now use iced's checkbox (editor.md §23).
3. **Copy all closed Share feedback** and cleared its text. It now leaves the sheet as it was, with a notice, as Report a problem's Copy the details does. `usage.rs` checks it.

Found on the way: **the download page didn't follow alpha.2's release.** The release workflow makes the release with its own token, and GitHub starts no workflow for that, so `pages.yml`'s `release` trigger never fired; the page moved to alpha.2 only with the next push to `main`. The release workflow now starts the website's deploy itself after a tag's release (`site.md` §7). It can only be seen working at the alpha.3 tag.

Also seen: the alpha.3 installer went into the folder of an earlier test install, which the installer remembers by its app id, so the strict "install over alpha.2 in its usual folder" case wasn't exercised. Settings are per user, not per folder, so they carried over.

### Removed

- **Three unused pages** of the docs, `docs/guide/src/index.md`, `download.md` and `download-files.md`, removed on the owner's go-ahead (2026-11-07). The new site replaced them, and no page, check or script used them. Afterwards, the site's checks pass with no broken links.

### The alpha.3 rehearsal

Release run 42335444493, from `rehearsal/v0.1.0-alpha.3` at 17a55a8, made the draft `rehearsal-6`. Every job passed; itch.io's was skipped, as it should be until its key is set.

- **18 files,** alpha.2's 14 plus the four new ones: the universal .dmg (14,895,321 bytes), the MCP bundle (12,731,145) and the two third-party license lists. `SHA256SUMS.txt` matches each file downloaded.
- **`coldopen --version`** from the Windows zip prints `coldopen 0.1.0-alpha.3 (17a55a8a4f2d)`.
- **The MCP bundle:**
  - it holds the CLI for Windows, macOS and Linux, with manifest version 0.3;
  - its Windows binary answers an MCP `initialize` as Cold Open 0.1.0-alpha.3, and lists 50 tools.
- **`store-files`:**
  - the winget manifests, the cask and the registry's `server.json` carry hashes that match `SHA256SUMS.txt`, and addresses under the `v0.1.0-alpha.3` tag;
  - the Flathub manifest names the tag and its commit.
- **`store-msix`:**
  - an x64 and an arm64 package, and a bundle of both;
  - Identity `TheYungStump.ColdOpen`, the Partner Center publisher, display name `TheYungStump`, package version 0.1.3.0, and the `coldopen.exe` alias.
- **A second rehearsal** after the manual test's fixes (run 42342990581, f547850, draft `rehearsal-7`) passed every job again: 18 files; the checksums match; `coldopen 0.1.0-alpha.3 (f5478508d469)`. Its new "Rebuild the website" step was skipped, as it must be for anything but a tag.
- **The store files and the bundle are made fresh by the tagged release.** A squash merge gives the tag a new commit, and the packages are built again, so the owner submits that run's artifacts, not these.

### Deferred

- **The Fountain save's headroom on a Mac** (decision 6): waits for the Mac tester's numbers on the alpha.3 build.
- **Flathub** waits for a stable release, and for a manifest the owner writes (decision 15).
- **The usage summary end to end on Linux** is covered by the integration tests CI runs there (the addresses and the file), not by a submitted form; the owner's form submission on Windows is the end-to-end check.

### What's left for the project owner

1. **The manual test** (`docs/release/manual-test-6c.md`), ending in a form submission that arrives with the summary.
2. **Merge #28** (on your go-ahead), then **tag `v0.1.0-alpha.3`** (on your go-ahead).
3. **After the release:**
   - Microsoft Store: upload the `store-msix` artifact's bundle in your submission (`docs/marketing/microsoft-store.md`).
   - winget: one pull request with the `store-files` artifact's manifests (`.github/packaging/winget/README.md`).
   - Homebrew: create `homebrew-tap` and add the cask (`.github/packaging/homebrew/README.md`). Done 2026-11-07.
   - The MCP registry: `mcp-publisher publish` with the `store-files` artifact's `server.json` (`docs/marketing/mcp-directories.md`). Done 2026-11-07.
   - itch.io: the page, the API key secret and the project variable (`docs/marketing/itch.md`).
4. **Send Dian** the alpha.3 `.dmg` for the bench.
5. **The drafts** in `docs/marketing/`, to send or post when you choose; the GitHub repository's details (`docs/marketing/github.md`).
6. **Search Console:** recheck the sitemap; Bing has it.

## Plan and status

| # | Step | Status |
| --- | --- | --- |
| 0 | Branch, this record, specs first: `usage-summary.md` (new), `site.md`, `release.md` §2, §7, §11, §12, `editor.md` §23 and §24, `bench.md` §5 | done |
| 1 | The Mac save: confirm #27 on CI's Mac (asserted), explain the 44 ms Fountain save, widen the headroom if cheap, the crash gate five times, a bench build for the Mac tester | done: CI's Mac asserts it (.fdx p95 0.62 ms); the crash gate passed five times; the bench build is out. The headroom waits for the Mac tester's numbers (decision 6) |
| 2 | The universal Mac build: fat LTO, sizes and budgets before and after | done: 14,892,881 bytes, under the budget |
| 3 | The brand: the icon, the mark, the image templates | done in the finalized brand (owner's change 8): icon 52191a4; images rendered by CI in #29 |
| 4 | The usage summary, the Settings sheet, Share feedback… | done; tests pass here (below) |
| 5 | The website: two landing pages, the download page and its recommendation, the started pages, analytics behind a switch, the search pages and tags, the site root | done: TheYungStump/cold-open#29, merged 2026-11-06 as 5245636, live, analytics on |
| 6 | Store packages: Flathub, winget, Homebrew, Microsoft Store (MSIX), itch.io | done: built and checked in CI; each store's README has the owner's step; Flathub waits for a stable release (decision 15) |
| 7 | Listings and outreach drafts: AlternativeTo, MCP directories, list authors, ads, Show HN, the GitHub repository's details | done: `docs/marketing/`; the MCP bundle is on each release |
| 8 | alpha.3: version, notes, changelog, the rehearsal | done: 17a55a8; rehearsed as `rehearsal-6`, every check passed (above) |
| 9 | Final checks, the manual test list, this report | done: `docs/release/manual-test-6c.md`, and this report |

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

## The project owner's changes during the phase

8. **The brand is final** (2026-11-06), replacing the Opening Frame stand-in (decision 5): a rounded C in blue `#0C447C` whose open side is a dark `#232326` arc, cut off by two diagonal gaps, around three lines of a script; COLD OPEN in a typewriter slab; `#2A2A2D` for dark bands and paper `#F7F5EF`. The wordmark uses Courier Prime Bold, which the app already bundles. The app icon (`make_icons.py`, 52191a4), the website and the brand images all follow it.
9. **The website goes first** (2026-11-06), since the ads link to it: it's TheYungStump/cold-open#29, from `main` on branch `site-landing-pages`, so it can merge, and deploy, before the rest of the phase. GitHub Pages deploys only from `main`, so the pages go live when #29 merges, on the owner's go-ahead. Pages that describe alpha.3's features (the usage summary, Settings, the universal Mac image) say "from 0.1.0-alpha.3". This branch takes `main` back in after #29 merges.

## Where the brief's item 1 stood at the start

The brief's two candidates for the Mac's slow .fdx save were already checked on 2026-11-04, after alpha.2 (`docs/progress/phase-6a.md`, decision 20 and "The Mac tester's alpha.2 bench"; TheYungStump/cold-open#27, merged as 4573d48):

- **The .fdx passthrough store** was ruled out: the bench's .fdx is written by Cold Open, its store is 5.7 KB, and copying it takes 0.001 ms.
- **The marker waiting behind a flush** was the likely cause, but not the flusher's: the journal tidy's folder flush after each save, which on macOS is `F_FULLFSYNC`, a flush of the whole drive (5 to 8 ms there). Only an .fdx renders fast enough (about 2 ms) for its next marker to arrive during that flush. The flush bought nothing, since recovery copes with the rename either on the disk or not, so #27 removed it. CI's quick Mac bench then measured p95 0.26 ms.

What's left for 6c (step 1): confirm it on CI's Mac with the budget asserted, explain the Fountain save's 44 ms, widen its headroom if that's cheap, run the crash gate five times, and send the Mac tester a bench build.

## Found before the work

- **Cloudflare Web Analytics** (its FAQ, read 2026-11-05: developers.cloudflare.com/web-analytics/faq/): it doesn't record query strings, to avoid collecting sensitive data. Hence decision 1.
- **Homebrew** (Homebrew's discussions and issues, read 2026-11-05): casks whose apps fail Gatekeeper are deprecated, with existing ones disabled from 2026-09-01, and Homebrew's cask policy doesn't accept bypassing Gatekeeper. Cold Open's Mac build is unsigned, so the official repository would refuse it. Hence decision 4.
- **The Microsoft Store** (Microsoft's announcements of 2025-05-19 and 2025-09-10): an individual developer account is free, with an ID and selfie check. Microsoft's Partner Center documentation says customers see the account's **publisher display name**, chosen at sign-up, and that for an individual account Microsoft checks only that no one else uses it. A community answer on Microsoft Q&A, citing policy 10.14, says a name that a customer would read as a business may need a company account, which costs $99. **To confirm in step 6 from Microsoft's own pages, and report before anything is submitted.**
- **The Microsoft Store's publisher name, confirmed from Microsoft's own pages** (2026-11-06; <https://learn.microsoft.com/en-us/windows/apps/publish/partner-center/open-a-developer-account> and the Store Policies v7.20, <https://learn.microsoft.com/en-us/windows/apps/publish/store-policies>, in force since 2026-10-22):
  - An **individual** account publishes "under your own name"; a **company** account publishes under a registered business's name. Individual accounts need a government ID and a selfie check.
  - Policy 10.14: a company account is required "if a reasonable consumer would interpret your application or publisher name to be that of a business entity", and for anyone acting in their trade or profession. Company accounts cost $99 and show customer support contact details in some regions.
  - Policy 10.5.1: Win32 and Desktop Bridge apps must always have a privacy policy (the site's privacy page serves).
  - **So a free individual account would put the owner's own name on the public listing**, against the rule that nothing personal is public, and a company account costs money. The MSIX is built and tested in CI, but **nothing goes near the Store until the owner decides** (see "What's left").
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
15. **Flathub waits** (2026-11-06): its requirements now say manifests "must not contain AI-generated or AI-assisted content" and that AI tools must not open submission pull requests, and its stable repository takes only stable software. The manifest here stays as CI's from-source build test; the submitted one is the owner's, for a stable release.
16. **The MCP directories go through the Official MCP Registry,** which lists a server by its package. Cold Open's CLI isn't on npm or PyPI, so each release carries an MCP bundle (`.mcpb`, the CLI for Windows x64, macOS universal and Linux x86-64), and `stores.py` fills the registry's `server.json`. Glama reads the registry; Smithery is a registry only since October 31.
17. **The store checks' first run** found three faults of mine, all fixed: `brew style` wants `depends_on macos: :monterey`; the zip keeps `coldopen.exe` beside the app, not in `bin\`; Flathub's linter can't see `/tmp`. Then Homebrew 6 refused the untrusted temporary tap: CI runs `brew trust` first, and the tap's instructions tell writers to.
18. **The usage summary's tests run on every OS** (`crates/app/tests/usage.rs`), so Linux's end-to-end is CI's: the sentinel session, off means no writes, and the addresses.

## New dependencies

| Dependency | Where | Why | License |
| --- | --- | --- | --- |
| `objc2` 0.6, `objc2-app-kit` 0.3, `objc2-foundation` 0.3, named directly (macOS only) | app | "Settings…" in the app menu. All three are already in the tree through `rfd`, at these versions: `Cargo.lock` gains no package. Only the `NSApplication`, `NSMenu`, `NSMenuItem`, `NSResponder`, `NSString` and `NSThread` features are asked for | MIT |

## The stores' rules (step 6; each store's own pages, read 2026-11-05 and 11-06)

Each store's folder under `.github/packaging/` has its README with the owner's steps; these are the rules that shape them.

- **Flathub** (<https://docs.flathub.org/docs/for-app-authors/requirements>, `/submission`, `/verification`):
  - "Flathub manifests must not contain AI-generated or AI-assisted content", and "AI tools or agents must not open or automate Flathub submission pull requests". **The manifest here was written with an AI assistant, so it can't be the one submitted.** It stays as CI's proof that Cold Open builds offline from source in Flathub's container; the owner writes the submitted one.
  - "The Flathub stable repository is dedicated to hosting only stable software." Cold Open is an alpha, so Flathub waits for a stable release.
  - "All source available submissions must be built entirely from source code"; "There is no network access during the build process."
  - An `io.github.` ID's repository is taken from its last part "as-is": `coldopen` against the repository `cold-open`. An exception or a new ID, before submitting.
  - "Static permissions must be kept to an absolute minimum"; explain each in the pull request (`--filesystem=home`, for the crash journal beside each script).
- **winget** (<https://learn.microsoft.com/en-us/windows/package-manager/package/repository>, `/windows-package-manager-policies`): one package version per pull request; the installer supports "non-interactive modes", comes "directly from the publisher's website", and installs "for both administrators and non-administrators"; Defender scans every installer. Unsigned installers and pre-releases aren't addressed.
- **Homebrew** (<https://docs.brew.sh/Acceptable-Casks>, `/How-to-Create-and-Maintain-a-Tap`, `/Tap-Trust`): official casks must "pass Homebrew's Gatekeeper checks", which an unnotarized app fails, hence our own tap, named `homebrew-tap`; "non-official taps require explicit trust by default since Homebrew 6.0.0", so writers run `brew trust theyungstump/tap`.
- **Microsoft Store** (above, "Found before the work"): an individual account publishes under the display name Partner Center shows (`TheYungStump` for this account); policy 10.5.1 asks every Win32 app for a privacy policy; the company option now shows no fee, but needs a work email at the organization's own domain.
- **itch.io** (<https://itch.io/docs/creators/faq>, `/quality-guidelines`, <https://itch.io/docs/butler/>): free tools are fine ("a minimum price ... which can be 0"); channel names containing `win`, `mac` or `linux` tag the platform; mark only platforms "you've actually tested"; "accurately tag your project if it contains materials produced by generative AI", which `docs/marketing/itch.md` does.

## The analytics and SignPath's terms (step 5)

SignPath Foundation's terms (<https://signpath.org/terms>, reread 2026-11-05) were checked against the analytics and the usage summary:

- **The privacy rule is about the signed program.** Under "Respect user privacy and security", software that collects data and sends it to systems the user didn't specify must say so in a privacy policy, show it at install, and offer a way to turn it off. The terms suggest the line "This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it", which the README and the site's privacy page already carry.
- **Cold Open still sends nothing.** The usage summary stays on the computer; it leaves only when the writer ticks it into feedback, reads it, and submits the page themselves: "specifically requested by the user". CI's privacy job still checks that no network client is in the app, the CLI or the MCP server.
- **The terms say nothing about a project's website analytics.** Their website requirements are the code signing policy (on the home page; the words "Code signing policy" on the home, download and release pages) and the privacy link or statement. The site keeps both, and the privacy statement now says, in the owner's words, that the website counts visits anonymously with Cloudflare Web Analytics, without cookies, and the app sends nothing. Cloudflare's own privacy policy is linked beside it, as the terms ask for third parties whose users are affected.
- **The download page must describe the program's functionality,** so its first paragraph now says what Cold Open does.

### CI's Mac on this branch (8241210, run 42232430952; asserted since step 1)

| Save | UI thread p95 | Whole save, median |
| --- | --- | --- |
| Fountain | 0.56 ms | 33.9 ms |
| .fdx | **0.62 ms** (snapshot p95 0.5, marker 0.1) | 11.0 ms |
| Project | 0.60 ms | 15.4 ms |

The .fdx save on the UI thread passes on CI's Mac at p95 0.62 ms (the tester's alpha.2 bench: 4.87). The tester's bench build is that run's `packages-macos` artifact (`docs/release/mac-tester.md` now says how to fetch one), and the same build will be in the alpha.3 rehearsal.

## Step 2: the universal Mac build

Release binaries on this PC (x86_64-pc-windows-msvc, `strip = true`), before and after `lto = "fat"`:

| Binary | Thin LTO | Fat LTO | Change |
| --- | --- | --- | --- |
| `coldopen-app.exe` | 18,684,416 | 17,667,072 | −5.4% |
| `coldopen.exe` | 8,949,760 | 8,465,920 | −5.4% |

- **The universal .dmg is 14,892,881 bytes** with fat LTO (CI, run 42232430952), down from 15,218,928 to 15,404,028 in phase 6a: under the 15,000,000-byte budget, so the exemption of decision 2 isn't needed. The Apple chip image is 7,321,522 bytes, the Intel one 8,735,140.
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
