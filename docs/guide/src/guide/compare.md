# Compare drafts

Cold Open compares two drafts scene by scene. It finds scenes that moved, as well as text added, deleted and changed, and works across formats: compare a Fountain draft with an .fdx one, or with a project.

## Compare with…

**Compare with…** (palette) asks for another draft, then compares your script as it is now (A) with it (B).

The view shows **B's pages with the changes marked**, and a list of the changed scenes in place of the navigator:

- Up and Down choose a scene, and Enter scrolls to it.
- **Save as PDF…** (Mod+Shift+E) writes the marked pages as a PDF, to send to someone.
- Esc closes the view.

Your script isn't changed, and you can't type into the view. A long script is compared in the background: you can keep working, and Esc stops waiting.

**Changes of style or of an element's settings** (bold, centering, extra blank lines) show in the list of changed scenes, but not on the pages.

## Review a draft as changes

Someone sent back a whole draft instead of tracked changes? **Review a draft as changes…** (palette) brings the draft in as [tracked changes](track-changes.md) against your script, under the name you give (the file's name to begin with). The changes panel opens, so you can accept or reject each one.

Some things can't be brought in as tracked changes: styles, notes and boneyard, element settings such as centering and blank lines, and the title page. They're listed in a notice for you to apply by hand. The draft itself is never changed.

## From the command line

```
coldopen diff draft2.fdx draft3.fountain
coldopen diff draft2.fdx draft3.fdx --pdf changes.pdf
coldopen diff draft2.fdx draft3.fdx --json
```

It prints a summary of the changed scenes, and with `--pdf` writes the marked pages. It exits 0 whether or not the drafts differ.

## A known quirk

A section that sits just above a scene heading belongs, for comparing, to the scene before it. So moving a section together with the scene under it reads as the section moving between scenes, not as one moved scene. Accepting the changes still gives the right script.
