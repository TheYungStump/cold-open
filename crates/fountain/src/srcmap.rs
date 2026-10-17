//! Where the normal form's text came from (spec §10, "Source map").
//!
//! Normalization only removes whitespace (trimmed line edges, empty action
//! lines, hidden-only lines), widens tabs in action to four spaces, and
//! joins merged elements with `\n`. Every other character is copied in
//! order. So the map is built by walking the normal form's printed text and
//! the source elements' printed text side by side, rather than by threading
//! offsets through each rewrite.

use std::ops::Range;

use coldopen_screenplay::Script;

use crate::norm::normalize_with_map;

/// A byte range of a normal-form element's printed text, copied from a
/// source element's printed text starting at byte `at`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub norm: Range<u32>,
    pub source: usize,
    pub at: u32,
}

/// The source of one normal-form element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ElementSource {
    /// The source element it starts in.
    pub source: usize,
    pub segments: Vec<Segment>,
}

/// The normal form's source map: one entry per normal-form element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    pub elements: Vec<ElementSource>,
}

impl SourceMap {
    /// The map of a script laid out as it is, without normalizing: each
    /// element comes whole from itself. Used for documents whose format is
    /// laid out unnormalized, such as .fdx (docs/specs/layout.md §4.8).
    pub fn identity(script: &Script) -> SourceMap {
        let elements = script
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let len = e.text.plain_text().len() as u32;
                ElementSource {
                    source: i,
                    segments: if len == 0 {
                        Vec::new()
                    } else {
                        vec![Segment {
                            norm: 0..len,
                            source: i,
                            at: 0,
                        }]
                    },
                }
            })
            .collect();
        SourceMap { elements }
    }

    /// The source element and byte offset for a byte offset in a
    /// normal-form element. A position where one segment ends and the next
    /// begins belongs to the next one. A position the normal form added (a
    /// joining `\n`) maps to the end of the text before it.
    pub fn to_source(&self, element: usize, byte: u32) -> Option<(usize, u32)> {
        let e = self.elements.get(element)?;
        let mut best = None;
        for s in &e.segments {
            if byte < s.norm.start {
                break;
            }
            if byte < s.norm.end {
                return Some((s.source, s.at + (byte - s.norm.start)));
            }
            best = Some((s.source, s.at + (s.norm.end - s.norm.start)));
        }
        best.or(Some((e.source, 0)))
    }

    /// The normal-form element and byte offset where a source position
    /// shows. A position in removed whitespace maps to the end of the
    /// copied text before it in the same source element, or to the start
    /// of the first text after it.
    pub fn to_norm(&self, source: usize, byte: u32) -> Option<(usize, u32)> {
        let mut before: Option<(usize, u32)> = None;
        let mut after: Option<(usize, u32)> = None;
        for (k, e) in self.elements.iter().enumerate() {
            for s in &e.segments {
                if s.source != source {
                    continue;
                }
                let len = s.norm.end - s.norm.start;
                if byte >= s.at && byte < s.at + len {
                    return Some((k, s.norm.start + (byte - s.at)));
                }
                if s.at + len <= byte {
                    // The last segment ending at or before the position;
                    // for a widened tab, that is its last space.
                    before = Some((k, s.norm.end));
                } else if after.is_none() {
                    after = Some((k, s.norm.start));
                }
            }
        }
        if before.is_some() {
            return before;
        }
        if after.is_some() {
            return after;
        }
        // A source element with no printed text in any segment, such as an
        // empty one: its first normal-form element, at its start.
        self.elements
            .iter()
            .position(|e| e.source == source)
            .map(|k| (k, 0))
    }
}

