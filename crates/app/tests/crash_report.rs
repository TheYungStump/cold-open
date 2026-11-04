//! Crash reports (docs/specs/crash-reports.md §10): a crash during typing,
//! in the app as its own process, leaves a report that holds none of the
//! typed text, no path and no name; the next start offers it once; "off"
//! writes nothing; and the addresses Report a problem and the crash sheet
//! open hold only what the sheet shows.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use coldopen_app::app::{FeedbackInput, Sheet};
use coldopen_app::files::Dirs;
use coldopen_app::{App, Boot, Message};

/// Words that appear nowhere in Cold Open's code or in Rust's messages,
/// with accents, so `slice` has a character of two bytes to cut into.
const WORDS: [&str; 5] = [
    "Zorvanta",
    "Brindlewick",
    "Ölbergmoor",
    "quellish",
    "Fenwarrow",
];

fn scene_lines() -> Vec<String> {
    let mut v = Vec::new();
    let text = |v: &mut Vec<String>, s: &str| {
        for c in s.chars() {
            v.push(format!("char {c}"));
        }
    };
    text(&mut v, "int. zorvanta tower - night");
    v.push("key enter".into());
    text(
        &mut v,
        "Brindlewick climbs to Ölbergmoor, quellish and slow.",
    );
    v.push("key enter".into());
    v.push("key tab".into());
    text(&mut v, "fenwarrow");
    v.push("key enter".into());
    text(&mut v, "Ölbergmoor again, Brindlewick?");
    v
}

/// Runs the driver with `COLDOPEN_TEST_CRASH` set, sends `lines`, and
/// waits for it to end (a crash) or for `quit`. Returns how many lines it
/// acknowledged and whether it exited cleanly.
fn run_driver(home: &Path, crash: &str, lines: &[String]) -> (usize, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_coldopen-driver"))
        .arg("--home")
        .arg(home)
        .env("COLDOPEN_TEST_CRASH", crash)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "ready");
    let all: String = lines.iter().map(|k| format!("{k}\n")).collect();
    std::thread::spawn(move || {
        let _ = stdin.write_all(all.as_bytes());
        let _ = stdin.flush();
        std::thread::sleep(Duration::from_secs(60));
        drop(stdin);
    });
    let start = Instant::now();
    let mut acked = 0;
    loop {
        line.clear();
        if out.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if let Some(n) = line.trim().strip_prefix("ack ") {
            acked = n.parse().unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(60), "driver too slow");
    }
    let status = child.wait().unwrap();
    (acked, status.success())
}

fn reports(home: &Path) -> Vec<PathBuf> {
    coldopen_app::crash::kept(&coldopen_app::crash::folder(&Dirs::under(home)))
}

/// What a report must never hold: the typed words, the test's folders, the
/// user's name and home folder, the computer's name.
fn assert_private(report: &str, home: &Path) {
    let lower = report.to_lowercase();
    for w in WORDS {
        assert!(!lower.contains(&w.to_lowercase()), "{w} in\n{report}");
    }
    let mut private: Vec<String> = vec![home.to_string_lossy().into_owned()];
    if let Some(name) = home.file_name() {
        private.push(name.to_string_lossy().into_owned());
    }
    for var in [
        "USERNAME",
        "USER",
        "HOME",
        "USERPROFILE",
        "COMPUTERNAME",
        "HOSTNAME",
    ] {
        if let Ok(v) = std::env::var(var)
            && v.chars().count() >= 3
        {
            private.push(v);
        }
    }
    if let Ok(host) = std::process::Command::new("hostname").output() {
        let h = String::from_utf8_lossy(&host.stdout).trim().to_string();
        if h.chars().count() >= 3 {
            private.push(h);
        }
    }
    for p in private {
        assert!(!lower.contains(&p.to_lowercase()), "{p:?} in\n{report}");
    }
    assert!(!report.contains(":\\") && !report.contains("/home/") && !report.contains("/Users/"));
}

#[test]
fn a_crash_while_typing_reports_none_of_the_text() {
    for mode in ["slice", "debug"] {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let mut lines = scene_lines();
        let typed = lines.len();
        lines.push("action app.crash_test".into());
        lines.push("key enter".into());
        let (acked, clean) = run_driver(home, mode, &lines);
        assert!(!clean, "{mode}: the driver should have crashed");
        assert!(
            acked >= typed,
            "{mode}: {acked} of {typed} typed lines acknowledged"
        );

        let kept = reports(home);
        assert_eq!(kept.len(), 1, "{mode}: {kept:?}");
        let name = kept[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("crash-") && name.ends_with("Z.txt"),
            "{name}"
        );
        let report = std::fs::read_to_string(&kept[0]).unwrap();
        eprintln!("{mode}:\n{report}");
        assert_private(&report, home);

        let r = coldopen_core::crash::Report::parse(&report).unwrap();
        assert!(report.starts_with("Cold Open crash report 1\n"));
        assert_eq!(r.version, coldopen_core::build_info::VERSION_LINE);
        assert_eq!(r.thread, "main");
        assert_eq!(r.last_action, "app.crash_test");
        assert!(!r.os.is_empty() && !r.renderer.is_empty() && !r.time.is_empty());
        assert!(r.message.contains("[quoted]"), "{mode}: {}", r.message);
        assert!(!r.frames.is_empty() && r.frames.len() <= coldopen_core::crash::FRAMES_MAX);
        for f in &r.frames {
            assert!(!f.contains('/') && !f.contains('\\'), "{f}");
        }
        // The crash journal still has every acknowledged key: the report is
        // extra, never in the way of recovery.
        let unsaved = Dirs::under(home).unsaved();
        assert!(
            std::fs::read_dir(&unsaved).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".coldopen-journal")),
            "no journal in {}",
            unsaved.display()
        );
    }
}

