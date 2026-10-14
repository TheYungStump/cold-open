//! Aligning reference text to our script (docs/specs/layout.md §13):
//! whitespace removed, quotes, dashes and case folded, then a two-pointer
//! walk that resynchronizes on unique anchors.

/// Characters with no meaning for alignment.
fn skip(c: char) -> bool {
    c.is_whitespace() || c == '\u{AD}' || c.is_control()
}

/// Appends the folded form of `c` to `out`.
pub fn fold(c: char, out: &mut Vec<char>) {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{2032}' | '`' | '\u{B4}' => {
            out.push('\'')
        }
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => out.push('"'),
        '\u{2010}'..='\u{2013}' | '\u{2212}' => out.push('-'),
        '\u{2014}' | '\u{2015}' => out.extend(['-', '-']),
        '\u{2026}' => out.extend(['.', '.', '.']),
        c => out.extend(c.to_uppercase()),
    }
}

/// A folded character and where it came from.
#[derive(Clone, Copy, Debug)]
pub struct Ch<P> {
    pub c: char,
    pub at: P,
}

/// Folds `text`, tagging every output character with `tag(byte_offset)`.
pub fn fold_str<P: Copy>(text: &str, mut tag: impl FnMut(usize) -> P, out: &mut Vec<Ch<P>>) {
    let mut buf = Vec::with_capacity(3);
    for (i, c) in text.char_indices() {
        if skip(c) {
            continue;
        }
        buf.clear();
        fold(c, &mut buf);
        let at = tag(i);
        out.extend(buf.iter().map(|&c| Ch { c, at }));
    }
}

/// Result of an alignment: for each reference character, the index of the
/// flow character it matched.
pub struct Alignment {
    pub ref_to_flow: Vec<Option<usize>>,
    pub flow_matched: usize,
    pub ref_matched: usize,
    /// Unmatched stretches: (flow start, flow len, ref start, ref len).
    pub gaps: Vec<(usize, usize, usize, usize)>,
}

const K: usize = 16;
const WINDOW: usize = 6000;

/// Finds where `needle` occurs exactly once in `hay[from..from+WINDOW]`.
fn find_unique(hay: &[char], from: usize, needle: &[char]) -> Option<usize> {
    let end = (from + WINDOW).min(hay.len());
    if needle.len() < K || end < from + needle.len() {
        return None;
    }
    let mut found = None;
    for p in from..=end - needle.len() {
        if hay[p..p + needle.len()] == *needle {
            if found.is_some() {
                return None;
            }
            found = Some(p);
        }
    }
    found
}

pub fn align<A, B>(flow: &[Ch<A>], refs: &[Ch<B>]) -> Alignment {
    let fc: Vec<char> = flow.iter().map(|c| c.c).collect();
    let rc: Vec<char> = refs.iter().map(|c| c.c).collect();
    let mut map = vec![None; rc.len()];
    let (mut i, mut j) = (0usize, 0usize);
    let mut gaps = Vec::new();
    let (mut fm, mut rm) = (0, 0);
    while i < fc.len() && j < rc.len() {
        if fc[i] == rc[j] {
            map[j] = Some(i);
            i += 1;
            j += 1;
            fm += 1;
            rm += 1;
            continue;
        }
        // Resynchronize: the nearest unique anchor on either side.
        let mut best: Option<(usize, usize)> = None;
        for probe in 0..64 {
            let (fi, rj) = (i + probe, j + probe);
            if fi + K <= fc.len()
                && let Some(p) = find_unique(&rc, j, &fc[fi..fi + K])
                && p >= j
            {
                let cand = (fi - i, p - j);
                if best.is_none_or(|b| cand.0 + cand.1 < b.0 + b.1) {
                    best = Some(cand);
                }
            }
            if rj + K <= rc.len()
                && let Some(p) = find_unique(&fc, i, &rc[rj..rj + K])
                && p >= i
            {
                let cand = (p - i, rj - j);
                if best.is_none_or(|b| cand.0 + cand.1 < b.0 + b.1) {
                    best = Some(cand);
                }
            }
            if best.is_some() {
                break;
            }
        }
        let (di, dj) = best.unwrap_or((1, 1));
        let (di, dj) = if di == 0 && dj == 0 { (1, 1) } else { (di, dj) };
        gaps.push((i, di, j, dj));
        i += di;
        j += dj;
    }
    if i < fc.len() || j < rc.len() {
        gaps.push((i, fc.len() - i, j, rc.len() - j));
    }
    Alignment {
        ref_to_flow: map,
        flow_matched: fm,
        ref_matched: rm,
        gaps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<Ch<usize>> {
        let mut v = Vec::new();
        fold_str(s, |i| i, &mut v);
        v
    }

    #[test]
    fn folds_and_aligns_through_differences() {
        let flow = chars("He said “stop” — and ran. Then the long quiet street was empty again.");
        let refs =
            chars("HE SAID \"STOP\" -- AND RAN. XX THEN THE LONG QUIET STREET WAS EMPTY AGAIN.");
        let a = align(&flow, &refs);
        assert_eq!(a.flow_matched, flow.len());
        assert_eq!(a.ref_matched, refs.len() - 2);
    }
}
