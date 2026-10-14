//! Document settings into a `PageFormat` (docs/specs/fdx.md §5).

use std::collections::{BTreeMap, BTreeSet};

// The longest page-number suffix and (MORE)/(CONT'D) string a format may
// carry, as `PageFormat::problems` enforces them.
use coldopen_screenplay::{
    ElementFormat, ElementKind, MAX_INSERT, MAX_PAGE_SUFFIX, PageFormat, Pt, SceneContinueds,
};

use crate::report::{Report, codes, plural};
use crate::values::{
    At, FdxAlign, alignment, hundredths, inches, kind_of_type, points, points_rounding,
    style_tokens, type_of_kind, yes_no,
};
use crate::xml::Node;

/// Top-level sections the reader uses (docs/specs/fdx.md §3).
const SECTIONS: [&str; 6] = [
    "Content",
    "PageLayout",
    "MoresAndContinueds",
    "SceneNumberOptions",
    "HeaderAndFooter",
    "TitlePage",
];

/// Sections whose absence leaves page settings at their defaults.
const SETTINGS_SECTIONS: [&str; 4] = [
    "PageLayout",
    "MoresAndContinueds",
    "SceneNumberOptions",
    "HeaderAndFooter",
];

/// The paragraph attributes shared by `ParagraphSpec`, `Content` paragraphs
/// and title-page paragraphs, as parsed values. `None` means absent or
/// unparseable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Spec {
    pub(crate) align: Option<FdxAlign>,
    pub(crate) first_indent: Option<Pt>,
    pub(crate) leading: Option<String>,
    pub(crate) left: Option<Pt>,
    pub(crate) right: Option<Pt>,
    pub(crate) space_before: Option<Pt>,
    /// Hundredths: single spacing is 100.
    pub(crate) spacing: Option<i32>,
    pub(crate) starts_new_page: Option<bool>,
}

impl Spec {
    /// Reads the eight paragraph attributes of `node`, reporting values
    /// that do not parse.
    pub(crate) fn parse(
        node: &Node,
        prefix: &str,
        paragraph: Option<usize>,
        r: &mut Report,
    ) -> Spec {
        let mut s = Spec::default();
        for (key, value) in &node.attrs {
            let what = format!("{prefix} {key}");
            let at = At {
                what: &what,
                paragraph,
            };
            match key.as_str() {
                "Alignment" => s.align = alignment(value, &at, r),
                "FirstIndent" => s.first_indent = inches(value, &at, r),
                "Leading" => s.leading = Some(value.clone()),
                "LeftIndent" => s.left = inches(value, &at, r),
                "RightIndent" => s.right = inches(value, &at, r),
                "SpaceBefore" => s.space_before = points(value, &at, r),
                "Spacing" => s.spacing = hundredths(value, &at, r),
                "StartsNewPage" => s.starts_new_page = yes_no(value, &at, r),
                _ => {}
            }
        }
        s
    }

    /// Every attribute as a format of ours implies it.
    pub(crate) fn from_format(f: &ElementFormat) -> Spec {
        Spec {
            align: Some(FdxAlign::from_align(f.align)),
            first_indent: Some(f.first_indent),
            leading: Some("Regular".into()),
            left: Some(f.left),
            right: Some(f.right),
            space_before: Some(f.space_before),
            spacing: Some(100),
            starts_new_page: Some(f.starts_new_page),
        }
    }

    /// `self`, with absent attributes taken from `base`.
    fn or(&self, base: &Spec) -> Spec {
        Spec {
            align: self.align.or(base.align),
            first_indent: self.first_indent.or(base.first_indent),
            leading: self.leading.clone().or_else(|| base.leading.clone()),
            left: self.left.or(base.left),
            right: self.right.or(base.right),
            space_before: self.space_before.or(base.space_before),
            spacing: self.spacing.or(base.spacing),
            starts_new_page: self.starts_new_page.or(base.starts_new_page),
        }
    }
}

