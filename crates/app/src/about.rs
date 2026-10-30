//! What the About sheet shows (docs/specs/editor.md §22): the version and
//! commit, where the source is, and every license in the build
//! (docs/specs/release.md §10).

use coldopen_core::build_info;

/// The source code's home.
pub const SOURCE_URL: &str = "https://github.com/TheYungStump/cold-open";

/// The user guide's address, once phase 6b publishes one. Its row in the
/// About sheet shows only when this is set.
pub const DOCS_URL: Option<&str> = None;

/// Where testers send feedback, once phase 6b sets it up. Its row shows
/// only when this is set.
pub const FEEDBACK_URL: Option<&str> = None;

/// Cold Open's own license, GPL-3.0-or-later.
pub const GPL: &str = include_str!("../LICENSE");
/// The licenses of the format, layout and font crates, which are also
/// available under MIT or Apache-2.0.
pub const MIT: &str = include_str!("../../../LICENSE-MIT");
pub const APACHE: &str = include_str!("../../../LICENSE-APACHE");

/// The notice that opens the Cold Open tab.
pub const NOTICE: &str = "Cold Open is free software: you can redistribute it and/or modify it \
under the terms of the GNU General Public License as published by the Free Software \
Foundation, either version 3 of the License, or (at your option) any later version. It \
is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without \
even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.";

/// The source at the commit this build came from, or the project's page
/// when the commit is unknown.
pub fn source_link() -> String {
    if build_info::COMMIT == "unknown" {
        SOURCE_URL.to_string()
    } else {
        format!("{SOURCE_URL}/tree/{}", build_info::COMMIT)
    }
}

/// The lines "Copy version info" puts on the clipboard, for a bug report.
/// `renderer` is what draws the window.
pub fn version_info(renderer: &str) -> String {
    format!(
        "Cold Open {}\nTarget: {}\nRenderer: {renderer}\nSource: {}\n",
        build_info::VERSION_LINE,
        build_info::TARGET,
        source_link()
    )
}

/// The third-party license file compiled into this build, or `None` in a
/// build made without it (a developer's `cargo build`).
pub fn third_party() -> Option<String> {
    let packed: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/third-party.deflate"));
    if packed.is_empty() {
        return None;
    }
    let bytes = miniz_oxide::inflate::decompress_to_vec(packed).ok()?;
    String::from_utf8(bytes).ok()
}

/// What a build without the third-party file says in its place.
pub const NO_THIRD_PARTY: &str = "This build was made without the list of third-party \
licenses. Release builds include it, here and as THIRD-PARTY-LICENSES.txt in the \
licenses folder beside the app. It is made with cargo-about from about.toml in the \
source (docs/specs/release.md §10).";

/// The About sheet's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AboutTab {
    #[default]
    ColdOpen,
    Libraries,
    Fonts,
    ThirdParty,
}

impl AboutTab {
    pub const ALL: [AboutTab; 4] = [
        AboutTab::ColdOpen,
        AboutTab::Libraries,
        AboutTab::Fonts,
        AboutTab::ThirdParty,
    ];

    pub fn title(self) -> &'static str {
        match self {
            AboutTab::ColdOpen => "Cold Open",
            AboutTab::Libraries => "Format libraries",
            AboutTab::Fonts => "Fonts",
            AboutTab::ThirdParty => "Third-party",
        }
    }

    /// The tab after this one, wrapping, or before it with `back`.
    pub fn next(self, back: bool) -> AboutTab {
        let n = AboutTab::ALL.len();
        let i = AboutTab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        AboutTab::ALL[if back { (i + n - 1) % n } else { (i + 1) % n }]
    }

    /// The tab's text, shown a line at a time.
    pub fn text(self) -> String {
        match self {
            AboutTab::ColdOpen => format!("{NOTICE}\n\n{GPL}"),
            AboutTab::Libraries => format!(
                "The Fountain and .fdx libraries, the layout engine and the font crate \
                 (coldopen-screenplay, coldopen-fountain, coldopen-fdx, coldopen-layout, \
                 coldopen-diff and coldopen-fonts) are also available under the MIT \
                 license or the Apache License 2.0, at your option.\n\n{MIT}\n\n{APACHE}"
            ),
            AboutTab::Fonts => format!(
                "Courier Prime, for the page, and Atkinson Hyperlegible Next, subset to \
                 Latin, for the interface, both under the SIL Open Font License 1.1.\n\n\
                 Courier Prime\n\n{}\n\nAtkinson Hyperlegible Next\n\n{}",
                coldopen_fonts::LICENSE,
                coldopen_fonts::UI_LICENSE
            ),
            AboutTab::ThirdParty => third_party().unwrap_or_else(|| NO_THIRD_PARTY.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tab_has_its_license() {
        assert!(
            AboutTab::ColdOpen
                .text()
                .contains("GNU GENERAL PUBLIC LICENSE")
        );
        assert!(
            AboutTab::Libraries
                .text()
                .contains("Permission is hereby granted")
        );
        assert!(AboutTab::Libraries.text().contains("Apache License"));
        assert!(
            AboutTab::Fonts
                .text()
                .contains("SIL OPEN FONT LICENSE Version 1.1")
        );
        assert!(!AboutTab::ThirdParty.text().is_empty());
    }

    #[test]
    fn tabs_wrap_both_ways() {
        assert_eq!(AboutTab::ThirdParty.next(false), AboutTab::ColdOpen);
        assert_eq!(AboutTab::ColdOpen.next(true), AboutTab::ThirdParty);
    }

    #[test]
    fn version_info_names_the_build() {
        let v = version_info("CPU");
        assert!(v.contains(build_info::VERSION));
        assert!(v.contains(build_info::TARGET));
        assert!(v.contains("Renderer: CPU"));
        assert!(v.contains(SOURCE_URL));
    }
}
