//! Checks shared by the cargo-fuzz target in `fuzz/` and the stable test
//! suite, so the fuzzed properties also run on every CI build.

use std::collections::BTreeMap;

use coldopen_screenplay::{Element, ElementKind, PageFormat, Pt, RichText, Run, Style};

use crate::{Blk, Doc, FdxDoc};

/// Reads `data`, writes it back unedited and reads that again: the
/// round-trip gate's G1 and G2 on any input (docs/specs/fdx.md §16.1).
pub fn roundtrip(data: &[u8]) -> Result<(), String> {
    let Ok(doc) = crate::read(data) else {
        return Ok(());
    };
    let written = doc.write_back();
    if !written.losses.is_empty() {
        let codes: Vec<&str> = written.losses.iter().map(|l| l.code).collect();
        return Err(format!("writing back unedited reports losses: {codes:?}"));
    }
    let out = crate::encode(&written.text, doc.encoding);
    if !doc.repaired() && out != data {
        let at = out.iter().zip(data).take_while(|(a, b)| a == b).count();
        return Err(format!(
            "G1: an unrepaired file written back unedited differs at byte {at}"
        ));
    }
    let again =
        crate::read(&out).map_err(|e| format!("G2: what was written cannot be read: {e}"))?;
    if let Some(i) = again.report.iter().find(|i| i.is_repair()) {
        return Err(format!("G2: what was written needs repair: {}", i.code));
    }
    if again.script != doc.script
        || again.format != doc.format
        || again.overrides != doc.overrides
        || again.title_lines != doc.title_lines
    {
        return Err("G2: reading what was written gives another document".into());
    }
    if !doc.repaired() && again.kept != doc.kept {
        return Err("G2: reading what was written gives another store".into());
    }
    let twice = crate::encode(&again.write_back().text, again.encoding);
    if twice != out {
        return Err("G2: writing what was read back gives other bytes".into());
    }
    Ok(())
}

/// A file split for the round-trip gate's G3 (docs/specs/fdx.md §16.1):
/// everything outside `Content`'s children, and each child with the
/// whitespace before it.
pub fn content_children(text: &str) -> Option<(String, Vec<String>)> {
    let mut report = crate::report::Report::default();
    let tree = crate::xml::parse(text, &mut report).ok()?;
    let content = tree.root.child("Content")?;
    let mut children = Vec::new();
    // Where the next child's entry starts: whitespace joins what follows.
    let mut pending = content.open_end;
    for c in &content.children {
        let span = match c {
            crate::xml::Child::Element(n) => n.span.clone(),
            crate::xml::Child::Text { raw, .. } | crate::xml::Child::Other(raw) => raw.clone(),
        };
        let this = text.get(pending..span.end)?;
        if matches!(c, crate::xml::Child::Text { .. }) && this.trim().is_empty() {
            continue;
        }
        children.push(this.to_string());
        pending = span.end;
    }
    let outside = format!("{}{}", text.get(..content.open_end)?, text.get(pending..)?);
    Some((outside, children))
}

/// A document made from arbitrary bytes, for the writer's fuzz target and
/// property test: every byte chooses something about the next element.
pub fn document_from_bytes(data: &[u8]) -> (Vec<Element>, BTreeMap<usize, Pt>) {
    const WORDS: [&str; 12] = [
        "Rain", " ", "fall", "s.", "\n", "\t", "a&b", "<x>", "it's", "\"q\"", "]]>", "\u{1}",
    ];
    let mut elements = Vec::new();
    let mut spacing = BTreeMap::new();
    let mut it = data.iter().copied();
    while let Some(k) = it.next() {
        let kind = ElementKind::ALL[usize::from(k) % ElementKind::ALL.len()];
        let mut runs = Vec::new();
        let n = it.next().unwrap_or(0) % 5;
        for _ in 0..n {
            let w = it.next().unwrap_or(0);
            let style = Style {
                bold: w & 0x10 != 0,
                italic: w & 0x20 != 0,
                underline: w & 0x40 != 0,
            };
            let text = WORDS[usize::from(w) % WORDS.len()];
            runs.push(if kind.is_raw_text() {
                Run::plain(text)
            } else if w >> 7 == 1 {
                Run::Note("n".into())
            } else {
                Run::styled(text, style)
            });
        }
        let flags = it.next().unwrap_or(0);
        let mut e = Element::new(kind, RichText::new(runs));
        if kind == ElementKind::PageBreak {
            e = Element::new(kind, "");
        }
        if kind == ElementKind::SceneHeading && flags & 1 != 0 {
            e.scene_number = Some(format!("{}A", flags >> 1));
        }
        e.dual = kind == ElementKind::Character && flags & 2 != 0;
        e.centered = kind == ElementKind::Action && flags & 4 != 0;
        if kind.is_printing() && flags & 8 != 0 {
            e.space_before = Some(flags >> 5);
        }
        if kind.is_printing() && flags & 16 != 0 {
            spacing.insert(elements.len(), Pt::points(i32::from(flags >> 4)));
        }
        if e.problems().is_empty() {
            elements.push(e);
        }
    }
    (elements, spacing)
}

