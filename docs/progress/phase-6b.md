# Phase 6b progress: the alpha: docs, website, feedback and testers

Branch `phase-6b-alpha`, started 2026-11-02 from `main` (5d6e764). This file is updated and pushed with every step.

Phase 6a built the packages, the bench and the release workflow. 6b gets the alpha ready to announce:

- user docs and a website on GitHub Pages;
- feedback that works without a GitHub account;
- opt-in crash reports with no server (spec feature 9);
- a tester program;
- housekeeping.

It ends with `v0.1.0-alpha.2` rehearsed, and tagged only when the project owner says so.

The README's code signing policy and privacy statement went first, in their own pull request (TheYungStump/cold-open#20), so the SignPath application can go in as soon as `v0.1.0-alpha.1` is out.

## Plan and status

| # | Step | Status |
| --- | --- | --- |
| A | README: code signing policy and privacy statement (separate PR) | PR TheYungStump/cold-open#20 open, waiting for the owner's go-ahead |
| 0 | Privacy and housekeeping: the name out of `spec.md`, `tools/privacy_scan.py`, CI's privacy job, the milestones | done |
| 1 | Specs: `crash-reports.md` and `site.md` (new), `editor.md` §23, `release.md` | done |
| 2 | Crash reports: the hook, the scrubber, the offer at start, the sheet, Crash on purpose | done |
| 3 | Symbols per release, and `tools/symbolicate.py` | done; checked on Windows here, Linux in CI |
| 4 | Report a problem…, the About rows, Open the user guide | done (the form's button waits for step 6) |
| 5 | Issue templates, the Discussions categories and their form | templates done; Discussions wait for the owner |
| 6 | The Google Form (made by the owner) and its prefill | waiting for the form's link and entry ids |
| 7 | User docs, and the generated CLI, command and keyboard references | |
| 8 | The website: mdBook, Pages, link and accessibility checks | |
| 9 | Tester guide, "what feedback helps", announcement drafts | |
| 10 | Changelog, alpha.2 notes, version bump, rehearsal | |
| 11 | Final checks, the manual test list, this report | |

## Decisions from the project owner (2026-11-02, before the phase)

1. **The no-account path is a Google Form,** made by the owner under a project Google account, with "Collect email addresses" off and no file-upload question, so testers don't need to sign in. No project email for now.
2. **The real name in `docs/spec.md`** is replaced in the file. The history stays as it is.
3. **The Contribute page asks for feedback only.** Pull requests open once a CLA exists and a lawyer has checked it. SignPath's terms rule out commercial dual-licensing.
4. **Crash backtraces are module+offset frames.** CI keeps each build's symbols in one file on the release, and `tools/symbolicate.py` turns a report into function names.

## Decisions made during the phase

1. **The privacy scan's private patterns** live outside the repository, in `%APPDATA%\coldopen-privacy.txt` on the owner's PC: the name's parts and the email's local part. So the scan can look for them without publishing them. CI runs the generic checks: email addresses other than noreply and example ones, and home folders named after someone.
2. **`coldopen-diff` joins the license-boundary check.** Phase 5b made it MIT OR Apache-2.0, but CI's list of permissive crates didn't name it, and it had no copies of the two license texts. It has both now, and CI checks it like the other five.
3. **CI's privacy job checks that the app, the CLI and the MCP server hold no network client** (reqwest, hyper, rustls and the like), so the privacy statement stays true. Today's tree is 383 crates, none of them one.

4. **Rust's own slicing messages quote less than they used to.** On today's Rust, slicing a string off a character boundary names the character (`inside 'é' (bytes 13..15 of string)`), not the whole string. The scrubber takes out either form. A test checks it on Rust's real messages, not just on our idea of them. Backticks are scrubbed from the first to the last, since a backtick in the script's text would otherwise end the span early.
5. **"Last action" is noted where every command passes.** The dispatcher notes each command's name, with one relaxed atomic store, and the router notes each app action. So typing, which goes straight to the dispatcher, shows as `text.insert`, and a palette action shows by its name. Parameters are never kept.
6. **Frames come from the `backtrace` crate's unwinder,** with the module and its base from the OS: `dladdr` on Linux and macOS, `GetModuleHandleExW` and `GetModuleFileNameW` on Windows. These are three small `unsafe` calls, each with its SAFETY note. The owner's plan approval covered them. `backtrace` is pinned at 0.3.76 (2025-09-26), since 0.3.77 was six days old.
7. **A test never opens a browser.** Links open only when the app starts with `Boot::open_links`: the real app, and the gate driver's `--open-links`. Every other app notes the addresses in `App::opened`. The first full test run opened the user guide's address once, before this change.
8. **The crash sheet shows the report's date in UTC,** so no time-zone data is read.
9. **The OS line** reuses the bench's probe (`bench::machine::os_version`), already checked to name no person or machine.
10. **"Crash on purpose" is in the palette of every build** during the alpha, behind a confirmation, so testers can try recovery and a crash report. `COLDOPEN_TEST_CRASH` (slice, debug, worker) works in debug builds only.
11. **Symbols:**
    - CI builds every release binary with line tables and strip off, then splits the symbols out and strips (`package.sh symbols`). Windows needs `strip = none` for the linker to write a PDB at all.
    - `tools/symbolicate.py` uses `llvm-symbolizer` when it's installed, else the OS's own tool: `addr2line`, `atos`, or Windows' dbghelp.
12. **Issue templates use the repository's existing labels** (`bug` only). A `crash` or `feedback` label would need creating first, which is a repository setting.

## New dependencies

| Dependency | Where | Why | License |
| --- | --- | --- | --- |
| `backtrace` 0.3.76, with `addr2line`, `gimli`, `object` and `rustc-demangle` | app | A panic's return addresses. Only the unwinder is called; LTO drops the symbolizer | MIT OR Apache-2.0 |
| `libc` (Unix), `windows-sys` 0.61 (Windows), named directly | app | Which module holds each frame. Both already in the tree | MIT OR Apache-2.0 |

## Checks

- `tools/privacy_scan.py` (2026-11-02):
  - with the private patterns, over the tree, all history's metadata, and the PR, issue and release texts: no findings after this step;
  - it catches a planted email address, home folder and private pattern.
