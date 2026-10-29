# The Cold Open project bundle (`.coldopen`)

- **Date:** 2026-10-18, updated 2026-10-22 (phase 5b: version 2, `revisions.json`)
- **Status:** implemented in phase 5a, in `crates/core` (`bundle.rs`, behind the `Format` seam). Version 2 (2026-10-22) is being implemented.
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
| `revisions.json` | (2026-10-22, version 2 only) The revision table (§5.1). |
| `script.fountain` | A readable copy of the script (§6). |

**Rules for writers.**

- Entries are compressed with deflate (method 8) or stored (method 0). No encryption, no directories, no other entries of Cold Open's own.
- Names are the ASCII names above.

**Cold Open's own writer**, which other tools need not copy:

- **Compression.** Deflate at level 2, through miniz_oxide (2026-10-19; level 6 until then). On Big Fish with tracked changes, level 2 deflates in about 3 ms where level 6 takes 12, for a file 14% larger (181 KB against 159 KB). Level 1 is no faster and 20% larger again.
- **The readable copy** is written by the Fountain writer's quick path (`coldopen_fountain::write_text`). It gives the same text as a Fountain save but doesn't read the text back to find losses, which a copy that is never read back doesn't need (§6). A test checks the bytes are the same.
- **Times.** No entry records a time: the date and time fields are zero and there is no timestamp extra field, so the time a file was saved never changes its bytes. Tools that show a time show the format's earliest.
- **No extra fields** and no archive comment.
- **Another tool's entries** (2026-10-28), kept as the reader found them (below), are written after its own, unchanged and in the order read, deflated like the rest.

The same document therefore always gives the same bytes. **Resolution:** a newer miniz_oxide could compress differently. That changes only the bytes, never what a reader gets, and would at most make recovery offer a journal whose work the file already holds.

**Rules for readers.**

