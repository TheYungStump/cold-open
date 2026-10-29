//! The .fdx writer (docs/specs/fdx.md §12 to §15).
//!
//! A document read from .fdx writes back from its passthrough store
//! everything that was not edited (§13.3); everything else is generated
//! here, in the file's own markup style (§14).

use std::collections::BTreeMap;

use coldopen_screenplay::{
    Align, Element, ElementFormat, ElementKind, PageFormat, Pt, RichText, Run, Style, TitleLine,
};

use crate::kept::{Kept, KeptParagraph, Section, XmlStyle};
use crate::settings::Spec;
use crate::values::{FdxAlign, type_of_kind};
use crate::xml::{Child, Node};

/// One block of the document being written.
#[derive(Clone, Copy, Debug)]
pub struct Blk<'a> {
    /// Its id: what the store's kept paragraphs and anchors name (§13).
    pub id: u64,
    pub element: &'a Element,
    /// Its spacing override, if it has one (§14.1).
    pub spacing: Option<Pt>,
}

/// What the writer is given (§12).
pub struct Doc<'a> {
    pub blocks: Vec<Blk<'a>>,
    pub format: &'a PageFormat,
    pub title_lines: &'a [TitleLine],
    /// The passthrough store of a document read from .fdx.
    pub kept: Option<&'a Kept>,
    /// The output will be UTF-8 although the file was not: the XML
    /// declaration's encoding is changed to match.
    pub declare_utf8: bool,
}

/// Something the document holds that .fdx cannot (§15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loss {
    /// The block it concerns.
    pub block: Option<u64>,
    pub code: &'static str,
    pub message: String,
}

pub struct Written {
    pub text: String,
    /// Empty when reading the text back gives the document again.
    pub losses: Vec<Loss>,
}

/// Writes a document as .fdx. Never panics.
pub fn write(doc: &Doc<'_>) -> Written {
    let style = doc.kept.map(|k| k.style.clone()).unwrap_or_default();
    let mut g = Gen::new(&style, doc.format);
    match doc.kept {
        Some(kept) => g.with_store(doc, kept),
        None => g.new_file(doc),
    }
    Written {
        text: g.out,
        losses: g.losses,
    }
}

/// Where the group starting at `i` ends (§14.1, §14.7): any page breaks
/// and blocks .fdx cannot hold, then one printing block, or a dual
/// dialogue block as layout pairs it. Blocks after the last printing one
/// form a group of their own.
pub(crate) fn group_end(blocks: &[Blk<'_>], i: usize) -> usize {
    let n = blocks.len();
    let mut j = i;
    while j < n && !blocks[j].element.kind.is_printing() {
        j += 1;
    }
    if j >= n {
        return n;
    }
    let p = blocks[j].element;
    if p.kind == ElementKind::Character && !p.dual {
        // A speech's lines, and how many; what .fdx leaves out between
        // them doesn't end it (§14.7).
        let members = |from: usize| {
            let (mut m, mut lines) = (from, 0);
            while m < n {
                let kind = blocks[m].element.kind;
                if kind.is_dialogue_member() {
                    lines += 1;
                } else if !left_out(kind) {
                    break;
                }
                m += 1;
            }
            (m, lines)
        };
        let (a_end, a) = members(j + 1);
        if a > 0
            && a_end < n
            && blocks[a_end].element.kind == ElementKind::Character
            && blocks[a_end].element.dual
        {
            let (b_end, b) = members(a_end + 1);
            if b > 0 {
                return b_end;
            }
        }
    }
    j + 1
}

/// What an .fdx file has no place for (§15): left out, and reported.
fn left_out(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Note | ElementKind::Boneyard | ElementKind::Section | ElementKind::Synopsis
    )
}

/// What an edited kept paragraph carries over (§14.4).
struct Carry<'k> {
    kept: &'k KeptParagraph,
    /// The kept text, parsed, and the paragraph in it.
    text: String,
    para: Node,
    /// The printing block it was read as, its spacing, and whether a page
    /// break came before it.
    read: Option<Element>,
    read_spacing: Option<Pt>,
    read_broke: bool,
    /// The paragraph was read nested in a dual-dialogue block.
    nested: bool,
}

/// Where each block finds the kept paragraph it was read as, ordinary or
/// nested in dual dialogue, so a group whose shape changed still carries
/// what each of its paragraphs held (§14.4, 2026-10-28).
#[derive(Default)]
pub(crate) struct Sources<'k> {
    by_block: BTreeMap<u64, &'k KeptParagraph>,
}

impl<'k> Sources<'k> {
    fn of(kept: &'k Kept) -> Sources<'k> {
        let mut by_block = BTreeMap::new();
        for kp in kept.paragraphs.values() {
            for b in kp.blocks.iter().filter(|b| b.element.kind.is_printing()) {
                by_block.insert(b.id, kp);
            }
        }
        Sources { by_block }
    }

    /// What block `id` carries from the paragraph it was read as.
    fn carry(&self, id: u64) -> Option<Carry<'k>> {
        let c = Carry::of(self.by_block.get(&id)?)?;
        if c.para.child("DualDialogue").is_some() {
            c.nested_for(id)
        } else {
            Some(c)
        }
    }
}

impl<'k> Carry<'k> {
    fn of(kept: &'k KeptParagraph) -> Option<Carry<'k>> {
        let (text, para) = parse_paragraph(&kept.text)?;
        let mut read_broke = false;
        let mut read = None;
        let mut read_spacing = None;
        for b in &kept.blocks {
            if b.element.kind == ElementKind::PageBreak {
                read_broke = true;
            } else if b.element.kind.is_printing() {
                read = Some(b.element.clone());
                read_spacing = b.spacing;
                break;
            }
        }
        Some(Carry {
            kept,
            text,
            para,
            read,
            read_spacing,
            read_broke,
            nested: false,
        })
    }

    /// The carry of the nested paragraph of a kept dual-dialogue block that
    /// was read as block `id`: nested paragraphs and printing blocks are
    /// paired in order, as the reader made them (§6.6).
    fn nested_for(&self, id: u64) -> Option<Carry<'k>> {
        let dd = self.para.child("DualDialogue")?;
        let (k, b) = self
            .kept
            .blocks
            .iter()
            .filter(|b| b.element.kind.is_printing())
            .enumerate()
            .find(|(_, b)| b.id == id)?;
        let n = dd.children_named("Paragraph").nth(k)?;
        Some(Carry {
            kept: self.kept,
            text: self.text.clone(),
            para: n.clone(),
            read: Some(b.element.clone()),
            read_spacing: b.spacing,
            read_broke: false,
            nested: true,
        })
    }

