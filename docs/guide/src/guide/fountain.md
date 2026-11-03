# A Fountain primer

[Fountain](https://fountain.io) is a plain-text way to write a screenplay. A Fountain file reads like a script in any text editor, and Cold Open lays it out as a script. It's Cold Open's own format, so you never need to know it: Cold Open writes it for you. But it's simple, and knowing it helps when you open a script in another program, paste text in, or ask an AI agent to write.

This is a short tour. The [Fountain syntax page](https://fountain.io/syntax) has the whole of it, and Cold Open follows all of it.

## The elements

```fountain
INT. LIGHTHOUSE - NIGHT

The lamp turns. Waves break below.

NORA
Still turning. Still here.

ELI
(from below)
Come down, Nora.

CUT TO:
```

- **A scene heading** is a line that starts with `INT.`, `EXT.`, `EST.`, `INT./EXT.` or `I/E`, with a blank line before it. Start any line with `.` to make it one anyway: `.FLASHBACK`. A number between `#` signs at the end is its scene number: `INT. HOUSE - DAY #12A#`.
- **Action** is any ordinary paragraph.
- **A character** is a line in capitals with a blank line before it and text right after it. That text is the **dialogue**. A line in brackets inside a speech is a **parenthetical**. Start a name with `@` to use lower case: `@McCLANE`.
- **A transition** is a line in capitals ending in `TO:`, or any line starting with `>`.
- **Centered text** goes between `>` and `<`: `> THE END <`.
- **Dual dialogue:** put `^` after the second character's name, and the two speeches print side by side.
- **Lyrics** start with `~`.
- **A page break** is a line of three or more `=`.

## Emphasis

`*italic*`, `**bold**`, `***bold italic***` and `_underline_`. A backslash before a star or underscore prints it as it is: `\*`.

## What doesn't print

- **Notes:** `[[like this]]`, anywhere, even inside a line.
- **Boneyard**, for cut material: `/*` to `*/`, over as many lines as you like.
- **Sections**, for acts and sequences: lines starting with `#`, `##` and so on.
- **Synopses**: lines starting with `=`.

Cold Open keeps all of these, shows them in the margin, and never prints them.

## The title page

A Fountain file can begin with `Key: value` lines:

```fountain
Title: The Lighthouse Keeper
Credit: Written by
Author: Jo Lee
Draft date: 2 November 2026
Contact:
    jo@example.com
```

A blank line ends the title page. In Cold Open, **Edit title page…** (Mod+Shift+T) edits it: see [The title page](title-page-and-reports.md).

## Why Fountain

- **It's yours.** Any text editor on any computer reads it, today and in fifty years.
- **It works with version control.** Git and similar tools show exactly which lines changed between drafts.
- **AI agents read it well.** It's plain text that looks like a script.

What Fountain can't hold, such as tracked changes or revision marks, a [Cold Open project](files.md) keeps.
