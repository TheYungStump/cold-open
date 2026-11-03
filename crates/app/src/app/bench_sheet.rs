//! "Measure performance…" (editor.md §22, docs/specs/bench.md §2): the
//! bench runs as a child process of the app, `--bench --progress`, so the
//! window stays responsive; a reader thread follows its progress lines,
//! and the sheet shows them at each tick.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use iced::widget::{column, row, scrollable, text};
use iced::{Element, Length, Task};

use super::view::{body, dialog_button, hint, title};
use super::{App, Message};
use crate::fonts;
use crate::page_view::KeyPress;
use crate::theme::Palette;

/// What the child has said so far.
#[derive(Clone, Debug, Default)]
struct Heard {
    /// The step it's on, of how many, and what it is.
    step: Option<(usize, usize, String)>,
    /// Everything after the progress lines: the summary and the files.
    tail: Vec<String>,
    /// The child's exit code, once it has ended.
    exit: Option<i32>,
}

/// The bench sheet.
pub struct BenchRun {
    child: Option<Child>,
    heard: Arc<Mutex<Heard>>,
    /// A copy of `heard` as of the last tick, for drawing.
    shown: Heard,
    started: bool,
    failed: Option<String>,
}

impl std::fmt::Debug for BenchRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BenchRun")
    }
}

