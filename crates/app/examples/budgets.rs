//! Measures the phase 3 budgets (docs/spec.md, size and speed budgets):
//! keystroke to screen, scrolling, repagination, memory and journal
//! latency, with the real app running headless and drawing with tiny-skia.
//! Since 2026-10-19 also the save budgets, for a feature-length script
//! saved as Fountain, as .fdx and as a project with tracked changes: the
//! time a save takes on the UI thread (gated on p95), and the whole save on
//! the worker (gated on the median since 2026-10-21, p95 reported: a shared
//! CI runner's disk can stall one file sync for 100 ms or more).
//!
//! ```text
//! cargo run --release -p coldopen-app --example budgets -- \
//!     [--script FILE] [--json OUT] [--assert]
//! ```
//!
//! Without `--script` it uses the synthetic 119-page script
//! (`layout::synthetic`). `--assert` exits with an error when a budget is
//! missed (CI does this on Linux only). Cold start, the windowed frame
//! times and binary size are measured by CI around the release build
//! (`.github/packaging`). The measurements are the bench's own
//! (`coldopen_app::bench`, docs/specs/bench.md), which a tester runs.

use std::path::PathBuf;
use std::time::Instant;

use coldopen_app::bench::machine::peak_rss_mib;
use coldopen_app::bench::measure::{
    SaveRow, journal, ms, open_and_layout, open_app, saves, scrolling, typing,
};
use iced::Size;
use serde_json::{Value, json};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut script: Option<PathBuf> = None;
    let mut json_out: Option<PathBuf> = None;
    let mut check = false;
    let mut renderer = "tiny-skia".to_string();
    // Only the save budgets, for measuring them alone.
    let mut saves_only = false;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--script" => script = args.next().map(PathBuf::from),
            "--json" => json_out = args.next().map(PathBuf::from),
            "--assert" => check = true,
            "--renderer" => renderer = args.next().unwrap_or_default(),
            "--saves-only" => saves_only = true,
            "--write-synthetic" => {
                // For the windowed benchmark, which opens a file.
                let out = args.next().expect("a path");
                let s = coldopen_layout::synthetic::feature_script(1);
                std::fs::write(out, coldopen_fountain::write(&s).text).unwrap();
                return;
            }
            other => panic!("unknown argument {other}"),
        }
    }
    let work = tempfile::tempdir().unwrap();
    let (name, source) = match &script {
        Some(p) => (
            p.file_name().unwrap().to_string_lossy().into_owned(),
            std::fs::read_to_string(p).unwrap(),
        ),
        None => (
            "synthetic (layout::synthetic, seed 1)".to_string(),
            coldopen_fountain::write(&coldopen_layout::synthetic::feature_script(1)).text,
        ),
    };
    let mut results = serde_json::Map::new();
    let mut rows: Vec<(String, String, String)> = Vec::new();
    if saves_only {
        println!(
            "| Save, {name} | UI thread | Whole save |
| --- | --- | --- |"
        );
        for r in saves(work.path(), &source, 21).expect("the saves") {
            println!(
                "| {} | median {:.2} ms, p95 {:.2} ms (slowest snapshot {:.2}, marker {:.2}, commit {:.2}) | median {:.1} ms, p95 {:.1} ms |",
                r.label,
                r.ui.median,
                r.ui.p95,
                r.steps[0],
                r.steps[1],
                r.steps[2],
                r.whole.median,
                r.whole.p95
            );
        }
        return;
    }

    // Full layout and the incremental update, as the session does them.
    {
        let parsed = coldopen_fountain::parse(&source).script;
        let doc = coldopen_core::Document::from_script(parsed);
        let t = Instant::now();
        let view = coldopen_core::View::new(&doc, 0);
        let full = ms(t);
        let pages = view.layout().pages.len();
        results.insert("pages".into(), json!(pages));
        results.insert("full_layout_ms".into(), json!(full));
        rows.push((
            "Full layout (normalize, source map, layout)".into(),
            format!("{full:.1} ms for {pages} pages"),
            "under 50 ms".into(),
        ));
    }
    // Opening the script: doc.open and the first layout (2026-10-30,
    // docs/specs/bench.md §5).
    let (open, _, _, _) = open_and_layout(work.path(), &source, 11).expect("the open");
    results.insert("open_median_ms".into(), json!(open.median));
    rows.push((
        "Open (doc.open and the first layout), median of 11".into(),
        format!("{:.1} ms", open.median),
        "under 200 ms".into(),
    ));

    // Memory first, before any 4K run can raise the peak: open the
    // script at 1080p and scroll through every page.
    let mem = {
        let home = work.path().join("mem");
        std::fs::create_dir_all(&home).unwrap();
        let file = home.join("script.fountain");
        std::fs::write(&file, &source).unwrap();
        let mut h = open_app(&home, &file, Size::new(1920.0, 1080.0), &renderer)
            .expect("a headless renderer");
        let pages = h.app.d.view().map_or(0, |v| v.layout().pages.len());
        scrolling(&mut h, 1.0, pages * 12);
        h.app.d.discard();
        peak_rss_mib()
    };
    results.insert("memory_mib".into(), json!(mem));
    rows.push((
        "Memory after scrolling every page".into(),
        mem.map_or_else(|| "not measured".into(), |m| format!("{m:.0} MiB")),
        "under 150 MB".into(),
    ));

    let sizes: [(&str, Size, f32); 3] = [
        ("1920×1080", Size::new(1920.0, 1080.0), 1.0),
        ("3840×2160", Size::new(3840.0, 2160.0), 1.0),
        (
            "1920×1080 at 2× (4K pixels)",
            Size::new(1920.0, 1080.0),
            2.0,
        ),
    ];
    let mut keystroke_1080 = f64::MAX;
    for (i, (label, size, scale)) in sizes.iter().enumerate() {
        let home = work.path().join(format!("home{i}"));
        std::fs::create_dir_all(&home).unwrap();
        let file = home.join("script.fountain");
        std::fs::write(&file, &source).unwrap();
        let mut h = open_app(&home, &file, *size, &renderer).expect("a headless renderer");
        let pages = h.app.d.view().map_or(0, |v| v.layout().pages.len()) as u32;
        let page = h
            .app
            .place_caret_on_page(90.min(pages * 3 / 4))
            .expect("a speech");
        let (u, f, t) = typing(&mut h, *scale, 60);
        if i == 0 {
            keystroke_1080 = t.p95;
        }
        let s = scrolling(&mut h, *scale, 40);
        results.insert(
            format!("keystroke_{i}"),
            json!({"size": label, "page": page + 1, "update_median": u.median, "update_p95": u.p95,
                   "frame_median": f.median, "frame_p95": f.p95, "total_median": t.median,
                   "total_p95": t.p95, "total_max": t.max}),
        );
        results.insert(
            format!("scroll_{i}"),
            json!({"size": label, "median": s.median, "p95": s.p95, "max": s.max}),
        );
        rows.push((
            format!("Keystroke to screen, {label}, page {}", page + 1),
            format!(
                "median {:.1} ms (update {:.1} + frame {:.1}), p95 {:.1} ms",
                t.median, u.median, f.median, t.p95
            ),
            "under 16 ms".into(),
        ));
        rows.push((
            format!("Scrolling frame, {label}"),
            format!("median {:.1} ms, p95 {:.1} ms", s.median, s.p95),
            "under 16 ms".into(),
        ));
        h.app.d.discard();
    }

    let save_rows = saves(work.path(), &source, 21).expect("the saves");
    for SaveRow {
        label,
        ui,
        whole,
        steps,
        ..
    } in &save_rows
    {
        results.insert(
            format!("save {label}"),
            json!({"ui_median": ui.median, "ui_p95": ui.p95, "ui_max": ui.max,
                   "ui_slowest_snapshot": steps[0], "ui_slowest_marker": steps[1],
                   "ui_slowest_commit": steps[2],
                   "whole_median": whole.median, "whole_p95": whole.p95, "whole_max": whole.max}),
        );
        rows.push((
            format!("Save on the UI thread, {label}"),
            format!(
                "median {:.2} ms, p95 {:.2} ms, max {:.2} ms (slowest snapshot {:.2}, marker {:.2}, commit {:.2})",
                ui.median, ui.p95, ui.max, steps[0], steps[1], steps[2]
            ),
            "p95 under 4 ms".into(),
        ));
        rows.push((
            format!("Whole save, {label}"),
            format!(
                "median {:.1} ms, p95 {:.1} ms, max {:.1} ms",
                whole.median, whole.p95, whole.max
            ),
            "median under 50 ms (p95 for information)".into(),
        ));
    }

    let (app, cli, fsync) = journal(work.path(), 300).expect("the journal");
    results.insert(
        "journal".into(),
        json!({"app_median": app.median, "app_p95": app.p95, "app_max": app.max,
               "cli_median": cli.median, "cli_p95": cli.p95,
               "fsync_median": fsync.median, "fsync_p95": fsync.p95, "fsync_max": fsync.max}),
    );
    rows.push((
        "Journal write per edit, on the UI thread (app mode)".into(),
        format!(
            "median {:.3} ms, p95 {:.3} ms, max {:.2} ms",
            app.median, app.p95, app.max
        ),
        "under 1 ms".into(),
    ));
    rows.push((
        "fsync on the flusher thread".into(),
        format!(
            "median {:.2} ms, p95 {:.2} ms, max {:.2} ms",
            fsync.median, fsync.p95, fsync.max
        ),
        "flush within 250 ms".into(),
    ));
    rows.push((
        "Edit with write and fsync (CLI mode)".into(),
        format!("median {:.2} ms, p95 {:.2} ms", cli.median, cli.p95),
        "(reference)".into(),
    ));

    println!(
        "### Budgets: {name}, {renderer}, {}\n",
        std::env::consts::OS
    );
    println!("| Measure | Result | Budget |\n| --- | --- | --- |");
    for (a, b, c) in &rows {
        println!("| {a} | {b} | {c} |");
    }
    results.insert("script".into(), json!(name));
    results.insert("os".into(), json!(std::env::consts::OS));
    results.insert("renderer".into(), json!(renderer));
    if let Some(out) = json_out {
        std::fs::write(
            out,
            serde_json::to_string_pretty(&Value::Object(results.clone())).unwrap(),
        )
        .unwrap();
    }
    if check {
        let full = results["full_layout_ms"].as_f64().unwrap_or(f64::MAX);
        let mut missed = Vec::new();
        if full >= 50.0 {
            missed.push(format!("full layout {full:.1} ms"));
        }
        if open.median >= 200.0 {
            missed.push(format!("open {:.1} ms", open.median));
        }
        // Keystroke to screen is not asserted here: headless frames redraw
        // the whole window, while the windowed app redraws only what
        // changed. CI measures it in a real window (measure.py).
        let _ = keystroke_1080;
        if mem.is_some_and(|m| m >= 150.0) {
            missed.push(format!("memory {:.0} MiB", mem.unwrap_or(0.0)));
        }
        if app.p95 >= 1.0 {
            missed.push(format!("journal write p95 {:.2} ms", app.p95));
        }
        for SaveRow {
            label, ui, whole, ..
        } in &save_rows
        {
            if ui.p95 >= 4.0 {
                missed.push(format!(
                    "{label} save on the UI thread p95 {:.2} ms",
                    ui.p95
                ));
            }
            // The median, not p95 (2026-10-21, docs/spec.md): the whole
            // save ends in a file sync, and a shared runner's disk can stall
            // one for 100 ms or more, which says nothing about the save's
            // own work. p95 stays in the table, and is noted when it is over.
            if whole.median >= 50.0 {
                missed.push(format!("{label} whole save median {:.1} ms", whole.median));
            } else if whole.p95 >= 50.0 {
                eprintln!(
                    "note: {label} whole save p95 {:.1} ms (max {:.1} ms) is over 50 ms; it is reported, not gated",
                    whole.p95, whole.max
                );
            }
        }
        if !missed.is_empty() {
            eprintln!("budgets missed: {}", missed.join(", "));
            std::process::exit(1);
        }
    }
}
