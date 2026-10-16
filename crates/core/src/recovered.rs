//! Recovered versions: the copies Cold Open sets aside rather than delete
//! (docs/specs/journal.md, "Recovered versions"), found by their names,
//! read, and cleaned up by the rule writers can rely on.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::document::Document;
use crate::format::FormatId;

/// What a copy is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Unsaved changes set aside: Don't save, Discard, a revert.
    Discarded,
    /// A journal whose header could not be read.
    Corrupt,
    /// A journal whose replay stopped early.
    Diverged,
    /// A file the reader repaired, kept as it was before the first save
    /// over it (docs/specs/journal.md, "Damaged originals").
    Original,
}

/// One set-aside copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Copy {
    pub copy: PathBuf,
    /// The script it belongs to.
    pub script: PathBuf,
    pub kind: Kind,
    /// When it was made, in milliseconds since the Unix epoch, from its
    /// name.
    pub at: u64,
}

/// Copies older than this may be cleaned up, once offered.
pub const KEEP_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// The copy a path names, if its name is one Cold Open gives a copy:
/// `.<script>.coldopen-journal.<tag>-<ms>` or
/// `.<script>.coldopen-original-<ms>`.
pub fn parse(path: &Path) -> Option<Copy> {
    let name = path.file_name()?.to_str()?;
    let name = name.strip_prefix('.')?;
    let digits = |s: &str| -> Option<u64> {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let (script, kind, at) = if let Some(i) = name.rfind(".coldopen-journal.") {
        let rest = &name[i + ".coldopen-journal.".len()..];
        let (tag, ms) = rest.rsplit_once('-')?;
        let kind = match tag {
            "discarded" => Kind::Discarded,
            "corrupt" => Kind::Corrupt,
            "diverged" => Kind::Diverged,
            _ => return None,
        };
        (&name[..i], kind, digits(ms)?)
    } else {
        let i = name.rfind(".coldopen-original-")?;
        let ms = &name[i + ".coldopen-original-".len()..];
        (&name[..i], Kind::Original, digits(ms)?)
    };
    if script.is_empty() {
        return None;
    }
    Some(Copy {
        copy: path.to_path_buf(),
        script: path.with_file_name(script),
        kind,
        at,
    })
}

/// Every copy in a folder, newest first.
pub fn in_folder(dir: &Path) -> Vec<Copy> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Copy> = entries
        .filter_map(Result::ok)
        .filter_map(|e| parse(&e.path()))
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.copy.cmp(&b.copy)));
    out
}

/// A script's copies, newest first.
pub fn of_script(script: &Path) -> Vec<Copy> {
    let dir = match script.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let name = script.file_name();
    in_folder(dir)
        .into_iter()
        .filter(|c| c.script.file_name() == name)
        .collect()
}

/// The document a copy holds and the format it was in: a journal copy
/// replayed as recovery replays it, without touching the disk; a damaged
/// original read as its script is read.
pub fn read(copy: &Copy) -> Result<(Document, FormatId), String> {
    let bytes = fs::read(&copy.copy).map_err(|e| e.to_string())?;
    match copy.kind {
        Kind::Original => {
            let format = FormatId::for_path(&copy.script).unwrap_or_default();
            let read = format.get().read(&bytes).map_err(|e| e.message)?;
            Ok((read.doc, format))
        }
        _ => crate::dispatcher::replay_copy(&copy.script, &bytes),
    }
}

/// Whether a copy can be restored, judged cheaply: a journal copy's
/// header reads.
pub fn readable(copy: &Copy) -> bool {
    match copy.kind {
        Kind::Original => copy.copy.is_file(),
        _ => fs::read(&copy.copy)
            .ok()
            .is_some_and(|b| crate::journal::read(&b).is_ok()),
    }
}

/// The copies the start-up clean-up deletes (docs/specs/journal.md,
/// "Recovered versions"): made more than 30 days before `now`, and shown
/// to the writer at least once (`offered` maps a copy's path to when it
/// was first shown). A copy never shown is never deleted.
pub fn expired<'a>(copies: &'a [Copy], offered: &BTreeMap<String, u64>, now: u64) -> Vec<&'a Copy> {
    copies
        .iter()
        .filter(|c| now.saturating_sub(c.at) > KEEP_MS)
        .filter(|c| offered.contains_key(&c.copy.display().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_are_known_by_their_names() {
        let c = parse(Path::new(
            "dir/.My Script.fdx.coldopen-journal.discarded-1760000000000",
        ))
        .unwrap();
        assert_eq!(c.kind, Kind::Discarded);
        assert_eq!(c.script, Path::new("dir/My Script.fdx"));
        assert_eq!(c.at, 1_760_000_000_000);
        let c = parse(Path::new(".a.fountain.coldopen-original-5")).unwrap();
        assert_eq!((c.kind, c.at), (Kind::Original, 5));
        for not in [
            ".a.fountain.coldopen-journal",
            ".a.fountain.coldopen-journal.other-5",
            ".a.fountain.coldopen-journal.discarded-",
            ".a.fountain.coldopen-journal.discarded-5x",
            "a.fountain.coldopen-journal.discarded-5",
            ".coldopen-original-5",
            "notes.txt",
        ] {
            assert!(parse(Path::new(not)).is_none(), "{not}");
        }
    }

    #[test]
    fn only_old_copies_already_shown_expire() {
        let day = 24 * 60 * 60 * 1000;
        let now = 100 * day;
        let copies: Vec<Copy> = [(1, "old-shown"), (2, "old-unseen"), (90 * day, "new-shown")]
            .iter()
            .map(|(at, n)| Copy {
                copy: PathBuf::from(n),
                script: PathBuf::from("s"),
                kind: Kind::Discarded,
                at: *at,
            })
            .collect();
        let offered: BTreeMap<String, u64> =
            [("old-shown".to_string(), 1), ("new-shown".to_string(), 1)].into();
        let gone: Vec<&str> = expired(&copies, &offered, now)
            .iter()
            .map(|c| c.copy.to_str().unwrap())
            .collect();
        assert_eq!(gone, ["old-shown"]);
    }
}
