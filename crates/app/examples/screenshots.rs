//! Writes screenshots of the app in each theme, headless, with iced_test:
//! `cargo run -p coldopen-app --example screenshots -- <out dir>`.
//! The script is the synthetic one from the layout crate (our own words).

use coldopen_app::app::{Reply, iced_settings};
use coldopen_app::theme::Choice;
use coldopen_app::{App, Boot, Message};
use iced::Size;

/// Writes one screenshot of `app` as `cold-open-<name>.png`.
fn shot(app: &App, out: &std::path::Path, name: &str) {
    let theme = app.theme();
    let mut sim =
        iced_test::Simulator::with_size(iced_settings(), Size::new(1280.0, 860.0), app.view());
    let snap = sim.snapshot(&theme).unwrap();
    let path = out.join(format!("cold-open-{name}.png"));
    // iced_test adds the renderer's name to the file name, and only
    // writes the file when it does not exist yet.
    let _ = std::fs::remove_file(out.join(format!("cold-open-{name}-tiny-skia.png")));
    snap.matches_image(&path).unwrap();
}

fn main() {
    let out = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/screenshots".into()),
    );
    std::fs::create_dir_all(&out).unwrap();
    let work = tempfile::tempdir().unwrap();
    let text = coldopen_fountain::write(&coldopen_layout::synthetic::feature_script(1)).text;
    for (name, theme) in [
        ("light", Choice::Light),
        ("dark", Choice::Dark),
        ("high-contrast", Choice::HighContrast),
    ] {
        // A fresh copy each time, so no run recovers another's journal.
        let home = work.path().join(name);
        std::fs::create_dir_all(&home).unwrap();
        let script = home.join("low-tide.fountain");
        std::fs::write(&script, &text).unwrap();
        let mut app = App::new(Boot {
            file: Some(script.clone()),
            dirs: Some(coldopen_app::files::Dirs::under(&home)),
            theme: Some(theme),
            every_entry: true,
            ..Boot::default()
        });
        app.demo_autocomplete();
        shot(&app, &out, name);
        app.d.discard();

        // Phase 4's screens (editor.md §13): a set-aside notice on the start
        // screen, Recovered versions, and the repair report.
        let mut app = App::new(Boot {
            file: Some(script.clone()),
            dirs: Some(coldopen_app::files::Dirs::under(&home)),
            theme: Some(theme),
            every_entry: true,
            ..Boot::default()
        });
        app.banners.clear();
        app.editor.insert_text(&mut app.d, "x");
        let _ = app.update(Message::Action("app.close".into()));
        let _ = app.update(Message::Reply(Reply::DontSave));
        shot(&app, &out, &format!("{name}-set-aside"));
        let _ = app.update(Message::Action("app.recovered".into()));
        shot(&app, &out, &format!("{name}-recovered"));
        let _ = app.update(Message::RecoveredClose);

        let broken = home.join("Ironworks.fdx");
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/fdx/broken-markup.fdx"),
            &broken,
        )
        .unwrap();
        let _ = app.update(Message::OpenPath(broken));
        shot(&app, &out, &format!("{name}-repaired"));
        let _ = app.update(Message::RepairDetails);
        shot(&app, &out, &format!("{name}-repaired-details"));
        app.d.discard();
        println!("wrote {}", out.display());
    }
}