- Entries are found through the central directory.
- An entry's CRC-32 and size must match what is read; an entry that fails is treated as damaged (§7).
- No entry may be larger than 16 MiB once inflated (256 MiB before 2026-10-28), nor all of them together larger than 64 MiB, and an archive may hold at most 64 entries. An entry inflates no further than the size it declares. A larger one is damaged, so a crafted file cannot exhaust memory or time: before, a 33 KB file inflated to 32 MiB of script and took 15 s to open (review finding CL5).
- (2026-10-28) A `script.fountain` read in place of `document.json` (§7) must have at most 250,000 lines, ten times a long feature's; a longer one is not read, and the bundle is refused with `unsupported_format`.
- A second entry with the same name is ignored, with a report.
- An entry with another name, which another tool added, is kept (2026-10-28, decided with the project's owner after review finding GP3): it is reported, and a save writes it back unchanged. It is not in `document.json` or the journal's header; recovery takes it from the bundle on disk when that bundle is the same document (§8). Converting the project to Fountain or .fdx, which have no place for it, lists it as a loss (`bundle_entries`), so it is never dropped without a question. Only a name that writes back as it is can be kept: printable ASCII with no backslash, not absolute, with no empty or `..` part; an entry with any other name is dropped and reported (`bundle_entry`, dropped), so the first save keeps a copy of the file (§7). Before, this paragraph promised that loss on every save, and the writer dropped such entries with none.

## 3. `meta.json`

```json
{"format": "coldopen-bundle", "v": 1, "app": "0.1.0", "doc_id": "6f0f3c1e-8a52-4d6b-9a8e-0b8f4ac2d3e7"}
```

| Field | Meaning |
| --- | --- |
| `format` | Always `"coldopen-bundle"`. |
| `v` | The bundle format's version, 1, 2 or (2026-10-28) 3. A bundle with a higher `v` was made by a newer Cold Open; it is refused with `unsupported_format` rather than read in part and saved over. (2026-10-28, review finding GP14) `format` and `v` are read before the rest of `meta.json`, so a newer file's fields this version does not know never hide its version. |

**Version 2** (2026-10-22, phase 5b). A bundle whose revision table is not the default (`docs/specs/revisions.md` §3) is written as version 2. It holds `revisions.json`, and its `document.json` may hold revision marks (`rev` in text runs and on blocks). Every other bundle is still written as version 1, byte for byte as before. **Resolution:** a phase 5a build reading a version 2 bundle as version 1 would find fields it does not know in `document.json`, fall back to `script.fountain` (§7), and lose every mark on its next save. The higher `v` makes it refuse the file instead. Version 2 readers take both versions.

**Version 3** (2026-10-28, the reviews of phases 4–5b). A bundle whose marks number a change's moves (`n` in a `moved` entry, `docs/specs/changes.md` §2.2) is written as version 3. A version 2 reader would refuse the field, read the readable copy instead, and lose every mark; a version 3 reader takes all three. Everything else is written as before.
| `app` | The version of the app that wrote the file. Informational. |
| `doc_id` | The document's identity, apart from its path (§8). |

## 4. `document.json`

The document, exactly as the crash journal's header holds it (`docs/specs/journal.md`, "The document in the header"), apart from its change table, which is in `changes.json`:

| Field | Meaning |
| --- | --- |
| `title_page` | Fountain title-page keys. |
| `blocks` | The blocks in order, each with its `id`, as in `docs/specs/commands.md` ("Blocks in JSON"), with their marks (`runs` with `ins` and `del`, and `change` on the block; `docs/specs/changes.md` §2). In a version 2 bundle (2026-10-22), runs and blocks may also carry `rev` (`docs/specs/revisions.md` §4). |
| `next_id` | The next block id to hand out. Ids are never reused, so ids survive a save and reopen. |
| `page_format`, `title_lines`, `spacing`, `fdx` | As in the journal header: the page and element settings, positioned title lines, paragraph spacing by block id, and the .fdx passthrough store. Each is absent when empty. |
| `tracking` | The tracking switches, `{"writer": bool, "agents": bool}` (`docs/specs/changes.md` §4). Absent means the defaults. |

The document must pass the checks a journal header's document passes (`docs/specs/journal.md`, recovery step 1): unique ids below `next_id`, every block keeping the model rules, a valid page format, and spacing inside the page body. In addition, every mark must name a change in `changes.json`. (2026-10-28, review findings GR7, GP13) The .fdx store's kept paragraphs each have blocks, are stored under their first block, and name only ids below `next_id`, as do its loose children's anchors.

**Layout.** A bundle prints its blocks as they stand, as an .fdx document does, with its own page format, title lines and spacing (`docs/specs/layout.md` §4.8). **Resolution:** not in Fountain's normal form, since the bundle keeps exactly what was written. A document read from Fountain is already in normal form, so its pages do not change when it becomes a bundle.

## 5. `changes.json`

The change table, as `docs/specs/changes.md` §3 defines it:

```json
{"v": 1, "changes": [{"id": "c_1", "by": {"kind": "writer", "name": "Maya"}, "t": 1760356801123}]}
```

- The ids run `c_1`, `c_2`, … with no gaps.
- The table is append-only for the life of the file: a change that has been accepted, rejected or undone keeps its entry.
- A bundle with no changes has `{"v": 1, "changes": []}`.
- (2026-10-22) A table whose entries carry a revision set (`rev`) is `{"v": 2, ...}` (`docs/specs/changes.md` §3). It occurs only in a version 2 bundle.

### 5.1 `revisions.json`

Added 2026-10-22 (phase 5b). The revision table, as `docs/specs/revisions.md` §3 defines it:

```json
{"v": 1, "colors": ["White", "Blue", "Pink"], "active": "r_1", "sets": [
  {"id": "r_1", "color": "Blue", "label": "Blue Rev.", "date": "2026-10-22", "started": 1761130000000}
]}
```

- The ids run `r_1`, `r_2`, … with no gaps, and the table is append-only for the life of the file.
- Every `rev` mark in `document.json` must name a set here; `active` must name one, when it is present.
- The entry is written only in a version 2 bundle.

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
| `changes.json` missing or unreadable, or missing entries a mark names | The document with its marks. Each change the table lacks gets the author `{"kind": "writer", "name": "Unknown"}` and time 0, so the writer can still accept or reject it. (2026-10-28, review findings GR2, CL4, GP12) A mark naming a change past 10,000,000, which a journal header may not name either, or a set past 10,000, is no real project's and would take that many stand-ins: `document.json` then fails its checks, as the next row says. | `bundle_changes` (fixed) |
| (2026-10-22) In a version 2 bundle, `revisions.json` missing or unreadable, or missing sets a mark names | The document with its revision marks. Each set the table lacks gets the color "Unknown", the label "Unknown Rev.", the date 1900-01-01 and time 0, and no set is active. The default colors are used. | `bundle_revisions` (fixed) |
| An entry that fails its CRC or size check | As if that entry were missing. | |
| A duplicate entry | It is ignored. | `bundle_entries` (kept) |
| (2026-10-28) An entry with another name | It is kept and written back (§2). | `bundle_entries` (kept) |

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
- Notes anchored to text. (2026-10-22: revision sets and their marks are in version 2 bundles, §3 and §5.1.)
