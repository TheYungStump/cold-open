//! The panic hook and the crash reports it keeps
//! (docs/specs/crash-reports.md). On any panic, while reports are on, a
//! report with nothing personal in it is written to
//! `<data>/crash-reports/`, to be offered at the next start (§6).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use coldopen_core::build_info;
use coldopen_core::crash::{self, Private, Report};

use crate::files::Dirs;

/// The newest reports kept (§6).
pub const KEEP: usize = 5;

static FOLDER: OnceLock<PathBuf> = OnceLock::new();
static ON: AtomicBool = AtomicBool::new(true);
/// The renderer as last known, for the report's `Renderer` line.
static RENDERER: Mutex<String> = Mutex::new(String::new());

/// Where the reports are kept.
pub fn folder(dirs: &Dirs) -> PathBuf {
    dirs.data.join("crash-reports")
}

/// Installs the hook, first thing in `main`. It writes a report, then
/// runs the default hook, which prints the panic as before. With no
/// folders found, it writes nothing.
pub fn install(dirs: Option<&Dirs>, on: bool) {
    if let Some(d) = dirs {
        let _ = FOLDER.set(folder(d));
    }
    set_on(on);
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if ON.load(Ordering::Relaxed)
            && let Some(f) = FOLDER.get()
        {
            write(f, &report(info));
        }
        default(info);
    }));
}

/// Reports on or off (§8).
pub fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// What draws the window, as About says it (§3).
pub fn set_renderer(label: String) {
    if let Ok(mut r) = RENDERER.lock() {
        *r = label;
    }
}

fn report(info: &std::panic::PanicHookInfo<'_>) -> Report {
    let private = Private::from_env();
    // `payload_as_str` is newer than our minimum Rust (1.89).
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("(no message)");
    let renderer = match RENDERER.try_lock() {
        Ok(r) if !r.is_empty() => r.clone(),
        _ => "unknown".into(),
    };
    Report {
        version: build_info::VERSION_LINE.to_string(),
        target: build_info::TARGET.to_string(),
        os: crate::bench::machine::os_version(),
        renderer,
        time: crate::bench::report::utc_now().iso(),
        thread: std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string(),
        last_action: match crash::last() {
            Ok(command) => command.to_string(),
            Err(code) => crate::actions::crash_name(code).to_string(),
        },
        location: info.location().map_or_else(
            || "unknown".into(),
            |l| {
                format!(
                    "{}:{}:{}",
                    crash::trim_location(l.file()),
                    l.line(),
                    l.column()
                )
            },
        ),
        message: crash::scrub_message(message, &private),
        frames: frames(),
    }
}

/// The panic's frames as module and offset (§5), newest first.
fn frames() -> Vec<String> {
    let mut ips = Vec::with_capacity(crash::FRAMES_MAX);
    backtrace::trace(|f| {
        ips.push(f.ip() as usize);
        ips.len() < crash::FRAMES_MAX
    });
    ips.into_iter()
        .map(|ip| match module::of(ip) {
            Some((name, base)) => crash::frame(Some(&name), ip.wrapping_sub(base)),
            None => crash::frame(None, ip),
        })
        .collect()
}

/// The module that holds an address: its file's path and its base.
mod module {
    #[cfg(unix)]
    #[allow(unsafe_code)]
    pub fn of(ip: usize) -> Option<(String, usize)> {
        let mut info = std::mem::MaybeUninit::<libc::Dl_info>::zeroed();
        // SAFETY: dladdr only reads the address's mapping and fills `info`;
        // it returns 0, and fills nothing, when no module holds the address.
        let found = unsafe { libc::dladdr(ip as *const libc::c_void, info.as_mut_ptr()) };
        if found == 0 {
            return None;
        }
        // SAFETY: dladdr returned non-zero, so it filled `info`.
        let info = unsafe { info.assume_init() };
        if info.dli_fname.is_null() {
            return None;
        }
        // SAFETY: a non-null `dli_fname` is a NUL-terminated string that the
        // loader keeps for as long as the module is loaded.
        let name = unsafe { std::ffi::CStr::from_ptr(info.dli_fname) };
        Some((name.to_string_lossy().into_owned(), info.dli_fbase as usize))
    }

    #[cfg(windows)]
    #[allow(unsafe_code)]
    pub fn of(ip: usize) -> Option<(String, usize)> {
        use windows_sys::Win32::System::LibraryLoader::{
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            GetModuleFileNameW, GetModuleHandleExW,
        };
        let mut module = std::ptr::null_mut();
        // SAFETY: with FROM_ADDRESS the second argument is an address, not
        // a name; UNCHANGED_REFCOUNT takes no reference that would need
        // freeing. It writes the handle to `module` and returns 0 on failure.
        let found = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                ip as *const u16,
                &mut module,
            )
        };
        if found == 0 || module.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        // SAFETY: `buf` is valid for its whole length, which is passed; the
        // call writes at most that many units and returns how many it wrote.
        let n = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) };
        let name = String::from_utf16_lossy(&buf[..(n as usize).min(buf.len())]);
        // A module's handle is its base address.
        Some((name, module as usize))
    }

    #[cfg(not(any(unix, windows)))]
    pub fn of(_ip: usize) -> Option<(String, usize)> {
        None
    }
}

