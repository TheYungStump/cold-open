//! The phase 3 crash gate: the app runs as its own process
//! (`coldopen-driver`), keys go in one at a time, and the process is killed
//! at random points mid-typing, with no chance to save or flush anything.
//! Reopening must give back every key the app had acknowledged, which is
//! every key that reached the screen (docs/specs/journal.md, "What a crash
//! can lose").

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot, Message};
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

const START: &str = "INT. LIGHTHOUSE - NIGHT\n\nThe lamp turns.\n";

/// The keys typed, as driver lines.
fn keys() -> Vec<String> {
    let mut v: Vec<String> = ["key down", "key end", "key enter"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut text = |s: &str| {
        for c in s.chars() {
            v.push(format!("char {c}"));
        }
    };
    text("Waves break below. NORA climbs the last step.");
    let mut v2 = v;
    v2.push("key enter".into());
    v2.push("key tab".into());
    let text2 = |v: &mut Vec<String>, s: &str| {
        for c in s.chars() {
            v.push(format!("char {c}"));
        }
    };
    text2(&mut v2, "nora");
    v2.push("key enter".into());
    text2(&mut v2, "Still turning. Still here.");
    v2.push("key enter".into());
    text2(&mut v2, "eli");
    v2.push("key enter".into());
    v2.push("key tab".into());
    text2(&mut v2, "from below");
    v2.push("key enter".into());
    text2(&mut v2, "Café's closed, Nora. Come down.");
    v2.push("key backspace".into());
    v2.push("key backspace".into());
    text2(&mut v2, "n!");
    v2
}

fn replay_line(h: &mut Headless, line: &str) {
    let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
    match cmd {
        "char" => h.type_text(rest),
        "key" => {
            let k = match rest {
                "enter" => Named::Enter,
                "tab" => Named::Tab,
                "backspace" => Named::Backspace,
                "down" => Named::ArrowDown,
                "end" => Named::End,
                other => panic!("{other}"),
            };
            h.named(k, Modifiers::empty());
        }
        other => panic!("{other}"),
    }
}

type State = Vec<(String, String)>;

fn state(app: &App) -> State {
    app.d
        .session()
        .unwrap()
        .doc
        .blocks
        .iter()
        .map(|b| (format!("{:?}", b.element.kind), b.element.text.plain_text()))
        .collect()
}

fn headless(home: &Path, file: Option<PathBuf>) -> Headless {
    let mut app = App::new(Boot {
        file,
        dirs: Some(Dirs::under(home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    Headless::new(app, iced::Size::new(1280.0, 860.0)).unwrap()
}

/// The document after each prefix of the keys, from a run in this process.
fn expected(script: bool) -> Vec<State> {
    let home = tempfile::tempdir().unwrap();
    let file = script.then(|| {
        let p = home.path().join("lighthouse.fountain");
        std::fs::write(&p, START).unwrap();
        p
    });
    let mut h = headless(home.path(), file);
    let mut out = vec![state(&h.app)];
    for line in keys() {
        replay_line(&mut h, &line);
        out.push(state(&h.app));
    }
    h.app.d.discard();
    out
}

/// A small deterministic generator, so a failure names its run.
fn kill_points(n: usize, runs: usize) -> Vec<usize> {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    (0..runs)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            1 + (x % n as u64) as usize
        })
        .collect()
}

/// Runs the driver, sends every key at once, and kills it as soon as it
/// has acknowledged `kill_at` of them. Returns the acknowledged count.
fn type_and_kill(home: &Path, file: Option<&Path>, kill_at: usize) -> usize {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_coldopen-driver"));
    cmd.arg("--home").arg(home);
    if let Some(f) = file {
        cmd.arg(f);
    }
    let mut child = cmd
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
    let all: String = keys().iter().map(|k| format!("{k}\n")).collect();
    std::thread::spawn(move || {
        let _ = stdin.write_all(all.as_bytes());
        let _ = stdin.flush();
        // Keep stdin open: the driver must not see the end of input.
        std::thread::sleep(Duration::from_secs(60));
        drop(stdin);
    });
    let start = Instant::now();
    let mut acked = 0;
    while acked < kill_at {
        line.clear();
        if out.read_line(&mut line).unwrap() == 0 {
            break;
        }
        if let Some(n) = line.trim().strip_prefix("ack ") {
            acked = n.parse().unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(60), "driver too slow");
    }
    // TerminateProcess on Windows, SIGKILL elsewhere: no destructors, no
    // flushes, nothing the app could do to help.
    child.kill().unwrap();
    child.wait().unwrap();
    acked
}

fn check(states: &[State], recovered: &State, acked: usize, run: usize) {
    let found = states
        .iter()
        .enumerate()
        .skip(acked)
        .find(|(_, s)| *s == recovered);
    eprintln!(
        "run {run}: {acked} of {} keys acknowledged, recovered the state after {:?}",
        states.len() - 1,
        found.map(|(j, _)| j)
    );
    assert!(
        found.is_some(),
        "run {run}: killed after {acked} acknowledged keys, recovered {recovered:#?}\nexpected at least {:#?}",
        states[acked]
    );
}

#[test]
fn killing_the_app_mid_typing_loses_no_acknowledged_key() {
    let states = expected(true);
    let n = states.len() - 1;
    for (run, kill_at) in kill_points(n, 8).into_iter().enumerate() {
        let home = tempfile::tempdir().unwrap();
        let script = home.path().join("lighthouse.fountain");
        std::fs::write(&script, START).unwrap();
        let acked = type_and_kill(home.path(), Some(&script), kill_at);
        assert!(acked >= kill_at, "run {run}: the driver stopped at {acked}");

        // Reopen in the app: the recovery dialog, then Keep.
        let mut h = headless(home.path(), Some(script.clone()));
        let changed = states[acked] != states[0];
        assert_eq!(h.app.recovery.is_some(), changed, "run {run}");
        h.run(vec![Message::RecoveryKeep]);
        check(&states, &state(&h.app), acked, run);
        // The file itself was never touched: no save happened.
        assert_eq!(std::fs::read_to_string(&script).unwrap(), START);
        h.app.d.discard();
    }
}

#[test]
fn a_script_never_saved_survives_a_kill_and_is_offered_at_start_up() {
    let states = expected(false);
    let n = states.len() - 1;
    let kill_at = n * 2 / 3;
    let home = tempfile::tempdir().unwrap();
    let acked = type_and_kill(home.path(), None, kill_at);
    let mut h = headless(home.path(), None);
    assert_eq!(h.app.leftover.len(), 1, "{:?}", h.app.leftover);
    let left = h.app.leftover[0].clone();
    h.run(vec![Message::OpenPath(left)]);
    assert!(h.app.recovery.as_ref().unwrap().never_saved);
    h.run(vec![Message::RecoveryKeep]);
    check(&states, &state(&h.app), acked, 0);
    h.app.d.discard();
}
