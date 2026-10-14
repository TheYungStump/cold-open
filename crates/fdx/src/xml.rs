//! A tolerant XML tree builder (docs/specs/fdx.md §2.2 to §2.4).
//!
//! quick-xml supplies the events; this module keeps its own stack of open
//! elements so it can repair mismatched end tags, close what is left open
//! and keep everything read before a syntax error.

use std::collections::{BTreeMap, BTreeSet};

use quick_xml::XmlVersion;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::reader::Reader;

use crate::FdxError;
use crate::report::{Report, codes, plural};

/// Elements nested deeper than this are dropped, which also bounds the
/// recursion of every walk over the tree.
pub(crate) const MAX_DEPTH: usize = 64;

/// One element: its name, attributes (first occurrence of each name, in
/// order) and children.
#[derive(Debug, Default)]
pub(crate) struct Node {
    pub(crate) name: String,
    pub(crate) attrs: Vec<(String, String)>,
    pub(crate) children: Vec<Child>,
}

#[derive(Debug)]
pub(crate) enum Child {
    Element(Node),
    Text(String),
}

impl Node {
    pub(crate) fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub(crate) fn elements(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter_map(|c| match c {
            Child::Element(n) => Some(n),
            Child::Text(_) => None,
        })
    }

    pub(crate) fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.elements().filter(move |n| n.name == name)
    }

    pub(crate) fn child(&self, name: &str) -> Option<&Node> {
        self.elements().find(|n| n.name == name)
    }

    /// The element's own text: its text children joined, untrimmed.
    pub(crate) fn text(&self) -> String {
        let mut s = String::new();
        for c in &self.children {
            if let Child::Text(t) = c {
                s.push_str(t);
            }
        }
        s
    }
}

struct Open {
    node: Node,
    /// Dropped instead of attached to its parent when closed.
    detached: bool,
    /// For a `Paragraph` directly in the first `Content`: its index.
    paragraph: Option<usize>,
    /// For the first `Content` directly in the root.
    first_content: bool,
}

struct Builder {
    version: XmlVersion,
    stack: Vec<Open>,
    open_names: BTreeMap<String, usize>,
    root: Option<Node>,
    root_closed: bool,
    content_seen: bool,
    paragraphs: usize,
    too_deep_reported: bool,
    after_root_reported: bool,
}

