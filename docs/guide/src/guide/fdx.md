# .fdx exchange and repair

Most productions send scripts as `.fdx` files, the format Final Draft uses. Cold Open opens them, lets you edit them, and saves them back so the people who sent them get the format they expect.

## What comes back as it went

- **Everything you didn't edit is written back exactly as it was,** down to the byte. A file opened and saved with no edits is identical to the original.
- **What Cold Open doesn't use passes through untouched:** another app's settings, page setup, and anything else in the file that Cold Open doesn't model.
- **What you did edit** is written in the same style as the rest of the file.
- **The pages:** Cold Open lays out an .fdx with its own page settings, so its page count matches the file's own closely. We measure it against reference scripts, and on a 120-page feature it matches page for page.

## What .fdx can't carry from Cold Open

Some things Cold Open holds have no public description of how .fdx stores them. Cold Open won't guess at a format, so these aren't written to .fdx. It asks before saving, and a [project](files.md) keeps all of them:

- **Notes, boneyard, sections and synopses** are left out.
- **Tracked changes** are written accepted: the file holds the script as it would be with every change accepted.
- **Revision sets and marks** are left out. A `Revisions` list the file already had passes through untouched.
- **A Fountain title page's key names** become plain lines.

## A damaged file

A damaged `.fdx` still opens. Cold Open repairs what it can: broken XML, a wrong encoding, characters XML doesn't allow, settings out of range. Then it says what it fixed:

- **"Cold Open repaired this file"** gives the counts and the first few problems, in plain words and without your script's text.
- **Details** (D) shows the whole report.
- **Save repaired copy…** (S) saves the repaired script under a new name.
- **OK** carries on with the repaired script. Nothing is written until you save.

The first save over a damaged file keeps a hidden copy of it as it was, which you'll find in [Recovered versions](recovery.md#recovered-versions).

**Repair an .fdx file** (palette) repairs an .fdx into a new file without opening it, and shows the same report. So does the command line:

```
coldopen repair damaged.fdx repaired.fdx
```

The input is never written. Add `--json` for a report a program can read. It holds codes and positions only, never script text, so it's safe to share when you [report a file problem](https://github.com/TheYungStump/cold-open/issues/new?template=file-problem.yml).

**You can use this even if you write in another app.** `coldopen repair` doesn't change your file, and a repaired copy opens anywhere .fdx does.

## Converting

- **Save as** with an `.fdx` name converts a Fountain script to .fdx, and the other way round. What the target can't hold is listed first, as above.
- `coldopen convert draft.fdx draft.fountain` does the same from the command line, and refuses a lossy conversion unless you add `--allow-lossy`.