/// The page format plus every type's paragraph settings, which serve as
/// the baseline for inline paragraph attributes (docs/specs/fdx.md §6.4).
pub(crate) struct Settings {
    pub(crate) format: PageFormat,
    specs: Specs,
    defaults: PageFormat,
}

/// Every type's paragraph settings in file order, indexed by the type name
/// as written and by its trimmed, ASCII-lowercased form, so a paragraph
/// finds its baseline in O(log n) however many types the file has.
#[derive(Default)]
struct Specs {
    specs: Vec<Spec>,
    /// The first settings for each exact name.
    exact: BTreeMap<String, usize>,
    /// The first settings for each folded name.
    folded: BTreeMap<String, usize>,
}

impl Specs {
    fn push(&mut self, type_name: &str, spec: Spec) {
        let n = self.specs.len();
        self.specs.push(spec);
        self.exact.entry(type_name.to_string()).or_insert(n);
        self.folded.entry(fold(type_name)).or_insert(n);
    }
}

/// A type name as `spec_for` compares it when no name matches exactly.
fn fold(type_name: &str) -> String {
    type_name.trim().to_ascii_lowercase()
}

impl Settings {
    /// The settings of a paragraph type: matched exactly, then ignoring
    /// ASCII case and surrounding spaces. The first match in file order
    /// wins.
    pub(crate) fn spec_for(&self, type_name: &str) -> Option<&Spec> {
        let s = &self.specs;
        s.exact
            .get(type_name)
            .or_else(|| s.folded.get(&fold(type_name)))
            .and_then(|&n| s.specs.get(n))
    }

    /// What a paragraph's inline attributes are compared with.
    pub(crate) fn baseline(&self, type_name: &str, kind: ElementKind) -> Spec {
        let base = self
            .defaults
            .elements
            .get(kind)
            .map(Spec::from_format)
            .unwrap_or_default();
        match self.spec_for(type_name) {
            Some(s) => s.or(&base),
            None => base,
        }
    }
}

pub(crate) fn read(root: &Node, r: &mut Report) -> Settings {
    scan_sections(root, r);
    let mut format = PageFormat::default();
    let specs = element_settings(root, &mut format, r);
    if let Some(pl) = root.child("PageLayout") {
        page_layout(pl, &mut format, r);
    }
    if let Some(mc) = root.child("MoresAndContinueds") {
        mores(mc, &mut format, r);
    }
    if let Some(sn) = root.child("SceneNumberOptions") {
        scene_numbers(sn, &mut format, r);
    }
    if let Some(hf) = root.child("HeaderAndFooter") {
        header(hf, &mut format, r);
    }
    let problems = format.problems();
    if !problems.is_empty() {
        r.push(
            codes::INVALID_FORMAT,
            format!(
                "the page format in the file is not usable ({}); the default format is used",
                problems.join("; ")
            ),
            None,
        );
        format = PageFormat::default();
    }
    Settings {
        format,
        specs,
        defaults: PageFormat::default(),
    }
}

/// Reports sections we don't read, repeated sections and missing ones.
fn scan_sections(root: &Node, r: &mut Report) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    // In order of first occurrence, with a set for the lookups.
    let mut ignored: Vec<&str> = Vec::new();
    let mut ignored_set: BTreeSet<&str> = BTreeSet::new();
    for n in root.elements() {
        let name = n.name.as_str();
        if name == "ElementSettings" {
            continue;
        }
        if SECTIONS.contains(&name) {
            if !seen.insert(name) {
                r.push(
                    codes::DUPLICATE_SECTION,
                    format!("a second {name} section is ignored; the first is used"),
                    None,
                );
            }
        } else if ignored_set.insert(name) {
            ignored.push(name);
        }
    }
    if !ignored.is_empty() {
        r.push(
            codes::SECTIONS_IGNORED,
            format!(
                "{} not read: {}",
                plural(ignored.len(), "section is", "sections are"),
                ignored.join(", ")
            ),
            None,
        );
    }
    let missing: Vec<&str> = SETTINGS_SECTIONS
        .iter()
        .copied()
        .filter(|s| !seen.contains(s))
        .collect();
    if !missing.is_empty() {
        r.push(
            codes::SECTION_MISSING,
            format!(
                "the file has no {}; those settings keep their defaults",
                missing.join(", ")
            ),
            None,
        );
    }
}