#[test]
fn reports_off_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let config = Dirs::under(home).config;
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("settings.json"), r#"{"crash_reports": "off"}"#).unwrap();
    let lines = vec![
        "char a".to_string(),
        "action app.crash_test".into(),
        "key enter".into(),
    ];
    let (_, clean) = run_driver(home, "debug", &lines);
    assert!(!clean);
    assert!(reports(home).is_empty());
}

#[test]
fn a_worker_panic_leaves_the_app_running() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let lines: Vec<String> = [
        "char a",
        "action app.crash_test",
        "key enter",
        "char b",
        "quit",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let (acked, clean) = run_driver(home, "worker", &lines);
    assert!(clean, "the app should survive a worker's panic");
    assert_eq!(acked, 4);
    let kept = reports(home);
    assert_eq!(kept.len(), 1);
    let r =
        coldopen_core::crash::Report::parse(&std::fs::read_to_string(&kept[0]).unwrap()).unwrap();
    assert_eq!(r.thread, "coldopen-test-worker");
    assert_eq!(r.message, "Crash on purpose, on a worker thread");
}

fn write_report(home: &Path, stamp: &str) -> PathBuf {
    let folder = coldopen_app::crash::folder(&Dirs::under(home));
    std::fs::create_dir_all(&folder).unwrap();
    let r = coldopen_core::crash::Report {
        version: "0.1.0-alpha.2 (5d6e764a1b2c)".into(),
        target: "x86_64-pc-windows-msvc".into(),
        os: "Windows 11 (26100)".into(),
        renderer: "CPU".into(),
        time: "2026-11-03T12:34:56Z".into(),
        thread: "main".into(),
        last_action: "text.insert".into(),
        location: "crates/app/src/editor/mod.rs:812:22".into(),
        message: "boom".into(),
        frames: vec!["Cold Open.exe+0x1a2b".into()],
    };
    let p = folder.join(format!("crash-{stamp}Z.txt"));
    std::fs::write(&p, r.render()).unwrap();
    p
}

fn app(home: &Path) -> App {
    let a = App::new(Boot {
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    assert_eq!(
        a.opened,
        Some(Vec::new()),
        "a test app never opens a browser"
    );
    a
}

#[test]
fn the_next_start_offers_a_report_once() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let p = write_report(home, "20261103-123456");
    let a = app(home);
    assert_eq!(a.banners.iter().filter(|b| b.crash).count(), 1);
    assert!(!p.exists(), "the report is marked seen");
    assert!(reports(home)[0].to_string_lossy().ends_with(".seen.txt"));
    let again = app(home);
    assert!(again.banners.iter().all(|b| !b.crash), "offered once");
}

#[test]
fn never_ask_again_turns_reports_off() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    write_report(home, "20261103-123456");
    let mut a = app(home);
    let _ = a.update(Message::Action("app.crash_reports_never".into()));
    assert!(a.banners.iter().all(|b| !b.crash));
    assert_eq!(
        a.settings.crash_reports,
        coldopen_app::files::CrashReports::Off
    );
    write_report(home, "20261104-000000");
    let b = app(home);
    assert!(b.banners.iter().all(|b| !b.crash), "off: no offer");
    // The palette turns them on again.
    let mut b = b;
    let _ = b.update(Message::Action("app.crash_reports_toggle".into()));
    assert_eq!(
        b.settings.crash_reports,
        coldopen_app::files::CrashReports::Ask
    );
}

