//! A first start on a clean machine (docs/specs/release.md §6): the app's
//! folders are where each OS expects them, and nothing is created anywhere
//! else.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use coldopen_app::files::{Dialogs, Dirs};
use coldopen_app::headless::Headless;
use coldopen_app::{App, Boot};

/// The environment of a user whose home is `root`, as each OS sets it.
fn clean_env(root: &Path) -> BTreeMap<&'static str, OsString> {
    let mut env = BTreeMap::new();
    if cfg!(windows) {
        env.insert("USERPROFILE", root.into());
        env.insert("APPDATA", root.join("AppData/Roaming").into());
        env.insert("LOCALAPPDATA", root.join("AppData/Local").into());
    } else {
        env.insert("HOME", root.into());
    }
    env
}

fn everything_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p.clone());
            }
            out.push(p);
        }
    }
    out.sort();
    out
}

#[test]
fn a_first_start_makes_only_the_apps_own_folders() {
    let root = tempfile::tempdir().unwrap();
    let env = clean_env(root.path());
    let dirs = Dirs::find_in(&|k| env.get(k).cloned()).expect("folders for this user");
    let r = root.path();
    if cfg!(windows) {
        assert_eq!(dirs.config, r.join("AppData/Roaming").join("Cold Open"));
        assert_eq!(dirs.data, r.join("AppData/Local").join("Cold Open"));
        assert_eq!(
            dirs.run,
            r.join("AppData/Local").join("Cold Open").join("run")
        );
    } else if cfg!(target_os = "macos") {
        let base = r.join("Library/Application Support/Cold Open");
        assert_eq!((&dirs.config, &dirs.data), (&base, &base));
        assert_eq!(dirs.run, base.join("run"));
    } else {
        assert_eq!(dirs.config, r.join(".config/coldopen"));
        assert_eq!(dirs.data, r.join(".local/share/coldopen"));
        assert_eq!(dirs.run, r.join(".local/share/coldopen/run"));
    }
    assert!(everything_under(r).is_empty());

    // The app starts with nothing there, on an untitled script, and the
    // writer types a line.
    let mut app = App::new(Boot {
        dirs: Some(dirs.clone()),
        ..Boot::default()
    });
    app.set_dialogs(Dialogs::scripted(|_| None));
    let mut h = Headless::new(app, iced::Size::new(1000.0, 700.0)).unwrap();
    assert!(h.app.d.session().is_some(), "a new script is open");
    h.type_text("INT. HOUSE - DAY");
    h.frame(1.0);
    h.app.d.discard();
    drop(h);

    let made = everything_under(r);
    assert!(!made.is_empty());
    for p in &made {
        let ours = [&dirs.config, &dirs.data, &dirs.run]
            .iter()
            .any(|d| p.starts_with(d) || d.starts_with(p));
        assert!(
            ours,
            "a first start made {} outside the app's folders",
            p.display()
        );
    }
}
