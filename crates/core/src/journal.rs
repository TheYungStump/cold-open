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
use crate::flush::{Durability, FlushStatus, Flusher};
use crate::format::FormatId;
use crate::io::{Fingerprint, open_rw};
use crate::ops::Transaction;

pub const FORMAT: &str = "coldopen";
pub const VERSION: u32 = 1;
/// What replaying entries means. Bumped whenever a command's replay
/// changes meaning; a journal with a higher number is kept aside, never
/// replayed (docs/specs/journal.md, "Header").
pub const SEMANTICS: u32 = 1;
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

/// `.script.fdx.coldopen-original-<ms>` next to `script.fdx`: a repaired
/// file kept as it was before the first save over it
/// (docs/specs/journal.md, "Damaged originals").
pub fn original_path(doc: &Path) -> PathBuf {
    let name = doc
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    doc.with_file_name(format!(".{name}.coldopen-original-{ms}"))
}

#[derive(Serialize, Deserialize)]
pub struct Header {
    pub journal: String,
    pub v: u32,
    pub app: String,
    pub doc: Document,
    pub disk: Option<Fingerprint>,
    /// Absent in journals from before 2026-10-16, which mean 1.
    #[serde(default = "first_semantics")]
    pub semantics: u32,
    /// The computer and the script's canonical path the journal was
    /// written for; absent in older journals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The file format `doc` was read from or saved as, so recovery
    /// compares like with like. Absent means Fountain, which is never
    /// written, so Fountain journals are as before (2026-10-16).
    #[serde(default, skip_serializing_if = "FormatId::is_fountain")]
    pub format: FormatId,
    /// The header's document is itself unsaved work, as for a restored
    /// copy, so the app offers it at start-up though it has no entries
    /// (docs/specs/journal.md, "Recovered versions"). Absent means false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unsaved: bool,
}

fn first_semantics() -> u32 {
    1
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
    /// `undo` and `redo` only (2026-10-16): the step they reverse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepRef>,
}

/// Which step an `undo` or `redo` reversed (docs/specs/journal.md, "Undo
/// and redo entries").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StepRef {
    /// The seq of the entry, in this journal, that made the step.
    Seq(u64),
    /// A step made before this journal's header: replay cannot find it in
    /// the history it rebuilds, so the entry carries it.
    Before {
        /// The operations the undo or redo applied.
        tx: Transaction,
        command: String,
        label: String,
        /// The step's author; absent means a writer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<Author>,
    },
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

/// Write failures to inject, for tests: no OS fails a write on demand.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct Faults {
    /// The next line written stops after this many bytes, then fails.
    pub short_write: Option<usize>,
    /// The next flush after a write fails.
    pub fail_sync: bool,
    /// The next cut back to an earlier length fails.
    pub fail_truncate: bool,
}

/// An open, locked journal.
#[derive(Debug)]
pub struct Journal {
    file: File,
    path: PathBuf,
    seq: u64,
    /// The file's length after the last whole line, where the next one
    /// goes.
    len: u64,
    /// The computer and script path written into each header.
    host: Option<String>,
    doc_path: Option<String>,
    /// The format written into each header.
    format: FormatId,
    /// Written into the next header that has no file behind it.
    unsaved: bool,
    /// Set when the file may end in a fragment that could not be cut off,
    /// or has no whole header: nothing written now could be replayed, so
    /// every write fails until [`Journal::start`] succeeds.
    broken: Option<String>,
    durability: Durability,
    /// Present in `Deferred` mode. Declared after `file` but dropped first
    /// by `delete`, so its final flush never races the file's removal.
    flusher: Option<Flusher>,
    #[cfg(test)]
    pub(crate) faults: Faults,
}

