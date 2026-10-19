//! `coldopen-driver`: the desktop app, headless, driven one key per line
//! on standard input, for the crash gate (docs/specs/editor.md §14). Never
//! shipped.
//!
//! ```text
//! coldopen-driver --home DIR [SCRIPT]
//! ```
//!
//! SCRIPT may be a .fountain, an .fdx or a .coldopen file.
//!
//! Input lines:
//! - `char X`: types the character X (the rest of the line, one character);
//! - `key NAME`: presses enter, tab, backspace, escape, up, down, left,
//!   right, home or end;
//! - `mod X`: presses Mod+X (Ctrl, or Cmd on macOS);
//! - `modshift X`: presses Mod+Shift+X;
//! - `action NAME`: runs an app action by name, as the palette would
//!   (docs/specs/editor.md §10), such as `changes.accept_here`;
//! - `saveas PATH`: Save as PATH, as if the save dialog had answered with
//!   it; the extension picks the format, so this converts;
//! - `quit`: closes the script cleanly and exits.
//!
//! After each line has gone through the app's update, and so has been
//! journaled, it prints `ack N` and flushes. It also runs whatever the
//! agent link delivers between lines.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use coldopen_app::files::{Ask, Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot, Message};
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
            "mod" | "modshift" => {
                let Some(c) = rest.chars().next() else {
                    continue;
                };
                h.shortcut(c, cmd == "modshift");
            }
            "action" if !rest.is_empty() => h.run(vec![Message::Action(rest.into())]),
            "saveas" if !rest.is_empty() => {
                let to = PathBuf::from(rest);
                h.run(vec![Message::Chosen(Ask::SaveAs(String::new()), Some(to))]);
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
