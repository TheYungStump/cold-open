# Crash journal and atomic saves

- **Date:** 2026-10-13, updated 2026-10-15 and 2026-10-16
- **Status:** implemented in `crates/core` (phase 1). The phase 3 changes (2026-10-15: authors and grouping in entries, the `doc.saved` marker, deferred flushing for the desktop app, saving off the UI thread, recovering never-saved documents) are being implemented.
- **Sources:** `docs/spec.md` (Reliability features), `CLAUDE.md` (Reliability), the phase 3 brief and its approved decisions (2026-10-15), the reviews of phases 1–3 (2026-10-15, listed in `docs/progress/review-fixes.md`). No external material.

**Promise:** a crash never costs more than the edits that had not yet been acknowledged. In the desktop app, an edit is acknowledged when it appears on screen. "Durability" below says exactly what each kind of crash can lose.

**The rule behind every section below** (2026-10-16): nothing that holds a writer's unsaved work, whether a journal or a script, is deleted, truncated or overwritten unless it is copied aside first, and nothing is written before the checks that could refuse it.

## Journal file

- **Location.** For `script.fountain`, the journal is `.script.fountain.coldopen-journal` in the same folder.
- **Format.** JSON Lines in UTF-8, each line ending in `\n`.
- **Header.** The first line is the header:

```json
{"journal":"coldopen","v":1,"app":"0.1.0","doc":{"title_page":{},"blocks":[{"id":"b_1","type":"action","text":"..."}],"next_id":2},"disk":{"len":1234,"fnv64":"af63bd4c8601b7df"}}
```

| Header field | Meaning |
| --- | --- |
| `doc` | A full snapshot of the base document, with ids and `next_id`. This makes the journal self-contained: recovery works even if the script file was changed, damaged or deleted. |
| `disk` | The length and FNV-1a 64-bit hash of the script file as last saved, or null if it has never been saved. It is used only to notice changes made by other programs. |
| `semantics` | (2026-10-16) What replaying the entries means, now 1. It goes up whenever a command's replay changes meaning, because results alone cannot show that ("What replay compares"). Absent means 1. A journal with a higher number than this version knows is treated like an unreadable header: kept aside, never replayed. |
| `host`, `path` | (2026-10-16) The computer's name and the script's canonical path when the header was written. Absent in older journals. |

**A journal from somewhere else** (2026-10-16). Writers keep scripts in git and in synced folders, and the journal can travel with the script. A journal whose `host` is not this computer, or whose `path` is not this script's canonical path, holds someone else's work, or work from another copy. `doc.open` never applies it on its own: where it would recover work, the open fails with `journal_foreign`, `details` giving `host` and `path`, and changes nothing. `recover: "apply"` recovers it anyway; `recover: "discard"` sets it aside, as always. A foreign journal with nothing to recover is simply started again. The desktop app asks the writer (`docs/specs/editor.md` §13).

- **Entries.** Each following line is one applied command:

```json
{"seq":1,"t":1760356801123,"command":"block.insert","params":{"type":"action","text":"Rain."},"result":{"id":"b_2"}}
```

| Entry field | Meaning |
| --- | --- |
| `seq` | Counts from 1. |
| `t` | Milliseconds since the Unix epoch. Informational only. |
| `command`, `params` | The command exactly as applied, with the defaults that depend on the document filled in (2026-10-16): the `style` a `text.insert` took from the text around it, and the `type` a `block.split` kept, inside a `batch` too. A later change to such a default therefore never changes what an old entry replays to. Static defaults, such as `text.replace`'s `match_case`, are always written already. |
| `result` | The command's result, including assigned ids, so replay can confirm it reached the same state. Replay compares only part of it ("What replay compares", below). |
| `by` | Optional (2026-10-15). The command's author, as in `docs/specs/commands.md` ("Authors and undo steps"). Absent means a writer. |
| `merge` | Optional (2026-10-15). `true` when the command joined the undo step before it. Absent means false. |
| `step` | `undo` and `redo` only, optional (2026-10-16). The step reversed: see "Undo and redo entries". |

Only edits, `undo`, `redo` and `doc.saved` markers are written. Queries, output commands and session commands are not. Replay stops at any other entry, so a damaged or hand-made journal can never make recovery write files.

**`doc.saved` markers** (2026-10-15). A marker records that a save of the document, as it stood at that point in the journal, was started:

```json
{"seq":9,"t":1760356802001,"command":"doc.saved","params":{"disk":{"len":1290,"fnv64":"03c1f2aa90b1e4d7"}},"result":{}}
```

