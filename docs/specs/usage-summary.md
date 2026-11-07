# The local usage summary

- **Date:** 2026-11-05
- **Status:** phase 6c, being implemented.
- **Sources:**
  - The phase 6c brief, item 6, and the project owner's decisions of 2026-11-05 (`docs/progress/phase-6c.md`): counting is on by default and disclosed up front, in the privacy statement and in Settings and About.
  - `docs/specs/editor.md` §23 (Report a problem, the form, the clipboard) and §24 (Settings), `docs/specs/crash-reports.md` (what a report may hold), `docs/specs/commands.md` (the command names), `crates/app/src/actions.rs` (the app's action table).
  - The privacy statement in `README.md` and `docs/guide/src/privacy.md`, and SignPath Foundation's terms (signpath.org/terms, read 2026-11-02): signed software may not send information to other systems unless the user asks it to.
- A choice where the sources leave room is marked **Resolution**.

## 1. Goals

1. **Counts only, kept on this computer.** The summary says which features were used and how often, so feedback can say "I use X a lot" without anyone guessing.
2. **Nothing that identifies a person, a script or a file.** No text, no names, no paths, no times finer than a day (§3).
3. **The writer is in control.** They can read every count in plain words, reset it, and turn counting off (§6, §7).
4. **Cold Open never sends it.** It leaves the computer only when the writer ticks "Include my usage summary" in Share feedback… or Report a problem…, reads it in full, and then submits it themselves in their browser or pastes it (§8).
5. **Cheap.** Counting costs an integer increment. Writing the file never happens on the UI thread (§5).

## 2. What it records

Every key comes from a fixed table compiled into the app, and every value is a whole number, a version string from the build, or a UTC date. Nothing the writer types can become a key or a value.

| Field | What it holds |
| --- | --- |
| `version` | The file's format, `1`. |
| `since` | The UTC date counting started, as `YYYY-MM-DD`. |
| `app_versions` | The versions of Cold Open that counted, such as `0.1.0-alpha.3`, without the commit. At most 20, oldest dropped first. |
| `sessions` | How many times the app was started while counting was on. |
| `session_lengths` | Finished sessions by length, in four buckets: `under_5_min`, `5_to_30_min`, `30_min_to_2_h`, `over_2_h`. |
| `commands` | The writer's requests by name, with a count each: core commands from `Command::NAMES` (`text.insert`, `doc.save`, `changes.accept`…), and app actions from the action table (`app.palette`, `view.zoom_in`, `edit.find`…). A request named with an index (`recent:3`) counts under its prefix (`recent`). |
| `agent_commands` | Core commands an agent ran through the agent link, by name, with a count each. Not the agent's name. |
| `opened` | Scripts opened, by format: `fountain` (a `.txt` opens as Fountain), `fdx`, `project`. |
| `saved` | Saves of the script, by format, the same three. Exports count as commands (`export.pdf`, `export.fdx`, `export.fountain`). |
| `renderer` | Sessions by what drew the window: `cpu`, `gpu`, `cpu_fallback` (the GPU renderer couldn't start), or `not_asked` (the system wasn't asked this session, §2.1). |

### 2.1 The renderer

- **Resolution:** the app asks the system which renderer drew the window only when About, Report a problem or the bench needs it (phase 6a, decision 17), since the question costs frames. The summary doesn't ask either. A session counts as `cpu` or `gpu` when something else asked, and as `not_asked` otherwise. The GPU's model, which the system's answer includes, is never kept.

### 2.2 What counts

- **A command counts when the dispatcher runs it,** whether it succeeds or not, at the place that already notes the last action for crash reports (`crash::note_command`). The app's own actions count where the router notes them (`actions::note`).
- **Typing counts once per command,** as the dispatcher sees it: a run of typed letters grouped into one `text.insert` counts once per command sent, not per letter.
- **Only the desktop app counts.** The CLI, `coldopen serve` and `coldopen mcp` never count. Agents' commands reach the app's dispatcher through the agent link, so the app counts them, under `agent_commands`.
- **Tests and the bench don't count:** they run with their own folders (`COLDOPEN_HOME`), and a test's app has counting off unless the test turns it on.

## 3. What it never records

- Script text of any kind: action, dialogue, notes, titles, the title page.
- Names: characters, scenes, locations, the writer's name for changes, agents' names, revision labels.
- File names, folder names and paths, of scripts, exports or anything else.
- Search and replace terms, autocomplete text, command parameters.
- Times of day, time zones, or any date but the UTC day counting started.
- The user's or the computer's name, the GPU's model, hardware or network identifiers.

The test in §10 checks this after a scripted session.

## 4. Where it lives

`usage.json` in the app's data folder (`editor.md` §11):

| OS | Folder |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Cold Open\` |
| macOS | `~/Library/Application Support/Cold Open/` |
| Linux | `$XDG_DATA_HOME/coldopen/`, else `~/.local/share/coldopen/` |

It's JSON with sorted keys, so the same counts always make the same file.

```json
{
  "agent_commands": {},
  "app_versions": ["0.1.0-alpha.3"],
  "commands": { "app.palette": 12, "doc.save": 46, "text.insert": 4210 },
  "opened": { "fdx": 3, "fountain": 9 },
  "renderer": { "cpu": 14 },
  "saved": { "fdx": 6, "fountain": 40 },
  "session_lengths": { "30_min_to_2_h": 4, "5_to_30_min": 6, "over_2_h": 1, "under_5_min": 3 },
  "sessions": 14,
  "since": "2026-11-05",
  "version": 1
}
```

A file that can't be read (broken, or from a newer version) is left alone and not counted into. The View sheet says so, and Reset replaces it.

## 5. When it's written

- **Counts are kept in memory** and added to the file:
  - at most once every 5 minutes while counts are waiting;
  - when the app quits;
  - before the View sheet, Share feedback… or Report a problem… shows the summary, so what the writer reads is what's on disk.
- **The write is atomic** (`coldopen_core::io::atomic_write`), and runs on a background thread. It reads the file, adds the waiting counts, and writes it back.
- **A crash loses at most the last 5 minutes of counts.** That's acceptable: they're counts, not work.
- **A session's length** is measured with the monotonic clock from start to quit and added at quit as one bucket. A crashed session has no length.

## 6. Turning it off

- **The setting:** `count_usage` in `settings.json`, `true` by default (the owner's decision).
- **Off:** nothing is counted, and the file is never written or created. Counts waiting in memory are dropped. The file stays as it was, so the writer can still view or reset it. Reading it happens only when they ask to view it.
- **On again:** counting resumes into the existing file. `since` stays the day counting first started, unless the file was reset.
- **Where it's switched:** the Settings sheet's Usage summary row (`editor.md` §24), the View sheet, and the palette (`app.usage_toggle`, "Turn usage counting on or off").

## 7. Viewing and resetting

- **View usage summary** (`app.usage_summary`; the palette, Settings, About's "Usage summary" row): a sheet with the summary in plain words (§9), scrolling, with:
  - **Copy** (C), the same text;
  - **Reset** (R), which asks first (Enter resets, Esc cancels), then deletes the file and the waiting counts;
  - **Turn counting off** or **on** (O);
  - **Close** (Esc).
- It says, above the summary: "Counted on this computer only. Cold Open never sends it. It goes out only if you tick 'Include my usage summary' when you share feedback."

## 8. Including it in feedback

- **Share feedback…** and **Report a problem…** have a tick: **Include my usage summary**. It's unticked every time the sheet opens.
- **Ticked,** the sheet shows the whole summary in plain words, exactly as it goes out, before any button that opens the browser or copies.
- **Unticked,** nothing of it is used.
- **With counting off and no file,** the tick reads "No usage summary (counting is off)" and can't be ticked. With an empty file, "No usage summary yet".
- **Where it goes** (`editor.md` §23): the form's long-answer box (`report`), or the GitHub issue's body, or the clipboard. When the address would run past `URL_MAX`, the whole text is copied and the sheet says to paste it into the page.

## 9. The plain-words text

The View sheet, the tick and the feedback all use one text, made by one function. For the example in §4:

```text
Usage summary from Cold Open, counted on this computer since 2026-11-05.
App versions: 0.1.0-alpha.3
Sessions: 14 (under 5 minutes: 3; 5 to 30 minutes: 6; 30 minutes to 2 hours: 4; over 2 hours: 1; still open or ended by a crash: 0)
Drawn with: CPU in 14 sessions
Scripts opened: Fountain 9, .fdx 3
Scripts saved: Fountain 40, .fdx 6
Commands you used: text.insert 4210, doc.save 46, app.palette 12
Commands agents used: none
It holds no script text, no names, no file names or folders, and no times finer than a day.
```

- Commands are listed by count, highest first, then by name.
- **Resolution:** command names stay as their ids, since they're what the file holds, and the command reference (`docs/guide/src/reference/`) explains each.

## 10. Tests

- **Nothing personal gets in.** A headless app, with counting on, runs a scripted session. It types action and dialogue holding a sentinel word, renames a character and a scene heading to sentinels, saves the script under a sentinel file name in a sentinel folder, then saves it as .fdx. Neither `usage.json` nor the plain-words text contains any sentinel, any part of the folder, or the user's name. The counts it does hold match the session.
- **Every key is from the tables.** A test reads `usage.json` after the session and checks each key against `Command::NAMES`, the action table and the fixed sets of §2.
- **Off means no writes.** With counting off, the same session creates no file. With a file already there, its bytes and its modified time don't change.
- **The feedback addresses** hold only what the sheet showed: the form's `report` box and the issue's body equal the plain-words text.
- **Reset** deletes the file. The next count starts a new one, with today's `since`.
