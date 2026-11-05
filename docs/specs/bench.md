# The bench: budgets on real machines

- **Date:** 2026-10-30; slow frames, and the report view judged with vsync, 2026-11-03.
- **Status:** phase 6a, being implemented in `crates/app` (`coldopen_app::bench`).
- **Sources:**
  - `docs/spec.md`, "Platforms, size and performance": the budget table.
  - `docs/progress/phase-3.md`, "Measured so far": how the budgets were first measured, with `examples/budgets.rs` headless and `.github/packaging/measure.py` windowed.
  - `docs/progress/phase-5a.md` (save budgets, decisions 44–47) and `docs/progress/phase-5b.md` (the report view's scrolling, "Deferred").
  - The phase 6a brief: one command or menu entry, synthetic scripts only, one small report holding the machine and the results and nothing personal.
  - iced 0.14's `ICED_BACKEND` and `ICED_PRESENT_MODE` settings, read in its source.
- A choice where the sources leave room is marked **Resolution**.

## 1. What it is for

The spec's budgets are release targets on every OS. CI runs on shared virtual machines with no GPU, which say little about a writer's computer. The bench lets a tester measure every budget on their own machine in one step, and send back one file.

## 2. Running it

- **From the app:** the palette entry **Measure performance…**. A sheet says what it will do and how long it takes (about two minutes), with **Start** (Enter) and Cancel (Esc). While it runs, the sheet shows each step. At the end it shows the summary, with **Show the report** (it opens the folder holding the two files) and Close.
  - The open script is left alone: the bench runs as a child process (`coldopen-app --bench --progress`), whose windows use their own temporary folders. The app stays usable meanwhile, and Esc stops the bench.
- **From a terminal:** `coldopen bench [--out FOLDER] [--quick]`, which runs the desktop app installed beside the CLI (`docs/specs/release.md` §4), or `coldopen-app --bench [--out FOLDER] [--quick]` directly. **Resolution:** on Windows the app is a windowed program, which a terminal doesn't show output from, so `coldopen bench` is the command to give testers there. It prints the summary and the two file names, and exits with 0 when every budget passed, 1 when one was missed and 2 when the bench couldn't run.
  - `--quick` runs fewer repetitions, for CI's smoke tests. Its numbers are not for judging budgets.
- **Where the files go:** the user's Downloads folder, else the Desktop, else the current folder. `--out` overrides that.
- **The files:** `coldopen-bench-<os>-<arch>-<yyyymmdd-hhmmss>.json` and `.txt` with the same name. **Resolution:** the time is UTC, as in the report, so neither says where the tester lives. **Resolution:** two files of a few kilobytes. A tester attaches both, or just the JSON.

## 3. What it measures

Every run uses the synthetic feature script, `coldopen_layout::synthetic::feature_script(1)`, written as Fountain into a new temporary folder that is removed at the end. Each windowed step runs with its own temporary `COLDOPEN_HOME`, so the tester's settings, recent files and `Unsaved/` are never read or written.

| Budget (`docs/spec.md`) | How | Judged on |
| --- | --- | --- |
| Cold start, under 1 s | The app starts itself 7 times with `COLDOPEN_EXIT_AFTER_FIRST_FRAME`, timing launch to the end of the first frame and exit | Median |
| Open a 120-page script, under 200 ms | `doc.open` plus the first layout, in process, 11 times | Median |
| Keystroke to screen, under 16 ms | (a) Headless: the update for one key plus a full redraw at 1920×1080. (b) Windowed: the app types into a speech on page 90, frame to frame (`COLDOPEN_BENCH=typing`) | Windowed p95; headless for information |
| Full repagination, 120 pages, under 50 ms | A new `View` of the script, 11 times | Median |
| Typical edit's repagination | The incremental update after typing on page 90 | Median, for information |
| Journal write per edit, under 1 ms | 300 edits in the app's durability mode | p95 |
| A save's time on the UI thread, under 4 ms | The snapshot, marker and commit of 21 saves each of the script as Fountain, .fdx and a project with tracked changes | p95 of each |
| A whole save, under 50 ms | The same 21 saves end to end, the write and file sync included | Median of each (p95 reported) |
| Memory with a feature script open, under 150 MB | Peak resident memory of a windowed run that scrolls through every page, a screen at a time (`COLDOPEN_BENCH=memory`). Peak on Windows and Linux; on macOS the size at the end, the most the OS reports without unsafe code | The peak |
| The report view's scrolling, under 16 ms | Windowed, `COLDOPEN_BENCH=reports`, with vsync on a GPU renderer (§6, 2026-11-03) | GPU renderer: p95 within one refresh of the screen, missed refreshes counted. CPU renderer: p95 under 16 ms |
| Scrolling the pages | Windowed, `COLDOPEN_BENCH=scrolling` | p95 against 16 ms, for information (the spec has no scrolling budget) |

**Renderers.**
- Every windowed step runs with the renderer the app would choose by default on this OS (`auto`, `docs/specs/release.md` §6), and the budgets are judged on that run.
- Where a GPU renderer can start and isn't the default, the windowed steps run with it too, reported for information.
- **Resolution:** windowed runs set `ICED_PRESENT_MODE=immediate`. Without it, the GPU renderer waits for the screen's refresh, and a 60 Hz screen could never show a frame-to-frame time under 16.7 ms, whatever the app's own work. The CPU renderer doesn't wait. On macOS, Metal still makes some frames wait for the refresh, so since 2026-11-03 the report view's runs use vsync instead and count refreshes (§6).
- **Windowed runs** ask the system which renderer drew the window only after their frames are measured, and read the peak memory before asking (2026-11-02; `editor.md` §22 says why).
- **Windowed runs** use a 1280×860 window, the app's default size, at the screen's own scale. **Resolution:** not 1920×1080 as in CI, because a laptop's screen may be smaller than that, and a window larger than the screen measures pixels nobody sees.

## 4. The report

**JSON** (`"bench": 1` is the format's version):

```json
{
  "bench": 1,
  "app": {"version": "0.1.0-alpha.1", "commit": "0123456789ab", "target": "x86_64-pc-windows-msvc"},
  "when": "2026-10-30T14:05:00Z",
  "machine": {
    "os": "windows", "os_version": "10.0.22631", "arch": "x86_64",
    "cpu": "…", "cores": 16, "memory_gib": 32,
    "gpus": ["…"], "renderer_default": "tiny-skia",
    "screen": {"width": 1920, "height": 1080, "scale": 1.0}
  },
  "script": {"name": "synthetic feature script, seed 1", "pages": 119, "blocks": 3412},
  "budgets": [
    {"id": "cold_start", "value_ms": 140.2, "stat": "median", "limit_ms": 1000, "pass": true, "runs": 7}
  ],
  "info": [
    {"id": "keystroke_headless", "renderer": "tiny-skia", "median_ms": 9.8, "p95_ms": 12.1}
  ],
  "passed": true
}
```

- **`budgets`** holds one entry per budget in §3, with `id`, `value_ms` (or `value_mib` for memory), `stat`, `limit_ms` (or `limit_mib`), `pass`, `runs`, and `renderer` where it applies.
  - The windowed p95 budgets also carry `median_ms` (2026-11-02), which tells a few slow frames from every frame being slow. The text report shows it in brackets.
  - They also carry `slow_frames` (2026-11-03): each frame that took longer than the limit, as `[frame, ms]`, numbered from 1 in the order measured, so the warm-up isn't counted. It shows whether the slow frames come together, at a steady rhythm or at random. It is left out when no frame was over. The text report lists them under the table, one line per budget, the first 20 of each. Under vsync, a slow frame is one that missed a refresh (§6).
  - A budget judged with vsync (§6) carries `refresh_hz` and `missed_refreshes`. Its `limit_ms` is one refresh. The text report says "within a refresh" and the screen's rate.
  - A run on a GPU renderer carries `work_median_ms` and `work_p95_ms` (2026-11-03): the frames' times less their wait for a drawable, for diagnosis only (§6). The text report lists them in a section of their own.
  - A step that couldn't run has `"pass": null` and a `"skipped"` reason drawn from a fixed list: `no_window`, `no_gpu`, `timeout`, `failed`.
- **`info`** holds the numbers kept for comparison: other renderers, headless frames, p95 of the whole save, page scrolling, and (2026-11-02) each UI-thread step of each kind of save, as `save_ui_<kind>_snapshot`, `_marker` and `_commit`. Since 2026-11-04 it also holds the worker's steps, as `save_worker_<kind>_render`, `_write` (the atomic write) and `_finish` (the wait for the marker's flush, then the next journal), so the whole save shows where its time went. It also holds `edit_after_save_<kind>`: the edit typed right after each save but the first, while that save's tidying may still be going on (`journal.md`, "Starting the journal again off the UI thread"). A windowed run's entry carries `refresh_hz`, `missed_refreshes` and the work figures when they apply, as a budget does.
- **`passed`** is true only when every budget passed and none was skipped.

**Text:** the same in a table, as the summary shows it, with one line per budget: name, result, limit, PASS, MISS or SKIPPED.

**Nothing personal.** The report holds only what is listed above. It never holds:

- a path, file name, user name, computer name or network address;
- the time zone, or any script text;
- the serial number or unique id of a device.

The CPU and GPU names are the model names the OS reports. `when` is in UTC. The temporary folder's path never appears, even in a `skipped` reason. A test runs the bench in quick mode and fails if either file contains the home folder, the user's name, the computer's name, the temporary folder or any line of the synthetic script.

## 5. What CI asserts and what only real machines judge

CI's runners are shared virtual machines with no GPU and a slow, shared disk. They run the bench on every pull request (`build.yml`, after the install smoke test) and keep both files as an artifact.

| Budget | CI (Linux) | CI (macOS, Windows) | Real machines |
| --- | --- | --- | --- |
| Open | asserted | reported | judged |
| Full repagination | asserted | reported | judged |
| Journal write | asserted | reported | judged |
| Save on the UI thread | asserted (p95) | reported | judged |
| Whole save | asserted (median) | reported | judged |
| Memory | asserted (CPU renderer) | reported | judged |
| Cold start | reported | reported | judged |
| Keystroke to screen | reported | reported | judged |
| Report view's scrolling | asserted (windowed, CPU renderer; since phase 6a's fix, p95 about 5 ms against 16) | reported | judged |
| Installer size | asserted on every OS (`docs/specs/release.md` §7) | asserted | — |
| Page breaks across OSes | asserted on every OS (layout goldens) | asserted | — |

