//! Sentence ends (layout.md §8), found on the source text before capitals.
#![deny(clippy::float_arithmetic)]

const TERMINATORS: [char; 4] = ['.', '!', '?', '…'];
const CLOSERS: [char; 6] = ['"', '\'', ')', ']', '\u{201D}', '\u{2019}'];
const OPENERS: [char; 8] = [
    '"', '\'', '(', '[', '\u{201C}', '\u{2018}', '\u{BF}', '\u{A1}',
];
const TITLES: [&str; 18] = [
    "MR", "MRS", "MS", "DR", "ST", "JR", "SR", "PROF", "REV", "GEN", "COL", "LT", "SGT", "CAPT",
    "CMDR", "GOV", "SEN", "VS",
];
const BEFORE_NUMBERS: [&str; 8] = ["NO", "VOL", "PG", "PP", "CH", "FIG", "APT", "RM"];

/// Byte offsets in `text` where a new sentence starts after a sentence end.
/// These are the split points for action and dialogue.
pub(crate) fn sentence_starts(text: &str, ellipsis_ends_sentence: bool) -> Vec<u32> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if !TERMINATORS.contains(&chars[i].1) {
            i += 1;
            continue;
        }
        let run_start = i;
        let mut j = i;
        while j < n && TERMINATORS.contains(&chars[j].1) {
            j += 1;
        }
        let run_end = j;
        while j < n && CLOSERS.contains(&chars[j].1) {
            j += 1;
        }
        i = run_end.max(run_start + 1);
        // 1. Whitespace must follow.
        if j >= n || !chars[j].1.is_whitespace() {
            continue;
        }
        // The next sentence starts at the next non-space character.
        let mut k = j;
        while k < n && chars[k].1.is_whitespace() {
            k += 1;
        }
        if k >= n {
            continue;
        }
        // 2. It must not start with a lowercase letter, after any openers.
        let mut c = k;
        while c < n && OPENERS.contains(&chars[c].1) {
            c += 1;
        }
        let first = chars.get(c).map(|x| x.1);
        if first.is_some_and(char::is_lowercase) {
            continue;
        }
        let run: Vec<char> = chars[run_start..run_end].iter().map(|x| x.1).collect();
        // 4. Ellipses.
        let dots = run.iter().filter(|&&c| c == '.').count();
        let has_ellipsis = dots >= 2 || run.contains(&'…');
        let ends_strong = matches!(run.last(), Some('?' | '!'));
        if has_ellipsis && !ends_strong && !ellipsis_ends_sentence {
            continue;
        }
        // 3. A single period after an abbreviation.
        if run == ['.'] && is_abbreviation(&chars[..run_start], first) {
            continue;
        }
        out.push(chars[k].0 as u32);
    }
    out
}

/// True when the word before a single period is an abbreviation that does
/// not end a sentence.
fn is_abbreviation(before: &[(usize, char)], next: Option<char>) -> bool {
    let start = before
        .iter()
        .rposition(|x| x.1.is_whitespace() || OPENERS.contains(&x.1))
        .map_or(0, |p| p + 1);
    let word: String = before[start..].iter().map(|x| x.1).collect();
    if word.is_empty() {
        return false;
    }
    // An initial: one letter of a script with case, such as "J.".
    let mut chars = word.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && (c.is_uppercase() || c.is_lowercase())
    {
        return true;
    }
    if word.contains('.') {
        return true;
    }
    let upper = word.to_uppercase();
    if TITLES.contains(&upper.as_str()) {
        return true;
    }
    BEFORE_NUMBERS.contains(&upper.as_str()) && next.is_some_and(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starts(s: &str) -> Vec<&str> {
        sentence_starts(s, false)
            .into_iter()
            .map(|o| &s[o as usize..])
            .collect()
    }

    #[test]
    fn simple_sentences() {
        assert_eq!(
            starts("One. Two! Three? Four"),
            vec!["Two! Three? Four", "Three? Four", "Four"]
        );
        assert_eq!(starts("Run!\nNow."), vec!["Now."]);
        assert_eq!(starts("Last one."), Vec::<&str>::new());
    }

    #[test]
    fn closers_and_openers() {
        assert_eq!(starts("He said \"Stop.\" Then left."), vec!["Then left."]);
        assert_eq!(starts("Fine.) (Next."), vec!["(Next."]);
        assert_eq!(starts("Go.” “Where?"), vec!["“Where?"]);
    }

    #[test]
    fn not_sentence_ends() {
        assert!(starts("It costs 3.5 dollars").is_empty());
        assert!(starts("\"Stop!\" she says.").is_empty());
        assert!(starts("Ask Mr. Smith. Now").len() == 1);
        assert_eq!(starts("Ask Mr. Smith. Now"), vec!["Now"]);
        assert!(starts("J. Edgar arrives").is_empty());
        assert!(starts("The U.S. Army waits").is_empty());
        assert!(starts("Room No. 5 is empty").is_empty());
        assert_eq!(starts("Say no. Then go."), vec!["Then go."]);
        assert!(starts("Wait -- then go").is_empty());
    }

    #[test]
    fn ellipses() {
        assert!(starts("Well... Maybe.").is_empty());
        assert_eq!(starts("Well...? Maybe."), vec!["Maybe."]);
        assert!(starts("Well… Maybe.").is_empty());
        let s = "Well... Maybe.";
        assert_eq!(sentence_starts(s, true).len(), 1);
    }

    #[test]
    fn caseless_scripts_pass() {
        assert_eq!(starts("走。 跑"), Vec::<&str>::new());
        assert_eq!(starts("走. 跑"), vec!["跑"]);
    }
}
