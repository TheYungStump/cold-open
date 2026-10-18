# The Cold Open project bundle (`.coldopen`)

- **Date:** 2026-10-18
- **Status:** phase 5a, being implemented in `crates/core` (`bundle.rs`, behind the `Format` seam).
- **Sources:**
  - `docs/spec.md`, "File formats": a project bundle holds everything Fountain cannot, as an open format documented in the repository.
  - `docs/specs/journal.md`, "Decisions for phase 5" (F18.3, F18.4, written 2026-10-16), and the phase 5a brief and its decisions (2026-10-18, `docs/progress/phase-5a.md`).
  - The ZIP file format specification, PKWARE's APPNOTE.TXT (version 6.3.10), for the archive layout, and RFC 1951 for deflate. Both are public standards; only the parts named here are used.
  - RFC 9562, for the layout of a random (version 4) UUID.
  - `docs/specs/changes.md`, `docs/specs/fountain.md`, `docs/specs/fdx.md`.
- No Final Draft material. A choice where the sources are silent is marked **Resolution**.

A `.coldopen` file is one script with everything Cold Open knows about it: its blocks with their ids, tracked changes, the page settings and passthrough data of an .fdx file it came from, and its tracking switches. It is an open format. Any tool may read it with a zip library and a JSON parser, and this document is all it needs.

## 1. Goals

1. **Lossless.** Saving and reopening a bundle gives the same document: every block id, mark, change, setting and kept .fdx part. Nothing is a loss.
2. **Readable without Cold Open.** Every part is JSON or Fountain inside an ordinary zip archive.
3. **The same bytes every time.** The same document always writes the same bytes, on every OS, so a fingerprint names a version of the file (`docs/specs/journal.md`, "Nothing to recover?").
4. **Never lost to damage.** A bundle with a damaged part opens with what can be read, and says what was repaired.

## 2. The archive

A bundle is a zip archive (APPNOTE.TXT) holding these entries, each at most once, in this order:

| Entry | Holds |
| --- | --- |
| `meta.json` | What the file is (§3). |
| `document.json` | The document (§4). |
| `changes.json` | The change table (§5). |
| `script.fountain` | A readable copy of the script (§6). |

**Rules for writers.**

- Entries are compressed with deflate (method 8) or stored (method 0). No encryption, no directories, no other entries.
- Names are the ASCII names above.

**Cold Open's own writer**, which other tools need not copy:

- **Compression.** Deflate at level 6, through miniz_oxide.
- **Times.** No entry records a time: the date and time fields are zero and there is no timestamp extra field, so the time a file was saved never changes its bytes. Tools that show a time show the format's earliest.
- **No extra fields** and no archive comment.

The same document therefore always gives the same bytes. **Resolution:** a newer miniz_oxide could compress differently. That changes only the bytes, never what a reader gets, and would at most make recovery offer a journal whose work the file already holds.

**Rules for readers.**

- Entries are found through the central directory.
- An entry's CRC-32 and size must match what is read; an entry that fails is treated as damaged (§7).
- No entry may be larger than 256 MiB once inflated, and an archive may hold at most 64 entries. A larger one is damaged, so a crafted file cannot exhaust memory.
- A second entry with the same name is ignored, with a report.
- An entry with another name is ignored, with a report. A save then writes the bundle without it and lists it as a loss (`bundle_entries`), so nothing a newer tool added vanishes without a question.

## 3. `meta.json`

```json
{"format": "coldopen-bundle", "v": 1, "app": "0.1.0", "doc_id": "6f0f3c1e-8a52-4d6b-9a8e-0b8f4ac2d3e7"}
```

| Field | Meaning |
| --- | --- |
| `format` | Always `"coldopen-bundle"`. |
| `v` | The bundle format's version, 1. A bundle with a higher `v` was made by a newer Cold Open; it is refused with `unsupported_format` rather than read in part and saved over. |
| `app` | The version of the app that wrote the file. Informational. |
| `doc_id` | The document's identity, apart from its path (§8). |

## 4. `document.json`

The document, exactly as the crash journal's header holds it (`docs/specs/journal.md`, "The document in the header"), apart from its change table, which is in `changes.json`:

| Field | Meaning |
| --- | --- |
| `title_page` | Fountain title-page keys. |
| `blocks` | The blocks in order, each with its `id`, as in `docs/specs/commands.md` ("Blocks in JSON"), with their marks (`runs` with `ins` and `del`, and `change` on the block; `docs/specs/changes.md` §2). |
| `next_id` | The next block id to hand out. Ids are never reused, so ids survive a save and reopen. |
| `page_format`, `title_lines`, `spacing`, `fdx` | As in the journal header: the page and element settings, positioned title lines, paragraph spacing by block id, and the .fdx passthrough store. Each is absent when empty. |
| `tracking` | The tracking switches, `{"writer": bool, "agents": bool}` (`docs/specs/changes.md` §4). Absent means the defaults. |

