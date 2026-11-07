# Changelog

What changed in each release of Cold Open, newest first. Each release's notes, with its files, are on the [releases page](https://github.com/TheYungStump/cold-open/releases); `docs/progress/` has the full record of how each part was built.

## 0.1.0-alpha.3 (2026-11-07)

**Added**

- **Settings** (Ctrl+Comma, or Cmd+Comma on a Mac, where it's also Cold Open ▸ Settings…): the theme, interface size, renderer, your name on tracked changes, agent access, crash reports and the usage summary, in one sheet that works from the keyboard alone.
- **Share feedback…** (palette, About, start screen): feedback typed in the app, opened in the form or a GitHub issue with it filled in.
- **A local usage summary:** counts of which commands, formats and renderer are used, and how long sessions last, on this computer only. Never sent; included in feedback only when you tick it and have seen it in full. View, Reset and an off switch in Settings and About.
- **Report a problem…** can include the usage summary too.
- **The universal macOS disk image,** for every Mac, under the 15 MB budget.
- **An MCP bundle** (`ColdOpen-<version>-mcp.mcpb`) on each release, for AI clients that install MCP bundles and for the Official MCP Registry.
- **The Linux builds' third-party license lists** as release files, for building from source.

**Changed**

- **The icon** and the website are in the project's final brand.
- **Smaller programs:** release builds use full link-time optimization, about 5% smaller.
- **The website** has a writers' home page, a page for AI users, a download page that recommends the right file, and pages comparing screenwriting apps.

**Fixed**

- **Saving an .fdx file on a Mac could pause the app for a few milliseconds.** After each save, Cold Open flushed the drive once more than it needed to, and the next save's first step could wait for it.

**For testers**

- **Measure performance** also times the save's background steps, and an edit typed right after a save.

## 0.1.0-alpha.2 (2026-11-04)

The alpha we're inviting testers to.

**Added**

- **Report a problem…** (palette, About, start screen). It opens a prefilled GitHub issue in the browser, or the feedback form for people without a GitHub account, with the version, commit, OS and renderer only.
- **Opt-in crash reports.**
  - A crash leaves a local report with no script text, file names or user names, and the next start offers it once.
  - Sending shows the whole report first, then opens a prefilled issue and copies the report to paste.
  - **Never ask again**, and **Turn crash reports on or off** in the palette.
- **Crash on purpose (to test recovery)** in the palette.
- **Open the user guide** (palette, About, start screen).
- **Interface size** (palette): **Make the interface larger**, **smaller**, and **Interface at normal size**, from 100% to 200%, kept in the settings.
- **The website and user guide** (<https://theyungstump.github.io/cold-open/>):
  - install notes for each system;
  - getting started, a Fountain primer, every feature, recovery, AI agents, known limitations and a FAQ;
  - keyboard, command-line and command references generated from the code;
  - the code signing policy and the privacy statement.
- **Each release's symbols** (`ColdOpen-<version>-symbols.zip`), so the project can read crash reports, and `tools/symbolicate.py`.
- **Issue templates** for bugs, file problems, crashes and feedback, and a discussion form for alpha testing.
- **Every command parameter has a description,** which agents see in the MCP tools' schemas.
- **`coldopen.exe` names the product and its version** in its file details on Windows.
- **The README's code signing policy and privacy statement.**

**Fixed**

- On Windows, links that open a prefilled page, such as Report a problem's, opened the Documents folder instead of the browser.
- Report a problem's **Close** button ran out of its box.
- "Crash on purpose?", and two error messages (a script in a folder that can't be written, and `coldopen bench` without the app), had a run of spaces in the middle of a sentence.
- A test of the journal's restart could fail when a kill landed just after a typed space. Recovery was right; the test read the script through the export, which trims trailing spaces.

**Project**

- `tools/privacy_scan.py`, run before every commit and in CI, and a CI check that no network client enters the programs.
- `coldopen-diff` joins the license-boundary check, with its license texts.
- The `crash_scrub` fuzz target.

## 0.1.0-alpha.1

The first alpha, published quietly so a released version exists. It brought the whole app, built in phases 1 to 6a:

- **Writing:**
  - smart element flow and autocomplete;
  - the scene navigator, find and replace, and the command palette;
  - page and draft views, themes, editable keys and element flow.
- **Never losing work:** a crash journal that saves every keystroke before it's drawn, atomic background saves, Recovered versions.
- **Files:**
  - Fountain;
  - .fdx with a byte-for-byte round trip of everything not edited, and repair with a report;
  - the `.coldopen` project bundle.
- **Revising:** track changes with authors, production revision sets and colors, script diff and "Review a draft as changes", the title page editor, and reports.
- **Output:** PDF export, the same byte for byte on every OS, with watermarks, scene numbers, marked changes and colored pages.
- **Every action as a command:**
  - the `coldopen` command-line tool;
  - JSON over stdio;
  - an MCP server whose edits arrive in the app live, as tracked changes.
- **Packages for Windows** (x64 and ARM64), **macOS** (Apple Silicon and Intel) and **Linux** (x86-64 and ARM64: .deb, AppImage, Flatpak), each under 15 MB. Also the bench, **Measure performance**.
