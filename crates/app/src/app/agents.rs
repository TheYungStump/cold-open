//! Agents in the app (docs/specs/editor.md §12, docs/specs/mcp.md §5):
//! keeping the link server in step with the open script and the agent
//! access setting, and running each agent request as a labelled step.

use std::path::PathBuf;

use coldopen_core::{CommandError, Dispatcher};
use iced::Task;
use serde_json::Value;

use super::{App, Message, Origin, Request};
use crate::link::{LinkEvent, LinkServer, LinkWriter};

/// An attached agent.
#[derive(Debug)]
pub struct Agent {
    /// From the handshake: what the status bar and undo labels show.
    pub name: String,
    writer: LinkWriter,
    /// Edits it made, for the status bar.
    pub edits: u64,
    /// The script it attached to. A request after the writer has moved to
    /// another script closes the link instead (review finding F7).
    doc: Option<PathBuf>,
}

impl App {
    /// Starts, updates or stops the link server so it matches the agent
    /// access setting and the open script, and tells agents about changes.
    pub(super) fn sync_link(&mut self) {
        let path = self.d.session().and_then(|s| s.path.clone());
        let want = self.settings.agent_access && path.is_some() && self.dirs.is_some();
        if !want {
            if self.link.is_some() {
                self.close_links();
                self.link = None;
                self.link_path = None;
            }
            return;
        }
        if self.link.is_none() && !self.link_failed {
            let Some(run) = self.dirs.as_ref().map(|d| d.run.clone()) else {
                return;
            };
            let rev = self.d.session().map_or(0, |s| s.rev);
            let docs = path.iter().map(|p| coldopen_link::canonical(p)).collect();
            match LinkServer::start(&run, docs, rev) {
                Ok(l) => {
                    self.link = Some(l);
                    self.link_path = path.clone();
                }
                Err(e) => {
                    self.link_failed = true;
                    self.banners
                        .push(format!("Agents can't attach to this script: {e}").into());
                }
            }
        }
        if self.link.is_none() {
            return;
        }
        if self.link_path != path {
            // An agent attached to one script never follows the writer to
            // another (review finding F7).
            self.close_links();
            self.link_path = path.clone();
        }
        // The session file names the script, and its identity when it is a
        // bundle (docs/specs/mcp.md §5.1); it is rewritten only on a change.
        let docs: Vec<_> = path.iter().map(|p| coldopen_link::canonical(p)).collect();
        let ids = vec![self.d.session().and_then(|s| s.doc.doc_id.clone())];
        if let Some(link) = &self.link
            && let Err(e) = link.set_docs(docs, ids)
        {
            self.banners
                .push(format!("The agent link could not be updated: {e}").into());
        }
        let Some(link) = &self.link else { return };
        let rev = self.d.session().map(|s| s.rev);
        if rev != self.sent_rev {
            self.sent_rev = rev;
            if let Some(r) = rev {
                link.set_rev(r);
                for a in self.agents.values() {
                    a.writer.changed(r);
                }
            }
        }
    }

    /// Closes every agent link; their steps stay in the history.
    pub(super) fn close_links(&mut self) {
        for a in std::mem::take(&mut self.agents).into_values() {
            a.writer.close();
        }
    }

    /// Link events waiting now, as messages, for the headless runner (the
    /// windowed app gets them through its subscription).
    pub fn link_messages(&self) -> Vec<Message> {
        self.link
            .as_ref()
            .map(|l| l.key.inbox.drain())
            .unwrap_or_default()
            .into_iter()
            .map(Message::Link)
            .collect()
    }

    pub(super) fn link_event(&mut self, ev: LinkEvent) -> Task<Message> {
        match ev {
            LinkEvent::Opened { link, name, writer } => {
                if self.link.is_none() || self.d.session().is_none() {
                    writer.close();
                } else {
                    let doc = self.d.session().and_then(|s| s.path.clone());
                    self.agents.insert(
                        link,
                        Agent {
                            name,
                            writer,
                            edits: 0,
                            doc,
                        },
                    );
                }
                Task::none()
            }
            LinkEvent::Closed { link } => {
                // The client said goodbye or went away: close our side too.
                if let Some(a) = self.agents.remove(&link) {
                    a.writer.close();
                }
                Task::none()
            }
            LinkEvent::Request { link, line } => {
                let Some(name) = self.agents.get(&link).map(|a| a.name.clone()) else {
                    return Task::none();
                };
                let open = self.d.session().and_then(|s| s.path.clone());
                if self.agents.get(&link).is_some_and(|a| a.doc != open) {
                    if let Some(a) = self.agents.remove(&link) {
                        a.writer.close();
                    }
                    return Task::none();
                }
                let (response, edited) = self.agent_request(&name, &line);
                if let Some(a) = self.agents.get_mut(&link) {
                    if edited {
                        a.edits += 1;
                    }
                    a.writer.send(response.to_string());
                }
                self.after_change(false)
            }
        }
    }

    /// Runs one request line from an agent: checked, run with the agent as
    /// author, the writer's caret kept on the same text, and the blocks it
    /// changed marked. Returns the response and whether it changed the
    /// document.
    fn agent_request(&mut self, name: &str, line: &str) -> (Value, bool) {
        let mut request: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let err = CommandError::new("invalid_request", format!("not valid JSON: {e}"));
                return (Dispatcher::response(Value::Null, Err(err)), false);
            }
        };
        let tool = request
            .as_object_mut()
            .and_then(|o| o.remove("tool"))
            .and_then(|t| t.as_str().map(|t| t.chars().take(64).collect::<String>()));
        let (id, cmd) = Dispatcher::read_request(&request);
        let cmd = match cmd {
            Ok(c) => c,
            Err(e) => return (Dispatcher::response(id, Err(e)), false),
        };
        let out = self.dispatch(
            Request::Command(cmd),
            Origin::Agent {
                name: name.to_string(),
                tool,
            },
        );
        let response = out.response.unwrap_or_else(|| {
            Err(CommandError::new(
                "internal_error",
                "the request was not answered",
            ))
        });
        (Dispatcher::response(id, response), out.edited)
    }

    /// The status bar's agent line, while any agent is attached.
    pub fn agent_status(&self) -> Option<String> {
        let first = self.agents.values().next()?;
        let edits: u64 = self.agents.values().map(|a| a.edits).sum();
        let who = if self.agents.len() == 1 {
            first.name.clone()
        } else {
            format!("{} agents", self.agents.len())
        };
        Some(format!(
            "{who} connected · {edits} {}",
            if edits == 1 { "edit" } else { "edits" }
        ))
    }
}
