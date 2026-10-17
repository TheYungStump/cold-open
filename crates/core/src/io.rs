//! File input and output: atomic saves, text decoding, fingerprints
//! (docs/specs/journal.md, "Atomic save" and "Encoding").

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// How the source file stored its text, so a save can keep it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceStyle {
    pub bom: bool,
    pub crlf: bool,
    pub encoding: Encoding,
}

/// The encoding a script was read in (docs/specs/journal.md, "Encoding").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Not UTF-8, so read as Windows-1252; saved as UTF-8, which a save
    /// reports as a loss.
    Windows1252,
}

pub struct Decoded {
    pub text: String,
    pub style: SourceStyle,
    /// Set when the bytes were not UTF-8 and were read as Windows-1252.
    pub not_utf8: bool,
    /// Set when UTF-16 text held sequences that are not UTF-16; they were
    /// replaced with U+FFFD.
    pub replaced: bool,
}

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Windows-1252 for bytes 0x80..=0x9F; the rest match Latin-1.
const CP1252: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

fn windows_1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252[usize::from(b - 0x80)],
            _ => char::from(b),
        })
        .collect()
}

/// UTF-16 text, and whether anything had to be replaced with U+FFFD.
fn utf16(bytes: &[u8], big_endian: bool) -> (String, bool) {
    let units = bytes.as_chunks::<2>().0.iter().map(|&pair| {
        if big_endian {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    });
    let mut replaced = bytes.len() % 2 == 1;
    let mut text: String = char::decode_utf16(units)
        .map(|c| {
            c.unwrap_or_else(|_| {
                replaced = true;
                char::REPLACEMENT_CHARACTER
            })
        })
        .collect();
    if bytes.len() % 2 == 1 {
        text.push(char::REPLACEMENT_CHARACTER);
    }
    (text, replaced)
}

/// Decodes file bytes. UTF-16 is recognized by its byte-order mark, or,
/// without one, by a zero byte next to the first character, which UTF-8
/// text never has (review finding F6). Otherwise a UTF-8 BOM is stripped,
/// and bytes that are not UTF-8 are read as Windows-1252 rather than
/// replaced.
pub fn decode(bytes: &[u8]) -> Decoded {
    let (text, bom, encoding, replaced) = if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (t, r) = utf16(rest, false);
        (t, true, Encoding::Utf16Le, r)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let (t, r) = utf16(rest, true);
        (t, true, Encoding::Utf16Be, r)
    } else if let [a, 0, ..] = bytes
        && *a != 0
    {
        let (t, r) = utf16(bytes, false);
        (t, false, Encoding::Utf16Le, r)
    } else if let [0, b, ..] = bytes
        && *b != 0
    {
        let (t, r) = utf16(bytes, true);
        (t, false, Encoding::Utf16Be, r)
    } else {
        let bom = bytes.starts_with(BOM);
        let body = if bom { &bytes[BOM.len()..] } else { bytes };
        match std::str::from_utf8(body) {
            Ok(s) => (s.to_string(), bom, Encoding::Utf8, false),
            Err(_) => (windows_1252(body), bom, Encoding::Windows1252, false),
        }
    };
    let lf = text.matches('\n').count();
    let crlf = text.matches("\r\n").count();
    Decoded {
        not_utf8: encoding == Encoding::Windows1252,
        style: SourceStyle {
            bom,
            crlf: lf > 0 && crlf * 2 > lf,
            encoding,
        },
        text,
        replaced,
    }
}

/// Encodes LF text for saving in the source file's style: UTF-16 stays
/// UTF-16 in the same byte order; everything else is written as UTF-8.
pub fn encode(text: &str, style: SourceStyle) -> Vec<u8> {
    let body = if style.crlf {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    match style.encoding {
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let be = style.encoding == Encoding::Utf16Be;
            let mut out = Vec::with_capacity(body.len() * 2 + 2);
            if style.bom {
                out.extend_from_slice(if be { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] });
            }
            for unit in body.encode_utf16() {
                out.extend_from_slice(&if be {
                    unit.to_be_bytes()
                } else {
                    unit.to_le_bytes()
                });
            }
            out
        }
        Encoding::Utf8 | Encoding::Windows1252 => {
            let mut out = Vec::with_capacity(body.len() + 3);
            if style.bom {
                out.extend_from_slice(BOM);
            }
            out.extend_from_slice(body.as_bytes());
            out
        }
    }
}

/// Length and FNV-1a 64-bit hash of a file's bytes. Used only to notice
/// changes made by other programs, not for security.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub len: u64,
    #[serde(with = "hex64")]
    pub fnv64: u64,
}

impl Fingerprint {
    pub fn of(bytes: &[u8]) -> Fingerprint {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Fingerprint {
            len: bytes.len() as u64,
            fnv64: h,
        }
    }
}

mod hex64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&format_args!("{v:016x}"))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let s = String::deserialize(d)?;
        u64::from_str_radix(&s, 16).map_err(serde::de::Error::custom)
    }
}

/// Writes a new file, failing if one is already there, and flushes it to
/// the disk: for copies kept aside, which must never replace anything.
pub fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    hide(path);
    Ok(())
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(target: &Path) -> PathBuf {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(
        ".{name}.coldopen-save-{}-{n}.tmp",
        std::process::id()
    ))
}

