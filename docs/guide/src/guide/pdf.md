# PDF export

**Export a PDF** (Mod+Shift+E) opens a short form:

| Option | Key | What it does |
| --- | --- | --- |
| **Scene numbers** | Mod+1 | Prints each scene's number beside its heading |
| **Title page** | Mod+2 | Prints the title page first (on by default) |
| **Print the current text** | Mod+3 | Shown when changes are pending: prints the script with every change accepted, instead of marked |
| **Colored pages** | Mod+4 | Shown when the script has revision marks: prints each revised page on its set's color |
| **Watermark** | | Text printed diagonally across every page, such as `DRAFT` or a reader's name |

Enter asks where to save it. The PDF is written in the background, so you can keep working; a notice says when it's done.

## What you get

- **The same pages everywhere.** The PDF is laid out by Cold Open itself, with its bundled Courier Prime font, so it comes out byte for byte the same on Windows, macOS and Linux. What you see on screen is what prints.
- **Standard screenplay format:** 12-point Courier, about 55 lines a page, with the usual margins and page numbers. Page breaks follow the usual rules: a scene heading never ends a page, a character never leaves their speech behind, and a speech split over a page prints `(MORE)` at the foot and the name with `(CONT'D)` at the top of the next.
- **Tracked changes** print marked, in their authors' colors, unless you choose "Print the current text".
- **Revisions** print with `*` in the margin, and each revised page's header names its set and date.
- **Notes, boneyard, sections and synopses** never print.

A character the font has no shape for (Chinese, Japanese or Korean text, or emoji) prints as a box, and the export says which.

The PDF and everything in it are yours. The font's license (the SIL Open Font License) doesn't cover what you print with it.

## From the command line

```
coldopen export draft.fountain draft.pdf
coldopen export draft.fdx review.pdf --watermark "DRAFT" --scene-numbers
coldopen export draft.fountain pages.pdf --no-title-page
```

`coldopen pages draft.fountain` prints the pages as plain text, the way they print, which is handy for checking where pages break.

**Compare as a PDF:** `coldopen diff a.fdx b.fdx --pdf changes.pdf`, or **Save as PDF…** in the compare view, prints the second draft with the changes marked. See [Compare drafts](compare.md).
