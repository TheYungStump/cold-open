//! The passthrough store (docs/specs/fdx.md §13): everything in an .fdx file
//! that the document's blocks do not hold, kept so the file can be written
//! back as it was.

use std::collections::BTreeMap;
use std::ops::Range;

use coldopen_screenplay::{Element, PageFormat, Pt, TitleLine};
use serde::{Deserialize, Serialize};

use crate::write::{self, Blk};
use crate::xml::{Child, Node, Tree};

/// How the file spells its markup, so generated parts match it (§13.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct XmlStyle {
    /// The line break in markup: `"\n"` or `"\r\n"`.
    pub newline: String,
    /// One level of indentation.
    pub unit: String,
    /// How an apostrophe in text is written, when not as itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apostrophe: Option<String>,
    /// How a quotation mark in text is written, when not as itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    /// Whether `>` in text is written as `&gt;`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub gt: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Default for XmlStyle {
    fn default() -> XmlStyle {
        XmlStyle {
            newline: "\n".into(),
            unit: "  ".into(),
            apostrophe: None,
            quote: None,
            gt: false,
        }
    }
}

/// One block as the reader made it, for telling whether it was edited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptBlock {
    pub id: u64,
    pub element: Element,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<Pt>,
}

/// A paragraph group the generator does not reproduce exactly (§13.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptParagraph {
    /// Its text in the file, with the whitespace before it; or, when not
    /// verbatim, the tree the reader repaired, written out.
    pub text: String,
    /// The text can be written back as it is. False after a structural
    /// repair inside it: then it is always generated (§13.2).
    pub verbatim: bool,
    /// The blocks it was read into.
    pub blocks: Vec<KeptBlock>,
}

/// Something in `Content` that is not a paragraph, anchored to the block
/// before it (§13.4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loose {
    /// The id of the last block before it, or 0 for the start of `Content`.
    pub after: u64,
    pub text: String,
}

/// One child of the root, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    /// Written back as it is, with the whitespace before it.
    Text(String),
    /// Where `Content` goes.
    Content,
    /// The title page: its `Content` can be generated again from the
    /// document's title lines when they change (§13.5).
    TitlePage {
        head: String,
        content: String,
        tail: String,
    },
}

/// The passthrough store of a document read from .fdx (§13.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    pub style: XmlStyle,
    /// Everything before the root's start tag.
    pub prolog: String,
    pub root_open: String,
    /// The root was written `<FinalDraft/>`: `root_open` is all of it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub root_self_closing: bool,
    pub sections: Vec<Section>,
    /// From the end of the last section to the end of the file.
    pub root_close: String,
    /// The whitespace before `Content` and its start tag.
    pub content_open: String,
    /// The whitespace before `</Content>`, and the end tag.
    pub content_close: String,
    /// `Content` was written `<Content/>`: `content_open` is all of it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub content_self_closing: bool,
    /// Empty general paragraphs at the end, which the model leaves out.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trailing: String,
    /// Kept paragraph groups, by the id of their first block.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub paragraphs: BTreeMap<u64, KeptParagraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loose: Vec<Loose>,
    /// The title lines read from `TitlePage`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub title_lines: Vec<TitleLine>,
}

impl Kept {
    /// The store with its block ids renumbered by `map`, as when a document
    /// is read back into a session that has handed out ids already. Ids not
    /// in `map` keep their number.
    pub fn renumbered(mut self, map: &dyn Fn(u64) -> u64) -> Kept {
        self.paragraphs = std::mem::take(&mut self.paragraphs)
            .into_values()
            .map(|mut p| {
                for b in &mut p.blocks {
                    b.id = map(b.id);
                }
                (p.blocks.first().map_or(0, |b| b.id), p)
            })
            .collect();
        for l in &mut self.loose {
            if l.after != 0 {
                l.after = map(l.after);
            }
        }
        self
    }

    /// The names of the sections kept, for a report of what Fountain
    /// cannot hold.
    pub fn section_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for s in &self.sections {
            let text = match s {
                Section::Text(t) => t.as_str(),
                Section::TitlePage { .. } => "<TitlePage",
                Section::Content => continue,
            };
            if let Some(name) = text
                .trim_start()
                .strip_prefix('<')
                .and_then(|t| {
                    t.split(|c: char| c.is_whitespace() || c == '>' || c == '/')
                        .next()
                })
                .filter(|n| !n.is_empty() && !n.starts_with('!') && !n.starts_with('?'))
                && !names.iter().any(|x| x == name)
            {
                names.push(name.to_string());
            }
        }
        names
    }
}

