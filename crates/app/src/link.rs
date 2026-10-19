//! The agent link server in the app (docs/specs/mcp.md §5): it publishes a
//! session file, accepts same-user connections, checks each handshake, and
//! passes requests into the app's update loop, where they run one at a time
//! between the writer's keystrokes.
//!
//! Threads here only move bytes. Every command runs in `App::update`.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use coldopen_link::{
    CHANGED_EVERY, Conn, HELLO_DEADLINE, HELLO_LIMIT, HelloError, LINE_LIMIT, LineReader,
    MAX_LINKS, Server, SessionFile,
};
use iced::futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

/// What the link threads tell the app.
#[derive(Clone, Debug)]
pub enum LinkEvent {
    /// A handshake succeeded.
    Opened {
        link: u64,
        name: String,
        writer: LinkWriter,
    },
    /// One request line.
    Request { link: u64, line: String },
    /// The client went away.
    Closed { link: u64 },
}

enum Out {
    Line(String),
    /// The newest rev is in the writer's `changed`.
    Changed,
    Close,
}

/// Connections still in their handshake, each holding a thread for up to
/// [`HELLO_DEADLINE`]; more are turned away at once (review findings F20,
/// C8).
const MAX_HANDSHAKES: usize = 8;

/// What a link's outbound queue can hold: one answer (a link has one
/// request at a time), one `changed` wake-up and a close.
const OUT_QUEUE: usize = 4;

/// Sends lines to one link. Dropping every copy closes the link quietly.
#[derive(Clone)]
pub struct LinkWriter {
    out: mpsc::SyncSender<Out>,
    /// Tells the link's reader that its request was answered, so it may
    /// read the next one.
    answered: mpsc::SyncSender<()>,
    /// The newest `changed` rev not yet written, and whether a wake-up for
    /// it is queued: a client that stops reading costs one queued event,
    /// however many edits are made.
    changed: Arc<(AtomicU64, AtomicBool)>,
}

impl std::fmt::Debug for LinkWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LinkWriter")
    }
}

impl LinkWriter {
    /// Sends the answer to the link's request, and lets its reader read
    /// the next one.
    pub fn send(&self, line: String) {
        let _ = self.out.try_send(Out::Line(line));
        let _ = self.answered.try_send(());
    }

    /// A `changed` event; bursts are sent at most every 50 ms.
    pub fn changed(&self, rev: u64) {
        self.changed.0.fetch_max(rev, Ordering::SeqCst);
        if !self.changed.1.swap(true, Ordering::SeqCst) {
            let _ = self.out.try_send(Out::Changed);
        }
    }

    /// Sends `{"event": "closed"}` and closes the link.
    pub fn close(&self) {
        let _ = self.out.try_send(Out::Close);
    }
}

/// What connection threads need to check a handshake.
#[derive(Debug, Default)]
struct Shared {
    docs: Vec<PathBuf>,
    doc_ids: Vec<Option<String>>,
    rev: u64,
    links: usize,
}

/// Where link events wait for the app. The windowed app drains it through
/// a subscription; the headless runner polls it.
#[derive(Debug)]
pub struct Inbox {
    rx: Mutex<Option<UnboundedReceiver<LinkEvent>>>,
}

impl Inbox {
    /// The receiver, once: for the subscription's stream.
    pub fn take(&self) -> Option<UnboundedReceiver<LinkEvent>> {
        self.rx.lock().ok().and_then(|mut r| r.take())
    }

    /// Events waiting now, without blocking (headless use).
    pub fn drain(&self) -> Vec<LinkEvent> {
        let mut out = Vec::new();
        if let Ok(mut guard) = self.rx.lock()
            && let Some(rx) = guard.as_mut()
        {
            while let Ok(ev) = rx.try_recv() {
                out.push(ev);
            }
        }
        out
    }
}

/// The subscription's identity: one per server.
#[derive(Clone, Debug)]
pub struct InboxKey {
    pub id: u64,
    pub inbox: Arc<Inbox>,
}