/// The normal form with its source map (spec §10). The element indices
/// equal [`normalize_with_map`]'s.
pub fn normalize_with_source_map(script: &Script) -> (Script, SourceMap) {
    let (norm, map) = normalize_with_map(script);
    let sources: Vec<String> = script
        .elements
        .iter()
        .map(|e| e.text.plain_text())
        .collect();
    let mut cur = Cursor {
        block: 0,
        byte: 0,
        tab_left: 0,
    };
    let mut elements = Vec::with_capacity(norm.elements.len());
    for (k, e) in norm.elements.iter().enumerate() {
        let start = map.get(k).copied().unwrap_or(0);
        if start != cur.block {
            cur = Cursor {
                block: start,
                byte: 0,
                tab_left: 0,
            };
        }
        // A merged element can take text from the source element the next
        // one starts in (the first part of a split action), so alignment
        // may reach into it; the shared cursor stops it taking more than
        // it shows.
        let last = map.get(k + 1).map_or(sources.len(), |&n| n + 1);
        let text = e.text.plain_text();
        elements.push(ElementSource {
            source: start,
            segments: align(&text, &sources, &mut cur, last.max(start + 1)),
        });
    }
    (norm, SourceMap { elements })
}

struct Cursor {
    block: usize,
    byte: usize,
    /// Spaces still to come from a widened tab at `byte`.
    tab_left: u8,
}

/// Adds a copied range, extending the last segment when it continues it.
/// A widened tab's spaces each get a segment of their own (`sealed`), and
/// nothing extends them, since their bytes are not a copy.
fn push(segs: &mut Vec<Segment>, sealed: &mut bool, norm: Range<u32>, source: usize, at: u32) {
    if !*sealed
        && let Some(last) = segs.last_mut()
        && last.source == source
        && last.norm.end == norm.start
        && last.at + (last.norm.end - last.norm.start) == at
    {
        last.norm.end = norm.end;
        return;
    }
    *sealed = false;
    segs.push(Segment { norm, source, at });
}

/// Aligns one normal-form element's text with the source text from the
/// cursor on, never going past source element `end` (exclusive).
fn align(text: &str, sources: &[String], cur: &mut Cursor, end: usize) -> Vec<Segment> {
    let mut segs = Vec::new();
    let mut sealed = false;
    for (nb, c) in text.char_indices() {
        let nb = nb as u32;
        let nr = nb..nb + c.len_utf8() as u32;
        loop {
            if cur.block >= end.min(sources.len()) {
                break; // nothing left to copy from: unmapped
            }
            let src = &sources[cur.block];
            let Some(s) = src[cur.byte..].chars().next() else {
                // End of this source element: a merge joins the next one
                // with a `\n` that has no source.
                if cur.block + 1 < end.min(sources.len()) {
                    cur.block += 1;
                    cur.byte = 0;
                    cur.tab_left = 0;
                    if c == '\n' {
                        break;
                    }
                    continue;
                }
                break;
            };
            if s == '\t' && c == ' ' {
                if cur.tab_left == 0 {
                    cur.tab_left = 4;
                }
                // Each widened space maps to the tab itself.
                sealed = true;
                push(
                    &mut segs,
                    &mut sealed,
                    nr.clone(),
                    cur.block,
                    cur.byte as u32,
                );
                sealed = true;
                cur.tab_left -= 1;
                if cur.tab_left == 0 {
                    cur.byte += 1;
                }
                break;
            }
            if s == c {
                push(
                    &mut segs,
                    &mut sealed,
                    nr.clone(),
                    cur.block,
                    cur.byte as u32,
                );
                cur.byte += s.len_utf8();
                cur.tab_left = 0;
                break;
            }
            if s.is_whitespace() {
                // Removed by normalization.
                cur.byte += s.len_utf8();
                cur.tab_left = 0;
                continue;
            }
            // Text the normal form added: no source.
            break;
        }
    }
    segs
}

#[cfg(test)]
mod tests {
    use super::*;
    use coldopen_screenplay::{Element, ElementKind};

    #[test]
    fn the_identity_map_maps_every_element_to_itself() {
        let s = script(vec![
            Element::new(ElementKind::Action, "  Rain.  "),
            Element::new(ElementKind::Dialogue, ""),
            Element::new(ElementKind::Dialogue, "Caf\u{e9}."),
        ]);
        let m = SourceMap::identity(&s);
        assert_eq!(m.elements.len(), 3);
        assert_eq!(m.to_source(0, 2), Some((0, 2)));
        assert_eq!(m.to_norm(0, 8), Some((0, 8)));
        assert!(m.elements[1].segments.is_empty());
        assert_eq!(m.to_source(1, 0), Some((1, 0)));
        assert_eq!(m.to_norm(2, 5), Some((2, 5)));
    }

