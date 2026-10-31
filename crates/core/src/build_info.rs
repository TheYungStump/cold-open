//! The version and commit of this build, for every front end
//! (docs/specs/release.md §9).

/// The workspace's version, from the root `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The first 12 hex digits of the commit this build comes from, or
/// `unknown`.
pub const COMMIT: &str = env!("COLDOPEN_COMMIT");

/// `<version> (<commit>)`, as `coldopen --version` and the About sheet
/// show it.
pub const VERSION_LINE: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("COLDOPEN_COMMIT"),
    ")"
);

/// The target this build was compiled for, such as
/// `x86_64-pc-windows-msvc`.
pub const TARGET: &str = env!("COLDOPEN_TARGET");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_line_names_the_version_and_commit() {
        assert_eq!(VERSION_LINE, format!("{VERSION} ({COMMIT})"));
        assert!(
            COMMIT == "unknown"
                || (COMMIT.len() == 12 && COMMIT.bytes().all(|b| b.is_ascii_hexdigit())),
            "{COMMIT}"
        );
        assert!(!TARGET.is_empty());
    }
}