impl Journal {
    /// Opens (creating if needed) and locks the journal at `path`. Returns
    /// it with its current contents, read through the locked handle.
    pub fn open_locked(
        path: &Path,
        durability: Durability,
    ) -> Result<(Journal, Vec<u8>), OpenError> {
        let mut file = open_rw(path).map_err(OpenError::Io)?;
        crate::io::hide(path);
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(OpenError::Locked),
            Err(TryLockError::Error(e)) => return Err(OpenError::Io(e)),
        }
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).map_err(OpenError::Io)?;
        let flusher = match durability {
            Durability::Deferred { max_delay, idle } => {
                let handle = file.try_clone().map_err(OpenError::Io)?;
                Some(Flusher::spawn(handle, max_delay, idle).map_err(OpenError::Io)?)
            }
            _ => None,
        };
        Ok((
            Journal {
                file,
                path: path.to_path_buf(),
                seq: 0,
                len: contents.len() as u64,
                host: None,
                doc_path: None,
                format: FormatId::default(),
                unsaved: false,
                broken: None,
                durability,
                flusher,
                #[cfg(test)]
                faults: Faults::default(),
            },
            contents,
        ))
    }

    /// Cuts the file back to `len`, the end of its last whole line.
    fn cut_back(&mut self, len: u64) -> io::Result<()> {
        #[cfg(test)]
        if std::mem::take(&mut self.faults.fail_truncate) {
            return Err(io::Error::other("injected truncate failure"));
        }
        self.file.set_len(len)?;
        self.file.seek(SeekFrom::Start(len))?;
        Ok(())
    }

    /// Names the computer and the script's canonical path, for the
    /// headers written from now on.
    pub fn set_identity(&mut self, host: Option<String>, doc_path: Option<String>) {
        self.host = host;
        self.doc_path = doc_path;
    }

    /// Names the file format, for the headers written from now on.
    pub fn set_format(&mut self, format: FormatId) {
        self.format = format;
    }

    /// Marks the headers written from now on, until one is written for a
    /// saved file, as holding unsaved work.
    pub fn set_unsaved(&mut self, unsaved: bool) {
        self.unsaved = unsaved;
    }

    /// Whether the journal is broken (see `broken`).
    pub fn is_broken(&self) -> bool {
        self.broken.is_some()
    }

    /// Writes one line and makes it durable as the mode asks.
    fn write_line(&mut self, line: &[u8]) -> io::Result<()> {
        #[cfg(test)]
        if let Some(n) = self.faults.short_write.take() {
            self.file.write_all(&line[..n.min(line.len())])?;
            return Err(io::Error::other("injected short write"));
        }
        self.file.write_all(line)?;
        #[cfg(test)]
        if std::mem::take(&mut self.faults.fail_sync) {
            return Err(io::Error::other("injected flush failure"));
        }
        self.written()
    }

    /// Makes what was just written durable, as the mode asks.
    fn written(&mut self) -> io::Result<()> {
        match (&self.durability, &self.flusher) {
            (Durability::EveryEntry, _) => self.file.sync_data(),
            (_, Some(f)) => {
                f.wrote();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Asks a deferred journal to flush now, as on a save.
    pub fn flush_now(&self) {
        if let Some(f) = &self.flusher {
            f.flush_now();
        }
    }

    /// The deferred flusher's state, or `None` in the other modes.
    pub fn status(&self) -> Option<FlushStatus> {
        self.flusher.as_ref().map(Flusher::status)
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
            semantics: SEMANTICS,
            host: self.host.clone(),
            path: self.doc_path.clone(),
            format: self.format,
            unsaved: self.unsaved && disk.is_none(),
        };
        if disk.is_some() {
            self.unsaved = false;
        }
        let mut line = serde_json::to_vec(&header).map_err(io::Error::other)?;
        line.push(b'\n');
        // If this fails, the old journal is still whole.
        self.file.set_len(0)?;
        self.seq = 0;
        self.len = 0;
        let written = self
            .file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.write_line(&line));
        if let Err(e) = written {
            self.broken = Some(format!("its header could not be written: {e}"));
            return Err(e);
        }
        self.len = line.len() as u64;
        self.broken = None;
        self.flush_now();
        Ok(())
    }

    /// Continues an existing journal after recovery: cuts any torn or
    /// untrusted tail and resumes numbering.
    pub fn resume(&mut self, valid_len: u64, seq: u64) -> io::Result<()> {
        self.file.set_len(valid_len)?;
        self.file.seek(SeekFrom::End(0))?;
        self.seq = seq;
        self.len = valid_len;
        self.written()?;
        self.flush_now();
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
        step: Option<StepRef>,
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
            step,
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
            step: None,
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
        if let Some(why) = &self.broken {
            return Err(io::Error::other(format!(
                "the journal is damaged ({why}); save the script to start it again"
            )));
        }
        // A journal whose flushes fail is not protecting anything; refuse
        // edits until a flush works again (docs/specs/editor.md §13).
        if let Some(e) = self.flusher.as_ref().and_then(Flusher::error) {
            return Err(io::Error::other(format!(
                "the journal could not be flushed to disk: {e}"
            )));
        }
        let mut line = serde_json::to_vec(entry).map_err(io::Error::other)?;
        line.push(b'\n');
        let before = self.len;
        if let Err(e) = self.write_line(&line) {
            // The caller rolls the edit back, so its entry goes too, and
            // the next entry follows a whole line (docs/specs/journal.md,
            // "Writing").
            if let Err(cut) = self.cut_back(before) {
                self.broken = Some(format!("a failed write could not be undone: {cut}"));
            }
            return Err(e);
        }
        self.len += line.len() as u64;
        self.seq += 1;
        Ok(())
    }

    /// Keeps a copy of the journal's bytes next to it, never deleting
    /// anything a writer might want back.
    pub fn copy_aside(&self, contents: &[u8], tag: &str) -> io::Result<PathBuf> {
        let mut ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        // Never over another copy: a copy made in the same millisecond
        // takes the next free one, so its name still says when it was made.
        loop {
            let mut name = self.path.as_os_str().to_owned();
            name.push(format!(".{tag}-{ms}"));
            let aside = PathBuf::from(name);
            match crate::io::write_new(&aside, contents) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => ms += 1,
                other => return other.map(|()| aside),
            }
        }
    }

    /// Releases the lock and removes the journal file.
    pub fn delete(mut self) -> io::Result<()> {
        let path = self.path.clone();
        if let Some(mut f) = self.flusher.take() {
            f.stop();
        }
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
    if header.semantics > SEMANTICS {
        return Err(format!(
            "the journal was written by a newer Cold Open (semantics {}; this version knows up to {SEMANTICS})",
            header.semantics
        ));
    }
    // A damaged or hand-made header must never put two blocks under one id
    // or run out of ids mid-replay (review finding C7).
    header
        .doc
        .check()
        .map_err(|e| format!("the journal header is damaged: {e}"))?;
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
        let (mut j, _) = Journal::open_locked(&p, Durability::OsOnly).ok().unwrap();
        j.start(&Document::default(), None).unwrap();
        for k in 0..3 {
            j.append(
                "block.delete",
                json!({ "id": format!("b_{k}") }),
                &json!({}),
                &Author::Writer,
                false,
                None,
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

    /// The header records its format (review finding F18.1), but a Fountain
    /// journal reads and writes exactly as before: absent means Fountain.
    /// A format this version does not know makes the header unreadable, so
    /// the journal is kept aside, never replayed as the wrong format.
    #[test]
    fn the_header_names_its_format_only_when_not_fountain() {
        let dir = tempfile::tempdir().unwrap();
        let (_, bytes) = sample(dir.path());
        let header = String::from_utf8(bytes).unwrap();
        let header = header.lines().next().unwrap().to_string();
        assert!(!header.contains("\"format\""), "{header}");
        assert_eq!(
            read(format!("{header}\n").as_bytes())
                .unwrap()
                .header
                .format,
            FormatId::Fountain
        );

        let named = header.replacen("{", "{\"format\":\"fountain\",", 1);
        assert_eq!(
            read(format!("{named}\n").as_bytes()).unwrap().header.format,
            FormatId::Fountain
        );
        let unknown = header.replacen("{", "{\"format\":\"wordstar\",", 1);
        let e = read(format!("{unknown}\n").as_bytes()).err().unwrap();
        assert!(e.contains("unreadable"), "{e}");
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
        let (mut j, _) = Journal::open_locked(&p, Durability::OsOnly).ok().unwrap();
        j.start(&Document::default(), None).unwrap();
        let agent = Author::agent("claude-code", Some("text_insert".into()));
        j.append("text.insert", json!({}), &json!({}), &agent, false, None)
            .unwrap();
        j.append(
            "text.insert",
            json!({}),
            &json!({}),
            &Author::Writer,
            true,
            None,
        )
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

    fn delete(j: &mut Journal, k: usize) -> io::Result<()> {
        j.append(
            "block.delete",
            json!({ "id": format!("b_{k}") }),
            &json!({}),
            &Author::Writer,
            false,
            None,
        )
    }

    /// Review finding F10: a write that fails part-way (a full disk) left
    /// its fragment in the journal. The next entry was appended to it, so
    /// that entry and every later one could never be replayed.
    #[test]
    fn a_failed_write_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(".f.fountain.coldopen-journal");
        let (mut j, _) = Journal::open_locked(&p, Durability::OsOnly).ok().unwrap();
        j.start(&Document::default(), None).unwrap();
        delete(&mut j, 1).unwrap();
        j.faults.short_write = Some(30);
        assert!(delete(&mut j, 2).is_err());
        delete(&mut j, 3).unwrap();
        delete(&mut j, 4).unwrap();
        let r = read(&j.contents().unwrap()).unwrap();
        assert_eq!(r.problem, None);
        let ids: Vec<&Value> = r.entries.iter().map(|e| &e.params["id"]).collect();
        assert_eq!(ids, [&json!("b_1"), &json!("b_3"), &json!("b_4")]);

        // A flush that fails after a whole line was written: the edit is
        // rolled back, so its entry must go too.
        j.faults.fail_sync = true;
        assert!(delete(&mut j, 5).is_err());
        delete(&mut j, 6).unwrap();
        let r = read(&j.contents().unwrap()).unwrap();
        assert_eq!(r.problem, None);
        let ids: Vec<&Value> = r.entries.iter().map(|e| &e.params["id"]).collect();
        assert_eq!(
            ids,
            [&json!("b_1"), &json!("b_3"), &json!("b_4"), &json!("b_6")]
        );
        assert_eq!(j.entries(), 4);
    }

    /// If a failed write cannot be cut back either, the journal can no
    /// longer protect anything: every later write fails until a save
    /// starts it again.
    #[test]
    fn a_journal_that_cannot_be_repaired_refuses_writes_until_restarted() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(".g.fountain.coldopen-journal");
        let (mut j, _) = Journal::open_locked(&p, Durability::OsOnly).ok().unwrap();
        j.start(&Document::default(), None).unwrap();
        j.faults.short_write = Some(10);
        j.faults.fail_truncate = true;
        assert!(delete(&mut j, 1).is_err());
        assert!(j.is_broken());
        assert!(delete(&mut j, 2).is_err());
        j.start(&Document::default(), None).unwrap();
        assert!(!j.is_broken());
        delete(&mut j, 3).unwrap();
        let r = read(&j.contents().unwrap()).unwrap();
        assert_eq!(r.problem, None);
        assert_eq!(r.entries.len(), 1);

        // A header that fails to write leaves the journal broken too.
        j.faults.short_write = Some(10);
        assert!(j.start(&Document::default(), None).is_err());
        assert!(j.is_broken());
        assert!(delete(&mut j, 4).is_err());
        j.start(&Document::default(), None).unwrap();
        delete(&mut j, 5).unwrap();
        assert_eq!(read(&j.contents().unwrap()).unwrap().entries.len(), 1);
    }

    #[test]
    fn a_second_opener_is_locked_out() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(".y.coldopen-journal");
        let (j, _) = Journal::open_locked(&p, Durability::OsOnly).ok().unwrap();
        assert!(matches!(
            Journal::open_locked(&p, Durability::OsOnly),
            Err(OpenError::Locked)
        ));
        drop(j);
        assert!(Journal::open_locked(&p, Durability::OsOnly).is_ok());
    }
}
