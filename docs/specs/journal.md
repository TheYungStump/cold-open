# Crash journal and atomic saves

- **Date:** 2026-10-13
- **Status:** implemented in `crates/core` (phase 1)
- **Sources:** `docs/spec.md` (Reliability features), `CLAUDE.md` (Reliability). No external material.

**Promise:** a crash never costs more than the edits that had not yet been acknowledged.

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

- **Entries.** Each following line is one applied command:

```json
{"seq":1,"t":1760356801123,"command":"block.insert","params":{"type":"action","text":"Rain."},"result":{"id":"b_2"}}
```

| Entry field | Meaning |
| --- | --- |
| `seq` | Counts from 1. |
| `t` | Milliseconds since the Unix epoch. Informational only. |
| `command`, `params` | The command exactly as applied. |
| `result` | The command's result, including assigned ids, so replay can confirm it reached the same state. |

Only edits, `undo` and `redo` are written. Queries, output commands and session commands are not. Replay stops at any entry that is not an edit, `undo` or `redo`, so a damaged or hand-made journal can never make recovery write files.

## Writing

1. An edit is validated and applied in memory.
2. Its entry is written to the journal in one write call. The CLI and stdio server also fsync it. The desktop app will fsync in the background at least once a second.
3. Only then is the result returned. If the write fails, the edit is rolled back and the request fails with `journal_error`.

While a session is open, it holds an exclusive lock on the journal (`File::lock`). A second process gets `doc_locked`. The operating system releases the lock if the process dies.

## Saving

1. Saving writes the script atomically (see below).
2. Then the journal is truncated in place, and a new header is written with `doc` set to the saved document and `disk` set to the new file.

The two possible crash points are both safe:

- **Between steps 1 and 2:** the old journal remains. It replays to exactly the saved state, which step 4 of recovery recognizes.
- **During step 2:** the header may be torn. That means "nothing to recover", which is correct, because the saved file is current.

## Closing

- A clean close with no unsaved changes deletes the journal.
- Closing with unsaved changes (for example, the stdio server reaching end of input) keeps the journal, so the work is recovered the next time the file is opened.
- `doc.close` with `discard: true` deletes the journal.

## Recovery on open

When `doc.open` finds a journal for the file:

1. **Lock the journal.** If the header cannot be read, copy the journal to `<journal>.corrupt-<ms>` (never delete it), warn, and start a fresh journal from the file.
2. **Replay.** Start from `doc` and replay the entries in order.
   - A final line with no `\n` is a write torn by the crash. It is ignored and cut off; it was never acknowledged.
   - A complete line that does not parse, or a `seq` out of order, stops replay at that point. The journal is copied aside, and a warning names how many entries were recovered.
   - If a replayed result differs from the recorded one, replay stops at that entry, the journal is copied aside to `<journal>.diverged-<ms>`, and a warning is returned.
3. **Discard if asked.** With `recover: "discard"`, the journal is copied to `<journal>.discarded-<ms>`, and a fresh journal is started from the file.
4. **Nothing to recover?** If there were no entries, or the replayed document writes as Fountain **without loss** and equals the file's bytes, open the file normally. A lossy write can produce the file's exact text while the document differs (for example, a scene heading retyped as a stray parenthetical), so a lossy write always recovers.
5. **Otherwise, recover.** Open the replayed document as unsaved, with its undo history rebuilt, and report `recovered: {commands, external_change}`.
   - `external_change` is true when the file no longer matches `disk`. In that case the file is never overwritten automatically; the writer decides.

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
