# Crash reports

- **Date:** 2026-11-02
- **Status:** phase 6b, being implemented.
- **Sources:**
  - `docs/spec.md`, "Reliability features" 9 ("Opt-in crash reports that never include script text"), and the privacy statement in `README.md`.
  - The phase 6b brief and the project owner's decisions of 2026-11-02 (`docs/progress/phase-6b.md`): a Google Form for people without a GitHub account, and backtraces as offsets resolved from symbols kept per release.
  - SignPath Foundation's terms (signpath.org/terms, read 2026-11-02): nothing is sent to another system unless the user asks for it.
  - Rust's standard library documentation for panic hooks (`std::panic::set_hook`, `PanicHookInfo`), and the documented messages of its own panics, such as slicing a `str` off a character boundary, which quote the string.
  - The `backtrace` crate's documentation (`trace`, `Frame::ip`, `Frame::module_base_address`).
  - GitHub's documentation on prefilling an issue form from a URL's query (each field by its `id`), and Google's on prefilled form links (`entry.<id>`).
  - `docs/specs/editor.md` §13 and §23, `docs/specs/journal.md`, `docs/specs/release.md` §2 and §9.
- No Final Draft material. A choice where the sources leave room is marked **Resolution**.

## 1. Goals

1. **A crash costs nothing,** as before: the crash journal brings back every keystroke (`journal.md`). A report is extra, never in the way of recovery.
2. **A report says where it broke,** so the project can fix it: the code location, the message, and a backtrace.
3. **A report holds nothing personal:** no script text, no file names or paths, no user or computer names.
4. **Nothing leaves the computer unless the writer sends it,** after seeing the whole report. Cold Open itself makes no network connection; sending means a page in the writer's browser that they submit, or text they paste.
5. **"Never ask again" means it.**

## 2. What counts as a crash

- **Covered:** a Rust panic in the desktop app (`coldopen-app`), on any thread. The report is written by a panic hook, before the panic unwinds or aborts the process.
- **Not covered**, and said so in the privacy statement and the user guide's known limitations:
  - a process ended from outside (Task Manager, `kill`, a power cut) or by the OS for want of memory;
  - a crash below Rust: a segmentation fault, an abort in a driver or system library, a GPU driver reset;
  - `coldopen` and `coldopen mcp`, which print a panic to standard error as before. **Resolution:** they run in a terminal or under an agent, where the message is seen; the app is where a crash goes unseen.
- **A panic on a worker thread the app survives** (a save or PDF worker, say) also writes a report. The app goes on, with the error the worker's failure already shows. The offer comes at the next start (§6).

## 3. The report

A plain-text file, so a writer can read it in any editor, and a developer can read it in an issue:

```text
Cold Open crash report 1
Version: 0.1.0-alpha.2 (5d6e764a1b2c)
Target: x86_64-pc-windows-msvc
OS: Windows 11 (26100)
Renderer: CPU
Time: 2026-11-03T12:34:56Z
Thread: main
Last action: text.insert
Location: crates/app/src/editor/mod.rs:812:22
Message: byte index 3 is not a char boundary; it is inside [quoted] (bytes 2..4) of [quoted]

Backtrace:
   0: Cold Open.exe+0x1a2b3c
   1: Cold Open.exe+0x19f004
   2: ntdll.dll+0x8d2a1
```

| Line | What it holds |
| --- | --- |
| `Version` | `build_info::VERSION_LINE`: the version and the commit (`release.md` §9). |
| `Target` | The build's target triple. |
| `OS` | The OS's name and version, from the system: no host name, no user name. |
| `Renderer` | The renderer the window was drawn with as last known: what the system reported to About, if it was asked, else the renderer the settings asked for (CPU or GPU), with "(the GPU renderer could not start)" when the app fell back (`editor.md` §22). |
| `Time` | When it crashed, in UTC, to the second. |
| `Thread` | The thread's name, such as `main`, or `unnamed`. |
| `Last action` | The name of the last request the app's router ran (`editor.md` §16): an app action or a command, such as `text.insert` or `app.export_pdf`. Never its parameters. A name outside the fixed lists of actions and commands (`recent:<n>`, say) is written as its prefix, `recent`. `none` before the first. |
| `Location` | Where the panic was raised, as `file:line:column`, with the path cut down by §4.2. |
| `Message` | The panic's message after §4.1. |
| `Backtrace` | One line per frame, as `<module>+0x<offset>`, at most 100 frames (§5). |

- **The format's number** (`1`) goes up if a line's meaning ever changes, so a script reading reports can tell.
- **Resolution (the time):** to the second and in UTC, so it names no time zone, and matches what a writer remembers ("it crashed this morning").

