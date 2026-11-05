# Phase 6c progress: distribution and discovery

Branch `phase-6c-distribution`, started 2026-11-05 from `main` (4573d48). This file is updated and pushed with every step.

Alpha.2 is out. SignPath Foundation turned the signing application down until the project shows public adoption: stars, forks, outside articles and discussions. 6c makes Cold Open easy to find and to install, and gives two ad campaigns somewhere to land: one for writers, one for people who use AI agents. It ends with `v0.1.0-alpha.3` rehearsed, and tagged only when the project owner says so.

The owner's constraints: no cost; no personal details anywhere public; the app makes no network connections of its own; the site uses Cloudflare Web Analytics only.

## Plan and status

| # | Step | Status |
| --- | --- | --- |
| 0 | Branch, this record, specs first: `usage-summary.md` (new), `site.md`, `release.md` §2, §7, §11, §12, `editor.md` §23 and §24, `bench.md` §5 | done |
| 1 | The Mac save: confirm #27 on CI's Mac (asserted), explain the 44 ms Fountain save, widen the headroom if cheap, the crash gate five times, a bench build for the Mac tester | |
| 2 | The universal Mac build: fat LTO, sizes and budgets before and after | |
| 3 | The brand stand-in (Opening Frame): the icon, the mark, the image templates | |
| 4 | The usage summary, the Settings sheet, Share feedback… | |
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
5. **macOS CI asserts the UI thread's save** (`bench.md` §5), so the gate "under 4 ms on CI's Mac" is checked on every pull request.

## New dependencies

None yet.

## Checks

None yet.
