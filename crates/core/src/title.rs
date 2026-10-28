//! The title page commands (docs/specs/commands.md, "The title page
//! commands"): `title.get`, `title.set` and `title.set_lines`. Title page
//! edits are journaled and undoable, never tracked and never
//! revision-marked (decided with the project's owner, 2026-10-22).

use coldopen_screenplay::{Align, Pt, RichText, TextRepr, TitleField, TitleLine};
use serde_json::{Value, json};

use crate::author::Author;
use crate::command::{TitleAlign, TitleLineParam, TitleSet, TitleSetLines, TitleValue};
use crate::document::Document;
use crate::error::CommandError;
use crate::ops::{Op, Transaction};

/// The longest title-page key, in characters.
pub const MAX_KEY: usize = 64;

/// Which form a document holds its title page in.
pub fn form(doc: &Document) -> &'static str {
    if !doc.title_lines.is_empty() {
        "lines"
    } else if !doc.title_page.is_empty() {
        "keys"
    } else {
        "none"
    }
}

/// `title.get`.
pub fn get(doc: &Document) -> Value {
    let keys: Vec<Value> = doc
        .title_page
        .fields
        .iter()
        .map(|f| json!({ "key": f.key, "values": f.values.iter().map(TextRepr::from_rich).collect::<Vec<_>>() }))
        .collect();
    let lines: Vec<Value> = doc
        .title_lines
        .iter()
        .map(|l| {
            let mut v = serde_json::to_value(TextRepr::from_rich(&l.text)).unwrap_or_default();
            if let Value::Object(m) = &mut v {
                m.insert("align".into(), json!(l.align));
                m.insert("left".into(), json!(l.left));
                m.insert("right".into(), json!(l.right));
                m.insert("space_before".into(), json!(l.space_before));
            }
            v
        })
        .collect();
    json!({ "form": form(doc), "keys": keys, "lines": lines })
}

/// While agents' edits are tracked, an agent may not edit the title page:
/// its edits could not be reviewed (decided with the project's owner,
/// 2026-10-22).
fn refuse_agent(by: &Author, tracked: bool) -> Result<(), CommandError> {
    if !by.is_writer() && tracked {
        return Err(CommandError::new(
            "writer_only",
            "an agent's edits are tracked changes in this script, and title page edits cannot be tracked; only the writer can edit the title page while agent tracking is on",
        ));
    }
    Ok(())
}

fn wrong_form(has: &str) -> CommandError {
    CommandError::new(
        "title_form",
        format!(
            "this title page is {}; use {}",
            if has == "lines" {
                "positioned lines"
            } else {
                "title-page keys"
            },
            if has == "lines" {
                "title.set_lines"
            } else {
                "title.set"
            }
        ),
    )
    .with_details(json!({ "form": has }))
}

/// A key as Fountain writes it (docs/specs/fountain.md §4): it starts with
/// a letter or digit, then letters, digits, spaces and `-_.'/&`, and does
/// not end in a space.
pub fn check_key(key: &str) -> Result<(), CommandError> {
    let n = key.chars().count();
    let first = key.chars().next();
    let ok = (1..=MAX_KEY).contains(&n)
        && first.is_some_and(char::is_alphanumeric)
        && !key.ends_with(' ')
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '\'' | '/' | '&'));
    if ok {
        Ok(())
    } else {
        Err(CommandError::invalid_params(format!(
            "\"{key}\" is not a title-page key: 1 to {MAX_KEY} letters, digits, spaces and - _ . ' / &, starting with a letter or digit"
        )))
    }
}

fn rich(v: &TitleValue) -> Result<RichText, CommandError> {
    let t = match v {
        TitleValue::Text(s) => RichText::plain(s.clone()),
        TitleValue::Runs(r) => RichText::new(r.clone()),
    }
    .as_input();
    let plain = t.plain_text();
    if plain.contains(['\n', '\r']) {
        return Err(CommandError::invalid_params(
            "a title-page value is one line; give each line as a value of its own",
        ));
    }
    // Fountain cannot hold an empty value, or space at either end of one
    // (docs/specs/fountain.md §4).
    if plain.trim().is_empty() || plain.trim() != plain {
        return Err(CommandError::invalid_params(
            "a title-page value has text, with no space at either end",
        ));
    }
    Ok(t)
}

