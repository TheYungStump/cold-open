//! Where a save in place spends its time, step by step, for each kind of
//! file, saved as the bench saves them (crates/app/src/bench/measure.rs,
//! `saves`): the synthetic feature script as Fountain, as .fdx written by
//! Cold Open, and as a project with tracked changes on every fourth scene,
//! one save after each edit, with the desktop app's deferred journal.
//!
//! For each save it times the UI thread's three steps (the snapshot, the
//! marker and the commit) and the worker's (rendering, the atomic write,
//! and the finisher: the wait for the marker's flush, then the next
//! journal), and notes whether the last save's tidying (the old journal
//! closed, the next renamed into place) had finished when the marker was
//! appended. With DELAY_MS, each save waits that long after the edit
//! before it starts, as a writer pausing would.
//!
//! ```text
//! cargo run --release -p coldopen-core --example save_steps -- [RUNS] [DELAY_MS]
//! ```
//!
//! Prints numbers only, never script text.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use coldopen_core::command::DocSave;
use coldopen_core::io::SourceStyle;
use coldopen_core::{Command, Dispatcher, Durability, Options};
use serde_json::{Value, json};

static EVENTS: Mutex<Vec<(Instant, &'static str)>> = Mutex::new(Vec::new());

fn hook(step: &str) {
    let name = match step {
        "next_written" => "next_written",
        "switched" => "switched",
        "tidy_started" => "tidy_started",
        "old_closed" => "old_closed",
        "renamed" => "renamed",
        "tidied" => "tidied",
        _ => "other",
    };
    EVENTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((Instant::now(), name));
}

/// When `step` last happened at or after `since`.
fn last_event(step: &str, since: Instant) -> Option<Instant> {
    EVENTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .rev()
        .find(|(t, s)| *s == step && *t >= since)
        .map(|(t, _)| *t)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn stats(mut v: Vec<f64>) -> String {
    if v.is_empty() {
        return "-".into();
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    format!(
        "median {:6.2}  p95 {:6.2}  max {:6.2}",
        at(0.5),
        at(0.95),
        v[v.len() - 1]
    )
}

fn run(d: &mut Dispatcher, c: &str, p: Value) -> Value {
    d.execute(Command::from_parts(c, p).unwrap())
        .unwrap_or_else(|e| panic!("{c}: {}", e.message))
}

/// The median and slowest of `runs` runs of `f`, in ms.
fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, f64) {
    let mut v = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        std::hint::black_box(f());
        v.push(ms(t.elapsed()));
    }
    v.sort_by(f64::total_cmp);
    (v[v.len() / 2], v[v.len() - 1])
}

#[derive(Default)]
struct Steps {
    snapshot: Vec<f64>,
    render: Vec<f64>,
    marker: Vec<f64>,
    write: Vec<f64>,
    finisher: Vec<f64>,
    commit: Vec<f64>,
    whole: Vec<f64>,
    /// From the last commit's end to this marker's start.
    since_commit: Vec<f64>,
    /// Markers appended while the last save's tidying was still going on,
    /// and after it had finished.
    marker_during_tidy: Vec<f64>,
    marker_after_tidy: Vec<f64>,
    /// How long the tidying took, from the commit's switch.
    tidy: Vec<f64>,
    /// The edit right after a save (its journal entry included), while the
    /// tidying was still going on, and after.
    edit_during_tidy: Vec<f64>,
    edit_after_tidy: Vec<f64>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let runs: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(21);
    let delay = Duration::from_millis(args.next().and_then(|n| n.parse().ok()).unwrap_or(0));
    coldopen_core::journal::set_restart_hook(hook);

    let work = tempfile::tempdir().unwrap();
    let dir = work.path();
    let source = coldopen_fountain::write(&coldopen_layout::synthetic::feature_script(1)).text;
    let fountain = dir.join("saves.fountain");
    std::fs::write(&fountain, &source).unwrap();
    let fdx = dir.join("saves.fdx");
    let project = dir.join("saves.coldopen");
    {
        let mut d = Dispatcher::new(Options::default());
        run(&mut d, "doc.open", json!({ "path": fountain }));
        run(
            &mut d,
            "export.fdx",
            json!({ "path": fdx, "allow_lossy": true }),
        );
        run(&mut d, "changes.track", json!({ "writer": true }));
        let blocks = run(&mut d, "doc.get", json!({}))["blocks"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let headings = blocks.iter().filter(|b| b["type"] == "scene_heading");
        for (k, b) in headings.enumerate() {
            if k.is_multiple_of(4) {
                run(
                    &mut d,
                    "block.insert",
                    json!({ "after": b["id"], "type": "action", "text": "A line added later." }),
                );
            }
        }
        run(&mut d, "doc.save", json!({ "path": project }));
        d.discard();
    }

    // The disk on its own: a small flush, as the journal's, and a flush of
    // a file the size of a save.
    {
        use std::io::Write;
        let mut f = std::fs::File::create(dir.join("fsync.bin")).unwrap();
        let mut small = Vec::new();
        for _ in 0..runs {
            f.write_all(&[b'x'; 200]).unwrap();
            let t = Instant::now();
            f.sync_data().unwrap();
            small.push(ms(t.elapsed()));
        }
        let big = vec![b'x'; source.len()];
        let mut whole = Vec::new();
        for _ in 0..runs {
            let mut f = std::fs::File::create(dir.join("big.bin")).unwrap();
            f.write_all(&big).unwrap();
            let t = Instant::now();
            f.sync_all().unwrap();
            whole.push(ms(t.elapsed()));
        }
        println!("disk: a 200-byte flush {}", stats(small));
        println!("      a {}-byte file's flush {}", big.len(), stats(whole));
    }

    for (label, file) in [
        ("Fountain", &fountain),
        (".fdx", &fdx),
        ("project", &project),
    ] {
        let mut d = Dispatcher::new(Options::default());
        d.set_durability(Durability::APP);
        run(&mut d, "doc.open", json!({ "path": file }));
        let session = d.session().unwrap();
        let doc = session.doc.clone();
        let block = doc
            .blocks
            .iter()
            .find(|b| b.element.kind == coldopen_screenplay::ElementKind::Action)
            .unwrap()
            .id;

        // The snapshot's parts, the rendering's parts.
        println!("\n== {label}: {} blocks", doc.blocks.len());
        let (clone, clone_max) = time(200, || doc.clone());
        let mut bare = doc.clone();
        let kept = bare.fdx.take();
        let (clone_bare, clone_bare_max) = time(200, || bare.clone());
        println!(
            "  snapshot (a copy of the document): median {clone:.3} ms, slowest {clone_max:.3}; without the .fdx store {clone_bare:.3}, slowest {clone_bare_max:.3}"
        );
        if let Some(k) = &kept {
            let (kc, kc_max) = time(200, || k.clone());
            println!(
                "  the .fdx store: {} bytes as JSON, {} kept paragraph groups, {} sections, {} loose; a copy of it alone {kc:.3} ms, slowest {kc_max:.3}",
                serde_json::to_vec(k).unwrap().len(),
                k.paragraphs.len(),
                k.sections.len(),
                k.loose.len(),
            );
        }
        let format = coldopen_core::FormatId::for_path(file).unwrap();
        let (write, _) = time(runs, || format.get().write(&doc, SourceStyle::default()));
        let (header, _) = time(runs, || serde_json::to_vec(&doc).unwrap());
        let written = format.get().write(&doc, SourceStyle::default());
        let (losses, _) = time(runs, || session.losses_json(&written.losses));
        let (read, _) = time(runs, || std::fs::read(file).unwrap());
        let (fp, _) = time(runs, || coldopen_core::io::Fingerprint::of(&written.bytes));
        println!(
            "  rendering's parts: the format's writer {write:.2} ms ({} bytes, {} losses); the losses as JSON {losses:.2} ms; reading the file back for the changed-file check {read:.2} ms; one fingerprint {fp:.2} ms; the next journal's header as JSON {header:.2} ms",
            written.bytes.len(),
            written.losses.len()
        );
        if label == "Fountain" {
            let script = doc.to_script();
            let (full, _) = time(runs, || coldopen_fountain::write(&script));
            let (text_only, _) = time(runs, || coldopen_fountain::write_text(&script));
            println!(
                "  inside the Fountain writer: {full:.2} ms, of which reading the text back to check it about {:.2} ms",
                full - text_only
            );
        }

        let mut s = Steps::default();
        let mut last_commit: Option<Instant> = None;
        for i in 0..runs {
            let text = if i % 2 == 0 { "x" } else { "y" };
            let t = Instant::now();
            run(
                &mut d,
                "text.insert",
                json!({ "id": block, "at": 0, "text": text }),
            );
            let edit = ms(t.elapsed());
            let edit_end = Instant::now();
            if let Some(c) = last_commit {
                match last_event("tidied", c) {
                    Some(done) if done <= edit_end => s.edit_after_tidy.push(edit),
                    _ => s.edit_during_tidy.push(edit),
                }
            }
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            let start = Instant::now();
            let job = d.save_prepare(&DocSave::default()).unwrap();
            let t_snapshot = start.elapsed();
            let t = Instant::now();
            let rendered = job.render();
            let t_render = t.elapsed();
            let marker_start = Instant::now();
            let prepared = d.save_mark(rendered).unwrap();
            let t_marker = marker_start.elapsed();
            let t = Instant::now();
            let written = coldopen_core::io::atomic_write(&prepared.target, &prepared.bytes);
            let t_write = t.elapsed();
            let t = Instant::now();
            prepared.finisher().run(written.is_ok());
            let t_finisher = t.elapsed();
            let t = Instant::now();
            d.save_commit(prepared, written).unwrap();
            let end = Instant::now();
            let t_commit = end - t;

            s.snapshot.push(ms(t_snapshot));
            s.render.push(ms(t_render));
            s.marker.push(ms(t_marker));
            s.write.push(ms(t_write));
            s.finisher.push(ms(t_finisher));
            s.commit.push(ms(t_commit));
            s.whole.push(ms(end - start));
            if let Some(c) = last_commit {
                s.since_commit.push(ms(marker_start - c));
                match last_event("tidied", c) {
                    Some(done) if done <= marker_start => s.marker_after_tidy.push(ms(t_marker)),
                    _ => s.marker_during_tidy.push(ms(t_marker)),
                }
            }
            last_commit = Some(end);
        }
        // Each switch to the next journal, paired with the tidying that
        // followed it.
        {
            let events = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
            let mut switched = None;
            for &(t, step) in events.iter() {
                match step {
                    "switched" => switched = Some(t),
                    "tidied" => {
                        if let Some(sw) = switched.take() {
                            s.tidy.push(ms(t - sw));
                        }
                    }
                    _ => {}
                }
            }
        }
        EVENTS.lock().unwrap_or_else(|e| e.into_inner()).clear();
        println!("  UI thread:  snapshot {}", stats(s.snapshot));
        println!("              marker   {}", stats(s.marker));
        println!("              commit   {}", stats(s.commit));
        println!("  worker:     render   {}", stats(s.render));
        println!("              write    {}", stats(s.write));
        println!("              finisher {}", stats(s.finisher));
        println!("  whole save           {}", stats(s.whole));
        println!(
            "  from the last commit to the marker {}",
            stats(s.since_commit)
        );
        println!("  the last save's tidying took {}", stats(s.tidy));
        println!(
            "  markers while it was still tidying: {} ({}); after: {} ({})",
            s.marker_during_tidy.len(),
            stats(s.marker_during_tidy.clone()),
            s.marker_after_tidy.len(),
            stats(s.marker_after_tidy.clone())
        );
        println!(
            "  edits right after a save while it was still tidying: {} ({}); after: {} ({})",
            s.edit_during_tidy.len(),
            stats(s.edit_during_tidy.clone()),
            s.edit_after_tidy.len(),
            stats(s.edit_after_tidy.clone())
        );
        d.shutdown();
    }
}