    fn script(elements: Vec<Element>) -> Script {
        Script {
            elements,
            ..Default::default()
        }
    }

    /// Every segment copies its bytes, except widened tabs.
    fn check(s: &Script) -> SourceMap {
        let (norm, map) = normalize_with_source_map(s);
        assert_eq!(norm.elements.len(), map.elements.len());
        for (e, m) in norm.elements.iter().zip(&map.elements) {
            let t = e.text.plain_text();
            for seg in &m.segments {
                let src = s.elements[seg.source].text.plain_text();
                let n = &t[seg.norm.start as usize..seg.norm.end as usize];
                let len = seg.norm.end - seg.norm.start;
                let o = &src[seg.at as usize..(seg.at + len).min(src.len() as u32) as usize];
                assert!(
                    n == o || (n == " " && o.starts_with('\t')),
                    "{n:?} vs {o:?}"
                );
            }
        }
        map
    }

    #[test]
    fn trims_map_around_removed_space() {
        let s = script(vec![Element::new(ElementKind::Dialogue, "  Hello  ")]);
        let m = check(&s);
        assert_eq!(
            m.elements[0].segments,
            vec![Segment {
                norm: 0..5,
                source: 0,
                at: 2
            }]
        );
        assert_eq!(m.to_source(0, 3), Some((0, 5)));
        // The trailing spaces show at the end of the text.
        assert_eq!(m.to_norm(0, 9), Some((0, 5)));
        assert_eq!(m.to_norm(0, 0), Some((0, 0)));
    }

    #[test]
    fn merged_dialogue_maps_to_both_blocks() {
        let s = script(vec![
            Element::new(ElementKind::Character, "BOB"),
            Element::new(ElementKind::Dialogue, "One."),
            Element::new(ElementKind::Dialogue, "Two."),
        ]);
        let m = check(&s);
        assert_eq!(m.elements.len(), 2);
        assert_eq!(
            m.elements[1].segments,
            vec![
                Segment {
                    norm: 0..4,
                    source: 1,
                    at: 0
                },
                Segment {
                    norm: 5..9,
                    source: 2,
                    at: 0
                },
            ]
        );
        assert_eq!(m.to_norm(2, 2), Some((1, 7)));
        assert_eq!(m.to_source(1, 7), Some((2, 2)));
        // The joining newline belongs to the end of the first block.
        assert_eq!(m.to_source(1, 4), Some((1, 4)));
    }

    #[test]
    fn split_action_and_tabs() {
        let s = script(vec![
            Element::new(ElementKind::Action, "One.\n\nTwo\tthree."),
            Element::new(ElementKind::Action, "x"),
        ]);
        let (norm, _) = normalize_with_source_map(&s);
        assert_eq!(norm.elements[1].text.plain_text(), "Two    three.");
        let m = check(&s);
        assert_eq!(m.elements[1].source, 0);
        assert_eq!(m.to_source(1, 0), Some((0, 6)));
        // Each widened space maps to the tab; the text after it is one on.
        assert_eq!(m.to_source(1, 4), Some((0, 9)));
        assert_eq!(m.to_source(1, 7), Some((0, 10)));
        assert_eq!(m.to_norm(0, 10), Some((1, 7)));
        assert_eq!(m.to_norm(1, 0), Some((2, 0)));
    }

    #[test]
    fn empty_and_transparent_elements() {
        let s = script(vec![
            Element::new(ElementKind::Action, ""),
            Element::new(ElementKind::Note, " keep me "),
            Element::new(ElementKind::Dialogue, "x"),
        ]);
        let m = check(&s);
        assert_eq!(m.to_norm(0, 0), Some((0, 0)));
        assert_eq!(m.to_norm(1, 1), Some((1, 1)));
    }
}
