"""Measures the windowed app in CI: cold start, and the self-driving typing
and scrolling benchmarks (docs/progress/phase-3.md, measurements), and
since 2026-10-26 scrolling the report view (docs/progress/phase-5b.md).

    python3 .github/packaging/measure.py <app binary> <budgets binary> <out.json>

Each run gets a fresh COLDOPEN_HOME. The app must open a real window, so on
Linux run this under xvfb-run. Prints a Markdown table and writes JSON.
"""

import json
import os
import statistics
import subprocess
import sys
import tempfile
import time


def run(app, env, args, timeout=300):
    full = dict(os.environ)
    full.update(env)
    start = time.perf_counter()
    p = subprocess.run(
        [app, *args], env=full, capture_output=True, text=True, timeout=timeout
    )
    wall = (time.perf_counter() - start) * 1000.0
    if p.returncode != 0:
        raise SystemExit(f"{app} exited with {p.returncode}: {p.stderr[-2000:]}")
    return wall, p.stdout


def main():
    app, budgets, out = sys.argv[1], sys.argv[2], sys.argv[3]
    results = {"os": sys.platform}
    rows = []
    with tempfile.TemporaryDirectory() as work:
        # Cold start: launch to first frame to exit, seven times.
        walls, firsts = [], []
        for i in range(7):
            wall, text = run(
                app,
                {
                    "COLDOPEN_HOME": os.path.join(work, f"cold{i}"),
                    "COLDOPEN_EXIT_AFTER_FIRST_FRAME": "1",
                },
                [],
                timeout=60,
            )
            walls.append(wall)
            for line in text.splitlines():
                if line.startswith("first frame "):
                    firsts.append(float(line.split()[2]))
        results["cold_start_wall_ms"] = statistics.median(walls)
        results["first_frame_ms"] = statistics.median(firsts) if firsts else None
        rows.append(
            (
                "Cold start (launch to first frame and exit, median of 7)",
                f"{statistics.median(walls):.0f} ms",
                "under 1 s",
            )
        )

        script = os.path.join(work, "synthetic.fountain")
        subprocess.run([budgets, "--write-synthetic", script], check=True)
        for renderer in ["tiny-skia", "wgpu"]:
            for kind in ["typing", "scrolling", "reports"]:
                home = os.path.join(work, f"{renderer}-{kind}")
                copy = os.path.join(work, f"{renderer}-{kind}.fountain")
                with open(script, "rb") as a, open(copy, "wb") as b:
                    b.write(a.read())
                try:
                    _, text = run(
                        app,
                        {
                            "COLDOPEN_HOME": home,
                            "COLDOPEN_BENCH": kind,
                            "ICED_BACKEND": renderer,
                            "COLDOPEN_BENCH_SIZE": "1920x1080",
                        },
                        [copy],
                    )
                except (SystemExit, subprocess.TimeoutExpired) as e:
                    rows.append(
                        (f"Windowed {kind}, 1920×1080, {renderer}", f"not measured ({str(e)[:80]})", "")
                    )
                    continue
                line = next((l for l in text.splitlines() if l.startswith("{")), None)
                if line is None:
                    continue
                r = json.loads(line)
                results[f"window_{renderer}_{kind}"] = r
                rows.append(
                    (
                        f"Windowed {kind}, 1920×1080, {renderer} (frame to frame)",
                        f"median {r['median_ms']:.1f} ms, p95 {r['p95_ms']:.1f} ms",
                        "under 16 ms",
                    )
                )

    with open(out, "w") as f:
        json.dump(results, f, indent=2)
    print(f"### Windowed app: {sys.platform}\n")
    print("| Measure | Result | Budget |\n| --- | --- | --- |")
    for a, b, c in rows:
        print(f"| {a} | {b} | {c} |")


if __name__ == "__main__":
    main()
