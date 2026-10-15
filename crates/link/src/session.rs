//! Session files: where the app listens, its token, and the scripts it has
//! open (docs/specs/mcp.md §5.1).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::transport::Conn;

/// One running app's session file, `<run>/<pid>.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionFile {
    pub v: u32,
    pub pid: u32,
    /// The app's version.
    pub app: String,
    pub endpoint: String,
    /// 64 hex digits, new each time the app starts.
    pub token: String,
    /// Canonical paths of the scripts open in the app.
    pub docs: Vec<PathBuf>,
}

/// `n` random bytes from the operating system, as hex.
pub fn random_hex(n: usize) -> io::Result<String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// A new 256-bit token.
pub fn new_token() -> io::Result<String> {
    random_hex(32)
}

/// A path as both sides compare it: canonical when it exists, otherwise
/// its folder's canonical form with the file name (a script never saved).
pub fn canonical(path: &Path) -> PathBuf {
    if let Ok(p) = fs::canonicalize(path) {
        return p;
    }
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    match (abs.parent(), abs.file_name()) {
        (Some(dir), Some(name)) => fs::canonicalize(dir)
            .map(|d| d.join(name))
            .unwrap_or(abs.clone()),
        _ => abs,
    }
}

impl SessionFile {
    pub fn path_in(run: &Path, pid: u32) -> PathBuf {
        run.join(format!("{pid}.json"))
    }

    /// Writes the file atomically: a temporary file only this user can
    /// read, then a rename over the old one.
    pub fn write(&self, run: &Path) -> io::Result<PathBuf> {
        fs::create_dir_all(run)?;
        let target = SessionFile::path_in(run, self.pid);
        let tmp = run.join(format!(".{}.json.{}", self.pid, random_hex(4)?));
        let text = serde_json::to_string(self).map_err(io::Error::other)?;
        let mut o = fs::OpenOptions::new();
        o.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let written = o.open(&tmp).and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        });
        let renamed = written.and_then(|()| fs::rename(&tmp, &target));
        if renamed.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        renamed.map(|()| target)
    }

    pub fn read(path: &Path) -> Option<SessionFile> {
        let text = fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Whether this file lists the script.
    pub fn has_doc(&self, doc: &Path) -> bool {
        let want = canonical(doc);
        self.docs.iter().any(|d| d == &want)
    }
}

fn session_files(run: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(run) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.parse::<u32>().is_ok())
        })
        .collect();
    v.sort();
    v
}

/// Session files in `run` that list the script, newest first.
pub fn find_sessions(run: &Path, doc: &Path) -> Vec<SessionFile> {
    let mut found: Vec<(std::time::SystemTime, SessionFile)> = session_files(run)
        .into_iter()
        .filter_map(|p| {
            let s = SessionFile::read(&p)?;
            let t = fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            s.has_doc(doc).then_some((t, s))
        })
        .collect();
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    found.into_iter().map(|(_, s)| s).collect()
}

/// Deletes session files whose app is gone: nothing answers at their
/// endpoint. Our own process's file is left alone.
pub fn remove_stale(run: &Path, own_pid: u32) {
    for p in session_files(run) {
        let Some(s) = SessionFile::read(&p) else {
            let _ = fs::remove_file(&p);
            continue;
        };
        if s.pid == own_pid {
            continue;
        }
        if Conn::connect(&s.endpoint).is_err() {
            let _ = fs::remove_file(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_find_and_clean_up() {
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path().join("run");
        let script = dir.path().join("a.fountain");
        fs::write(&script, "x").unwrap();
        let s = SessionFile {
            v: 1,
            pid: 4242,
            app: "0.1.0".into(),
            endpoint: "nowhere-at-all".into(),
            token: new_token().unwrap(),
            docs: vec![canonical(&script)],
        };
        assert_eq!(s.token.len(), 64);
        let p = s.write(&run).unwrap();
        assert_eq!(SessionFile::read(&p).unwrap(), s);
        // A different spelling of the same path still matches.
        let other = dir.path().join(".").join("a.fountain");
        assert_eq!(find_sessions(&run, &other), std::slice::from_ref(&s));
        assert!(find_sessions(&run, &dir.path().join("b.fountain")).is_empty());
        // Nobody listens at its endpoint, so it is stale.
        remove_stale(&run, 1);
        assert!(!p.exists());
    }

    #[test]
    fn a_missing_file_canonicalizes_by_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("never-saved.fountain");
        assert_eq!(
            canonical(&p),
            fs::canonicalize(dir.path())
                .unwrap()
                .join("never-saved.fountain")
        );
    }
}
