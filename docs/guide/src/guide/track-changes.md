# Track changes and agents' edits

Track changes records what each person inserts and deletes, and when, so you can accept or reject each change. Deleted text isn't gone: it stays on the page, struck through, until someone settles it.

It's separate from [production revisions](revisions.md), which mark what changed between drafts a production has sent out.

## Tracking your own changes

**Track my changes** (palette, or click the hint bar's text) turns tracking of your edits on or off. While it's on, the hint bar says "Tracking your changes".

The first time, Cold Open asks what name your changes should carry ("Writer" until you choose). **Change my name…** in the changes panel, or "Change my name for changes…" in the palette, changes it later.

**What you see:**

- Insertions in their author's color, deletions struck through in that color. Each author gets their own, in the order they first appear.
- A thin bar in the margin beside every changed line.
- A label in the margin for changes to whole elements: "New", "Deleted", "Was Action" for an element whose type changed, "Split", "Joined", and "Moved" on a scene that moved.
- A dot in the navigator beside each scene with changes.
- "N to review" in the status bar.

## Accepting and rejecting

| To | Do |
| --- | --- |
| Accept the change at the caret | Mod+Shift+Enter |
| Reject the change at the caret | Mod+Shift+Backspace |
| Go to the next or previous change | F7, Shift+F7 |
| Accept or reject everything in this scene | the palette, or the changes panel |
| Accept or reject everything | the palette, or the panel's Accept all and Reject all (they ask first) |

Each is one undo step: Mod+Z gives the change back.

## The changes panel

**Mod+Shift+R** opens the panel beside the pages. It lists every pending change in script order, with who made it, when, what it does, which scene it's in, and a short excerpt.

- Click a change, or choose it with Up and Down and press Enter, to go to it.
- **A** accepts it and **R** rejects it, then the next is chosen.
- Filter by author with the list at the top.
- Esc gives the keyboard back to the page.

## Agents' edits

When an AI agent edits your script through [Cold Open's agent server](agents.md), its edits are **tracked changes under the agent's name**, by default. You review them like notes from a collaborator: accept what you like, reject the rest.

- An agent can withdraw its own changes, but it can never accept a change, or reject anyone else's.
- **Track agent edits** (palette) turns this off for the script you're in. Then agents' edits go straight in, marked with a violet bar until you dismiss the marks.

## Saving changes

- **A [project](files.md)** keeps every change, with its author and time.
- **Fountain and .fdx can't hold tracked changes.** Saving one with changes pending asks first, and offers a project. **Save anyway** accepts every pending change, then saves, and keeps the version with the changes marked in [Recovered versions](recovery.md#recovered-versions).
- **A PDF** prints the changes marked, in their authors' colors. "Print the current text" in the export form prints them accepted.

## Bringing in someone else's draft

A collaborator sent a whole new draft rather than tracked changes? **Review a draft as changes…** (palette) brings it in as tracked changes against your script, under the name you give. See [Compare drafts](compare.md#review-a-draft-as-changes).
