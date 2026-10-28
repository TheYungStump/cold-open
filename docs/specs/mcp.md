# MCP server and the agent link

- **Date:** 2026-10-15, updated 2026-10-16
- **Status:** implemented (phase 3, 2026-10-15): the server in `crates/mcp`, run as `coldopen mcp`; the link's protocol and transport in `crates/link`, shared by the server and the app; the app's end of the link in `crates/app`. The agent gate (`crates/cli/tests/agent_gate.rs`) runs a real `coldopen mcp` process against the app. Phase 4 (2026-10-16) added the tools for .fdx export, file repair and recovered copies (§3). Phase 5a (2026-10-18) adds agent edits as tracked changes (§6.1), the `changes_*` tools, the changes resource and `doc_ids` in the session file; being implemented.
- **Sources:**
  - `docs/spec.md`: Command interface, "Agents edit in the open", License (`coldopen mcp` and connecting any agent are always free).
  - The phase 3 brief and its approved decisions (2026-10-15).
  - `docs/specs/commands.md`, `docs/specs/journal.md`, `docs/specs/editor.md`.
  - The Model Context Protocol specification, <https://modelcontextprotocol.io/specification/latest>, read 2026-10-15 (revision 2026-07-28): tools, resources, tool-name characters, tool results, error reporting.
  - Anthropic's tool-name rule for the Claude API (letters, digits, `_` and `-` only, at most 64 characters), which shapes the tool names below.
  - The reviews of phases 1–3 (2026-10-15), listed in `docs/progress/review-fixes.md`.
- No Final Draft material. A choice where the sources are silent is marked **Resolution**.

## 1. Goals

1. **Every command is a tool.** An agent can do anything the app can do, with the same names and params as `docs/specs/commands.md`.
2. **Edit in the open.** When the writer has the script open in the desktop app, the agent's edits go into that open document. The writer sees them appear, and can undo them like any other step.
3. **Local and private.** The link between `coldopen mcp` and the app accepts only the same user on the same machine. It is never reachable over a network.
4. **Free.** Nothing here needs an account, a key or a network connection.

## 2. Running the server

```bash
coldopen mcp [--doc PATH] [--no-attach] [--untracked]
```

- **Transport.** The server speaks MCP over standard input and output (the stdio transport), built on the `rmcp` SDK. It supports the protocol revisions that `rmcp` supports, and is tested against the latest.
- **Starting with a script.** With `--doc`, the server opens or attaches to that script before serving, as `doc_open` would. `--no-attach` never attaches (§5) and always opens the file in this process.
- **`--untracked`** (2026-10-18) records the agent's edits untracked in scripts the server opens itself (§6.1).
- **One client per process.** Over stdio each server process has exactly one client, so the open document is that process's state. Tools act on it, and `doc_open` switches it. **Resolution:** the protocol has no sessions, so its advice is to pass state handles explicitly. That advice is aimed at shared servers; a stdio server is never shared, so tools take no document handle.
- **Server instructions.** The server's instructions tell the agent:
  - to start with `doc_open`;
  - that blocks are named by ids such as `b_12`, and offsets count characters;
  - that `batch` makes several changes one undo step;
  - that when the script is open in the app, the writer sees each edit live and can undo it.

## 3. Tools

**Names.** A command's dotted name becomes the tool name with each `.` replaced by `_`: `block.insert` → `block_insert`, `undo` → `undo`. **Resolution:** the protocol allows dots in tool names, but the Claude API does not, and underscores work everywhere.

