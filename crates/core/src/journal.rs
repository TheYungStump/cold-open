//! The crash journal (docs/specs/journal.md): a locked sidecar file holding
//! a snapshot of the last saved document followed by every applied command.

use std::fs::{self, File, TryLockError};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::author::Author;
use crate::document::Document;
use crate::io::{Fingerprint, open_rw};

pub const FORMAT: &str = "coldopen";
pub const VERSION: u32 = 1;
/// The command name of a save marker (docs/specs/journal.md).
pub const SAVED: &str = "doc.saved";

/// `.script.fountain.coldopen-journal` next to `script.fountain`.
pub fn path_for(doc: &Path) -> PathBuf {
    let name = doc
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    doc.with_file_name(format!(".{name}.coldopen-journal"))
}

#[derive(Serialize, Deserialize)]
pub struct Header {
    pub journal: String,
    pub v: u32,
    pub app: String,
    pub doc: Document,
    pub disk: Option<Fingerprint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub t: u64,
    pub command: String,
    pub params: Value,
    pub result: Value,
    /// Absent for a writer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<Author>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub merge: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Entry {
    /// The fingerprint a `doc.saved` marker records, or `None` for any
    /// other entry.
    pub fn saved_disk(&self) -> Option<Fingerprint> {
        if self.command != SAVED {
            return None;
        }
        serde_json::from_value(self.params.get("disk")?.clone()).ok()
    }
}

pub enum OpenError {
    /// Another process holds the journal.
    Locked,
    Io(io::Error),
}

/// An open, locked journal.
#[derive(Debug)]
pub struct Journal {
    file: File,
    path: PathBuf,
    seq: u64,
    fsync: bool,
}

