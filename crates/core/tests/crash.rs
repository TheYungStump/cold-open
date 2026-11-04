//! The crash scrubber's fuzzed property (docs/specs/crash-reports.md §10),
//! on every CI build too: random bytes, and random text around a string
//! quoted as Rust quotes one.

use proptest::prelude::*;

/// CI's find (2026-11-03): unquoted `D\0���` after the quoted `"D���"`
/// reads `D���` once the scrubber takes the NUL out. That text was never
/// quoted, so the check compares the text around a string as the scrubber
/// leaves it.
#[test]
fn letters_joined_by_a_dropped_control_character_were_never_quoted() {
    let data = [0, 68, 128, 128, 128, 0, 0, 34, 34, 68, 0, 128, 128, 128, 0];
    assert_eq!(coldopen_core::fuzzing::crash_scrub(&data), Ok(()));
}

/// The fuzz run's find (2026-11-03): `[quoted]` followed by `^����` reads
/// `]^����`, the string that was quoted, though none of it was kept.
#[test]
fn the_scrubbers_own_words_joined_to_the_text_after_were_never_quoted() {
    let data = [0, 93, 94, 133, 133, 133, 133, 0, 94, 133, 133, 133, 133];
    assert_eq!(coldopen_core::fuzzing::crash_scrub(&data), Ok(()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn random_bytes_scrub_safely(data in proptest::collection::vec(any::<u8>(), 0..2000)) {
        prop_assert_eq!(coldopen_core::fuzzing::crash_scrub(&data), Ok(()));
    }

    #[test]
    fn a_quoted_string_never_survives(
        before in r"\PC{0,80}",
        secret in r"\PC{4,60}",
        after in r"\PC{0,80}",
    ) {
        let data = format!("{before}\0{secret}\0{after}");
        prop_assert_eq!(coldopen_core::fuzzing::crash_scrub(data.as_bytes()), Ok(()));
    }
}
