# Crash journal and atomic saves

- **Date:** 2026-10-13, updated 2026-10-15 and 2026-10-16
- **Status:** implemented in `crates/core` (phases 1 and 3, and the review fixes of 2026-10-16). The phase 4 changes (2026-10-16: the header's format, the document's .fdx parts, damaged originals, hidden files on Windows, Recovered versions and their clean-up, and the decisions for phase 5 at the end) are being implemented.
- **Sources:** `docs/spec.md` (Reliability features, File formats), `CLAUDE.md` (Reliability), the phase 3 brief and its approved decisions (2026-10-15), the reviews of phases 1–3 (2026-10-15, listed in `docs/progress/review-fixes.md`; section "F18" there for the decisions at the end), the phase 4 brief and its decisions (2026-10-16, `docs/progress/phase-4.md`). No external material.

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
| `format` | (2026-10-16, phase 4) The file format `doc` was read from or saved as, such as `"fdx"`. Recovery writes the replayed document in this format when it checks whether the file already holds it (step 4), so it compares like with like. Absent means Fountain, and Fountain is never written, so Fountain journals are exactly as before. A format this version does not know makes the header unreadable: the journal is kept aside (step 1), never replayed as the wrong format. |

**The document in the header** (2026-10-16, phase 4). `doc` is the whole document, so it carries what an .fdx file gave it besides the blocks (`docs/specs/fdx.md` §12, §13):

| `doc` field | Meaning |
| --- | --- |
| `page_format` | The file's page and element settings. Absent means the default format, as for every Fountain script. |
| `title_lines` | The title page as positioned lines. Absent means none. |
| `spacing` | Paragraph spacing overrides in points, by block id (`{"b_12": 1200}` is 12 pt). Absent means none. |
| `fdx` | The .fdx passthrough store: everything in the file that the blocks do not hold, kept to write the file back. Absent for a document not read from .fdx. |
| `unsaved` | `true` when the header's document is itself unsaved work, as for a restored copy ("Recovered versions"). Absent means false. |

All five are left out when absent, so a Fountain journal is byte for byte what it was. Builds from before phase 4 cannot read a header that holds them: they keep such a journal aside as unreadable (recovery step 1), and never replay or lose it.

The header holds one copy of the store, written at each save, as it already held one copy of the blocks. The store holds only the kept parts of the file (`docs/specs/fdx.md` §13.2): about a tenth of Big Fish's paragraphs, and its settings. `crates/fdx/tests/roundtrip.rs` reports its size for every corpus file.

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

1. **Prepare**, on the UI thread: write the document in its format (Fountain, or the format it was opened in; 2026-10-16), check for losses, and append a `doc.saved` marker with the fingerprint of the bytes about to be written.
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
4. **Nothing to recover?** If the replayed document writes in the header's format (`format`, Fountain when absent) **without loss** and equals the file's bytes, open the file normally. A lossy write can produce the file's exact text while the document differs (for example, a scene heading retyped as a stray parenthetical), so a lossy write always recovers.
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

## Damaged originals

Added 2026-10-16 (phase 4). An .fdx file that the reader had to repair (`docs/specs/fdx.md` §9, §17) may hold something the repair could not keep. So the first save in place of a document opened with a `repaired` report does this before anything else is written:

1. The file's bytes as they are now are copied to `.<name>.coldopen-original-<ms>` in the same folder, written and flushed like a save's temporary file.
2. Only then does the save go ahead as usual. If the copy cannot be made, the save fails with `io_error` and writes nothing.
3. The save's result carries an `original_kept` warning naming the copy. Later saves make no further copy.

The copy is a set-aside copy like the others ("Recovered versions"): hidden on Windows, listed, restorable and cleaned up by the same rules. "Save as" to another path writes nothing over the damaged file, so it keeps no copy.

## Hidden files

Added 2026-10-16 (phase 4). Journals and set-aside copies start with a dot, which hides them on Linux and macOS. On Windows, which ignores the dot, they are also given the hidden attribute, so writers never see them in their script folders:

- Journals, copies kept aside (`.discarded`, `.corrupt`, `.diverged`) and damaged originals are created hidden.
- A journal from before phase 4 is made hidden when it is next opened.
- A save's temporary file is not hidden, because it is renamed onto the script, and the script must stay visible.
- On Windows, creating a file over an existing hidden file fails, so Cold Open never does that: a journal is only ever cut back with a truncation, and every copy has a new name.

## Recovered versions

Added 2026-10-16 (phase 4). Set-aside copies hold work a writer may want back, so they are never deleted silently, and writers never need to know where they are.

**What is listed.** For a script, every file in its folder named after its journal plus `.discarded-<ms>`, `.corrupt-<ms>` or `.diverged-<ms>`, and every damaged original, `.<name>.coldopen-original-<ms>`. The time is the `<ms>` in the name, so copying a folder keeps it. `recovered.list` (`docs/specs/commands.md`) lists them, newest first. The desktop app shows them for the open script, the recent scripts and the untitled scripts in `Unsaved/` (`docs/specs/editor.md`).

