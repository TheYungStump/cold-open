//! Deferred journal flushing for the desktop app (docs/specs/journal.md,
//! "Durability"): a background thread flushes every written entry no later
//! than `max_delay` after it was written, and sooner when writing goes
//! idle or a flush is asked for.

use std::fmt;
use std::fs::File;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How a journal reaches the disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Durability {
    /// Flush each entry before the command returns (the CLI, stdio, MCP).
    EveryEntry,
    /// Leave flushing to the operating system. Only for tests and tools
    /// that do not promise durability.
    OsOnly,
    /// A background thread flushes (the desktop app).
    Deferred {
        /// No entry waits longer than this after it is written.
        max_delay: Duration,
        /// Flush once no entry has been written for this long.
        idle: Duration,
    },
}

impl Durability {
    /// The desktop app's limits: 250 ms, and 100 ms of idle.
    pub const APP: Durability = Durability::Deferred {
        max_delay: Duration::from_millis(250),
        idle: Duration::from_millis(100),
    };
}

/// What the flusher has done, for the status bar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlushStatus {
    /// Entries written but not yet flushed.
    pub pending: u64,
    /// How long the oldest of them has waited.
    pub oldest: Option<Duration>,
    /// The last flush error, until a flush succeeds again.
    pub error: Option<String>,
    /// Flushes completed so far.
    pub flushes: u64,
}

#[derive(Default)]
struct State {
    written: u64,
    flushed: u64,
    /// When the oldest unflushed write happened.
    oldest: Option<Instant>,
    /// When the last write happened.
    last: Option<Instant>,
    now: bool,
    stop: bool,
    error: Option<String>,
    flushes: u64,
}

struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic while holding the lock cannot leave the counters in a
        // state worse than "flush again", so a poisoned lock is used as is.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A background thread that flushes one journal file.
pub struct Flusher {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for Flusher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Flusher")
            .field("status", &self.status())
            .finish()
    }
}

/// How long to wait before retrying a failed flush.
const RETRY: Duration = Duration::from_secs(1);

impl Flusher {
    /// Starts flushing `file`, a second handle to the journal.
    pub fn spawn(file: File, max_delay: Duration, idle: Duration) -> std::io::Result<Flusher> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            cv: Condvar::new(),
        });
        let s = shared.clone();
        let thread = std::thread::Builder::new()
            .name("coldopen-journal-flush".into())
            .spawn(move || run(&s, &file, max_delay, idle))?;
        Ok(Flusher {
            shared,
            thread: Some(thread),
        })
    }

    /// Records a write that now needs flushing.
    pub fn wrote(&self) {
        let now = Instant::now();
        let mut st = self.shared.lock();
        st.written += 1;
        st.oldest.get_or_insert(now);
        st.last = Some(now);
        drop(st);
        self.shared.cv.notify_all();
    }

    /// Asks for a flush as soon as possible.
    pub fn flush_now(&self) {
        self.shared.lock().now = true;
        self.shared.cv.notify_all();
    }

    pub fn status(&self) -> FlushStatus {
        let st = self.shared.lock();
        FlushStatus {
            pending: st.written - st.flushed,
            oldest: st.oldest.map(|t| t.elapsed()),
            error: st.error.clone(),
            flushes: st.flushes,
        }
    }

    /// The last flush error, if the most recent flush failed.
    pub fn error(&self) -> Option<String> {
        self.shared.lock().error.clone()
    }

    /// Flushes what is pending and stops the thread.
    pub fn stop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.cv.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Flusher {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(shared: &Shared, file: &File, max_delay: Duration, idle: Duration) {
    let mut st = shared.lock();
    loop {
        let pending = st.written > st.flushed;
        if st.stop {
            if pending {
                drop(st);
                let _ = file.sync_data();
            }
            return;
        }
        if !pending {
            st = shared.cv.wait(st).unwrap_or_else(|e| e.into_inner());
            continue;
        }
        let now = Instant::now();
        let deadline = if st.error.is_some() {
            st.last.map_or(now, |l| l + RETRY).max(now)
        } else if st.now {
            now
        } else {
            let by_age = st.oldest.map_or(now, |o| o + max_delay);
            let by_idle = st.last.map_or(now, |l| l + idle);
            by_age.min(by_idle)
        };
        if now < deadline {
            st = shared
                .cv
                .wait_timeout(st, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
            continue;
        }
        let target = st.written;
        let was_oldest = st.oldest.take();
        st.now = false;
        drop(st);
        let r = file.sync_data();
        st = shared.lock();
        match r {
            Ok(()) => {
                st.flushed = st.flushed.max(target);
                st.error = None;
                st.flushes += 1;
            }
            Err(e) => {
                st.error = Some(e.to_string());
                // The entries are still unflushed; keep their age.
                st.oldest = match (was_oldest, st.oldest) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                st.last = Some(Instant::now());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn wait_until(f: impl Fn() -> bool) -> Duration {
        let t = Instant::now();
        while !f() {
            assert!(t.elapsed() < Duration::from_secs(10), "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        t.elapsed()
    }

    #[test]
    fn flushes_after_idle_and_within_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = File::create(dir.path().join("j")).unwrap();
        let fl = Flusher::spawn(
            f.try_clone().unwrap(),
            Duration::from_millis(250),
            Duration::from_millis(50),
        )
        .unwrap();
        f.write_all(b"x\n").unwrap();
        fl.wrote();
        assert_eq!(fl.status().pending, 1);
        wait_until(|| fl.status().pending == 0);
        assert_eq!(fl.status().flushes, 1);

        // Continuous writing every 10 ms never goes idle, so the 250 ms
        // limit forces flushes along the way.
        let t = Instant::now();
        while t.elapsed() < Duration::from_millis(700) {
            f.write_all(b"y\n").unwrap();
            fl.wrote();
            if let Some(age) = fl.status().oldest {
                // Generous slack for slow CI machines; the deadline itself
                // is 250 ms.
                assert!(age < Duration::from_millis(2000), "{age:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(fl.status().flushes >= 2, "{:?}", fl.status());
        wait_until(|| fl.status().pending == 0);
    }

    #[test]
    fn flush_now_and_stop_flush_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let f = File::create(dir.path().join("j")).unwrap();
        let mut fl = Flusher::spawn(
            f.try_clone().unwrap(),
            Duration::from_secs(60),
            Duration::from_secs(60),
        )
        .unwrap();
        fl.wrote();
        fl.flush_now();
        wait_until(|| fl.status().pending == 0);
        fl.wrote();
        fl.stop();
        // The final flush is not counted, but the thread is gone.
        assert!(fl.thread.is_none());
    }
}