#[test]
fn the_crash_sheet_sends_by_the_browser_and_the_clipboard() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    write_report(home, "20261102-000000");
    write_report(home, "20261103-123456");
    let mut a = app(home);
    let _ = a.update(Message::Action("app.crash_reports".into()));
    assert_eq!(a.sheet, Some(Sheet::CrashReport));
    let _ = a.update(Message::Feedback(FeedbackInput::SendGithub));
    let opened = a.opened.clone().unwrap();
    assert_eq!(opened.len(), 1);
    let url = &opened[0];
    assert!(
        url.starts_with("https://github.com/TheYungStump/cold-open/issues/new?template=crash.yml&")
    );
    assert!(url.len() <= coldopen_app::feedback::URL_MAX);
    assert!(url.contains("&summary=boom%20at%20crates%2Fapp%2Fsrc%2Feditor%2Fmod.rs%3A812%3A22"));
    assert!(!url.contains("0x1a2b"), "frames go by the clipboard");
    // The form, for people without a GitHub account: the same lines.
    let _ = a.update(Message::Feedback(FeedbackInput::SendForm));
    let opened = a.opened.clone().unwrap();
    assert_eq!(opened.len(), 2);
    let url = &opened[1];
    assert!(url.starts_with("https://docs.google.com/forms/d/e/"));
    assert!(url.len() <= coldopen_app::feedback::URL_MAX);
    assert!(url.contains("=Crash&"));
    assert!(url.ends_with("=boom%20at%20crates%2Fapp%2Fsrc%2Feditor%2Fmod.rs%3A812%3A22"));
    assert!(!url.contains("0x1a2b"), "frames go by the clipboard");
    // Older, then delete it: the newer one is left.
    let _ = a.update(Message::Feedback(FeedbackInput::Older));
    let _ = a.update(Message::Feedback(FeedbackInput::Delete));
    let _ = a.update(Message::Reply(coldopen_app::app::Reply::Save));
    let left = reports(home);
    assert_eq!(left.len(), 1);
    assert!(left[0].to_string_lossy().contains("20261103"));
}

#[test]
fn report_a_problem_fills_in_the_details_only() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let mut a = app(home);
    let _ = a.update(Message::Action("app.report_problem".into()));
    assert_eq!(a.sheet, Some(Sheet::ReportProblem));
    let _ = a.update(Message::Feedback(FeedbackInput::Issue));
    assert_eq!(a.sheet, None);
    let opened = a.opened.clone().unwrap();
    assert_eq!(opened.len(), 1);
    let url = &opened[0];
    assert!(
        url.starts_with("https://github.com/TheYungStump/cold-open/issues/new?template=bug.yml&")
    );
    for key in ["version=", "os=", "renderer="] {
        assert!(url.contains(&format!("&{key}")), "{key} in {url}");
    }
    assert_eq!(url.matches('&').count(), 3, "{url}");
    assert_private(&url.replace("%20", " "), home);
    // The form: the kind and the same details.
    let _ = a.update(Message::Action("app.report_problem".into()));
    let _ = a.update(Message::Feedback(FeedbackInput::Form));
    assert_eq!(a.sheet, None);
    let url = a.opened.as_ref().unwrap().last().unwrap().clone();
    assert!(url.starts_with("https://docs.google.com/forms/d/e/"));
    assert!(url.contains("=Bug&"));
    assert_eq!(url.matches('&').count(), 4, "{url}");
    assert_private(&url.replace("%20", " "), home);
    // The user guide opens the site's guide.
    let _ = a.update(Message::Action("app.user_guide".into()));
    assert_eq!(
        a.opened.as_ref().unwrap().last().unwrap(),
        coldopen_app::about::DOCS_URL
    );
}

/// The sheets' keys, through the real key path, as the gate driver and a
/// writer press them.
#[test]
fn the_sheets_answer_their_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    write_report(home, "20261103-123456");
    let mut h = coldopen_app::headless::Headless::new(app(home), iced::Size::new(1280.0, 860.0))
        .expect("a headless renderer");
    h.run(vec![Message::Action("app.report_problem".into())]);
    h.type_text("g");
    assert_eq!(h.app.sheet, None, "G opened the issue and closed the sheet");
    h.run(vec![Message::Action("app.report_problem".into())]);
    h.type_text("f");
    assert_eq!(h.app.sheet, None, "F opened the form and closed the sheet");
    h.run(vec![Message::Action("app.crash_reports".into())]);
    assert_eq!(h.app.sheet, Some(Sheet::CrashReport));
    h.type_text("g");
    h.type_text("f");
    h.named(
        iced::keyboard::key::Named::Escape,
        iced::keyboard::Modifiers::empty(),
    );
    assert_eq!(h.app.sheet, None);
    let opened = h.app.opened.clone().unwrap();
    assert_eq!(opened.len(), 4, "{opened:?}");
    assert!(opened[0].contains("template=bug.yml"));
    assert!(opened[1].contains("=Bug&"));
    assert!(opened[2].contains("template=crash.yml"));
    assert!(opened[3].contains("=Crash&"));
    // Crash on purpose asks first, and Esc keeps the app running.
    h.run(vec![Message::Action("app.crash_test".into())]);
    assert!(matches!(
        h.app.prompt,
        Some(coldopen_app::app::Prompt::CrashTest)
    ));
    h.named(
        iced::keyboard::key::Named::Escape,
        iced::keyboard::Modifiers::empty(),
    );
    assert!(h.app.prompt.is_none());
}
