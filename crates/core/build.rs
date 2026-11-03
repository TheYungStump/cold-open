//! Names the commit this build comes from (docs/specs/release.md §9):
//! `COLDOPEN_COMMIT` when set (CI sets it), else `git` when the build runs
//! in a clone, else `unknown`. It is information only; nothing Cold Open
//! writes to a script or journal depends on it.

use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=COLDOPEN_COMMIT");
    let commit = std::env::var("COLDOPEN_COMMIT")
        .ok()
        .and_then(|c| hex12(&c))
        .or_else(from_git)
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=COLDOPEN_COMMIT={commit}");
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=COLDOPEN_TARGET={target}");
}

/// The first 12 digits of a commit id, if `s` is one.
fn hex12(s: &str) -> Option<String> {
    let s = s.trim();
    (s.len() >= 12 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| s[..12].to_ascii_lowercase())
}

fn git(args: &[&str]) -> Option<String> {
    let dir = std::env::var_os("CARGO_MANIFEST_DIR")?;
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn from_git() -> Option<String> {
    let commit = hex12(&git(&["rev-parse", "HEAD"])?)?;
    // Build again when HEAD moves: HEAD itself, the branch it names, and
    // packed refs, wherever this clone or worktree keeps them.
    let mut watch = vec!["HEAD".to_string(), "packed-refs".to_string()];
    if let Some(r) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watch.push(r);
    }
    for name in watch {
        if let Some(p) = git(&["rev-parse", "--path-format=absolute", "--git-path", &name])
            && Path::new(&p).exists()
        {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    Some(commit)
}
