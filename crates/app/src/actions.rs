//! App actions: everything the palette lists and the keymap binds
//! (docs/specs/editor.md §9, §10). Core commands are actions too, by their
//! dotted names; app actions change only the view or the app's own state.

use coldopen_core::Command;

/// An app action with its title.
pub struct ActionInfo {
    pub name: &'static str,
    pub title: &'static str,
}

/// App actions, in palette order.
pub const APP_ACTIONS: &[ActionInfo] = &[
    ActionInfo {
        name: "app.palette",
        title: "Show the command palette",
    },
    ActionInfo {
        name: "app.new",
        title: "New script",
    },
    ActionInfo {
        name: "app.open",
        title: "Open a script",
    },
    ActionInfo {
        name: "app.save_as",
        title: "Save as",
    },
    ActionInfo {
        name: "app.close",
        title: "Close the script",
    },
    ActionInfo {
        name: "app.export_pdf",
        title: "Export a PDF",
    },
    ActionInfo {
        name: "app.quit",
        title: "Quit",
    },
    ActionInfo {
        name: "app.undo_agent",
        title: "Undo agent edits",
    },
    ActionInfo {
        name: "app.history",
        title: "Show the undo history",
    },
    ActionInfo {
        name: "app.agent_access",
        title: "Turn agent access on or off",
    },
    ActionInfo {
        name: "app.agent_disconnect",
        title: "Disconnect agents",
    },
    ActionInfo {
        name: "app.dismiss_marks",
        title: "Dismiss agent marks",
    },
    ActionInfo {
        name: "app.dismiss_messages",
        title: "Dismiss messages (also Esc)",
    },
    ActionInfo {
        name: "app.renderer_cpu",
        title: "Draw with the CPU (less memory; after a restart)",
    },
    ActionInfo {
        name: "app.renderer_gpu",
        title: "Draw with the GPU (smoother on 4K and Retina; after a restart)",
    },
    ActionInfo {
        name: "app.about",
        title: "About Cold Open and licenses",
    },
    ActionInfo {
        name: "edit.cut",
        title: "Cut",
    },
    ActionInfo {
        name: "edit.copy",
        title: "Copy",
    },
    ActionInfo {
        name: "edit.paste",
        title: "Paste",
    },
    ActionInfo {
        name: "edit.select_all",
        title: "Select all",
    },
    ActionInfo {
        name: "edit.bold",
        title: "Bold",
    },
    ActionInfo {
        name: "edit.italic",
        title: "Italic",
    },
    ActionInfo {
        name: "edit.underline",
        title: "Underline",
    },
    ActionInfo {
        name: "edit.find",
        title: "Find",
    },
    ActionInfo {
        name: "edit.replace",
        title: "Find and replace",
    },
    ActionInfo {
        name: "edit.find_next",
        title: "Find next",
    },
    ActionInfo {
        name: "edit.find_previous",
        title: "Find previous",
    },
    ActionInfo {
        name: "element.scene_heading",
        title: "Element: Scene Heading",
    },
    ActionInfo {
        name: "element.action",
        title: "Element: Action",
    },
    ActionInfo {
        name: "element.character",
        title: "Element: Character",
    },
    ActionInfo {
        name: "element.parenthetical",
        title: "Element: Parenthetical",
    },
    ActionInfo {
        name: "element.dialogue",
        title: "Element: Dialogue",
    },
    ActionInfo {
        name: "element.transition",
        title: "Element: Transition",
    },
    ActionInfo {
        name: "element.shot",
        title: "Element: Shot",
    },
    ActionInfo {
        name: "element.general",
        title: "Element: General",
    },
    ActionInfo {
        name: "element.lyric",
        title: "Element: Lyric",
    },
    ActionInfo {
        name: "element.dual",
        title: "Toggle dual dialogue",
    },
    ActionInfo {
        name: "view.zoom_in",
        title: "Zoom in",
    },
    ActionInfo {
        name: "view.zoom_out",
        title: "Zoom out",
    },
    ActionInfo {
        name: "view.zoom_reset",
        title: "Actual size",
    },
    ActionInfo {
        name: "view.draft",
        title: "Toggle the draft view",
    },
    ActionInfo {
        name: "view.navigator",
        title: "Show or hide the navigator",
    },
    ActionInfo {
        name: "view.focus",
        title: "Focus mode",
    },
    ActionInfo {
        name: "view.next_pane",
        title: "Move focus to the next pane",
    },
    ActionInfo {
        name: "view.theme_system",
        title: "Theme: follow the system",
    },
    ActionInfo {
        name: "view.theme_light",
        title: "Theme: light",
    },
    ActionInfo {
        name: "view.theme_dark",
        title: "Theme: dark",
    },
    ActionInfo {
        name: "view.theme_high_contrast",
        title: "Theme: high contrast",
    },
    ActionInfo {
        name: "nav.previous_scene",
        title: "Go to the previous scene",
    },
    ActionInfo {
        name: "nav.next_scene",
        title: "Go to the next scene",
    },
    ActionInfo {
        name: "nav.move_scene_up",
        title: "Move the current scene up",
    },
    ActionInfo {
        name: "nav.move_scene_down",
        title: "Move the current scene down",
    },
];