impl Journal {
    /// Opens (creating if needed) and locks the journal at `path`. Returns
    /// it with its current contents, read through the locked handle.
    pub fn open_locked(path: &Path, fsync: bool) -> Result<(Journal, Vec<u8>), OpenError> {
        let mut file = open_rw(path).map_err(OpenError::Io)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(OpenError::Locked),
            Err(TryLockError::Error(e)) => return Err(OpenError::Io(e)),
        }
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).map_err(OpenError::Io)?;
        Ok((
            Journal {
                file,
                path: path.to_path_buf(),
                seq: 0,
                fsync,
            },
            contents,
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Starts over: truncates the journal and writes a header with `doc`
    /// as the new base. Used after open and after every save.
    pub fn start(&mut self, doc: &Document, disk: Option<Fingerprint>) -> io::Result<()> {
        let header = Header {
            journal: FORMAT.into(),
            v: VERSION,
            app: env!("CARGO_PKG_VERSION").into(),
            doc: doc.clone(),
            disk,
        };
        let mut line = serde_json::to_vec(&header).map_err(io::Error::other)?;
        line.push(b'\n');
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&line)?;
        self.file.sync_data()?;
        self.seq = 0;
        Ok(())
    }

    /// Continues an existing journal after recovery: cuts any torn or
    /// untrusted tail and resumes numbering.
    pub fn resume(&mut self, valid_len: u64, seq: u64) -> io::Result<()> {
        self.file.set_len(valid_len)?;
        self.file.seek(SeekFrom::End(0))?;
        self.file.sync_data()?;
        self.seq = seq;
        Ok(())
    }

    /// Appends one applied command in a single write.
    pub fn append(
        &mut self,
        command: &str,
        params: Value,
        result: &Value,
        by: &Author,
        merge: bool,
    ) -> io::Result<()> {
        let entry = Entry {
            seq: self.seq + 1,
            t: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            command: command.into(),
            params,
            result: result.clone(),
            by: (!by.is_writer()).then(|| by.clone()),
            merge,
        };
        self.write_entry(&entry)
    }

    /// Appends a `doc.saved` marker for a save of these bytes that is about
    /// to start.
    pub fn append_saved(&mut self, disk: Fingerprint) -> io::Result<()> {
        let entry = Entry {
            seq: self.seq + 1,
            t: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            command: SAVED.into(),
            params: serde_json::json!({ "disk": disk }),
            result: serde_json::json!({}),
            by: None,
            merge: false,
        };
        self.write_entry(&entry)
    }

    /// Entries written since the header.
    pub fn entries(&self) -> u64 {
        self.seq
    }

    /// The journal's bytes, read through the locked handle.
    pub fn contents(&mut self) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        self.file.seek(SeekFrom::Start(0))?;
        let r = self.file.read_to_end(&mut out);
        self.file.seek(SeekFrom::End(0))?;
        r.map(|_| out)
    }

    fn write_entry(&mut self, entry: &Entry) -> io::Result<()> {
        let mut line = serde_json::to_vec(entry).map_err(io::Error::other)?;
        line.push(b'\n');
        self.file.write_all(&line)?;
        if self.fsync {
            self.file.sync_data()?;
        }
        self.seq += 1;
        Ok(())
    }

    /// Keeps a copy of the journal's bytes next to it, never deleting
    /// anything a writer might want back.
    pub fn copy_aside(&self, contents: &[u8], tag: &str) -> io::Result<PathBuf> {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let mut name = self.path.as_os_str().to_owned();
        name.push(format!(".{tag}-{ms}"));
        let aside = PathBuf::from(name);
        fs::write(&aside, contents)?;
        Ok(aside)
    }

    /// Releases the lock and removes the journal file.
    pub fn delete(self) -> io::Result<()> {
        let path = self.path.clone();
        drop(self);
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// A journal read back for recovery.
pub struct ReadBack {
    pub header: Header,
    pub entries: Vec<Entry>,
    /// Bytes up to the end of the last trusted entry.
    pub valid_len: u64,
    /// Set when a complete line could not be trusted; replay stops there.
    pub problem: Option<String>,
    header_end: u64,
    entry_ends: Vec<u64>,
}

impl ReadBack {
    /// Bytes up to the end of the header.
    pub fn header_len(&self) -> u64 {
        self.header_end
    }

    /// Bytes up to the end of entry `k` (0-based).
    pub fn entry_end(&self, k: usize) -> u64 {
        self.entry_ends.get(k).copied().unwrap_or(self.valid_len)
    }
}

/// Parses journal bytes. Fails only when the header is unusable. A final
/// line without `\n` is a write torn by a crash and is dropped silently.
pub fn read(contents: &[u8]) -> Result<ReadBack, String> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (i, &b) in contents.iter().enumerate() {
        if b == b'\n' {
            lines.push((start, i + 1));
            start = i + 1;
        }
    }
    let Some(&(h0, h1)) = lines.first() else {
        return Err("the journal has no complete header".into());
    };
    let header: Header = serde_json::from_slice(&contents[h0..h1])
        .map_err(|e| format!("the journal header is unreadable: {e}"))?;
    if header.journal != FORMAT || header.v != VERSION {
        return Err(format!(
            "unsupported journal format {} v{}",
            header.journal, header.v
        ));
    }
    let mut entries = Vec::new();
    let mut entry_ends = Vec::new();
    let mut valid_len = h1 as u64;
    let mut problem = None;
    for &(s, e) in &lines[1..] {
        match serde_json::from_slice::<Entry>(&contents[s..e]) {
            Ok(entry) if entry.seq == entries.len() as u64 + 1 => {
                entries.push(entry);
                valid_len = e as u64;
                entry_ends.push(valid_len);
            }
            Ok(entry) => {
                problem = Some(format!(
                    "entry {} is out of order after {} entries",
                    entry.seq,
                    entries.len()
                ));
                break;
            }
            Err(err) => {
                problem = Some(format!("entry {} is unreadable: {err}", entries.len() + 1));
                break;
            }
        }
    }
    Ok(ReadBack {
        header,
        entries,
        valid_len,
        problem,
        header_end: h1 as u64,
        entry_ends,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample(dir: &Path) -> (PathBuf, Vec<u8>) {
        let p = dir.join(".x.fountain.coldopen-journal");
        let (mut j, _) = Journal::open_locked(&p, false).ok().unwrap();
        j.start(&Document::default(), None).unwrap();
        for k in 0..3 {
            j.append(
                "block.delete",
                json!({ "id": format!("b_{k}") }),
                &json!({}),
                &Author::Writer,
                false,
            )
            .unwrap();
        }
        drop(j);
        let bytes = fs::read(&p).unwrap();
        (p, bytes)
    }

    #[test]
    fn sidecar_name() {
        assert_eq!(
            path_for(Path::new("dir/My Script.fountain")),
            Path::new("dir/.My Script.fountain.coldopen-journal")
        );
    }

    #[test]
    fn every_truncation_recovers_a_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let (_, bytes) = sample(dir.path());
        let header_len = bytes.iter().position(|&b| b == b'\n').unwrap() + 1;
        let ends: Vec<usize> = bytes
            .iter()
            .enumerate()
            .filter(|&(_, &b)| b == b'\n')
            .map(|(i, _)| i + 1)
            .collect();
        for cut in 0..=bytes.len() {
            let r = read(&bytes[..cut]);
            if cut < header_len {
                assert!(r.is_err(), "cut {cut}");
                continue;
            }
            let r = r.unwrap();
            let complete = ends.iter().filter(|&&e| e <= cut).count() - 1;
            assert_eq!(r.entries.len(), complete, "cut {cut}");
            assert!(r.problem.is_none(), "cut {cut}");
            assert_eq!(r.valid_len as usize, ends[complete], "cut {cut}");
        }
    }

    #[test]
    fn a_corrupt_middle_line_stops_replay() {
        let dir = tempfile::tempdir().unwrap();
        let (_, bytes) = sample(dir.path());
        let mut text = String::from_utf8(bytes).unwrap();
        text = text.replacen("\"seq\":2", "\"seq\":", 1);
        let r = read(text.as_bytes()).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert!(r.problem.is_some());
    }

    #[test]
    fn authors_merges_and_markers_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(".m.fountain.coldopen-journal");
        let (mut j, _) = Journal::open_locked(&p, false).ok().unwrap();
        j.start(&Document::default(), None).unwrap();
        let agent = Author::agent("claude-code", Some("text_insert".into()));
        j.append("text.insert", json!({}), &json!({}), &agent, false)
            .unwrap();
        j.append("text.insert", json!({}), &json!({}), &Author::Writer, true)
            .unwrap();
        j.append_saved(Fingerprint::of(b"x")).unwrap();
        assert_eq!(j.entries(), 3);
        let bytes = j.contents().unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        // A writer's entry carries neither field.
        assert!(
            text.contains(r#""by":{"kind":"agent","name":"claude-code","tool":"text_insert"}"#)
        );
        assert_eq!(text.matches(r#""merge":true"#).count(), 1);
        let r = read(&bytes).unwrap();
        assert_eq!(r.entries[0].by, Some(agent));
        assert!(r.entries[1].merge);
        assert_eq!(r.entries[2].saved_disk(), Some(Fingerprint::of(b"x")));
        assert_eq!(r.entries[0].saved_disk(), None);
    }

    #[test]
    fn a_second_opener_is_locked_out() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(".y.coldopen-journal");
        let (j, _) = Journal::open_locked(&p, false).ok().unwrap();
        assert!(matches!(
            Journal::open_locked(&p, false),
            Err(OpenError::Locked)
        ));
        drop(j);
        assert!(Journal::open_locked(&p, false).is_ok());
    }
}
