//! Where Report a problem and crash reports go (docs/specs/editor.md §23,
//! docs/specs/crash-reports.md §7): a GitHub issue or the project's Google
//! Form, filled in from the address's query. Cold Open opens the address
//! in the browser and sends nothing itself.

use coldopen_core::build_info;
use coldopen_core::crash::Report;

/// New issues, by template.
pub const ISSUES: &str = "https://github.com/TheYungStump/cold-open/issues/new";

/// The Google Form's address (its `viewform` page). `None` hides the
/// form's buttons, for a build without a form.
pub const FORM_URL: Option<&str> = Some(
    "https://docs.google.com/forms/d/e/1FAIpQLSfuNuITmor09PTaR-_IX17x8w-yB3xDLDU9JgD6U3zyilyE7g/viewform",
);

/// The form's question ids, from its "Get pre-filled link" (`entry.<id>`).
pub struct FormEntries {
    pub kind: &'static str,
    pub details: &'static str,
    pub version: &'static str,
    pub os: &'static str,
    pub renderer: &'static str,
    pub report: &'static str,
}

/// [`FORM_URL`]'s questions (2026-11-03). The kind's choices include
/// "Bug" and "Crash", spelled as [`bug_form`] and [`crash_form`] send them.
pub const FORM_ENTRIES: FormEntries = FormEntries {
    kind: "1518187895",
    details: "1721458030",
    version: "731550696",
    os: "27572256",
    renderer: "2093373239",
    report: "485406472",
};

/// The longest address opened, which Windows' shell passes on whole
/// (crash-reports.md §7).
pub const URL_MAX: usize = 2000;

/// What Report a problem fills in (editor.md §23): nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Details {
    pub version: String,
    pub target: String,
    pub os: String,
    pub renderer: String,
}

impl Details {
    /// This build on this computer, drawn with `renderer`.
    pub fn now(renderer: String) -> Details {
        Details {
            version: build_info::VERSION_LINE.to_string(),
            target: build_info::TARGET.to_string(),
            os: crate::bench::machine::os_version(),
            renderer,
        }
    }

    /// The lines the sheet shows and Copy puts on the clipboard.
    pub fn lines(&self) -> String {
        format!(
            "Version: {}\nTarget: {}\nOS: {}\nRenderer: {}\n",
            self.version, self.target, self.os, self.renderer
        )
    }

    fn version_target(&self) -> String {
        format!("{} ({})", self.version, self.target)
    }
}

/// A new bug issue with the details filled in.
pub fn bug_issue(d: &Details) -> String {
    let base = format!("{ISSUES}?template=bug.yml");
    fit(
        &base,
        &[
            ("version", d.version_target()),
            ("os", d.os.clone()),
            ("renderer", d.renderer.clone()),
        ],
        None,
    )
}

/// The form, set to a bug, with the details filled in.
pub fn bug_form(d: &Details) -> Option<String> {
    let url = FORM_URL?;
    let e = &FORM_ENTRIES;
    Some(fit(
        &format!("{url}?usp=pp_url"),
        &[
            (&entry(e.kind), "Bug".to_string()),
            (&entry(e.version), d.version_target()),
            (&entry(e.os), d.os.clone()),
            (&entry(e.renderer), d.renderer.clone()),
        ],
        None,
    ))
}

/// A new crash issue: the report's lines filled in, and its summary cut to
/// fit. The whole report goes by the clipboard.
pub fn crash_issue(r: &Report) -> String {
    fit(
        &format!("{ISSUES}?template=crash.yml"),
        &[
            ("version", format!("{} ({})", r.version, r.target)),
            ("os", r.os.clone()),
            ("renderer", r.renderer.clone()),
        ],
        Some(("summary", r.summary())),
    )
}

/// The form, set to a crash, with the report's lines and summary.
pub fn crash_form(r: &Report) -> Option<String> {
    let url = FORM_URL?;
    let e = &FORM_ENTRIES;
    Some(fit(
        &format!("{url}?usp=pp_url"),
        &[
            (&entry(e.kind), "Crash".to_string()),
            (&entry(e.version), format!("{} ({})", r.version, r.target)),
            (&entry(e.os), r.os.clone()),
            (&entry(e.renderer), r.renderer.clone()),
        ],
        Some((&entry(e.report), r.summary())),
    ))
}

