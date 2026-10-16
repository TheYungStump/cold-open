//! `coldopen-driver`: the desktop app, headless, driven one key per line
//! on standard input, for the crash gate (docs/specs/editor.md §14). Never
//! shipped.
//!
//! ```text
//! coldopen-driver --home DIR [SCRIPT]
//! ```
//!
//! Input lines:
//! - `char X`: types the character X (the rest of the line, one character);
//! - `key NAME`: presses enter, tab, backspace, escape, up, down, left,
//!   right, home or end;
//! - `mod X`: presses Mod+X (Ctrl, or Cmd on macOS);
//! - `quit`: closes the script cleanly and exits.
//!
//! After each line has gone through the app's update, and so has been
//! journaled, it prints `ack N` and flushes. It also runs whatever the
//! agent link delivers between lines.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot};
use iced::keyboard::Modifiers;
use iced::keyboard::key::Named;

fn named(name: &str) -> Option<Named> {
    Some(match name {
        "enter" => Named::Enter,
        "tab" => Named::Tab,
        "backspace" => Named::Backspace,
        "delete" => Named::Delete,
        "escape" => Named::Escape,
        "up" => Named::ArrowUp,
        "down" => Named::ArrowDown,
        "left" => Named::ArrowLeft,
        "right" => Named::ArrowRight,
        "home" => Named::Home,
        "end" => Named::End,
        _ => return None,
    })
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut home: Option<PathBuf> = None;
    let mut file: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--home" => home = args.next().map(PathBuf::from),
            _ => file = Some(PathBuf::from(a)),
        }
    }
    let Some(home) = home else {
        eprintln!("usage: coldopen-driver --home DIR [SCRIPT]");
        return ExitCode::FAILURE;
    };
    let mut app = App::new(Boot {
        file,
        dirs: Some(Dirs::under(&home)),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    let Some(mut h) = Headless::new(app, iced::Size::new(1280.0, 860.0)) else {
        eprintln!("coldopen-driver: no headless renderer");
        return ExitCode::FAILURE;
    };
    let out = std::io::stdout();
    let mut out = out.lock();
    let _ = writeln!(out, "ready");
    let _ = out.flush();
    let mut n = 0u64;
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        h.pump();
        let (cmd, rest) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        match cmd {
            "char" => {
                let Some(c) = rest.chars().next() else {
                    continue;
                };
                h.type_text(&c.to_string());
            }
            "key" => match named(rest) {
                Some(k) => h.named(k, Modifiers::empty()),
                None => continue,
            },
            "mod" => {
                let Some(c) = rest.chars().next() else {
                    continue;
                };
                h.shortcut(c, false);
            }
            "quit" => {
                h.app.d.shutdown();
                break;
            }
            _ => continue,
        }
        n += 1;
        if writeln!(out, "ack {n}").and_then(|()| out.flush()).is_err() {
            break;
        }
    }
    ExitCode::SUCCESS
}