/// Whether text outside the root holds only what XML allows there:
/// whitespace, comments, processing instructions and a doctype.
fn clean_misc(s: &str) -> bool {
    let mut rest = s;
    loop {
        rest = rest.trim_start_matches([' ', '\t', '\r', '\n']);
        if rest.is_empty() {
            return true;
        }
        let end = if rest.starts_with("<!--") {
            rest.find("-->").map(|k| k + 3)
        } else if rest.starts_with("<?") {
            rest.find("?>").map(|k| k + 2)
        } else if rest.starts_with("<!DOCTYPE") {
            rest.find('>').map(|k| k + 1)
        } else {
            None
        };
        match end {
            Some(k) => rest = &rest[k..],
            None => return false,
        }
    }
}

/// The XML declaration at the start of a prolog, alone.
fn declaration_only(prolog: &str, nl: &str) -> String {
    match prolog.find("?>") {
        Some(k) if prolog.starts_with("<?xml") => format!("{}{nl}", &prolog[..k + 2]),
        _ => String::new(),
    }
}

/// Text or markup as written, without characters XML does not allow,
/// which the reader reported (§9, `invalid_char`).
fn xml_text(text: &str, raw: &Range<usize>) -> String {
    text.get(raw.clone())
        .unwrap_or("")
        .chars()
        .filter(|&c| write::valid_xml_char(c))
        .collect()
}

/// Whether `tail` is whitespace and then the end tag of `name`, which may
/// have whitespace before its `>`.
fn ends_with_end_tag(tail: &str, name: &str) -> bool {
    let Some(lt) = tail.rfind('<') else {
        return false;
    };
    let Some(rest) = tail[lt..]
        .strip_prefix("</")
        .and_then(|r| r.strip_prefix(name))
    else {
        return false;
    };
    whitespace(&tail[..lt]) && rest.strip_suffix('>').is_some_and(whitespace)
}

fn whitespace(s: &str) -> bool {
    s.chars()
        .all(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n')
}

/// Where a child of an element starts and ends in the text.
fn child_span(c: &Child) -> Range<usize> {
    match c {
        Child::Element(n) => n.span.clone(),
        Child::Text { raw, .. } | Child::Other(raw) => raw.clone(),
    }
}

/// The file's markup style, from `Content`'s first paragraph and its text.
fn detect_style(text: &str, content: Option<&Node>) -> XmlStyle {
    let mut style = XmlStyle::default();
    let Some(content) = content else {
        if text.contains("\r\n") {
            style.newline = "\r\n".into();
        }
        return style;
    };
    let mut prev = content.open_end;
    for c in &content.children {
        if let Child::Element(p) = c
            && p.name == "Paragraph"
        {
            let lead = text.get(prev..p.span.start).unwrap_or("");
            if whitespace(lead) && lead.contains('\n') {
                if lead.contains("\r\n") {
                    style.newline = "\r\n".into();
                }
                let indent = lead.rsplit('\n').next().unwrap_or("");
                let chars: Vec<char> = indent.chars().collect();
                if !chars.is_empty()
                    && chars.len().is_multiple_of(2)
                    && chars.iter().all(|&c| c == chars[0])
                {
                    style.unit = chars[..chars.len() / 2].iter().collect();
                }
            }
            break;
        }
        prev = child_span(c).end;
    }
    // References used in text: look at the raw text of runs.
    let mut scan = |n: &Node| {
        for c in &n.children {
            if let Child::Text { raw, .. } = c {
                let raw = text.get(raw.clone()).unwrap_or("");
                for r in ["&#x27;", "&apos;", "&#39;"] {
                    if style.apostrophe.is_none() && raw.contains(r) {
                        style.apostrophe = Some(r.into());
                    }
                }
                for r in ["&quot;", "&#x22;", "&#34;"] {
                    if style.quote.is_none() && raw.contains(r) {
                        style.quote = Some(r.into());
                    }
                }
                if raw.contains("&gt;") {
                    style.gt = true;
                }
            }
        }
    };
    walk_texts(content, &mut scan, 0);
    style
}

fn walk_texts(n: &Node, f: &mut dyn FnMut(&Node), depth: usize) {
    if n.name == "Text" {
        f(n);
    }
    if depth > crate::xml::MAX_DEPTH {
        return;
    }
    for c in n.elements() {
        walk_texts(c, f, depth + 1);
    }
}

/// What the store is built from.
pub(crate) struct Source<'a> {
    pub(crate) text: &'a str,
    pub(crate) tree: &'a Tree,
    pub(crate) elements: &'a [Element],
    pub(crate) overrides: &'a BTreeMap<usize, Pt>,
    pub(crate) groups: &'a [Range<usize>],
    pub(crate) format: &'a PageFormat,
    pub(crate) title_lines: &'a [TitleLine],
}

