//! The client side of a link, as `coldopen mcp` uses it: the handshake,
//! requests matched to their responses by id, and the app's events on
//! their own channel.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use interprocess::local_socket::SendHalf;
use serde_json::{Value, json};

use crate::{Client, Conn, HELLO_LIMIT, HelloOk, LINE_LIMIT, LineReader, SessionFile};

type Pending = Arc<Mutex<BTreeMap<u64, Sender<Value>>>>;

/// An attached link.
pub struct LinkClient {
    /// Taken away when the app closes the link, which hangs up.
    send: Arc<Mutex<Option<SendHalf>>>,
    pending: Pending,
    next: AtomicU64,
    events: Mutex<Receiver<Value>>,
    closed: Arc<AtomicBool>,
    /// What the app said in the handshake.
    pub hello: HelloOk,
}

/// Why attaching failed.
#[derive(Debug)]
pub enum AttachError {
    /// Nothing answered at the endpoint, or the link broke.
    Io(io::Error),
    /// The app refused the handshake: its code and message.
    Refused(String, String),
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachError::Io(e) => write!(f, "{e}"),
            AttachError::Refused(code, msg) => write!(f, "{code}: {msg}"),
        }
    }
}

impl LinkClient {
    /// Connects to the app named by a session file and attaches to `doc`.
    pub fn attach(
        session: &SessionFile,
        client: &Client,
        doc: &Path,
    ) -> Result<LinkClient, AttachError> {
        let mut conn = Conn::connect(&session.endpoint).map_err(AttachError::Io)?;
        writeln!(conn, "{}", crate::hello_line(&session.token, client, doc))
            .and_then(|()| conn.flush())
            .map_err(AttachError::Io)?;
        let (recv, send) = conn.split();
        let mut reader = LineReader::new(recv);
        let line = reader
            .read_line(HELLO_LIMIT)
            .map_err(AttachError::Io)?
            .ok_or_else(|| AttachError::Io(io::ErrorKind::UnexpectedEof.into()))?;
        let hello = crate::parse_reply(&line).map_err(|(c, m)| AttachError::Refused(c, m))?;
        let pending: Pending = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        let send = Arc::new(Mutex::new(Some(send)));
        let (ev_tx, ev_rx) = mpsc::channel();
        {
            let pending = pending.clone();
            let closed = closed.clone();
            let send = send.clone();
            std::thread::Builder::new()
                .name("link-client".into())
                .spawn(move || read_loop(reader, &pending, &ev_tx, &closed, &send))
                .map_err(AttachError::Io)?;
        }
        Ok(LinkClient {
            send,
            pending,
            next: AtomicU64::new(1),
            events: Mutex::new(ev_rx),
            closed,
            hello,
        })
    }

    /// Sends a request; its response arrives on the returned channel.
    pub fn request(
        &self,
        command: &str,
        params: Value,
        tool: Option<&str>,
    ) -> io::Result<Receiver<Value>> {
        if self.is_closed() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the app closed the link",
            ));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        if let Ok(mut p) = self.pending.lock() {
            p.insert(id, tx);
        }
        let mut req = json!({ "v": 1, "id": id, "command": command, "params": params });
        if let Some(t) = tool {
            req["tool"] = json!(t);
        }
        let mut guard = self
            .send
            .lock()
            .map_err(|_| io::Error::other("the link's writer failed"))?;
        let send = guard
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "the app closed the link"))?;
        writeln!(send, "{req}").and_then(|()| send.flush())?;
        Ok(rx)
    }

    /// Sends a request and waits for its response envelope.
    pub fn call(
        &self,
        command: &str,
        params: Value,
        tool: Option<&str>,
        timeout: Duration,
    ) -> io::Result<Value> {
        let rx = self.request(command, params, tool)?;
        rx.recv_timeout(timeout).map_err(|e| match e {
            mpsc::RecvTimeoutError::Timeout => {
                io::Error::new(io::ErrorKind::TimedOut, "the app did not answer in time")
            }
            mpsc::RecvTimeoutError::Disconnected => {
                io::Error::new(io::ErrorKind::BrokenPipe, "the app closed the link")
            }
        })
    }

    /// The next event from the app, if one is waiting: `changed` or
    /// `closed`.
    pub fn try_event(&self) -> Option<Value> {
        self.events.lock().ok()?.try_recv().ok()
    }

    /// Waits up to `timeout` for the next event.
    pub fn next_event(&self, timeout: Duration) -> Option<Value> {
        self.events.lock().ok()?.recv_timeout(timeout).ok()
    }

    /// Says goodbye: the app closes the link, which ends it on both sides.
    pub fn bye(&self) {
        if let Ok(mut guard) = self.send.lock()
            && let Some(send) = guard.as_mut()
        {
            let _ = writeln!(send, "{}", crate::BYE).and_then(|()| send.flush());
        }
    }

    /// The app closed the link, or it broke.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

fn read_loop(
    mut reader: LineReader<interprocess::local_socket::RecvHalf>,
    pending: &Pending,
    events: &Sender<Value>,
    closed: &AtomicBool,
    send: &Mutex<Option<SendHalf>>,
) {
    while let Ok(Some(line)) = reader.read_line(LINE_LIMIT) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if v.get("event").is_some() {
            let bye = v["event"] == "closed";
            let _ = events.send(v);
            if bye {
                break;
            }
            continue;
        }
        let waiter = v["id"]
            .as_u64()
            .and_then(|id| pending.lock().ok()?.remove(&id));
        if let Some(w) = waiter {
            let _ = w.send(v);
        }
    }
    closed.store(true, Ordering::SeqCst);
    // Hang up: with both halves gone the app's side sees the end.
    if let Ok(mut s) = send.lock() {
        s.take();
    }
    // Anyone still waiting gets a broken pipe rather than a hang.
    if let Ok(mut p) = pending.lock() {
        p.clear();
    }
}