- **Why some are reported only:** cold start and keystroke on a runner measure its virtual display and the CPU it is lent, not the app. Asserting them would fail builds at random. macOS and Windows runners vary more than Linux ones, so the headless budgets are asserted on Linux only, as since phase 3.
- **The release gate** (`docs/spec.md`, milestone 6) is the real machines' reports: the project owner's Windows PC, the macOS tester, and Linux from CI plus any Linux machine available. Their files go into `docs/progress/` with the phase's report.

## 6. Metal's wait for a drawable (2026-11-03)

- **Status:** the project owner chose option 2, judging with vsync (2026-11-03). What the budget measures now is at the end of this section.
- **Sources:**
  - The Mac tester's reports in `docs/progress/phase-6a-bench/` (`…macos-aarch64-20261102-035521`; `…20261103-042802` is on `phase-6b-alpha`).
  - A probe on CI's Mac (GitHub Actions run 41659857828, branch `claude/probe-mac-frames`). It recorded every frame of the bench, split at the frame's redraw, and timed each present in a patched copy of iced_wgpu.
  - The source of iced_wgpu 0.14 (its compositor) and wgpu-hal 27 (its Metal surface).

### What Metal does

- A frame is drawn into a *drawable*, a texture the window's Metal layer lends the app. iced_wgpu asks wgpu for a frame latency of 1, which wgpu's Metal backend turns into a pool of 2 drawables.
- `immediate` turns off the layer's display sync, so a finished frame goes to the window server at once instead of at the next refresh. But the window server still composites the window at the display's refresh, and keeps a drawable until it has used it.
- So when the app draws faster than the display shows frames, it sometimes asks for a drawable while the window server holds both. The app then waits for the next refresh to release one, up to a whole refresh.
- The CPU renderer never waits like this: it hands the layer a finished image.

