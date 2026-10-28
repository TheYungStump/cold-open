//! The scenes, characters and locations reports (docs/specs/reports.md §4),
//! read from the document and its layout. Every order is fixed and sorted.

use std::collections::BTreeMap;

use coldopen_layout::{Layout, LineKind};
use coldopen_screenplay::ElementKind;
use coldopen_screenplay::names::{self, Setting};
use serde_json::{Value, json};

use crate::document::{BlockId, Document, word_count};

/// The blocks the reports read: those in the current text, not pending
/// deletions (docs/specs/changes.md §2).
fn current(doc: &Document, i: usize) -> bool {
    doc.blocks[i].marks.deleted.is_none()
}

/// For each block, the scene heading it falls under, as `scene.list`
/// draws scenes.
fn scene_of(doc: &Document) -> Vec<Option<BlockId>> {
    let mut out = vec![None; doc.blocks.len()];
    for s in doc.scenes() {
        let id = doc.blocks[s.start].id;
        for slot in &mut out[s.start..s.end] {
            *slot = Some(id);
        }
    }
    out
}

/// One speech: a character block with the dialogue and lyric blocks after
/// it, up to the next printing block of another kind (§4.2).
struct Speech {
    character: usize,
    key: String,
    name: String,
    extensions: Vec<String>,
    lines: Vec<usize>,
}

fn speeches(doc: &Document) -> Vec<Speech> {
    let mut out = Vec::new();
    for (i, b) in doc.blocks.iter().enumerate() {
        if b.element.kind != ElementKind::Character || !current(doc, i) {
            continue;
        }
        let Some(s) = names::speaker(&b.element.text.plain_text()) else {
            continue;
        };
        let mut lines = Vec::new();
        for (k, m) in doc.blocks.iter().enumerate().skip(i + 1) {
            let kind = m.element.kind;
            if kind.is_transparent() && kind != ElementKind::PageBreak {
                continue;
            }
            if !kind.is_dialogue_member() {
                break;
            }
            if matches!(kind, ElementKind::Dialogue | ElementKind::Lyric) && current(doc, k) {
                lines.push(k);
            }
        }
        out.push(Speech {
            character: i,
            key: s.key,
            name: s.name,
            extensions: s.extensions,
            lines,
        });
    }
    out
}

/// Printed dialogue and lyric lines, by block, from the layout.
fn printed_lines(layout: &Layout, map: &[usize]) -> BTreeMap<usize, usize> {
    let mut out = BTreeMap::new();
    for page in &layout.pages {
        for l in &page.lines {
            if let (LineKind::Content(ElementKind::Dialogue | ElementKind::Lyric), Some((e, ..))) =
                (l.kind, l.source)
                && let Some(&b) = map.get(e)
            {
                *out.entry(b).or_insert(0) += 1;
            }
        }
    }
    out
}

/// The body page a block's first line prints on.
fn page_of(layout: &Layout, map: &[usize], block: usize) -> Option<u32> {
    let e = map.partition_point(|&b| b < block);
    (map.get(e) == Some(&block))
        .then(|| layout.first_page(e))
        .flatten()
}

/// `report.scenes`: `scene.list`'s rows with the heading read, and the
/// characters who speak, in order of first speech (§4.1).
pub fn scenes(doc: &Document, scene_list: Vec<Value>) -> Value {
    let scene = scene_of(doc);
    let mut speakers: BTreeMap<BlockId, Vec<(String, String)>> = BTreeMap::new();
    for s in speeches(doc) {
        let Some(id) = scene[s.character] else {
            continue;
        };
        let names = speakers.entry(id).or_default();
        if !names.iter().any(|(k, _)| *k == s.key) {
            names.push((s.key, s.name));
        }
    }
    let rows: Vec<Value> = scene_list
        .into_iter()
        .map(|mut row| {
            let h = names::heading(row["heading"].as_str().unwrap_or(""));
            let id: Option<BlockId> = serde_json::from_value(row["id"].clone()).ok();
            let characters: Vec<String> = id
                .and_then(|id| speakers.get(&id))
                .map(|v| v.iter().map(|(_, n)| n.clone()).collect())
                .unwrap_or_default();
            if let Value::Object(m) = &mut row {
                m.remove("synopsis");
                m.remove("blocks");
                m.insert("setting".into(), json!(h.setting.name()));
                m.insert("location".into(), json!(h.location));
                m.insert("time".into(), json!(h.time));
                m.insert("characters".into(), json!(characters));
            }
            row
        })
        .collect();
    json!({ "scenes": rows })
}