impl Hash for InboxKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// The subscription's stream: link events as app messages.
pub fn stream(key: &InboxKey) -> iced::futures::stream::BoxStream<'static, crate::app::Message> {
    use iced::futures::StreamExt;
    match key.inbox.take() {
        Some(rx) => rx.map(crate::app::Message::Link).boxed(),
        None => iced::futures::stream::empty().boxed(),
    }
}

/// The running server.
pub struct LinkServer {
    pub key: InboxKey,
    pub endpoint: String,
    token: String,
    run: PathBuf,
    pid: u32,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
}

static NEXT_SERVER: AtomicU64 = AtomicU64::new(1);

impl LinkServer {
    /// Starts listening and publishes the session file for `docs`.
    pub fn start(run: &Path, docs: Vec<PathBuf>, rev: u64) -> io::Result<LinkServer> {
        let pid = std::process::id();
        coldopen_link::remove_stale(run, pid);
        let server = Server::start(run, pid)?;
        let token = coldopen_link::new_token()?;
        let endpoint = server.endpoint.clone();
        let (tx, rx) = unbounded();
        let shared = Arc::new(Mutex::new(Shared {
            docs,
            doc_ids: Vec::new(),
            rev,
            links: 0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let mut hasher = DefaultHasher::new();
        endpoint.hash(&mut hasher);
        let id = NEXT_SERVER.fetch_add(1, Ordering::Relaxed) ^ (hasher.finish() << 16);
        let link = LinkServer {
            key: InboxKey {
                id,
                inbox: Arc::new(Inbox {
                    rx: Mutex::new(Some(rx)),
                }),
            },
            endpoint,
            token: token.clone(),
            run: run.to_path_buf(),
            pid,
            shared: shared.clone(),
            stop: stop.clone(),
        };
        link.publish()?;
        std::thread::Builder::new()
            .name("agent-link".into())
            .spawn(move || accept_loop(server, token, shared, tx, stop))?;
        Ok(link)
    }

    fn publish(&self) -> io::Result<()> {
        let (docs, doc_ids) = self
            .shared
            .lock()
            .map(|s| (s.docs.clone(), s.doc_ids.clone()))
            .unwrap_or_default();
        let path = SessionFile::path_in(&self.run, self.pid);
        if docs.is_empty() {
            let _ = std::fs::remove_file(path);
            return Ok(());
        }
        SessionFile {
            v: coldopen_link::VERSION,
            pid: self.pid,
            app: env!("CARGO_PKG_VERSION").into(),
            endpoint: self.endpoint.clone(),
            token: self.token.clone(),
            docs,
            doc_ids: if doc_ids.iter().any(Option::is_some) {
                doc_ids
            } else {
                Vec::new()
            },
        }
        .write(&self.run)
        .map(|_| ())
    }

    /// The scripts open in the app, as canonical paths, with each one's
    /// identity when it is a bundle; rewrites the session file when they
    /// change.
    pub fn set_docs(&self, docs: Vec<PathBuf>, doc_ids: Vec<Option<String>>) -> io::Result<()> {
        let changed = match self.shared.lock() {
            Ok(mut s) if s.docs != docs || s.doc_ids != doc_ids => {
                s.docs = docs;
                s.doc_ids = doc_ids;
                true
            }
            _ => false,
        };
        if changed { self.publish() } else { Ok(()) }
    }

    pub fn set_rev(&self, rev: u64) {
        if let Ok(mut s) = self.shared.lock() {
            s.rev = rev;
        }
    }

    pub fn session_file(&self) -> PathBuf {
        SessionFile::path_in(&self.run, self.pid)
    }
}

impl Drop for LinkServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(self.session_file());
        // Wake the accept loop so it sees the stop flag.
        let _ = Conn::connect(&self.endpoint);
    }
}

fn accept_loop(
    server: Server,
    token: String,
    shared: Arc<Mutex<Shared>>,
    tx: UnboundedSender<LinkEvent>,
    stop: Arc<AtomicBool>,
) {
    let server = Arc::new(server);
    let next = AtomicU64::new(1);
    let handshakes = Arc::new(AtomicUsize::new(0));
    loop {
        let conn = server.accept();
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(conn) = conn else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        if !server.same_user(&conn) {
            continue;
        }
        if handshakes.fetch_add(1, Ordering::SeqCst) >= MAX_HANDSHAKES {
            handshakes.fetch_sub(1, Ordering::SeqCst);
            let mut conn = conn;
            let _ = writeln!(
                conn,
                "{}",
                coldopen_link::reply_err(HelloError::TooManyLinks)
            );
            let _ = conn.flush();
            continue;
        }
        let link = next.fetch_add(1, Ordering::Relaxed);
        let (token, shared, tx) = (token.clone(), shared.clone(), tx.clone());
        let hs = handshakes.clone();
        let _ = std::thread::Builder::new()
            .name(format!("agent-link-{link}"))
            .spawn(move || serve_link(link, conn, &token, &shared, &tx, &hs));
    }
}

/// Reads the first line within the deadline, at most [`HELLO_LIMIT`] bytes.
fn read_hello(conn: &mut Conn) -> Option<String> {
    conn.set_nonblocking(true).ok()?;
    let start = Instant::now();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let line = loop {
        if let Some(i) = buf.iter().position(|&b| b == b'\n') {
            buf.truncate(i);
            break Some(buf);
        }
        if buf.len() > HELLO_LIMIT || start.elapsed() > HELLO_DEADLINE {
            break None;
        }
        match conn.read(&mut chunk) {
            // A Windows named pipe in non-blocking mode reads nothing, not
            // `WouldBlock`, while the client has not written yet; a client
            // that has gone away fails the read instead. So a hello that
            // arrives a moment late is still read (found while testing
            // review finding F20).
            Ok(0) if cfg!(windows) => std::thread::sleep(Duration::from_millis(5)),
            Ok(0) => break None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break None,
        }
    };
    conn.set_nonblocking(false).ok()?;
    String::from_utf8(line?).ok()
}

fn check_hello(
    line: &str,
    token: &str,
    shared: &Mutex<Shared>,
) -> Result<(String, PathBuf, u64), HelloError> {
    let h = coldopen_link::parse_hello(line)?;
    if !coldopen_link::tokens_equal(&h.token, token) {
        return Err(HelloError::BadToken);
    }
    let mut s = shared.lock().map_err(|_| HelloError::BadHello)?;
    let doc = coldopen_link::canonical(&h.doc);
    if !s.docs.contains(&doc) {
        return Err(HelloError::WrongDoc);
    }
    if s.links >= MAX_LINKS {
        return Err(HelloError::TooManyLinks);
    }
    s.links += 1;
    Ok((coldopen_link::clean_name(&h.client.name), doc, s.rev))
}

fn serve_link(
    link: u64,
    mut conn: Conn,
    token: &str,
    shared: &Mutex<Shared>,
    tx: &UnboundedSender<LinkEvent>,
    handshakes: &AtomicUsize,
) {
    let hello = read_hello(&mut conn);
    handshakes.fetch_sub(1, Ordering::SeqCst);
    let Some(line) = hello else {
        let _ = writeln!(conn, "{}", coldopen_link::reply_err(HelloError::BadHello));
        return;
    };
    let (name, doc, rev) = match check_hello(&line, token, shared) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(conn, "{}", coldopen_link::reply_err(e));
            return;
        }
    };
    if writeln!(
        conn,
        "{}",
        coldopen_link::reply_ok(env!("CARGO_PKG_VERSION"), &doc, rev)
    )
    .is_err()
    {
        release(shared);
        return;
    }
    let (recv, send) = conn.split();
    let (out_tx, out_rx) = mpsc::sync_channel(OUT_QUEUE);
    let (answered_tx, answered_rx) = mpsc::sync_channel(1);
    let changed = Arc::new((AtomicU64::new(0), AtomicBool::new(false)));
    let writer = LinkWriter {
        out: out_tx,
        answered: answered_tx,
        changed: changed.clone(),
    };
    let _ = std::thread::Builder::new()
        .name(format!("agent-link-{link}-out"))
        .spawn(move || write_loop(send, out_rx, &changed));
    let _ = tx.unbounded_send(LinkEvent::Opened { link, name, writer });
    let mut reader = LineReader::new(recv);
    while let Ok(Some(line)) = reader.read_line(LINE_LIMIT) {
        if line.trim() == coldopen_link::BYE
            || tx
                .unbounded_send(LinkEvent::Request { link, line })
                .is_err()
        {
            break;
        }
        // One request at a time: the next line is read only once the app
        // has answered this one, so a client that sends without waiting
        // queues at most one request in front of the writer, and the rest
        // wait in its own socket (review findings F20, C8). The app drops
        // the link's writer when it closes the link, which ends the wait.
        if answered_rx.recv().is_err() {
            break;
        }
    }
    release(shared);
    let _ = tx.unbounded_send(LinkEvent::Closed { link });
}