### The evidence

The report view on CI's Mac (3 virtual cores, a paravirtual GPU, a 60 Hz virtual screen), three runs of 120 frames for each row:

| Run | Median | p95 | Frames over 16 ms | Of those, mostly waiting for a drawable |
| --- | --- | --- | --- | --- |
| Metal, `immediate` (as the bench runs) | 3.1–6.0 ms | 14.3–30.6 ms | 4, 11, 21 | 4, 11, 18 |
| Metal, with vsync | 16.8–18.1 ms | — | — | the wait's median 15.1–16.0 ms, one refresh |
| Metal, `immediate`, 3 drawables | 2.1–2.6 ms | 13.2–22.5 ms | 5, 6, 8 | 5, 5, 7 |
| tiny-skia | 6.9–12.4 ms | 12.3–22.2 ms | 3, 4, 27 | none (no drawables) |

- **With that wait left out,** Metal's frames took a median of 1.5–1.9 ms and a p95 of 5.6–10.8 ms. Drawing and submitting a frame took about 1 ms, and the app's update and view under 0.3 ms.
- **The slow frames fall at random steps.** They don't come at the jump back to the top, nor with the caret's tick.
- **With vsync on, the same wait is a whole refresh.** That is the wait `immediate` was meant to leave out (§3).
- **Page scrolling shows it too:** 11, 13 and 15 slow frames, 37 of the 39 mostly waiting.
- **CI's runner adds stalls of its own,** of 50 to 120 ms, with every renderer. Its GPU also takes about 9 ms to finish each frame, which a real GPU doesn't, and that lengthens the waits. So CI shows the mechanism but can't judge the budget.

