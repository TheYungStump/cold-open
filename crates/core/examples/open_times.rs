//! Measures opening and saving a script the way the app does it
//! (docs/spec.md, "Open a 120-page script: under 200 ms"): `doc.open` and
//! the first layout, then an edit and `doc.save` in place, on a copy in a
//! temporary folder. Also prints the size of the journal's header, which
//! holds the document and, for .fdx, what it keeps for other apps.
//!
//! ```text
//! cargo run --release -p coldopen-core --example open_times -- FILE [RUNS]
//! ```
//!
//! Prints numbers only, never script text.

use std::time::Instant;

use coldopen_core::{Command, Dispatcher, Options, journal};
use serde_json::json;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(file) = args.next() else {
        eprintln!("usage: open_times FILE [RUNS]");
        std::process::exit(2);
    };
    let runs: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(11);
    let src = std::path::PathBuf::from(file);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(src.file_name().unwrap());
    let run = |d: &mut Dispatcher, c: &str, p| {
        d.execute(Command::from_parts(c, p).unwrap())
            .unwrap_or_else(|e| panic!("{c}: {}", e.message))
    };
    let (mut open, mut layout, mut save) = (Vec::new(), Vec::new(), Vec::new());
    let mut first_edit = Vec::new();
    let mut header = 0;
    let mut pages = 0;
    for _ in 0..runs {
        std::fs::copy(&src, &path).unwrap();
        let _ = std::fs::remove_file(journal::path_for(&path));
        let mut d = Dispatcher::new(Options::default());
        let t = Instant::now();
        run(&mut d, "doc.open", json!({ "path": path }));
        open.push(ms(t));
        let t = Instant::now();
        pages = d.view().map_or(0, |v| v.layout().pages.len());
        layout.push(ms(t));
        run(
            &mut d,
            "block.insert",
            json!({ "type": "action", "text": "A measured line." }),
        );
        let t = Instant::now();
        run(&mut d, "doc.save", json!({}));
        save.push(ms(t));
        let t = Instant::now();
        run(
            &mut d,
            "block.insert",
            json!({ "type": "action", "text": "Another." }),
        );
        first_edit.push(ms(t));
        // The header the journal started again with at the save, read once
        // the session is gone (the journal is locked while it is open).
        drop(d);
        header = std::fs::read(journal::path_for(&path))
            .map(|b| b.iter().position(|&c| c == b'\n').unwrap_or(b.len()))
            .unwrap_or(0);
    }
    let open_m = median(open);
    let layout_m = median(layout);
    println!(
        "{} bytes, {pages} pages: open {open_m:.1} ms + first layout {layout_m:.1} ms = {:.1} ms; \
         save {:.1} ms; first edit after the save {:.1} ms; journal header {header} bytes \
         (median of {runs})",
        std::fs::metadata(&src).unwrap().len(),
        open_m + layout_m,
        median(save),
        median(first_edit),
    );
}
