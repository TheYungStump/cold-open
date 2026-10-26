//! Keyboard shortcuts: the built-in map and the writer's `keymap.json`
//! (docs/specs/editor.md §10). The file holds changes only, is never
//! rewritten by the app, and a bad entry never stops the app.

use std::collections::BTreeMap;

use iced::keyboard::{self, Key, Modifiers, key::Named};
use serde_json::Value;

/// A key with its modifiers, as written in the keymap (`Mod+Shift+K`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    /// The key, lower-cased: a letter, digit, punctuation or a named key.
    pub key: String,
}

/// Whether "Mod" means Cmd (macOS) or Ctrl (Windows, Linux).
pub const MAC: bool = cfg!(target_os = "macos");

/// The largest keymap file read.
const MAX_FILE: usize = 1 << 20;

const NAMED: [(&str, Named); 15] = [
    ("enter", Named::Enter),
    ("tab", Named::Tab),
    ("escape", Named::Escape),
    ("backspace", Named::Backspace),
    ("delete", Named::Delete),
    ("insert", Named::Insert),
    ("home", Named::Home),
    ("end", Named::End),
    ("pageup", Named::PageUp),
    ("pagedown", Named::PageDown),
    ("up", Named::ArrowUp),
    ("down", Named::ArrowDown),
    ("left", Named::ArrowLeft),
    ("right", Named::ArrowRight),
    ("space", Named::Space),
];

impl Chord {
    /// Reads `Mod+Shift+K` and the like. `None` for anything malformed.
    pub fn parse(s: &str) -> Option<Chord> {
        let mut c = Chord {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
            key: String::new(),
        };
        let parts: Vec<&str> = s.split('+').collect();
        // A lone "+" key: "Mod++" splits into ["Mod", "", ""].
        let (mods, key) = if s.ends_with("++") {
            (&parts[..parts.len().saturating_sub(2)], "+")
        } else {
            let (k, m) = parts.split_last()?;
            (m, *k)
        };
        for m in mods {
            match m.trim().to_ascii_lowercase().as_str() {
                "mod" => {
                    if MAC {
                        c.logo = true;
                    } else {
                        c.ctrl = true;
                    }
                }
                "ctrl" | "control" => c.ctrl = true,
                "alt" | "option" => c.alt = true,
                "shift" => c.shift = true,
                "super" | "cmd" | "win" | "logo" => c.logo = true,
                _ => return None,
            }
        }
        let key = key.trim();
        let lower = key.to_ascii_lowercase();
        let ok = NAMED.iter().any(|(n, _)| *n == lower)
            || (lower.starts_with('f')
                && lower[1..]
                    .parse::<u8>()
                    .is_ok_and(|n| (1..=24).contains(&n)))
            || key.chars().count() == 1;
        if !ok {
            return None;
        }
        c.key = lower;
        Some(c)
    }

    /// The chord for a key press, or `None` for keys alone that type text
    /// (no Ctrl, Alt or Logo, and not a named or function key).
    pub fn from_key(key: &Key, m: Modifiers) -> Option<Chord> {
        let name = match key.as_ref() {
            keyboard::key::Key::Named(n) => {
                if let Some((s, _)) = NAMED.iter().find(|(_, k)| *k == n) {
                    (*s).to_string()
                } else {
                    let dbg = format!("{n:?}");
                    if dbg.starts_with('F') && dbg[1..].parse::<u8>().is_ok() {
                        dbg.to_ascii_lowercase()
                    } else {
                        return None;
                    }
                }
            }
            keyboard::key::Key::Character(s) => s.to_lowercase(),
            keyboard::key::Key::Unidentified => return None,
        };
        Some(Chord {
            ctrl: m.control(),
            alt: m.alt(),
            shift: m.shift(),
            logo: m.logo(),
            key: name,
        })
    }