Replay runs nothing for a marker. It only remembers the fingerprint, for `external_change` (recovery step 5). The marker is written before the file is replaced, so whether or not that save completed, the file on disk matches either the header's `disk` or a marker's.

**Undo and redo entries** (2026-10-16). Replay must reverse exactly the step the writer reversed, so each `undo` or `redo` entry says which step that was:

- `"step": 7`: the step made by entry 7 of this journal (a typing group is named by the entry that began it). Replay checks that the step on top of its rebuilt history is that one; if not, it stops there, as for any other divergence.
- `"step": {"tx": [...], "command": "typing", "label": "Typing", "by": {...}}`: a step made before this journal's header, so not in the history replay rebuilds. This happens after a save, which starts the journal again while the undo history keeps every earlier step. The entry carries the step: `tx` lists the operations the undo or redo applied (`{"op": "insert" | "remove" | "replace" | "move", ...}`, as in `crates/core/src/ops.rs`), and `command`, `label` and `by` describe it. Replay applies `tx` and puts the step on the other stack, so later entries can name it by this entry's `seq`.

A save also ends any typing group, so typing never joins a step across a journal restart. Entries without `step`, written before 2026-10-16, replay as before: the step on top is reversed, and its command name is compared.

**What replay compares** (2026-10-16). A replayed result must reproduce only what later entries depend on:

- every id the recorded result names as `id`, at the top level or in a batch's `results` (later entries address blocks by these ids);
- for `undo` and `redo`, the command name of the step reversed (`undone`, `redone`), and the step itself where the entry names it.

Nothing else is compared. A field the recorded result lacks is never asked for, so a newer version may add result fields (as phase 3 added `label` to undo results) without stranding older journals, and labels may be reworded or translated. Because only ids and undo targets are checked, a change to what a command *does* must not hide behind an unchanged result: it needs a new `semantics` number (see "Header"). `tests/fixtures/journal/` holds journals written by the builds of phases 1 to 3, and `crates/core/tests/old_journals.rs` checks that each still recovers in full.

**Old readers.** The new fields and markers are additive, so the journal stays version 1. A Cold Open from before phase 3 that meets a phase 3 journal stops replay at the first command or marker it does not know, keeps a copy of the journal (step 2 below), and recovers everything before that point.

## Writing

1. An edit is validated and applied in memory.
2. Its entry is written to the journal in one write call, on the thread that applied the edit.
3. Only then is the result returned and, in the desktop app, shown on screen. If the write fails, the edit is rolled back and the request fails with `journal_error`.

**A failed write leaves nothing behind** (2026-10-16). A write can stop part-way, as on a full disk, or the flush after a whole line can fail. Either way the edit is rolled back, so its entry must go too: the journal is cut back to the end of its last whole line, and the next entry follows that line. If the cut fails as well, the journal is marked broken. Every later edit then fails with `journal_error`, saying that a save starts the journal again, and the first save that writes a new header does. A save off the UI thread writes no `doc.saved` marker into a broken journal, since it protects nothing. A save whose new header cannot be written leaves the journal broken in the same way.

While a session is open, it holds an exclusive lock on the journal (`File::lock`). A second process gets `doc_locked`. The operating system releases the lock if the process dies.

## Durability

A write call puts the entry in the operating system's cache, which survives the app crashing or being killed. To survive the operating system crashing or a power cut, the entry must also be flushed to the disk (`fsync`). Front ends flush in one of two ways:

| Mode | Used by | Rule |
| --- | --- | --- |
| **Every entry** | The CLI, `serve --stdio`, `coldopen mcp` when it opens the file itself | Each entry is flushed before its result is returned. |
| **Deferred** | The desktop app | A background thread flushes. No entry stays unflushed for more than **250 ms** after it is written. The thread also flushes at once when a save starts or finishes, when the document closes, and when typing goes idle, meaning 100 ms with no new entry. |