    /// The outer paragraph's own parts that only a dual-dialogue block has
    /// a place for: attributes beyond its type and the inline layout ones,
    /// and children beyond `DualDialogue` and `Text`.
    fn outer_parts(&self) -> bool {
        const LAYOUT: [&str; 9] = [
            "Alignment",
            "FirstIndent",
            "Leading",
            "LeftIndent",
            "RightIndent",
            "SpaceBefore",
            "Spacing",
            "StartsNewPage",
            "Type",
        ];
        self.para.attrs.iter().any(|(k, _)| !LAYOUT.contains(&k.as_str()))
            || self.para.children.iter().any(|ch| {
                matches!(ch, Child::Element(n) if !matches!(n.name.as_str(), "DualDialogue" | "Text"))
            })
    }
}

pub(crate) struct Gen<'a> {
    style: &'a XmlStyle,
    format: &'a PageFormat,
    pub(crate) out: String,
    pub(crate) losses: Vec<Loss>,
}

impl<'a> Gen<'a> {
    pub(crate) fn new(style: &'a XmlStyle, format: &'a PageFormat) -> Gen<'a> {
        Gen {
            style,
            format,
            out: String::new(),
            losses: Vec::new(),
        }
    }

    fn nl(&self) -> &'a str {
        &self.style.newline
    }

    fn indent(&self, depth: usize) -> String {
        self.style.indent(depth)
    }

    fn loss(&mut self, block: u64, code: &'static str, message: &str) {
        self.losses.push(Loss {
            block: Some(block),
            code,
            message: message.into(),
        });
    }

    /// Writes one group (§14.1, §14.7), carrying what an edited kept
    /// paragraph held (§14.4): the group's own, or, where the group changed
    /// shape, the paragraph each block was read as (`sources`).
    pub(crate) fn group(
        &mut self,
        blocks: &[Blk<'_>],
        lead: &str,
        carry: Option<&KeptParagraph>,
        sources: &Sources<'_>,
    ) {
        let mut starts_page = false;
        let mut k = 0;
        while k < blocks.len() && !blocks[k].element.kind.is_printing() {
            let b = blocks[k];
            match b.element.kind {
                ElementKind::PageBreak => starts_page = true,
                kind => self.transparent_loss(b.id, kind),
            }
            k += 1;
        }
        // What .fdx leaves out inside a dual-dialogue block (§14.7).
        let mut rest: Vec<Blk<'_>> = Vec::new();
        for &b in &blocks[k..] {
            if left_out(b.element.kind) {
                self.transparent_loss(b.id, b.element.kind);
            } else {
                rest.push(b);
            }
        }
        let rest = rest.as_slice();
        if rest.is_empty() {
            // Breaks with nothing after them print nothing (§14.7).
            return;
        }
        // A dual-dialogue wrapper's attributes are not a speech's (§14.4).
        // When the group changed shape, each paragraph carries from the
        // paragraph its block was read as, ordinary or nested.
        let dual = rest.len() > 1;
        let own = carry.and_then(Carry::of);
        let own_dual = own
            .as_ref()
            .is_some_and(|c| c.para.child("DualDialogue").is_some());
        if dual {
            let outer = own.filter(|_| own_dual);
            self.dual(rest, starts_page, lead, outer.as_ref(), sources);
        } else {
            let b = rest[0];
            let c = match own {
                Some(c) if !own_dual => Some(c),
                Some(c) => {
                    // The dual-dialogue block this was the first speaker of
                    // is no more: what only its wrapper could hold is lost.
                    // A wrapper the reader could not read as two speakers
                    // (§6.6, `dual_dialogue_shape`) never was one.
                    if c.outer_parts() && c.kept.blocks.iter().any(|k| k.element.dual) {
                        self.loss(
                            b.id,
                            "kept_parts",
                            "attributes or elements the dual-dialogue block held have no place in ordinary speeches and are left out",
                        );
                    }
                    sources.carry(b.id)
                }
                None => sources.carry(b.id),
            };
            self.paragraph(b, starts_page, lead, 2, c.as_ref());
        }
    }

    fn transparent_loss(&mut self, block: u64, kind: ElementKind) {
        let (code, what) = match kind {
            ElementKind::Note => ("note_element", "a note"),
            ElementKind::Boneyard => ("boneyard_element", "boneyard"),
            ElementKind::Section => ("section_element", "a section"),
            ElementKind::Synopsis => ("synopsis_element", "a synopsis"),
            _ => return,
        };
        self.loss(
            block,
            code,
            &format!("{what} has no place in .fdx files and is left out"),
        );
    }

    /// A paragraph at `depth` (2 in `Content`).
    fn paragraph(
        &mut self,
        b: Blk<'_>,
        starts_page: bool,
        lead: &str,
        depth: usize,
        carry: Option<&Carry<'_>>,
    ) {
        let e = b.element;
        if e.dual && depth == 2 {
            self.loss(
                b.id,
                "dual_unpaired",
                "a dual-dialogue character with no speech to pair it with is written as an ordinary one",
            );
        }
        let mut attrs: Vec<(String, String)> = Vec::new();
        if e.centered && e.kind == ElementKind::Action {
            attrs.push(("Alignment".into(), "Center".into()));
        }
        if e.kind == ElementKind::SceneHeading
            && let Some(n) = &e.scene_number
        {
            attrs.push(("Number".into(), n.clone()));
        }
        if let Some(sb) = self.spacing(b) {
            if (Pt::ZERO..=self.format.body_height()).contains(&sb) {
                attrs.push(("SpaceBefore".into(), sb.to_string()));
            } else {
                self.loss(
                    b.id,
                    "spacing_out_of_range",
                    "the space above this paragraph is more than a page holds and is not kept",
                );
            }
        }
        if starts_page {
            attrs.push(("StartsNewPage".into(), "Yes".into()));
        }
        attrs.push(("Type".into(), type_of_kind(e.kind).into()));
        let mut children: Vec<(usize, String)> = Vec::new();
        let mut run_attrs = None;
        if let Some(c) = carry {
            self.carry_paragraph(c, b, starts_page, depth, &mut attrs, &mut children);
            run_attrs = Some(c);
        }
        attrs.sort_by(|a, b| a.0.cmp(&b.0));
        self.out.push_str(lead);
        self.out.push_str("<Paragraph");
        push_attrs(&mut self.out, &attrs);
        self.out.push('>');
        let runs = match run_attrs {
            Some(c) => carried_runs(&c.para, &e.text),
            None => plain_runs(&e.text),
        };
        self.runs_and_end(&runs, &children, depth, b.id);
        self.hidden_losses(b);
    }

    /// The space above a block: its override, or its blank lines (§16.2).
    fn spacing(&self, b: Blk<'_>) -> Option<Pt> {
        b.spacing.or_else(|| {
            b.element
                .space_before
                .map(|n| Pt(self.format.line_height.0.saturating_mul(i32::from(n))))
        })
    }

    fn hidden_losses(&mut self, b: Blk<'_>) {
        for r in b.element.text.runs() {
            match r {
                Run::Note(_) => {
                    self.loss(b.id, "inline_note", "a note inside the text is left out");
                }
                Run::Boneyard(_) => {
                    self.loss(
                        b.id,
                        "inline_boneyard",
                        "boneyard inside the text is left out",
                    );
                }
                Run::Text { .. } => {}
            }
        }
    }

    /// One `<Text>` run.
    fn run(&mut self, text: &str, style: Style, extra: &[(String, String)], block: u64) {
        let mut attrs: Vec<(String, String)> = extra.to_vec();
        let tokens = style_attr(style);
        if !tokens.is_empty() {
            attrs.push(("Style".into(), tokens));
        }
        attrs.sort_by(|a, b| a.0.cmp(&b.0));
        self.out.push_str("<Text");
        push_attrs(&mut self.out, &attrs);
        self.out.push('>');
        if escape_text(text, self.style, &mut self.out) {
            self.loss(
                block,
                "invalid_xml_char",
                "characters XML cannot hold are left out",
            );
        }
        self.out.push_str("</Text>");
    }

    /// The attributes and children of an edited kept paragraph that carry
    /// over (§14.4).
    fn carry_paragraph(
        &mut self,
        c: &Carry<'_>,
        b: Blk<'_>,
        starts_page: bool,
        depth: usize,
        attrs: &mut Vec<(String, String)>,
        children: &mut Vec<(usize, String)>,
    ) {
        let e = b.element;
        let (read, read_spacing, read_broke) = (c.read.as_ref(), c.read_spacing, c.read_broke);
        let same_kind = read.is_some_and(|r| r.kind == e.kind);
        let modelled = [
            "Alignment",
            "Number",
            "SpaceBefore",
            "StartsNewPage",
            "Type",
        ];
        // Inline layout attributes are measured against the settings of the
        // paragraph's type (§6.4), so they carry only under the same type.
        let source_type = c.para.attr("Type");
        let written_type = match source_type {
            Some(t) if same_kind && type_of_kind(e.kind) != t => t,
            _ => type_of_kind(e.kind),
        };
        let same_type = source_type == Some(written_type);
        const LAYOUT: [&str; 8] = [
            "Alignment",
            "FirstIndent",
            "Leading",
            "LeftIndent",
            "RightIndent",
            "SpaceBefore",
            "Spacing",
            "StartsNewPage",
        ];
        for (k, v) in &c.para.attrs {
            if !same_type && LAYOUT.contains(&k.as_str()) {
                continue;
            }
            let keep = match k.as_str() {
                "Alignment" => read.is_some_and(|r| r.centered == e.centered) && !e.centered,
                "SpaceBefore" => read_spacing == b.spacing && self.spacing(b).is_none(),
                // A nested paragraph takes no break (§6.6): only one read
                // nested carries what it had.
                "StartsNewPage" => {
                    (depth == 2 || c.nested) && read_broke == starts_page && !starts_page
                }
                "Number" => same_kind && e.kind != ElementKind::SceneHeading,
                "Type" => same_kind && type_of_kind(e.kind) != v.as_str(),
                _ => true,
            };
            if !keep {
                continue;
            }
            if modelled.contains(&k.as_str()) {
                attrs.retain(|(name, _)| name != k);
            }
            attrs.push((k.clone(), v.clone()));
        }
        let mut runs_before = 0usize;
        for ch in &c.para.children {
            let Child::Element(n) = ch else {
                continue;
            };
            match n.name.as_str() {
                "Text" => runs_before += 1,
                "DualDialogue" => {}
                "SceneProperties" if e.kind != ElementKind::SceneHeading => {}
                _ => children.push((runs_before, crate::kept::element_text(&c.text, n))),
            }
        }
    }

    /// A dual-dialogue block (§14.7).
    fn dual(
        &mut self,
        blocks: &[Blk<'_>],
        starts_page: bool,
        lead: &str,
        carry: Option<&Carry<'_>>,
        sources: &Sources<'_>,
    ) {
        let space = self.format.elements.character.space_before;
        let mut attrs: Vec<(String, String)> = vec![("SpaceBefore".into(), space.to_string())];
        if starts_page {
            attrs.push(("StartsNewPage".into(), "Yes".into()));
        }
        attrs.push(("Type".into(), "General".into()));
        let mut extra_children: Vec<(usize, String)> = Vec::new();
        if let Some(c) = carry {
            for (k, v) in &c.para.attrs {
                if !matches!(k.as_str(), "SpaceBefore" | "StartsNewPage" | "Type") {
                    attrs.push((k.clone(), v.clone()));
                }
            }
            for ch in &c.para.children {
                if let Child::Element(n) = ch
                    && !matches!(n.name.as_str(), "DualDialogue" | "Text")
                {
                    extra_children.push((0, crate::kept::element_text(&c.text, n)));
                }
            }
        }
        attrs.sort_by(|a, b| a.0.cmp(&b.0));
        let nl = self.nl().to_string();
        self.out.push_str(lead);
        self.out.push_str("<Paragraph");
        push_attrs(&mut self.out, &attrs);
        self.out.push('>');
        for (_, child) in &extra_children {
            self.out.push_str(&nl);
            self.out.push_str(&self.indent(3));
            self.out.push_str(child);
        }
        self.out.push_str(&nl);
        self.out.push_str(&self.indent(3));
        self.out.push_str("<DualDialogue>");
        let nested_lead = format!("{nl}{}", self.indent(4));
        // Each speech carries the nested paragraph it was read as, by its
        // block, whatever was added to or taken from the block; a speech
        // that was an ordinary paragraph carries that.
        for b in blocks {
            let c = carry
                .and_then(|c| c.nested_for(b.id))
                .or_else(|| sources.carry(b.id));
            self.paragraph(*b, false, &nested_lead, 4, c.as_ref());
        }
        self.out.push_str(&nl);
        self.out.push_str(&self.indent(3));
        self.out.push_str("</DualDialogue>");
        self.out.push_str(&nl);
        self.out.push_str(&self.indent(2));
        self.out.push_str("</Paragraph>");
    }

    /// `Content`'s paragraphs for a document with a store (§13.3).
    fn content(&mut self, doc: &Doc<'_>, kept: &Kept) {
        let blocks = &doc.blocks;
        let lead = format!("{}{}", self.nl(), self.indent(2));
        // Loose children by the block they now follow (§13.4).
        let ids: Vec<u64> = blocks.iter().map(|b| b.id).collect();
        let mut present: Vec<u64> = ids.clone();
        present.sort_unstable();
        let mut loose: BTreeMap<u64, Vec<&str>> = BTreeMap::new();
        for l in &kept.loose {
            let anchor = if l.after == 0 || present.binary_search(&l.after).is_ok() {
                l.after
            } else {
                match present.partition_point(|&id| id < l.after) {
                    0 => 0,
                    k => present[k - 1],
                }
            };
            loose.entry(anchor).or_default().push(&l.text);
        }
        fn emit(out: &mut String, id: u64, loose: &mut BTreeMap<u64, Vec<&str>>) {
            if let Some(texts) = loose.remove(&id) {
                for t in texts {
                    out.push_str(t);
                }
            }
        }
        emit(&mut self.out, 0, &mut loose);
        let anchors = anchors(kept);
        let sources = Sources::of(kept);
        let mut i = 0;
        while i < blocks.len() {
            let group = group_end(blocks, i);
            let kp = blocks[i..group]
                .iter()
                .find(|b| b.element.kind.is_printing())
                .and_then(|b| anchors.get(&b.id).copied());
            // The kept text, while the paragraph's blocks are as they were
            // read, start where this group starts, and take in at least the
            // whole group; otherwise the group is generated, carrying what
            // the paragraph held (§13.3).
            let end = match kp {
                Some(kp)
                    if kp.verbatim
                        && kp.blocks.len() >= group - i
                        && unchanged(kp, &blocks[i..]) =>
                {
                    self.out.push_str(&kp.text);
                    i + kp.blocks.len()
                }
                _ => {
                    self.group(&blocks[i..group], &lead, kp, &sources);
                    group
                }
            };
            for b in &blocks[i..end] {
                emit(&mut self.out, b.id, &mut loose);
            }
            i = end;
        }
        // Anything whose anchor is gone and has no block before it.
        for texts in loose.into_values() {
            for t in texts {
                self.out.push_str(t);
            }
        }
        self.out.push_str(&kept.trailing);
    }

    fn with_store(&mut self, doc: &Doc<'_>, kept: &Kept) {
        if doc.declare_utf8 {
            self.out.push_str(&declare_utf8(&kept.prolog));
        } else {
            self.out.push_str(&kept.prolog);
        }
        // The root's children are written first, so a root that was
        // `<FinalDraft/>` is opened only if something now goes in it.
        let outer = std::mem::take(&mut self.out);
        let nl = self.nl();
        let mut wrote_content = false;
        let mut wrote_title = false;
        for s in &kept.sections {
            match s {
                Section::Text(t) => self.out.push_str(t),
                Section::Content => {
                    wrote_content = true;
                    let before = std::mem::take(&mut self.out);
                    self.content(doc, kept);
                    let body = std::mem::replace(&mut self.out, before);
                    if kept.content_self_closing && body.is_empty() {
                        self.out.push_str(&kept.content_open);
                    } else if kept.content_self_closing {
                        self.out.push_str(&opened(&kept.content_open));
                        self.out.push_str(&body);
                        self.out
                            .push_str(&format!("{nl}{}</Content>", self.indent(1)));
                    } else {
                        self.out.push_str(&kept.content_open);
                        self.out.push_str(&body);
                        self.out.push_str(&kept.content_close);
                    }
                    if !doc.title_lines.is_empty()
                        && kept.title_lines.is_empty()
                        && !kept.title_page_text
                        && !kept
                            .sections
                            .iter()
                            .any(|s| matches!(s, Section::TitlePage { .. }))
                    {
                        wrote_title = true;
                        self.title_page(doc.title_lines);
                    }
                }
                Section::TitlePage {
                    head,
                    content,
                    tail,
                } => {
                    self.out.push_str(head);
                    // Only the first title page is read (§7); a store from
                    // before 2026-10-25 may name a later one too.
                    if wrote_title || doc.title_lines == kept.title_lines.as_slice() {
                        self.out.push_str(content);
                    } else {
                        self.title_edited(content, &kept.title_lines, doc.title_lines);
                    }
                    wrote_title = true;
                    self.out.push_str(tail);
                }
            }
        }
        if !wrote_content && !doc.blocks.is_empty() {
            self.out
                .push_str(&format!("{nl}{}<Content>", self.indent(1)));
            self.content(doc, kept);
            self.out
                .push_str(&format!("{nl}{}</Content>", self.indent(1)));
        }
        if !wrote_title
            && !doc.title_lines.is_empty()
            && kept.title_lines.is_empty()
            && !kept.title_page_text
        {
            self.title_page(doc.title_lines);
        }
        if !wrote_title
            && (kept.title_page_text || !kept.title_lines.is_empty())
            && doc.title_lines != kept.title_lines.as_slice()
        {
            // The title page is kept as text and cannot be rewritten (§13.5).
            self.losses.push(Loss {
                block: None,
                code: "title_page_kept",
                message: "this file's title page can only be written back as it was, so the edits to it are not saved".into(),
            });
        }
        let inner = std::mem::replace(&mut self.out, outer);
        if kept.root_self_closing && inner.is_empty() {
            self.out.push_str(&kept.root_open);
        } else if kept.root_self_closing {
            self.out.push_str(&opened(&kept.root_open));
            self.out.push_str(&inner);
            self.out.push_str(&format!("{nl}</FinalDraft>"));
        } else {
            self.out.push_str(&kept.root_open);
            self.out.push_str(&inner);
        }
        self.out.push_str(&kept.root_close);
    }

    /// A `TitlePage` section of title lines alone (§14.3).
    fn title_page(&mut self, lines: &[TitleLine]) {
        let nl = self.nl().to_string();
        self.out
            .push_str(&format!("{nl}{nl}{}<TitlePage>", self.indent(1)));
        let lead = format!("{nl}{}", self.indent(2));
        self.title_content(&lead, lines);
        self.out
            .push_str(&format!("{nl}{}</TitlePage>", self.indent(1)));
    }

    /// The title page's `Content` (§14.3).
    fn title_content(&mut self, lead: &str, lines: &[TitleLine]) {
        let nl = self.nl().to_string();
        self.out.push_str(lead);
        self.out.push_str("<Content>");
        let line_lead = format!("{nl}{}", self.indent(3));
        for line in lines {
            self.title_line(&line_lead, line, None);
        }
        self.out
            .push_str(&format!("{nl}{}</Content>", self.indent(2)));
    }

    /// A kept title page's `Content` once its lines have changed (§13.5):
    /// lines as they were read are written as the file has them, edited
    /// lines carry what their paragraphs held, and everything else in
    /// `Content` stays after the paragraph it followed.
    fn title_edited(&mut self, content: &str, read: &[TitleLine], lines: &[TitleLine]) {
        let parts = crate::kept::title_parts(content).filter(|p| p.paragraphs.len() == read.len());
        let pairs = parts
            .as_ref()
            .and_then(|_| coldopen_screenplay::words::common(read, lines));
        let (Some(parts), Some(pairs)) = (parts, pairs) else {
            let nl = self.nl();
            let lead = content.find('<').map_or_else(
                || format!("{nl}{}", self.indent(2)),
                |k| content[..k].to_string(),
            );
            self.title_content(&lead, lines);
            return;
        };
        let lead = format!("{}{}", self.nl(), self.indent(3));
        self.out.push_str(&parts.open);
        self.out.push_str(&parts.after[0]);
        let (mut r, mut l) = (0, 0);
        let end = std::iter::once((read.len(), lines.len()));
        for (pr, pl) in pairs.into_iter().chain(end) {
            // Between two equal lines, lines paired in order are edits.
            while r < pr && l < pl {
                self.title_line(&lead, &lines[l], Some((&parts.paragraphs[r], &read[r])));
                self.out.push_str(&parts.after[r + 1]);
                (r, l) = (r + 1, l + 1);
            }
            while r < pr {
                self.out.push_str(&parts.after[r + 1]);
                r += 1;
            }
            while l < pl {
                self.title_line(&lead, &lines[l], None);
                l += 1;
            }
            if pr < read.len() {
                self.out.push_str(&parts.paragraphs[pr]);
                self.out.push_str(&parts.after[pr + 1]);
                (r, l) = (pr + 1, pl + 1);
            }
        }
        self.out.push_str(&parts.close);
    }

    /// One title line (§14.3). An edited line is given the paragraph it
    /// replaces, as the file has it, and the line read from it, and carries
    /// what that paragraph held (§13.5).
    fn title_line(&mut self, lead: &str, line: &TitleLine, replaces: Option<(&str, &TitleLine)>) {
        let mut attrs: Vec<(String, String)> = vec![
            ("Alignment".into(), align_name(line.align).into()),
            ("LeftIndent".into(), inches(line.left)),
            ("RightIndent".into(), inches(line.right)),
            ("SpaceBefore".into(), line.space_before.to_string()),
        ];
        let mut children: Vec<(usize, String)> = Vec::new();
        let carry = replaces.and_then(|(source, read)| Some((parse_paragraph(source)?, read)));
        if let Some(((text, para), read)) = &carry {
            carry_title(para, text, read, line, &mut attrs, &mut children);
        }
        attrs.sort_by(|a, b| a.0.cmp(&b.0));
        self.out.push_str(lead);
        self.out.push_str("<Paragraph");
        push_attrs(&mut self.out, &attrs);
        self.out.push('>');
        let runs = match &carry {
            Some(((_, para), _)) => carried_runs(para, &line.text),
            None => plain_runs(&line.text),
        };
        self.runs_and_end(&runs, &children, 3, 0);
    }

    /// A paragraph's runs at `depth`, with the children it carries in their
    /// places among them, and its end tag.
    fn runs_and_end(
        &mut self,
        runs: &[RunOut],
        children: &[(usize, String)],
        depth: usize,
        block: u64,
    ) {
        let inner = format!("{}{}", self.nl(), self.indent(depth + 1));
        let mut k = 0;
        for (r, (text, style, extra)) in runs.iter().enumerate() {
            while k < children.len() && children[k].0 <= r {
                self.out.push_str(&inner);
                self.out.push_str(&children[k].1);
                k += 1;
            }
            self.out.push_str(&inner);
            self.run(text, *style, extra, block);
        }
        for (_, child) in &children[k..] {
            self.out.push_str(&inner);
            self.out.push_str(child);
        }
        self.out.push_str(self.nl());
        let close = self.indent(depth);
        self.out.push_str(&close);
        self.out.push_str("</Paragraph>");
    }

    /// A whole file for a document with no store (§14.6).
    fn new_file(&mut self, doc: &Doc<'_>) {
        let nl = self.nl().to_string();
        let f = self.format;
        self.out
            .push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>");
        self.out.push_str(&nl);
        self.out
            .push_str("<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"1\">");
        self.out
            .push_str(&format!("{nl}{nl}{}<Content>", self.indent(1)));
        let lead = format!("{nl}{}", self.indent(2));
        let blocks = &doc.blocks;
        let mut i = 0;
        while i < blocks.len() {
            let end = group_end(blocks, i);
            self.group(&blocks[i..end], &lead, None, &Sources::default());
            i = end;
        }
        self.out
            .push_str(&format!("{nl}{}</Content>", self.indent(1)));
        if !doc.title_lines.is_empty() {
            self.title_page(doc.title_lines);
        }
        for kind in [
            ElementKind::SceneHeading,
            ElementKind::Action,
            ElementKind::Character,
            ElementKind::Parenthetical,
            ElementKind::Dialogue,
            ElementKind::Transition,
            ElementKind::Shot,
            ElementKind::General,
            ElementKind::Lyric,
        ] {
            if let Some(ef) = f.elements.get(kind) {
                self.element_settings(kind, ef);
            }
        }
        let i1 = self.indent(1);
        let i2 = self.indent(2);
        let i3 = self.indent(3);
        let yes = |b: bool| if b { "Yes" } else { "No" };
        let mut s = String::new();
        s.push_str(&format!("{nl}{nl}{i1}<PageLayout"));
        push_attrs(
            &mut s,
            &[
                ("BottomMargin".into(), f.margin_bottom.to_string()),
                (
                    "BreakDialogueAndActionAtSentences".into(),
                    yes(f.break_at_sentences).into(),
                ),
                ("DocumentLeading".into(), "Normal".into()),
                ("HeaderMargin".into(), f.header_top.to_string()),
                ("TopMargin".into(), f.margin_top.to_string()),
            ],
        );
        s.push('>');
        s.push_str(&format!("{nl}{i2}<PageSize"));
        push_attrs(
            &mut s,
            &[
                ("Height".into(), inches(f.page_height)),
                ("Width".into(), inches(f.page_width)),
            ],
        );
        s.push_str(&format!("/>{nl}{i1}</PageLayout>"));

        let pn = &f.page_numbers;
        s.push_str(&format!("{nl}{nl}{i1}<HeaderAndFooter"));
        push_attrs(
            &mut s,
            &[
                ("FooterFirstPage".into(), "No".into()),
                ("FooterVisible".into(), "No".into()),
                ("HeaderFirstPage".into(), yes(pn.first <= 1).into()),
                ("HeaderVisible".into(), yes(pn.visible).into()),
                ("StartingPage".into(), "1".into()),
            ],
        );
        s.push('>');
        s.push_str(&format!(
            "{nl}{i2}<Header>{nl}{i3}<Paragraph Alignment=\"Right\">"
        ));
        if pn.visible {
            let i4 = self.indent(4);
            s.push_str(&format!(
                "{nl}{i4}<DynamicLabel Type=\"Page #\"/>{nl}{i4}<Text>"
            ));
            escape_text(&pn.suffix, self.style, &mut s);
            s.push_str("</Text>");
        }
        s.push_str(&format!(
            "{nl}{i3}</Paragraph>{nl}{i2}</Header>{nl}{i1}</HeaderAndFooter>"
        ));

        let more = f.more.clone();
        let contd = f.contd.clone();
        s.push_str(&format!(
            "{nl}{nl}{i1}<MoresAndContinueds>{nl}{i2}<DialogueBreaks"
        ));
        push_attrs(
            &mut s,
            &[
                (
                    "AutomaticCharacterContinueds".into(),
                    yes(f.contd_after_action).into(),
                ),
                ("BottomOfPage".into(), yes(more.is_some()).into()),
                (
                    "DialogueBottom".into(),
                    more.unwrap_or_else(|| "(MORE)".into()),
                ),
                (
                    "DialogueTop".into(),
                    contd.clone().unwrap_or_else(|| "(CONT'D)".into()),
                ),
                ("TopOfNext".into(), yes(contd.is_some()).into()),
            ],
        );
        let (sc_bottom, sc_top) = f
            .scene_continueds
            .as_ref()
            .map_or((None, None), |sc| (sc.bottom.clone(), sc.top.clone()));
        s.push_str(&format!("/>{nl}{i2}<SceneBreaks"));
        push_attrs(
            &mut s,
            &[
                ("ContinuedNumber".into(), "No".into()),
                (
                    "SceneBottom".into(),
                    sc_bottom.clone().unwrap_or_else(|| "(CONTINUED)".into()),
                ),
                ("SceneBottomOfPage".into(), yes(sc_bottom.is_some()).into()),
                (
                    "SceneTop".into(),
                    sc_top.clone().unwrap_or_else(|| "CONTINUED:".into()),
                ),
                ("SceneTopOfNext".into(), yes(sc_top.is_some()).into()),
            ],
        );
        s.push_str(&format!("/>{nl}{i1}</MoresAndContinueds>"));

        s.push_str(&format!("{nl}{nl}{i1}<SceneNumberOptions"));
        push_attrs(
            &mut s,
            &[
                ("LeftLocation".into(), inches(f.scene_number_left)),
                ("RightLocation".into(), inches(f.scene_number_right)),
                ("ShowNumbersOnLeft".into(), "Yes".into()),
                ("ShowNumbersOnRight".into(), "Yes".into()),
            ],
        );
        s.push_str("/>");
        s.push_str(&format!("{nl}</FinalDraft>{nl}"));
        self.out.push_str(&s);
    }

    fn element_settings(&mut self, kind: ElementKind, ef: &ElementFormat) {
        let nl = self.nl().to_string();
        let (i1, i2) = (self.indent(1), self.indent(2));
        let mut tokens = Vec::new();
        if ef.bold {
            tokens.push("Bold");
        }
        if ef.italic {
            tokens.push("Italic");
        }
        if ef.underline {
            tokens.push("Underline");
        }
        if ef.caps {
            tokens.push("AllCaps");
        }
        let yes = |b: bool| if b { "Yes" } else { "No" };
        let mut s = format!(
            "{nl}{nl}{i1}<ElementSettings Type=\"{}\">{nl}{i2}<FontSpec",
            type_of_kind(kind)
        );
        push_attrs(
            &mut s,
            &[
                ("Font".into(), "Courier Prime".into()),
                ("Size".into(), self.format.font_size.to_string()),
                ("Style".into(), tokens.join("+")),
            ],
        );
        s.push_str(&format!("/>{nl}{i2}<ParagraphSpec"));
        push_attrs(
            &mut s,
            &[
                ("Alignment".into(), align_name(ef.align).into()),
                ("FirstIndent".into(), inches(ef.first_indent)),
                ("Leading".into(), "Regular".into()),
                ("LeftIndent".into(), inches(ef.left)),
                ("RightIndent".into(), inches(ef.right)),
                ("SpaceBefore".into(), ef.space_before.to_string()),
                ("Spacing".into(), "1".into()),
                ("StartsNewPage".into(), yes(ef.starts_new_page).into()),
            ],
        );
        s.push_str(&format!("/>{nl}{i2}<Behavior"));
        push_attrs(
            &mut s,
            &[("PaginateAs".into(), type_of_kind(ef.paginate_as).into())],
        );
        s.push_str(&format!("/>{nl}{i1}</ElementSettings>"));
        self.out.push_str(&s);
    }
}

/// A run to write: its text, its style and its other attributes.
type RunOut = (String, Style, Vec<(String, String)>);

/// Whether the blocks from here on are exactly a kept paragraph's (§13.3).
/// The kept paragraphs by their anchor: the first of their blocks that is
/// not a page break, which is the paragraph's own (§13.3, 2026-10-28). The
/// store keys a paragraph by its group's first block, which is the page
/// break `StartsNewPage` put before it, when there is one; but the break is
/// a block of its own. Deleting it, adding one, or moving the paragraph's
/// scene away from it must not part the paragraph from its block, or hand
/// its kept parts to whatever comes after the break (review findings GR8,
/// GP2).
fn anchors(kept: &Kept) -> BTreeMap<u64, &KeptParagraph> {
    kept.paragraphs
        .values()
        .filter_map(|kp| {
            kp.blocks
                .iter()
                .find(|b| b.element.kind != ElementKind::PageBreak)
                .map(|b| (b.id, kp))
        })
        .collect()
}

fn unchanged(kp: &KeptParagraph, blocks: &[Blk<'_>]) -> bool {
    kp.blocks.len() <= blocks.len()
        && kp
            .blocks
            .iter()
            .zip(blocks)
            .all(|(k, b)| k.id == b.id && k.element == *b.element && k.spacing == b.spacing)
}

/// A paragraph of the file, parsed: the text it was parsed from (the
/// paragraph inside a root element) and its node.
fn parse_paragraph(source: &str) -> Option<(String, Node)> {
    let text = format!("<FinalDraft>{source}</FinalDraft>");
    let mut report = crate::report::Report::default();
    let tree = crate::xml::parse(&text, &mut report).ok()?;
    let para = tree.root.children.into_iter().find_map(|c| match c {
        Child::Element(n) if n.name == "Paragraph" => Some(n),
        _ => None,
    })?;
    Some((text, para))
}

/// What an edited title line carries from the paragraph it replaces
/// (§13.5): the paragraph's other attributes, each of the four §14.3
/// writes as the file spelled it while the line keeps the value read, and
/// its children other than `Text`.
fn carry_title(
    para: &Node,
    text: &str,
    read: &TitleLine,
    line: &TitleLine,
    attrs: &mut Vec<(String, String)>,
    children: &mut Vec<(usize, String)>,
) {
    const MODELLED: [&str; 4] = ["Alignment", "LeftIndent", "RightIndent", "SpaceBefore"];
    let spec = Spec::parse(para, "", None, &mut crate::report::Report::default());
    // `same`: the line keeps the value read. `taken`: the reader took the
    // file's value as written, not clamped or replaced.
    let mut as_read = |name: &str, same: bool, taken: bool| {
        if !same {
            return;
        }
        match para.attr(name) {
            None => attrs.retain(|(k, _)| k != name),
            Some(v) if taken => {
                if let Some(a) = attrs.iter_mut().find(|(k, _)| k == name) {
                    a.1 = v.to_string();
                }
            }
            Some(_) => {}
        }
    };
    as_read(
        "Alignment",
        line.align == read.align,
        spec.align.map(FdxAlign::to_align) == Some(read.align),
    );
    as_read(
        "LeftIndent",
        line.left == read.left,
        spec.left == Some(read.left),
    );
    as_read(
        "RightIndent",
        line.right == read.right,
        spec.right == Some(read.right),
    );
    as_read(
        "SpaceBefore",
        line.space_before == read.space_before,
        spec.space_before == Some(read.space_before),
    );
    for (k, v) in &para.attrs {
        if !MODELLED.contains(&k.as_str()) {
            attrs.push((k.clone(), v.clone()));
        }
    }
    let mut runs_before = 0usize;
    for ch in &para.children {
        let Child::Element(n) = ch else {
            continue;
        };
        if n.name == "Text" {
            runs_before += 1;
        } else {
            children.push((runs_before, crate::kept::element_text(text, n)));
        }
    }
}

/// The runs of a text: text, style and no other attributes.
fn plain_runs(text: &RichText) -> Vec<RunOut> {
    let mut out = Vec::new();
    for r in text.runs() {
        if let Run::Text { text, style, .. } = r {
            out.push((text.clone(), *style, Vec::new()));
        }
    }
    if out.is_empty() {
        out.push((String::new(), Style::PLAIN, Vec::new()));
    }
    out
}

/// The runs of an edited element with the kept runs' other attributes
/// carried per character (§14.4).
fn carried_runs(para: &Node, e: &RichText) -> Vec<RunOut> {
    // The old printed text, one attribute set per character.
    let mut old_chars: Vec<char> = Vec::new();
    let mut old_attrs: Vec<usize> = Vec::new();
    let mut sets: Vec<Vec<(String, String)>> = Vec::new();
    // Each distinct attribute set once, found in O(log n).
    let mut index: BTreeMap<Vec<(String, String)>, usize> = BTreeMap::new();
    for t in para.children_named("Text") {
        let caps = t
            .attr("Style")
            .is_some_and(|s| crate::values::style_tokens(s).all_caps);
        let value = if caps {
            t.text().to_uppercase()
        } else {
            t.text()
        };
        let set: Vec<(String, String)> = t
            .attrs
            .iter()
            .filter(|(k, _)| k != "Style")
            .cloned()
            .collect();
        let k = *index.entry(set.clone()).or_insert_with(|| {
            sets.push(set);
            sets.len() - 1
        });
        for ch in value.chars() {
            old_chars.push(ch);
            old_attrs.push(k);
        }
    }
    if sets.iter().all(Vec::is_empty) {
        return plain_runs(e);
    }
    let new: Vec<(char, Style)> = e
        .runs()
        .iter()
        .flat_map(|r| match r {
            Run::Text { text, style, .. } => text.chars().map(|c| (c, *style)).collect(),
            _ => Vec::new(),
        })
        .collect();
    // Common start and end with the old text (§14.4).
    let prefix = old_chars
        .iter()
        .zip(&new)
        .take_while(|(a, (b, _))| *a == b)
        .count();
    let max_suffix = old_chars.len().min(new.len()) - prefix;
    let suffix = old_chars
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(max_suffix)
        .take_while(|(a, (b, _))| *a == b)
        .count();
    let default = old_attrs.first().copied().unwrap_or(0);
    let mut attr_of = Vec::with_capacity(new.len());
    for i in 0..new.len() {
        let a = if i < prefix {
            old_attrs[i]
        } else if i >= new.len() - suffix {
            old_attrs[old_chars.len() - (new.len() - i)]
        } else if i > 0 {
            // Inserted text takes the attributes of the character before.
            *attr_of.last().unwrap_or(&default)
        } else {
            old_attrs
                .get(old_chars.len() - suffix)
                .copied()
                .unwrap_or(default)
        };
        attr_of.push(a);
    }
    let mut out: Vec<RunOut> = Vec::new();
    let mut last: Option<(Style, usize)> = None;
    for (i, (ch, style)) in new.iter().enumerate() {
        let key = (*style, attr_of[i]);
        let extend = last == Some(key);
        match out.last_mut() {
            Some(run) if extend => run.0.push(*ch),
            _ => out.push((ch.to_string(), *style, sets[attr_of[i]].clone())),
        }
        last = Some(key);
    }
    if out.is_empty() {
        out.push((String::new(), Style::PLAIN, sets[default].clone()));
    }
    out
}

fn style_attr(style: Style) -> String {
    let mut tokens = Vec::new();
    if style.bold {
        tokens.push("Bold");
    }
    if style.italic {
        tokens.push("Italic");
    }
    if style.underline {
        tokens.push("Underline");
    }
    tokens.join("+")
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Left => "Left",
        Align::Center => "Center",
        Align::Right => "Right",
    }
}

/// Inches with two decimals, or as many as the value needs to read back
/// exactly, up to five (§14.5).
pub(crate) fn inches(p: Pt) -> String {
    let units = i64::from(p.0);
    for digits in 2..=5u32 {
        let scale = 10i64.pow(digits);
        // units / 7200 inches, rounded half away from zero to `digits`.
        let num = units * scale;
        let q = if num >= 0 {
            (num + 3600) / 7200
        } else {
            (num - 3600) / 7200
        };
        let back = coldopen_screenplay::Pt::parse_inches(&fixed(q, digits)).map(|(p, _)| p);
        if back == Some(p) || digits == 5 {
            return fixed(q, digits);
        }
    }
    String::new()
}

fn fixed(q: i64, digits: u32) -> String {
    let scale = 10i64.pow(digits);
    let sign = if q < 0 { "-" } else { "" };
    let a = q.unsigned_abs();
    format!(
        "{sign}{}.{:0width$}",
        a / scale as u64,
        a % scale as u64,
        width = digits as usize
    )
}

/// Attributes as ` name="value"`, escaped for a double-quoted value.
fn push_attrs(out: &mut String, attrs: &[(String, String)]) {
    for (k, v) in attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        for c in v.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '"' => out.push_str("&quot;"),
                '\t' => out.push_str("&#9;"),
                '\n' => out.push_str("&#10;"),
                '\r' => out.push_str("&#13;"),
                c if valid_xml_char(c) => out.push(c),
                _ => {}
            }
        }
        out.push('"');
    }
}

