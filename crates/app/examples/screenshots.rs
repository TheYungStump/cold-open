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
    // writes the file when it does not exist yet: the GPU's renderer, or
    // tiny-skia without one.
    for renderer in ["tiny-skia", "wgpu"] {
        let _ = std::fs::remove_file(out.join(format!("cold-open-{name}-{renderer}.png")));
    }
    snap.matches_image(&path).unwrap();
}

/// Writes one screenshot with the mouse resting on the text `on`, so its
/// tooltip shows.
fn shot_hovering(app: &App, out: &std::path::Path, name: &str, on: &str) {
    let theme = app.theme();
    let mut sim =
        iced_test::Simulator::with_size(iced_settings(), Size::new(1280.0, 860.0), app.view());
    let at = sim.find(on).unwrap().bounds().center();
    sim.point_at(at);
    let _ = sim.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position: at,
    })]);
    let snap = sim.snapshot(&theme).unwrap();
    let path = out.join(format!("cold-open-{name}.png"));
    for renderer in ["tiny-skia", "wgpu"] {
        let _ = std::fs::remove_file(out.join(format!("cold-open-{name}-{renderer}.png")));
    }
    snap.matches_image(&path).unwrap();
}

/// Lets the page area report its size, as the window's first frame does,
/// so the page is drawn at the zoom that fits it (editor.md §3).
fn settle(app: &mut App) {
    let sent: Vec<Message> = {
        let mut sim =
            iced_test::Simulator::with_size(iced_settings(), Size::new(1280.0, 860.0), app.view());
        let _ = sim.simulate([iced::Event::Window(iced::window::Event::RedrawRequested(
            std::time::Instant::now(),
        ))]);
        sim.into_messages()
            .filter(|m| matches!(m, Message::Scrolled(_)))
            .collect()
    };
    for m in sent {
        let _ = app.update(m);
    }
}

/// Tracked edits on the first page: the writer's insertion and deletion in
/// the first action, and an agent's rewrite of the next one.
fn tracked_demo(app: &mut App) {
    use coldopen_core::{Author, Command};
    let doc = &app.d.session().unwrap().doc;
    let actions: Vec<(coldopen_core::BlockId, usize)> = doc
        .blocks
        .iter()
        .filter(|b| b.element.kind == coldopen_screenplay::ElementKind::Action)
        .take(2)
        .map(|b| (b.id, b.element.text.char_len()))
        .collect();
    let run = |app: &mut App, name: &str, params: serde_json::Value, by: &Author| {
        let cmd = Command::from_parts(name, params).unwrap();
        app.d.execute_as(cmd, by).unwrap();
    };
    let writer = app.d.writer().clone();
    let (first, len) = &actions[0];
    run(
        app,
        "text.insert",
        serde_json::json!({ "id": first, "at": *len, "text": " The kettle clicks off." }),
        &writer,
    );
    run(
        app,
        "text.delete",
        serde_json::json!({ "id": first, "from": 0, "to": 4 }),
        &writer,
    );
    let (second, len) = &actions[1];
    let agent = Author::agent("Claude", Some("text_insert".into()));
    run(
        app,
        "text.insert",
        serde_json::json!({ "id": second, "at": *len, "text": " Somewhere a gull answers." }),
        &agent,
    );
}