/// Builds the store (§13.1), keeping only the paragraphs the generator
/// does not reproduce exactly (§13.2). Block ids are 1-based element
/// indices, as a document opened from the file numbers them.
pub(crate) fn build(src: &Source<'_>) -> Kept {
    let text = src.text;
    let root = &src.tree.root;
    let content = root.child("Content");
    let style = detect_style(text, content);
    let nl = style.newline.clone();

    let prolog = text.get(..root.span.start).unwrap_or("");
    let prolog = if !src.tree.outside_dropped && clean_misc(prolog) {
        prolog.to_string()
    } else {
        declaration_only(prolog, &nl)
    };
    let mut kept = Kept {
        prolog,
        root_open: open_tag(text, root),
        root_self_closing: root.self_closing,
        content_self_closing: false,
        sections: Vec::new(),
        root_close: String::new(),
        content_open: String::new(),
        content_close: String::new(),
        trailing: String::new(),
        paragraphs: BTreeMap::new(),
        loose: Vec::new(),
        title_lines: src.title_lines.to_vec(),
        style,
    };

    let mut prev = root.open_end;
    let mut content_done = false;
    let mut pending_text = String::new();
    for c in &root.children {
        let span = child_span(c);
        let lead = text.get(prev..span.start).unwrap_or("");
        let lead = if whitespace(lead) {
            lead.to_string()
        } else {
            // Skipped bytes lie between: whitespace before them goes too.
            pending_text.clear();
            format!("{nl}{}", kept.style.unit)
        };
        prev = span.end;
        match c {
            Child::Text { raw, .. } => {
                let t = &xml_text(text, raw);
                if whitespace(t) {
                    // The whitespace before the next child.
                    pending_text.push_str(t);
                    continue;
                }
                kept.sections
                    .push(Section::Text(format!("{pending_text}{lead}{t}")));
            }
            Child::Other(raw) => {
                let t = &xml_text(text, raw);
                kept.sections
                    .push(Section::Text(format!("{pending_text}{lead}{t}")));
            }
            Child::Element(n) if n.name == "Content" && !content_done => {
                content_done = true;
                kept.content_open = format!("{pending_text}{lead}{}", open_tag(text, n));
                kept.sections.push(Section::Content);
                if n.self_closing {
                    kept.content_self_closing = true;
                } else {
                    content_parts(src, n, &mut kept);
                }
            }
            Child::Element(n) if n.name == "TitlePage" && !n.repaired => {
                let section = title_page(text, n, &format!("{pending_text}{lead}"));
                kept.sections.push(section);
            }
            Child::Element(n) => {
                let body = element_text(text, n);
                kept.sections
                    .push(Section::Text(format!("{pending_text}{lead}{body}")));
            }
        }
        pending_text.clear();
    }
    // The root's end, and what follows it.
    let after = text.get(root.span.end..).unwrap_or("");
    let after_clean = !src.tree.after_root_dropped && clean_misc(after);
    if root.self_closing {
        kept.root_close = if after_clean {
            after.to_string()
        } else {
            nl.clone()
        };
        return kept;
    }
    let tail = text.get(prev..root.span.end).unwrap_or("");
    let end_tag = format!("</{}>", root.name);
    kept.root_close = if root.closed && ends_with_end_tag(tail, &root.name) {
        format!("{pending_text}{tail}")
    } else {
        format!("{pending_text}{nl}{end_tag}")
    };
    if after_clean && root.closed {
        kept.root_close.push_str(after);
    } else {
        kept.root_close.push_str(&nl);
    }
    kept
}

/// An element's start tag: as written, unless one of its attributes was
/// dropped. A self-closing tag stays self-closing.
fn open_tag(text: &str, n: &Node) -> String {
    if !n.tag_repaired {
        return text.get(n.span.start..n.open_end).unwrap_or("").to_string();
    }
    let tag = write::start_tag(n);
    if n.self_closing {
        format!("{}/>", tag.trim_end_matches('>'))
    } else {
        tag
    }
}

/// An element's text: as written when nothing inside it was repaired,
/// otherwise its repaired tree written out.
pub(crate) fn element_text(text: &str, n: &Node) -> String {
    if n.repaired {
        let mut out = String::new();
        write::serialize(text, n, &mut out);
        out
    } else {
        text.get(n.span.clone()).unwrap_or("").to_string()
    }
}

