//! Layout fixtures: short scenes of our own, one rule per case, each with a
//! golden page view (tests/fixtures/layout/README.md).

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use coldopen_layout::pdf::{self, PdfOptions};
use coldopen_layout::{Engine, Fonts, Options, Rules, ValidFormat};
use coldopen_screenplay::PageFormat;
use common::pdf_read::assert_draws_layout;
use common::{assert_golden, fixtures_dir, merge_patch, pdf_dump};

/// The format for a case: the default, then `_<base>.format.json` if the
/// case names one with `"extends"`, then the case's own patch.
fn format_for(dir: &Path, case: &str) -> ValidFormat {
    let mut v = serde_json::to_value(PageFormat::default()).unwrap();
    let own = dir.join(format!("{case}.format.json"));
    if let Ok(text) = fs::read_to_string(&own) {
        let mut patch: serde_json::Value = serde_json::from_str(&text).unwrap();
        if let Some(base) = patch.as_object_mut().and_then(|o| o.remove("extends")) {
            let base = base.as_str().unwrap().to_string();
            let b: serde_json::Value = serde_json::from_str(
                &fs::read_to_string(dir.join(format!("_{base}.format.json"))).unwrap(),
            )
            .unwrap();
            merge_patch(&mut v, &b);
        }
        merge_patch(&mut v, &patch);
    }
    let f: PageFormat = serde_json::from_value(v).unwrap_or_else(|e| panic!("{case}: {e}"));
    ValidFormat::new(f).unwrap_or_else(|p| panic!("{case}: invalid format: {p:?}"))
}

/// What a case's `<case>.options.json` asks for.
struct CaseOptions {
    layout: Options,
    /// PDF options, when the case has PDF goldens.
    pdf: Option<PdfOptions>,
    /// The rules the case is there to show. Turning any one of them off
    /// must move lines on its pages.
    decides: Vec<String>,
    /// Rules the case only records: turning one off must change a page's
    /// recorded break cause, even if no line moves (a level-1 fallback).
    records: Vec<String>,
}

/// The keys `<case>.options.json` may hold. Anything else is a typo that
/// would otherwise be ignored, such as `"decide"`.
const OPTION_KEYS: [&str; 7] = [
    "scene_numbers",
    "no_title_page",
    "pdf",
    "watermark",
    "title",
    "decides",
    "records",
];

/// The names `"decides"` accepts: the rule switches in [`Rules`].
const RULE_NAMES: [&str; 6] = [
    "heading",
    "name",
    "parenthetical",
    "transition",
    "sentences",
    "min_lines",
];

/// The default rules with one of them turned off. `min_lines` is a count,
/// so "off" lowers the two-line minimum to one line.
fn without(rule: &str) -> Option<Rules> {
    let mut r = Rules::default();
    match rule {
        "heading" => r.heading = false,
        "name" => r.name = false,
        "parenthetical" => r.parenthetical = false,
        "transition" => r.transition = false,
        "sentences" => r.sentences = false,
        "min_lines" => r.min_lines = 1,
        _ => return None,
    }
    Some(r)
}

fn options_for(dir: &Path, case: &str) -> CaseOptions {
    let mut o = CaseOptions {
        layout: Options::default(),
        pdf: None,
        decides: Vec::new(),
        records: Vec::new(),
    };
    let file = format!("{case}.options.json");
    let Ok(text) = fs::read_to_string(dir.join(&file)) else {
        return o;
    };
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"));
    let keys = v
        .as_object()
        .unwrap_or_else(|| panic!("{file}: not a JSON object"));
    for k in keys.keys() {
        assert!(
            OPTION_KEYS.contains(&k.as_str()),
            "{file}: unknown key {k:?}; the keys are {OPTION_KEYS:?}"
        );
    }
    o.layout.scene_numbers = v["scene_numbers"].as_bool().unwrap_or(false);
    o.layout.no_title_page = v["no_title_page"].as_bool().unwrap_or(false);
    if v["pdf"].as_bool().unwrap_or(false) {
        o.pdf = Some(PdfOptions {
            watermark: v["watermark"].as_str().map(str::to_string),
            title: v["title"].as_str().map(str::to_string),
            ..Default::default()
        });
    }
    for (key, out) in [("decides", &mut o.decides), ("records", &mut o.records)] {
        let Some(d) = v.get(key) else { continue };
        let list = d
            .as_array()
            .unwrap_or_else(|| panic!("{file}: \"{key}\" must be a list of rule names"));
        for r in list {
            let name = r
                .as_str()
                .filter(|n| RULE_NAMES.contains(n))
                .unwrap_or_else(|| {
                    panic!("{file}: \"{key}\" names {r}; the rules are {RULE_NAMES:?}")
                });
            out.push(name.to_string());
        }
    }
    o
}