/// Writes a document as a new .fdx file and reads it back: it must read
/// with no repair and nothing kept, and give the document's .fdx normal
/// form (docs/specs/fdx.md §16.2) less what the writer listed as lost.
pub fn write_document(elements: &[Element], spacing: &BTreeMap<usize, Pt>) -> Result<(), String> {
    let format = PageFormat::default();
    let blocks: Vec<Blk<'_>> = elements
        .iter()
        .enumerate()
        .map(|(i, element)| Blk {
            id: i as u64 + 1,
            element,
            spacing: spacing.get(&i).copied(),
        })
        .collect();
    let written = crate::write(&Doc {
        blocks,
        format: &format,
        title_lines: &[],
        kept: None,
        declare_utf8: false,
    });
    let doc = crate::read(written.text.as_bytes())
        .map_err(|e| format!("cannot read what was written: {e}"))?;
    if let Some(i) = doc.report.iter().find(|i| {
        i.is_repair()
            || (i.class == crate::codes::KEPT && i.code != crate::codes::TRAILING_EMPTY_DROPPED)
    }) {
        return Err(format!(
            "what was written reads with {}: {}",
            i.code, i.message
        ));
    }
    if doc.format != format {
        return Err("the default format does not read back as itself".into());
    }
    let (want, want_spacing) = normal_form(elements, spacing, &format);
    if doc.script.elements != want {
        return Err(format!(
            "read back {:?}\nexpected {:?}",
            doc.script.elements, want
        ));
    }
    if doc.overrides != want_spacing {
        return Err(format!(
            "spacing read back {:?}, expected {:?}",
            doc.overrides, want_spacing
        ));
    }
    // Writing what was read gives the same text again.
    if doc.write_back().text != written.text {
        return Err("writing what was read back gives other text".into());
    }
    Ok(())
}

fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// The .fdx normal form of a document (docs/specs/fdx.md §16.2), with what
/// §15 lists as lost removed: what reading a new file written from it gives.
pub fn normal_form(
    elements: &[Element],
    spacing: &BTreeMap<usize, Pt>,
    format: &PageFormat,
) -> (Vec<Element>, BTreeMap<usize, Pt>) {
    let body = format.body_height();
    // Which characters layout pairs as the right side of dual dialogue.
    let blocks: Vec<Blk<'_>> = elements
        .iter()
        .enumerate()
        .map(|(i, element)| Blk {
            id: i as u64,
            element,
            spacing: None,
        })
        .collect();
    let mut paired = vec![false; elements.len()];
    let mut i = 0;
    while i < blocks.len() {
        let end = crate::write::group_end(&blocks, i);
        let printing: Vec<usize> = (i..end)
            .filter(|&k| elements[k].kind.is_printing())
            .collect();
        if printing.len() > 1 {
            for k in printing {
                paired[k] = elements[k].dual;
            }
        }
        i = end;
    }
    let mut out: Vec<Element> = Vec::new();
    let mut out_spacing = BTreeMap::new();
    let mut pending_break = false;
    for (i, e) in elements.iter().enumerate() {
        match e.kind {
            ElementKind::PageBreak => {
                pending_break = true;
                continue;
            }
            k if !k.is_printing() => continue,
            _ => {}
        }
        if pending_break && !out.is_empty() {
            out.push(Element::new(ElementKind::PageBreak, ""));
        }
        pending_break = false;
        let runs: Vec<Run> = e
            .text
            .runs()
            .iter()
            .filter_map(|r| match r {
                Run::Text { text, style, .. } => Some(Run::styled(
                    text.chars().filter(|&c| xml_char(c)).collect::<String>(),
                    *style,
                )),
                _ => None,
            })
            .collect();
        let mut n = Element::new(e.kind, RichText::new(runs));
        n.scene_number = e
            .scene_number
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        n.centered = e.centered;
        n.dual = paired[i];
        let sb = spacing.get(&i).copied().or_else(|| {
            e.space_before
                .map(|lines| Pt(format.line_height.0.saturating_mul(i32::from(lines))))
        });
        let usual = format.elements.get(e.kind).map(|f| f.space_before);
        if let Some(sb) = sb
            && (Pt::ZERO..=body).contains(&sb)
            && Some(sb) != usual
        {
            out_spacing.insert(out.len(), sb);
        }
        out.push(n);
    }
    // Empty general paragraphs at the end are dropped when read (§6.7),
    // and a break with nothing after it prints nothing.
    while let Some(last) = out.last() {
        if (last.kind == ElementKind::General && last.text.is_empty())
            || last.kind == ElementKind::PageBreak
        {
            out.pop();
        } else {
            break;
        }
    }
    let len = out.len();
    out_spacing.retain(|&k, _| k < len);
    (out, out_spacing)
}

