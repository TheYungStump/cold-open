//! "Measure performance…" (docs/specs/editor.md §22, docs/specs/bench.md
//! §2) through the running app.

use std::path::Path;
use std::time::{Duration, Instant};

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot, Message};
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

fn app(home: &Path, out: &Path) -> Headless {
    let p = home.join("lab.fountain");
    std::fs::write(&p, "INT. LAB - NIGHT\n\nRain on the roof.\n").unwrap();
    let mut app = App::new(Boot {
        file: Some(p),
        dirs: Some(Dirs::under(home)),
        bench_command: Some((
            env!("CARGO_BIN_EXE_coldopen-app").into(),
            vec!["--quick".into(), "--out".into(), out.display().to_string()],
        )),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, iced::Size::new(1200.0, 800.0)).expect("a headless renderer")
}

#[test]
fn the_sheet_asks_first_and_esc_closes_it() {
    let home = tempfile::tempdir().unwrap();
    let mut h = app(home.path(), home.path());
    h.run(vec![Message::Action("app.bench".into())]);
    assert_eq!(h.app.bench_status(), Some((false, None)));
    h.frame(1.0);
    h.named(Named::Escape, Modifiers::empty());
    assert_eq!(h.app.bench_status(), None);
}

/// The whole bench, quick, from the sheet: it opens windows of its own, so
/// it runs only when asked (`COLDOPEN_BENCH_TEST=1`; CI's budgets job,
/// which has a display).
#[test]
fn the_sheet_runs_the_bench_and_finds_its_report() {
    if std::env::var_os("COLDOPEN_BENCH_TEST").is_none_or(|v| v != "1") {
        eprintln!("skipped: set COLDOPEN_BENCH_TEST=1 to run the bench from the sheet");
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut h = app(home.path(), out.path());
    h.run(vec![Message::Action("app.bench".into())]);
    h.named(Named::Enter, Modifiers::empty());
    let start = Instant::now();
    let exit = loop {
        h.run(vec![Message::Tick]);
        if let Some((true, Some(code))) = h.app.bench_status() {
            break code;
        }
        assert!(
            start.elapsed() < Duration::from_secs(600),
            "the bench took too long"
        );
        h.frame(1.0);
        std::thread::sleep(Duration::from_millis(250));
    };
    h.frame(1.0);
    assert!(exit == 0 || exit == 1, "the bench could not run: {exit}");
    let names: Vec<String> = std::fs::read_dir(out.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().any(|n| n.ends_with(".json")), "{names:?}");
    assert!(names.iter().any(|n| n.ends_with(".txt")), "{names:?}");
}