fn entry(id: &str) -> String {
    format!("entry.{id}")
}

/// An address to open, and whether everything the sheet showed is in it.
/// When it isn't, the caller copies [`all_text`] first and says to paste it
/// (editor.md §23).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub url: String,
    pub whole: bool,
}

/// `base` with every field, when the whole address fits in [`URL_MAX`].
fn whole(base: &str, fields: &[(&str, String)]) -> Option<String> {
    let mut url = base.to_string();
    for (k, v) in fields.iter().filter(|(_, v)| !v.is_empty()) {
        url.push('&');
        url.push_str(k);
        url.push('=');
        url.push_str(&encode(v));
    }
    (url.len() <= URL_MAX).then_some(url)
}

/// Everything Share feedback or Report a problem sends, as one block: the
/// writer's text, the details, then the usage summary if it was ticked.
/// Copy all, and the clipboard when an address can't hold it all.
pub fn all_text(text: &str, d: &Details, summary: Option<&str>) -> String {
    let mut out = String::new();
    if !text.trim().is_empty() {
        out.push_str(text.trim_end());
        out.push_str("\n\n");
    }
    out.push_str(&d.lines());
    if let Some(s) = summary {
        out.push('\n');
        out.push_str(s);
    }
    out
}

/// Share feedback's GitHub issue (the feedback template): the text, the
/// details, and the summary if ticked (editor.md §23).
pub fn share_issue(text: &str, d: &Details, summary: Option<&str>) -> Address {
    let base = format!("{ISSUES}?template=feedback.yml");
    let mut fields = vec![
        ("version", d.version_target()),
        ("os", d.os.clone()),
        ("renderer", d.renderer.clone()),
        ("what", text.trim_end().to_string()),
    ];
    if let Some(s) = summary {
        fields.push(("usage", s.to_string()));
    }
    match whole(&base, &fields) {
        Some(url) => Address { url, whole: true },
        None => Address {
            url: fit(&base, &fields[..3], Some(("what", text.to_string()))),
            whole: false,
        },
    }
}

/// Share feedback's form: the kind left for the writer to choose, the text
/// in `details`, the details in their boxes, the summary in `report`.
pub fn share_form(text: &str, d: &Details, summary: Option<&str>) -> Option<Address> {
    let url = FORM_URL?;
    let e = &FORM_ENTRIES;
    let base = format!("{url}?usp=pp_url");
    let mut fields = vec![
        (entry(e.version), d.version_target()),
        (entry(e.os), d.os.clone()),
        (entry(e.renderer), d.renderer.clone()),
        (entry(e.details), text.trim_end().to_string()),
    ];
    if let Some(s) = summary {
        fields.push((entry(e.report), s.to_string()));
    }
    let refs: Vec<(&str, String)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    Some(match whole(&base, &refs) {
        Some(url) => Address { url, whole: true },
        None => Address {
            url: fit(
                &base,
                &refs[..3],
                Some((&entry(e.details), text.to_string())),
            ),
            whole: false,
        },
    })
}

/// Report a problem's issue with the usage summary ticked: the bug
/// template's fields, and the summary in `usage` when it fits.
pub fn bug_issue_with(d: &Details, summary: Option<&str>) -> Address {
    let Some(s) = summary else {
        return Address {
            url: bug_issue(d),
            whole: true,
        };
    };
    let base = format!("{ISSUES}?template=bug.yml");
    let fields = [
        ("version", d.version_target()),
        ("os", d.os.clone()),
        ("renderer", d.renderer.clone()),
        ("usage", s.to_string()),
    ];
    match whole(&base, &fields) {
        Some(url) => Address { url, whole: true },
        None => Address {
            url: bug_issue(d),
            whole: false,
        },
    }
}