/// A short title for a core command.
pub fn command_title(name: &str) -> &'static str {
    match name {
        "doc.new" => "New document (params)",
        "doc.open" => "Open a file by path",
        "doc.save" => "Save",
        "doc.close" => "Close the document (params)",
        "doc.get" => "Show the document as JSON",
        "doc.export" => "Export as text",
        "doc.revert" => "Revert to the saved file",
        "block.insert" => "Insert a block",
        "block.set_type" => "Change a block's element",
        "block.set_text" => "Set a block's text",
        "block.set_attrs" => "Set a block's attributes",
        "block.delete" => "Delete a block",
        "block.split" => "Split a block",
        "block.join" => "Join a block to the one before",
        "text.replace" => "Replace text",
        "text.insert" => "Insert text",
        "text.delete" => "Delete text",
        "text.style" => "Style text",
        "text.find" => "Find text",
        "text.complete" => "Suggest completions",
        "scene.list" => "List scenes",
        "scene.move" => "Move a scene",
        "export.pdf" => "Export a PDF by path",
        "undo" => "Undo",
        "redo" => "Redo",
        "batch" => "Run several edits as one",
        "history.list" => "List the undo history",
        _ => "Command",
    }
}

/// Whether a name is an action the keymap may bind.
pub fn known(name: &str) -> bool {
    APP_ACTIONS.iter().any(|a| a.name == name) || Command::NAMES.contains(&name)
}

/// Everything the palette lists: (name, title).
pub fn all() -> Vec<(&'static str, &'static str)> {
    let mut v: Vec<(&'static str, &'static str)> =
        APP_ACTIONS.iter().map(|a| (a.name, a.title)).collect();
    for n in Command::NAMES {
        v.push((n, command_title(n)));
    }
    v
}

/// Subsequence match score for the palette: higher is better, `None` for
/// no match. Matches at word starts and runs score higher.
pub fn score(query: &str, name: &str, title: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let hay = format!("{title} {name}").to_lowercase();
    let q = query.to_lowercase();
    let hc: Vec<char> = hay.chars().collect();
    let mut score = 0;
    let mut i = 0;
    let mut prev_hit: Option<usize> = None;
    for qc in q.chars() {
        let mut found = None;
        while i < hc.len() {
            if hc[i] == qc {
                found = Some(i);
                break;
            }
            i += 1;
        }
        let j = found?;
        score += 1;
        if j == 0 || !hc[j - 1].is_alphanumeric() {
            score += 5;
        }
        if prev_hit == Some(j.wrapping_sub(1)) {
            score += 3;
        }
        prev_hit = Some(j);
        i = j + 1;
    }
    if hay.starts_with(&q) {
        score += 20;
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_and_action_is_listed_once() {
        let all = all();
        for n in Command::NAMES {
            assert_eq!(all.iter().filter(|(a, _)| *a == n).count(), 1, "{n}");
            assert_ne!(command_title(n), "Command", "{n} needs a title");
        }
        for (k, _) in crate::keymap::defaults() {
            let _ = k;
        }
        for (_, a) in crate::keymap::defaults() {
            assert!(known(a), "{a}");
        }
    }

    #[test]
    fn palette_scoring_prefers_word_starts() {
        let z = score("zi", "view.zoom_in", "Zoom in").unwrap();
        let other = score("zi", "app.quit", "Quit").unwrap_or(-1);
        assert!(z > other);
        assert!(score("qqq", "app.quit", "Quit").is_none());
    }
}