## 4. Scrubbing

Applied in the hook, before anything is written. The writer sees the result in full before it goes anywhere (§7), so a gap in these rules is a second line of defence, not the only one.

### 4.1 The message

In this order:

1. **Quoted spans become `[quoted]`.** Rust's own messages put the text they quote in quotes: a `str` sliced off a character boundary quotes the string in backticks, and the `Debug` form of a string or a path, as `unwrap` and `expect` print an error, puts it in double quotes. So:
   - `` `…` `` and `"…"`, with a backslash escaping the next character inside double quotes, as `Debug` writes it;
   - `'…'`, but only from a `'` that doesn't follow a letter or a digit, so the apostrophe in "can't" opens nothing;
   - a quote that never closes takes everything after it.
2. **Absolute paths become `[path]`**, up to the next space or closing bracket:
   - a drive letter and a backslash or slash (`C:\…`, `D:/…`);
   - a network path (`\\server\…`);
   - a slash at the start of a word followed by a letter, a dot or `~` (`/home/…`, `/Users/…`, `~/…`), but not a fraction or "and/or".
3. **The home folder and the user name become `[home]` and `[user]`.** They come from the environment (`HOME` or `USERPROFILE`; `USER` or `USERNAME`), matched without regard to case. The user name only as a whole word, and only when it is three characters or more, so a one-letter account name doesn't blank every word with that letter.
4. **Control characters** (other than the tab) are removed, and line breaks become spaces.
5. **At most 500 characters**, cut at a character boundary, with `…` when cut.

**A rule for Cold Open's own code:** a panic message never interpolates script text, a path or a name without quoting it (with `{:?}`, which quotes). The scrubber then removes it. Code review checks this. It can't be enforced by a test.

### 4.2 The location

The panic's file is cut to the part a developer needs, by the first rule that matches:

- inside the workspace: from `crates/` on (`crates/app/src/editor/mod.rs`);
- a dependency from a registry: from the crate's folder on (`iced_widget-0.14.2/src/text_input.rs`), found after the last `/registry/src/<index>/` or `\registry\src\<index>\`;
- the standard library: from `library/` on (`library/core/src/str/mod.rs`);
- anything else: the file name alone.

A release build's paths are CI's, but a developer's build has their own, so the rule applies to every build.

### 4.3 The backtrace

Frames are code addresses, which hold nothing personal. Each module is named by its file name only (`Cold Open.exe`, `libc.so.6`): never its folder, which could be a home folder or an AppImage's mount point.

## 5. Frames and symbols

- **Each frame** is its instruction address minus the base address its module was loaded at, the offset within that module. The offset is the same on every run of one build. It differs between builds, so a report names its version and commit, and the symbols of that build resolve it (`release.md` §9).
- **The module** comes from the `backtrace` crate's `Frame::module_base_address`. Where it gives none, the hook asks the OS which module holds the address (`dladdr` on Linux and macOS, `GetModuleHandleExW` and `GetModuleFileNameW` on Windows). A frame whose module can't be found is written as `?+0x<address>`.
- **Names.** Release builds are stripped and the hook never resolves names: resolving reads debug information, which a stripped build doesn't have and a debug build takes seconds to parse. **Resolution:** a developer resolves a report with `tools/symbolicate.py <report> <symbols folder>`, which reads the symbols for the report's target (`release.md` §2) and prints each frame with its function, file and line. In a developer's own build the symbols are the build's own files.
- **At most 100 frames.** The hook's own frames come first. They are kept, since without names they can't be told apart, and they cost a few lines.

## 6. Storage and the offer

- **Where:** `<data>/crash-reports/` (`editor.md` §11), one file per crash, `crash-<YYYYMMDD>-<HHMMSS>Z.txt`, written to a temporary file in the same folder and then renamed, as every save is (`journal.md`). A second crash in the same second adds `-2`, and so on.
- **How many:** the newest 5 are kept. Writing a report deletes the oldest beyond that.
- **Seen or not:** a report the writer hasn't been offered yet is named `….txt`. Once offered, it is renamed `….seen.txt`, and the offer doesn't come back for it.
- **At the next start,** when an unseen report exists and reports are on (§8), a notice says: "Cold Open closed unexpectedly. A crash report was saved, with no script text." It has **Look at it** and **Never ask again**, and its Dismiss means "not now". It waits behind the recovery dialog when a script's work is being recovered: recovery comes first. Showing the notice marks every unseen report seen.
- **The palette's "Look at crash reports"** (`app.crash_reports`) opens the sheet (§7) at any time on the newest kept report, seen or not. With none kept, a notice says so.

## 7. Looking and sending

