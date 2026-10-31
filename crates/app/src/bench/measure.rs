//! The budgets measured in the bench's own process, headless
//! (docs/specs/bench.md §3), and the statistics they share. CI's
//! `examples/budgets.rs` calls the same functions.

use std::path::Path;
use std::time::Instant;

use coldopen_core::command::DocSave;
use coldopen_core::{Command, Dispatcher, Durability, Options};
use iced::keyboard::Modifiers;
use iced::{Event, Point, Size, mouse};
use serde_json::{Value, json};

use crate::files::{Dialogs, Dirs};
use crate::headless::Headless;
use crate::{App, Boot};

/// The median, 95th percentile and slowest of some times.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub median: f64,
    pub p95: f64,
    pub max: f64,
}

pub fn stats(mut v: Vec<f64>) -> Stats {
    if v.is_empty() {
        return Stats::default();
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    Stats {
        median: at(0.5),
        p95: at(0.95),
        max: v[v.len() - 1],
    }
}

pub fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn run(d: &mut Dispatcher, c: &str, p: Value) -> Result<Value, String> {
    let cmd = Command::from_parts(c, p).map_err(|e| format!("{c}: {e}"))?;
    d.execute(cmd).map_err(|e| format!("{c}: {}", e.message))
}

/// Opening the script: `doc.open` plus the first layout, which is what the
/// app waits for before drawing; and a full repagination, a new view of the
/// open document. Returns (open, full layout, pages, blocks).
pub fn open_and_layout(
    dir: &Path,
    source: &str,
    runs: usize,
) -> Result<(Stats, Stats, usize, usize), String> {
    let path = dir.join("open.fountain");
    let (mut open, mut layout) = (Vec::new(), Vec::new());
    let (mut pages, mut blocks) = (0, 0);
    for _ in 0..runs {
        std::fs::write(&path, source).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(coldopen_core::journal::path_for(&path));
        let mut d = Dispatcher::new(Options::default());
        let t = Instant::now();
        run(&mut d, "doc.open", json!({ "path": path }))?;
        pages = d.view().map_or(0, |v| v.layout().pages.len());
        open.push(ms(t));
        let doc = &d.session().ok_or("no session")?.doc;
        blocks = doc.blocks.len();
        let t = Instant::now();
        let view = coldopen_core::View::new(doc, 0);
        layout.push(ms(t));
        drop(view);
        d.discard();
    }
    Ok((stats(open), stats(layout), pages, blocks))
}

/// The app, headless, with the script open, in its own folders under
/// `home`.
pub fn open_app(home: &Path, script: &Path, size: Size, renderer: &str) -> Option<Headless> {
    let mut app = App::new(Boot {
        file: Some(script.to_path_buf()),
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::with_renderer(app, size, renderer)
}

/// Keystroke to screen, headless: the update for one typed key (command,
/// journal write, repagination, caret), then drawing the next frame.
/// Returns (update, frame, both).
pub fn typing(h: &mut Headless, scale: f32, keys: usize) -> (Stats, Stats, Stats) {
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

/// Frame time while scrolling the pages with the wheel, headless.
pub fn scrolling(h: &mut Headless, scale: f32, steps: usize) -> Stats {
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
pub fn journal(dir: &Path, edits: usize) -> Result<(Stats, Stats, Stats), String> {
    let one = |durability: Option<Durability>, name: &str| -> Result<Stats, String> {
        let mut d = Dispatcher::new(Options::default());
        if let Some(du) = durability {
            d.set_durability(du);
        }
        let p = dir.join(name);
        run(&mut d, "doc.new", json!({"path": p, "start": "action"}))?;
        let id = d.session().ok_or("no session")?.doc.blocks[0].id;
        let mut v = Vec::new();
        for i in 0..edits {
            let cmd = Command::from_parts("text.insert", json!({"id": id, "at": i, "text": "x"}))?;
            let t = Instant::now();
            d.execute(cmd).map_err(|e| e.message)?;
            v.push(ms(t));
        }
        d.discard();
        Ok(stats(v))
    };
    let app = one(Some(Durability::APP), "app.fountain")?;
    let cli = one(None, "cli.fountain")?;
    let mut f = std::fs::File::create(dir.join("fsync.bin")).map_err(|e| e.to_string())?;
    let mut v = Vec::new();
    for _ in 0..edits.min(100) {
        use std::io::Write;
        f.write_all(&[b'x'; 200]).map_err(|e| e.to_string())?;
        let t = Instant::now();
        f.sync_data().map_err(|e| e.to_string())?;
        v.push(ms(t));
    }
    Ok((app, cli, stats(v)))
}

/// One kind of save measured: the UI thread's time, the whole save, and
/// the slowest snapshot, marker and commit on the UI thread.
pub struct SaveRow {
    pub label: &'static str,
    pub id: &'static str,
    pub ui: Stats,
    pub whole: Stats,
    pub steps: [f64; 3],
}

/// One save of each kind of file, as the desktop app saves (journal.md,
/// "Saving off the UI thread"), `runs` times after an edit: the UI thread's
/// steps (the snapshot, the marker, the commit with the journal's new
/// header) and the whole save, the worker's rendering and write included.
/// The script is `source` as Fountain, as .fdx written by Cold Open, and as
/// a project with tracked changes on every fourth scene.
pub fn saves(dir: &Path, source: &str, runs: usize) -> Result<Vec<SaveRow>, String> {
    let fountain = dir.join("saves.fountain");
    std::fs::write(&fountain, source).map_err(|e| e.to_string())?;
    let fdx = dir.join("saves.fdx");
    let project = dir.join("saves.coldopen");
    let mut d = Dispatcher::new(Options::default());
    run(&mut d, "doc.open", json!({"path": fountain}))?;
    run(
        &mut d,
        "export.fdx",
        json!({"path": fdx, "allow_lossy": true}),
    )?;
    run(&mut d, "changes.track", json!({"writer": true}))?;
    let blocks = run(&mut d, "doc.get", json!({}))?["blocks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let headings = blocks.iter().filter(|b| b["type"] == "scene_heading");
    for (k, b) in headings.enumerate() {
        if k.is_multiple_of(4) {
            run(
                &mut d,
                "block.insert",
                json!({"after": b["id"], "type": "action", "text": "A line added later."}),
            )?;
        }
    }
    run(&mut d, "doc.save", json!({"path": project}))?;
    d.discard();

    let mut out = Vec::new();
    for (label, id, file) in [
        ("Fountain", "fountain", &fountain),
        (".fdx", "fdx", &fdx),
        ("project, tracked changes", "project", &project),
    ] {
        let mut d = Dispatcher::new(Options::default());
        d.set_durability(Durability::APP);
        run(&mut d, "doc.open", json!({"path": file}))?;
        let block = d
            .session()
            .ok_or("no session")?
            .doc
            .blocks
            .iter()
            .find(|b| b.element.kind == coldopen_screenplay::ElementKind::Action)
            .ok_or("no action to type in")?
            .id;
        let (mut ui, mut whole) = (Vec::new(), Vec::new());
        // The slowest snapshot, marker and commit, so a stall shows where
        // it fell.
        let mut steps = [0.0f64; 3];
        let save = |d: &mut Dispatcher| -> Result<[f64; 3], String> {
            let t = Instant::now();
            let job = d.save_prepare(&DocSave::default()).map_err(|e| e.message)?;
            let snapshot = ms(t);
            // The worker.
            let rendered = job.render();
            let t = Instant::now();
            let prepared = d.save_mark(rendered).map_err(|e| e.message)?;
            let mark = ms(t);
            // The worker again, as the app's: the write, then the wait for
            // the journal's flush of the marker.
            let written = coldopen_core::io::atomic_write(&prepared.target, &prepared.bytes);
            prepared.wait_for_journal();
            let t = Instant::now();
            d.save_commit(prepared, written).map_err(|e| e.message)?;
            Ok([snapshot, mark, ms(t)])
        };
        for i in 0..runs {
            let text = if i % 2 == 0 { "x" } else { "y" };
            run(
                &mut d,
                "text.insert",
                json!({"id": block, "at": 0, "text": text}),
            )?;
            let s = save(&mut d)?;
            for (max, v) in steps.iter_mut().zip(s) {
                *max = max.max(v);
            }
            ui.push(s.iter().sum());
        }
        // The whole save, end to end, in a row of its own.
        for i in 0..runs {
            let text = if i % 2 == 0 { "x" } else { "y" };
            run(
                &mut d,
                "text.insert",
                json!({"id": block, "at": 0, "text": text}),
            )?;
            let t = Instant::now();
            save(&mut d)?;
            whole.push(ms(t));
        }
        d.shutdown();
        out.push(SaveRow {
            label,
            id,
            ui: stats(ui),
            whole: stats(whole),
            steps,
        });
    }
    Ok(out)
}