| Tool | Command | Notes |
| --- | --- | --- |
| `doc_new`, `doc_open`, `doc_save`, `doc_close`, `doc_revert` | session | `doc_open` takes an extra `attach` param (§5). When attached, only `doc_open`, `doc_close` and `doc_save` without a path are allowed (§5.4). |
| `doc_get`, `doc_export`, `scene_list`, `text_find`, `text_complete`, `history_list`, `recovered_list` | query | `recovered_list` added 2026-10-16. |
| `block_insert`, `block_set_type`, `block_set_text`, `block_set_attrs`, `block_delete`, `block_split`, `block_join`, `text_insert`, `text_delete`, `text_style`, `text_replace`, `scene_move`, `batch` | edit | Each call is one undo step, labelled `<client name> · <tool>`. |
| `undo`, `redo` | history | An agent may undo or redo only its own steps (`not_your_step`). |
| `changes_list` | query | Added 2026-10-18: the pending tracked changes. |
| `changes_reject` | edit | Added 2026-10-18: an agent may reject only its own changes (§6.1). |
| `changes_accept`, `changes_track` | writer-only | Added 2026-10-18. Listed, but an agent always gets `writer_only`: accepting a change, and choosing what is tracked, are the writer's. |
| `export_pdf`, `export_fdx`, `export_fountain` | output | The last two added 2026-10-16. |
| `file_repair` | file | Added 2026-10-16: writes a repaired copy of an .fdx file to a new path (`docs/specs/commands.md`, "Files"). |
| `recovered_restore`, `recovered_delete` | writer-only | Added 2026-10-16. Listed, as every command is, but an agent always gets `writer_only`: the writer's set-aside work is theirs to restore or delete (§6). |
| `revision_list`, `title_get`, `report_scenes`, `report_characters`, `report_locations` | query | Added 2026-10-22 (phase 5b). |
| `title_set`, `title_set_lines` | edit | Added 2026-10-22. Refused with `writer_only` while the script's agent tracking is on, so every agent edit stays reviewable (decided with the project's owner); allowed when it is off. |
| `revision_start`, `revision_end`, `revision_colors`, `revision_clear`, `changes_import` | writer-only | Added 2026-10-22. Listed, but an agent always gets `writer_only`: revision sets are the production's record, and an import's changes carry a writer's name. An agent's edits are revision-marked like anyone's while a set is active. |
| `diff` | file | Added 2026-10-22: compares two drafts (`docs/specs/diff.md`). `overwrite` for its PDF is refused, as for `export_pdf`. |

- **Input schemas.** Each tool's input schema is its command's params as JSON Schema, generated from the same Rust types the dispatcher deserializes, so they cannot drift apart. Unknown params are refused, as in the envelope.
- **Results.**
  - A successful call returns the command's result as `structuredContent`, and the same JSON as one text block.
  - A failed command returns `isError: true`, with the error object (`code`, `message`, `details`) as `structuredContent` and the text `<code>: <message>`. The agent can correct itself from that.
  - Malformed requests and unknown tools are protocol errors.
- **Order.** The tool list is always in the order above, so a client can cache it.
- **Descriptions.** Each tool's description is one or two sentences, in our own words, taken from `docs/specs/commands.md`.

## 4. Resources

With a document open:

| URI | MIME type | Content |
| --- | --- | --- |
| `coldopen://doc/fountain` | `text/plain` | The script as Fountain (`doc.export`, format `fountain`). |
| `coldopen://doc/blocks` | `application/json` | The title page and blocks with ids (`doc.get`). |
| `coldopen://doc/pages` | `text/plain` | The page view (`doc.export`, format `pages`). |
| `coldopen://doc/scenes` | `application/json` | The scene list (`scene.list`). |
| `coldopen://scene/{id}` (template) | `text/plain` | One scene, heading to end, as Fountain. |
| `coldopen://doc/changes` | `application/json` | (2026-10-18) The pending tracked changes (`changes.list`). |
| `coldopen://doc/revisions` | `application/json` | (2026-10-22) The revision sets (`revision.list`). |
| `coldopen://report/scenes`, `coldopen://report/characters`, `coldopen://report/locations` | `application/json` | (2026-10-22) The reports (`docs/specs/reports.md`). |

- **No document.** With no document open, the list is empty, and reading any of these fails with "resource not found" (JSON-RPC `-32602`).
- **Updates.** When the document changes, by any author including the writer in the app, the server reports every resource above as updated to clients that subscribed. Opening or closing a document changes the resource list, which the server announces. Clients on revision 2026-07-28 subscribe with `subscriptions/listen`; older clients with `resources/subscribe`. Both are supported.
- **Reports** (2026-10-22, phase 5b) are the three `coldopen://report/` resources above, updated with the others.