fn title_page(text: &str, n: &Node, lead: &str) -> Section {
    let mut prev = n.open_end;
    for c in &n.children {
        let span = child_span(c);
        if let Child::Element(c) = c
            && c.name == "Content"
        {
            let between = text.get(prev..span.start).unwrap_or("");
            if !whitespace(between) {
                break;
            }
            return Section::TitlePage {
                head: format!("{lead}{}", text.get(n.span.start..prev).unwrap_or("")),
                content: text.get(prev..span.end).unwrap_or("").to_string(),
                tail: text.get(span.end..n.span.end).unwrap_or("").to_string(),
            };
        }
        prev = span.end;
    }
    Section::Text(format!("{lead}{}", text.get(n.span.clone()).unwrap_or("")))
}

/// `Content`: its end, its loose children, the trailing paragraphs and the
/// paragraphs the generator does not reproduce.
fn content_parts(src: &Source<'_>, content: &Node, kept: &mut Kept) {
    let text = src.text;
    let nl = kept.style.newline.clone();
    let para_lead = format!("{nl}{}{}", kept.style.unit, kept.style.unit);
    let len = src.elements.len();
    let mut prev = content.open_end;
    let mut paragraph = 0usize;
    // Elements emitted before the current child, as the id of the last one.
    let mut last_id = 0u64;
    let mut pending = String::new();
    // Once a trailing paragraph is met, everything after it stays after it.
    let mut in_trailing = false;
    for c in &content.children {
        let span = child_span(c);
        let between = text.get(prev..span.start).unwrap_or("");
        let lead = if whitespace(between) {
            format!("{pending}{between}")
        } else {
            // Skipped bytes lie between: whitespace before them goes too.
            pending.clear();
            para_lead.clone()
        };
        prev = span.end;
        match c {
            Child::Element(p) if p.name == "Paragraph" => {
                let group = src.groups.get(paragraph).cloned().unwrap_or(0..0);
                paragraph += 1;
                let body = element_text(text, p);
                let source = format!("{lead}{body}");
                pending.clear();
                if group.start >= len || in_trailing {
                    // Left out of the model as a trailing empty paragraph.
                    in_trailing = true;
                    kept.trailing.push_str(&source);
                    continue;
                }
                let group = group.start..group.end.min(len);
                if group.is_empty() {
                    // A paragraph that gave no block (an empty dual
                    // dialogue): kept in place like a loose child.
                    kept.loose.push(Loose {
                        after: last_id,
                        text: source,
                    });
                    continue;
                }
                let blocks: Vec<KeptBlock> = group
                    .clone()
                    .map(|i| KeptBlock {
                        id: i as u64 + 1,
                        element: src.elements[i].clone(),
                        spacing: src.overrides.get(&i).copied(),
                    })
                    .collect();
                last_id = group.end as u64;
                if !p.repaired
                    && generated(&blocks, src.format, &kept.style, &para_lead)
                        == Some(source.clone())
                {
                    continue;
                }
                kept.paragraphs.insert(
                    group.start as u64 + 1,
                    KeptParagraph {
                        text: source,
                        verbatim: !p.repaired,
                        blocks,
                    },
                );
            }
            Child::Text { raw, .. } => {
                let t = &xml_text(text, raw);
                if whitespace(t) {
                    pending.push_str(t);
                    continue;
                }
                push_loose(kept, in_trailing, last_id, format!("{lead}{t}"));
                pending.clear();
            }
            Child::Other(raw) => {
                let t = &xml_text(text, raw);
                push_loose(kept, in_trailing, last_id, format!("{lead}{t}"));
                pending.clear();
            }
            Child::Element(n) => {
                let body = element_text(text, n);
                push_loose(kept, in_trailing, last_id, format!("{lead}{body}"));
                pending.clear();
            }
        }
    }
    let tail = text.get(prev..content.span.end).unwrap_or("");
    let end_tag = "</Content>";
    kept.content_close = if content.closed && ends_with_end_tag(tail, "Content") {
        format!("{pending}{tail}")
    } else {
        format!("{nl}{}{end_tag}", kept.style.unit)
    };
}

fn push_loose(kept: &mut Kept, trailing: bool, after: u64, text: String) {
    if trailing {
        kept.trailing.push_str(&text);
    } else {
        kept.loose.push(Loose { after, text });
    }
}

/// The group as the writer would generate it, if the writer would take
/// exactly these blocks as one group.
fn generated(
    blocks: &[KeptBlock],
    format: &PageFormat,
    style: &XmlStyle,
    lead: &str,
) -> Option<String> {
    let blks: Vec<Blk<'_>> = blocks
        .iter()
        .map(|b| Blk {
            id: b.id,
            element: &b.element,
            spacing: b.spacing,
        })
        .collect();
    if write::group_end(&blks, 0) != blks.len() {
        return None;
    }
    let mut g = write::Gen::new(style, format);
    g.group(&blks, lead, None);
    Some(g.out)
}