    /// How the chord reads in menus and the palette.
    pub fn label(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str(if MAC { "Option+" } else { "Alt+" });
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.logo {
            s.push_str(if MAC { "Cmd+" } else { "Super+" });
        }
        let k = match self.key.as_str() {
            "pageup" => "Page Up".to_string(),
            "pagedown" => "Page Down".to_string(),
            k if k.len() == 1 => k.to_uppercase(),
            k => {
                let mut cs = k.chars();
                cs.next()
                    .map(|f| f.to_uppercase().collect::<String>() + cs.as_str())
                    .unwrap_or_default()
            }
        };
        s.push_str(&k);
        s
    }
}

/// The built-in shortcuts (editor.md §10).
pub fn defaults() -> Vec<(&'static str, &'static str)> {
    let mut v = vec![
        ("Mod+K", "app.palette"),
        ("Mod+Shift+P", "app.palette"),
        ("Mod+N", "app.new"),
        ("Mod+O", "app.open"),
        ("Mod+S", "doc.save"),
        ("Mod+Shift+S", "app.save_as"),
        ("Mod+W", "app.close"),
        ("Mod+Shift+E", "app.export_pdf"),
        ("Mod+Q", "app.quit"),
        ("Mod+Z", "undo"),
        ("Mod+Shift+Z", "redo"),
        ("Mod+Alt+Z", "app.undo_agent"),
        ("Mod+X", "edit.cut"),
        ("Mod+C", "edit.copy"),
        ("Mod+V", "edit.paste"),
        ("Mod+A", "edit.select_all"),
        ("Mod+B", "edit.bold"),
        ("Mod+I", "edit.italic"),
        ("Mod+U", "edit.underline"),
        ("Mod+1", "element.scene_heading"),
        ("Mod+2", "element.action"),
        ("Mod+3", "element.character"),
        ("Mod+4", "element.parenthetical"),
        ("Mod+5", "element.dialogue"),
        ("Mod+6", "element.transition"),
        ("Mod+7", "element.shot"),
        ("Mod+8", "element.general"),
        ("Mod+F", "edit.find"),
        ("F3", "edit.find_next"),
        ("Mod+G", "edit.find_next"),
        ("Shift+F3", "edit.find_previous"),
        ("Mod+Shift+G", "edit.find_previous"),
        ("Mod+=", "view.zoom_in"),
        ("Mod+-", "view.zoom_out"),
        ("Mod+0", "view.zoom_reset"),
        ("Mod+Shift+D", "view.draft"),
        ("Mod+\\", "view.navigator"),
        ("Mod+Shift+F", "view.focus"),
        ("F6", "view.next_pane"),
        ("Alt+PageUp", "nav.previous_scene"),
        ("Alt+PageDown", "nav.next_scene"),
        ("Mod+Shift+H", "app.history"),
        ("Mod+Shift+Enter", "changes.accept_here"),
        ("Mod+Shift+Backspace", "changes.reject_here"),
        ("F7", "changes.next"),
        ("Shift+F7", "changes.previous"),
        ("Mod+Shift+R", "view.changes"),
        ("Mod+Shift+I", "app.reports"),
        ("Mod+Shift+T", "app.title_page"),
    ];
    if MAC {
        v.push(("Mod+Alt+F", "edit.replace"));
    } else {
        v.push(("Mod+H", "edit.replace"));
        v.push(("Mod+Y", "redo"));
    }
    v
}

/// Shortcuts by chord.
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    map: BTreeMap<Chord, String>,
}

impl Keymap {
    /// The built-in map with the writer's file applied. Returns warnings
    /// for every entry that was ignored.
    pub fn load(file: Option<&str>, known: &dyn Fn(&str) -> bool) -> (Keymap, Vec<String>) {
        let mut km = Keymap::default();
        for (k, a) in defaults() {
            if let Some(c) = Chord::parse(k) {
                km.map.insert(c, a.to_string());
            }
        }
        let warnings = match file {
            Some(text) => km.apply(text, known),
            None => Vec::new(),
        };
        (km, warnings)
    }

