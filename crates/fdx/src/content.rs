//! `Content` paragraphs into a `Script` (docs/specs/fdx.md §6).

use std::collections::BTreeMap;
use std::ops::Range;

use coldopen_screenplay::{Element, ElementKind, Pt, RichText, Run, Script, Style};

use crate::SceneProps;
use crate::report::{Report, codes, plural};
use crate::settings::{Settings, Spec, describe_unknown_style};
use crate::values::{At, FdxAlign, color_is, kind_of_type, points_rounding, style_tokens, yes_no};
use crate::xml::Node;

pub(crate) struct ContentOut {
    pub(crate) script: Script,
    pub(crate) overrides: BTreeMap<usize, Pt>,
    pub(crate) scenes: Vec<SceneProps>,
    /// For each `Content` paragraph, in order, the elements it was read
    /// into, before trailing empty paragraphs were dropped (§13.1).
    pub(crate) groups: Vec<Range<usize>>,
}

struct Reader<'a> {
    settings: &'a Settings,
    r: &'a mut Report,
    elements: Vec<Element>,
    overrides: BTreeMap<usize, Pt>,
    scenes: Vec<SceneProps>,
}

pub(crate) fn read(root: &Node, settings: &Settings, r: &mut Report) -> ContentOut {
    let mut rd = Reader {
        settings,
        r,
        elements: Vec::new(),
        overrides: BTreeMap::new(),
        scenes: Vec::new(),
    };
    let mut groups = Vec::new();
    match root.child("Content") {
        None => rd.r.push(
            codes::NO_CONTENT,
            "the file has no Content element; the script is empty",
            None,
        ),
        Some(content) => {
            for n in content.elements().filter(|n| n.name != "Paragraph") {
                ignored_element(rd.r, "<Content>", &n.name, None);
            }
            for (i, p) in content.children_named("Paragraph").enumerate() {
                let before = rd.elements.len();
                if p.child("DualDialogue").is_some() {
                    rd.dual(i, p);
                } else {
                    rd.paragraph(i, p, true);
                }
                groups.push(before..rd.elements.len());
            }
            rd.drop_trailing_empty();
        }
    }
    ContentOut {
        script: Script {
            elements: rd.elements,
            ..Script::default()
        },
        overrides: rd.overrides,
        scenes: rd.scenes,
        groups,
    }
}

