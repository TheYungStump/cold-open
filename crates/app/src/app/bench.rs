//! A self-driving benchmark for the windowed app, for the renderer
//! comparison and CI (docs/progress/phase-3.md, measurements): each frame
//! types one key or scrolls one step, the time from one frame to the next
//! is recorded, and the app prints the numbers and exits. It measures what
//! the writer sees, including the renderer's own damage tracking and
//! presenting, which headless frames leave out.

use std::time::Instant;

use iced::Task;
use iced::keyboard::{Key, Modifiers};
use iced::widget::scrollable;
use iced_runtime::widget::operation as widget_op;

use super::{App, Message, PAGES};
use crate::page_view::KeyPress;

/// What the benchmark does each frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchKind {
    /// Types into a speech on page 90.
    Typing,
    /// Scrolls the pages with the wheel's step.
    Scrolling,
    /// Scrolls the report view's scenes with the wheel's step (editor.md
    /// §20; the manual test of 2026-10-26 found it slow).
    Reports,
}

/// A benchmark run, from the environment (see `main.rs`).
#[derive(Clone, Debug)]
pub struct Bench {
    pub kind: BenchKind,
    /// Frames measured, after a short warm-up.
    pub frames: usize,
    /// A label for the report, such as the window size and renderer.
    pub label: String,
}

#[derive(Debug)]
pub(super) struct BenchState {
    bench: Bench,
    times: Vec<Instant>,
}

impl BenchState {
    pub(super) fn new(bench: Bench) -> BenchState {
        BenchState {
            bench,
            times: Vec::new(),
        }
    }
}

const WARM_UP: usize = 5;
const TEXT: &str = "and the sea goes on ";

impl App {
    /// Puts the caret at the end of a speech on page `target`, or the last
    /// page before it that has one. Returns that page, from 0.
    pub fn place_caret_on_page(&mut self, target: u32) -> Option<u32> {
        let (block, page) = {
            let view = self.d.view()?;
            let layout = view.layout();
            let mut best = None;
            let mut first = None;
            for (e, el) in view.norm.elements.iter().enumerate() {
                if el.kind != coldopen_screenplay::ElementKind::Dialogue {
                    continue;
                }
                let page = layout.first_page(e);
                if first.is_none() {
                    first = page.map(|p| (view.map[e], p));
                }
                match page {
                    Some(p) if p <= target => best = Some((view.map[e], p)),
                    Some(_) => break,
                    None => {}
                }
            }
            // A short script may have no speech that early: use its first.
            best.or(first)?
        };
        let b = self.d.session()?.doc.blocks.get(block)?;
        let c = crate::editor::Caret::at(b.id, b.element.text.char_len());
        self.editor.caret = c;
        self.editor.anchor = c;
        Some(page)
    }

    pub(super) fn bench_frame(&mut self) -> Task<Message> {
        let Some(state) = &mut self.bench else {
            return Task::none();
        };
        state.times.push(Instant::now());
        let n = state.times.len();
        let kind = state.bench.kind;
        if n == 1 {
            if kind == BenchKind::Typing {
                let pages = self.d.cached_view().map_or(0, |v| v.layout().pages.len()) as u32;
                self.place_caret_on_page(90.min(pages * 3 / 4));
            }
            if kind == BenchKind::Reports {
                self.reports_open();
            }
            return self.after_change(true);
        }
        if n > WARM_UP + state.bench.frames {
            self.bench_report();
            self.d.discard();
            return iced::exit();
        }
        match kind {
            BenchKind::Typing => {
                let c = TEXT.chars().nth(n % TEXT.len()).unwrap_or(' ').to_string();
                self.key(KeyPress {
                    key: Key::Character(c.as_str().into()),
                    modifiers: Modifiers::empty(),
                    text: Some(c),
                })
            }
            BenchKind::Reports => {
                let rows = self
                    .reports
                    .as_ref()
                    .and_then(|r| r.table())
                    .map_or(0, |t| t.rows.len());
                let (offset, height) = self
                    .reports
                    .as_ref()
                    .map_or((0, 0), |r| (r.offset, r.height));
                let end = (rows as u32 * 22).saturating_sub(height);
                if offset + 44 >= end {
                    widget_op::scroll_to(
                        super::reports::REPORT_ROWS,
                        scrollable::AbsoluteOffset {
                            x: None,
                            y: Some(0.0),
                        },
                    )
                } else {
                    widget_op::scroll_by(
                        super::reports::REPORT_ROWS,
                        scrollable::AbsoluteOffset { x: 0.0, y: 44.0 },
                    )
                }
            }
            BenchKind::Scrolling => {
                let at_end = self.viewport.as_ref().is_some_and(|v| {
                    v.absolute_offset().y + v.bounds().height >= v.content_bounds().height - 2.0
                });
                if at_end {
                    widget_op::scroll_to(
                        PAGES,
                        scrollable::AbsoluteOffset {
                            x: None,
                            y: Some(0.0),
                        },
                    )
                } else {
                    widget_op::scroll_by(PAGES, scrollable::AbsoluteOffset { x: 0.0, y: 80.0 })
                }
            }
        }
    }

    fn bench_report(&self) {
        let Some(state) = &self.bench else { return };
        let t = &state.times[WARM_UP..];
        let mut gaps: Vec<f64> = t
            .windows(2)
            .map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0)
            .collect();
        if gaps.is_empty() {
            return;
        }
        gaps.sort_by(f64::total_cmp);
        let at = |q: f64| gaps[((gaps.len() - 1) as f64 * q).round() as usize];
        let kind = match state.bench.kind {
            BenchKind::Typing => "typing",
            BenchKind::Scrolling => "scrolling",
            BenchKind::Reports => "reports",
        };
        println!(
            "{{\"bench\": \"{kind}\", \"label\": \"{}\", \"frames\": {}, \"median_ms\": {:.2}, \"p95_ms\": {:.2}, \"max_ms\": {:.2}}}",
            state.bench.label,
            gaps.len(),
            at(0.5),
            at(0.95),
            gaps[gaps.len() - 1]
        );
    }
}