/// Report a problem's form with the usage summary ticked, in `report`.
pub fn bug_form_with(d: &Details, summary: Option<&str>) -> Option<Address> {
    let plain = bug_form(d)?;
    let Some(s) = summary else {
        return Some(Address {
            url: plain,
            whole: true,
        });
    };
    let url = format!("{plain}&{}={}", entry(FORM_ENTRIES.report), encode(s));
    Some(if url.len() <= URL_MAX {
        Address { url, whole: true }
    } else {
        Address {
            url: plain,
            whole: false,
        }
    })
}

/// `base` with each field added to its query, and `long` last, cut with
/// `…` until the whole address is at most [`URL_MAX`].
fn fit(base: &str, fields: &[(&str, String)], long: Option<(&str, String)>) -> String {
    let mut url = base.to_string();
    for (k, v) in fields {
        url.push('&');
        url.push_str(k);
        url.push('=');
        url.push_str(&encode(v));
    }
    let Some((k, v)) = long else {
        return url;
    };
    let mut chars: Vec<char> = v.chars().collect();
    let mut cut = false;
    loop {
        let mut text: String = chars.iter().collect();
        if cut {
            text.push('…');
        }
        let candidate = format!("{url}&{k}={}", encode(&text));
        if candidate.len() <= URL_MAX || chars.is_empty() {
            return candidate;
        }
        // Each character is at most 12 bytes encoded; drop enough for the
        // excess, and at least one.
        let excess = candidate.len() - URL_MAX;
        let drop = excess.div_ceil(12).max(1).min(chars.len());
        chars.truncate(chars.len() - drop);
        cut = true;
    }
}