The crash report sheet (`editor.md` §23) shows:

- "Cold Open crashed on <date>. This report holds no script text, file names or user names. Read it before you send it." **Resolution:** the date is the report's, in UTC, so no time-zone data is read for it;
- **the whole report,** exactly as it is on disk, in a scrolling area;
- **Send on GitHub** (G):
  - copies the whole report to the clipboard;
  - then opens a new issue in the browser from the crash template (`.github/ISSUE_TEMPLATE/crash.yml`), with the version, the OS, the renderer and a summary (the location and the message) filled in;
  - the template's report field says the whole report is on the clipboard, to paste there;
- **Send with the form** (F), "no GitHub account needed": the same, with the Google Form's prefilled link. The kind is set to Crash, and the version, OS, renderer and summary are filled in. Shown only once the form's address is set in the build (`editor.md` §23);
- **Copy** (C): the whole report to the clipboard, and nothing else;
- **Delete** (Delete): asks "Delete this report?", then deletes it;
- **Older** and **Newer** (Left, Right), when more than one is kept;
- **Close** (Esc).

**Nothing is sent by Cold Open.** The page opens in the writer's browser, where they can change it, and nothing happens until they submit it there.

**URL length.** A prefilled address is kept under 2,000 characters, which Windows' shell passes on intact. The summary is cut, with `…`, to fit. The full report always goes by the clipboard, never in the address. **Resolution:** a backtrace of 100 frames is about 3,000 characters, past what the address can carry; pasting it costs one step.

## 8. Never ask again

- **`crash_reports`** in `settings.json` is `"ask"` (the default) or `"off"`.
- **Off** writes no report and shows no offer. The hook still chains to the default one, which prints the panic to standard error.
- **Never ask again** sets it to off. The notice then says: "Crash reports are off. 'Turn crash reports on or off' in the palette turns them back on."
- **The palette's "Turn crash reports on or off"** (`app.crash_reports_toggle`) switches it, and says which it is now.
- Turning them off doesn't delete the reports kept. "Look at crash reports" still shows them, with Delete.
- **Resolution:** off stops the writing too. A report nobody will be offered helps no one, and keeping fewer files is the private choice.

## 9. Crash on purpose

So a tester can try recovery and a report without waiting for a real crash:

- **The palette's "Crash on purpose (to test recovery)"** (`app.crash_test`) asks: "Cold Open will close at once, as if it crashed. Everything you've typed is in the crash journal and comes back at the next start. Crash now?". **Crash now** is Enter; **Cancel** is Esc.
- **Crash now** panics on the UI thread with the message `Crash on purpose, from the palette`, in a function named `crash_on_purpose`, so a resolved backtrace shows it by name.
- **In debug builds only**, `COLDOPEN_TEST_CRASH` picks what the panic is, for the tests in §10:
  - `slice`: slices the text of the caret's block at a byte inside its first character of two or more bytes, so Rust's own message quotes that text;
  - `debug`: panics with the block's text formatted with `{:?}`;
  - `worker`: panics on a new thread named `coldopen-test-worker`, which the app survives.

  Release builds ignore it.

## 10. Tests

- **The scrubber,** as unit tests:
  - a slicing message;
  - `Debug` strings with escaped quotes, and chars;
  - apostrophes;
  - an unclosed quote;
  - Windows, network and Unix paths, and `~/`;
  - the home folder and the user name, in both cases;
  - control characters;
  - 10,000 characters of input.
- **The location rule** for the workspace, the registry on Windows and Unix, std and other paths.
- **A fuzz target, `crash_scrub`:** the scrubber never panics (a panic in the hook aborts the process), its output is never over the cap, and no quoted span of its input (between matching backticks or double quotes) appears in its output.
- **The report from a crash during typing** (`crates/app/tests/crash_report.rs`):
  - runs the gate driver as its own process, with its own folders;
  - types a short scene whose words aren't used anywhere in Cold Open's code, accents included;
  - crashes it with `slice`, then with `debug`;
  - checks each report: it holds none of the typed words, no part of the test's folder path, and not the user or computer name. Its lines are those of §3, and it is named as §6 says.
- **The offer:**
  - a headless app with an unseen report shows the notice once, and renames the report seen;
  - with `crash_reports: "off"`, it neither shows the notice nor writes a report;
  - a worker's panic leaves the app running and writes a report.
- **The addresses:** the prefilled GitHub and form addresses hold the fields of §7 and no path or user name, and stay under 2,000 characters with a 500-character message.
- **Symbols** (CI, Linux): a release build of the driver crashes on purpose. `tools/symbolicate.py` with that build's symbols must name `crash_on_purpose` in one of the report's frames.
