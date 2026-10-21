//! Where tool calls go (docs/specs/mcp.md §5.5, §6): over the link into
//! the app when the app has the script open, otherwise into this process's
//! own dispatcher, which journals every entry as the CLI does.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use coldopen_core::command::Class;
use coldopen_core::{Author, Command, CommandError, Dispatcher, Options};
use coldopen_link::{AttachError, Client, LinkClient};
use serde_json::{Value, json};
use tokio::sync::watch;

/// How long a request over the link may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(120);

enum Mode {
    /// Nothing open yet.
    Idle,
    Local(Box<Dispatcher>),
    Attached {
        link: Arc<LinkClient>,
        doc: PathBuf,
    },
    /// The app closed the script this server was attached to.
    Gone,
}

/// Counters that move when the script changes or when one is opened or
/// closed, for resource notifications.
#[derive(Clone)]
pub struct Signals {
    pub changes: Arc<watch::Sender<u64>>,
    pub lists: Arc<watch::Sender<u64>>,
}

impl Signals {
    pub fn new() -> Signals {
        Signals {
            changes: Arc::new(watch::channel(0).0),
            lists: Arc::new(watch::channel(0).0),
        }
    }

    fn changed(&self) {
        self.changes.send_modify(|v| *v += 1);
    }

    fn listed(&self) {
        self.lists.send_modify(|v| *v += 1);
    }
}

impl Default for Signals {
    fn default() -> Signals {
        Signals::new()
    }
}

pub struct Backend {
    mode: Mode,
    /// Where the app's session files are, if anywhere.
    run: Option<PathBuf>,
    /// A script to open before the first request (`--doc`), and whether it
    /// may attach.
    pending: Option<(PathBuf, bool)>,
    pub signals: Signals,
    /// `--untracked`: agent edits in scripts opened here are not tracked.
    pub untracked: bool,
}

fn failure(code: &'static str, message: impl Into<String>) -> Value {
    Dispatcher::response(Value::Null, Err(CommandError::new(code, message)))
}

fn ok(result: Value) -> Value {
    Dispatcher::response(Value::Null, Ok(result))
}

impl Backend {
    pub fn new(run: Option<PathBuf>, doc: Option<(PathBuf, bool)>) -> Backend {
        Backend {
            mode: Mode::Idle,
            run,
            pending: doc,
            signals: Signals::new(),
            untracked: false,
        }
    }

    /// A dispatcher of this process's own, for a script opened here.
    fn local_dispatcher(&self) -> Box<Dispatcher> {
        let mut d = Box::new(Dispatcher::new(Options::default()));
        if self.untracked {
            d.set_agent_tracking(Some(false));
        }
        d
    }

    pub fn has_document(&self) -> bool {
        match &self.mode {
            Mode::Local(d) => d.session().is_some(),
            Mode::Attached { .. } => true,
            Mode::Idle | Mode::Gone => false,
        }
    }

    pub fn attached(&self) -> Option<&Path> {
        match &self.mode {
            Mode::Attached { doc, .. } => Some(doc),
            _ => None,
        }
    }

    /// Opens the `--doc` script, once, now that the client is known.
    fn open_pending(&mut self, client: &str) {
        if let Some((doc, attach)) = self.pending.take() {
            let params = json!({
                "path": doc,
                "attach": if attach { "auto" } else { "never" },
            });
            let _ = self.open(params, client);
        }
    }

    /// Runs a command in this process's own dispatcher, made on first
    /// use.
    fn run_local(&mut self, cmd: Command, by: &Author) -> Value {
        if !matches!(self.mode, Mode::Local(_)) {
            self.mode = Mode::Local(self.local_dispatcher());
        }
        match &mut self.mode {
            Mode::Local(d) => Dispatcher::response(Value::Null, d.execute_as(cmd, by)),
            _ => failure("no_document", "no script is open"),
        }
    }

    /// Ends whatever is open: detaches from the app, or shuts the local
    /// session down (its unsaved work stays in the journal).
    fn leave(&mut self) {
        match std::mem::replace(&mut self.mode, Mode::Idle) {
            Mode::Local(mut d) => d.shutdown(),
            Mode::Attached { link, .. } => link.bye(),
            Mode::Idle | Mode::Gone => {}
        }
    }

    /// Runs a command for the client's tool and returns the response
    /// envelope.
    pub fn call(&mut self, command: &str, params: Value, client: &str, tool: &str) -> Value {
        self.open_pending(client);
        if command == "doc.open" {
            return self.open(params, client);
        }
        let cmd = match Command::from_parts(command, params.clone()) {
            Ok(c) => c,
            Err(e) => return failure("invalid_params", e),
        };
        let class = cmd.class();
        let link = match &self.mode {
            Mode::Attached { link, .. } => Some(link.clone()),
            _ => None,
        };
        let response = if let Some(link) = link {
            if command == "doc.close" {
                self.leave();
                self.signals.listed();
                return ok(json!({ "detached": true }));
            }
            match link.call(command, params, Some(tool), CALL_TIMEOUT) {
                Ok(r) => r,
                Err(_) if link.is_closed() => {
                    self.mode = Mode::Gone;
                    self.signals.listed();
                    return gone();
                }
                Err(e) => failure("io_error", format!("the link to the app failed: {e}")),
            }
        } else if matches!(self.mode, Mode::Gone) && command != "doc.new" {
            return gone();
        } else {
            let by = Author::agent(client, Some(tool.to_string()));
            self.run_local(cmd, &by)
        };
        if response["ok"] == true {
            match class {
                Class::Edit | Class::History => self.signals.changed(),
                Class::Session => {
                    self.signals.listed();
                    self.signals.changed();
                }
                Class::Query | Class::Output | Class::File => {}
            }
        }
        response
    }

