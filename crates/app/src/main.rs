//! `coldopen-app [script]`: the Cold Open desktop app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use coldopen_app::app::{Bench, BenchKind};
use coldopen_app::{App, Boot};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The bench (docs/specs/bench.md): no window of its own; the app it
    // measures runs as its children.
    if args.first().is_some_and(|a| a == "--bench") {
        return coldopen_app::bench::main(&args[1..]);
    }
    match app() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Cold Open could not start: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn app() -> iced::Result {
    let launched = std::time::Instant::now();
    choose_renderer();
    let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let boot = Boot {
        file,
        // CI's cold-start measurement and windowed smoke test.
        exit_after_first_frame: std::env::var_os("COLDOPEN_EXIT_AFTER_FIRST_FRAME")
            .is_some_and(|v| v == "1")
            .then_some(launched),
        bench: bench(),
        ..Boot::default()
    };
    let mut window = iced::window::Settings {
        size: iced::Size::new(1280.0, 860.0),
        // The close button asks about unsaved changes first.
        exit_on_close_request: false,
        ..iced::window::Settings::default()
    };
    // The self-driving benchmark's window (docs/progress/phase-3.md).
    if let Some((w, h)) = std::env::var("COLDOPEN_BENCH_SIZE")
        .ok()
        .and_then(|s| {
            s.split_once('x')
                .map(|(w, h)| (w.parse().ok(), h.parse().ok()))
        })
        .and_then(|(w, h)| Some((w?, h?)))
    {
        window.size = iced::Size::new(w, h);
    }
    window.fullscreen = std::env::var_os("COLDOPEN_BENCH_FULLSCREEN").is_some_and(|v| v == "1");
    iced::application(move || App::boot(boot.clone()), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .settings(coldopen_app::app::iced_settings())
        .window(window)
        .run()
}

/// `COLDOPEN_BENCH=typing|scrolling|reports|memory` runs the self-driving benchmark,
/// `COLDOPEN_BENCH_FRAMES` frames long (120 by default).
fn bench() -> Option<Bench> {
    let kind = match std::env::var("COLDOPEN_BENCH").ok()?.as_str() {
        "typing" => BenchKind::Typing,
        "scrolling" => BenchKind::Scrolling,
        "reports" => BenchKind::Reports,
        "memory" => BenchKind::Memory,
        _ => return None,
    };
    let frames = std::env::var("COLDOPEN_BENCH_FRAMES")
        .ok()
        .and_then(|f| f.parse().ok())
        .unwrap_or(120);
    let label = std::env::var("COLDOPEN_BENCH_LABEL").unwrap_or_default();
    Some(Bench {
        kind,
        frames,
        label,
    })
}

/// Tells iced which renderer to use, from `settings.json`, unless
/// `ICED_BACKEND` is set already (the benchmarks set it).
fn choose_renderer() {
    if std::env::var_os("ICED_BACKEND").is_some() {
        return;
    }
    let renderer = coldopen_app::files::Dirs::find()
        .map(|d| coldopen_app::files::Settings::load(&d).0.renderer)
        .unwrap_or_default();
    // SAFETY: this runs first in `main`, before any thread exists, so
    // nothing can read the environment while it changes. iced reads this
    // variable only, so there is no safe way to pass the choice.
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("ICED_BACKEND", renderer.backends());
    }
}
