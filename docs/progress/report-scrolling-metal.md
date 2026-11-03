# The report view's scrolling on Metal (2026-11-03)

The follow-up to phase 6b's decision 28: why scrolling the report view misses its budget (p95 under 16 ms) with Metal on macOS, while the CPU renderer passes.

## What was seen

- **The Mac tester** (M1 Pro, macOS 26.5, a 3440×1440 screen), on a5d157d: p95 20.3 ms, median 5.9 ms with Metal. At least 7 of the 120 frames took over 20 ms. The CPU renderer scrolled the same view steadily: median 8.1 ms, p95 8.7 ms. On b901828 it was p95 16.3 ms.
- **CI's Mac** on phase 6a: median 4.1 ms, p95 19.4 ms with Metal.

## What was done

1. **The bench says which frames were slow** (`docs/specs/bench.md` §4). The app prints its frame times in order, and each windowed budget's report keeps its frames over the limit, numbered from the first measured. The text report lists them under the table. The Mac tester's next report will show them.
2. **A probe on CI's Mac,** on a throwaway branch (`claude/probe-mac-frames`, run 41659857828), not for merging. It recorded every frame, split each one at its redraw, and noted which scroll step it followed. A patched copy of iced_wgpu timed each present: the wait for a drawable, drawing and submitting, waiting for the GPU (when asked to), and presenting. It ran the report view with Metal as the bench does, with vsync, with 3 drawables instead of 2 and waiting for the GPU, then the CPU renderer, page scrolling and typing, three times each.

## What was found

The slow frames are waits for a Metal drawable: the window server holds both of the app's two drawables until the display's next refresh. They aren't the app's work. The numbers and the reasoning are in `docs/specs/bench.md` §6. In short, for the report view with Metal on CI:

- 33 of the 36 frames over 16 ms spent most of their time waiting for a drawable.
- With that wait left out, frames took a median of 1.5–1.9 ms and a p95 of 5.6–10.8 ms. Drawing and submitting took about 1 ms.
- With vsync on, the same wait takes a whole refresh (median 15–16 ms on CI's 60 Hz screen).
- The slow frames don't fall at the jump back to the top, nor with the caret's tick.

## The owner's decision

The measurement counted time spent waiting on the display, so the brief called for the evidence to be written up and the project owner to decide before anything changed. `docs/specs/bench.md` §6 gave three options. The owner chose the second (2026-11-03):

> Judge with vsync: the report-view budget passes when frames make their refresh, i.e. the p95 frame-to-frame interval is within one refresh period of the display it ran on (8.3 ms at 120 Hz, 16.7 ms at 60 Hz), and it counts missed refreshes. Also report the app's own work per frame with the drawable wait subtracted, for diagnosis only. Use the same method on Windows and Linux wherever presentation is vsynced, and note in the spec (dated) what the budget measures.

## What changed

`docs/specs/bench.md` §6 ("What the budget measures") is the rule. In short:

- **The report view's runs use vsync** (`ICED_PRESENT_MODE=fifo`). On a GPU renderer, the p95 passes within one refresh, and missed refreshes are counted. The slow frames are those that missed one.
- **The refresh is measured:** each report-view run first draws 30 frames with nothing changing, and their median is the refresh. Vsync is judged only when that median is a rate of 45 to 250 Hz. Otherwise, and on the CPU renderer, the run is judged against 16 ms as before.
- **The app's own work per frame** (the frame's time less its wait for a drawable) is reported for every GPU run, for diagnosis only.
- **No patched dependency.** wgpu already marks the drawable acquire with a `profiling` scope. The app turns on `profiling`'s tracing backend, and in bench runs installs a small tracing subscriber (`crates/app/src/bench/acquire.rs`) that times that one scope. Both crates were already compiled into the app: `profiling` through wgpu, `tracing` through winit and softbuffer. The only crate new to `Cargo.lock` is `profiling-procmacros`, which stays off and isn't built.
- **Keystroke and page scrolling** still run with `immediate`. They gain the work figure, but their rule is unchanged.

### Decisions made along the way

1. **"Within one refresh" means under 1.5 refreshes.** Under vsync, frame times bunch at whole refreshes, with timer jitter either side, so a p95 of 16.8 ms at 60 Hz is one refresh. Holding it to exactly 16.67 ms would fail at random.
2. **The refresh is measured, not asked for.** iced doesn't report the screen's rate, and the OS calls need unsafe code or new crates. Idle frames under vsync each wait for the refresh, so their median is the refresh.
3. **Only the report view's budget changed,** as the owner's words say. Keystroke to screen keeps `immediate` and 16 ms. Whether it should be judged with vsync too is a question for the owner.
4. **On Windows and Linux the default renderer is the CPU renderer,** which doesn't sync to the screen, so the report-view budget there is unchanged. Their GPU comparison runs use vsync and report refreshes.

## Measured with the new rule (b45b407)

| Where | Report view | Refreshes missed | Own work (median, p95) |
| --- | --- | --- | --- |
| This PC (Windows, RTX 4090, Vulkan, a 144 Hz screen), one run | p95 7.3 ms, median 7.0, within a refresh (6.9 ms) | 1 of 120 | 0.84, 1.22 ms |
| CI's Mac, quick bench in the packages job (30 frames) | p95 19.2 ms, median 16.7, within a refresh (16.6 ms at 60 Hz): pass | 0 | 0.8, 1.3 ms |
| CI's Mac, full bench in the budgets job (120 frames) | p95 32.1 ms, median 16.8, against 17.0 ms at 59 Hz: miss | 13 | 1.2, 3.7 ms |

- **CI's Mac misses refreshes because of the runner, not the app.** Its frames' own work stayed under 4 ms at p95. Its GPU takes about 9 ms to finish each frame, and it stalls on its own: in the same run, cold start missed too (1,205 ms against 1 s). It stays reported, not asserted (`docs/specs/bench.md` §5).
- **Windows and Linux** draw with the CPU renderer by default, so their report-view budget keeps the 16 ms rule. Every budget passed on CI: Windows p95 10.3 ms, Linux 8.8 ms.
  - Windows' GPU comparison runs use CI's software adapter (Dx12). Their idle median was outside 45–250 Hz, so they're reported as unsynced.
- **The packages grew by 30 to 50 KB each** with the tracing backend: Windows x64 installer 8,254,912 bytes (+30,021), macOS arm64 dmg 7,612,024 (+39,613), all under 15 MB.
- **Seen in the new slow-frame lists:** on Windows and Linux, keystroke's slow frames fell at frames 35 and 70 in both runs, so some work comes every 35 keystrokes (perhaps a line wrap). Both within budget.

## Still to do

- CI's runs of this PR on all three OSes, and the Mac tester's rerun on a real Mac (the final check).
- Delete the probe branch once it's no longer needed.
