//! Reading and writing take time in proportion to the input, even for
//! hostile shapes (review finding F15: an 80 KB file took 22 seconds to
//! open). Each check times one size and four times that size; linear work
//! grows about 4×, quadratic about 16×. Timing the growth, not a fixed
//! budget, keeps the check fair on slow machines and in debug builds.

use std::time::{Duration, Instant};

use coldopen_fountain::{parse, write};

/// The fastest of three runs, which damps scheduler noise.
fn time(f: impl Fn()) -> Duration {
    (0..3)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

/// Fails if the work for `shape(4 * n)` grows like n², not n.
fn grows_linearly(name: &str, n: usize, shape: impl Fn(usize) -> String) {
    let small = shape(n);
    let large = shape(4 * n);
    let read = |s: &String| {
        let parsed = parse(s);
        std::hint::black_box(write(&parsed.script));
    };
    let a = time(|| read(&small));
    let b = time(|| read(&large));
    let ratio = b.as_secs_f64() / a.as_secs_f64().max(1e-6);
    println!(
        "{name}: {} bytes {a:?}, {} bytes {b:?}, ratio {ratio:.1}",
        small.len(),
        large.len()
    );
    assert!(
        ratio < 9.0,
        "{name} grows like the square of its size: ratio {ratio:.1}"
    );
}

#[test]
fn a_character_with_many_unclosed_parentheticals() {
    grows_linearly("open parens", 4_000, |n| {
        format!("BOB\n{}", "(\n".repeat(n))
    });
}

#[test]
fn one_long_speech_of_names_and_open_parentheticals() {
    // The reviewer's 240 KB case took 46 seconds.
    grows_linearly("speech", 2_000, |n| "BOB\n(\n".repeat(n));
}

#[test]
fn a_long_line_of_unmatched_emphasis() {
    grows_linearly("emphasis", 4_000, |n| "_a* ".repeat(n));
}
