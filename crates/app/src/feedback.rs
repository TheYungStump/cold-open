//! Where Report a problem and crash reports go (docs/specs/editor.md §23,
//! docs/specs/crash-reports.md §7): a GitHub issue or the project's Google
//! Form, filled in from the address's query. Cold Open opens the address
//! in the browser and sends nothing itself.

use coldopen_core::build_info;
use coldopen_core::crash::Report;

/// New issues, by template.
pub const ISSUES: &str = "https://github.com/TheYungStump/cold-open/issues/new";

/// The Google Form's address (its `viewform` page), once the project
/// makes it. Until then the form's buttons don't show.
pub const FORM_URL: Option<&str> = None;

/// The form's question ids, from its "Get pre-filled link" (`entry.<id>`).
pub struct FormEntries {
    pub kind: &'static str,
    pub details: &'static str,
    pub version: &'static str,
    pub os: &'static str,
    pub renderer: &'static str,
    pub report: &'static str,
}

/// Filled in with [`FORM_URL`].
pub const FORM_ENTRIES: FormEntries = FormEntries {
    kind: "",
    details: "",
    version: "",
    os: "",
    renderer: "",
    report: "",
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
    fn no_form_until_its_address_is_set() {
        if FORM_URL.is_none() {
            assert!(bug_form(&details()).is_none());
        } else {
            let url = bug_form(&details()).unwrap();
            assert!(url.len() <= URL_MAX && url.contains("usp=pp_url"));
        }
    }
}