fn release(shared: &Mutex<Shared>) {
    if let Ok(mut s) = shared.lock() {
        s.links = s.links.saturating_sub(1);
    }
}

fn write_loop(mut send: impl Write, rx: mpsc::Receiver<Out>, changed: &(AtomicU64, AtomicBool)) {
    let mut pending: Option<u64> = None;
    let mut last: Option<Instant> = None;
    loop {
        let due = |last: Option<Instant>| {
            last.map_or(Duration::ZERO, |t| {
                CHANGED_EVERY.saturating_sub(t.elapsed())
            })
        };
        let msg = if pending.is_some() {
            match rx.recv_timeout(due(last)) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => return,
            }
        };
        let ok = match msg {
            Some(Out::Line(s)) => writeln!(send, "{s}").and_then(|()| send.flush()).is_ok(),
            Some(Out::Changed) => {
                changed.1.store(false, Ordering::SeqCst);
                let r = changed.0.load(Ordering::SeqCst);
                pending = Some(pending.map_or(r, |p| p.max(r)));
                true
            }
            Some(Out::Close) => {
                if let Some(r) = pending.take() {
                    let _ = writeln!(
                        send,
                        "{}",
                        serde_json::json!({"event": "changed", "rev": r})
                    );
                }
                let _ = writeln!(send, "{}", serde_json::json!({"event": "closed"}));
                let _ = send.flush();
                return;
            }
            None => true,
        };
        if !ok {
            return;
        }
        if let Some(r) = pending
            && due(last).is_zero()
        {
            let line = serde_json::json!({"event": "changed", "rev": r});
            if writeln!(send, "{line}")
                .and_then(|()| send.flush())
                .is_err()
            {
                return;
            }
            pending = None;
            last = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Antigravity's finding 10: a client that stops reading made every
    /// `changed` event wait in an unbounded queue. They coalesce now: one
    /// wake-up waits, carrying the newest rev.
    #[test]
    fn changed_events_coalesce_while_the_client_is_not_reading() {
        let (out, rx) = mpsc::sync_channel(OUT_QUEUE);
        let (answered, _answered_rx) = mpsc::sync_channel(1);
        let changed = Arc::new((AtomicU64::new(0), AtomicBool::new(false)));
        let w = LinkWriter {
            out,
            answered,
            changed: changed.clone(),
        };
        for rev in 1..=1000 {
            w.changed(rev);
        }
        w.send("answer".into());
        let queued: Vec<Out> = rx.try_iter().collect();
        assert_eq!(queued.len(), 2);
        assert!(matches!(queued[0], Out::Changed));
        assert_eq!(changed.0.load(Ordering::SeqCst), 1000);
    }
}