/// `title.set`: one key's values, added, changed or removed.
pub fn plan_set(
    doc: &Document,
    p: &TitleSet,
    by: &Author,
    tracked: bool,
) -> Result<(Transaction, Value), CommandError> {
    refuse_agent(by, tracked)?;
    if form(doc) == "lines" {
        return Err(wrong_form("lines"));
    }
    check_key(&p.key)?;
    let values = p.values.iter().map(rich).collect::<Result<Vec<_>, _>>()?;
    let mut page = doc.title_page.clone();
    let at = page
        .fields
        .iter()
        .position(|f| f.key.eq_ignore_ascii_case(&p.key));
    match (at, values.is_empty()) {
        (Some(i), true) => {
            page.fields.remove(i);
        }
        (Some(i), false) => page.fields[i].values = values,
        (None, true) => return Ok((Transaction::default(), json!({}))),
        (None, false) => {
            let i = p.at.unwrap_or(page.fields.len()).min(page.fields.len());
            page.fields
                .insert(i, TitleField::new(p.key.clone(), values));
        }
    }
    if page == doc.title_page {
        return Ok((Transaction::default(), json!({})));
    }
    Ok((
        Transaction(vec![Op::TitlePage {
            title_page: page,
            title_lines: doc.title_lines.clone(),
        }]),
        json!({}),
    ))
}

/// Points from a JSON number, to the hundredth.
fn points(v: f64, what: &str) -> Result<Pt, CommandError> {
    let h = (v * 100.0).round();
    if !h.is_finite() || h.abs() > f64::from(i32::MAX) {
        return Err(CommandError::invalid_params(format!(
            "{what} is out of range"
        )));
    }
    Ok(Pt(h as i32))
}

fn line(doc: &Document, p: &TitleLineParam) -> Result<TitleLine, CommandError> {
    let text = TextRepr {
        text: p.text.clone(),
        runs: p.runs.clone(),
    }
    .into_rich()
    .map_err(CommandError::invalid_params)?
    .as_input();
    let format = crate::view::page_format(doc);
    let f = format.get();
    let (left, right) = (points(p.left, "left")?, points(p.right, "right")?);
    let space_before = points(p.space_before, "space_before")?;
    if left.0 < 0 || right.0 > f.page_width.0 || right.0 - left.0 < f.pitch.0 {
        return Err(CommandError::new(
            "invalid_attr",
            "a title line's left and right edges must make a column of at least one cell on the page",
        ));
    }
    if space_before.0 < 0 || space_before > f.body_height() {
        return Err(CommandError::new(
            "invalid_attr",
            "a title line's space above must be from 0 to the page body's height",
        ));
    }
    Ok(TitleLine {
        text,
        align: match p.align {
            TitleAlign::Left => Align::Left,
            TitleAlign::Center => Align::Center,
            TitleAlign::Right => Align::Right,
        },
        left,
        right,
        space_before,
    })
}

/// `title.set_lines`: the whole list of positioned lines.
pub fn plan_set_lines(
    doc: &Document,
    p: &TitleSetLines,
    by: &Author,
    tracked: bool,
) -> Result<(Transaction, Value), CommandError> {
    refuse_agent(by, tracked)?;
    if form(doc) == "keys" {
        return Err(wrong_form("keys"));
    }
    let lines = p
        .lines
        .iter()
        .map(|l| line(doc, l))
        .collect::<Result<Vec<_>, _>>()?;
    if lines == doc.title_lines {
        return Ok((Transaction::default(), json!({})));
    }
    Ok((
        Transaction(vec![Op::TitlePage {
            title_page: doc.title_page.clone(),
            title_lines: lines,
        }]),
        json!({}),
    ))
}
