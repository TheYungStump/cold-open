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
| A | README: code signing policy and privacy statement (separate PR) | squash-merged as c46d068 (#20), 2026-11-03, with the owner's go-ahead |
| 0 | Privacy and housekeeping: the name out of `spec.md`, `tools/privacy_scan.py`, CI's privacy job, the milestones | done |
| 1 | Specs: `crash-reports.md` and `site.md` (new), `editor.md` §23, `release.md` | done |
| 2 | Crash reports: the hook, the scrubber, the offer at start, the sheet, Crash on purpose | done |
| 3 | Symbols per release, and `tools/symbolicate.py` | done; checked on Windows here, Linux in CI |
| 4 | Report a problem…, the About rows, Open the user guide | done, with the form from step 6 |
| 5 | Issue templates, the Discussions categories and their form | templates done; Discussions wait for the owner |
| 6 | The Google Form (made by the owner) and its prefill | done 2026-11-03: in the app, the website and the issue chooser |
| 7 | User docs, and the generated CLI, command and keyboard references | done |
| 8 | The website: mdBook, Pages, link and accessibility checks | built and checked here and in CI; the first deploy waits for Pages to be turned on |
| 9 | Tester guide, "what feedback helps", announcement drafts | done: `docs/release/announcements/`; the form's setup is `docs/release/google-form-setup.md` |
| 10 | Changelog, alpha.2 notes, version bump, rehearsal | done; rehearsal run 41585943031 from `rehearsal/v0.1.0-alpha.2` (128a74a), draft checked |
| 11 | Final checks, the manual test list, this report | done: the manual test list is `docs/release/manual-test-6b.md`, and the Mac page is updated |

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
13. **Every command parameter has a description now:** 88 short doc comments on the parameter types in `crates/core/src/command.rs`. They feed the agents' tool schemas as well as the command reference, where 56 of 123 parameters had none.
14. **The site's links are checked by `tools/site_links.py`,** rather than lychee: one tool fewer, since mdBook's HTML is regular enough to read with two patterns. It also checks every `#anchor`.
15. **mdBook has no footer or skip link,** so a small preprocessor (`tools/site_footer.py`) adds the footer with the code signing policy and privacy links to every page. A short script (`docs/guide/theme/a11y.js`) adds "Skip to the content". Without scripts, the page's landmarks still let a screen reader skip the sidebar.
16. **Contrast, checked here in the built site:** every text element of a long page passes WCAG AA in the light theme and in the dark one, except highlight.js's red in dark code blocks (4.46:1). `coldopen.css` lightens it. axe-core runs on every page in CI.
17. **The guide's first page is `guide/index.md`,** not `README.md`: mdBook turns a README into `index.html` but leaves links to it as `README.html`.
18. **This PC's Python is missing `html/entities.py`** from its standard library, so the site tools use nothing beyond Python's core modules. Not changed: it's the owner's install.
19. **What CI found on the draft PR** (2026-11-03, 429bcbf), all fixed:
    - **axe-core on mdBook's page template:** 108 violations of three rules, every one from the template, none from our pages, and no contrast failures.
      - The sidebar toggle is a `<label>` that mdBook marks `aria-expanded`, which a label can't carry. The keyboard couldn't reach it either. `a11y.js` (formerly `skip-link.js`) makes it a button, focusable, that Enter and Space press.
      - The sidebar's links were 21 px tall; `coldopen.css` makes them at least 24 (WCAG 2.5.8).
      - The keyboard reference's wide table scrolled sideways with no way to reach it by keyboard. A wrapper that overflows now takes focus and is named after its heading.
      - **The rerun (3132049) left 4 violations, all on `404.html`.** Its `<base href="/cold-open/">` (book.toml's `site-url`) points its CSS and scripts at `/cold-open/`, but CI served the site at `/`, so that one page loaded none of them. CI now serves the site under `/cold-open/`, as GitHub Pages does.
    - **The scrubber's property test** (Linux, `random_bytes_scrub_safely`) failed on a false alarm, not a leak. An unquoted `D ���` after the quoted `"D���"` reads `D���` once the NUL is taken out. The check now compares the text around a string as the scrubber leaves it (`crash::one_line`). The input is a regression test.
    - **macOS symbols:** Cargo leaves `coldopen-app.dSYM` in the release folder as a link into `deps/`, and `cp -R` copied the link. The upload then found only a dangling link. `package.sh` copies what it points at (`cp -RL`).
20. **The README PR's red check was a test fault, not lost work** (2026-11-03). `restart.rs`, from 6a, failed once on Ubuntu, and only sometimes. A kill at `tidy_started` lands somewhere in the typing. When it lands just after a typed space (`" B1. "`), recovery gives back the space too, but the test read the script through the Fountain export, which trims a trailing space. It then expected `" F."` to follow `"B1."`, not `"B1. "`. The continuation is now checked on the block's own text. It passed five runs in a row here. #20 changed only the README and was squash-merged as c46d068 on the owner's go-ahead.
21. **The announcement drafts** (`docs/release/announcements/`).
    - Reddit can't be read from this machine: the fetcher and the built-in browser both refuse it, and its rules weren't read through a mirror. Each draft therefore says what was verified from search results and news, and has a checklist to confirm in the sidebar.
    - **r/opensource:** its rules of late October 2026 (TNW, RuntimeWire, WinBuzzer) exclude Cold Open: AI-assisted code, an MCP server, under a year old, one contributor. The draft recommends not posting, and gives alternatives.
    - **r/Screenwriting:** ask the moderators first, with a draft message.
    - **r/linux:** possibly a 3-month minimum for personal projects, a proposal that may not have been adopted. If it was, the repository qualifies from 2027-01-15.
    - **Fountain:** a request to fountain.io's apps list, held until there's a project email.
    - Every draft mentions the AI-assisted development openly, as the README does. Removing that line is the owner's call.
22. **The Google Form's setup** is written for the owner (`docs/release/google-form-setup.md`): a project account, the seven questions the app's prefill expects (the choices "Bug" and "Crash" by their exact text), settings that need no sign-in, and the prefilled link to send back.
23. **`tools/site_footer.py` reads and writes UTF-8 itself.** On Windows, Python's stdin follows the code page, and a local build showed "–" as "â€“". CI's Linux wasn't affected.
24. **`coldopen.exe` carries the product name and version** as the app has since 6a: a `build.rs` with `winresource`, for SignPath's file-metadata condition. The installer already had both: Inno Setup's product name defaults to `AppName`, and the version is passed in. CI's Windows install step now checks the app and the CLI name "Cold Open" and `Cargo.toml`'s version.
25. **The form is wired in** (2026-11-03), from the owner's prefilled link: `FORM_URL` and its six question ids in `crates/app/src/feedback.rs`.
    - **Checked on its public page here:** it needs no sign-in and collects no email. The kind's choices include "Bug" and "Crash", spelled as the app sends them. A prefilled crash link chose Crash and filled every box.
    - **The crash form has one box for the report,** so the app puts the one-line summary there. The banner tells the writer to paste the whole report, which replaces it.
    - **The issue chooser's "No GitHub account?"** goes straight to the form, one click fewer. The website's feedback page links it too, and explains where the answers go.
    - **Tests:** the form's address holds the kind, version, OS and renderer and nothing else; a crash's fits under 2,000 characters; and F opens it from both sheets.
    - **The form's text has a typo:** the Version question's description says "About Cold Open show it". The owner can fix it in Google Forms; the app's prefill doesn't depend on it.
26. **The fuzz run's `crash_scrub` find was another false alarm** (2026-11-03, run 41585968031). The quoted string was `]^����`, and the text after it began `^����`. The scrubber turned the quote into `[quoted]`, whose last `]` joined that text to read the string again, though none of it was kept.
    - **The check now compares against the same message scrubbed with nothing between the quotes.** Whatever is in both was never quoted: the text around, the scrubber's own words, and any join between them.
    - This covers decision 19's find too, so the two special cases it replaced are gone.
    - The input is a regression test in `crates/core/tests/crash.rs`.
    - The other 12 targets ran ten minutes each with no finds.
27. **GitHub settings** (2026-11-03, the owner's go-ahead):
    - **Pages is on,** built by GitHub Actions. Only `main` may deploy, so the site goes live when this PR merges. After that, every push to `main` redeploys it, so styling can change at any time.
    - **Discussions is on.** GitHub made Announcements, Q&A and Ideas (as planned), and also General, Polls and Show and tell. "Alpha testing" can only be made on the web page. Its slug must be `alpha-testing` to match `.github/DISCUSSION_TEMPLATE/alpha-testing.yml`.
    - **The owner confirmed two-factor sign-in on GitHub,** as the code signing policy says, and fixed the form's typo.
28. **The Mac tester's rerun on a5d157d missed the report view's scrolling again** (2026-11-03, `docs/progress/phase-6a-bench/…20261103-042802`).
    - **The numbers:** p95 20.3 ms against 16, with a median of 5.9 ms on Metal. On b901828 it was p95 16.3 ms. Every other budget passed: keystroke p95 10.5 ms, page scrolling 14.3 ms, memory 137 MiB.
    - **They point at the GPU path.** At least 7 of the 120 frames took over 20 ms while most took about 6. The CPU renderer scrolled the same view steadily (p95 8.7 ms, median 8.1). CI's virtual Mac showed the same pattern on 6a (median 4.1 ms, p95 19.4 ms), so it can be reproduced there.
    - **It's a fix of its own,** on its own branch: find which frames are slow and why, then fix it. It's outside this PR.
    - **`v0.1.0-alpha.1` is tagged anyway** (2026-11-03, the owner's go-ahead), on `main` at c46d068, with this miss added to its known issues. It's the quiet release the SignPath application needs, and the fix can be in alpha.2 before the announcement.
29. **The owner's manual test, first pass** (2026-11-03, packages from CI run 41661443732 on 5af0e27). §1 the website: pass. §2 and §3: no faults reported. §4 and §5 found four faults, all fixed:
    - **G and F opened the Documents folder, not the browser,** on Windows. `explorer <address>` handled the About sheet's short links, but not the prefilled issue and form addresses. Links now open through `ShellExecuteW`, the shell's own "open", as the `open` and `opener` crates do. That needs `windows-sys`'s `Win32_UI_Shell` feature, which adds no crate. The bench sheet's folder link goes the same way. Tests never open links (decision 7), so this needs the owner's retest.
    - **Report a problem's Close button ran out of the box.** The four buttons need about 545 pixels and the card was 520. The card is 640 now, and both feedback sheets' button rows wrap to a second line if they ever run out of room.
    - **"Crash on purpose?" had a gap in its sentence.** The string was broken across two lines without a `\`, so the second line's indent was part of it. A search found the same in two error messages (`folder_read_only` in the dispatcher, and `coldopen bench` without the app), now fixed too.
    - **A new test, `crates/app/tests/sheet_layout.rs`,** opens every question and sheet and fails when text runs past its card, is squeezed narrower than its longest word (how the Close button failed: a squeezed button draws its label past itself), or has a run of spaces mid-line. It failed on Report a problem before the fix.
30. **Interface size** (2026-11-03, the owner's suggestion that dialog text is small). The palette's **Make the interface larger**, **smaller** and **Interface at normal size** step through 100 to 200%, kept in `settings.json`. It's iced's `scale_factor`, so it scales the whole window like the system's own scale, the pages included. Scaling only the interface's text would mean a size on every one of its few hundred text calls, and the page has its own zoom. No keys: Ctrl+Shift+= types `+` on many layouts. A benchmark always runs at 100%. Specified in `editor.md` §3.
31. **The owner's manual test passes** (2026-11-03, the retest with packages from CI run 41734110827 on 2dae886). Every section passed, the fixes of decision 29 and the interface size included. G and F open the browser now. Two steps wait for the merge: §4 steps 4–5 and §5 steps 6–7, since GitHub reads issue templates from `main` only.
32. **The Windows uninstall sometimes took 11 to 15 seconds on CI** (2026-11-04), against about 2 on most runs, and failed #22's run 41745580352 at 15.4. The installer's close waited "3 seconds" by counting its quarter-second pauses. Each check in between makes a new WMI connection and query, and that time went uncounted. The waits are now by the clock, one connection serves every check, and both logs note how long each step took. CI prints those lines on every run (`release.md` §3.1). The 15-second limit stays.

## New dependencies

| Dependency | Where | Why | License |
| --- | --- | --- | --- |
| `backtrace` 0.3.76, with `addr2line`, `gimli`, `object` and `rustc-demangle` | app | A panic's return addresses. Only the unwinder is called; LTO drops the symbolizer | MIT OR Apache-2.0 |
| `libc` (Unix), `windows-sys` 0.61 (Windows), named directly | app | Which module holds each frame. Both already in the tree. `windows-sys` also has `Win32_UI_Shell` since decision 29, for `ShellExecuteW` | MIT OR Apache-2.0 |
| mdBook 0.5.4 (CI tool; the release binary, checked by SHA-256) | the website | Builds the site from `docs/guide` | MPL-2.0 |
| Playwright 1.48.2 and @axe-core/playwright 4.10.1 (CI only, npm) | the `site` job | Accessibility of every page in light and dark mode | Apache-2.0; MPL-2.0 |
| PyYAML 6.0.2 (CI only) | the `site` job | Checks the issue and discussion templates parse | MIT |
| actions/configure-pages, upload-pages-artifact, deploy-pages | `pages.yml` | Publishing to GitHub Pages | MIT |

## Checks

- `tools/privacy_scan.py` (2026-11-02):
  - with the private patterns, over the tree, all history's metadata, and the PR, issue and release texts: no findings after this step;
  - it catches a planted email address, home folder and private pattern.
- **CI on cb45533 (Linux, branch push): every job passed.** That includes:
  - **the crash report job:** a release build's report was clean, and frame 6 resolved to `coldopen_app::crash::crash_on_purpose`;
  - **Report a problem:** it opened `…/issues/new?template=bug.yml&version=…&os=…&renderer=CPU` through a stand-in `xdg-open`;
  - **the privacy job.**
- **Linux x86-64 package sizes with symbols split off:** .deb 8,835,224, AppImage 12,237,304, Flatpak 7,336,616 bytes. That's 50 to 70 KB more than 6a's, all under 15,000,000.
- **Windows, here:** a release-profile driver crashed on purpose, and `tools/symbolicate.py` resolved its report through dbghelp: `crash_on_purpose` (crash.rs:268), then `crash_now`, `reply` and `global_key`.
- **Tests:** the workspace, 786 passing and 1 ignored, before the references and parameter docs. Core, MCP, CLI and the reference goldens passed after them.
- **The draft PR:** TheYungStump/cold-open#21, opened for CI on all three OSes.

### Final checks (2026-11-03)

- **The gates here, on the form commit (a483736):** `cargo fmt --check` and `clippy --all-targets -D warnings` are clean. `cargo test --workspace --locked`: 794 passed, none failed, 1 ignored. That includes the crash, typing and agent gates, the reference goldens and the crash report tests.
- **CI on 4f80abc,** the PR's run and the branch's: every job passed on Windows, macOS and Linux. That covers the tests, the budgets, the packages, the website, the privacy job, the license boundary, minimum Rust, and the Linux crash report job.
- **The alpha.2 rehearsal:** run 41585943031 from 128a74a, draft release `rehearsal-2`.
  - 13 files plus `SHA256SUMS.txt`, which lists all 13.
  - Every package is under 15,000,000 bytes; the largest is the x86-64 AppImage at 12,233,208.
  - The symbols zip is 124.7 MB, and holds all five builds' symbols: Windows x64 and ARM64, Linux x86-64 and ARM64, and macOS.
  - Here, the Windows x64 zip and installer matched their lines in `SHA256SUMS.txt`.
  - `coldopen --version` from the zip prints `coldopen 0.1.0-alpha.2 (128a74a62fa6)`. Both programs' file details name Cold Open 0.1.0-alpha.2, and the CLI's description is "Cold Open command-line tool".
  - The notes carry the file table with sizes, the line saying the symbols file isn't for installing, and the Code signing policy and Privacy links.
- **Fuzzing:**
  - Run 41585968031 on 128a74a ran every target for ten minutes. Twelve were clean. `crash_scrub` raised the false alarm in decision 26, now fixed.
  - Run 41654862400 reran `crash_scrub` for ten minutes on the fix (a483736): no finds.
- **CI on fc938ff,** the PR's run and the branch's: every job passed on all three OSes.
- **The privacy scan, with the private patterns,** before every commit: no findings.

## What's left

**For the project owner:**

1. **The manual test passed** (decision 31). After the merge: §4 steps 4–5 and §5 steps 6–7 (the issue templates), and the live site.
2. **Make the "Alpha testing" Discussions category** on the web page (decision 27), and remove General, Polls and Show and tell if they aren't wanted. Pages and Discussions are on, and two-factor sign-in is confirmed.
3. **Send the Mac tester** the updated `docs/release/mac-tester.md`.
4. **`v0.1.0-alpha.1` is tagged** (decision 28). Next: OK the Code signing policy and Privacy line in its notes, then send the SignPath application.
5. **Merge #21, then tag `v0.1.0-alpha.2`.** Each needs your go-ahead. After that, the rehearsal branches and drafts can go, when you say so.
6. **The announcements** are drafts for you to read and post (`docs/release/announcements/`). I never post them.

**Deferred:**

- The fountain.io apps-list request waits for a project email.
- Issue labels `crash` and `feedback` don't exist yet, so the templates use `bug` only.
- Crash reports cover panics only. Aborts, segfaults and out-of-memory kills write none (`crash-reports.md`), and the crash journal still recovers the work.
- 6a's known issues remain:
  - on macOS, double-clicking a script starts Cold Open without opening it;
  - on Windows, making Cold Open the default app is the user's choice;
  - the icon is a stand-in.
- Contributions of code wait for a CLA (decision 3 of the owner's).
