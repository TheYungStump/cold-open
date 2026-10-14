//! Checks shared by the cargo-fuzz target in `fuzz/` and the stable test
//! suite, so the fuzzed properties also run on every CI build.

use coldopen_screenplay::{ElementKind, Pt};

use crate::FdxDoc;

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
