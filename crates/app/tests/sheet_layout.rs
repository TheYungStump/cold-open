//! Every sheet and question fits its card: no text runs past the card's
//! edge (a row of buttons wider than the card did, in Report a problem),
//! and no sentence has a run of spaces in the middle (a string broken
//! across lines without `\` did, in Crash on purpose), both found in the
//! 2026-11-03 manual test.

use std::path::{Path, PathBuf};

use coldopen_app::app::{After, CARD, NameThen, Prompt, Sheet, iced_settings};
use coldopen_app::files::Dirs;
use coldopen_app::{App, Boot, Message};
use iced::{Rectangle, Size};
use iced_test::selector::Candidate;

/// The card's padding, in view.rs.
const PADDING: f32 = 18.0;

/// What is wrong with the card `app` shows, if anything.
fn fault(app: &App) -> Option<String> {
    let mut sim =
        iced_test::Simulator::with_size(iced_settings(), Size::new(1280.0, 860.0), app.view());
    let card = match sim.find(iced::widget::Id::from(CARD)) {
        Ok(t) => t.bounds(),
        Err(_) => return Some("no card is shown".into()),
    };
    // Within the card's padding, side to side. A row too wide for the card
    // squeezes its last button into the padding, then past the border.
    // Text below the card's bottom is inside a scrollable, so height is
    // left alone.
    let inside = |b: Rectangle| {
        b.x >= card.x + PADDING - 0.5 && b.x + b.width <= card.x + card.width - PADDING + 0.5
    };
    sim.find(move |c: Candidate<'_>| match c {
        Candidate::Text {
            bounds, content, ..
        } if overlaps(bounds, card) => {
            // A word can't wrap, so text is at least as wide as its
            // longest word, at 4 px a character or more in every font
            // the app uses. A button squeezed by a row too wide for the
            // card is narrower, and its label is drawn past it.
            let word = content
                .split_whitespace()
                .map(|w| w.chars().count())
                .max()
                .unwrap_or(0);
            if !inside(bounds) {
                Some(format!(
                    "{content:?} runs past the card: {bounds:?} in {card:?}"
                ))
            } else if bounds.width + 0.5 < 4.0 * word as f32 {
                Some(format!("{content:?} is squeezed to {} px", bounds.width))
            } else if content.lines().any(|l| l.trim_start().contains("   ")) {
                Some(format!("{content:?} has a run of spaces"))
            } else {
                None
            }
        }
        _ => None,
    })
    .ok()
}

/// Whether `b` starts within `card`: text behind the card, beside it, is
/// left out.
fn overlaps(b: Rectangle, card: Rectangle) -> bool {
    b.x < card.x + card.width
        && b.x + b.width > card.x
        && b.y >= card.y
        && b.y < card.y + card.height
}

fn script(home: &Path) -> PathBuf {
    let p = home.join("The Lighthouse Keeper's Daughter, revised.fountain");
    std::fs::write(
        &p,
        "INT. LIGHTHOUSE - NIGHT\n\nRain on the glass.\n\nMARA\nIs anyone up there?\n",
    )
    .unwrap();
    p
}

fn app(home: &Path, file: Option<PathBuf>) -> App {
    let mut a = App::new(Boot {
        file,
        dirs: Some(Dirs::under(home)),
        // So the usage summary's ticks have a summary to show.
        count_usage: true,
        ..Boot::default()
    });
    a.banners.clear();
    a
}

fn write_reports(home: &Path) {
    let folder = coldopen_app::crash::folder(&Dirs::under(home));
    std::fs::create_dir_all(&folder).unwrap();
    for stamp in ["20261103-123456", "20261102-101010"] {
        let r = coldopen_core::crash::Report {
            version: "0.1.0-alpha.2 (5d6e764a1b2c)".into(),
            target: "x86_64-pc-windows-msvc".into(),
            os: "Windows 11 Home (22621)".into(),
            renderer: "GPU: NVIDIA GeForce RTX 4090 (Vulkan)".into(),
            time: "2026-11-03T12:34:56Z".into(),
            thread: "main".into(),
            last_action: "text.insert".into(),
            location: "crates/app/src/editor/mod.rs:812:22".into(),
            message: "byte index 3 is not a char boundary; it is inside [quoted] (bytes 2..4)"
                .into(),
            frames: vec!["Cold Open.exe+0x1a2b3c".into(); 20],
        };
        std::fs::write(folder.join(format!("crash-{stamp}Z.seen.txt")), r.render()).unwrap();
    }
}