/// A Blue revision set on the first page: words added to the first
/// action, the start of the second taken out, and a speech retyped.
fn revisions_demo(app: &mut App) {
    use coldopen_core::Command;
    let run = |app: &mut App, name: &str, params: serde_json::Value| {
        let cmd = Command::from_parts(name, params).unwrap();
        app.d.execute(cmd).unwrap();
    };
    run(
        app,
        "revision.start",
        serde_json::json!({ "date": "2026-10-22" }),
    );
    let doc = &app.d.session().unwrap().doc;
    let actions: Vec<(coldopen_core::BlockId, usize)> = doc
        .blocks
        .iter()
        .filter(|b| b.element.kind == coldopen_screenplay::ElementKind::Action)
        .take(2)
        .map(|b| (b.id, b.element.text.char_len()))
        .collect();
    let speech = doc
        .blocks
        .iter()
        .find(|b| b.element.kind == coldopen_screenplay::ElementKind::Dialogue)
        .map(|b| b.id);
    let (first, len) = &actions[0];
    run(
        app,
        "text.insert",
        serde_json::json!({ "id": first, "at": *len, "text": " The kettle clicks off." }),
    );
    let (second, _) = &actions[1];
    run(
        app,
        "text.delete",
        serde_json::json!({ "id": second, "from": 0, "to": 4 }),
    );
    if let Some(id) = speech {
        run(
            app,
            "text.insert",
            serde_json::json!({ "id": id, "at": 0, "text": "Listen. " }),
        );
    }
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

        // The palette with recent scripts deep in long folders
        // (editor.md §9): each row on one line, the folder shortened in the
        // middle, and the whole path on hover.
        let mut app = App::new(Boot {
            file: Some(script.clone()),
            dirs: Some(coldopen_app::files::Dirs::under(&home)),
            theme: Some(theme),
            ..Boot::default()
        });
        app.banners.clear();
        let deep = home
            .join("Writing")
            .join("Feature drafts for the spring fellowship")
            .join("Second act rewrites after the table read");
        app.settings.recent = vec![
            deep.join("The Lighthouse Keeper's Daughter, revised.fountain"),
            deep.join("low-tide.fdx"),
        ];
        let _ = app.update(Message::Action("app.palette".into()));
        let _ = app.update(Message::PaletteInput("open".into()));
        shot(&app, &out, &format!("{name}-palette"));
        shot_hovering(
            &app,
            &out,
            &format!("{name}-palette-hover"),
            "Open recent: low-tide.fdx",
        );
        app.d.discard();

        // Notices (editor.md §13): the newest with "+2 more", then the
        // full list.
        let mut app = App::new(Boot {
            file: Some(script.clone()),
            dirs: Some(coldopen_app::files::Dirs::under(&home)),
            theme: Some(theme),
            ..Boot::default()
        });
        app.banners = vec![
            "Exported 119 pages to low-tide.pdf.".to_string().into(),
            coldopen_app::app::Banner::set_aside(),
            "The keymap file has an unknown action, \"view.zen\"; it was skipped."
                .to_string()
                .into(),
        ];
        shot(&app, &out, &format!("{name}-notices"));
        let _ = app.update(Message::BannersList(true));
        shot(&app, &out, &format!("{name}-notices-list"));
        app.d.discard();

        // Phase 5a (editor.md §17): the writer's tracked typing and a
        // deletion, an agent's tracked edit, and the changes panel.
        let mut app = App::new(Boot {
            file: Some(script.clone()),
            dirs: Some(coldopen_app::files::Dirs::under(&home)),
            theme: Some(theme),
            every_entry: true,
            ..Boot::default()
        });
        app.banners.clear();
        app.settings.author_name = Some("Dana".into());
        app.d.set_writer_name(Some("Dana".into()));
        let _ = app.update(Message::Action("changes.track_mine".into()));
        tracked_demo(&mut app);
        let _ = app.update(Message::Action("view.changes".into()));
        app.banners.clear();
        settle(&mut app);
        shot(&app, &out, &format!("{name}-changes"));
        app.d.discard();

        // Phase 5b (editor.md §18): a Blue set with an insertion, a
        // deletion and a retype on the first page, and the start form.
        let mut app = App::new(Boot {
            file: Some(script.clone()),
            dirs: Some(coldopen_app::files::Dirs::under(&home)),
            theme: Some(theme),
            every_entry: true,
            ..Boot::default()
        });
        app.banners.clear();
        revisions_demo(&mut app);
        // The edits went straight to the dispatcher.
        let _ = app.refresh();
        let _ = app.update(Message::Tick);
        settle(&mut app);
        shot(&app, &out, &format!("{name}-revisions"));
        let _ = app.update(Message::Action("app.revision_end".into()));
        app.banners.clear();
        let _ = app.update(Message::Action("app.revision_start".into()));
        shot(&app, &out, &format!("{name}-revision-form"));
        let _ = app.update(Message::Escaped);
        // The report view (editor.md §20), on the characters.
        let _ = app.update(Message::Action("app.reports".into()));
        let _ = app.update(Message::ReportTab(1));
        shot(&app, &out, &format!("{name}-reports"));
        app.reports = None;
        // The title page sheet (editor.md §19), keys and then lines.
        let _ = app.update(Message::Action("app.title_page".into()));
        shot(&app, &out, &format!("{name}-title-keys"));
        app.title_sheet = None;
        app.d.discard();
        let fdx = home.join("lighthouse.fdx");
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/fdx/title-page.fdx"),
            &fdx,
        )
        .unwrap();
        let mut app = App::new(Boot {
            file: Some(fdx),
            dirs: Some(coldopen_app::files::Dirs::under(&home)),
            theme: Some(theme),
            ..Boot::default()
        });
        settle(&mut app);
        let _ = app.update(Message::Action("app.title_page".into()));
        shot(&app, &out, &format!("{name}-title-lines"));
        app.d.discard();

        // Compare with… (editor.md §21), on the diff crate's two drafts.
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../diff/tests/fixtures");
        let (a, b) = (home.join("draft-a.fountain"), home.join("draft-b.fountain"));
        std::fs::copy(fixtures.join("a.fountain"), &a).unwrap();
        std::fs::copy(fixtures.join("b.fountain"), &b).unwrap();
        let mut app = App::new(Boot {
            file: Some(a),
            dirs: Some(coldopen_app::files::Dirs::under(&home)),
            theme: Some(theme),
            ..Boot::default()
        });
        settle(&mut app);
        let _ = app.update(Message::Chosen(coldopen_app::files::Ask::Compare, Some(b)));
        let _ = app.update(Message::Compare(coldopen_app::app::CompareInput::Row(3)));
        shot(&app, &out, &format!("{name}-compare"));
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
        // A recent file on a drive that isn't connected (editor.md §11).
        let unplugged = std::path::PathBuf::from("Z:/Writing/low-tide (old draft).fountain");
        app.settings.recent.push(unplugged.clone());
        let _ = app.update(Message::RecentChecked {
            checked: vec![unplugged.clone()],
            missing: vec![unplugged],
        });
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