**The Mac tester's numbers fit the same mechanism.** The CPU renderer scrolled the report view steadily (median 8.1, p95 8.7 ms). Metal's frames were mostly fast (median 5.9 ms), with at least 7 of 120 over 20 ms: a wait of up to one 60 Hz refresh (16.7 ms) on top of a frame of a few ms. Keystroke and page scrolling on Metal were spiky the same way (medians 5.7 and 7.8 ms, p95 10.5 and 14.3 ms), but passed.

### What follows

On Metal, the windowed frame times count time spent waiting for the display, which §3 means to leave out. Writers run with vsync, where every frame waits for the refresh anyway. What the budget asks is whether the app's own work fits within one.

**Options** (for the project owner; nothing changed yet):

1. **Leave the wait out of the GPU renderer's frame times.** Time how long each present waits for a drawable, and subtract it. This keeps p95 against 16 ms, and the detail of the numbers. iced_wgpu 0.14 has no hook for it, so it needs a small patch to iced_wgpu carried in the repo until iced has one.
2. **Judge the GPU renderer with vsync on, as writers run it:** a frame is late when it takes longer than one refresh. This needs no patched dependency, but it's a different statistic, and a frame on time says nothing about how much room was left.
3. **Keep the measurement,** and record the Mac's miss as the window server's wait.

### What the budget measures (2026-11-03, the project owner's decision)

The project owner chose option 2, and asked for the app's own work beside it, for diagnosis only.

- **On a GPU renderer, the report view scrolls with vsync on** (`ICED_PRESENT_MODE=fifo`), as writers run it. The budget asks whether frames make their refresh. It passes when the p95 frame-to-frame time is within one refresh of the screen the run was on: 16.7 ms at 60 Hz, 8.3 ms at 120 Hz.
  - **Resolution (2026-11-03, the project owner's decision to keep this allowance):** under vsync a frame can only appear at a refresh, so frame times bunch at whole refreshes, give or take the timer's jitter around each refresh. A p95 of 16.8 ms at 60 Hz is a frame that made its refresh, and holding it to exactly 16.67 ms would fail at random. So a frame time counts as one refresh until it is halfway to the next: the p95 passes when it is under 1.5 refreshes, and a frame over that waited for at least one more refresh.
  - **Missed refreshes are counted:** each frame time is rounded to whole refreshes, one is taken off, and the rest are added up over the run. Each frame that missed one is a slow frame (§4).
- **The refresh is found by the run itself.** Before its warm-up, the run draws 30 frames with nothing changing, and the median of their times is the refresh.
  - **Resolution:** iced doesn't report the screen's refresh rate, and the OS calls that do need unsafe code or new dependencies. Idle frames under vsync each wait for the refresh, so their median is the refresh.
  - **Vsync is judged only when that median is between 4 and 22.2 ms** (45 to 250 Hz). A shorter median means presentation isn't synced, as on some virtual screens. A longer one can't be told apart from frames too slow to keep up. Either way the run is judged against 16 ms, as before.
- **The CPU renderer doesn't sync to the screen,** so it's judged against 16 ms as before. It's the default on Windows and Linux. Where they run the GPU renderer for comparison, its report-view run uses vsync too and is reported the same way.
- **The app's own work per frame, for diagnosis only:** each frame's time less the time it waited for a drawable, as a median and p95. It's reported for every run on a GPU renderer, and judges nothing.
  - **How the wait is timed, with no patched dependency:** wgpu marks the call that hands out the next drawable (`Surface::get_current_texture`) with a profiling scope. The app turns on the `profiling` crate's tracing backend, and in bench runs only installs a small tracing subscriber that times that one scope and turns every other scope off. Outside the bench no subscriber is installed, so the scopes cost a check of a level that is off.
- **Keystroke to screen and page scrolling** still run with `immediate`, unchanged.
- **CI's Mac** shows the mechanism but can't judge the budget (above): its GPU takes about 9 ms to finish a frame, and its runner stalls on its own. It stays reported, not asserted.