/// `report.characters`: speeches, printed lines, words and scenes for each
/// speaker, most words first (§4.2).
pub fn characters(doc: &Document, layout: &Layout, map: &[usize]) -> Value {
    struct Row {
        name: String,
        speeches: usize,
        lines: usize,
        words: usize,
        scenes: Vec<BlockId>,
        extensions: Vec<String>,
        first_page: Option<u32>,
    }
    let scene = scene_of(doc);
    let printed = printed_lines(layout, map);
    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    for s in speeches(doc) {
        let row = rows.entry(s.key.clone()).or_insert_with(|| Row {
            name: s.name.clone(),
            speeches: 0,
            lines: 0,
            words: 0,
            scenes: Vec::new(),
            extensions: Vec::new(),
            first_page: None,
        });
        if let Some(id) = scene[s.character]
            && !row.scenes.contains(&id)
        {
            row.scenes.push(id);
        }
        for e in s.extensions {
            if !names::has_contd_marker(&e, "(CONT'D)") && !row.extensions.contains(&e) {
                row.extensions.push(e);
            }
        }
        if s.lines.is_empty() {
            continue;
        }
        row.speeches += 1;
        row.lines += s
            .lines
            .iter()
            .map(|b| printed.get(b).copied().unwrap_or(0))
            .sum::<usize>();
        row.words += s
            .lines
            .iter()
            .map(|&b| word_count(&doc.blocks[b].element))
            .sum::<usize>();
        if row.first_page.is_none() {
            row.first_page = page_of(layout, map, s.character);
        }
    }
    let mut sorted: Vec<(String, Row)> = rows.into_iter().collect();
    sorted.sort_by(|(ka, a), (kb, b)| b.words.cmp(&a.words).then(ka.cmp(kb)));
    let out: Vec<Value> = sorted
        .into_iter()
        .map(|(_, r)| {
            json!({
                "name": r.name,
                "speeches": r.speeches,
                "lines": r.lines,
                "words": r.words,
                "scenes": r.scenes,
                "extensions": r.extensions,
                "first_page": r.first_page,
            })
        })
        .collect();
    json!({ "characters": out })
}

/// `report.locations`: each location by setting and time of day, sorted by
/// name (§4.3).
pub fn locations(doc: &Document) -> Value {
    /// A time of day's scenes, keyed so that no time sorts last.
    type Times = BTreeMap<(bool, String), (String, Vec<BlockId>)>;
    struct Loc {
        name: String,
        scenes: Vec<BlockId>,
        settings: BTreeMap<Setting, Times>,
    }
    let mut locs: BTreeMap<String, Loc> = BTreeMap::new();
    for s in doc.scenes() {
        let b = &doc.blocks[s.start];
        if b.element.kind != ElementKind::SceneHeading || !current(doc, s.start) {
            continue;
        }
        let h = names::heading(&b.element.text.plain_text());
        let loc = locs.entry(h.location_key.clone()).or_insert_with(|| Loc {
            name: h.location.clone(),
            scenes: Vec::new(),
            settings: BTreeMap::new(),
        });
        loc.scenes.push(b.id);
        // No time sorts last: (false, key) before (true, "").
        let time = match &h.time {
            Some(t) => (false, names::time_key(t)),
            None => (true, String::new()),
        };
        loc.settings
            .entry(h.setting)
            .or_default()
            .entry(time)
            .or_insert_with(|| (h.time.clone().unwrap_or_default(), Vec::new()))
            .1
            .push(b.id);
    }
    let out: Vec<Value> = locs
        .into_values()
        .map(|l| {
            let settings: Vec<Value> = l
                .settings
                .into_iter()
                .map(|(setting, times)| {
                    let scenes: Vec<BlockId> = {
                        let mut v: Vec<BlockId> =
                            times.values().flat_map(|(_, s)| s.clone()).collect();
                        v.sort_by_key(|id| doc.index_of(*id));
                        v
                    };
                    let times: Vec<Value> = times
                        .into_iter()
                        .map(|((none, key), (_, scenes))| {
                            json!({ "time": (!none).then_some(key), "scenes": scenes })
                        })
                        .collect();
                    json!({ "setting": setting.name(), "scenes": scenes, "times": times })
                })
                .collect();
            json!({ "location": l.name, "scenes": l.scenes, "settings": settings })
        })
        .collect();
    json!({ "locations": out })
}

/// Which report (docs/specs/reports.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Scenes,
    Characters,
    Locations,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Scenes, Kind::Characters, Kind::Locations];

    /// The query that gives it.
    pub fn command(self) -> &'static str {
        match self {
            Kind::Scenes => "report.scenes",
            Kind::Characters => "report.characters",
            Kind::Locations => "report.locations",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Kind::Scenes => "Scenes",
            Kind::Characters => "Characters",
            Kind::Locations => "Locations",
        }
    }
}

/// A report as rows for a table, CSV or a view (docs/specs/reports.md §5):
/// scenes by their number in the script, lists joined with "; ".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub header: Vec<&'static str>,
    pub rows: Vec<Vec<String>>,
    /// For each row, the block a view goes to: the scene, a speaker's first
    /// scene, or a location's first scene at that time.
    pub targets: Vec<Option<String>>,
}

/// A JSON value as a cell: strings as they are, null as nothing.
fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn joined(v: &Value) -> String {
    v.as_array()
        .into_iter()
        .flatten()
        .map(cell)
        .collect::<Vec<_>>()
        .join("; ")
}