**One file, two handles** (2026-10-16, from Grok's review). The flusher thread flushes a second handle to the journal (a `try_clone` of the one entries are written through). This relies on a flush of any handle reaching the disk with everything written to that file, which holds wherever both handles share one open file in the operating system: Windows, macOS and Linux on local disks. A network file system that buffered writes per handle could keep an entry from the disk longer than 250 ms. No such system is supported for scripts today; if one is, the flusher should own the writing handle instead.

The 250 ms limit is hard in the sense that the app never waits longer before asking the disk to flush. A flush that the disk itself takes longer to finish is reported, never hidden: the status bar shows the journal as protected only when every written entry has been flushed.

**What a crash can lose in the desktop app (Deferred mode):**

- **The app crashes, panics or is killed:** nothing that was on screen. Every change shown had already been written to the operating system. Only keystrokes still waiting in the operating system's input queue, never processed and never shown, are lost.
- **The operating system crashes or the power fails:** at most the entries written in the last 250 ms, plus those written during the flush that was in progress.
- **The journal cannot be written** (a full disk, a removed drive): the edit is refused and is not shown, and the app says why. Nothing is lost silently.
- **Storage that ignores flush requests**, such as some USB sticks and network shares: nothing beyond what the operating system's cache keeps.

In Every-entry mode, an operating-system crash loses nothing that was acknowledged.

## Saving

0. **Another program's changes** (2026-10-16). A session remembers the fingerprint of its file as it last read or wrote it (at open, revert and every save it made). A save in place first reads the file. If it is not that version, the save fails with `file_changed` and writes nothing, unless `overwrite` is true. A file that has gone is simply written again. Changes made outside while the file is open (a `git pull`, a sync client, another editor) are therefore never replaced without the writer's say. After a recovery, the fingerprint is the file's as it was opened: the recovery report has already said whether another program changed it (`external_change`).
1. Saving writes the script atomically (see below).
2. Then the journal is truncated in place, and a new header is written with `doc` set to the saved document and `disk` set to the new file.

The two possible crash points are both safe:

- **Between steps 1 and 2:** the old journal remains. It replays to exactly the saved state, which step 4 of recovery recognizes.
- **During step 2:** the header may be torn. That means "nothing to recover", which is correct, because the saved file is current.

**Saving off the UI thread** (desktop app, 2026-10-15). Writing and flushing a 120-page script can take tens of milliseconds, so the app saves in three steps, and the writer can keep typing throughout:

1. **Prepare**, on the UI thread: write the document as Fountain, check for losses, and append a `doc.saved` marker with the fingerprint of the bytes about to be written.
2. **Write**, on a worker thread: the atomic save below.
3. **Commit**, back on the UI thread. If the write succeeded and nothing was edited since step 1, the journal is restarted exactly as in step 2 above. If edits arrived in the meantime, the journal is kept as it is: it already holds those edits after the marker, and the document stays unsaved. If the write failed, the error is shown, and the marker is harmless.

Every crash point is safe. The journal always holds the last saved base plus every later edit, so replay reaches the latest acknowledged state. The file is either the old version or the new one, each named by the header or a marker. "Save as" to a new path is done in one step, as before, because a file dialog has just paused the writer anyway.

**One save at a time** (2026-10-16). If a second save could run while a prepared one is being written, the older write could land last, and the commit of either could mark newer edits as saved. So:

- From prepare to commit, the save holds the session's only save token, numbered among that session's saves. Every other save of that session, `doc.save` (in place or "save as") or another prepare, fails with `save_in_progress` and changes nothing. The error is retryable. The app's own Mod+S during a save waits and saves again after it; an agent's `doc_save` gets the error and tries again.
- The commit ends the save, whatever the outcome, so a failed write never leaves the token held.
- A commit applies only to the session it was prepared for, and only for the token it holds. If that session was closed, reopened or reverted meanwhile, the commit changes nothing and reports `save_superseded`: the file was written, with the version saved then. The session open now keeps its own unsaved state.

**"Save as" to another path** (2026-10-16). The target may be open in another Cold Open process, or its last session may have crashed and left unsaved work in its journal. So before anything is written:

1. The target's journal is locked. If another process holds it, the save fails with `doc_locked`.
2. Whatever the target's journal holds is judged exactly as opening the target would judge it ("Recovery on open"). Where the open would recover work, the save fails with `unsaved_changes`, with `details.path` naming the target. Where the open would copy the journal aside (unreadable, or replay stopped early), the save copies it aside too.
3. Only then is the file written, the target's journal started from the saved document, and the old path's journal deleted.

A refused save writes nothing: the target's file and journal are exactly as they were.

## Closing

- A clean close with no unsaved changes deletes the journal.
- Closing with unsaved changes (for example, the stdio server reaching end of input) keeps the journal, so the work is recovered the next time the file is opened.
- `doc.close` with `discard: true` throws the unsaved changes away, but keeps them (2026-10-16): when the session has any, the journal is first copied to `<journal>.discarded-<ms>`, then deleted, and a `journal_discarded` warning names the copy. If the copy cannot be made, the close fails with `io_error` and nothing changes. A clean session has nothing to keep and leaves no copy.
- An agent may not discard a writer's recovered work (`docs/specs/mcp.md` §6).
- `coldopen run` without `--save` (2026-10-16). A batch can be rerun from its file, so its own unsaved changes are deleted with no copy. Recovered work is never the batch's to delete: if the batch left it untouched, the journal stays as it is, for the next open; if the batch changed it, the journal is copied aside as for `discard`, and `run` names the copy.

## Recovery on open

When `doc.open` finds a journal for the file:

1. **Lock the journal.** If the header cannot be read, copy the journal to `<journal>.corrupt-<ms>` (never delete it), warn, and start a fresh journal from the file. A header that reads but holds a damaged document is treated the same way (2026-10-16): two blocks with one id, an id not below `next_id`, a `next_id` that leaves no id to hand out, or a block that breaks a model rule. Ids are handed out with an overflow check, so even a document that slips past cannot panic.
2. **Replay.** Start from `doc` and replay the entries in order.
   - A final line with no `\n` is a write torn by the crash. It is ignored and cut off; it was never acknowledged.
   - A complete line that does not parse, or a `seq` out of order, stops replay at that point. The journal is copied aside, and a warning names how many entries were recovered.
   - If a replayed result does not reproduce the recorded one ("What replay compares"), replay stops at that entry, the journal is copied aside to `<journal>.diverged-<ms>`, and a warning is returned. The diverged entry's change is not kept (2026-10-16): the recovered document is exactly what the trusted entries before it give, which is what the journal is cut back to, so a later crash recovers the same document again.
3. **Discard if asked.** With `recover: "discard"`, the journal is copied to `<journal>.discarded-<ms>`, and a fresh journal is started from the file.
4. **Nothing to recover?** If the replayed document writes as Fountain **without loss** and equals the file's bytes, open the file normally. A lossy write can produce the file's exact text while the document differs (for example, a scene heading retyped as a stray parenthetical), so a lossy write always recovers.
   - **No entries** (2026-10-16). Nothing is to recover when the file is the version the header's `disk` or a marker names, or holds the header's document as above. Otherwise the header is the only copy of the last save: the save's rename never reached the disk (a power cut), or another program changed the file and then Cold Open crashed. The header's document is then recovered, with `commands: 0` and `external_change: true`, and the writer decides. Before 2026-10-16, such a journal was overwritten without a word.
5. **Otherwise, recover.** Open the replayed document as unsaved, with its undo history rebuilt, and report `recovered: {commands, external_change, last}`, where `last` is the newest replayed entry's `t`.
   - **Undo history.** It is rebuilt with each step's author and label. Entries marked `merge` join the step before them, exactly as they did when they were made.
   - `external_change` is true when the file matches neither `disk` nor the fingerprint of any `doc.saved` marker in the journal. In that case the file is never overwritten automatically; the writer decides.
   - `commands` counts the commands replayed; markers are not counted. It is 0 when only the header was recovered.

**A file that does not exist** (2026-10-15). If the script file is missing but a journal for it exists, as for a document created with `doc.new` that was never saved, the journal alone is recovered:
- the file's bytes count as empty in step 4;
- `external_change` is false when the header's `disk` is null, and true otherwise, because a file that was once saved is now gone.

Without a journal, a missing file fails with `io_error` as before.

Because such a journal is the only copy of the work, `doc.new` never starts over on top of it (2026-10-16): `doc.new` at a path whose journal holds anything fails with `already_exists` and leaves the journal untouched.

## Atomic save

Every save, on every OS, follows these steps:

1. If the target is a symbolic link, write to the file it points to, so the link survives.
2. Create `.<name>.coldopen-save-<pid>-<n>.tmp` in the target's folder, failing if it already exists.
3. Write all bytes and `sync_all()` them. On macOS this uses `F_FULLFSYNC`, which reaches the disk.
4. On Unix, copy the original file's permission bits to the temporary file.
5. Rename the temporary file over the target:
   - **Linux and macOS:** `rename(2)` replaces the target atomically. Then `fsync` the folder so the rename is durable.
   - **Windows:** `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH`. Antivirus, search indexers and sync clients often hold files open briefly, so access-denied, sharing and lock violations are retried with backoff for about a second.
6. On any error, delete the temporary file and report the error. The original file is never left half-written.

## Encoding

- **Reading.** A UTF-8 byte-order mark is removed. Bytes that are not valid UTF-8 are decoded as Windows-1252, with a warning, rather than replaced.
- **Writing.** The writer produces UTF-8 with LF line endings. `doc.save` restores the byte-order mark and CRLF line endings if the opened file mostly used them. This depends only on the input file, never on the operating system.
