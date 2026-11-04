# Production revisions

Once a script is in production, each new round of changes is a **revision set** with its own color: Blue pages, then Pink, then Yellow. Everyone can see what changed since the last pages they were given. Cold Open marks revised lines with `*` in the margin and prints each revised page with its set's name and date in the header.

This is different from [track changes](track-changes.md): track changes is for reviewing edits before they're accepted; revision sets record what changed between drafts that went out.

## Starting a set

**Start revision…** (palette) asks for:

- **the color:** the next one in the list, which you can change with Up and Down, or by typing any name;
- **the label:** "Blue Rev." to begin with, following the color until you edit it;
- **the date:** today's, as `YYYY-MM-DD`.

Enter starts it. From then on, everything you change is marked with the set's color, and the hint bar shows the active set ("Blue Rev. 10/22/26"). Click it for the revision commands.

**End revision** (palette) stops marking. Starting a set while one is active asks to end that one first.

## The colors

The usual order is **White, Blue, Pink, Yellow, Green, Goldenrod**, then **Buff, Salmon, Cherry**, then the same nine again as "Double Blue" and so on. Productions vary it, so **Revision colors…** (palette) lets you edit the list, as names between commas.

## What you see

- Revised text in its set's color on screen, and a `*` in the right margin of every revised line, exactly as the PDF prints them.
- Each revised page's header: the set's label and date.
- A `*` after the heading of each scene with revisions, in the navigator.

## Clearing marks

**Clear revision marks…** (palette) clears one set's marks, or every set's. The sets stay in the list. It's one undo step.

## Saving and printing

- **A [project](files.md)** keeps the sets and marks.
- **Fountain and .fdx can't hold them.** Saving asks first, with a project as the default. **Save anyway** takes them out and keeps the marked version in [Recovered versions](recovery.md#recovered-versions).
- **PDF export** prints the marks and the revised pages' headers. **Colored pages** (Mod+4 in the export form) prints each revised page on its set's color.

## Not yet

Locked pages, A and B pages (12A, 12B), and locked scene numbers aren't in the alpha. See [Known limitations](limitations.md).