/// Writes a report and keeps the newest [`KEEP`]. Errors are dropped: the
/// hook must never fail, and the default hook still prints the panic.
fn write(folder: &Path, r: &Report) {
    if std::fs::create_dir_all(folder).is_err() {
        return;
    }
    let stamp = crate::bench::report::utc_now().compact();
    let mut stem = format!("crash-{stamp}Z");
    let mut n = 1;
    while folder.join(format!("{stem}.txt")).exists()
        || folder.join(format!("{stem}.seen.txt")).exists()
    {
        n += 1;
        stem = format!("crash-{stamp}Z-{n}");
    }
    let _ =
        coldopen_core::io::atomic_write(&folder.join(format!("{stem}.txt")), r.render().as_bytes());
    for old in kept(folder).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
}

/// When a report was written, from its name: its stamp and its number
/// within the second, so they sort oldest first.
fn sort_key(path: &Path) -> Option<(String, u32)> {
    let name = path.file_name()?.to_str()?;
    let stem = name
        .strip_suffix(".seen.txt")
        .or_else(|| name.strip_suffix(".txt"))?
        .strip_prefix("crash-")?;
    let (stamp, n) = stem.split_once("Z")?;
    let n = match n.strip_prefix('-') {
        Some(n) => n.parse().ok()?,
        None if n.is_empty() => 1,
        None => return None,
    };
    Some((stamp.to_string(), n))
}

/// The reports kept, newest first.
pub fn kept(folder: &Path) -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut v: Vec<(String, u32, PathBuf)> = dir
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter_map(|p| sort_key(&p).map(|(s, n)| (s, n, p)))
        .collect();
    v.sort();
    v.into_iter().rev().map(|(_, _, p)| p).collect()
}

/// Whether the writer has been offered this report (§6).
pub fn seen(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".seen.txt")
}

/// Marks a report offered: `….txt` becomes `….seen.txt`.
pub fn mark_seen(path: &Path) -> PathBuf {
    if seen(path) {
        return path.to_path_buf();
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let to = path.with_file_name(name.replace(".txt", ".seen.txt"));
    match std::fs::rename(path, &to) {
        Ok(()) => to,
        Err(_) => path.to_path_buf(),
    }
}

/// The function a resolved backtrace names (§9). Never inlined, so its
/// name is in the backtrace.
#[inline(never)]
#[cold]
#[allow(clippy::panic)]
pub fn crash_on_purpose(text: Option<&str>) -> ! {
    match (std::env::var("COLDOPEN_TEST_CRASH").ok().as_deref(), text) {
        (Some("slice"), Some(t)) if cfg!(debug_assertions) => {
            // A byte inside the first character of two bytes or more, so
            // Rust's own message is about the writer's text.
            let cut = t
                .char_indices()
                .find(|(_, c)| c.len_utf8() > 1)
                .map_or(t.len() + 1, |(i, _)| i + 1);
            let piece = &t[..cut];
            panic!("unreachable: {} bytes", piece.len())
        }
        (Some("debug"), Some(t)) if cfg!(debug_assertions) => {
            panic!("test crash in {t:?}")
        }
        _ => panic!("Crash on purpose, from the palette"),
    }
}

/// A panic on a worker thread the app survives (§9, `worker`).
#[allow(clippy::panic)]
pub fn crash_a_worker() {
    let _ = std::thread::Builder::new()
        .name("coldopen-test-worker".into())
        .spawn(|| panic!("Crash on purpose, on a worker thread"))
        .map(|h| h.join());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_sort_by_their_stamp_and_number() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "crash-20261103-120000Z.seen.txt",
            "crash-20261103-120000Z-2.txt",
            "crash-20261102-235959Z.txt",
            "crash-20261104-000000Z.txt",
            "notes.txt",
        ] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        let names: Vec<String> = kept(dir.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "crash-20261104-000000Z.txt",
                "crash-20261103-120000Z-2.txt",
                "crash-20261103-120000Z.seen.txt",
                "crash-20261102-235959Z.txt",
            ]
        );
        let p = dir.path().join("crash-20261104-000000Z.txt");
        let to = mark_seen(&p);
        assert!(seen(&to) && to.exists() && !p.exists());
        assert_eq!(mark_seen(&to), to);
    }

    #[test]
    fn writing_keeps_the_newest_five() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..7 {
            std::fs::write(
                dir.path().join(format!("crash-2026110{i}-000000Z.txt")),
                "x",
            )
            .unwrap();
        }
        write(dir.path(), &Report::default());
        let k = kept(dir.path());
        assert_eq!(k.len(), KEEP);
        assert!(k.iter().all(|p| !p.ends_with("crash-20261100-000000Z.txt")));
    }

    #[test]
    fn frames_name_modules_by_file_alone() {
        let f = frames();
        assert!(!f.is_empty());
        let named = f.iter().filter(|l| !l.starts_with("?+")).count();
        assert!(named > 0, "{f:?}");
        for l in &f {
            assert!(!l.contains('/') && !l.contains('\\'), "{l}");
        }
    }
}
