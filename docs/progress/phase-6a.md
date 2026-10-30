# Phase 6a progress: release engineering for the alpha

Branch `phase-6a-release`, started 2026-10-30 from `main` (eb84cd3). Pull request: TheYungStump/cold-open#19 (draft). This file is updated and pushed with every step.

Phase 6 is split in two. **6a** (this phase) builds installable packages for every platform the spec lists, with every budget met on real machines. **6b** brings user docs, the website, the tester group, feedback and opt-in crash reports. 6a builds none of 6b, but leaves room for it: the About sheet shows rows for the user guide and for feedback as soon as their addresses are set (`crates/app/src/about.rs`).

## Plan and status

| # | Step | Status |
| --- | --- | --- |
| 0 | Specs first: `release.md` and `bench.md` (new), `journal.md`, `commands.md`, `editor.md` §21 and §22 | done (a46d61f) |
| 1 | One version (`0.1.0-alpha.1`) and the commit, everywhere | done (0a1beca) |
| 2 | Third-party licenses (cargo-about) and the About sheet | done (1a2c1a8) |
| 3 | The bench: `coldopen-app --bench`, `coldopen bench`, Measure performance | done (29e90e8) |
| 4 | The report view's scrolling | done (0e37814) |
| 5 | PDFs off the UI thread: the compare view's and the script's | done (52f9974, 56b3b77, c129768) |
| 6 | First run, read-only and synced folders | done (7771a4c, 332338f) |
| 7 | Packages: Windows installer and zip, macOS dmg, .deb, AppImage, Flatpak; icon; CLI on each OS | done (6e1d6fd) |
| 8 | Workflows: build, install and start every package; release on a tag | in CI (see below) |
| 9 | Manual test, Mac tester's page, README, this report | written; the manual test waits on CI's packages |

## Decisions from the project owner (2026-10-30, before the phase)

1. **Icon:** the owner provides the artwork. Until then a plain stand-in goes everywhere an icon goes, made by `.github/packaging/icon/make_icons.py`. Swapping the art in: export PNGs at 16 to 1024 pixels into a folder, then run `make_icons.py --from <folder>`.
2. **A folder that can't be written:** the script opens as an untitled copy in `Unsaved/`, with a banner.
3. **Flatpak:** `--filesystem=home`.
4. **The release:** a draft rehearsal from the branch first. After the merge go-ahead, the tag `v0.1.0-alpha.1` on `main`, pushed only with the owner's OK.

## Decisions made during the phase

