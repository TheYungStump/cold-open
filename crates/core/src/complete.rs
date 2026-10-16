//! Autocomplete suggestions, the `text.complete` query
//! (docs/specs/editor.md §6).

use std::collections::BTreeMap;

use coldopen_screenplay::ElementKind;
use serde::Serialize;

use crate::document::Document;

/// One suggestion: the text that replaces the range, and how often the
/// script already uses it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Item {
    pub text: String,
    pub count: usize,
}

/// Suggestions for finishing the text before an offset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Completion {
    pub context: Option<&'static str>,
    /// Character offsets of the range an item replaces.
    pub from: usize,
    pub to: usize,
    pub items: Vec<Item>,
}

const EXTENSIONS: [&str; 4] = ["V.O.", "O.S.", "O.C.", "CONT'D"];
const PREFIXES: [&str; 5] = ["INT.", "EXT.", "INT./EXT.", "EST.", "I/E."];
const TIMES: [&str; 11] = [
    "DAY",
    "NIGHT",
    "MORNING",
    "AFTERNOON",
    "EVENING",
    "DAWN",
    "DUSK",
    "LATER",
    "CONTINUOUS",
    "MOMENTS LATER",
    "SAME",
];
const TRANSITIONS: [&str; 6] = [
    "CUT TO:",
    "DISSOLVE TO:",
    "SMASH CUT TO:",
    "MATCH CUT TO:",
    "FADE OUT.",
    "FADE TO BLACK.",
];

fn upper(s: &str) -> String {
    s.to_uppercase()
}

/// Counts of each value in script order: (value, count, first, last).
#[derive(Default)]
struct Tally {
    seen: BTreeMap<String, (usize, usize, usize)>,
}

impl Tally {
    fn add(&mut self, value: String, pos: usize) {
        if value.is_empty() {
            return;
        }
        let e = self.seen.entry(value).or_insert((0, pos, pos));
        e.0 += 1;
        e.2 = pos;
    }

    fn count(&self, v: &str) -> usize {
        self.seen.get(v).map_or(0, |e| e.0)
    }
}

/// Builds the item list: candidates that start with `typed` (ignoring
/// case), in the order given, without duplicates. Nothing when the only
/// match is exactly what is already typed.
fn finish(typed: &str, ordered: Vec<(String, usize)>, suffix: &str) -> Vec<Item> {
    let t = upper(typed);
    let mut out: Vec<Item> = Vec::new();
    for (value, count) in ordered {
        if !value.starts_with(&t) || out.iter().any(|i| i.text == format!("{value}{suffix}")) {
            continue;
        }
        out.push(Item {
            text: format!("{value}{suffix}"),
            count,
        });
    }
    if out.len() == 1 && out[0].text == format!("{t}{suffix}") {
        out.clear();
    }
    out
}

/// Built-in values first by use, then the rest of the script's values,
/// most used first: the order for extensions, times and transitions.
fn by_use_then_builtin(builtin: &[&str], tally: &Tally) -> Vec<(String, usize)> {
    let mut list: Vec<(String, usize, usize, usize)> = Vec::new();
    for (k, b) in builtin.iter().enumerate() {
        let c = tally.count(b);
        list.push((b.to_string(), c, k, 0));
    }
    for (v, &(count, first, _)) in &tally.seen {
        if !builtin.contains(&v.as_str()) {
            list.push((v.clone(), count, builtin.len(), first));
        }
    }
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
    list.into_iter().map(|(v, c, _, _)| (v, c)).collect()
}

/// The text of a character's extension, inside its first brackets.
fn extension_of(text: &str) -> Option<String> {
    let open = text.find('(')?;
    let rest = &text[open + 1..];
    let close = rest.find(')').unwrap_or(rest.len());
    Some(upper(rest[..close].trim()))
}

/// A scene heading's location and time: the text after the first space, up
/// to and after the last " - ".
fn location_and_time(text: &str) -> (Option<String>, Option<String>) {
    let Some(sp) = text.find(' ') else {
        return (None, None);
    };
    let rest = &text[sp + 1..];
    match rest.rfind(" - ") {
        Some(d) => (
            Some(upper(rest[..d].trim())),
            Some(upper(rest[d + 3..].trim())),
        ),
        None => (Some(upper(rest.trim())), None),
    }
}

