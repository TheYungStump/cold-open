//! The crash scrubber's fuzzed property (docs/specs/crash-reports.md §10),
//! on every CI build too: random bytes, and random text around a string
//! quoted as Rust quotes one.

use proptest::prelude::*;

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