**Preview.** A journal copy is replayed exactly as recovery would replay it (steps 1 and 2 of "Recovery on open"), without touching the disk, and the document it reaches is shown. A damaged original is read as its script would be. A copy whose header cannot be read is listed as unreadable: it can be deleted, not restored.

**Restore.** `recovered.restore` opens the copy's document as a new, unsaved script journaled at a path the caller gives (the app uses `Unsaved/`). The new journal's header holds that document with `unsaved: true` and no `disk`, so:

- a crash before the first save recovers it, as a never-saved document is recovered ("A file that does not exist");
- the app's start-up scan offers it as leftover work, as it does a journal with entries;
- a save as anything writes a new file. Nothing is ever written over the copy, the script it came from, or that script's journal.

**Notices.** When a command sets work aside, its warning (`journal_discarded`, `journal_unreadable`, `journal_partial`, `original_kept`) names the copy in its message for the CLI and JSON clients, and in `details.copy`. The desktop app never shows that path. It says "Your unsaved changes were set aside. Open Recovered versions to restore them.", with a button that opens the list.

**Clean-up.** At start-up the desktop app deletes a set-aside copy only when both of these hold:

- it was made more than 30 days ago, by the time in its name;
- the writer has been shown it in the Recovered versions list at least once.

The app records when it first showed each copy, in `recovered.json` in its data folder. A copy that has never been shown is never deleted, however old it is. A row that the next start would delete says so. The CLI and the MCP server never delete copies.

## Decisions for phase 5

Written 2026-10-16, before phase 5, as the review of phases 1–3 asked (`docs/progress/review-fixes.md`, F18.2 to F18.4).

**Ids that survive a reload, and the bundle (F18.3).**

- **Phase 5 brings the project bundle** (`docs/spec.md`, File formats). It is a `.coldopen` zip holding:
  - `document.json`: the model with its block ids, `next_id`, page format, title lines, spacing and any .fdx passthrough store;
  - `script.fountain`: a readable copy for git diffs and other tools, rewritten at each save and never read back while `document.json` is there;
  - `meta.json`: the format version, the app version and a `doc_id`;
  - phase 5's tracked changes and revision sets, as JSON.
- **Ids survive** because `document.json` stores them. A `.fountain` or `.fdx` file still numbers its blocks from `b_1` when opened. Matching ids by diffing the old blocks against the new ones is not needed: script diff (phase 5) matches scenes by their content, and tracked changes live in bundles.
- **The journal stays a separate file** next to the bundle (`.<name>.coldopen.coldopen-journal`). A zip cannot take one more entry and an fsync per keystroke, and the journal works as it does today.
- **Its header gains `doc_id`.** It also names the bundle's change data by fingerprint rather than copying it, so the header stays as small as it is now (Grok's note in the review).
- No bundle code is written in phase 4.

**A document's identity apart from its path (F18.4).** Phase 4 does not need one:

- "Save as" to .fdx changes the path, as any save as does. The journal moves with the save, and agents' links close on the path change, as today.
- Attaching (`docs/specs/mcp.md` §5) keys on the canonical path, which works for .fdx unchanged.
- **The bundle brings `doc_id`** (a random UUID made when the bundle is created). It is mirrored in the journal header and in the app's session file, next to the path, which stays an attribute.

**Track changes (F18.2).** Not implemented in phase 4, and nothing in phase 4 makes it harder:

- Phase 4 changes neither `RichText` nor what any command does when replayed, so `semantics` stays 1.
- The .fdx store is keyed by block id, outside the elements, so marking runs with change ids later needs no change to it.
- The review's rules still hold for phase 5:
  - change metadata goes inside the text runs;
  - agent edits become tracked changes only through an explicit, journaled param and a new `semantics` number.

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
- **UTF-16** (2026-10-16, review finding F6). Windows PowerShell 5.1's `>` and Notepad's old "Unicode" option write UTF-16. It is recognized by its byte-order mark (`FF FE` little-endian, `FE FF` big-endian) or, without one, by a zero byte next to the first character, which UTF-8 text never has. It is decoded as UTF-16, with a `utf16` warning, and saved as UTF-16 again, in the same byte order and with the mark if it had one. Sequences that are not UTF-16 become U+FFFD, with a `utf16_damaged` warning.
- **Writing.** The writer produces UTF-8 with LF line endings. `doc.save` restores the byte-order mark and CRLF line endings if the opened file mostly used them, and UTF-16 if it was UTF-16. This depends only on the input file, never on the operating system.
- **A Windows-1252 file** (2026-10-16) is saved as UTF-8, which programs that still read it as Windows-1252 show wrongly. So that save reports a `reencoded` loss and, like any lossy save, fails with `lossy_format` unless `allow_lossy` is given (the app asks "Save anyway"). Once saved, the file is UTF-8 and later saves ask nothing.