fn char_index(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

/// Suggestions for block `index` at character offset `at`.
pub fn complete(doc: &Document, index: usize, at: usize) -> Completion {
    let Some(block) = doc.blocks.get(index) else {
        return Completion::default();
    };
    let text = block.element.text.plain_text();
    let byte = text.char_indices().nth(at).map_or(text.len(), |(b, _)| b);
    let before = &text[..byte];
    let others = || {
        doc.blocks
            .iter()
            .enumerate()
            .filter(move |(i, _)| *i != index)
    };
    match block.element.kind {
        ElementKind::Character => {
            if let Some(open) = before.rfind('(') {
                let from_byte = open + 1;
                let typed = before[from_byte..].trim_start();
                let from = char_index(before, before.len() - typed.len());
                let mut tally = Tally::default();
                for (i, b) in others() {
                    if b.element.kind == ElementKind::Character
                        && let Some(e) = extension_of(&b.element.text.plain_text())
                    {
                        tally.add(e, i);
                    }
                }
                let closes = text[byte..].starts_with(')');
                let suffix = if closes { "" } else { ")" };
                let items = finish(typed, by_use_then_builtin(&EXTENSIONS, &tally), suffix);
                return Completion {
                    context: Some("extension"),
                    from,
                    to: at,
                    items,
                };
            }
            let typed = before.trim_start();
            let from = char_index(before, before.len() - typed.len());
            let items = if typed.trim().is_empty() {
                Vec::new()
            } else {
                finish(typed, character_order(doc, index), "")
            };
            Completion {
                context: Some("character"),
                from,
                to: at,
                items,
            }
        }
        ElementKind::SceneHeading => {
            let Some(sp) = before.find(' ') else {
                let items = if before.is_empty() {
                    Vec::new()
                } else {
                    let ordered = PREFIXES.iter().map(|p| (p.to_string(), 0)).collect();
                    finish(before, ordered, " ")
                };
                return Completion {
                    context: Some("scene_prefix"),
                    from: 0,
                    to: at,
                    items,
                };
            };
            let rest = &before[sp + 1..];
            if let Some(d) = rest.rfind(" - ") {
                let start = sp + 1 + d + 3;
                let typed = before[start..].trim_start();
                let from = char_index(before, before.len() - typed.len());
                let mut tally = Tally::default();
                for (i, b) in others() {
                    if b.element.kind == ElementKind::SceneHeading
                        && let (_, Some(t)) = location_and_time(&b.element.text.plain_text())
                    {
                        tally.add(t, i);
                    }
                }
                let items = finish(typed, by_use_then_builtin(&TIMES, &tally), "");
                return Completion {
                    context: Some("time"),
                    from,
                    to: at,
                    items,
                };
            }
            let typed = rest.trim_start();
            let from = char_index(before, before.len() - typed.len());
            let mut tally = Tally::default();
            for (i, b) in others() {
                if b.element.kind == ElementKind::SceneHeading
                    && let (Some(l), _) = location_and_time(&b.element.text.plain_text())
                {
                    tally.add(l, i);
                }
            }
            let mut list: Vec<(String, usize, usize)> = tally
                .seen
                .iter()
                .map(|(v, &(c, _, last))| (v.clone(), c, last))
                .collect();
            list.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
            let items = if typed.is_empty() {
                Vec::new()
            } else {
                finish(
                    typed,
                    list.into_iter().map(|(v, c, _)| (v, c)).collect(),
                    "",
                )
            };
            Completion {
                context: Some("location"),
                from,
                to: at,
                items,
            }
        }
        ElementKind::Transition => {
            let typed = before.trim_start();
            let from = char_index(before, before.len() - typed.len());
            let mut tally = Tally::default();
            for (i, b) in others() {
                if b.element.kind == ElementKind::Transition {
                    tally.add(upper(b.element.text.plain_text().trim()), i);
                }
            }
            let items = if typed.is_empty() {
                Vec::new()
            } else {
                finish(typed, by_use_then_builtin(&TRANSITIONS, &tally), "")
            };
            Completion {
                context: Some("transition"),
                from,
                to: at,
                items,
            }
        }
        _ => Completion {
            context: None,
            from: at,
            to: at,
            items: Vec::new(),
        },
    }
}

/// Character names in suggestion order (editor.md §6, `character`).
fn character_order(doc: &Document, index: usize) -> Vec<(String, usize)> {
    let name_of = |i: usize| {
        doc.blocks[i]
            .element
            .character_name()
            .map(|n| upper(&n))
            .filter(|n| !n.is_empty())
    };
    let mut tally = Tally::default();
    for i in 0..doc.blocks.len() {
        if i != index
            && let Some(n) = name_of(i)
        {
            tally.add(n, i);
        }
    }
    // Speakers in the current scene before the caret's block, most recent
    // first, without repeats.
    let scene_start = (0..index)
        .rev()
        .find(|&i| doc.blocks[i].element.kind == ElementKind::SceneHeading)
        .unwrap_or(0);
    let mut recent: Vec<String> = Vec::new();
    for i in (scene_start..index).rev() {
        if let Some(n) = name_of(i)
            && !recent.contains(&n)
        {
            recent.push(n);
        }
    }
    let mut ordered: Vec<String> = Vec::new();
    if recent.len() >= 2 {
        ordered.push(recent[1].clone());
    }
    for n in &recent {
        if !ordered.contains(n) {
            ordered.push(n.clone());
        }
    }
    let mut rest: Vec<(&String, usize, usize)> = tally
        .seen
        .iter()
        .filter(|(n, _)| !ordered.contains(n))
        .map(|(n, &(c, first, _))| (n, c, first))
        .collect();
    rest.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    ordered.extend(rest.into_iter().map(|(n, _, _)| n.clone()));
    ordered
        .into_iter()
        .map(|n| {
            let c = tally.count(&n);
            (n, c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use coldopen_screenplay::{Element, Script};

    fn doc(elements: Vec<(ElementKind, &str)>) -> Document {
        Document::from_script(Script {
            elements: elements
                .into_iter()
                .map(|(k, t)| Element::new(k, t))
                .collect(),
            ..Default::default()
        })
    }

    fn texts(c: &Completion) -> Vec<&str> {
        c.items.iter().map(|i| i.text.as_str()).collect()
    }

    use ElementKind::*;

    #[test]
    fn character_names_follow_the_conversation() {
        let d = doc(vec![
            (Character, "ZED"),
            (Dialogue, "x"),
            (SceneHeading, "INT. LAB - DAY"),
            (Character, "MAYA"),
            (Dialogue, "x"),
            (Character, "THEO"),
            (Dialogue, "x"),
            (Character, "M"),
        ]);
        let c = complete(&d, 7, 1);
        assert_eq!(c.context, Some("character"));
        assert_eq!(texts(&c), vec!["MAYA"]);
        let empty = doc(vec![
            (SceneHeading, "INT. LAB - DAY"),
            (Character, "MAYA"),
            (Character, "THEO"),
            (Character, "ZED"),
            (Character, "ZED"),
            (Character, "T"),
        ]);
        // Before the previous speaker (ZED) comes THEO.
        let c = complete(&empty, 5, 1);
        assert_eq!(texts(&c), vec!["THEO"]);
        let c = complete(&empty, 5, 0);
        assert!(c.items.is_empty());
    }

    #[test]
    fn character_ranking_puts_the_other_speaker_first() {
        let d = doc(vec![
            (SceneHeading, "INT. LAB - DAY"),
            (Character, "MAYA"),
            (Character, "MARK"),
            (Character, "MA"),
        ]);
        let c = complete(&d, 3, 2);
        assert_eq!(texts(&c), vec!["MAYA", "MARK"]);
    }

    #[test]
    fn extensions_close_the_bracket() {
        let d = doc(vec![(Character, "MAYA (V.O.)"), (Character, "THEO (o")]);
        let c = complete(&d, 1, 7);
        assert_eq!(c.context, Some("extension"));
        assert_eq!((c.from, c.to), (6, 7));
        assert_eq!(texts(&c), vec!["O.S.)", "O.C.)"]);
        let c = complete(&d, 1, 6);
        assert_eq!(texts(&c)[0], "V.O.)");
    }

    #[test]
    fn scene_headings_complete_in_three_parts() {
        let d = doc(vec![
            (SceneHeading, "INT. LAB - NIGHT"),
            (SceneHeading, "EXT. LAB - DUSK"),
            (SceneHeading, "INT. LIGHTHOUSE - NIGHT"),
            (SceneHeading, "in"),
        ]);
        let c = complete(&d, 3, 2);
        assert_eq!(c.context, Some("scene_prefix"));
        assert_eq!(texts(&c), vec!["INT. ", "INT./EXT. "]);

        let d = doc(vec![
            (SceneHeading, "INT. LAB - NIGHT"),
            (SceneHeading, "EXT. LAB - DUSK"),
            (SceneHeading, "INT. LIGHTHOUSE - NIGHT"),
            (SceneHeading, "INT. L"),
        ]);
        let c = complete(&d, 3, 6);
        assert_eq!(c.context, Some("location"));
        assert_eq!((c.from, c.to), (5, 6));
        assert_eq!(texts(&c), vec!["LAB", "LIGHTHOUSE"]);

        let d = doc(vec![
            (SceneHeading, "INT. LAB - NIGHT"),
            (SceneHeading, "EXT. LAB - DUSK"),
            (SceneHeading, "INT. LAB - NIGHT"),
            (SceneHeading, "INT. LAB - "),
        ]);
        let c = complete(&d, 3, 11);
        assert_eq!(c.context, Some("time"));
        assert_eq!(texts(&c)[..3], ["NIGHT", "DUSK", "DAY"]);
    }

    #[test]
    fn transitions_and_other_kinds() {
        let d = doc(vec![
            (Transition, "SMASH CUT TO:"),
            (Transition, "s"),
            (Action, "x"),
        ]);
        let c = complete(&d, 1, 1);
        assert_eq!(texts(&c), vec!["SMASH CUT TO:"]);
        let c = complete(&d, 2, 1);
        assert_eq!(c.context, None);
        assert!(c.items.is_empty());
    }

    #[test]
    fn an_exact_match_alone_hides_the_list() {
        let d = doc(vec![(Character, "MAYA"), (Character, "MAYA")]);
        assert!(complete(&d, 1, 4).items.is_empty());
    }
}