## 5. The agent link

### 5.1 Finding the app

When the desktop app has a script open and agent access is on (`docs/specs/editor.md` §12), it publishes a **session file** in its run folder (`docs/specs/editor.md` §11):

```json
{"v": 1, "pid": 4242, "app": "0.1.0", "endpoint": "\\\\.\\pipe\\coldopen-6b0c…", "token": "9f2e…", "docs": ["C:\\Scripts\\lighthouse.fountain"]}
```

| Field | Meaning |
| --- | --- |
| `pid` | The app's process id. The file is named `<pid>.json`. |
| `endpoint` | Where the app listens (§5.2). |
| `token` | 64 hex digits: 256 bits from the operating system's random source, new each time the app starts. |
| `docs` | The canonical paths of the scripts open in the app. |
| `doc_ids` | (2026-10-18) For each path in `docs`, in the same order, the document's `doc_id` when it is a bundle (`docs/specs/bundle.md` §8), or null. Absent in files from earlier versions; readers that do not know it ignore it. |

- **Writing.** The app writes the file atomically (a temporary file, then a rename), rewrites it whenever the open script changes, and removes it on exit.
- **Stale files.** At start-up the app deletes files whose process no longer runs.
- **On Unix,** the file is created with mode 0600.

`doc_open` with `attach: "auto"` (the default) canonicalizes its path and looks for a session file that lists it. If one is found and the handshake (§5.3) succeeds, the server is **attached**. Otherwise it opens the file itself, as before.

- **The app has the file open but attaching fails,** for example because agent access is off: the open fails with `doc_locked`, and its message explains why.
- **`attach: "never"`** skips the search, and fails with `doc_locked` if the app has the file open.

### 5.2 Endpoints

| OS | Endpoint | Protection |
| --- | --- | --- |
| Linux, macOS | A Unix domain socket, `<run folder>/<pid>.sock` | The run folder is created with mode 0700 and must be owned by the user. If it has any other owner or mode, the app does not host a link and says so. The socket gets mode 0600 where the OS allows a mode before the socket is bound (Linux); macOS does not, and there the 0700 folder and the peer check protect it. Each connection's peer user id (`SO_PEERCRED` on Linux, `getpeereid` on macOS) must equal the app's, or the connection is closed at once. |
| Windows | A named pipe, `\\.\pipe\coldopen-<32 random hex digits>` | It is created as the first instance of its name and rejects remote clients. Its access list grants access to the current user only. Other users can neither connect nor create another instance of the name. |

**Resolution:** a Unix socket path is limited to about 100 bytes. When the run folder makes it longer, the app uses `$TMPDIR/coldopen-<uid>/` instead, with the same 0700 ownership checks.

### 5.3 Handshake

The client's first line, sent within 5 seconds of connecting (a hello that arrives a moment after the connection is still read; until 2026-10-16 the app on Windows refused one that the client had not written by the server's first read):

```json
{"hello": {"v": 1, "token": "9f2e…", "client": {"name": "claude-code", "version": "2.1"}, "doc": "C:\\Scripts\\lighthouse.fountain"}}
```

The app's reply:

```json
{"hello": {"v": 1, "ok": true, "app": "0.1.0", "doc": {"path": "C:\\Scripts\\lighthouse.fountain", "rev": 214}}}
```

When the handshake fails, the reply carries `"ok": false` and an error instead, and the app closes the connection. The error codes are:
- `bad_token`;
- `wrong_doc` (that script is no longer open);
- `agent_access_off`;
- `too_many_links` (more than 4 at once);
- `bad_hello` (anything malformed, including a first line over 64 KiB).

Rules:
- **The token** is compared in constant time.
- **The client's name** comes from MCP's client information. It is cut to 64 characters, with control characters removed. An empty name becomes "agent". It is fixed for the life of the link: requests cannot change it.

### 5.4 Requests, responses and events

After the handshake, each line from the client is a version 1 command envelope (`docs/specs/commands.md`) with one extra optional member, `tool`: the MCP tool being run, at most 64 characters. Each reply is the envelope's response.