/// Percent-encoding for a query value: everything but the unreserved
/// characters of RFC 3986.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details() -> Details {
        Details {
            version: "0.1.0-alpha.2 (5d6e764a1b2c)".into(),
            target: "x86_64-pc-windows-msvc".into(),
            os: "Windows 11 Home (26100)".into(),
            renderer: "CPU".into(),
        }
    }

    #[test]
    fn encoding_keeps_only_unreserved_characters() {
        assert_eq!(encode("a b&c=d/é~"), "a%20b%26c%3Dd%2F%C3%A9~");
    }

    #[test]
    fn a_bug_issue_holds_the_details_and_nothing_else() {
        let url = bug_issue(&details());
        assert!(
            url.starts_with(
                "https://github.com/TheYungStump/cold-open/issues/new?template=bug.yml&"
            )
        );
        assert!(url.contains(
            "&version=0.1.0-alpha.2%20%285d6e764a1b2c%29%20%28x86_64-pc-windows-msvc%29"
        ));
        assert!(url.contains("&os=Windows%2011%20Home%20%2826100%29"));
        assert!(url.contains("&renderer=CPU"));
        assert_eq!(url.matches('&').count(), 3);
    }

    #[test]
    fn a_crash_issue_fits_with_a_long_message() {
        let r = Report {
            version: "0.1.0-alpha.2 (5d6e764a1b2c)".into(),
            target: "x86_64-pc-windows-msvc".into(),
            os: "Windows 11 Home (26100)".into(),
            renderer: "GPU: NVIDIA GeForce RTX 4090 (Vulkan)".into(),
            location: "crates/app/src/editor/mod.rs:812:22".into(),
            message: "é".repeat(coldopen_core::crash::MESSAGE_MAX),
            frames: vec!["Cold Open.exe+0x1".into(); 100],
            ..Report::default()
        };
        let url = crash_issue(&r);
        assert!(url.len() <= URL_MAX, "{}", url.len());
        assert!(url.contains("template=crash.yml"));
        assert!(url.contains("&summary="));
        assert!(url.ends_with(&encode("…")), "the summary is cut");
        // The frames go by the clipboard, never in the address.
        assert!(!url.contains("Cold%20Open.exe%2B0x1"));
        let short = Report {
            message: "boom".into(),
            ..r
        };
        assert!(
            crash_issue(&short).ends_with(&encode("boom at crates/app/src/editor/mod.rs:812:22"))
        );
    }

    #[test]
    fn the_form_holds_the_kind_and_details_and_nothing_else() {
        let url = bug_form(&details()).expect("the form's address is set");
        assert!(url.starts_with(&format!("{}?usp=pp_url&", FORM_URL.unwrap())));
        assert!(url.contains("&entry.1518187895=Bug&"));
        assert!(url.contains(
            "&entry.731550696=0.1.0-alpha.2%20%285d6e764a1b2c%29%20%28x86_64-pc-windows-msvc%29"
        ));
        assert!(url.contains("&entry.27572256=Windows%2011%20Home%20%2826100%29"));
        assert!(url.ends_with("&entry.2093373239=CPU"));
        assert_eq!(url.matches('&').count(), 4, "{url}");
        // What happened is the writer's to fill in.
        assert!(!url.contains(&entry(FORM_ENTRIES.details)));
    }

    #[test]
    fn share_feedback_sends_what_the_sheet_shows() {
        let d = details();
        let summary = "Usage summary from Cold Open.\nCommands you used: undo 2\n";
        let a = share_issue("It's lovely.", &d, Some(summary));
        assert!(a.whole);
        assert!(a.url.contains("template=feedback.yml"));
        assert!(a.url.contains("&what=It%27s%20lovely."));
        assert!(a.url.ends_with(&format!("&usage={}", encode(summary))));
        let f = share_form("It's lovely.", &d, Some(summary)).unwrap();
        assert!(f.whole);
        assert!(f.url.contains(&format!(
            "&{}=It%27s%20lovely.",
            entry(FORM_ENTRIES.details)
        )));
        assert!(f.url.ends_with(&format!(
            "&{}={}",
            entry(FORM_ENTRIES.report),
            encode(summary)
        )));
        // The kind is the writer's to choose.
        assert!(!f.url.contains(&entry(FORM_ENTRIES.kind)));
        // Without the tick, nothing of the summary.
        assert!(!share_issue("x", &d, None).url.contains("usage"));
        assert!(
            !share_form("x", &d, None)
                .unwrap()
                .url
                .contains(FORM_ENTRIES.report)
        );
    }

    #[test]
    fn too_long_for_an_address_goes_by_the_clipboard() {
        let d = details();
        let long = "word ".repeat(2000);
        let a = share_issue(&long, &d, None);
        assert!(!a.whole);
        assert!(a.url.len() <= URL_MAX);
        let f = share_form(&long, &d, Some("summary")).unwrap();
        assert!(!f.whole);
        assert!(!f.url.contains(FORM_ENTRIES.report));
        let all = all_text(&long, &d, Some("summary"));
        assert!(all.starts_with("word word"));
        assert!(all.contains("Version: 0.1.0-alpha.2"));
        assert!(all.ends_with("\nsummary"));
        let b = bug_issue_with(&d, Some(&"x".repeat(3000)));
        assert!(!b.whole);
        assert_eq!(b.url, bug_issue(&d));
        let short = bug_form_with(&d, Some("Commands you used: undo 2")).unwrap();
        assert!(short.whole);
        assert!(short.url.contains(FORM_ENTRIES.report));
    }

    #[test]
    fn the_crash_form_fits_with_a_long_message() {
        let r = Report {
            version: "0.1.0-alpha.2 (5d6e764a1b2c)".into(),
            target: "x86_64-pc-windows-msvc".into(),
            os: "Windows 11 Home (26100)".into(),
            renderer: "CPU".into(),
            location: "crates/app/src/editor/mod.rs:812:22".into(),
            message: "é".repeat(coldopen_core::crash::MESSAGE_MAX),
            frames: vec!["Cold Open.exe+0x1".into(); 100],
            ..Report::default()
        };
        let url = crash_form(&r).expect("the form's address is set");
        assert!(url.len() <= URL_MAX, "{}", url.len());
        assert!(url.contains("&entry.1518187895=Crash&"));
        assert!(url.contains("&entry.485406472="));
        assert!(url.ends_with(&encode("…")), "the summary is cut");
        assert!(!url.contains("Cold%20Open.exe%2B0x1"));
    }
}