/// Parses `text` into a tree whose root is `FinalDraft`.
pub(crate) fn parse(text: &str, report: &mut Report) -> Result<Node, FdxError> {
    let mut reader = Reader::from_str(text);
    {
        let config = reader.config_mut();
        config.check_end_names = false;
        config.allow_unmatched_ends = true;
        config.allow_dangling_amp = true;
        config.trim_text(false);
        config.expand_empty_elements = false;
    }
    let mut b = Builder {
        version: XmlVersion::Implicit1_0,
        stack: Vec::new(),
        open_names: BTreeMap::new(),
        root: None,
        root_closed: false,
        content_seen: false,
        paragraphs: 0,
        too_deep_reported: false,
        after_root_reported: false,
    };
    let mut seen_element = false;
    loop {
        let event = reader.read_event();
        match event {
            Ok(Event::Start(e)) => {
                seen_element = true;
                b.open(&e, report)?;
            }
            Ok(Event::Empty(e)) => {
                seen_element = true;
                b.open(&e, report)?;
                b.close_top();
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_string();
                b.end(&name, report);
            }
            Ok(Event::Text(t)) => b.append(&t.xml_content(b.version)),
            Ok(Event::CData(c)) => b.append(&c.xml_content(b.version)),
            Ok(Event::GeneralRef(r)) => {
                let resolved = b.reference(&r, report);
                b.append(&resolved);
            }
            Ok(Event::Decl(d)) => {
                if let Ok(v) = d.xml_version() {
                    b.version = v;
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::Comment(_) | Event::PI(_) | Event::DocType(_)) => {}
            Err(err) => {
                let at = reader.error_position();
                if !seen_element {
                    return Err(FdxError::NotXml(format!("at byte {at}: {err}")));
                }
                let para = b.current_paragraph();
                report.push(
                    codes::TRUNCATED_XML,
                    format!(
                        "XML error at byte {at}: {err}; everything after it is ignored, and what came before is kept"
                    ),
                    para,
                );
                break;
            }
        }
    }
    if !b.stack.is_empty() {
        let names: Vec<&str> = b.stack.iter().map(|o| o.node.name.as_str()).collect();
        let message = format!(
            "{} left open at the end of the file, closed: {}",
            plural(names.len(), "element was", "elements were"),
            names.join(" > ")
        );
        let para = b.current_paragraph();
        report.push(codes::UNCLOSED_ELEMENT, message, para);
        while !b.stack.is_empty() {
            b.close_top();
        }
    }
    match b.root {
        Some(root) => Ok(root),
        None => Err(FdxError::NotXml("no XML element found".into())),
    }
}

impl Builder {
    fn current_paragraph(&self) -> Option<usize> {
        self.stack.get(2).and_then(|o| o.paragraph)
    }

    fn open(&mut self, e: &BytesStart<'_>, report: &mut Report) -> Result<(), FdxError> {
        let name = e.name().as_ref().to_string();
        if self.root.is_none() && self.stack.is_empty() && name != "FinalDraft" {
            return Err(FdxError::NoFinalDraftRoot(name));
        }
        let para = self.current_paragraph();
        let parent_detached = self.stack.last().is_some_and(|o| o.detached);
        let mut detached = parent_detached;
        if self.stack.is_empty() && self.root_closed {
            detached = true;
            if !self.after_root_reported {
                self.after_root_reported = true;
                report.push(
                    codes::CONTENT_AFTER_ROOT,
                    format!(
                        "elements after the end of FinalDraft are ignored, starting with <{name}>"
                    ),
                    None,
                );
            }
        }
        if self.stack.len() >= MAX_DEPTH && !parent_detached {
            detached = true;
            if !self.too_deep_reported {
                self.too_deep_reported = true;
                report.push(
                    codes::TOO_DEEP,
                    format!(
                        "elements nested more than {MAX_DEPTH} deep are dropped with their content"
                    ),
                    para,
                );
            }
        }
        let depth = self.stack.len();
        let first_content = !detached
            && depth == 1
            && name == "Content"
            && !self.content_seen
            && self.stack[0].node.name == "FinalDraft";
        if first_content {
            self.content_seen = true;
        }
        let paragraph =
            if !detached && depth == 2 && name == "Paragraph" && self.stack[1].first_content {
                self.paragraphs += 1;
                Some(self.paragraphs - 1)
            } else {
                None
            };
        // A paragraph's own attributes belong to it.
        let attrs = self.attributes(e, report, paragraph.or(para));
        *self.open_names.entry(name.clone()).or_insert(0) += 1;
        self.stack.push(Open {
            node: Node {
                name,
                attrs,
                children: Vec::new(),
            },
            detached,
            paragraph,
            first_content,
        });
        Ok(())
    }

    fn attributes(
        &self,
        e: &BytesStart<'_>,
        report: &mut Report,
        para: Option<usize>,
    ) -> Vec<(String, String)> {
        let element = e.name().as_ref().to_string();
        let mut out: Vec<(String, String)> = Vec::new();
        // A set, so an element with thousands of attributes is still read
        // in O(n log n).
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut attrs = e.attributes();
        attrs.with_checks(false);
        for a in attrs {
            match a {
                Ok(a) => {
                    let key = a.key.as_ref().to_string();
                    let value = match a.normalized_value(self.version) {
                        Ok(v) => v.into_owned(),
                        Err(err) => {
                            report.push(
                                codes::ATTRIBUTE_VALUE,
                                format!(
                                    "attribute {key} of <{element}>: {err}; the value is used as written"
                                ),
                                para,
                            );
                            a.value.clone().into_owned()
                        }
                    };
                    if !seen.insert(key.clone()) {
                        report.push(
                            codes::DUPLICATE_ATTRIBUTE,
                            format!(
                                "attribute {key} appears more than once on <{element}>; the first value is kept"
                            ),
                            para,
                        );
                        continue;
                    }
                    out.push((key, value));
                }
                Err(err) => report.push(
                    codes::ATTRIBUTE_SYNTAX,
                    format!("a malformed attribute of <{element}> was skipped: {err}"),
                    para,
                ),
            }
        }
        out
    }

    fn close_top(&mut self) {
        let Some(open) = self.stack.pop() else {
            return;
        };
        if let Some(n) = self.open_names.get_mut(&open.node.name) {
            *n = n.saturating_sub(1);
        }
        if open.detached {
            return;
        }
        match self.stack.last_mut() {
            Some(parent) => parent.node.children.push(Child::Element(open.node)),
            None => {
                self.root = Some(open.node);
                self.root_closed = true;
            }
        }
    }

    fn end(&mut self, name: &str, report: &mut Report) {
        let para = self.current_paragraph();
        if self.open_names.get(name).copied().unwrap_or(0) == 0 {
            report.push(
                codes::STRAY_END_TAG,
                format!("the end tag </{name}> matches no open element and is ignored"),
                para,
            );
            return;
        }
        let mut closed = Vec::new();
        while let Some(top) = self.stack.last() {
            if top.node.name == name {
                self.close_top();
                break;
            }
            closed.push(top.node.name.clone());
            self.close_top();
        }
        if !closed.is_empty() {
            report.push(
                codes::MISMATCHED_END_TAG,
                format!(
                    "the end tag </{name}> also closed {}",
                    closed
                        .iter()
                        .map(|n| format!("<{n}>"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                para,
            );
        }
    }

    fn append(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let Some(top) = self.stack.last_mut() else {
            return;
        };
        if top.detached {
            return;
        }
        if let Some(Child::Text(t)) = top.node.children.last_mut() {
            t.push_str(text);
        } else {
            top.node.children.push(Child::Text(text.to_string()));
        }
    }

    /// The text an entity or character reference stands for, or the
    /// reference as written when it cannot be resolved.
    fn reference(&self, r: &BytesRef<'_>, report: &mut Report) -> String {
        let name: &str = r;
        let para = self.current_paragraph();
        if r.is_char_ref() {
            return match r.resolve_char_ref() {
                Ok(Some(c)) => c.to_string(),
                _ => {
                    report.tally(codes::BAD_CHAR_REF, name, para, |key, n| {
                        format!(
                            "the character reference &{key}; names no valid character and is kept as written ({})",
                            plural(n, "time", "times")
                        )
                    });
                    format!("&{name};")
                }
            };
        }
        match name {
            "lt" => "<".into(),
            "gt" => ">".into(),
            "amp" => "&".into(),
            "apos" => "'".into(),
            "quot" => "\"".into(),
            _ => {
                report.tally(codes::UNKNOWN_ENTITY, name, para, |key, n| {
                    format!(
                        "the entity &{key}; is not one of the five XML defines and is kept as written ({})",
                        plural(n, "time", "times")
                    )
                });
                format!("&{name};")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(text: &str) -> (Node, Vec<crate::Issue>) {
        let mut r = Report::default();
        let root = parse(text, &mut r).unwrap();
        (root, r.finish())
    }

    fn codes_of(issues: &[crate::Issue]) -> Vec<&'static str> {
        issues.iter().map(|i| i.code).collect()
    }

    #[test]
    fn builds_a_tree_with_untrimmed_text() {
        let (root, issues) = parse_ok(
            "<?xml version=\"1.0\"?>\n<FinalDraft Version=\"4\"><Content><Paragraph Type=\"Action\"><Text>  a &amp; b&#x27;s\r\nc  </Text><Text/></Paragraph></Content></FinalDraft>",
        );
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(root.attr("Version"), Some("4"));
        let p = root.child("Content").unwrap().child("Paragraph").unwrap();
        let texts: Vec<String> = p.children_named("Text").map(Node::text).collect();
        assert_eq!(texts, ["  a & b's\nc  ", ""]);
    }

    #[test]
    fn errors_only_for_non_xml_and_wrong_root() {
        let mut r = Report::default();
        assert!(matches!(parse("", &mut r), Err(FdxError::NotXml(_))));
        assert!(matches!(parse("hello", &mut r), Err(FdxError::NotXml(_))));
        assert!(matches!(parse("<<<", &mut r), Err(FdxError::NotXml(_))));
        assert_eq!(
            parse("<html><FinalDraft/></html>", &mut r).unwrap_err(),
            FdxError::NoFinalDraftRoot("html".into())
        );
    }

    #[test]
    fn repairs_end_tags() {
        let (root, issues) = parse_ok(
            "<FinalDraft><Content><Paragraph><Text>a</Paragraph></Bogus><Paragraph><Text>b</Text></Paragraph>",
        );
        assert_eq!(
            codes_of(&issues),
            [
                codes::MISMATCHED_END_TAG,
                codes::STRAY_END_TAG,
                codes::UNCLOSED_ELEMENT
            ]
        );
        assert_eq!(issues[0].paragraph, Some(0));
        assert_eq!(issues[1].paragraph, None);
        let content = root.child("Content").unwrap();
        assert_eq!(content.children_named("Paragraph").count(), 2);
    }

    #[test]
    fn keeps_what_came_before_a_syntax_error() {
        let (root, issues) =
            parse_ok("<FinalDraft><Content><Paragraph><Text>a</Text></Paragraph><Paragraph <<");
        assert_eq!(codes_of(&issues)[0], codes::TRUNCATED_XML);
        let content = root.child("Content").unwrap();
        assert_eq!(content.children_named("Paragraph").count(), 1);
    }

    #[test]
    fn attributes_and_references() {
        let (root, issues) = parse_ok(
            "<FinalDraft A=\"1\" A=\"2\" B=\"x&bogus;y\" C=\"&#x27;\"><Text>&nbsp;&#0;&lt;&apos;&quot;&gt; & </Text></FinalDraft>",
        );
        assert_eq!(root.attr("A"), Some("1"));
        assert_eq!(root.attr("B"), Some("x&bogus;y"));
        assert_eq!(root.attr("C"), Some("'"));
        assert_eq!(root.child("Text").unwrap().text(), "&nbsp;&#0;<'\"> & ");
        assert_eq!(
            codes_of(&issues),
            [
                codes::DUPLICATE_ATTRIBUTE,
                codes::ATTRIBUTE_VALUE,
                codes::UNKNOWN_ENTITY,
                codes::BAD_CHAR_REF
            ]
        );
    }

    #[test]
    fn depth_is_bounded() {
        let mut s = String::from("<FinalDraft>");
        for _ in 0..200 {
            s.push_str("<a>");
        }
        s.push_str("deep");
        let (root, issues) = parse_ok(&s);
        assert!(codes_of(&issues).contains(&codes::TOO_DEEP));
        let mut depth = 1;
        let mut n = &root;
        while let Some(c) = n.child("a") {
            depth += 1;
            n = c;
        }
        assert_eq!(depth, MAX_DEPTH);
    }

    #[test]
    fn content_after_root_is_ignored() {
        let (root, issues) =
            parse_ok("<FinalDraft><Content/></FinalDraft><Extra><Content/></Extra>");
        assert_eq!(codes_of(&issues), [codes::CONTENT_AFTER_ROOT]);
        assert_eq!(root.elements().count(), 1);
    }
}
