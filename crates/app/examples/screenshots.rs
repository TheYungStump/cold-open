//! Writes screenshots of the app in each theme, headless, with iced_test:
//! `cargo run -p coldopen-app --example screenshots -- <out dir>`.
//! The script is the synthetic one from the layout crate (our own words).

use coldopen_app::app::iced_settings;
use coldopen_app::theme::Choice;
use coldopen_app::{App, Boot};
use iced::Size;

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
        let theme = app.theme();
        let mut sim =
            iced_test::Simulator::with_size(iced_settings(), Size::new(1280.0, 860.0), app.view());
        let snap = sim.snapshot(&theme).unwrap();
        let path = out.join(format!("cold-open-{name}.png"));
        // iced_test adds the renderer's name to the file name, and only
        // writes the file when it does not exist yet.
        let _ = std::fs::remove_file(out.join(format!("cold-open-{name}-tiny-skia.png")));
        snap.matches_image(&path).unwrap();
        drop(sim);
        app.d.discard();
        println!("wrote {}", out.display());
    }
}
