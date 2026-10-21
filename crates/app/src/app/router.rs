//! One command router (docs/specs/editor.md §16; review finding F18.5).
//! Keys, buttons, the palette and its params form, and agents all send
//! what they want done through [`App::dispatch`], which decides in one
//! place what each source may do and which questions come first.

use std::collections::BTreeMap;
use std::path::PathBuf;

use coldopen_core::command::Class;
use coldopen_core::{Author, BlockId, Command, CommandError};
use coldopen_screenplay::Element;
use iced::Task;
use serde_json::Value;

use super::{After, App, Message};

/// Who sent a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A shortcut.
    Key,
    /// A button or the start screen.
    Button,
    /// A palette entry.
    Palette,
    /// The palette's params form: a command with params as written.
    Form,
    /// An agent over the link, by its handshake name and MCP tool.
    Agent { name: String, tool: Option<String> },
}

impl Origin {
    fn is_agent(&self) -> bool {
        matches!(self, Origin::Agent { .. })
    }
}

/// What is asked: an app action (editor.md §10) or a core command.
#[derive(Clone, Debug)]
pub enum Request {
    Action(String),
    Command(Command),
}

/// What a request led to: a task for the app, and for a command the
/// response to give back (to the params form, or to an agent).
pub struct Outcome {
    pub task: Task<Message>,
    pub response: Option<Result<Value, CommandError>>,
    /// The request changed the document (an edit, undo or redo).
    pub edited: bool,
}

impl Outcome {
    fn task(task: Task<Message>) -> Outcome {
        Outcome {
            task,
            response: None,
            edited: false,
        }
    }

    fn answer(response: Result<Value, CommandError>) -> Outcome {
        Outcome {
            task: Task::none(),
            response: Some(response),
            edited: false,
        }
    }
}

fn refused(cmd: &Command, why: &str) -> CommandError {
    CommandError::new(
        "not_allowed_attached",
        format!("{} is not allowed {why}", cmd.name()),
    )
}

impl App {
    /// Runs one request from any source (editor.md §16).
    pub fn dispatch(&mut self, request: Request, origin: Origin) -> Outcome {
        match (request, origin) {
            (Request::Action(name), o) if !o.is_agent() => Outcome::task(self.action_by(&name, &o)),
            (Request::Action(name), _) => Outcome::answer(Err(CommandError::new(
                "not_allowed_attached",
                format!("{name} is an app action; agents send commands"),
            ))),
            (Request::Command(cmd), Origin::Agent { name, tool }) => {
                self.agent_command(cmd, &name, tool)
            }
            (Request::Command(cmd), _) => self.writer_command(cmd),
        }
    }

    /// A command the writer sent with params (the palette's form). Session
    /// commands run the app's flows, so the form never skips a question
    /// the screens ask (review finding F7).
    fn writer_command(&mut self, cmd: Command) -> Outcome {
        match cmd {
            Command::DocOpen(p) => {
                Outcome::task(self.leave(After::OpenPath(PathBuf::from(p.path))))
            }
            Command::DocNew(p) if p.path.is_some() => {
                self.banners
                    .push("A new script starts untitled; name it with Save as.".into());
                Outcome::task(Task::none())
            }
            Command::DocNew(_) => Outcome::task(self.leave(After::New)),
            Command::DocClose(_) => Outcome::task(self.leave(After::Close)),
            Command::DocRevert(_) => Outcome::task(self.proceed(After::Revert)),
            Command::DocSave(p) => Outcome::task(match p.path {
                Some(path) => self.save_as(PathBuf::from(path), p.allow_lossy, p.overwrite),
                None => self.save_with(p.allow_lossy, p.overwrite),
            }),
            Command::RecoveredRestore(p) => {
                Outcome::task(self.leave_for_restore(PathBuf::from(p.copy), PathBuf::from(p.path)))
            }
            Command::RecoveredDelete(p) => Outcome::task(self.ask_delete(PathBuf::from(p.copy))),
            other => {
                let edits = matches!(other.class(), Class::Edit | Class::History);
                let r = self.d.execute(other);
                let edited = edits && r.is_ok();
                let task = self.after_change(false);
                Outcome {
                    task,
                    response: Some(r),
                    edited,
                }
            }
        }
    }

    /// A command from an attached agent: refused where it would replace,
    /// close or rename the writer's script, or would need a question only
    /// the writer can answer; otherwise run with the agent as author, the
    /// writer's caret kept on the same text and the changed blocks marked.
    /// A save while the writer's save is written gets core's
    /// `save_in_progress` (journal.md, "One save at a time").
    fn agent_command(&mut self, cmd: Command, name: &str, tool: Option<String>) -> Outcome {
        if coldopen_link::refused(cmd.name(), &cmd.params()) {
            return Outcome::answer(Err(refused(
                &cmd,
                "while attached to the writer's script in the app",
            )));
        }
        if let Command::Batch(b) = &cmd
            && b.commands.len() > coldopen_link::BATCH_LIMIT
        {
            return Outcome::answer(Err(CommandError::invalid_params(format!(
                "a batch over the link holds at most {} commands",
                coldopen_link::BATCH_LIMIT
            ))));
        }
        let changes = matches!(cmd.class(), Class::Edit | Class::History);
        let before: BTreeMap<BlockId, Element> = if changes {
            self.d
                .session()
                .map(|s| {
                    s.doc
                        .blocks
                        .iter()
                        .map(|b| (b.id, b.element.clone()))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        let by = Author::agent(name, tool);
        let outcome = self.d.execute_as(cmd.clone(), &by);
        let edited = changes && outcome.is_ok();
        if let Ok(result) = &outcome
            && edited
        {
            self.editor.follow(&cmd, result);
            let tracked = result.get("change").is_some();
            if !tracked && let Some(s) = self.d.session() {
                for b in &s.doc.blocks {
                    if before.get(&b.id) != Some(&b.element) {
                        self.agent_marks.insert(b.id);
                    }
                }
            }
        }
        Outcome {
            task: Task::none(),
            response: Some(outcome),
            edited,
        }
    }
}