/// Reads `data` twice and checks the reader's guarantees: it does not
/// panic, gives the same result both times, and a document it returns
/// satisfies the model rules (docs/specs/fdx.md §10).
pub fn read(data: &[u8]) -> Result<(), String> {
    let first = crate::read(data);
    let second = crate::read(data);
    if first != second {
        return Err("reading the same bytes twice gave different results".into());
    }
    match first {
        Ok(doc) => check(&doc),
        Err(_) => Ok(()),
    }
}

/// The rules every document the reader returns must satisfy.
fn check(doc: &FdxDoc) -> Result<(), String> {
    let problems = doc.format.problems();
    if !problems.is_empty() {
        return Err(format!("the format fails validation: {problems:?}"));
    }
    let elements = &doc.script.elements;
    for (i, e) in elements.iter().enumerate() {
        let p = e.problems();
        if !p.is_empty() {
            return Err(format!("element {i} breaks the model rules: {p:?}"));
        }
        if e.space_before.is_some() {
            return Err(format!("element {i} has space_before set"));
        }
    }
    if let Some(k) = doc.overrides.keys().find(|k| **k >= elements.len()) {
        return Err(format!("override for element {k}, past the end"));
    }
    // The format passed validation above, so its body height is sound.
    let body = doc.format.body_height();
    if let Some((k, v)) = doc
        .overrides
        .iter()
        .find(|(_, v)| **v < Pt::ZERO || **v > body)
    {
        return Err(format!(
            "override of {v} pt for element {k} is outside 0 to the body height ({body} pt)"
        ));
    }
    if let Some((n, l)) = doc
        .title_lines
        .iter()
        .enumerate()
        .find(|(_, l)| l.space_before < Pt::ZERO || l.space_before > body)
    {
        return Err(format!(
            "title line {n} has {} pt above, outside 0 to the body height ({body} pt)",
            l.space_before
        ));
    }
    for s in &doc.scenes {
        if elements.get(s.element).map(|e| e.kind) != Some(ElementKind::SceneHeading) {
            return Err(format!(
                "scene properties point at element {}, not a scene heading",
                s.element
            ));
        }
    }
    if elements
        .windows(2)
        .any(|w| w[0].kind == ElementKind::PageBreak && w[1].kind == ElementKind::PageBreak)
        || elements
            .first()
            .is_some_and(|e| e.kind == ElementKind::PageBreak)
    {
        return Err("page breaks must not lead or repeat".into());
    }
    if let Some(issue) = doc
        .report
        .iter()
        .find(|i| !crate::codes::ALL.contains(&i.code))
    {
        return Err(format!("unlisted report code {}", issue.code));
    }
    Ok(())
}