    fn apply(&mut self, text: &str, known: &dyn Fn(&str) -> bool) -> Vec<String> {
        if text.len() > MAX_FILE {
            return vec!["keymap.json is over 1 MiB and was ignored".into()];
        }
        let v: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => return vec![format!("keymap.json is not valid JSON: {e}")],
        };
        if v.get("v").and_then(Value::as_u64) != Some(1) {
            return vec!["keymap.json needs \"v\": 1; the file was ignored".into()];
        }
        let Some(list) = v.get("bindings").and_then(Value::as_array) else {
            return vec!["keymap.json has no \"bindings\" list; the file was ignored".into()];
        };
        let mut warnings = Vec::new();
        for (i, b) in list.iter().enumerate() {
            let Some(keys) = b.get("keys").and_then(Value::as_str) else {
                warnings.push(format!("keymap.json: binding {} has no \"keys\"", i + 1));
                continue;
            };
            let Some(chord) = Chord::parse(keys) else {
                warnings.push(format!("keymap.json: \"{keys}\" is not a key I know"));
                continue;
            };
            match b.get("action") {
                Some(Value::Null) => {
                    self.map.remove(&chord);
                }
                Some(Value::String(a)) if known(a) => {
                    self.map.insert(chord, a.clone());
                }
                Some(Value::String(a)) => {
                    warnings.push(format!(
                        "keymap.json: unknown action \"{a}\" for \"{keys}\""
                    ));
                }
                _ => warnings.push(format!(
                    "keymap.json: \"{keys}\" needs an \"action\" name or null"
                )),
            }
        }
        warnings
    }

    pub fn action(&self, chord: &Chord) -> Option<&str> {
        self.map.get(chord).map(String::as_str)
    }

    /// The shortcuts for an action, for menus and the palette.
    pub fn keys_for(&self, action: &str) -> Vec<String> {
        self.map
            .iter()
            .filter(|(_, a)| a.as_str() == action)
            .map(|(c, _)| c.label())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(a: &str) -> bool {
        a.contains('.') || a == "undo"
    }

    #[test]
    fn chords_parse_and_print() {
        let c = Chord::parse("Mod+Shift+k").unwrap();
        assert_eq!(c.key, "k");
        assert!(c.shift);
        assert!(Chord::parse("Mod+").is_none());
        assert!(Chord::parse("Hyper+K").is_none());
        assert!(Chord::parse("F24").is_some());
        assert!(Chord::parse("F25").is_none());
        assert_eq!(Chord::parse("Mod++").unwrap().key, "+");
        assert_eq!(
            Chord::parse("Alt+PageDown").unwrap().label(),
            if MAC {
                "Option+Page Down"
            } else {
                "Alt+Page Down"
            }
        );
    }

    #[test]
    fn the_file_changes_and_removes_bindings() {
        let (km, w) = Keymap::load(
            Some(
                r#"{"v": 1, "bindings": [
                    {"keys": "Mod+Shift+K", "action": "app.palette"},
                    {"keys": "Mod+1", "action": null},
                    {"keys": "Mod+J", "action": "no_such_thing"},
                    {"keys": "Bogus+J", "action": "app.palette"}
                ]}"#,
            ),
            &known,
        );
        assert_eq!(w.len(), 2, "{w:?}");
        assert_eq!(
            km.action(&Chord::parse("Mod+Shift+K").unwrap()),
            Some("app.palette")
        );
        assert_eq!(km.action(&Chord::parse("Mod+1").unwrap()), None);
        // Untouched defaults remain.
        assert_eq!(km.action(&Chord::parse("Mod+Z").unwrap()), Some("undo"));
        assert_eq!(Keymap::load(Some("[1,2"), &known).1.len(), 1);
        assert_eq!(
            Keymap::load(Some(&"x".repeat(MAX_FILE + 1)), &known)
                .1
                .len(),
            1
        );
    }
}
