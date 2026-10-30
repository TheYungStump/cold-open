//! A tolerant XML tree builder (docs/specs/fdx.md §2.2 to §2.4) that keeps
//! where everything came from, so the writer can copy what was not edited
//! (§13).
//!
//! quick-xml supplies the events; this module keeps its own stack of open
//! elements so it can repair mismatched end tags and close what is left
//! open. After a syntax error it skips to the next `<` and reads on with a
//! fresh reader over the rest of the text (§2.2).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use quick_xml::XmlVersion;
use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::reader::Reader;

use crate::FdxError;
use crate::report::{Locator, Report, codes, plural};

/// Elements nested deeper than this are dropped, which also bounds the
/// recursion of every walk over the tree.
pub(crate) const MAX_DEPTH: usize = 64;

/// After this many syntax errors the rest of the input is ignored, so a
/// file of noise is still read in linear time (§2.2).
const MAX_SYNTAX_ERRORS: usize = 1000;

/// One element: its name, attributes (first occurrence of each name, in
/// order), children, and where it is in the decoded text.
#[derive(Clone, Debug, Default)]
pub(crate) struct Node {
    pub(crate) name: String,
    pub(crate) attrs: Vec<(String, String)>,
    pub(crate) children: Vec<Child>,
    /// From its `<` to the end of its end tag, or to where it was closed.
    pub(crate) span: Range<usize>,
    /// Where its start tag ends.
    pub(crate) open_end: usize,
    /// Written `<X/>` rather than `<X></X>`.
    pub(crate) self_closing: bool,
    /// Closed by its own end tag (or self-closing), not by a repair.
    pub(crate) closed: bool,
    /// A structural repair happened inside it (a `fixed` or `dropped`
    /// issue at the XML level), so its text in the file is not what it
    /// means: it must be written from this tree, not copied (§13.2).
    pub(crate) repaired: bool,
    /// Its own start tag lost an attribute, so the tag itself must be
    /// written from the tree.
    pub(crate) tag_repaired: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum Child {
    Element(Node),
    /// Text, references and CDATA, resolved; `raw` is where they are.
    Text {
        value: String,
        raw: Range<usize>,
    },
    /// A comment or processing instruction, kept as written.
    Other(Range<usize>),
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
            _ => None,
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
            if let Child::Text { value, .. } = c {
                s.push_str(value);
            }
        }
        s
    }
}

/// The parsed file.
#[derive(Debug)]
pub(crate) struct Tree {
    pub(crate) root: Node,
    /// Elements after the root's end were dropped (`content_after_root`).
    pub(crate) after_root_dropped: bool,
    /// Text or bytes outside the root were skipped (`skipped_bytes`).
    pub(crate) outside_dropped: bool,
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

struct Builder<'t> {
    version: XmlVersion,
    stack: Vec<Open>,
    open_names: BTreeMap<String, usize>,
    root: Option<Node>,
    root_closed: bool,
    content_seen: bool,
    paragraphs: usize,
    too_deep_reported: bool,
    after_root_reported: bool,
    /// Something outside the root was skipped, so the text around the root
    /// is not written back as it is.
    outside_dropped: bool,
    locator: Locator<'t>,
}

fn new_reader(text: &str) -> Reader<&[u8]> {
    let mut reader = Reader::from_str(text);
    let config = reader.config_mut();
    config.check_end_names = false;
    config.allow_unmatched_ends = true;
    config.allow_dangling_amp = true;
    config.trim_text(false);
    config.expand_empty_elements = false;
    reader
}