/// Text or markup as the file had it, without the characters XML does
/// not allow (§9, `invalid_char`). Markup holding one (a comment,
/// processing instruction, doctype or CDATA section) is left out whole:
/// taking the character out could end it early (`<?x\0>?>`).
pub(crate) fn writable_raw(raw: &str) -> String {
    if raw.chars().all(valid_xml_char) {
        raw.to_string()
    } else if raw.starts_with('<') {
        String::new()
    } else {
        raw.chars().filter(|&c| valid_xml_char(c)).collect()
    }
}

pub(crate) fn valid_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// Text escaped as §14.2 says. Returns true when characters XML cannot hold
/// were left out.
fn escape_text(text: &str, style: &XmlStyle, out: &mut String) -> bool {
    let mut dropped = false;
    let mut prev = ['\0', '\0'];
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            // `]]>` may not appear in text as it is.
            '>' if style.gt || prev == [']', ']'] => out.push_str("&gt;"),
            '\'' => out.push_str(style.apostrophe.as_deref().unwrap_or("'")),
            '"' => out.push_str(style.quote.as_deref().unwrap_or("\"")),
            '\r' => out.push_str("&#13;"),
            c if valid_xml_char(c) => out.push(c),
            _ => dropped = true,
        }
        prev = [prev[1], c];
    }
    dropped
}

