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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    Changed(u64),
    Close,
}

/// Sends lines to one link. Dropping every copy closes the link quietly.
#[derive(Clone)]
pub struct LinkWriter(mpsc::Sender<Out>);

impl std::fmt::Debug for LinkWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LinkWriter")
    }
}

impl LinkWriter {
    pub fn send(&self, line: String) {
        let _ = self.0.send(Out::Line(line));
    }

    /// A `changed` event; bursts are sent at most every 50 ms.
    pub fn changed(&self, rev: u64) {
        let _ = self.0.send(Out::Changed(rev));
    }

    /// Sends `{"event": "closed"}` and closes the link.
    pub fn close(&self) {
        let _ = self.0.send(Out::Close);
    }
}

/// What connection threads need to check a handshake.
#[derive(Debug, Default)]
struct Shared {
    docs: Vec<PathBuf>,
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
        let docs = self
            .shared
            .lock()
            .map(|s| s.docs.clone())
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
        }
        .write(&self.run)
        .map(|_| ())
    }

    /// The scripts open in the app, as canonical paths; rewrites the
    /// session file when they change.
    pub fn set_docs(&self, docs: Vec<PathBuf>) -> io::Result<()> {
        let changed = match self.shared.lock() {
            Ok(mut s) if s.docs != docs => {
                s.docs = docs;
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
        let link = next.fetch_add(1, Ordering::Relaxed);
        let (token, shared, tx) = (token.clone(), shared.clone(), tx.clone());
        let _ = std::thread::Builder::new()
            .name(format!("agent-link-{link}"))
            .spawn(move || serve_link(link, conn, &token, &shared, &tx));
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
) {
    let Some(line) = read_hello(&mut conn) else {
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
    let (out_tx, out_rx) = mpsc::channel();
    let writer = LinkWriter(out_tx);
    let _ = std::thread::Builder::new()
        .name(format!("agent-link-{link}-out"))
        .spawn(move || write_loop(send, out_rx));
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
    }
    release(shared);
    let _ = tx.unbounded_send(LinkEvent::Closed { link });
}

fn release(shared: &Mutex<Shared>) {
    if let Ok(mut s) = shared.lock() {
        s.links = s.links.saturating_sub(1);
    }
}

fn write_loop(mut send: impl Write, rx: mpsc::Receiver<Out>) {
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
            Some(Out::Changed(r)) => {
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
