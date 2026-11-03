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

## What wasn't changed, and why

The budget and the measurement are as they were. The brief says that if the measurement counts time spent waiting on the display, it's written up with the evidence and the project owner decides before it changes. `docs/specs/bench.md` §6 lists three options:

1. leave the wait out of the GPU renderer's frame times (a small patch to iced_wgpu);
2. judge the GPU renderer with vsync on, by frames that take longer than a refresh;
3. keep the measurement, and record the Mac's miss as the window server's.

## Still to do

- The owner's choice, then the change it calls for.
- CI's macOS bench showing the report view under the limit over a few runs, and the Mac tester's rerun on a real Mac.
- Delete the probe branch once it's no longer needed.
