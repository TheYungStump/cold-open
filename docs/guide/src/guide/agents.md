# AI agents (MCP)

Cold Open comes with an MCP server, `coldopen mcp`, so an AI agent can read and edit your script. MCP (the Model Context Protocol) is the standard way AI assistants such as Claude use tools.

Connecting an agent is free, and so is the server. Cold Open itself never sends your script anywhere. **The agent you connect is a separate program:** what it reads from your script, and where it sends it, is up to that program and its own privacy policy.

## Setting it up

The server is `coldopen mcp`, which talks over standard input and output. Any MCP client that runs a local ("stdio") server can use it.

**Claude Code:**

```
claude mcp add coldopen -- coldopen mcp
```

**Claude Desktop:** add this to its `claude_desktop_config.json` (in Claude Desktop: Settings ▸ Developer ▸ Edit Config), then restart Claude Desktop:

```json
{
  "mcpServers": {
    "coldopen": { "command": "coldopen", "args": ["mcp"] }
  }
}
```

A desktop app may not see your terminal's PATH, so give the full path to `coldopen` if it isn't found:

- **Windows:** `C:\\Users\\<you>\\AppData\\Local\\Programs\\Cold Open\\bin\\coldopen.exe` (with the backslashes doubled, as JSON needs).
- **macOS:** `/Applications/Cold Open.app/Contents/MacOS/coldopen`.
- **Linux:** `/usr/bin/coldopen` from the .deb. For the Flatpak, use `"command": "flatpak"` with `"args": ["run", "--command=coldopen", "io.github.theyungstump.coldopen", "mcp"]`.

**Other clients** take the same two things: the command `coldopen` and the argument `mcp`.

## How it works with the app

When the script is open in Cold Open, the agent's `doc_open` **attaches to the app** instead of opening the file again. Then:

- you see each edit appear as it's made;
- the agent's edits arrive as **tracked changes under its name**, for you to accept or reject ([Track changes](track-changes.md#agents-edits));
- Mod+Z undoes an agent's step like your own, and **Undo agent edits** (Mod+Alt+Z) undoes all its steps on top;
- the status bar shows a connected agent, with **Disconnect**, and **Turn agent access on or off** in the palette stops any agent attaching.

The link between them stays on your computer: a socket or pipe that only your user account can open, and a fresh secret each time Cold Open starts. There's no network connection.

Without the app, `coldopen mcp` opens the file itself, with the same crash journal.

## What an agent can and can't do

**It can:** read the script (as Fountain, as elements with ids, as pages, or scene by scene), list scenes, read the reports, find text, insert, change, delete and move elements and scenes, run a batch of edits as one undo step, list tracked changes, withdraw its own, compare drafts, and save the script in place.

**It can't**, because these are yours:

- accept any change, or reject someone else's;
- turn tracking on or off;
- start or end revision sets, or bring in a draft as changes;
- edit the title page while its edits are being tracked;
- accept a loss when saving (such as saving tracked changes into Fountain);
- restore or delete Recovered versions;
- close, replace or rename the script open in the app.

The [command reference](../reference/commands.md) lists every tool and its parameters.

## Untracked edits: `--untracked`

On its own, without the app, an agent's tracked edits can't be saved into a Fountain or .fdx file, which can't hold tracked changes. It can save a project instead, or leave them in the journal for you to review in the app.

If you'd rather the agent's edits went straight in, start the server with:

```
coldopen mcp --untracked
```

Then the agent's edits are recorded untracked in scripts the server opens itself. In the app, **Track agent edits** in the palette does the same for the script you're in.

Other options: `--doc PATH` opens (or attaches to) a script before the agent starts, and `--no-attach` always opens the file in the server, never in the app.