1. **The commit** comes from `COLDOPEN_COMMIT` (CI sets it), else `git`, else `unknown`. It shows in About, `--version` and the bench report, never in a file Cold Open writes, so two builds of one version write the same files.
2. **Workspace dependency requirements** follow the pre-release version (`"0.1.0-alpha.1"`). Cargo then refuses a mismatch on its own.
3. **cargo-about runs in CI only.** It compiles C code (zstd), and this PC's Windows SDK lacks the C runtime headers. Its output is JSON, turned into text by `third_party.py` (our own crates left out, each license text once). Local builds show a note in its place.
4. **iced's `sysinfo` feature** (127 KB on Windows, measured) tells About and the bench which renderer and GPU draw the window, and the CPU and OS. It came in under the 300 KB the plan allowed.
5. **The bench runs as a child process** in the app (`--bench --progress`), so the window stays usable. Windowed runs are the app's own children, with their own temporary folders and vsync off (`ICED_PRESENT_MODE=immediate`); otherwise a 60 Hz screen could never show under 16.7 ms. The report's file names use UTC, as its contents do.
6. **`coldopen bench`** finds the app beside it and runs it. On Windows the app is a windowed program, whose output a terminal doesn't show.
7. **The report view's rows** are one widget drawn like the page view: only the lines in sight, one text call each.
8. **Export a PDF also moved off the UI thread.** The audit found it took 63 ms on this PC for 120 pages. `export.pdf` is now `export_pdf_prepare` (checks and a copy) then `PdfJob::run`. The compare view's PDF is written from the marked pages the view already holds. While one PDF is being written, a second is refused with a banner.
9. **`folder_read_only`** also covers `doc.new` and Save as into such a folder. A journal that already exists keeps its own error, since it may hold work.
10. **Synced folders:** the atomic rename already retried sharing and lock errors for about a second on Windows. 6a adds tests that hold the file as a sync client does, and the manual test covers OneDrive.
11. **Installer tool:** Inno Setup 6, per user (`PrivilegesRequired=lowest`). The CLI goes in `bin\` on the user's PATH (a task, ticked by default). It offers "Open with" for `.fountain` and `.fdx`, and is the default only for `.coldopen`.
12. **macOS:** one universal dmg if under 15 MB, else one per architecture, decided by the packaging step. The CLI sits inside the bundle, and the palette's **Install the command-line tool** links it into `/usr/local/bin` or `~/.local/bin`, never over another program's file.
13. **macOS file associations:** Finder sends a double-clicked document as an Apple Event, which iced and winit don't expose. Handling it would need a small `unsafe` objc2 delegate, so per the plan it is a known issue in the release notes, not done without the owner's approval. The app starts; the file needs Open.
14. **Linux:** builds on Ubuntu 22.04 (glibc 2.35). The .deb is built with `dpkg-deb` and `dpkg-shlibdeps` and names the window libraries winit loads at run time. The AppImage uses appimagetool 1.9.1 and runtime 20251108, pinned by SHA-256. The Flatpak installs the same binaries on freedesktop 25.08.
15. **The window icon** is the 64-pixel PNG, decoded with `miniz_oxide`, which the app already has, rather than iced's image feature. Wayland gets the app id `io.github.theyungstump.coldopen`.
16. **The package readme** is `.github/packaging/TESTING.txt`, rewritten for the alpha and shipped as `README.txt`. **`measure.py`** is no longer used by CI, since the bench replaces it. It is left in place until the owner says to remove it.

## New dependencies

| Dependency | Where | Why | License |
| --- | --- | --- | --- |
| iced feature `sysinfo` (sysinfo 0.33) | app | The renderer, GPU, CPU and OS for About and the bench. +127 KB on Windows | MIT |
| `sysinfo` 0.33, named directly | app | The bench's machine probe. Already in the tree through iced | MIT |
| `miniz_oxide`, named directly | app, and its build script | The third-party list compiled in deflated, and the window icon. Already in the tree | MIT OR Zlib OR Apache-2.0 |
| `winresource` 0.1 | app's build script, Windows only | The .exe's icon and version details | MIT |
| cargo-about 0.9.3 (CI tool) | CI | The third-party license file | MIT OR Apache-2.0 |
| Inno Setup 6, appimagetool 1.9.1, the AppImage type2 runtime, flatpak-builder (CI tools) | CI | The installers and packages | Inno Setup License (free), MIT, MIT, LGPL-2.1+ (build tools only, nothing shipped from them except the AppImage runtime, MIT) |

## Measurements

### This PC (Windows 11, i9-13900K, RTX 4090, 1920×1080), quick bench, before the report view's fix

Every budget passed except the report view's scrolling (p95 32.7 ms):

| Budget | Result |
| --- | --- |
| Open a 120-page script | 42.9 ms |
| Repagination | 20.2 ms |
| Journal write | well under 0.1 ms |
| Save on the UI thread | 1.6, 1.0, 1.2 ms |
| Whole save | 23.6, 8.4, 12.3 ms |
| Cold start | 186 ms |
| Keystroke to screen | 9.4 ms |
| Memory | 75 MiB |

After the fix, the report view scrolls in 7.2 ms a frame (p95 10.3 ms) at 1920×1080 on the CPU renderer, against 11.2 ms for the pages.

### CI: the full bench in the budgets job (3b581d4, 2026-10-30)

The reports are in `docs/progress/phase-6a-bench/`. CI's runners are shared virtual machines: Linux asserts the budgets listed in `docs/specs/bench.md` §5, and the rest are reported.

| Budget | Linux (EPYC, 2 cores, xvfb) | Windows (EPYC, 2 cores, no GPU) | macOS (virtual M1, 3 cores, Metal) | Limit |
| --- | --- | --- | --- | --- |
| Open | 16.3 ms | 46.1 ms | 34.8 ms | 200 ms |
| Repagination | 11.0 ms | 22.4 ms | 19.5 ms | 50 ms |
| Journal write (p95) | 0.010 ms | 0.020 ms | 0.040 ms | 1 ms |
| Save on the UI thread (p95: Fountain, .fdx, project) | 0.87, 0.86, 1.0 ms | 1.2, 0.89, 1.1 ms | **10.2**, 2.4, 2.9 ms | 4 ms |
| Whole save (median) | 10.9, 2.9, 8.0 ms | 30.7, 21.5, 25.2 ms | 19.8, 8.8, 21.3 ms | 50 ms |
| Cold start | 40.5 ms | 164 ms | 813 ms | 1 s |
| Keystroke to screen (p95) | 4.4 ms (CPU) | 8.2 ms (CPU) | 15.8 ms (Metal) | 16 ms |
| Report view scrolling (p95) | 5.2 ms (CPU) | 7.4 ms (CPU) | 13.5 ms (Metal) | 16 ms |
| Memory | 53 MiB | 74 MiB | 137 MiB | 150 MB |

- **Linux and Windows pass every budget.** The report view scrolled at 50 and 81 ms a frame on these runners before the fix.
- **macOS misses one:** a Fountain save's UI-thread time, p95 10.2 ms. That is the slowest of 21 saves on a 3-core virtual machine, whose median was well under 4 ms. The quick run in the packaging job missed more (cold start 1.4 s, keystroke 23.6 ms with Metal) on a runner busy installing. The Mac tester's report, on a real Mac, is the gate.

### Real machines

Pending: the owner's Windows report and the Mac tester's.

## Checks

- **Local, 2026-10-30 (Windows 11, 3b581d4's code):**
  - `cargo test --workspace --locked`: 753 tests, none failing. That includes the crash, typing and agent gates, `old_journals.rs`, and the bench sheet's run of the real bench (`COLDOPEN_BENCH_TEST=1`).
  - `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`, plus clippy of the app for Linux and macOS.
- **Corpus and reference, with the corpus required (release build):**
  - Every corpus .fdx file writes back byte for byte, and title edits check out on 34 files.
  - The reference gate gives phase 4's numbers. Big Fish: wraps 100.0%, resynced breaks 97.4% (max Δ 2 lines), pages 120/120, scene starts 99.5%. The other two reference scripts: 100% on every measure.
- **Fuzzing:** the workflow, run by hand on the branch (41094779485), ten minutes per target. Result pending.
- **CI:** pending the run on the final commit.
