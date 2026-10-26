//! The readable summary (docs/specs/diff.md §4.2): one line for the whole,
//! one for the title page, one per scene. No script text but headings.

use coldopen_screenplay::ElementKind;

use crate::{BlockOp, BlockOpKind, Diff, SceneOp, SceneOpKind, TitleOpKind, UnitRef};

/// What the summary calls a block of `kind`, for `n` of them.
pub fn kind_noun(kind: ElementKind, n: usize) -> &'static str {
    let (one, many) = match kind {
        ElementKind::SceneHeading => ("heading", "headings"),
        ElementKind::Action => ("action", "actions"),
        ElementKind::Character => ("character name", "character names"),
        ElementKind::Parenthetical => ("parenthetical", "parentheticals"),
        ElementKind::Dialogue => ("speech", "speeches"),
        ElementKind::Transition => ("transition", "transitions"),
        ElementKind::Lyric => ("lyric", "lyrics"),
        ElementKind::Shot => ("shot", "shots"),
        ElementKind::General => ("general line", "general lines"),
        ElementKind::PageBreak => ("page break", "page breaks"),
        ElementKind::Section => ("section", "sections"),
        ElementKind::Synopsis => ("synopsis", "synopses"),
        ElementKind::Note => ("note", "notes"),
        ElementKind::Boneyard => ("boneyard block", "boneyard blocks"),
    };
    if n == 1 { one } else { many }
}

fn op_word(op: BlockOpKind) -> &'static str {
    match op {
        BlockOpKind::Changed => "changed",
        BlockOpKind::Added => "added",
        BlockOpKind::Deleted => "deleted",
        BlockOpKind::Retyped => "retyped",
        BlockOpKind::Restyled => "restyled",
        BlockOpKind::Attributes => "with other attributes",
    }
}

/// "1 speech changed, 2 actions added": the blocks by op and kind, in the
/// order each first appears.
fn blocks_phrase(blocks: &[BlockOp]) -> String {
    let mut groups: Vec<((BlockOpKind, ElementKind), usize)> = Vec::new();
    for b in blocks {
        let kind =
            b.b.as_ref()
                .or(b.a.as_ref())
                .map_or(ElementKind::Action, |r| r.kind);
        match groups.iter_mut().find(|(k, _)| *k == (b.op, kind)) {
            Some((_, n)) => *n += 1,
            None => groups.push(((b.op, kind), 1)),
        }
    }
    groups
        .iter()
        .map(|((op, kind), n)| format!("{n} {} {}", kind_noun(*kind, *n), op_word(*op)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// "Scene 15 (was 12) INT. LAB - NIGHT", or "The opening".
fn label(number: String, u: &UnitRef) -> String {
    if u.index == 0 {
        "The opening".into()
    } else if u.heading.is_empty() {
        number
    } else {
        format!("{number} {}", u.heading)
    }
}

fn scene_line(s: &SceneOp) -> String {
    let what = blocks_phrase(&s.blocks);
    match (s.op, &s.a, &s.b) {
        (SceneOpKind::Added, _, Some(b)) => {
            format!("{}: added.", label(format!("Scene {}", b.index), b))
        }
        (SceneOpKind::Deleted, Some(a), _) => {
            format!("{}: deleted.", label(format!("Was scene {}", a.index), a))
        }
        (op, Some(a), Some(b)) => {
            let number = if a.index == b.index {
                format!("Scene {}", b.index)
            } else {
                format!("Scene {} (was {})", b.index, a.index)
            };
            let mut parts = Vec::new();
            if op == SceneOpKind::Moved {
                parts.push(match s.after {
                    Some(Some(p)) => format!("moved after scene {p}"),
                    _ => "moved to the start".to_string(),
                });
            }
            if !what.is_empty() {
                parts.push(what);
            }
            format!("{}: {}.", label(number, b), parts.join("; "))
        }
        _ => String::new(),
    }
}

/// The summary of `diff` between drafts named `a` and `b`.
pub fn summary_text(diff: &Diff, a: &str, b: &str) -> String {
    let c = &diff.summary.scenes;
    let mut counts = Vec::new();
    for (n, word) in [
        (c.changed, "changed"),
        (c.moved, "moved"),
        (c.added, "added"),
        (c.deleted, "deleted"),
    ] {
        if n > 0 {
            if counts.is_empty() {
                let noun = if n == 1 { "scene" } else { "scenes" };
                counts.push(format!("{n} {noun} {word}"));
            } else {
                counts.push(format!("{n} {word}"));
            }
        }
    }
    let mut head = counts.join(", ");
    if !diff.title_page.is_empty() {
        if head.is_empty() {
            head = "the title page changed".into();
        } else {
            head.push_str("; the title page changed");
        }
    }
    if head.is_empty() {
        head = "no differences".into();
    }
    let mut out = format!("{a} \u{2192} {b}: {head}.\n");
    if !diff.title_page.is_empty() {
        let mut items: Vec<String> = Vec::new();
        let mut lines: Vec<(TitleOpKind, usize)> = Vec::new();
        for t in &diff.title_page {
            match &t.key {
                Some(k) => items.push(format!(
                    "{k} {}",
                    match t.op {
                        TitleOpKind::Changed => "changed",
                        TitleOpKind::Added => "added",
                        TitleOpKind::Deleted => "deleted",
                    }
                )),
                None => match lines.iter_mut().find(|(op, _)| *op == t.op) {
                    Some((_, n)) => *n += 1,
                    None => lines.push((t.op, 1)),
                },
            }
        }
        for (op, n) in lines {
            let word = match op {
                TitleOpKind::Changed => "changed",
                TitleOpKind::Added => "added",
                TitleOpKind::Deleted => "deleted",
            };
            items.push(format!(
                "{n} {} {word}",
                if n == 1 { "line" } else { "lines" }
            ));
        }
        out.push_str(&format!("Title page: {}.\n", items.join(", ")));
    }
    for s in &diff.scenes {
        out.push_str(&scene_line(s));
        out.push('\n');
    }
    out
}