/// A list of scene ids as their numbers in the script.
fn numbered(v: &Value, index: &BTreeMap<String, u64>) -> String {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_str().and_then(|id| index.get(id)))
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

fn first(v: &Value) -> Option<String> {
    v.as_array()?.first()?.as_str().map(str::to_string)
}

/// The report's rows. `index` maps each scene's id to its number in the
/// script, as `scene.list` numbers them.
pub fn table(kind: Kind, report: &Value, index: &BTreeMap<String, u64>) -> Table {
    let list = |k: &str| report[k].as_array().cloned().unwrap_or_default();
    let mut t = Table::default();
    match kind {
        Kind::Scenes => {
            t.header = vec![
                "index",
                "number",
                "heading",
                "setting",
                "location",
                "time",
                "page",
                "eighths",
                "words",
                "characters",
            ];
            for s in list("scenes") {
                t.rows.push(vec![
                    cell(&s["index"]),
                    cell(&s["number"]),
                    cell(&s["heading"]),
                    cell(&s["setting"]),
                    cell(&s["location"]),
                    cell(&s["time"]),
                    cell(&s["page"]),
                    cell(&s["eighths"]),
                    cell(&s["words"]),
                    joined(&s["characters"]),
                ]);
                t.targets.push(s["id"].as_str().map(str::to_string));
            }
        }
        Kind::Characters => {
            t.header = vec![
                "name",
                "speeches",
                "lines",
                "words",
                "scenes",
                "extensions",
                "first_page",
            ];
            for c in list("characters") {
                t.rows.push(vec![
                    cell(&c["name"]),
                    cell(&c["speeches"]),
                    cell(&c["lines"]),
                    cell(&c["words"]),
                    numbered(&c["scenes"], index),
                    joined(&c["extensions"]),
                    cell(&c["first_page"]),
                ]);
                t.targets.push(first(&c["scenes"]));
            }
        }
        Kind::Locations => {
            t.header = vec!["location", "setting", "time", "scenes"];
            for l in list("locations") {
                for s in l["settings"].as_array().into_iter().flatten() {
                    for time in s["times"].as_array().into_iter().flatten() {
                        t.rows.push(vec![
                            cell(&l["location"]),
                            cell(&s["setting"]),
                            cell(&time["time"]),
                            numbered(&time["scenes"], index),
                        ]);
                        t.targets.push(first(&time["scenes"]));
                    }
                }
            }
        }
    }
    t
}

/// Columns that hold counts, right-aligned in the text form.
pub fn numeric(column: &str) -> bool {
    matches!(
        column,
        "index" | "page" | "eighths" | "words" | "speeches" | "lines" | "first_page"
    )
}

/// A CSV field: quoted when it holds a comma, a quote, CR or LF, with
/// quotes doubled (RFC 4180).
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The table as CSV (RFC 4180): a header row, then one row each, CRLF
/// line ends.
pub fn csv(t: &Table) -> String {
    let row = |cells: Vec<String>| {
        let mut line = cells
            .iter()
            .map(|c| csv_field(c))
            .collect::<Vec<_>>()
            .join(",");
        line.push_str("\r\n");
        line
    };
    let mut out = row(t.header.iter().map(|h| h.to_string()).collect());
    for r in &t.rows {
        out.push_str(&row(r.clone()));
    }
    out
}

/// The table for people: each column as wide as its widest cell, counts
/// right-aligned.
pub fn text(t: &Table) -> String {
    let widths: Vec<usize> = t
        .header
        .iter()
        .enumerate()
        .map(|(k, h)| {
            t.rows
                .iter()
                .map(|r| r[k].chars().count())
                .chain(std::iter::once(h.chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: Vec<String>| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let pad = " ".repeat(widths[k].saturating_sub(c.chars().count()));
                if numeric(t.header[k]) {
                    format!("{pad}{c}")
                } else {
                    format!("{c}{pad}")
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    let mut out = line(t.header.iter().map(|h| h.replace('_', " ")).collect());
    out.push('\n');
    for r in &t.rows {
        out.push_str(&line(r.clone()));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_only_what_needs_it() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a, b"), "\"a, b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn locations_give_a_row_per_setting_and_time() {
        let report = json!({"locations": [{"location": "LAB", "scenes": ["b_1", "b_9"], "settings": [
            {"setting": "int", "scenes": ["b_1", "b_9"], "times": [
                {"time": "DAY", "scenes": ["b_1"]}, {"time": null, "scenes": ["b_9"]}]}]}]});
        let index = BTreeMap::from([("b_1".to_string(), 1), ("b_9".to_string(), 4)]);
        let t = table(Kind::Locations, &report, &index);
        assert_eq!(
            csv(&t),
            "location,setting,time,scenes\r\nLAB,int,DAY,1\r\nLAB,int,,4\r\n"
        );
        assert_eq!(t.targets, [Some("b_1".into()), Some("b_9".into())]);
        assert_eq!(text(&t).lines().count(), 3);
    }
}
