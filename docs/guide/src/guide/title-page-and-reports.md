# The title page and reports

## The title page

**Edit title page…** (Mod+Shift+T, or the palette) opens a sheet on the right. The page area shows the title page as it prints, and follows what you type.

**A Fountain script** has a title page of keys:

- Title, Credit, Author, Source, Draft date, Contact, Notes and Copyright are always there, empty or not, followed by any other keys the script has.
- Each field holds its values one per line: Enter starts another line.
- **Add a key…** adds one by name. Emptying a field removes its key.

**An .fdx script** has a title page of positioned lines. Each row is one line:

- its text;
- its alignment (Mod+L, Mod+E or Mod+R for left, center and right, or the button that steps through them);
- the space above it, in lines.

Enter in a line's text adds a line after it, and Mod+Shift+K removes the line.

**Moving about:** Tab and Shift+Tab move between fields. Each change is one undo step, labelled "Title page". Mod+Z in the sheet first undoes typing, then whole steps.

**Closing:** Esc, Done, or a click on the page.

Title page edits are never tracked or revision-marked.

## Reports

**Reports…** (Mod+Shift+I) opens three reports in place of the pages:

| Report | What it lists |
| --- | --- |
| **Scenes** | Every scene: its number, heading, page and length in eighths; interior or exterior, the place and the time of day; who speaks in it; and its words |
| **Characters** | Every speaking character: their speeches, printed lines and words of dialogue, the scenes they speak in, and the extensions used with their name, such as `V.O.` |
| **Locations** | Every place, alphabetically: the scenes in it, as interior, exterior or both, and at what times of day |

- Ctrl+Tab and Ctrl+Shift+Tab change reports.
- Up and Down choose a row, and Enter goes to it in the script: a scene, a character's first speech, a location's first scene.
- **Copy as CSV** (Mod+C) copies the report for a spreadsheet.
- Esc closes the reports.

From the command line:

```
coldopen report characters draft.fountain
coldopen report scenes draft.fdx --format csv > scenes.csv
coldopen report locations draft.fountain --format json
```

**How counting works:**

- A character is counted by their name in capitals, without extensions: `Maya`, `MAYA` and `MAYA (O.S.)` are one character.
- Lines are the printed lines of their dialogue, as you'd count them on the page, including those continued after a page break. Parentheticals don't count.
- `INT. LAB - NIGHT` and `EXT. LAB - DAY` are one location, in two settings.