impl Drop for BenchRun {
    fn drop(&mut self) {
        // Closing the sheet stops a bench that is still running.
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl BenchRun {
    fn done(&self) -> bool {
        self.shown.exit.is_some() || self.failed.is_some()
    }

    /// The report's folder, from the line naming the JSON file.
    fn folder(&self) -> Option<PathBuf> {
        self.shown
            .tail
            .iter()
            .find_map(|l| l.strip_prefix("Report: "))
            .and_then(|p| PathBuf::from(p.trim()).parent().map(PathBuf::from))
    }
}

impl App {
    pub(super) fn bench_open(&mut self) {
        self.bench_run = Some(BenchRun {
            child: None,
            heard: Arc::default(),
            shown: Heard::default(),
            started: false,
            failed: None,
        });
    }

    fn bench_start(&mut self) {
        let command = match &self.bench_command {
            Some((exe, args)) => Ok((exe.clone(), args.clone())),
            None => std::env::current_exe().map(|e| (e, Vec::new())),
        };
        let Some(run) = &mut self.bench_run else {
            return;
        };
        if run.started {
            return;
        }
        run.started = true;
        let (exe, extra) = match command {
            Ok(c) => c,
            Err(e) => {
                run.failed = Some(format!("Cold Open could not find itself: {e}"));
                return;
            }
        };
        let spawned = Command::new(exe)
            .args(["--bench", "--progress"])
            .args(extra)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                run.failed = Some(format!("The bench could not start: {e}"));
                return;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            run.failed = Some("The bench could not start.".into());
            return;
        };
        let heard = Arc::clone(&run.heard);
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(mut h) = heard.lock() else { break };
                match parse_progress(&line) {
                    Some(step) => h.step = Some(step),
                    None => h.tail.push(line),
                }
            }
        });
        run.child = Some(child);
    }

    /// The bench sheet, if open: whether the bench has started, and its
    /// exit code once it has ended.
    pub fn bench_status(&self) -> Option<(bool, Option<i32>)> {
        self.bench_run.as_ref().map(|r| {
            let exit = r.shown.exit.or(r.failed.as_ref().map(|_| 2));
            (r.started, exit)
        })
    }

    /// At each tick: what the child has said, and whether it has ended.
    pub(super) fn bench_tick(&mut self) {
        let Some(run) = &mut self.bench_run else {
            return;
        };
        if let Some(c) = &mut run.child
            && let Ok(Some(status)) = c.try_wait()
        {
            run.child = None;
            // Let the reader thread take the last lines first.
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Ok(mut h) = run.heard.lock() {
                h.exit = Some(status.code().unwrap_or(2));
            }
        }
        if let Ok(h) = run.heard.lock() {
            run.shown = h.clone();
        }
    }

    /// Enter starts the bench, then shows the report; Esc closes the sheet
    /// and stops a bench still running.
    pub(super) fn bench_key(
        &mut self,
        _kp: &KeyPress,
        enter: bool,
        esc: bool,
    ) -> Option<Task<Message>> {
        let run = self.bench_run.as_ref()?;
        if esc {
            self.bench_run = None;
        } else if enter {
            if !run.started {
                return Some(self.update(Message::BenchStart));
            }
            if run.done() {
                return Some(self.update(Message::BenchShow));
            }
        }
        Some(Task::none())
    }

    pub(super) fn bench_message(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::BenchStart => self.bench_start(),
            Message::BenchShow => {
                if let Some(folder) = self.bench_run.as_ref().and_then(BenchRun::folder)
                    && let Err(e) = crate::files::open_link(&folder.to_string_lossy())
                {
                    self.banners
                        .push(format!("Couldn't open the folder: {e}").into());
                }
            }
            Message::BenchClose => self.bench_run = None,
            _ => {}
        }
        Task::none()
    }

    pub(super) fn bench_view<'a>(
        &'a self,
        pal: Palette,
        run: &'a BenchRun,
    ) -> Element<'a, Message> {
        if let Some(e) = &run.failed {
            return column![
                title(pal, "Measure performance"),
                body(pal, e.as_str()),
                dialog_button("Close", Message::BenchClose, true),
            ]
            .spacing(12)
            .into();
        }
        if !run.started {
            return column![
                title(pal, "Measure performance"),
                body(
                    pal,
                    "Cold Open measures how fast it opens, types, saves and scrolls on this \
                     computer, with a script it makes for the purpose. It takes about two \
                     minutes. Windows will open and close by themselves meanwhile; your \
                     script is left alone."
                ),
                body(
                    pal,
                    "It writes two small files to your Downloads folder. They hold this \
                     computer's model names (operating system, processor, graphics) and the \
                     times, and nothing else: no names, paths or script text."
                ),
                row![
                    dialog_button("Start", Message::BenchStart, true),
                    dialog_button("Cancel", Message::BenchClose, false),
                ]
                .spacing(8),
                hint(pal, "Enter: start  ·  Esc: cancel"),
            ]
            .spacing(12)
            .into();
        }
        if !run.done() {
            let what = run.shown.step.as_ref().map_or_else(
                || "Starting…".to_string(),
                |(i, n, what)| format!("Step {i} of {n}: {what}…"),
            );
            return column![
                title(pal, "Measuring performance"),
                body(pal, what),
                hint(pal, "Windows may open and close. Esc: stop"),
            ]
            .spacing(12)
            .into();
        }
        let summary = run
            .shown
            .tail
            .iter()
            .filter(|l| !l.starts_with("Report: ") && !l.starts_with("Summary: "))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let verdict = match run.shown.exit {
            Some(0) => "Every budget passed.",
            Some(1) => "Some budgets were missed. The report says which.",
            _ => "The bench could not finish.",
        };
        let mut buttons = row![].spacing(8);
        if run.folder().is_some() {
            buttons = buttons.push(dialog_button("Show the report", Message::BenchShow, true));
        }
        buttons = buttons.push(dialog_button(
            "Close",
            Message::BenchClose,
            run.folder().is_none(),
        ));
        column![
            title(pal, "Performance measured"),
            body(pal, verdict),
            scrollable(
                text(summary)
                    .font(fonts::PAGE)
                    .size(11)
                    .color(pal.text)
                    .wrapping(text::Wrapping::None)
            )
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::default(),
                horizontal: scrollable::Scrollbar::default(),
            })
            .height(Length::Fixed(320.0)),
            body(
                pal,
                "Please send both files in the folder to the Cold Open team."
            ),
            buttons,
            hint(pal, "Enter: show the report  ·  Esc: close"),
        ]
        .spacing(12)
        .into()
    }
}

/// `progress <step>/<steps> <what>`, as `bench::main` prints it.
fn parse_progress(line: &str) -> Option<(usize, usize, String)> {
    let rest = line.strip_prefix("progress ")?;
    let (count, what) = rest.split_once(' ')?;
    let (i, n) = count.split_once('/')?;
    Some((i.parse().ok()?, n.parse().ok()?, what.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_lines_are_read() {
        assert_eq!(
            parse_progress("progress 3/9 The journal"),
            Some((3, 9, "The journal".into()))
        );
        assert_eq!(parse_progress("Open a 120-page script  median 40 ms"), None);
    }
}
