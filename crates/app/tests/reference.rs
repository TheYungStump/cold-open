//! The keyboard reference on the website (docs/specs/site.md §4): every
//! action the palette lists, with its title and default keys on Windows and
//! Linux and on macOS, rendered from the key map and the action list and
//! checked against the page. `COLDOPEN_BLESS=1 cargo test -p coldopen-app
//! --test reference` rewrites the page; CI refuses to.

use coldopen_app::actions::{APP_ACTIONS, command_title};
use coldopen_app::keymap::{Chord, defaults_for};
use coldopen_core::Command;

const PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/guide/src/reference/keyboard.md"
);

const INTRO: &str = r#"# Keyboard reference

<!-- Made from the key map and the palette's list by crates/app/tests/reference.rs. Don't edit this page: change the code, then run `COLDOPEN_BLESS=1 cargo test -p coldopen-app --test reference`. -->

Every action is in the command palette (Ctrl+K, or Cmd+K on a Mac), with its keys beside it. This page lists them all, with the keys they have when Cold Open is new. You can change any of them in `keymap.json` (see [Your own keys](../guide/getting-started.md#your-own-keys)).

The keys that move the caret and edit text (the arrows, Home, End, Page Up and Down, Backspace, Delete, Enter and Tab) are built in: see [Getting started](../guide/getting-started.md).

"#;

/// The default keys of `action`, as one OS writes them.
fn keys(action: &str, mac: bool) -> String {
    let k: Vec<String> = defaults_for(mac)
        .into_iter()
        .filter(|(_, a)| *a == action)
        .filter_map(|(k, _)| Chord::parse_for(k, mac))
        .map(|c| format!("`{}`", c.label_for(mac)))
        .collect();
    if k.is_empty() {
        "—".into()
    } else {
        k.join(", ")
    }
}

fn row(name: &str, title: &str) -> String {
    let title = if name == "app.install_cli" {
        format!("{title} (macOS only)")
    } else {
        title.to_string()
    };
    format!(
        "| {} | `{name}` | {} | {} |\n",
        title.replace('|', "\\|"),
        keys(name, false),
        keys(name, true)
    )
}

fn render() -> String {
    let mut out = String::from(INTRO);
    let head = "| Action | Name | Windows and Linux | macOS |\n| --- | --- | --- | --- |\n";
    out.push_str("## The app's actions\n\n");
    out.push_str(head);
    for a in APP_ACTIONS {
        out.push_str(&row(a.name, a.title));
    }
    out.push_str(
        "\n## Commands\n\nThe commands of the [command reference](commands.md), as the palette lists them. \
         One with parameters opens a small form for them.\n\n",
    );
    out.push_str(head);
    for n in Command::NAMES {
        out.push_str(&row(n, command_title(n)));
    }
    out
}

#[test]
fn every_default_key_names_a_listed_action() {
    for mac in [false, true] {
        for (k, a) in defaults_for(mac) {
            assert!(Chord::parse_for(k, mac).is_some(), "{k}");
            assert!(
                APP_ACTIONS.iter().any(|x| x.name == a) || Command::NAMES.contains(&a),
                "{a} is bound but not listed"
            );
        }
    }
}

#[test]
fn the_keyboard_reference_matches_the_code() {
    let want = render();
    if std::env::var("COLDOPEN_BLESS").is_ok_and(|v| v == "1") {
        assert!(
            !std::env::var("CI").is_ok_and(|v| !v.is_empty()),
            "refusing to bless the keyboard reference with CI set"
        );
        std::fs::write(PAGE, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(PAGE)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "docs/guide/src/reference/keyboard.md is out of date: run `COLDOPEN_BLESS=1 cargo test -p coldopen-app --test reference` and review it"
    );
}
