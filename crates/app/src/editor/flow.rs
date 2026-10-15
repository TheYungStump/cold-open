//! Smart element flow: which element Enter and Tab lead to
//! (docs/specs/editor.md §5.2). The table is template data; a user's
//! `flow.json` can change any cell.

use std::collections::BTreeMap;

use coldopen_screenplay::ElementKind;
use serde_json::Value;

/// One row of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The kind of the new block that Enter at the end of a block starts.
    pub enter: ElementKind,
    /// What an empty block becomes when Enter is pressed in it.
    pub empty_enter: ElementKind,
    pub tab: Option<ElementKind>,
    pub shift_tab: Option<ElementKind>,
}

/// The element flow table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowTable {
    rows: BTreeMap<ElementKind, Row>,
}

impl Default for FlowTable {
    /// The built-in feature-film template (editor.md §5.2).
    fn default() -> FlowTable {
        use ElementKind::*;
        let row = |enter, empty_enter, tab, shift_tab| Row {
            enter,
            empty_enter,
            tab,
            shift_tab,
        };
        let rows = [
            (
                SceneHeading,
                row(Action, Action, Some(Action), Some(Transition)),
            ),
            (
                Action,
                row(Action, SceneHeading, Some(Character), Some(SceneHeading)),
            ),
            (
                Character,
                row(Dialogue, Action, Some(Transition), Some(Action)),
            ),
            (
                Parenthetical,
                row(Dialogue, Dialogue, Some(Dialogue), Some(Character)),
            ),
            (
                Dialogue,
                row(Character, Action, Some(Parenthetical), Some(Character)),
            ),
            (
                Transition,
                row(
                    SceneHeading,
                    SceneHeading,
                    Some(SceneHeading),
                    Some(Character),
                ),
            ),
            (Shot, row(Action, Action, Some(Action), Some(SceneHeading))),
            (Lyric, row(Lyric, Dialogue, Some(Dialogue), Some(Character))),
            (General, row(General, Action, Some(Action), Some(Action))),
            (Section, row(Action, Action, None, None)),
            (Synopsis, row(Action, Action, None, None)),
            (Note, row(Action, Action, None, None)),
            (Boneyard, row(Action, Action, None, None)),
            (PageBreak, row(Action, Action, None, None)),
        ]
        .into_iter()
        .collect();
        FlowTable { rows }
    }
}

impl FlowTable {
    pub fn row(&self, kind: ElementKind) -> Row {
        self.rows.get(&kind).copied().unwrap_or(Row {
            enter: ElementKind::Action,
            empty_enter: ElementKind::Action,
            tab: None,
            shift_tab: None,
        })
    }

    /// Applies a user's `flow.json` on top of this table. Returns a warning
    /// for each entry that was ignored; a bad file never fails.
    pub fn apply_overrides(&mut self, json: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        let v: Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => return vec![format!("flow.json is not valid JSON: {e}")],
        };
        if v.get("v").and_then(Value::as_u64) != Some(1) {
            return vec!["flow.json needs \"v\": 1; the file was ignored".into()];
        }
        let Some(kinds) = v.get("kinds").and_then(Value::as_object) else {
            return vec!["flow.json has no \"kinds\" object; the file was ignored".into()];
        };
        for (name, fields) in kinds {
            let Some(kind) = ElementKind::from_name(name) else {
                warnings.push(format!("flow.json: unknown element \"{name}\" ignored"));
                continue;
            };
            let Some(fields) = fields.as_object() else {
                warnings.push(format!("flow.json: \"{name}\" must be an object"));
                continue;
            };
            let mut row = self.row(kind);
            for (field, value) in fields {
                let target = match value {
                    Value::Null => None,
                    Value::String(s) => match ElementKind::from_name(s) {
                        Some(k) => Some(k),
                        None => {
                            warnings.push(format!(
                                "flow.json: \"{name}.{field}\": unknown element \"{s}\" ignored"
                            ));
                            continue;
                        }
                    },
                    _ => {
                        warnings.push(format!(
                            "flow.json: \"{name}.{field}\" must be an element name or null"
                        ));
                        continue;
                    }
                };
                match (field.as_str(), target) {
                    ("enter", Some(k)) => row.enter = k,
                    ("empty_enter", Some(k)) => row.empty_enter = k,
                    ("tab", t) => row.tab = t,
                    ("shift_tab", t) => row.shift_tab = t,
                    ("enter" | "empty_enter", None) => {
                        warnings.push(format!("flow.json: \"{name}.{field}\" cannot be null"))
                    }
                    _ => warnings.push(format!(
                        "flow.json: unknown field \"{name}.{field}\" ignored"
                    )),
                }
            }
            self.rows.insert(kind, row);
        }
        warnings
    }
}

/// The short label for a kind, for the hint bar and menus.
pub fn kind_label(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::SceneHeading => "Scene Heading",
        ElementKind::Action => "Action",
        ElementKind::Character => "Character",
        ElementKind::Parenthetical => "Parenthetical",
        ElementKind::Dialogue => "Dialogue",
        ElementKind::Transition => "Transition",
        ElementKind::Lyric => "Lyric",
        ElementKind::Shot => "Shot",
        ElementKind::General => "General",
        ElementKind::PageBreak => "Page Break",
        ElementKind::Section => "Section",
        ElementKind::Synopsis => "Synopsis",
        ElementKind::Note => "Note",
        ElementKind::Boneyard => "Boneyard",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ElementKind::*;

    #[test]
    fn the_brief_s_flow() {
        let t = FlowTable::default();
        assert_eq!(t.row(SceneHeading).enter, Action);
        assert_eq!(t.row(Character).enter, Dialogue);
        assert_eq!(t.row(Dialogue).enter, Character);
        assert_eq!(t.row(Dialogue).empty_enter, Action);
        assert_eq!(t.row(Dialogue).tab, Some(Parenthetical));
        assert_eq!(t.row(Section).tab, None);
    }

    #[test]
    fn overrides_change_cells_and_report_mistakes() {
        let mut t = FlowTable::default();
        let w = t.apply_overrides(
            r#"{"v": 1, "kinds": {
                "dialogue": {"enter": "action", "tab": null},
                "nonsense": {"enter": "action"},
                "action": {"tab": "transition", "color": "red", "enter": "lyricz"}
            }}"#,
        );
        assert_eq!(t.row(Dialogue).enter, Action);
        assert_eq!(t.row(Dialogue).tab, None);
        assert_eq!(t.row(Action).tab, Some(Transition));
        assert_eq!(t.row(Action).enter, Action);
        assert_eq!(w.len(), 3, "{w:?}");
        assert_eq!(t.clone().apply_overrides("{").len(), 1);
        assert_eq!(
            t.clone().apply_overrides(r#"{"v": 2, "kinds": {}}"#).len(),
            1
        );
    }
}
