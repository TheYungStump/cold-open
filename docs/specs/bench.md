# The bench: budgets on real machines

- **Date:** 2026-10-30
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
  - The open script is left alone. **Resolution:** the bench starts only with no unsaved changes, and asks to save or discard them first, as Close does. Its windows come and go during the run.
- **From a terminal:** `coldopen-app --bench [--out FOLDER] [--quick]`. It prints the summary and the two file names, and exits with 0 when every budget passed, 1 when one was missed and 2 when the bench couldn't run.
  - `--quick` runs fewer repetitions, for CI's smoke tests. Its numbers are not for judging budgets.
- **Where the files go:** the user's Downloads folder, else the Desktop, else the current folder. `--out` overrides that.
- **The files:** `coldopen-bench-<os>-<arch>-<yyyymmdd-hhmmss>.json` and `.txt` with the same name, using the local time. **Resolution:** two files of a few kilobytes. A tester attaches both, or just the JSON.

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
| Memory with a feature script open, under 150 MB | Peak resident memory of a windowed run that scrolls through every page | The peak |
| The report view's scrolling, under 16 ms | Windowed, `COLDOPEN_BENCH=reports` | p95 |
| Scrolling the pages | Windowed, `COLDOPEN_BENCH=scrolling` | p95 against 16 ms, for information (the spec has no scrolling budget) |

**Renderers.**
- Every windowed step runs with the renderer the app would choose by default on this OS (`auto`, `docs/specs/release.md` §6), and the budgets are judged on that run.
- Where a GPU renderer can start and isn't the default, the windowed steps run with it too, reported for information.
- **Resolution:** windowed runs set `ICED_PRESENT_MODE=immediate`. Without it, the GPU renderer waits for the screen's refresh, and a 60 Hz screen could never show a frame-to-frame time under 16.7 ms, whatever the app's own work. The CPU renderer doesn't wait.
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
  - A step that couldn't run has `"pass": null` and a `"skipped"` reason drawn from a fixed list: `no_window`, `no_gpu`, `timeout`, `failed`.
- **`info`** holds the numbers kept for comparison: other renderers, headless frames, p95 of the whole save, page scrolling.
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
| Report view's scrolling | asserted once phase 6a's fix holds a margin in CI (windowed, CPU renderer) | reported | judged |
| Installer size | asserted on every OS (`docs/specs/release.md` §7) | asserted | — |
| Page breaks across OSes | asserted on every OS (layout goldens) | asserted | — |

- **Why some are reported only:** cold start and keystroke on a runner measure its virtual display and the CPU it is lent, not the app. Asserting them would fail builds at random. macOS and Windows runners vary more than Linux ones, so the headless budgets are asserted on Linux only, as since phase 3.
- **The release gate** (`docs/spec.md`, milestone 6) is the real machines' reports: the project owner's Windows PC, the macOS tester, and Linux from CI plus any Linux machine available. Their files go into `docs/progress/` with the phase's report.