    /// `doc_open`: attach to the app when it has the script open,
    /// otherwise open it here.
    fn open(&mut self, mut params: Value, client: &str) -> Value {
        let attach = match params.as_object_mut().and_then(|o| o.remove("attach")) {
            None => true,
            Some(v) if v == "auto" => true,
            Some(v) if v == "never" => false,
            Some(v) => {
                return failure(
                    "invalid_params",
                    format!("attach must be \"auto\" or \"never\", not {v}"),
                );
            }
        };
        let Some(path) = params["path"].as_str().map(PathBuf::from) else {
            return failure("invalid_params", "doc_open needs a \"path\"");
        };
        let by = Author::agent(client, Some("doc_open".to_string()));
        let cmd = match Command::from_parts("doc.open", params) {
            Ok(c) => c,
            Err(e) => return failure("invalid_params", e),
        };
        // Nothing is closed until the new script is open, and unsaved work
        // is never dropped to open another (review finding C5).
        if let Mode::Local(d) = &self.mode
            && d.session().is_some_and(|s| s.dirty)
        {
            return failure(
                "unsaved_changes",
                "the open script has unsaved changes; save it, or close it with discard, before opening another",
            );
        }
        let mut refusal = None;
        if attach && let Some(run) = &self.run {
            for s in coldopen_link::find_sessions(run, &path) {
                let who = Client {
                    name: client.to_string(),
                    version: None,
                };
                match LinkClient::attach(&s, &who, &path) {
                    Ok(link) => {
                        self.leave();
                        let link = Arc::new(link);
                        self.watch(link.clone());
                        let rev = link.hello.rev;
                        let doc = link.hello.doc.clone();
                        let scenes = link
                            .call("scene.list", json!({}), Some("doc_open"), CALL_TIMEOUT)
                            .ok()
                            .and_then(|r| r["result"]["scenes"].as_array().map(Vec::len));
                        self.mode = Mode::Attached {
                            link,
                            doc: doc.clone(),
                        };
                        self.signals.listed();
                        return ok(json!({
                            "path": doc,
                            "attached": true,
                            "rev": rev,
                            "scenes": scenes,
                            "warnings": [],
                            "recovered": null,
                        }));
                    }
                    Err(AttachError::Refused(code, msg)) => {
                        refusal = Some(format!("{code}: {msg}"))
                    }
                    Err(AttachError::Io(_)) => {}
                }
            }
        }
        // The same script, open here already: it is clean (see above), and
        // must let go of its journal before it is opened again.
        let reopening = matches!(&self.mode, Mode::Local(d) if d
            .session()
            .and_then(|s| s.path.as_deref())
            .is_some_and(|p| coldopen_link::canonical(p) == coldopen_link::canonical(&path)));
        if reopening {
            self.leave();
        }
        let mut fresh = self.local_dispatcher();
        let mut response = Dispatcher::response(Value::Null, fresh.execute_as(cmd, &by));
        if response["ok"] == true {
            self.leave();
            self.mode = Mode::Local(fresh);
        }
        if response["error"]["code"] == "doc_locked" {
            let why = match (attach, refusal) {
                (false, _) => "the script is open in the Cold Open app; open it with attach \"auto\" to edit it there".to_string(),
                (true, Some(r)) => format!("the script is open in the Cold Open app, which refused this agent ({r})"),
                (true, None) => "the script is open in another Cold Open process that does not take agents (another `coldopen mcp` or `coldopen serve`)".to_string(),
            };
            response["error"]["message"] = json!(why);
        }
        if response["ok"] == true {
            self.signals.listed();
            self.signals.changed();
        }
        response
    }

    /// Forwards the app's events to the resource signals, on a thread of
    /// its own.
    fn watch(&self, link: Arc<LinkClient>) {
        let signals = self.signals.clone();
        let _ = std::thread::Builder::new()
            .name("link-events".into())
            .spawn(move || {
                loop {
                    match link.next_event(Duration::from_millis(500)) {
                        Some(ev) if ev["event"] == "changed" => signals.changed(),
                        Some(ev) if ev["event"] == "closed" => {
                            signals.listed();
                            return;
                        }
                        Some(_) => {}
                        None if link.is_closed() => {
                            signals.listed();
                            return;
                        }
                        None => {}
                    }
                }
            });
    }

    /// Notices that the app closed the link since the last call.
    pub fn refresh(&mut self) {
        if let Mode::Attached { link, .. } = &self.mode
            && link.is_closed()
        {
            self.mode = Mode::Gone;
        }
    }

    pub fn shutdown(&mut self) {
        self.leave();
    }
}

fn gone() -> Value {
    failure(
        "no_document",
        "the Cold Open app closed the script this agent was attached to; open it again with doc_open",
    )
}
