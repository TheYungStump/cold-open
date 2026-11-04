//! The command-line reference on the website (docs/specs/site.md §4),
//! rendered from the CLI's own clap definitions and checked against the
//! page, so the two cannot drift. `COLDOPEN_BLESS=1 cargo test -p
//! coldopen-cli reference` rewrites the page; CI refuses to.

use clap::CommandFactory;

use super::Cli;

const PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/guide/src/reference/cli.md"
);

/// Help text for people: one line, without the spec references the code's
/// comments carry for developers.
fn plain(s: &str) -> String {
    let mut out = s.split_whitespace().collect::<Vec<_>>().join(" ");
    while let Some(i) = out.find(" (docs/specs/") {
        match out[i..].find(')') {
            Some(j) => out.replace_range(i..i + j + 1, ""),
            None => break,
        }
    }
    // A `<name>` in the text would read as an HTML tag.
    let mut out = out.replace('|', "\\|").replace('<', "\\<");
    // clap drops a help text's last full stop.
    if !out.is_empty() && !out.ends_with(['.', '!', '?', ':', ')']) {
        out.push('.');
    }
    out
}

fn argument(a: &clap::Arg) -> String {
    let values: Vec<String> = a
        .get_value_names()
        .map(|v| v.iter().map(|n| format!("<{n}>")).collect())
        .unwrap_or_default();
    let name = match (a.get_long(), a.get_short()) {
        (Some(l), Some(s)) => format!("-{s}, --{l}"),
        (Some(l), None) => format!("--{l}"),
        (None, Some(s)) => format!("-{s}"),
        (None, None) => values.join(" "),
    };
    let takes = a.get_action().takes_values();
    let name = if a.is_positional() || !takes {
        name
    } else {
        format!("{name} {}", values.join(" "))
    };
    let mut help = a
        .get_long_help()
        .or(a.get_help())
        .map(|h| plain(&h.to_string()))
        .unwrap_or_default();
    let choices: Vec<String> = a
        .get_possible_values()
        .iter()
        .filter(|p| !p.is_hide_set())
        .map(|p| format!("`{}`", p.get_name()))
        .collect();
    if takes && !choices.is_empty() {
        help.push_str(&format!(" One of {}.", choices.join(", ")));
    }
    let defaults: Vec<String> = a
        .get_default_values()
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    if takes && !defaults.is_empty() {
        help.push_str(&format!(" Default: `{}`.", defaults.join(" ")));
    }
    format!("| `{name}` | {} |\n", help.trim())
}

fn command(out: &mut String, path: &str, c: &clap::Command) {
    let about = c
        .get_long_about()
        .or(c.get_about())
        .map(|a| plain(&a.to_string()))
        .unwrap_or_default();
    out.push_str(&format!("## `{path}`\n\n{about}\n\n"));
    let usage = c
        .clone()
        .bin_name(path)
        .render_usage()
        .to_string()
        .replace("Usage: ", "");
    out.push_str(&format!("```text\n{usage}\n```\n\n"));
    let args: Vec<&clap::Arg> = c
        .get_arguments()
        .filter(|a| !a.is_hide_set() && !matches!(a.get_id().as_str(), "help" | "version"))
        .collect();
    if !args.is_empty() {
        out.push_str("| Argument | What it does |\n| --- | --- |\n");
        for a in args {
            out.push_str(&argument(a));
        }
        out.push('\n');
    }
    for sub in c.get_subcommands().filter(|s| !s.is_hide_set()) {
        if sub.get_name() != "help" {
            command(out, &format!("{path} {}", sub.get_name()), sub);
        }
    }
}

pub fn render() -> String {
    let cli = Cli::command();
    let mut out = String::from(
        "# Command-line reference\n\n\
         <!-- Made from the CLI's own definitions by crates/cli/src/reference.rs. \
         Don't edit this page: change the code, then run \
         `COLDOPEN_BLESS=1 cargo test -p coldopen-cli reference`. -->\n\n\
         `coldopen` comes with the app on every system (see [Install](../guide/install.md)). \
         Every subcommand sends the same commands the app does, so a script is changed \
         the same way from here as from the window. `coldopen --version` prints the \
         version and commit, and `coldopen help <subcommand>` prints a subcommand's help.\n\n",
    );
    let subs: Vec<&clap::Command> = cli
        .get_subcommands()
        .filter(|s| !s.is_hide_set() && s.get_name() != "help")
        .collect();
    out.push_str("| Subcommand | What it does |\n| --- | --- |\n");
    for s in &subs {
        let about = s
            .get_about()
            .map(|a| plain(&a.to_string()))
            .unwrap_or_default();
        out.push_str(&format!(
            "| [`{}`](#coldopen-{}) | {about} |\n",
            s.get_name(),
            s.get_name()
        ));
    }
    out.push('\n');
    for s in subs {
        command(&mut out, &format!("coldopen {}", s.get_name()), s);
    }
    out.trim_end().to_string() + "\n"
}

#[test]
fn the_cli_reference_matches_the_code() {
    let want = render();
    if std::env::var("COLDOPEN_BLESS").is_ok_and(|v| v == "1") {
        assert!(
            !std::env::var("CI").is_ok_and(|v| !v.is_empty()),
            "refusing to bless the CLI reference with CI set"
        );
        std::fs::write(PAGE, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(PAGE)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "docs/guide/src/reference/cli.md is out of date: run `COLDOPEN_BLESS=1 cargo test -p coldopen-cli reference` and review it"
    );
}
