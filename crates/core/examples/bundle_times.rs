//! Measures a script as a Cold Open project bundle (docs/specs/bundle.md):
//! the bundle's size, `doc.open` and the first layout, an edit and
//! `doc.save` in place, and the size of the journal's header, first as
//! converted, then with tracked changes on every fourth scene, made by the
//! writer and by an agent (docs/specs/changes.md). Works on copies in a
//! temporary folder.
//!
//! ```text
//! cargo run --release -p coldopen-core --example bundle_times -- FILE [RUNS]
//! ```
//!
//! Prints numbers only, never script text.

use std::path::Path;
use std::time::Instant;

use coldopen_core::{Author, Command, Dispatcher, Options, journal};
use serde_json::{Value, json};

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn run(d: &mut Dispatcher, by: &Author, c: &str, p: Value) -> Value {
    d.execute_as(Command::from_parts(c, p).unwrap(), by)
        .unwrap_or_else(|e| panic!("{c}: {}", e.message))
}

/// Tracked edits on every fourth scene: the writer inserts a block and adds
/// to an action, an agent deletes the start of another. Returns how many
/// scenes were changed and how many changes are pending.
fn track_a_quarter(d: &mut Dispatcher) -> (usize, usize) {
    let writer = d.writer().clone();
    let agent = Author::agent("measure", Some("text_delete".into()));
    run(d, &writer, "changes.track", json!({ "writer": true }));
    let doc = run(d, &writer, "doc.get", json!({}));
    let blocks = doc["blocks"].as_array().unwrap().clone();
    let mut scene = 0usize;
    let mut changed = 0;
    let mut i = 0;
    while i < blocks.len() {
        if blocks[i]["type"] != "scene_heading" {
            i += 1;
            continue;
        }
        let heading = blocks[i]["id"].clone();
        let actions: Vec<&Value> = blocks[i + 1..]
            .iter()
            .take_while(|b| b["type"] != "scene_heading")
            .filter(|b| b["type"] == "action")
            .collect();
        if scene.is_multiple_of(4) {
            changed += 1;
            run(
                d,
                &writer,
                "block.insert",
                json!({ "after": heading, "type": "action", "text": "A line the writer added." }),
            );
            if let Some(a) = actions.first() {
                let len = a["text"].as_str().unwrap_or("").chars().count();
                run(
                    d,
                    &writer,
                    "text.insert",
                    json!({ "id": a["id"], "at": len, "text": " And more." }),
                );
            }
            if let Some(a) = actions.get(1) {
                let len = a["text"].as_str().unwrap_or("").chars().count();
                if len > 6 {
                    run(
                        d,
                        &agent,
                        "text.delete",
                        json!({ "id": a["id"], "from": 0, "to": 6 }),
                    );
                }
            }
        }
        scene += 1;
        i += 1;
    }
    let pending = run(d, &writer, "changes.list", json!({}))["changes"]
        .as_array()
        .unwrap()
        .len();
    (changed, pending)
}

/// Opens `src` (a copy), edits and saves it in place `runs` times; prints
/// the medians.
fn measure(label: &str, src: &Path, runs: usize) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(src.file_name().unwrap());
    let writer = Author::writer();
    let (mut open, mut layout, mut save) = (Vec::new(), Vec::new(), Vec::new());
    let mut header = 0;
    let mut pages = 0;
    let mut pending = 0;
    for _ in 0..runs {
        std::fs::copy(src, &path).unwrap();
        let _ = std::fs::remove_file(journal::path_for(&path));
        let mut d = Dispatcher::new(Options::default());
        let t = Instant::now();
        run(&mut d, &writer, "doc.open", json!({ "path": path }));
        open.push(ms(t));
        let t = Instant::now();
        pages = d.view().map_or(0, |v| v.layout().pages.len());
        layout.push(ms(t));
        pending = d.session().map_or(0, |s| s.doc.pending().len());
        run(
            &mut d,
            &writer,
            "block.insert",
            json!({ "type": "action", "text": "A measured line." }),
        );
        let t = Instant::now();
        run(&mut d, &writer, "doc.save", json!({}));
        save.push(ms(t));
        drop(d);
        header = std::fs::read(journal::path_for(&path))
            .map(|b| b.iter().position(|&c| c == b'\n').unwrap_or(b.len()))
            .unwrap_or(0);
    }
    let (o, l) = (median(open), median(layout));
    println!(
        "{label}: {} bytes, {pages} pages, {pending} pending changes: open {o:.1} ms + first layout {l:.1} ms = {:.1} ms; save {:.1} ms; journal header {header} bytes (median of {runs})",
        std::fs::metadata(src).unwrap().len(),
        o + l,
        median(save),
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(file) = args.next() else {
        eprintln!("usage: bundle_times FILE [RUNS]");
        std::process::exit(2);
    };
    let runs: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(11);
    let src = std::path::PathBuf::from(file);
    let work = tempfile::tempdir().unwrap();
    let writer = Author::writer();

    // The script as converted, then with tracked changes, each as a bundle.
    let copy = work.path().join(src.file_name().unwrap());
    std::fs::copy(&src, &copy).unwrap();
    let plain = work.path().join("plain.coldopen");
    let tracked = work.path().join("tracked.coldopen");
    let tracked_fdx = work.path().join("tracked.fdx");
    let mut d = Dispatcher::new(Options::default());
    run(&mut d, &writer, "doc.open", json!({ "path": copy }));
    run(&mut d, &writer, "doc.save", json!({ "path": plain }));
    let (scenes, pending) = track_a_quarter(&mut d);
    run(&mut d, &writer, "doc.save", json!({ "path": tracked }));
    // The same document saved over as .fdx keeps its changes in the
    // session, and its journal header copies the change table.
    run(
        &mut d,
        &writer,
        "doc.save",
        json!({ "path": tracked_fdx, "allow_lossy": true }),
    );
    run(
        &mut d,
        &writer,
        "block.insert",
        json!({ "type": "action", "text": "x" }),
    );
    drop(d);
    let fdx_header = std::fs::read(journal::path_for(&tracked_fdx))
        .map(|b| b.iter().position(|&c| c == b'\n').unwrap_or(b.len()))
        .unwrap_or(0);
    println!("{scenes} scenes changed, {pending} pending changes");
    measure("as converted", &plain, runs);
    measure("with tracked changes", &tracked, runs);
    println!(
        "the tracked document saved as .fdx (changes kept in the session): journal header {fdx_header} bytes"
    );
}