/// A page view without the break causes and diagnostics in its page
/// headers, so only the lines and the page boundaries are compared.
fn lines_only(view: &str) -> String {
    view.lines()
        .map(|l| {
            if l.starts_with("=== page ") {
                l.split(" · ").next().unwrap_or(l)
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn cases(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter_map(|n| n.strip_suffix(".fountain").map(str::to_string))
        .collect();
    v.sort();
    v
}

#[test]
fn layout_fixtures() {
    let dir = fixtures_dir();
    let fonts = Fonts::new().unwrap();
    let cases = cases(&dir);
    assert!(!cases.is_empty());
    for case in &cases {
        let src = fs::read_to_string(dir.join(format!("{case}.fountain"))).unwrap();
        let script = coldopen_fountain::parse(&src).script;
        let format = format_for(&dir, case);
        let CaseOptions {
            layout: options,
            pdf: pdf_options,
            decides,
            records,
        } = options_for(&dir, case);
        let engine = Engine::new(&script, &format, &options);
        let layout = engine.run();
        let view = layout.page_view(&fonts);

        // Determinism, and every page can be laid out again from its start.
        assert_eq!(
            Engine::new(&script, &format, &options)
                .run()
                .page_view(&fonts),
            view,
            "{case}"
        );
        for p in &layout.pages {
            let again = engine.page_from(p.start);
            assert_eq!(again.next, p.next, "{case}: page {:?} resync", p.number);
        }
        assert_golden(&dir.join(format!("{case}.pages.txt")), view.as_bytes());

        // Every case's PDF draws exactly what its layout placed (§11, §12),
        // read back with the widths the PDF declares.
        let out = pdf::write(&layout, &fonts, &pdf_options.clone().unwrap_or_default()).unwrap();
        assert_draws_layout(&out.bytes, &layout, &fonts);

        // A case that says it decides a rule must depend on it: with the
        // rule off, lines move. One that only records a rule must at least
        // show it in a page's break cause.
        let view_without = |rule: &str| {
            Engine::new(&script, &format, &options)
                .with_rules(without(rule).unwrap())
                .run()
                .page_view(&fonts)
        };
        for rule in &decides {
            assert!(
                lines_only(&view_without(rule)) != lines_only(&view),
                "{case}: its options say it decides `{rule}`, but no line moves with that rule \
                 off. Rewrite the case so the rule decides it, move `{rule}` to \"records\" if \
                 only the recorded break cause changes, or take it out."
            );
        }
        for rule in &records {
            let off = view_without(rule);
            assert!(
                off != view && lines_only(&off) == lines_only(&view),
                "{case}: \"records\" `{rule}` means only the recorded break cause changes with \
                 that rule off; here {}.",
                if off == view {
                    "nothing changes"
                } else {
                    "lines move, so use \"decides\""
                }
            );
        }

        if let Some(po) = pdf_options {
            let out = pdf::write(&layout, &fonts, &po).unwrap();
            assert_eq!(
                pdf::write(&layout, &fonts, &po).unwrap(),
                out,
                "{case}: PDF not deterministic"
            );
            let mut dump = pdf_dump(&out.bytes);
            for w in &out.warnings {
                dump.push_str(&format!("\n% warning {}: {}", w.code, w.chars));
            }
            dump.push('\n');
            assert_golden(&dir.join(format!("{case}.pdf.txt")), dump.as_bytes());
            assert_golden(&dir.join(format!("{case}.pdf")), &out.bytes);
        }
    }
}

/// Every golden and side file belongs to a case.
#[test]
fn no_orphan_files() {
    let dir = fixtures_dir();
    let cases: BTreeSet<String> = cases(&dir).into_iter().collect();
    for e in fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == "README.md" || name.starts_with('_') {
            continue;
        }
        let case = name.split('.').next().unwrap();
        assert!(cases.contains(case), "{name} has no {case}.fountain");
    }
}
