//! Reading and comparing page-view text, for `self-check`.

/// Marks a page header. Anything after it on the line is ignored, because
/// the layout engine's headers carry extra text after the page number.
pub const HEADER: &str = "=== page ";

/// A page view split into pages. Lines before the first header, if any,
/// are kept apart so they show up as a difference.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageView {
    pub preamble: Vec<String>,
    pub pages: Vec<Vec<String>>,
}

pub fn parse(text: &str) -> PageView {
    let mut pv = PageView::default();
    if text.is_empty() {
        return pv;
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    for line in body.split('\n') {
        if line.starts_with(HEADER) {
            pv.pages.push(Vec::new());
        } else if let Some(p) = pv.pages.last_mut() {
            p.push(line.to_string());
        } else {
            pv.preamble.push(line.to_string());
        }
    }
    pv
}

/// The first place two page views differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// 1-based page, or 0 for lines before the first header.
    pub page: usize,
    /// 0-based row within the page.
    pub row: usize,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

/// Compares the PDF's grid (`ours`) with a page-view file (`theirs`).
pub fn compare(ours: &PageView, theirs: &PageView) -> Option<Difference> {
    if let Some(d) = compare_rows(0, &ours.preamble, &theirs.preamble) {
        return Some(d);
    }
    let empty = Vec::new();
    for i in 0..ours.pages.len().max(theirs.pages.len()) {
        match (ours.pages.get(i), theirs.pages.get(i)) {
            (Some(a), Some(b)) => {
                if let Some(d) = compare_rows(i + 1, a, b) {
                    return Some(d);
                }
            }
            (a, b) => {
                // One side has no such page: report its first row (or the
                // header itself when the page is empty).
                let a = a.unwrap_or(&empty);
                let b = b.unwrap_or(&empty);
                return Some(Difference {
                    page: i + 1,
                    row: 0,
                    ours: if ours.pages.len() > i {
                        Some(a.first().cloned().unwrap_or_default())
                    } else {
                        None
                    },
                    theirs: if theirs.pages.len() > i {
                        Some(b.first().cloned().unwrap_or_default())
                    } else {
                        None
                    },
                });
            }
        }
    }
    None
}

fn compare_rows(page: usize, a: &[String], b: &[String]) -> Option<Difference> {
    (0..a.len().max(b.len())).find_map(|r| {
        let (x, y) = (a.get(r), b.get(r));
        (x != y).then(|| Difference {
            page,
            row: r,
            ours: x.cloned(),
            theirs: y.cloned(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_split_pages_and_extra_header_text_is_ignored() {
        let a = parse("=== page 1 ===\nA\n\nB\n=== page 2 ===\n");
        let b = parse("=== page 1 (12 lines, ends in dialogue) ===\nA\n\nB\n=== page 2 \n");
        assert_eq!(
            a.pages,
            vec![
                vec!["A".to_string(), String::new(), "B".to_string()],
                vec![]
            ]
        );
        assert_eq!(compare(&a, &b), None);
        assert_eq!(parse(""), PageView::default());
    }

    #[test]
    fn the_first_difference_is_reported() {
        let a = parse("=== page 1 ===\nA\n=== page 2 ===\nX\nY\n");
        let b = parse("=== page 1 ===\nA\n=== page 2 ===\nX\nZ\n");
        let d = compare(&a, &b).unwrap();
        assert_eq!((d.page, d.row), (2, 1));
        assert_eq!(
            (d.ours.as_deref(), d.theirs.as_deref()),
            (Some("Y"), Some("Z"))
        );
        // A missing row and a missing page.
        let c = parse("=== page 1 ===\nA\n=== page 2 ===\nX\n");
        let d = compare(&a, &c).unwrap();
        assert_eq!((d.page, d.row, d.theirs), (2, 1, None));
        let e = parse("=== page 1 ===\nA\n");
        let d = compare(&a, &e).unwrap();
        assert_eq!((d.page, d.ours.as_deref(), d.theirs), (2, Some("X"), None));
        // Trailing whitespace and CR are differences, not noise.
        assert!(
            compare(
                &parse("=== page 1 ===\nA\n"),
                &parse("=== page 1 ===\nA \n")
            )
            .is_some()
        );
        assert!(
            compare(
                &parse("=== page 1 ===\nA\n"),
                &parse("=== page 1 ===\r\nA\r\n")
            )
            .is_some()
        );
        // Text before the first header.
        let d = compare(&parse("=== page 1 ===\n"), &parse("junk\n=== page 1 ===\n")).unwrap();
        assert_eq!(d.page, 0);
    }
}