impl Reader<'_> {
    /// Inserts a page break unless nothing printed precedes it or a break
    /// is already there (docs/specs/fdx.md §6.5).
    fn page_break(&mut self) {
        match self.elements.last() {
            None => {}
            Some(e) if e.kind == ElementKind::PageBreak => {}
            Some(_) => self.elements.push(Element::new(ElementKind::PageBreak, "")),
        }
    }

    /// Reads one paragraph and returns the index of its element. `top` is
    /// false for paragraphs nested in dual dialogue.
    fn paragraph(&mut self, i: usize, p: &Node, top: bool) -> usize {
        let type_name = p.attr("Type").unwrap_or("");
        let modelled = kind_of_type(type_name);
        let kind = modelled.unwrap_or(ElementKind::General);
        if modelled.is_none() {
            let key = if p.attr("Type").is_some() {
                format!("\"{type_name}\"")
            } else {
                "(no Type)".to_string()
            };
            self.r
                .tally(codes::UNKNOWN_PARAGRAPH_TYPE, &key, Some(i), |key, n| {
                    format!(
                        "paragraph type {key} is not one we model; {} read as general text",
                        plural(n, "paragraph is", "paragraphs are")
                    )
                });
        }
        let allowed: &[&str] = &["Text", "SceneProperties", "Tabstops"];
        for n in p.elements().filter(|n| !allowed.contains(&n.name.as_str())) {
            ignored_element(self.r, "<Paragraph>", &n.name, Some(i));
        }
        let mut el = Element::new(kind, RichText::new(runs(p, Some(i), self.r)));

        if let Some(number) = p.attr("Number") {
            if kind == ElementKind::SceneHeading {
                el.scene_number = Some(number.trim().to_string());
                if !el.problems().is_empty() {
                    el.scene_number = None;
                    self.r.push(
                        codes::INVALID_SCENE_NUMBER,
                        format!("the scene number \"{number}\" is not valid and is dropped"),
                        Some(i),
                    );
                }
            } else {
                self.r.push(
                    codes::SCENE_NUMBER_IGNORED,
                    format!("this {kind} paragraph has a scene number, which is dropped"),
                    Some(i),
                );
            }
        }

        // Inline paragraph attributes against the baseline (§6.4).
        let inline = Spec::parse(p, "paragraph", Some(i), self.r);
        let base = self.settings.baseline(type_name, kind);
        let mut ignored: Vec<&str> = Vec::new();
        if let Some(a) = inline.align {
            let b = base.align.unwrap_or(FdxAlign::Left);
            if a.folded() != b.folded() {
                if kind == ElementKind::Action && a == FdxAlign::Center {
                    el.centered = true;
                } else {
                    ignored.push("Alignment");
                }
            }
        }
        // An override is held to the bound a type's own spacing has: from 0
        // to the body height of the format in use (§6.4).
        let body = self.settings.format.body_height();
        let space_override = match inline.space_before {
            Some(sb) if Some(sb) != base.space_before => {
                if (Pt::ZERO..=body).contains(&sb) {
                    Some(sb)
                } else {
                    self.r.push(
                        codes::INVALID_VALUE,
                        format!(
                            "this paragraph's own SpaceBefore of {sb} pt is outside 0 to {body} pt, the body height; its type's spacing is used"
                        ),
                        Some(i),
                    );
                    None
                }
            }
            _ => None,
        };
        if inline.first_indent.is_some() && inline.first_indent != base.first_indent {
            ignored.push("FirstIndent");
        }
        if let Some(l) = &inline.leading
            && !base
                .leading
                .as_deref()
                .is_some_and(|b| b.eq_ignore_ascii_case(l))
        {
            ignored.push("Leading");
        }
        if inline.left.is_some() && inline.left != base.left {
            ignored.push("LeftIndent");
        }
        if inline.right.is_some() && inline.right != base.right {
            ignored.push("RightIndent");
        }
        if inline.spacing.is_some() && inline.spacing != base.spacing {
            ignored.push("Spacing");
        }
        let starts_page = if top {
            match inline.starts_new_page {
                Some(v) => {
                    if !v && modelled.is_some() && base.starts_new_page == Some(true) {
                        ignored.push("StartsNewPage");
                    }
                    v
                }
                // Resolution (§6.5): an unmodelled type falls back to its
                // own settings.
                None if modelled.is_none() => self
                    .settings
                    .spec_for(type_name)
                    .and_then(|s| s.starts_new_page)
                    .unwrap_or(false),
                None => false,
            }
        } else {
            if inline.starts_new_page == Some(true) {
                ignored.push("StartsNewPage");
            }
            false
        };

        if starts_page {
            self.page_break();
        }
        self.elements.push(el);
        let index = self.elements.len() - 1;
        if let Some(sb) = space_override {
            self.overrides.insert(index, sb);
        }

        let mut props = p.children_named("SceneProperties");
        if let Some(sp) = props.next() {
            if top && kind == ElementKind::SceneHeading {
                self.scenes.push(SceneProps {
                    element: index,
                    page: sp.attr("Page").map(str::to_string),
                    length: sp.attr("Length").map(str::to_string),
                    title: sp.attr("Title").map(str::to_string),
                });
            } else {
                self.r.push(
                    codes::SCENE_PROPERTIES_IGNORED,
                    format!("scene properties on this {kind} paragraph are ignored"),
                    Some(i),
                );
            }
        }
        for extra in props {
            ignored_element(self.r, "<Paragraph>", &extra.name, Some(i));
        }

        if !ignored.is_empty() {
            self.r.push(
                codes::PARAGRAPH_FORMAT_IGNORED,
                format!(
                    "this paragraph's own {} cannot be expressed and {} ignored",
                    ignored.join(", "),
                    if ignored.len() == 1 { "is" } else { "are" }
                ),
                Some(i),
            );
        }
        index
    }

    /// A paragraph holding dual dialogue (docs/specs/fdx.md §6.6).
    fn dual(&mut self, i: usize, p: &Node) {
        let before = self.elements.len();
        if let Some(v) = p.attr("StartsNewPage") {
            let at = At::para("paragraph StartsNewPage", i);
            if yes_no(v, &at, self.r) == Some(true) {
                self.page_break();
            }
        }
        let after_break = self.elements.len();
        let allowed: &[&str] = &["Text", "DualDialogue", "Tabstops"];
        for n in p.elements().filter(|n| !allowed.contains(&n.name.as_str())) {
            if n.name == "SceneProperties" {
                self.r.push(
                    codes::SCENE_PROPERTIES_IGNORED,
                    "scene properties on a dual-dialogue paragraph are ignored",
                    Some(i),
                );
            } else {
                ignored_element(self.r, "<Paragraph>", &n.name, Some(i));
            }
        }
        for dd in p.children_named("DualDialogue") {
            for n in dd.elements().filter(|n| n.name != "Paragraph") {
                ignored_element(self.r, "<DualDialogue>", &n.name, Some(i));
            }
            let nested: Vec<&Node> = dd.children_named("Paragraph").collect();
            let kinds: Vec<ElementKind> = nested
                .iter()
                .map(|n| {
                    n.attr("Type")
                        .and_then(kind_of_type)
                        .unwrap_or(ElementKind::General)
                })
                .collect();
            let second = second_speaker(&kinds);
            let mut second_index = None;
            for (j, n) in nested.iter().enumerate() {
                let index = self.paragraph(i, n, false);
                if second == Some(j) {
                    second_index = Some(index);
                }
            }
            match second_index {
                Some(index) => self.elements[index].dual = true,
                None => {
                    let shape = if nested.is_empty() {
                        "no paragraphs".to_string()
                    } else {
                        nested
                            .iter()
                            .map(|n| n.attr("Type").unwrap_or("(no Type)"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    self.r.push(
                        codes::DUAL_DIALOGUE_SHAPE,
                        format!(
                            "dual dialogue holds {shape}, not two speakers; it is read as ordinary paragraphs"
                        ),
                        Some(i),
                    );
                }
            }
        }
        let outer = RichText::new(runs(p, Some(i), self.r));
        if !outer.is_empty() {
            if outer.plain_text().chars().any(|c| !c.is_whitespace()) {
                self.elements
                    .push(Element::new(ElementKind::General, outer));
                self.r.push(
                    codes::DUAL_DIALOGUE_TEXT,
                    "the dual-dialogue paragraph has text of its own; it follows the dual block as general text",
                    Some(i),
                );
            } else {
                self.r.push(
                    codes::DUAL_DIALOGUE_TEXT,
                    "the dual-dialogue paragraph has whitespace of its own, which is dropped",
                    Some(i),
                );
            }
        }
        // A break with nothing after it belongs to no element.
        if self.elements.len() == after_break && after_break > before {
            self.elements.pop();
        }
    }

    fn drop_trailing_empty(&mut self) {
        let mut dropped = 0;
        loop {
            match self.elements.last() {
                Some(e) if e.kind == ElementKind::General && e.text.is_empty() => {
                    self.elements.pop();
                    dropped += 1;
                }
                Some(e) if e.kind == ElementKind::PageBreak && dropped > 0 => {
                    self.elements.pop();
                }
                _ => break,
            }
        }
        if dropped > 0 {
            let len = self.elements.len();
            self.overrides.retain(|&k, _| k < len);
            self.scenes.retain(|s| s.element < len);
            self.r.push(
                codes::TRAILING_EMPTY_DROPPED,
                format!(
                    "{} at the end of the script {} dropped",
                    plural(
                        dropped,
                        "empty general paragraph",
                        "empty general paragraphs"
                    ),
                    if dropped == 1 { "is" } else { "are" }
                ),
                None,
            );
        }
    }
}

/// The position of the second speaker's character when `kinds` is exactly
/// two dialogue groups (docs/specs/fdx.md §6.6).
fn second_speaker(kinds: &[ElementKind]) -> Option<usize> {
    if kinds.first() != Some(&ElementKind::Character) {
        return None;
    }
    let characters: Vec<usize> = kinds
        .iter()
        .enumerate()
        .filter(|(_, k)| **k == ElementKind::Character)
        .map(|(j, _)| j)
        .collect();
    let members_only = kinds
        .iter()
        .all(|k| *k == ElementKind::Character || k.is_dialogue_member());
    // Each speaker needs at least one line, as layout pairs them
    // (docs/specs/layout.md §4.4).
    match characters.as_slice() {
        [first, second] if members_only && second - first > 1 && second + 1 < kinds.len() => {
            Some(*second)
        }
        _ => None,
    }
}

pub(crate) fn ignored_element(r: &mut Report, parent: &str, name: &str, para: Option<usize>) {
    r.tally(
        codes::ELEMENT_IGNORED,
        &format!("<{name}> in {parent}"),
        para,
        |key, n| {
            format!(
                "{key} is not read ({})",
                plural(n, "occurrence", "occurrences")
            )
        },
    );
}

/// A run attribute we don't model: its name, report code, whether a value
/// is the harmless default, and what a different value means.
type RunCheck = (&'static str, &'static str, fn(&str) -> bool, &'static str);

/// The runs of a paragraph's `Text` children (docs/specs/fdx.md §6.2).
pub(crate) fn runs(p: &Node, para: Option<usize>, r: &mut Report) -> Vec<Run> {
    let mut out = Vec::new();
    for t in p.children_named("Text") {
        for n in t.elements() {
            ignored_element(r, "<Text>", &n.name, para);
        }
        let mut text = t.text();
        let mut style = Style::PLAIN;
        if let Some(raw) = t.attr("Style") {
            let tokens = style_tokens(raw);
            style = tokens.style;
            if tokens.all_caps {
                text = text.to_uppercase();
                r.tally(codes::RUN_ALL_CAPS, "", para, |_, n| {
                    format!(
                        "{} styled AllCaps {} upper-cased when read",
                        plural(n, "text run", "text runs"),
                        if n == 1 { "was" } else { "were" }
                    )
                });
            }
            for token in &tokens.unknown {
                r.tally(codes::UNKNOWN_STYLE, token, para, describe_unknown_style);
            }
        }
        let checks: [RunCheck; 6] = [
            (
                "Font",
                codes::RUN_FONT,
                |v| v.to_ascii_lowercase().contains("courier"),
                "a font other than Courier",
            ),
            (
                "Size",
                codes::RUN_SIZE,
                |v| points_rounding(v).map(|(p, _)| p) == Some(Pt::points(12)),
                "a size other than 12 pt",
            ),
            (
                "Color",
                codes::RUN_COLOR,
                |v| color_is(v, b'0'),
                "a color other than black",
            ),
            (
                "Background",
                codes::RUN_BACKGROUND,
                |v| color_is(v, b'F'),
                "a background other than white",
            ),
            (
                "AdornmentStyle",
                codes::RUN_ADORNMENT,
                |v| v.trim().parse::<i64>() == Ok(0),
                "an adornment style",
            ),
            (
                "RevisionID",
                codes::RUN_REVISION,
                |v| v.trim().parse::<i64>() == Ok(0),
                "a revision mark",
            ),
        ];
        for (key, code, ok, what) in checks {
            if let Some(v) = t.attr(key)
                && !ok(v)
            {
                r.tally(code, what, para, |what, n| {
                    format!(
                        "{} {what}, which is ignored",
                        plural(n, "text run has", "text runs have")
                    )
                });
            }
        }
        if text.contains('\t') {
            r.tally(codes::TAB_CHARACTER, "", para, |_, n| {
                format!(
                    "{} tab characters; they are kept, and layout does not print them",
                    plural(n, "text run holds", "text runs hold")
                )
            });
        }
        out.push(Run::styled(text, style));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ElementKind::*;

    #[test]
    fn second_speaker_needs_two_groups() {
        assert_eq!(
            second_speaker(&[Character, Dialogue, Character, Dialogue]),
            Some(2)
        );
        assert_eq!(
            second_speaker(&[Character, Parenthetical, Dialogue, Character, Lyric]),
            Some(3)
        );
        assert_eq!(second_speaker(&[Character, Dialogue]), None);
        assert_eq!(second_speaker(&[Dialogue, Character, Dialogue]), None);
        assert_eq!(
            second_speaker(&[Character, Dialogue, Character, Dialogue, Character]),
            None
        );
        assert_eq!(
            second_speaker(&[Character, Action, Character, Dialogue]),
            None
        );
        assert_eq!(second_speaker(&[]), None);
        // A speaker with no lines is not paired (layout.md §4.4).
        assert_eq!(second_speaker(&[Character, Character, Dialogue]), None);
        assert_eq!(second_speaker(&[Character, Dialogue, Character]), None);
    }
}
