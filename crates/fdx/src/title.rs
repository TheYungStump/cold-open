//! The title page as positioned paragraphs (docs/specs/fdx.md §7).

use coldopen_screenplay::{Align, PageFormat, Pt, RichText, TitleLine};

use crate::content::{ignored_element, runs};
use crate::report::{Report, codes, plural};
use crate::settings::Spec;
use crate::values::FdxAlign;
use crate::xml::Node;

pub(crate) fn read(root: &Node, format: &PageFormat, r: &mut Report) -> Vec<TitleLine> {
    let Some(content) = root.child("TitlePage").and_then(|t| t.child("Content")) else {
        return Vec::new();
    };
    let column = (format.elements.action.left, format.elements.action.right);
    // No title paragraph may have more space above it than a page body
    // holds (§7).
    let body = format.body_height();
    let mut lines = Vec::new();
    for (n, p) in content.children_named("Paragraph").enumerate() {
        for c in p
            .elements()
            .filter(|c| c.name != "Text" && c.name != "Tabstops")
        {
            ignored_element(r, "a title-page <Paragraph>", &c.name, None);
        }
        let what = format!("title page paragraph {n}");
        let text = RichText::new(runs(p, None, r));
        let spec = Spec::parse(p, &what, None, r);
        let align = match spec.align {
            Some(FdxAlign::Full) => {
                r.tally(codes::ALIGNMENT_FULL, "title page", None, |_, n| {
                    format!(
                        "{} Full alignment, read as left",
                        plural(n, "title-page paragraph uses", "title-page paragraphs use")
                    )
                });
                Align::Left
            }
            Some(a) => a.to_align(),
            None => Align::Left,
        };
        let mut left = spec.left.unwrap_or(column.0);
        let mut right = spec.right.unwrap_or(column.1);
        if left < Pt::ZERO || right > format.page_width || left >= right {
            r.push(
                codes::INVALID_VALUE,
                format!(
                    "{what} runs from {left} pt to {right} pt, which is not a column on the page; the action column is used"
                ),
                None,
            );
            (left, right) = column;
        }
        let mut space_before = spec.space_before.unwrap_or(Pt::ZERO);
        if space_before < Pt::ZERO {
            r.push(
                codes::INVALID_VALUE,
                format!("{what} has negative space above; 0 is used"),
                None,
            );
            space_before = Pt::ZERO;
        } else if space_before > body {
            r.push(
                codes::INVALID_VALUE,
                format!(
                    "{what} has {space_before} pt above, more than the body height; {body} pt is used"
                ),
                None,
            );
            space_before = body;
        }
        let unexpressed = spec.first_indent.is_some_and(|f| f != Pt::ZERO)
            || spec.spacing.is_some_and(|s| s != 100)
            || spec
                .leading
                .as_deref()
                .is_some_and(|l| !l.eq_ignore_ascii_case("Regular"))
            || spec.starts_new_page == Some(true);
        if unexpressed {
            r.tally(codes::TITLE_FORMAT_IGNORED, "", None, |_, n| {
                format!(
                    "{} a first-line indent, line spacing, leading or page break that is ignored",
                    plural(n, "title-page paragraph has", "title-page paragraphs have")
                )
            });
        }
        lines.push(TitleLine {
            text,
            align,
            left,
            right,
            space_before,
        });
    }
    lines
}
