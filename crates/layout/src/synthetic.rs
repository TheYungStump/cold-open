//! A deterministic feature-length script of our own, for speed and memory
//! budgets (docs/spec.md, "Platforms, size and performance"). It has the
//! density of a real screenplay: action of one to five lines, exchanges of
//! two to eight speeches of one to six lines, some parentheticals, voice-over
//! and off-screen extensions, the odd dual dialogue and regular transitions.
//! It lays out to about 120 US Letter pages with the default format.
//!
//! The words are our own list; no real script is used. The generator is a
//! fixed linear congruential sequence, so every OS gets the same script.

use coldopen_screenplay::{Element, ElementKind, Script, TitleField, TitlePage};

const WORDS: &[&str] = &[
    "the", "a", "and", "of", "to", "in", "on", "with", "for", "at", "her", "his", "their", "light",
    "door", "window", "rain", "street", "room", "car", "table", "phone", "glass", "hand", "face",
    "voice", "night", "morning", "wall", "floor", "stairs", "radio", "letter", "coat", "chair",
    "lamp", "kitchen", "harbor", "station", "office", "garden", "bridge", "slowly", "again",
    "still", "almost", "quietly", "suddenly", "outside", "inside", "away", "looks", "turns",
    "waits", "listens", "opens", "closes", "watches", "reaches", "steps", "pulls", "drops",
    "holds", "crosses", "leans", "smiles", "stops", "breathes", "counts", "old", "cold", "empty",
    "narrow", "bright", "wet", "small", "heavy", "broken", "distant",
];

const LINES: &[&str] = &[
    "I told you this would happen.",
    "We don't have time for this.",
    "Where were you last night?",
    "You think I don't know that?",
    "Then why did you come back?",
    "It was never about the money.",
    "Listen to me. Just this once.",
    "I kept the letter. All of them.",
    "Nobody leaves until the rain stops.",
    "You should have called first.",
    "That's not what I asked you.",
    "Maybe we were both wrong.",
    "The train leaves at six. Be on it.",
    "I can't do this without you.",
    "Say it again. Slowly.",
    "Are you sure that's the house?",
];

const NAMES: &[&str] = &[
    "MAYA",
    "THEO",
    "NINA",
    "DR. IVES",
    "RUTH",
    "SAMUEL",
    "OFFICER BELL",
    "JUNE",
];

const PLACES: &[&str] = &[
    "KITCHEN",
    "HARBOR",
    "TRAIN STATION",
    "MAYA'S APARTMENT",
    "POLICE STATION",
    "ROOFTOP",
    "DINER",
    "LIGHTHOUSE",
    "HOSPITAL CORRIDOR",
    "PARKING GARAGE",
    "BRIDGE",
    "RADIO ROOM",
];

const TIMES: &[&str] = &["DAY", "NIGHT", "MORNING", "DUSK", "CONTINUOUS", "LATER"];

const PARENS: &[&str] = &[
    "(beat)",
    "(quietly)",
    "(re: the letter)",
    "(not looking up)",
];

const TRANSITIONS: &[&str] = &["CUT TO:", "DISSOLVE TO:", "SMASH CUT TO:"];

/// A fixed pseudo-random sequence (Knuth's MMIX constants).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

/// An action paragraph of about `lines` lines of 60 cells.
fn action(r: &mut Lcg, lines: usize) -> String {
    let mut s = String::new();
    let mut sentence_words = 0;
    let target = lines * 60;
    while s.len() < target {
        let mut w = r.pick(WORDS).to_string();
        if sentence_words == 0 {
            let mut cs = w.chars();
            if let Some(f) = cs.next() {
                w = f.to_uppercase().collect::<String>() + cs.as_str();
            }
        }
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(&w);
        sentence_words += 1;
        if sentence_words >= r.range(5, 12) {
            s.push('.');
            sentence_words = 0;
        }
    }
    if !s.ends_with('.') {
        s.push('.');
    }
    s
}

/// A speech of about `lines` lines of 35 cells.
fn speech(r: &mut Lcg, lines: usize) -> String {
    let mut s = String::new();
    while s.len() < lines * 33 {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(r.pick(LINES));
    }
    s
}

/// The script for `seed`. Seed 1 gives about 120 pages.
pub fn feature_script(seed: u64) -> Script {
    let mut r = Lcg(seed);
    let mut e: Vec<Element> = Vec::new();
    for scene in 0..117 {
        let prefix = if r.below(3) == 0 { "EXT." } else { "INT." };
        e.push(Element::new(
            ElementKind::SceneHeading,
            format!("{prefix} {} - {}", r.pick(PLACES), r.pick(TIMES)),
        ));
        for _ in 0..r.range(1, 2) {
            let n = r.range(1, 5);
            e.push(Element::new(ElementKind::Action, action(&mut r, n)));
        }
        let speakers = [r.pick(NAMES), r.pick(NAMES)];
        let exchanges = r.range(1, 2);
        for x in 0..exchanges {
            for k in 0..r.range(2, 8) {
                let mut name = speakers[k % 2].to_string();
                match r.below(14) {
                    0 => name.push_str(" (V.O.)"),
                    1 => name.push_str(" (O.S.)"),
                    _ => {}
                }
                e.push(Element::new(ElementKind::Character, name));
                if r.below(10) == 0 {
                    e.push(Element::new(ElementKind::Parenthetical, r.pick(PARENS)));
                }
                let n = r.range(1, 6).min(r.range(1, 6));
                e.push(Element::new(ElementKind::Dialogue, speech(&mut r, n)));
            }
            if x + 1 < exchanges || r.below(2) == 0 {
                let n = r.range(1, 3);
                e.push(Element::new(ElementKind::Action, action(&mut r, n)));
            }
        }
        if scene % 10 == 7 {
            // A dual-dialogue beat.
            e.push(Element::new(ElementKind::Character, speakers[0]));
            e.push(Element::new(ElementKind::Dialogue, "Now?"));
            e.push(Element::new(ElementKind::Character, speakers[1]).dual());
            e.push(Element::new(ElementKind::Dialogue, "Not yet."));
        }
        if r.below(4) == 0 {
            e.push(Element::new(ElementKind::Transition, r.pick(TRANSITIONS)));
        }
    }
    Script {
        title_page: TitlePage {
            fields: vec![
                TitleField::new("Title", vec!["Low Tide".into()]),
                TitleField::new("Credit", vec!["written by".into()]),
                TitleField::new("Author", vec!["The Cold Open test suite".into()]),
            ],
        },
        elements: e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, ValidFormat, layout};

    #[test]
    fn about_120_pages_and_the_same_every_time() {
        let s = feature_script(1);
        assert_eq!(s, feature_script(1));
        let pages = layout(&s, &ValidFormat::default(), &Options::default())
            .pages
            .len();
        assert!((118..=122).contains(&pages), "{pages} pages");
        let speeches = s
            .elements
            .iter()
            .filter(|e| e.kind == ElementKind::Dialogue)
            .count();
        let action = s
            .elements
            .iter()
            .filter(|e| e.kind == ElementKind::Action)
            .count();
        assert!(
            speeches > 500 && action > 150,
            "{speeches} speeches, {action} action"
        );
    }
}
