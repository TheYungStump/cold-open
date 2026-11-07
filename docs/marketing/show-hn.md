# Show HN (draft, for when v0.1.0-alpha.3 is out)

For the project owner to post at news.ycombinator.com/submit, from their own account. Nothing has been posted. HN's guidelines for Show HN: it must be something people can try, the title starts "Show HN:", and the post should say what's new and why it's interesting, with no marketing language.

## Title (≤80 characters)

Show HN: Cold Open – a screenwriting app in Rust that journals every keystroke

Alternatives:
- Show HN: A screenplay editor where every action is a command (GUI, CLI, JSON, MCP)
- Show HN: Cold Open, a free screenwriting app that round-trips Final Draft .fdx byte for byte

## URL

https://github.com/TheYungStump/cold-open

(The repository rather than the site: HN readers want the code first. The site is linked from the README.)

## Text

I've been building Cold Open, a free, open-source screenwriting app (GPL; the format and layout libraries are MIT/Apache), for Windows, macOS and Linux. It's an alpha. Some of the technical choices:

- **Every action is a command.** The desktop app (iced), a CLI, newline-delimited JSON over stdio and an MCP server all send the same serde-serialized commands to one dispatcher. Undo, the crash journal and track changes all live there, once.
- **A command journal instead of autosave.** Each edit is appended to a journal before the UI shows it (about 0.01 ms). After a crash, the app replays the journal on top of the last save. Saves are atomic (temp file, fsync, rename), and a save's work on the UI thread is under 4 ms on a 120-page script; the rest happens on a worker. A test kills the app at every step of a save, with typing in flight, and checks no acknowledged key is lost.
- **Deterministic pagination.** Its own character-grid layout with bundled Courier Prime, so a script has the same page breaks on every OS; CI checks layout goldens on all three. Against three Final Draft PDFs, it matches 97–100% of page breaks.
- **.fdx round trips byte for byte.** Unknown elements are kept untouched, and a damaged file opens with a repair report (`coldopen repair`). It's a clean-room implementation from public sample files and our own specs: no one on the project has used Final Draft.
- **Agents edit as tracked changes.** `coldopen mcp` lets an agent attach to the script open in the app; its edits arrive under its name, and only the writer can accept them.
- **Small:** every installer is under 15 MB.

Much of the code was written with AI coding assistants under my direction, and I reviewed every change; the specs and progress records for each phase are in docs/.

I'd love feedback from anyone who writes scripts, and from anyone with opinions on the journal and save design.

## Before posting

- Check the numbers against the release you link (`docs/progress/phase-6c.md`, the bench files).
- Be around for a few hours after posting to answer questions.
- If the AI-assisted line draws the thread off topic, answer once, plainly, and move on.
