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
| 2 | Crash reports: the hook, the scrubber, the offer at start, the sheet, Crash on purpose | |
| 3 | Symbols per release, and `tools/symbolicate.py` | |
| 4 | Report a problem…, the About rows, Open the user guide | |
| 5 | Issue templates, the Discussions categories and their form | |
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

## New dependencies

None yet.

## Checks

- `tools/privacy_scan.py` (2026-11-02):
  - with the private patterns, over the tree, all history's metadata, and the PR, issue and release texts: no findings after this step;
  - it catches a planted email address, home folder and private pattern.