#[test]
fn every_question_fits_its_card() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let file = script(home);
    let long = home
        .join("Writing")
        .join("Second act rewrites after the table read")
        .join("The Lighthouse Keeper's Daughter, revised for the spring fellowship.fountain");
    let prompts = vec![
        Prompt::Unsaved(After::Close),
        Prompt::Lossy {
            message: "Fountain can't hold 2 revision sets, 14 revision marks and the title page's \
                      layout."
                .into(),
            path: None,
            overwrite: false,
            export: false,
            tracked: 3,
            revisions: true,
        },
        Prompt::Changed {
            message: "Another program changed this script since Cold Open last read or saved it."
                .into(),
            allow_lossy: false,
        },
        Prompt::Foreign {
            path: long.clone(),
            host: "WRITING-LAPTOP-2".into(),
            at: "2026-11-03 12:34".into(),
        },
        Prompt::DeleteCopy(long.clone()),
        Prompt::DeleteCrashReport(home.join("crash-20261103-123456Z.txt")),
        Prompt::CrashTest,
        Prompt::AuthorName {
            name: "Dana".into(),
            then: NameThen::TrackMine,
        },
        Prompt::SettleAll {
            accept: true,
            count: 128,
        },
        Prompt::SettleAll {
            accept: false,
            count: 128,
        },
        Prompt::RevisionEndFirst {
            active: "Goldenrod".into(),
        },
        Prompt::RevisionColors(
            "White, Blue, Pink, Yellow, Green, Goldenrod, Buff, Salmon, Cherry".into(),
        ),
        Prompt::RevisionClear { choice: 0 },
        Prompt::ImportName {
            path: long.clone(),
            name: "Dana".into(),
        },
        Prompt::MissingRecent(long),
    ];
    let mut a = app(home, Some(file));
    let mut faults = Vec::new();
    for p in prompts {
        let name = format!("{p:?}");
        a.prompt = Some(p);
        if let Some(f) = fault(&a) {
            faults.push(format!("{name}: {f}"));
        }
    }
    a.prompt = None;
    for action in ["app.revision_start", "app.export_pdf"] {
        let _ = a.update(Message::Action(action.into()));
        if let Some(f) = fault(&a) {
            faults.push(format!("{action}: {f}"));
        }
        let _ = a.update(Message::Escaped);
        a.prompt = None;
    }
    a.d.discard();
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn every_sheet_fits_its_card() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    write_reports(home);
    let file = script(home);
    let mut a = app(home, Some(file));
    let mut faults = Vec::new();
    for action in [
        "app.history",
        "app.about",
        "app.report_problem",
        "app.crash_reports",
        "app.recovered",
        // 2026-11-05 (editor.md §23, §24).
        "app.settings",
        "app.share_feedback",
        "app.usage_summary",
    ] {
        let _ = a.update(Message::Action(action.into()));
        if let Some(f) = fault(&a) {
            faults.push(format!("{action}: {f}"));
        }
        a.sheet = None;
        let _ = a.update(Message::Escaped);
        let _ = a.update(Message::RecoveredClose);
    }
    let _ = a.update(Message::Action("app.about".into()));
    let _ = a.update(Message::AboutTab(coldopen_app::about::AboutTab::ThirdParty));
    if let Some(f) = fault(&a) {
        faults.push(format!("About, third party: {f}"));
    }
    assert_eq!(a.sheet, Some(Sheet::About));
    // The ticks, ticked: the summary in full under them.
    a.sheet = None;
    let _ = a.update(Message::Action("app.share_feedback".into()));
    let _ = a.update(Message::Share(coldopen_app::app::ShareInput::Include));
    if let Some(f) = fault(&a) {
        faults.push(format!("Share feedback, ticked: {f}"));
    }
    a.sheet = None;
    let _ = a.update(Message::Action("app.report_problem".into()));
    let _ = a.update(Message::Share(
        coldopen_app::app::ShareInput::ProblemInclude,
    ));
    if let Some(f) = fault(&a) {
        faults.push(format!("Report a problem, ticked: {f}"));
    }
    a.d.discard();
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}