/// A self-closing tag (with any whitespace before it) as a start tag.
fn opened(tag: &str) -> String {
    let t = tag.trim_end();
    match t.strip_suffix("/>") {
        Some(open) => format!("{open}>"),
        None => tag.to_string(),
    }
}

/// The prolog with its declaration's encoding changed to UTF-8.
fn declare_utf8(prolog: &str) -> String {
    let Some(start) = prolog.find("<?xml") else {
        return prolog.to_string();
    };
    let Some(len) = prolog[start..].find("?>") else {
        return prolog.to_string();
    };
    let decl = &prolog[start..start + len];
    let Some(at) = decl.find("encoding") else {
        return prolog.to_string();
    };
    let rest = &decl[at + "encoding".len()..];
    let Some(q) = rest.find(['"', '\'']) else {
        return prolog.to_string();
    };
    let quote = &rest[q..=q];
    let Some(close) = rest[q + 1..].find(quote) else {
        return prolog.to_string();
    };
    let value_start = start + at + "encoding".len() + q + 1;
    let value_end = value_start + close;
    format!("{}UTF-8{}", &prolog[..value_start], &prolog[value_end..])
}

/// An element written out from the tree, for parts the reader repaired
/// (§13.5). Children that were not repaired are copied as written.
pub(crate) fn serialize(text: &str, n: &Node, out: &mut String) {
    if !valid_name(&n.name) {
        // Not an element anything could read back; it is left out.
        return;
    }
    out.push_str(&start_tag(n));
    if n.children.is_empty() && n.self_closing {
        out.pop();
        out.push_str("/>");
        return;
    }
    for c in &n.children {
        match c {
            Child::Text { raw, .. } | Child::Other(raw) => {
                out.push_str(&writable_raw(text.get(raw.clone()).unwrap_or("")));
            }
            Child::Element(child) if child.repaired => serialize(text, child, out),
            Child::Element(child) => out.push_str(text.get(child.span.clone()).unwrap_or("")),
        }
    }
    out.push_str("</");
    out.push_str(&n.name);
    out.push('>');
}

