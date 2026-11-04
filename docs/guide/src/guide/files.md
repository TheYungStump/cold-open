# Files: Fountain, .fdx and projects

Cold Open opens and saves three kinds of script file. It saves a script back to the kind it came in, unless you choose another with **Save as**.

| | Fountain (`.fountain`) | Final Draft–compatible (`.fdx`) | Cold Open project (`.coldopen`) |
| --- | --- | --- | --- |
| **What it is** | Plain text ([primer](fountain.md)) | The XML format most productions exchange | A zip holding the script and everything about it, in open formats |
| **Opens in** | Any text editor, and most screenwriting apps | Final Draft and most screenwriting apps | Cold Open; or unzip it to read the Fountain and JSON inside |
| **Keeps tracked changes** | No | No | Yes |
| **Keeps revision sets and marks** | No | No | Yes |
| **Keeps notes, boneyard, sections, synopses** | Yes | No | Yes |
| **Keeps parts Cold Open doesn't use** (another app's settings and the like) | — | Yes, untouched | Yes, from the .fdx it came from |
| **Best for** | Your own writing, version control, sharing with Fountain apps | Sending to and receiving from a production | Revising: changes, revisions, notes, all in one file |

**When to use which:**

- **Writing on your own:** Fountain. It's readable anywhere and never locked in.
- **A production sent you an .fdx:** keep it as .fdx, so they get back the format they sent. See [.fdx exchange](fdx.md).
- **Tracking changes, or production revisions:** a project. Save as **Cold Open project…** in the palette. You can always save a copy as Fountain or .fdx to send out.

## When a format can't hold something

Cold Open never drops anything without asking. If you save to a format that can't hold part of your script, it says what would be lost before writing anything, and offers three choices:

- **Save as Cold Open project…** keeps everything. It's the default when tracked changes or revision marks would be lost.
- **Save anyway** writes the file without what it can't hold. Tracked changes are accepted first, and revision marks taken out, and the version with them is kept in [Recovered versions](recovery.md#recovered-versions).
- **Cancel.**

**Save a copy as .fdx** and **Save a copy as Fountain** (palette) write a copy in that format and leave the script you're working on as it is.

## Opening

- **Mod+O** opens `.fountain`, `.spmd`, `.txt`, `.fdx` and `.coldopen` files.
- **Recent files** are on the start screen and in the palette. A file that's gone (on an unplugged drive, say) stays in the list, marked Missing, until you remove it.
- **A script in a folder Cold Open can't write** (a read-only share, or a CD) opens as an untitled copy, with a notice. Save as… keeps it somewhere else, and the original is never touched.

## Inside a project

A `.coldopen` file is an ordinary zip. Inside are `script.fountain` (a readable copy of the script), `document.json` (the script with every element's id), `changes.json` (tracked changes), `revisions.json` (revision sets) and `meta.json`. Its format is open and documented in the [bundle spec](https://github.com/TheYungStump/cold-open/blob/main/docs/specs/bundle.md), so other tools can read it, and the same script always saves to the same bytes.