/// Parses `text` into a tree whose root is `FinalDraft`.
pub(crate) fn parse(text: &str, report: &mut Report) -> Result<Tree, FdxError> {
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
        outside_dropped: false,
        locator: Locator::new(text),
    };
    let mut offset = 0usize;
    // A byte-order mark left in the text (the decoder removed the first)
    // would be skipped by the parser without counting it in its
    // positions, so it is skipped here, and reported.
    if let Some(rest) = text.strip_prefix('\u{FEFF}') {
        offset = text.len() - rest.len();
        b.outside_dropped = true;
        report.push_at(
            codes::SKIPPED_BYTES,
            "a second byte-order mark at the start of the text was skipped",
            None,
            (1, 1),
        );
    }
    let mut reader = new_reader(&text[offset..]);
    let mut errors = 0usize;
    let mut first_error: Option<String> = None;
    loop {
        let start = offset + reader.buffer_position() as usize;
        let event = reader.read_event();
        let end = offset + reader.buffer_position() as usize;
        match event {
            Ok(Event::Start(e)) => b.open(&e, start..end, false, report)?,
            Ok(Event::Empty(e)) => {
                b.open(&e, start..end, true, report)?;
                b.close_top(end, true);
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_string();
                b.end(&name, start..end, report);
            }
            Ok(Event::Text(t)) => {
                let value = b.valid_chars(t.xml_content(b.version).into_owned(), start, report);
                if b.stack.is_empty() && !value.chars().all(char::is_whitespace) {
                    // Text outside the root is not XML (§2.2).
                    let at = b.locator.locate(start);
                    report.push_at(
                        codes::SKIPPED_BYTES,
                        format!(
                            "{} of text outside the FinalDraft element were skipped",
                            plural(end - start, "byte", "bytes")
                        ),
                        None,
                        at,
                    );
                    b.outside_dropped = true;
                }
                b.append(&value, start..end);
            }
            Ok(Event::CData(c)) => {
                let value = b.valid_chars(c.xml_content(b.version).into_owned(), start, report);
                b.append(&value, start..end);
            }
            Ok(Event::GeneralRef(r)) => {
                let resolved = b.reference(&r, start, report);
                let resolved = b.valid_chars(resolved, start, report);
                b.append(&resolved, start..end);
            }
            Ok(Event::Decl(d)) => {
                if let Ok(v) = d.xml_version() {
                    b.version = v;
                }
                if !text
                    .get(start..end)
                    .unwrap_or("")
                    .chars()
                    .all(crate::write::valid_xml_char)
                {
                    let at = b.locator.locate(start);
                    // Inside an element no declaration belongs: it is left
                    // out, and the elements around it are written from the
                    // tree (§9, `invalid_char`).
                    let (message, para) = if b.stack.is_empty() {
                        (
                            "the XML declaration held characters XML does not allow; a plain \
                             one is written in its place",
                            None,
                        )
                    } else {
                        (
                            "an XML declaration inside an element held characters XML does not \
                             allow and was left out",
                            b.current_paragraph(),
                        )
                    };
                    report.push_at(codes::INVALID_CHAR, message.to_string(), para, at);
                    b.mark_repaired();
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::Comment(_) | Event::PI(_) | Event::DocType(_)) => {
                // Left out whole when written: taking the character out
                // could end it early (§9, `invalid_char`).
                if !text
                    .get(start..end)
                    .unwrap_or("")
                    .chars()
                    .all(crate::write::valid_xml_char)
                {
                    let at = b.locator.locate(start);
                    let para = b.current_paragraph();
                    report.push_at(
                        codes::INVALID_CHAR,
                        "a comment, processing instruction or doctype holding characters XML \
                         does not allow was left out"
                            .to_string(),
                        para,
                        at,
                    );
                    b.mark_repaired();
                }
                b.other(start..end)
            }
            Err(err) => {
                errors += 1;
                first_error.get_or_insert_with(|| err.to_string());
                // Skip to the next `<` after the markup that failed, and
                // read on from there (§2.2).
                let next = text.as_bytes()[start + 1..]
                    .iter()
                    .position(|&c| c == b'<')
                    .map_or(text.len(), |k| start + 1 + k);
                if b.root.is_none() && b.stack.is_empty() && next >= text.len() {
                    break;
                }
                if errors > MAX_SYNTAX_ERRORS {
                    let at = b.locator.locate(start);
                    let para = b.current_paragraph();
                    report.push_at(
                        codes::TRUNCATED_XML,
                        format!(
                            "more than {MAX_SYNTAX_ERRORS} XML errors; everything after line {}, column {} is ignored, and what came before is kept",
                            at.0, at.1
                        ),
                        para,
                        at,
                    );
                    b.mark_repaired();
                    break;
                }
                {
                    if b.stack.is_empty() {
                        b.outside_dropped = true;
                    }
                    let at = b.locator.locate(start);
                    let para = b.current_paragraph();
                    report.push_at(
                        codes::SKIPPED_BYTES,
                        format!(
                            "{} that could not be read as XML ({err}) were skipped",
                            plural(next - start, "byte", "bytes")
                        ),
                        para,
                        at,
                    );
                    b.mark_repaired();
                }
                if next >= text.len() {
                    break;
                }
                offset = next;
                reader = new_reader(&text[next..]);
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
        let at = b.locator.locate(text.len());
        report.push_at(codes::UNCLOSED_ELEMENT, message, para, at);
        b.mark_repaired();
        while !b.stack.is_empty() {
            b.close_top(text.len(), false);
        }
    }
    match b.root {
        Some(root) => Ok(Tree {
            root,
            after_root_dropped: b.after_root_reported,
            outside_dropped: b.outside_dropped,
        }),
        None => Err(FdxError::NotXml(
            first_error.unwrap_or_else(|| "no XML element found".into()),
        )),
    }
}

impl Builder<'_> {
    fn current_paragraph(&self) -> Option<usize> {
        self.stack.get(2).and_then(|o| o.paragraph)
    }

    /// Marks the open elements as holding a structural repair: the
    /// innermost one, which passes the mark to its parent when it closes,
    /// and every element leaves the stack through `close_top`, so all of
    /// them are marked by the end, as before. Marking each of them at every
    /// repair took time as nesting times repairs (2026-10-28, review
    /// finding CL3).
    fn mark_repaired(&mut self) {
        if let Some(o) = self.stack.last_mut() {
            o.node.repaired = true;
        }
    }

    fn open(
        &mut self,
        e: &BytesStart<'_>,
        span: Range<usize>,
        self_closing: bool,
        report: &mut Report,
    ) -> Result<(), FdxError> {
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
                let at = self.locator.locate(span.start);
                report.push_at(
                    codes::CONTENT_AFTER_ROOT,
                    format!(
                        "elements after the end of FinalDraft are ignored, starting with <{name}>"
                    ),
                    None,
                    at,
                );
            }
        }
        if self.stack.len() >= MAX_DEPTH && !parent_detached {
            detached = true;
            self.mark_repaired();
            if !self.too_deep_reported {
                self.too_deep_reported = true;
                let at = self.locator.locate(span.start);
                report.push_at(
                    codes::TOO_DEEP,
                    format!(
                        "elements nested more than {MAX_DEPTH} deep are dropped with their content"
                    ),
                    para,
                    at,
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
        let (attrs, dropped) = self.attributes(e, span.start, report, paragraph.or(para));
        if dropped {
            self.mark_repaired();
        }
        *self.open_names.entry(name.clone()).or_insert(0) += 1;
        self.stack.push(Open {
            node: Node {
                name,
                attrs,
                children: Vec::new(),
                open_end: span.end,
                span,
                self_closing,
                closed: false,
                repaired: dropped,
                tag_repaired: dropped,
            },
            detached,
            paragraph,
            first_content,
        });
        Ok(())
    }

    /// The element's attributes, and whether any was dropped.
    fn attributes(
        &mut self,
        e: &BytesStart<'_>,
        at: usize,
        report: &mut Report,
        para: Option<usize>,
    ) -> (Vec<(String, String)>, bool) {
        let element = e.name().as_ref().to_string();
        let mut out: Vec<(String, String)> = Vec::new();
        let mut dropped = false;
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
                            let pos = self.locator.locate(at);
                            report.push_at(
                                codes::ATTRIBUTE_VALUE,
                                format!(
                                    "attribute {key} of <{element}>: {err}; the value is used as written"
                                ),
                                para,
                                pos,
                            );
                            a.value.clone().into_owned()
                        }
                    };
                    let value = if value.chars().all(crate::write::valid_xml_char) {
                        value
                    } else {
                        dropped = true;
                        let pos = self.locator.locate(at);
                        report.push_at(
                            codes::INVALID_CHAR,
                            format!(
                                "attribute {key} of <{element}> held characters XML does not allow; they were dropped"
                            ),
                            para,
                            pos,
                        );
                        value
                            .chars()
                            .filter(|&c| crate::write::valid_xml_char(c))
                            .collect()
                    };
                    if !seen.insert(key.clone()) {
                        dropped = true;
                        let pos = self.locator.locate(at);
                        report.push_at(
                            codes::DUPLICATE_ATTRIBUTE,
                            format!(
                                "attribute {key} appears more than once on <{element}>; the first value is kept"
                            ),
                            para,
                            pos,
                        );
                        continue;
                    }
                    out.push((key, value));
                }
                Err(err) => {
                    dropped = true;
                    let pos = self.locator.locate(at);
                    report.push_at(
                        codes::ATTRIBUTE_SYNTAX,
                        format!("a malformed attribute of <{element}> was skipped: {err}"),
                        para,
                        pos,
                    );
                }
            }
        }
        (out, dropped)
    }

    /// Closes the innermost open element at `end`; `own` when its own end
    /// tag (or `/>`) closed it.
    fn close_top(&mut self, end: usize, own: bool) {
        let Some(mut open) = self.stack.pop() else {
            return;
        };
        if let Some(n) = self.open_names.get_mut(&open.node.name) {
            *n = n.saturating_sub(1);
        }
        open.node.span.end = end.max(open.node.open_end);
        open.node.closed = own;
        if open.detached {
            return;
        }
        match self.stack.last_mut() {
            Some(parent) => {
                parent.node.repaired |= open.node.repaired;
                parent.node.children.push(Child::Element(open.node));
            }
            None => {
                self.root = Some(open.node);
                self.root_closed = true;
            }
        }
    }

    fn end(&mut self, name: &str, span: Range<usize>, report: &mut Report) {
        let para = self.current_paragraph();
        if self.open_names.get(name).copied().unwrap_or(0) == 0 {
            let at = self.locator.locate(span.start);
            report.push_at(
                codes::STRAY_END_TAG,
                format!("the end tag </{name}> matches no open element and is ignored"),
                para,
                at,
            );
            self.mark_repaired();
            return;
        }
        let mut closed = Vec::new();
        while let Some(top) = self.stack.last() {
            if top.node.name == name {
                self.close_top(span.end, true);
                break;
            }
            closed.push(top.node.name.clone());
            self.mark_repaired();
            self.close_top(span.start, false);
        }
        if !closed.is_empty() {
            let at = self.locator.locate(span.start);
            report.push_at(
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
                at,
            );
        }
    }

    fn append(&mut self, text: &str, raw: Range<usize>) {
        let Some(top) = self.stack.last_mut() else {
            return;
        };
        if top.detached {
            return;
        }
        // Neighbouring text, references and CDATA are one child, as long as
        // nothing was skipped between them.
        if let Some(Child::Text { value, raw: r }) = top.node.children.last_mut()
            && r.end == raw.start
        {
            value.push_str(text);
            r.end = raw.end;
        } else {
            top.node.children.push(Child::Text {
                value: text.to_string(),
                raw,
            });
        }
    }

    /// Text without the characters XML 1.0 does not allow, which are
    /// dropped with a report (§2.3).
    fn valid_chars(&mut self, value: String, at: usize, report: &mut Report) -> String {
        if value.chars().all(crate::write::valid_xml_char) {
            return value;
        }
        let kept: String = value
            .chars()
            .filter(|&c| crate::write::valid_xml_char(c))
            .collect();
        let n = value.chars().count() - kept.chars().count();
        let pos = self.locator.locate(at);
        let para = self.current_paragraph();
        report.push_at(
            codes::INVALID_CHAR,
            format!(
                "{} that XML does not allow in text ({}) were dropped",
                plural(n, "character", "characters"),
                "control characters other than tab and line ends"
            ),
            para,
            pos,
        );
        self.mark_repaired();
        kept
    }

    fn other(&mut self, raw: Range<usize>) {
        if let Some(top) = self.stack.last_mut()
            && !top.detached
        {
            top.node.children.push(Child::Other(raw));
        }
    }

    /// The text an entity or character reference stands for, or the
    /// reference as written when it cannot be resolved.
    fn reference(&mut self, r: &BytesRef<'_>, at: usize, report: &mut Report) -> String {
        let name: &str = r;
        let para = self.current_paragraph();
        if r.is_char_ref() {
            return match r.resolve_char_ref() {
                Ok(Some(c)) if crate::write::valid_xml_char(c) => c.to_string(),
                _ => {
                    let _ = at;
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
        let tree = parse(text, &mut r).unwrap();
        (tree.root, r.finish())
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
    fn records_where_everything_is() {
        let text = "<?xml version=\"1.0\"?>\n<FinalDraft>\n  <!-- c -->\n  <Content>\n    <Paragraph Type=\"Action\"><Text>a&amp;b</Text><Text/></Paragraph>\n  </Content>\n</FinalDraft>\n";
        let (root, issues) = parse_ok(text);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(&text[root.span.clone()], &text[22..text.len() - 1]);
        assert!(root.closed && !root.repaired);
        assert!(matches!(&root.children[1], Child::Other(r) if &text[r.clone()] == "<!-- c -->"));
        let p = root.child("Content").unwrap().child("Paragraph").unwrap();
        assert_eq!(
            &text[p.span.clone()],
            "<Paragraph Type=\"Action\"><Text>a&amp;b</Text><Text/></Paragraph>"
        );
        assert_eq!(
            &text[p.span.start..p.open_end],
            "<Paragraph Type=\"Action\">"
        );
        let runs: Vec<&Node> = p.children_named("Text").collect();
        assert!(!runs[0].self_closing && runs[1].self_closing);
        match &runs[0].children[0] {
            Child::Text { value, raw } => {
                assert_eq!(value, "a&b");
                assert_eq!(&text[raw.clone()], "a&amp;b");
            }
            other => panic!("{other:?}"),
        }
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
        assert_eq!((issues[0].line, issues[0].column), (Some(1), Some(40)));
        assert_eq!(issues[1].paragraph, None);
        let content = root.child("Content").unwrap();
        let paras: Vec<&Node> = content.children_named("Paragraph").collect();
        assert_eq!(paras.len(), 2);
        assert!(paras[0].repaired && paras[0].closed);
        assert!(!paras[0].child("Text").unwrap().closed);
        assert!(!paras[1].repaired);
        assert!(root.repaired && !root.closed);
    }

    /// A syntax error no longer ends the file: the reader skips to the next
    /// markup and reads on (§2.2).
    #[test]
    fn reads_on_after_a_syntax_error() {
        let (root, issues) = parse_ok(
            "<FinalDraft><Content><Paragraph><Text>a</Text></Paragraph><!x junk><Paragraph><Text>b</Text></Paragraph></Content></FinalDraft>",
        );
        assert_eq!(codes_of(&issues)[0], codes::SKIPPED_BYTES);
        let content = root.child("Content").unwrap();
        let texts: Vec<String> = content
            .children_named("Paragraph")
            .filter_map(|p| p.child("Text"))
            .map(Node::text)
            .collect();
        assert_eq!(texts, ["a", "b"]);
    }

    #[test]
    fn noise_is_cut_off_after_many_errors() {
        let mut s = String::from("<FinalDraft><Content>");
        for _ in 0..3000 {
            s.push_str("< ");
        }
        let (_, issues) = parse_ok(&s);
        let skipped = issues
            .iter()
            .filter(|i| i.code == codes::SKIPPED_BYTES)
            .count();
        assert_eq!(skipped, MAX_SYNTAX_ERRORS);
        assert!(codes_of(&issues).contains(&codes::TRUNCATED_XML));
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
        assert!(root.repaired, "a dropped attribute is a repair");
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
        let mut r = Report::default();
        let tree = parse(
            "<FinalDraft><Content/></FinalDraft><Extra><Content/></Extra>",
            &mut r,
        )
        .unwrap();
        assert_eq!(codes_of(&r.finish()), [codes::CONTENT_AFTER_ROOT]);
        assert_eq!(tree.root.elements().count(), 1);
        assert!(tree.after_root_dropped);
    }
}