/// An element's start tag written from the tree, leaving out attributes
/// whose names XML would not read back.
pub(crate) fn start_tag(n: &Node) -> String {
    let mut out = String::from("<");
    out.push_str(&n.name);
    let attrs: Vec<(String, String)> = n
        .attrs
        .iter()
        .filter(|(k, _)| valid_name(k))
        .cloned()
        .collect();
    push_attrs(&mut out, &attrs);
    out.push('>');
    out
}

/// Whether a name is one XML reads back as a name: a letter, `_` or `:`
/// first (or any character beyond ASCII), then also digits, `-` and `.`.
pub(crate) fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let start = |c: char| c.is_ascii_alphabetic() || c == '_' || c == ':' || !c.is_ascii();
    start(first) && chars.all(|c| start(c) || c.is_ascii_digit() || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inches_read_back_exactly() {
        for units in [0, 72, 10800, -720, 1, 7199, 26600, 54000] {
            let p = Pt(units);
            let s = inches(p);
            assert_eq!(Pt::parse_inches(&s).map(|(q, _)| q), Some(p), "{units} {s}");
        }
        assert_eq!(inches(Pt::inch100(150)), "1.50");
        assert_eq!(inches(Pt::inch100(-10)), "-0.10");
    }

    #[test]
    fn declarations_name_utf8() {
        assert_eq!(
            declare_utf8("<?xml version=\"1.0\" encoding=\"windows-1252\"?>\n"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
        );
        assert_eq!(
            declare_utf8("<?xml version='1.0'?>"),
            "<?xml version='1.0'?>"
        );
    }

    #[test]
    fn text_is_escaped_as_the_file_does() {
        let mut out = String::new();
        let style = XmlStyle {
            apostrophe: Some("&#x27;".into()),
            ..XmlStyle::default()
        };
        assert!(!escape_text("a<b & c's ]]> \"x\"\r", &style, &mut out));
        assert_eq!(out, "a&lt;b &amp; c&#x27;s ]]&gt; \"x\"&#13;");
        let mut out = String::new();
        assert!(escape_text("a\u{1}b", &XmlStyle::default(), &mut out));
        assert_eq!(out, "ab");
    }
}
