//! Measures the phase 3 budgets (docs/spec.md, size and speed budgets):
//! keystroke to screen, scrolling, repagination, memory and journal
//! latency, with the real app running headless and drawing with tiny-skia.
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
//! (`.github/packaging`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot};
use coldopen_core::{Command, Dispatcher, Durability, Options};
use coldopen_screenplay::ElementKind;
use iced::keyboard::Modifiers;
use iced::{Event, Point, Size, mouse};
use serde_json::{Value, json};

struct Stats {
    median: f64,
    p95: f64,
    max: f64,
}

fn stats(mut v: Vec<f64>) -> Stats {
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    Stats {
        median: at(0.5),
        p95: at(0.95),
        max: v[v.len() - 1],
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// The process's peak resident memory in MiB, where the OS says it
/// cheaply: the peak on Linux and Windows, the current size on macOS.
fn peak_rss_mib() -> Option<f64> {
    if cfg!(target_os = "linux") {
        let s = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
        let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
        return Some(kb / 1024.0);
    }
    let pid = std::process::id().to_string();
    if cfg!(windows) {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {pid}).PeakWorkingSet64"),
            ])
            .output()
            .ok()?;
        let bytes: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        return Some(bytes / 1024.0 / 1024.0);
    }
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    let kb: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb / 1024.0)
}

fn open(home: &Path, script: &Path, size: Size, renderer: &str) -> Headless {
    let mut app = App::new(Boot {
        file: Some(script.to_path_buf()),
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::with_renderer(app, size, renderer).expect("a headless renderer")
}

/// Puts the caret at the end of a speech on page `target` (or the last
/// page that has one before it).
fn caret_on_page(h: &mut Headless, target: u32) -> u32 {
    let (block, page) = {
        let view = h.app.d.view().expect("a view");
        let layout = view.layout();
        let mut best = None;
        for (e, el) in view.norm.elements.iter().enumerate() {
            if el.kind != ElementKind::Dialogue {
                continue;
            }
            match layout.first_page(e) {
                Some(p) if p <= target => best = Some((view.map[e], p)),
                Some(_) => break,
                None => {}
            }
        }
        best.expect("a speech")
    };
    let doc = &h.app.d.session().expect("a script").doc;
    let b = &doc.blocks[block];
    let c = coldopen_app::editor::Caret {
        block: b.id,
        offset: b.element.text.char_len(),
    };
    h.app.editor.caret = c;
    h.app.editor.anchor = c;
    page
}

/// Keystroke to screen: the update for one typed key (command, journal
/// write, repagination, caret) plus drawing the next frame.
fn typing(h: &mut Headless, scale: f32, keys: usize) -> (Stats, Stats, Stats) {
    let mut update = Vec::new();
    let mut frame = Vec::new();
    let mut total = Vec::new();
    // Warm up: the first frame loads glyphs.
    h.frame(scale);
    let text = "and the sea goes on ";
    for (i, c) in text.chars().cycle().take(keys).enumerate() {
        let s = c.to_string();
        let t = Instant::now();
        let key = iced::keyboard::Key::Character(s.as_str().into());
        h.key(key, Modifiers::empty(), Some(&s));
        let u = ms(t);
        let t2 = Instant::now();
        h.frame(scale);
        let f = ms(t2);
        if i >= 3 {
            update.push(u);
            frame.push(f);
            total.push(u + f);
        }
    }
    (stats(update), stats(frame), stats(total))
}

/// Frame time while scrolling with the wheel over the pages.
fn scrolling(h: &mut Headless, scale: f32, steps: usize) -> Stats {
    let at = Point::new(h.size.width * 0.6, h.size.height * 0.5);
    h.cursor = mouse::Cursor::Available(at);
    h.events(vec![Event::Mouse(mouse::Event::CursorMoved {
        position: at,
    })]);
    h.frame(scale);
    let mut v = Vec::new();
    for _ in 0..steps {
        let t = Instant::now();
        h.events(vec![Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: -4.0 },
        })]);
        h.frame(scale);
        v.push(ms(t));
    }
    stats(v)
}

/// Journal latency: the write on the UI thread in the app's mode, the
/// write plus fsync in the CLI's mode, and a bare fsync.
fn journal(dir: &Path) -> (Stats, Stats, Stats) {
    let run = |durability: Option<Durability>, name: &str| {
        let mut d = Dispatcher::new(Options::default());
        if let Some(du) = durability {
            d.set_durability(du);
        }
        let p = dir.join(name);
        d.execute(Command::from_parts("doc.new", json!({"path": p, "start": "action"})).unwrap())
            .unwrap();
        let id = d.session().unwrap().doc.blocks[0].id;
        let mut v = Vec::new();
        for i in 0..300 {
            let cmd = Command::from_parts("text.insert", json!({"id": id, "at": i, "text": "x"}))
                .unwrap();
            let t = Instant::now();
            d.execute(cmd).unwrap();
            v.push(ms(t));
        }
        d.discard();
        stats(v)
    };
    let app = run(Some(Durability::APP), "app.fountain");
    let cli = run(None, "cli.fountain");
    let mut f = std::fs::File::create(dir.join("fsync.bin")).unwrap();
    let mut v = Vec::new();
    for _ in 0..100 {
        use std::io::Write;
        f.write_all(&[b'x'; 200]).unwrap();
        let t = Instant::now();
        f.sync_data().unwrap();
        v.push(ms(t));
    }
    (app, cli, stats(v))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut script: Option<PathBuf> = None;
    let mut json_out: Option<PathBuf> = None;
    let mut check = false;
    let mut renderer = "tiny-skia".to_string();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--script" => script = args.next().map(PathBuf::from),
            "--json" => json_out = args.next().map(PathBuf::from),
            "--assert" => check = true,
            "--renderer" => renderer = args.next().unwrap_or_default(),
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

    // Memory first, before any 4K run can raise the peak: open the
    // script at 1080p and scroll through every page.
    let mem = {
        let home = work.path().join("mem");
        std::fs::create_dir_all(&home).unwrap();
        let file = home.join("script.fountain");
        std::fs::write(&file, &source).unwrap();
        let mut h = open(&home, &file, Size::new(1920.0, 1080.0), &renderer);
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
        let mut h = open(&home, &file, *size, &renderer);
        let pages = h.app.d.view().map_or(0, |v| v.layout().pages.len()) as u32;
        let page = caret_on_page(&mut h, 90.min(pages * 3 / 4));
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

    let (app, cli, fsync) = journal(work.path());
    results.insert(
        "journal".into(),
        json!({"app_median": app.median, "app_p95": app.p95, "app_max": app.max,
               "cli_median": cli.median, "cli_p95": cli.p95,
               "fsync_median": fsync.median, "fsync_p95": fsync.p95, "fsync_max": fsync.max}),
    );
    rows.push((
        "Journal write on the UI thread (app mode)".into(),
        format!(
            "median {:.3} ms, p95 {:.3} ms, max {:.2} ms",
            app.median, app.p95, app.max
        ),
        "under 10 ms".into(),
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
        // Keystroke to screen is not asserted here: headless frames redraw
        // the whole window, while the windowed app redraws only what
        // changed. CI measures it in a real window (measure.py).
        let _ = keystroke_1080;
        if mem.is_some_and(|m| m >= 150.0) {
            missed.push(format!("memory {:.0} MiB", mem.unwrap_or(0.0)));
        }
        if app.p95 >= 10.0 {
            missed.push(format!("journal write p95 {:.2} ms", app.p95));
        }
        if !missed.is_empty() {
            eprintln!("budgets missed: {}", missed.join(", "));
            std::process::exit(1);
        }
    }
}