/// Writes `bytes` to `target` atomically: a crash leaves either the old
/// file or the new one, never a mix. See docs/specs/journal.md.
pub fn atomic_write(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = match fs::symlink_metadata(target) {
        Ok(m) if m.file_type().is_symlink() => fs::canonicalize(target)?,
        _ => target.to_path_buf(),
    };
    let tmp = temp_path(&target);
    let result = write_then_replace(&tmp, &target, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_then_replace(tmp: &Path, target: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    #[cfg(unix)]
    if let Ok(meta) = fs::metadata(target) {
        f.set_permissions(meta.permissions())?;
    }
    drop(f);
    replace(tmp, target)
}

#[cfg(unix)]
fn replace(tmp: &Path, target: &Path) -> io::Result<()> {
    fs::rename(tmp, target)?;
    // Syncing the folder makes the rename itself durable. It is best effort:
    // the new file is already complete and in place, and some systems
    // (macOS with F_FULLFSYNC) refuse to sync a directory handle.
    if let Some(dir) = target.parent() {
        let dir = if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        };
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(())
}

#[cfg(windows)]
fn replace(tmp: &Path, target: &Path) -> io::Result<()> {
    // Antivirus, indexers and sync clients briefly hold files open; retry
    // those failures for about a second.
    const ACCESS_DENIED: i32 = 5;
    const SHARING_VIOLATION: i32 = 32;
    const LOCK_VIOLATION: i32 = 33;
    let mut delay = std::time::Duration::from_millis(10);
    let mut waited = std::time::Duration::ZERO;
    loop {
        match win::move_replace(tmp, target) {
            Ok(()) => return Ok(()),
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(ACCESS_DENIED | SHARING_VIOLATION | LOCK_VIOLATION)
                ) && waited < std::time::Duration::from_secs(1) =>
            {
                std::thread::sleep(delay);
                waited += delay;
                delay = (delay * 2).min(std::time::Duration::from_millis(200));
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod win {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        fn GetFileAttributesW(name: *const u16) -> u32;
        fn SetFileAttributesW(name: *const u16, attributes: u32) -> i32;
    }

    /// Adds the hidden attribute to a file, keeping its others.
    pub fn hide(p: &Path) -> io::Result<()> {
        let name = wide(p);
        // SAFETY: `name` is a NUL-terminated UTF-16 string that lives until
        // each call returns; neither function keeps the pointer.
        let attrs = unsafe { GetFileAttributesW(name.as_ptr()) };
        if attrs == INVALID_FILE_ATTRIBUTES {
            return Err(io::Error::last_os_error());
        }
        if attrs & FILE_ATTRIBUTE_HIDDEN != 0 {
            return Ok(());
        }
        // SAFETY: as above.
        let ok = unsafe { SetFileAttributesW(name.as_ptr(), attrs | FILE_ATTRIBUTE_HIDDEN) };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    /// Renames `from` over `to`, replacing it, and returns only once the
    /// rename is on disk.
    pub fn move_replace(from: &Path, to: &Path) -> io::Result<()> {
        let from = wide(from);
        let to = wide(to);
        // SAFETY: both buffers are NUL-terminated UTF-16 strings that live
        // until the call returns; MoveFileExW does not keep the pointers.
        let ok = unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// Hides a file Cold Open keeps for itself (a journal, a copy kept aside)
/// on Windows, which ignores the leading dot that hides it elsewhere
/// (docs/specs/journal.md, "Hidden files"). Best effort: a file that
/// cannot be hidden still works.
pub fn hide(path: &Path) {
    #[cfg(windows)]
    {
        let _ = win::hide(path);
    }
    #[cfg(not(windows))]
    {
        let _ = path;
    }
}

/// Opens a file for reading and writing, creating it if needed.
pub fn open_rw(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_handles_bom_crlf_and_cp1252() {
        let d = decode(b"\xEF\xBB\xBFa\r\nb\r\n");
        assert_eq!(d.text, "a\r\nb\r\n");
        assert_eq!(
            d.style,
            SourceStyle {
                bom: true,
                crlf: true,
                encoding: Encoding::Utf8,
            }
        );
        assert!(!d.not_utf8);

        let d = decode(b"caf\xe9 \x93hi\x94");
        assert_eq!(d.text, "caf\u{e9} \u{201C}hi\u{201D}");
        assert!(d.not_utf8);

        let d = decode(b"a\nb\r\nc\n");
        assert!(!d.style.crlf);
    }

    #[test]
    fn encode_restores_style() {
        let s = SourceStyle {
            bom: true,
            crlf: true,
            encoding: Encoding::Utf8,
        };
        assert_eq!(encode("a\nb\n", s), b"\xEF\xBB\xBFa\r\nb\r\n");
    }

    #[test]
    fn fingerprint_is_stable() {
        let f = Fingerprint::of(b"hello");
        assert_eq!(f.len, 5);
        assert_eq!(format!("{:016x}", f.fnv64), "a430d84680aabd0b");
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#"{"len":5,"fnv64":"a430d84680aabd0b"}"#);
        assert_eq!(serde_json::from_str::<Fingerprint>(&json).unwrap(), f);
    }

    #[test]
    fn atomic_write_replaces_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.fountain");
        atomic_write(&p, b"one").unwrap();
        atomic_write(&p, b"two").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["s.fountain"]);
    }

    #[test]
    fn atomic_write_failure_leaves_original() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.fountain");
        atomic_write(&p, b"original").unwrap();
        // A directory where the file should be makes the rename fail.
        let blocked = dir.path().join("blocked");
        fs::create_dir(&blocked).unwrap();
        assert!(atomic_write(&blocked, b"new").is_err());
        assert_eq!(fs::read(&p).unwrap(), b"original");
        let leftovers = fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.fountain");
        fs::write(&p, b"x").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o640)).unwrap();
        atomic_write(&p, b"y").unwrap();
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