The document must pass the checks a journal header's document passes (`docs/specs/journal.md`, recovery step 1): unique ids below `next_id`, every block keeping the model rules, a valid page format, and spacing inside the page body. In addition, every mark must name a change in `changes.json`.

**Layout.** A bundle prints its blocks as they stand, as an .fdx document does, with its own page format, title lines and spacing (`docs/specs/layout.md` §4.8). **Resolution:** not in Fountain's normal form, since the bundle keeps exactly what was written. A document read from Fountain is already in normal form, so its pages do not change when it becomes a bundle.

## 5. `changes.json`

The change table, as `docs/specs/changes.md` §3 defines it:

```json
{"v": 1, "changes": [{"id": "c_1", "by": {"kind": "writer", "name": "Maya"}, "t": 1760356801123}]}
```

- The ids run `c_1`, `c_2`, … with no gaps.
- The table is append-only for the life of the file: a change that has been accepted, rejected or undone keeps its entry.
- A bundle with no changes has `{"v": 1, "changes": []}`.

## 6. `script.fountain`

The script as Fountain (`docs/specs/fountain.md`), written from the document with every pending change accepted (`docs/specs/changes.md` §9), in UTF-8 with LF line ends. It is rewritten at every save and **never read back** while `document.json` can be read. It lets other tools, a reader without Cold Open, or `git diff` see the script.

**Resolution:** for `git diff`, a `.gitattributes` line `*.coldopen diff=coldopen`, together with a `diff.coldopen.textconv` command that prints the `script.fountain` entry of the file it is given (with Info-ZIP, `unzip -p <file> script.fountain`), shows changes as Fountain text. This is documented here for writers and not set up by Cold Open.

## 7. Reading a damaged bundle

Cold Open's reader recovers what it can and reports the rest in `doc.open`'s `repaired` (as for .fdx, `docs/specs/fdx.md` §9). The first save in place then keeps a copy of the damaged file (`docs/specs/journal.md`, "Damaged originals").

| Damage | What is read | Report |
| --- | --- | --- |
| Not a zip archive at all | Nothing: `unsupported_format`. | |
| `meta.json` missing or unreadable | Everything else. A new `doc_id` is made. | `bundle_meta` (fixed) |
| `meta.json` with a `v` above 1 | Nothing: `unsupported_format`, so a newer file is never saved over. | |
| `document.json` missing, unreadable, or failing the checks of §4 | `script.fountain` read as a Fountain file. Ids are numbered from `b_1`, and changes and switches are lost. | `bundle_document` (dropped) |
| Both of those, or `script.fountain` missing too | Nothing: `unsupported_format`. | |
| `changes.json` missing or unreadable, or missing entries a mark names | The document with its marks. Each change the table lacks gets the author `{"kind": "writer", "name": "Unknown"}` and time 0, so the writer can still accept or reject it. | `bundle_changes` (fixed) |
| An entry that fails its CRC or size check | As if that entry were missing. | |
| A duplicate or unknown entry | It is ignored. | `bundle_entries` (kept) |

## 8. `doc_id`

- **What it is.** A random version 4 UUID (RFC 9562), written in lower-case hex with hyphens. It is made from the operating system's random source when a bundle is first written at a path: by "Save as", by a conversion, or by `doc.new` at a `.coldopen` path. A save in place keeps it. A copy of the file is the same document, so it keeps the same id.
- **Where it is mirrored** (F18.4): in the crash journal's header, and in the desktop app's session file for its agent link, next to each path (`docs/specs/mcp.md` §5.1). The path stays an attribute.
- **What it decides today.** A journal whose header names another `doc_id` than the bundle at its path was written for another document, one that has since been replaced by a different project with the same name. Recovery then treats the file as changed by another program (`external_change`), so it is never overwritten without the writer's say.

## 9. The crash journal

The journal stays a separate file next to the bundle, `.<name>.coldopen.coldopen-journal`, and works as it does for other formats (`docs/specs/journal.md`). A zip cannot take one more entry and a flush per keystroke. Two things differ:

- The header's `format` is `"coldopen"` and it carries `doc_id`.
- The header does not copy the change table: it names the table by the number of its entries and their fingerprint (`docs/specs/journal.md`, "The change data"). Because the table is append-only, the bundle on disk, even one saved after the header was written, always starts with the table the header names.

## 10. What is not in a bundle

- The undo history and the crash journal. Undo is unlimited while a script is open, as for other formats, and starts again when it is reopened.
- Revision sets, notes anchored to text and other phase 5b data. They will come with new entries, or new fields under a new `v`.