/// `a * num / den`, rounded half away from zero, or `None` when the result
/// does not fit a `Pt` or `den` is not positive. Works in i128, where no
/// i32 inputs can overflow.
fn mul_div(a: Pt, num: i32, den: i32) -> Option<Pt> {
    if den <= 0 {
        return None;
    }
    let p = i128::from(a.0) * i128::from(num);
    let d = i128::from(den);
    let q = (p.abs() * 2 + d) / (d * 2);
    i32::try_from(if p < 0 { -q } else { q }).ok().map(Pt)
}

fn element_settings(root: &Node, format: &mut PageFormat, r: &mut Report) -> Specs {
    let mut specs = Specs::default();
    let mut present: BTreeSet<ElementKind> = BTreeSet::new();
    let mut unmodelled: Vec<String> = Vec::new();
    let mut fonts: Vec<&'static str> = Vec::new();
    for es in root.children_named("ElementSettings") {
        let type_name = es.attr("Type").unwrap_or("");
        let kind = kind_of_type(type_name);
        // A modelled kind repeats under any spelling; another type only
        // under the same name. No modelled name is ever an unmodelled one.
        let duplicate = match kind {
            Some(k) => present.contains(&k),
            None => specs.exact.contains_key(type_name),
        };
        if duplicate {
            r.push(
                codes::DUPLICATE_SETTINGS,
                format!("repeated settings for \"{type_name}\" are ignored; the first are used"),
                None,
            );
            continue;
        }
        let prefix = format!("ElementSettings \"{type_name}\"");
        let Some(kind) = kind else {
            // Kept as a baseline for inline attributes, never applied, so
            // its values are parsed without reports.
            let spec = es
                .child("ParagraphSpec")
                .map(|ps| Spec::parse(ps, &prefix, None, &mut Report::default()))
                .unwrap_or_default();
            specs.push(type_name, spec);
            unmodelled.push(type_name.to_string());
            continue;
        };
        present.insert(kind);
        let spec = es
            .child("ParagraphSpec")
            .map(|ps| Spec::parse(ps, &prefix, None, r))
            .unwrap_or_default();
        let Some(f) = format.elements.get_mut(kind) else {
            continue;
        };
        apply_spec(&spec, f, &prefix, r);
        if let Some(fs) = es.child("FontSpec") {
            if let Some(style) = fs.attr("Style") {
                let t = style_tokens(style);
                f.caps = t.all_caps;
                f.bold = t.style.bold;
                f.italic = t.style.italic;
                f.underline = t.style.underline;
                for token in &t.unknown {
                    r.tally(codes::UNKNOWN_STYLE, token, None, describe_unknown_style);
                }
            }
            let font_ok = fs
                .attr("Font")
                .is_none_or(|font| font.to_ascii_lowercase().contains("courier"));
            let size_ok = fs
                .attr("Size")
                .is_none_or(|s| points_rounding(s).map(|(p, _)| p) == Some(Pt::points(12)));
            if !(font_ok && size_ok) {
                fonts.push(type_of_kind(kind));
            }
        }
        if let Some(paginate) = es.child("Behavior").and_then(|b| b.attr("PaginateAs")) {
            match kind_of_type(paginate) {
                Some(p) => f.paginate_as = p,
                None => {
                    r.push(
                        codes::UNKNOWN_PAGINATE_AS,
                        format!(
                            "{prefix} paginates as \"{paginate}\", which we don't model; it keeps its own rules"
                        ),
                        None,
                    );
                    f.paginate_as = kind;
                }
            }
        }
        specs.push(type_name, spec);
    }
    if !unmodelled.is_empty() {
        r.push(
            codes::SETTINGS_FOR_UNMODELLED_TYPE,
            format!(
                "settings for types we don't model are ignored: {}",
                unmodelled
                    .iter()
                    .map(|t| format!("\"{t}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
        );
    }
    let missing: Vec<&str> = format
        .elements
        .iter()
        .map(|(k, _)| k)
        .filter(|k| !present.contains(k))
        .map(type_of_kind)
        .collect();
    if !missing.is_empty() {
        r.push(
            codes::SETTINGS_MISSING,
            format!(
                "the file has no settings for {}; they keep their defaults",
                missing.join(", ")
            ),
            None,
        );
    }
    if !fonts.is_empty() {
        r.push(
            codes::SETTINGS_FONT_IGNORED,
            format!(
                "the font settings of {} are not Courier at 12 pt; Courier Prime at the format's size is used",
                fonts.join(", ")
            ),
            None,
        );
    }
    specs
}

fn apply_spec(spec: &Spec, f: &mut ElementFormat, prefix: &str, r: &mut Report) {
    if let Some(v) = spec.left {
        f.left = v;
    }
    if let Some(v) = spec.right {
        f.right = v;
    }
    if let Some(v) = spec.first_indent {
        f.first_indent = v;
    }
    if let Some(a) = spec.align {
        if a == FdxAlign::Full {
            r.push(
                codes::ALIGNMENT_FULL,
                format!("{prefix} uses Full alignment, read as left"),
                None,
            );
        }
        f.align = a.to_align();
    }
    let space = spec.space_before.unwrap_or(f.space_before);
    match spec.spacing {
        Some(s) if s != 100 => match mul_div(space, s, 100) {
            Some(scaled) => {
                r.push(
                    codes::LINE_SPACING,
                    format!(
                        "{prefix} has Spacing {}, which we don't model; its space above becomes {scaled} pt",
                        Pt(s)
                    ),
                    None,
                );
                f.space_before = scaled;
            }
            None => {
                r.push(
                    codes::INVALID_VALUE,
                    format!(
                        "{prefix} has Spacing {}, which puts its space above out of range; {space} pt is kept",
                        Pt(s)
                    ),
                    None,
                );
                f.space_before = space;
            }
        },
        _ => f.space_before = space,
    }
    if let Some(v) = spec.starts_new_page {
        f.starts_new_page = v;
    }
}

fn page_layout(pl: &Node, format: &mut PageFormat, r: &mut Report) {
    if let Some(v) = pl.attr("TopMargin")
        && let Some(p) = points(v, &At::new("PageLayout TopMargin"), r)
    {
        format.margin_top = p;
    }
    if let Some(v) = pl.attr("BottomMargin")
        && let Some(p) = points(v, &At::new("PageLayout BottomMargin"), r)
    {
        format.margin_bottom = p;
    }
    if let Some(v) = pl.attr("HeaderMargin")
        && let Some(p) = points(v, &At::new("PageLayout HeaderMargin"), r)
    {
        format.header_top = p;
    }
    if let Some(v) = pl.attr("BreakDialogueAndActionAtSentences") {
        let at = At::new("PageLayout BreakDialogueAndActionAtSentences");
        if let Some(b) = yes_no(v, &at, r) {
            format.break_at_sentences = b;
        }
    }
    if let Some(v) = pl.attr("DocumentLeading")
        && v != "Normal"
    {
        r.push(
            codes::DOCUMENT_LEADING,
            format!("DocumentLeading \"{v}\" is not modelled; lines keep the format's height"),
            None,
        );
    }
    if let Some(size) = pl.child("PageSize") {
        if let Some(v) = size.attr("Width")
            && let Some(p) = inches(v, &At::new("PageSize Width"), r)
        {
            format.page_width = p;
        }
        if let Some(v) = size.attr("Height")
            && let Some(p) = inches(v, &At::new("PageSize Height"), r)
        {
            format.page_height = p;
        }
    }
}

/// `s` cut to its first line and then to at most `max` characters, and
/// whether anything was cut (docs/specs/fdx.md §5.1, §5.3).
fn one_line(s: &str, max: usize) -> (&str, bool) {
    let line = s.split(['\n', '\r']).next().unwrap_or("");
    let end = line.char_indices().nth(max).map_or(line.len(), |(i, _)| i);
    (&line[..end], end < s.len())
}

/// An insert string such as `(MORE)`, verbatim, cut to the format's bounds
/// when it is longer, or `None` when it is unusable (docs/specs/fdx.md
/// §5.1). Either way the format stays valid.
fn insert_string(node: &Node, key: &str, r: &mut Report) -> Option<String> {
    let v = node.attr(key)?;
    let (kept, cut) = one_line(v, MAX_INSERT);
    if kept.trim().is_empty() {
        r.push(
            codes::INVALID_VALUE,
            format!("{key} has no text on its first line, so it is not a usable insert; the default is kept"),
            None,
        );
        return None;
    }
    if cut {
        r.push(
            codes::INVALID_VALUE,
            format!(
                "{key} is longer than one line of {MAX_INSERT} characters; it is cut to \"{kept}\""
            ),
            None,
        );
    }
    Some(kept.to_string())
}

fn mores(mc: &Node, format: &mut PageFormat, r: &mut Report) {
    let defaults = PageFormat::default();
    if let Some(db) = mc.child("DialogueBreaks") {
        if let Some(v) = db.attr("BottomOfPage")
            && let Some(on) = yes_no(v, &At::new("DialogueBreaks BottomOfPage"), r)
        {
            format.more = if on {
                insert_string(db, "DialogueBottom", r).or(defaults.more.clone())
            } else {
                None
            };
        }
        if let Some(v) = db.attr("TopOfNext")
            && let Some(on) = yes_no(v, &At::new("DialogueBreaks TopOfNext"), r)
        {
            format.contd = if on {
                insert_string(db, "DialogueTop", r).or(defaults.contd.clone())
            } else {
                None
            };
        }
        if let Some(v) = db.attr("AutomaticCharacterContinueds") {
            let at = At::new("DialogueBreaks AutomaticCharacterContinueds");
            if let Some(on) = yes_no(v, &at, r) {
                format.contd_after_action = on;
            }
        }
    }
    if let Some(sb) = mc.child("SceneBreaks") {
        let mut flag = |key: &str| {
            sb.attr(key)
                .and_then(|v| yes_no(v, &At::new(&format!("SceneBreaks {key}")), r))
                .unwrap_or(false)
        };
        let bottom = flag("SceneBottomOfPage");
        let top = flag("SceneTopOfNext");
        let numbered = flag("ContinuedNumber");
        if bottom || top {
            format.scene_continueds = Some(SceneContinueds {
                bottom: bottom
                    .then(|| sb.attr("SceneBottom").map(str::to_string))
                    .flatten(),
                top: top
                    .then(|| sb.attr("SceneTop").map(str::to_string))
                    .flatten(),
            });
            let numbering = if numbered {
                ", with scene numbers,"
            } else {
                ""
            };
            r.push(
                codes::SCENE_CONTINUEDS_UNSUPPORTED,
                format!(
                    "scene continueds{numbering} are switched on; they are stored but not printed yet"
                ),
                None,
            );
        }
    }
}

fn scene_numbers(sn: &Node, format: &mut PageFormat, r: &mut Report) {
    if let Some(v) = sn.attr("LeftLocation")
        && let Some(p) = inches(v, &At::new("SceneNumberOptions LeftLocation"), r)
    {
        format.scene_number_left = p;
    }
    if let Some(v) = sn.attr("RightLocation")
        && let Some(p) = inches(v, &At::new("SceneNumberOptions RightLocation"), r)
    {
        format.scene_number_right = p;
    }
    let mut hidden = Vec::new();
    for (key, side) in [
        ("ShowNumbersOnLeft", "left"),
        ("ShowNumbersOnRight", "right"),
    ] {
        if let Some(v) = sn.attr(key) {
            let what = format!("SceneNumberOptions {key}");
            if yes_no(v, &At::new(&what), r) == Some(false) {
                hidden.push(side);
            }
        }
    }
    if !hidden.is_empty() {
        r.push(
            codes::SCENE_NUMBER_SIDE,
            format!(
                "scene numbers are hidden on the {} side; ours print on both sides when they print",
                hidden.join(" and ")
            ),
            None,
        );
    }
}

/// What a header or footer paragraph holds: whether it has the page-number
/// label, the suffix text right after it, and everything else.
#[derive(Default)]
struct HeaderParts {
    page_label: bool,
    suffix: Option<String>,
    /// In order of first occurrence.
    others: Vec<String>,
    others_seen: BTreeSet<String>,
}

fn add_other(parts: &mut HeaderParts, what: String) {
    if parts.others_seen.insert(what.clone()) {
        parts.others.push(what);
    }
}

fn header_parts(section: Option<&Node>) -> HeaderParts {
    let mut parts = HeaderParts::default();
    let Some(section) = section else {
        return parts;
    };
    for p in section.children_named("Paragraph") {
        let mut after_label = false;
        for c in p.elements() {
            match c.name.as_str() {
                "DynamicLabel" => {
                    let t = c.attr("Type").unwrap_or("");
                    if t == "Page #" && !parts.page_label {
                        parts.page_label = true;
                        parts.suffix = Some(String::new());
                        after_label = true;
                    } else {
                        add_other(&mut parts, format!("the \"{t}\" label"));
                        after_label = false;
                    }
                }
                "Text" => {
                    let text = c.text();
                    let rest = if after_label {
                        let end = text.find(['\t', '\n']).unwrap_or(text.len());
                        if let Some(s) = parts.suffix.as_mut() {
                            s.push_str(&text[..end]);
                        }
                        if end < text.len() {
                            after_label = false;
                        }
                        &text[end..]
                    } else {
                        &text[..]
                    };
                    if !rest.trim().is_empty() {
                        add_other(&mut parts, "text".into());
                    }
                }
                _ => {}
            }
        }
    }
    parts
}

fn header(hf: &Node, format: &mut PageFormat, r: &mut Report) {
    let parts = header_parts(hf.child("Header"));
    let visible = match hf.attr("HeaderVisible") {
        Some(v) => yes_no(v, &At::new("HeaderAndFooter HeaderVisible"), r).unwrap_or(true),
        None => true,
    };
    format.page_numbers.visible = visible && parts.page_label;
    if let Some(v) = hf.attr("HeaderFirstPage")
        && let Some(on) = yes_no(v, &At::new("HeaderAndFooter HeaderFirstPage"), r)
    {
        format.page_numbers.first = if on { 1 } else { 2 };
    }
    if let Some(s) = parts.suffix {
        let (kept, cut) = one_line(&s, MAX_PAGE_SUFFIX);
        if cut {
            r.push(
                codes::INVALID_VALUE,
                format!(
                    "the text after the header's page number is longer than one line of {MAX_PAGE_SUFFIX} characters; the suffix is cut to \"{kept}\""
                ),
                None,
            );
        }
        format.page_numbers.suffix = kept.to_string();
    }
    if let Some(v) = hf.attr("StartingPage") {
        match v.trim().parse::<i64>() {
            Ok(1) => {}
            Ok(n) => r.push(
                codes::STARTING_PAGE,
                format!("page numbering starts at {n} in the file; ours always starts at 1"),
                None,
            ),
            Err(_) => r.push(
                codes::INVALID_VALUE,
                format!(
                    "HeaderAndFooter StartingPage = \"{v}\" is not a number; the default is kept"
                ),
                None,
            ),
        }
    }
    if visible && !parts.others.is_empty() {
        r.push(
            codes::HEADER_CONTENT_IGNORED,
            format!(
                "the header also holds {}, which we don't print",
                parts.others.join(", ")
            ),
            None,
        );
    }
    let footer_visible = hf
        .attr("FooterVisible")
        .and_then(|v| yes_no(v, &At::new("HeaderAndFooter FooterVisible"), r))
        .unwrap_or(false);
    let footer = header_parts(hf.child("Footer"));
    if footer_visible && (footer.page_label || !footer.others.is_empty()) {
        r.push(
            codes::FOOTER_IGNORED,
            "the footer is visible and has content, which we don't print",
            None,
        );
    }
}

pub(crate) fn describe_unknown_style(key: &str, n: usize) -> String {
    format!(
        "the style \"{key}\" is not one we model and is ignored ({})",
        plural(n, "time", "times")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_div_rounds_half_away() {
        assert_eq!(mul_div(Pt(1200), 200, 100), Some(Pt(2400)));
        assert_eq!(mul_div(Pt(1), 150, 100), Some(Pt(2)));
        assert_eq!(mul_div(Pt(-1), 150, 100), Some(Pt(-2)));
        assert_eq!(mul_div(Pt(i32::MAX), 200, 100), None);
    }

    #[test]
    fn mul_div_never_overflows() {
        assert_eq!(mul_div(Pt(i32::MIN), i32::MIN, 100), None);
        assert_eq!(mul_div(Pt(i32::MIN), i32::MIN, 1), None);
        assert_eq!(mul_div(Pt(i32::MAX), i32::MAX, 1), None);
        assert_eq!(
            mul_div(Pt(i32::MIN), i32::MAX, i32::MAX),
            Some(Pt(i32::MIN))
        );
        assert_eq!(mul_div(Pt(i32::MIN), 100, 100), Some(Pt(i32::MIN)));
        assert_eq!(mul_div(Pt(i32::MIN), -100, 100), None);
        assert_eq!(mul_div(Pt(12), 1, 0), None);
        assert_eq!(mul_div(Pt(12), 1, -1), None);
    }

    #[test]
    fn string_bounds_match_the_format_validator() {
        let x = |n| "x".repeat(n);
        let mut f = PageFormat::default();
        f.page_numbers.suffix = x(MAX_PAGE_SUFFIX);
        f.more = Some(x(MAX_INSERT));
        f.contd = Some(x(MAX_INSERT));
        assert_eq!(f.problems(), Vec::<String>::new());
        let mut g = f.clone();
        g.page_numbers.suffix = x(MAX_PAGE_SUFFIX + 1);
        assert_eq!(g.problems().len(), 1, "{:?}", g.problems());
        let mut g = f.clone();
        g.more = Some(x(MAX_INSERT + 1));
        assert_eq!(g.problems().len(), 1, "{:?}", g.problems());
        let mut g = f;
        g.contd = Some(x(MAX_INSERT + 1));
        assert_eq!(g.problems().len(), 1, "{:?}", g.problems());
    }

    #[test]
    fn one_line_cuts_at_the_first_line_then_the_length() {
        assert_eq!(one_line("(MORE)", 32), ("(MORE)", false));
        assert_eq!(one_line("", 16), ("", false));
        assert_eq!(one_line("abcd", 4), ("abcd", false));
        assert_eq!(one_line("abcdef", 4), ("abcd", true));
        assert_eq!(one_line("ab\ncd", 16), ("ab", true));
        assert_eq!(one_line("ab\rcd", 16), ("ab", true));
        assert_eq!(one_line("\nab", 16), ("", true));
        // Characters, not bytes.
        assert_eq!(
            one_line("\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}", 4),
            ("\u{e9}\u{e9}\u{e9}\u{e9}", true)
        );
    }
}