- **One at a time.** Requests run one at a time, in order, in the app's own event loop, between the writer's keystrokes. A command from an agent and a keystroke can never interleave halfway.
- **Authors.** Each command runs with the author `{"kind": "agent", "name": <from the handshake>, "tool": <tool>}`. It is journaled and labelled as `docs/specs/commands.md` describes.
- **Refused commands.** These fail with `not_allowed_attached`: `doc.new`, `doc.open`, `doc.close`, `doc.revert`, `doc.save` with a `path`, and (2026-10-16) `recovered.restore` and `recovered.delete`. An agent can't replace, close or rename the writer's document. Saving it in place is allowed, and so is exporting a copy to a new path. The app decides this in its command router (`docs/specs/editor.md` §16).
- **Leaving.** A client that is done sends `{"bye":true}`. The app then sends `closed` and closes the link from its side. (2026-10-15: a client can't close a pipe while its own reading thread still holds it, so it asks.)
- **Limits.** A line over 8 MiB, or a `batch` of more than 10,000 commands, is refused, and an over-long line closes the link.
- **Events.** The app also writes these lines to every link:
  - `{"event": "changed", "rev": n}`: after any change by any author, sent at most once every 50 ms;
  - `{"event": "closed"}`: when the writer closes the script, quits, turns agent access off or presses Disconnect. The app then closes the link.

Steps an agent made stay in the history after it disconnects.

### 5.5 In `coldopen mcp`

- **Forwarding.** While attached, every tool call except `doc_open` and `doc_close` becomes one request on the link. The response becomes the tool result.
- **Attaching.** `doc_open`'s result then reads `{path, attached: true, rev, scenes, warnings: [], recovered: null}`.
- **Detaching.** `doc_close` sends `{"bye":true}`, returns `{detached: true}`, and leaves the writer's document alone. `doc_open` of a different script detaches once the new script is open (2026-10-16); if it cannot be opened, the agent stays attached.
- **When the app closes the script.** If the link closes from the app's side, later tools fail with `no_document`, and the message explains that the app closed the script.
- **A link belongs to one script** (2026-10-16). A link is bound to the script it attached to. When the app's open script changes path, by opening another script, starting a new one or "Save as" under another name, every link is closed (agents get `closed`), and a request that still arrives for the old script closes its link instead of running. An agent never edits a script the writer did not attach it to.
- **Resource updates** come from the app's `changed` events.

## 6. Without the app

When the script isn't open in the app, `coldopen mcp` opens it with its own dispatcher, exactly as `coldopen serve --stdio` does:
- it takes the journal lock, so the app then can't open the file (`doc_locked`);
- it flushes every journal entry (`docs/specs/journal.md`, Every-entry mode);
- it keeps unsaved work in the journal when the client goes away;
- `doc_open` keeps the open script until the new one is open (2026-10-16). It is refused with `unsaved_changes` while the open script has unsaved changes, as `doc.open` is everywhere, and a script that cannot be opened (a missing file, say) leaves the open one exactly as it was.

Commands still run as the agent, so the undo labels and the `not_your_step` rule are the same as when attached.

**The writer's recovered work** (2026-10-16). After the app crashes, `coldopen mcp` is often the first to open the script, so it recovers the writer's journal. The agent may read, edit and save that work, or leave it in the journal, but it may never set it aside: while the session holds recovered work that includes any of the writer's entries and has not been saved, `doc_close` with `discard`, `doc_revert`, and `doc_open` with `recover: "discard"` all fail with `not_your_step` (`details.recovered` is true), and nothing changes. Each of these would keep a copy, but the writer would never be shown it. Work the agent recovered from its own entries may be discarded as usual.

**Choices that are the writer's** (2026-10-16). An agent may not pass `allow_lossy` to `doc_save`: what Fountain drops is the writer's call, which their own save asks about ("Save anyway?"). An agent that wants to save a script holding, say, a Shot changes that element itself, as an undoable step. Nor may an agent pass `overwrite` to `doc_save` or `export_pdf` (`writer_only`): replacing a file another program changed, or any file that is not the open document, is the writer's decision, and an agent would simply pass the flag. An agent saves or exports to a new path instead. Nor (2026-10-16) may an agent restore or delete a set-aside copy (`writer_only`), since the writer is the one shown those copies. §7's reasoning about same-user programs does not cover this: an MCP client is often a model following text it read, so the tools themselves are the boundary.

### 6.1 Agent edits are tracked changes

Added 2026-10-18 (phase 5a). Sources: `docs/spec.md` ("Agents edit in the open"), `docs/specs/changes.md`, the phase 5a brief and its decisions (2026-10-18).

- **By default.** Each document's `tracking.agents` switch is on unless the writer turns it off for that script (`changes.track`, `docs/specs/editor.md` §12). While it is on, every edit an agent makes, attached or not, lands as a tracked change under the agent's name: the client's name from the MCP handshake, or "agent" when it gives none (§5.3). This is decided in core, from the command's author, so the link and `coldopen mcp`'s own dispatcher behave the same, and no client can turn it off: an agent's `tracked: false` fails with `writer_only`.
- **Settling.** An agent can list changes, and reject its own (`changes_reject`), which withdraws its suggestion. It cannot accept any change, nor reject the writer's or another agent's (`writer_only`, `not_your_change`; `docs/specs/changes.md` §6). It cannot edit away another author's pending marks either (`not_your_change`, §5.4 there).
- **Undo** is unchanged: an agent may undo its own steps, which removes its marks with them.
- **Without the app** (§6). A .fountain or .fdx file cannot hold tracked changes, so an agent working on one alone cannot save its tracked edits there: the save is lossy, and accepting a loss is the writer's (`writer_only`). It can save the script as a `.coldopen` bundle at a new path, which keeps them, or leave them in the journal for the writer to recover in the app. **Resolution:** for people who use an agent on a Fountain file without the app, `coldopen mcp --untracked` records the agent's edits untracked in scripts the server opens itself, whatever the script's switch says. The person who configures the MCP server chooses this in their client's settings; an agent cannot change the command line it was started with. It is not document state: each edit's journal entry says whether it was tracked. Attached to the app, the app's switch decides.
- **Server instructions** add one line: edits are tracked changes the writer reviews, and `text` in blocks is the current text.

## 7. Security

| Threat | Answer |
| --- | --- |
| A remote machine | No network endpoint exists. Unix sockets are local by nature, and the Windows pipe rejects remote clients. |
| Another user on the same machine | They can't enter the 0700 run folder or open the 0600 socket, and the peer check refuses them anyway. On Windows the pipe's access list excludes them. Even with a connection they would still need the token, which is stored only in the user's own folder. |
| Another user pretending to be the app | On Unix only the user can create files in the run folder. On Windows the app creates the first instance of a random pipe name, and no one else may create another instance. |
| A stale session file after a crash, or a reused process id | Connecting fails or the token is wrong; the server then opens the file itself. |
| A program run by the same user | It can already read and change the user's files, so the link gives it nothing new. That's why agent access is on by default, with a visible indicator, one-click disconnect and an off switch (`docs/specs/editor.md` §12). |
| Flooding | At most 4 links, a 5-second handshake deadline, and the size limits of §5.4. The writer's own input is never queued behind more than one agent command at a time. A connect waits at most 2 seconds for a busy endpoint, so nothing hangs on a pipe whose instances are all in use. Enforced since 2026-10-16 (review findings F20 and C8): each link has at most one request waiting for the app, because its next line is read only once the app has answered, so a client that sends without waiting fills its own socket instead of the app's queue; `changed` events to a client that stops reading coalesce into one; and at most 8 connections may be in their handshake at once, so more are turned away at once with `too_many_links`. |

Tokens never appear in logs, errors or the journal.

## 8. Not yet

- Agent edits as tracked changes arrived in phase 5a (§6.1), and report resources in phase 5b (§4).
- Attaching to a script that is open in another `coldopen mcp` or `serve --stdio` process, rather than in the app.
- Any network transport.
