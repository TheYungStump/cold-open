//! A draft cut into units (docs/specs/diff.md §3.1): the opening before the
//! first scene heading, when there is one, then one unit per heading with a
//! setting.

use coldopen_screenplay::names::{Setting, heading};
use coldopen_screenplay::{Element, ElementKind};

/// One unit: its index (the opening is 0, scenes count from 1) and its
/// elements' indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Unit {
    pub index: usize,
    pub blocks: Vec<usize>,
}

/// The units of a draft. Elements left out of the comparison (§2) belong
/// to none; sections and synopses that are compared belong to the unit
/// they fall in.
pub(crate) fn units(els: &[Element], transparent: bool) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    let mut scenes = 0;
    for (i, e) in els.iter().enumerate() {
        if crate::left_out(e.kind, transparent) {
            continue;
        }
        // A heading with no setting (a forced heading, or a Shot as
        // Fountain writes it) stays a block of the unit before it (§3.1).
        let starts = e.kind == ElementKind::SceneHeading
            && heading(&e.text.plain_text()).setting != Setting::None;
        if starts {
            scenes += 1;
            out.push(Unit {
                index: scenes,
                blocks: vec![i],
            });
        } else if let Some(u) = out.last_mut() {
            u.blocks.push(i);
        } else {
            out.push(Unit {
                index: 0,
                blocks: vec![i],
            });
        }
    }
    out
}