/// Reads `data`, edits its title lines as `ops` chooses (two bytes per
/// edit: which edit, and where), and writes it back with its store
/// (docs/specs/fdx.md §13.5). What was written must read with nothing to
/// repair as the same script with the edited lines, and every line the
/// edits left alone must be written as a paragraph of the file that reads
/// as that line.
pub fn title_edit(data: &[u8], ops: &[u8]) -> Result<(), String> {
    use coldopen_screenplay::{Align, TitleLine};

    let Ok(doc) = crate::read(data) else {
        return Ok(());
    };
    let title = |kept: &crate::Kept| {
        kept.sections.iter().find_map(|s| match s {
            crate::Section::TitlePage { content, .. } => crate::kept::title_parts(content),
            _ => None,
        })
    };
    let parts = title(&doc.kept).filter(|p| p.paragraphs.len() == doc.title_lines.len());
    let mut lines = doc.title_lines.clone();
    // Which line of the file each line is, while no edit has touched it.
    let mut origin: Vec<Option<usize>> = (0..lines.len()).map(Some).collect();
    let (left, right) = (
        doc.format.elements.action.left,
        doc.format.elements.action.right,
    );
    for (n, op) in ops.chunks(2).enumerate() {
        let (what, at) = (op[0] % 4, usize::from(*op.get(1).unwrap_or(&0)));
        if lines.is_empty() && what != 2 {
            continue;
        }
        let i = at % lines.len().max(1);
        match what {
            0 => {
                lines[i].text = RichText::plain(format!("Edited {n}"));
                origin[i] = None;
            }
            1 => {
                lines.remove(i);
                origin.remove(i);
            }
            2 => {
                let i = at % (lines.len() + 1);
                let line = TitleLine {
                    text: RichText::plain(format!("New {n}")),
                    align: Align::Center,
                    left,
                    right,
                    space_before: Pt::ZERO,
                };
                lines.insert(i, line);
                origin.insert(i, None);
            }
            _ => {
                lines[i].align = [Align::Left, Align::Center, Align::Right][at % 3];
                lines[i].space_before = Pt::points(i32::from(op[0] % 48));
                origin[i] = None;
            }
        }
    }
    let written = crate::write(&Doc {
        blocks: doc.blocks(),
        format: &doc.format,
        title_lines: &lines,
        kept: Some(&doc.kept),
        declare_utf8: matches!(
            doc.encoding.encoding,
            crate::Encoding::Windows1252 | crate::Encoding::Lossy
        ),
    });
    if parts.is_none() && written.losses.iter().any(|l| l.code == "title_page_kept") {
        return Ok(());
    }
    if let Some(l) = written.losses.first() {
        return Err(format!("editing title lines reports a loss: {}", l.code));
    }
    let again = crate::read(written.text.as_bytes())
        .map_err(|e| format!("what was written cannot be read: {e}"))?;
    if let Some(i) = again.report.iter().find(|i| i.is_repair()) {
        return Err(format!("what was written needs repair: {}", i.code));
    }
    if again.script != doc.script || again.format != doc.format {
        return Err("editing title lines changed the script".into());
    }
    if again.title_lines != lines {
        return Err("what was written does not read as the edited title lines".into());
    }
    // An edit that made a line equal to one of the file's may pair it with
    // that line in place of one left alone, which is still right.
    let Some(parts) = parts else {
        return Ok(());
    };
    let made_equal = lines
        .iter()
        .zip(&origin)
        .any(|(l, o)| o.is_none() && doc.title_lines.contains(l));
    if made_equal {
        return Ok(());
    }
    let out = title(&again.kept).ok_or("what was written has no title page to cut")?;
    for (j, (line, o)) in lines.iter().zip(&origin).enumerate() {
        if o.is_none() {
            continue;
        }
        let verbatim = doc
            .title_lines
            .iter()
            .zip(&parts.paragraphs)
            .any(|(read, p)| read == line && out.paragraphs.get(j) == Some(p));
        if !verbatim {
            return Err(format!(
                "title line {j}, which no edit touched, is not written as the file had it"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_spaces_outside_the_body() {
        let doc = crate::read(
            b"<FinalDraft><Content><Paragraph Type=\"Action\"/></Content>\
              <TitlePage><Content><Paragraph/></Content></TitlePage></FinalDraft>",
        )
        .unwrap();
        check(&doc).unwrap();
        let body = doc.format.body_height();
        for (v, ok) in [
            (Pt::ZERO, true),
            (body, true),
            (Pt(-1), false),
            (Pt(body.0 + 1), false),
            (Pt(i32::MAX), false),
            (Pt(i32::MIN), false),
        ] {
            let mut d = doc.clone();
            d.overrides.insert(0, v);
            assert_eq!(check(&d).is_ok(), ok, "override {v:?}");
            let mut d = doc.clone();
            d.title_lines[0].space_before = v;
            assert_eq!(check(&d).is_ok(), ok, "title line {v:?}");
        }
    }
}
