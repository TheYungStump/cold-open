//! The word rule shared by tracked `block.set_text`, revision marking and
//! script diff (docs/specs/changes.md §5.1): a word is a run of letters,
//! digits and apostrophes, and every other character is a word of its own.

use std::ops::Range;

/// The most pairs of words [`common_words`] compares before it gives up.
pub const MAX_PAIRS: usize = 1_000_000;

/// Whether `c` belongs inside a word.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '\u{2019}'
}

/// The words of `chars`, as ranges of indices into it, in order and
/// covering it.
pub fn split(chars: &[char]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if is_word_char(chars[i]) {
            while i < chars.len() && is_word_char(chars[i]) {
                i += 1;
            }
        } else {
            i += 1;
        }
        out.push(start..i);
    }
    out
}

/// The longest common subsequence of `a` and `b`, as pairs of indices in
/// increasing order, or `None` when they hold more than [`MAX_PAIRS`] pairs
/// to compare. Ties go to the earliest pairs, so the result depends only on
/// the two sequences.
pub fn common<T: PartialEq>(a: &[T], b: &[T]) -> Option<Vec<(usize, usize)>> {
    let (n, m) = (a.len(), b.len());
    if n.checked_mul(m)? > MAX_PAIRS {
        return None;
    }
    // lengths[i][j]: the LCS of a[i..] and b[j..].
    let w = m + 1;
    let mut lengths = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lengths[i * w + j] = if a[i] == b[j] {
                lengths[(i + 1) * w + j + 1] + 1
            } else {
                lengths[(i + 1) * w + j].max(lengths[i * w + j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * w + j] >= lengths[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        let c: Vec<char> = s.chars().collect();
        split(&c)
            .into_iter()
            .map(|r| c[r].iter().collect())
            .collect()
    }

    #[test]
    fn words_and_single_characters() {
        assert_eq!(
            words("I don't, she said."),
            ["I", " ", "don't", ",", " ", "she", " ", "said", "."]
        );
        assert_eq!(words("café 42"), ["café", " ", "42"]);
        assert!(words("").is_empty());
    }

    #[test]
    fn the_common_subsequence_prefers_early_pairs() {
        assert_eq!(common(&[1, 2, 3], &[1, 3]), Some(vec![(0, 0), (2, 1)]));
        assert_eq!(common(&[1, 1], &[1]), Some(vec![(0, 0)]));
        assert_eq!(common::<u8>(&[], &[1]), Some(vec![]));
        let long = vec![0u8; 1001];
        assert_eq!(common(&long, &long), None);
    }
}
