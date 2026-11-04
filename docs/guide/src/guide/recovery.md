# Your work is safe: recovery

Cold Open is built so that a crash, a power cut or a damaged file never costs you more than the last moment of typing.

## The crash journal

While a script is open, every key you press is written to a **crash journal** before it shows on screen, and the journal reaches the disk within a quarter of a second.

- **If Cold Open crashes,** or is closed from outside (Task Manager, Force Quit), nothing that was on screen is lost.
- **If the power goes,** you lose at most the last quarter of a second.
- **The next time you open the script,** Cold Open puts back everything since your last save, and your undo history with it.

The journal is a hidden file next to the script: `.lighthouse.fountain.coldopen-journal` for `lighthouse.fountain`. Saving starts it again, and closing a saved script deletes it. If you keep scripts in git, add `.*.coldopen-journal` to your `.gitignore`.

An **untitled script** has a journal too, in Cold Open's own folder, so even a script you never saved is offered back at the next start.

**The status bar** shows the journal's state: "Protected" when everything is on disk. If the disk ever can't keep up, it says "Protecting…", and if writing fails it says "Not protected" with the reason, and won't take more edits until it can protect them again.

## Getting your work back

When Cold Open opens a script and finds unsaved work in its journal, it says so:

- how many changes came back, and when the last was made;
- which scenes they touched;
- a warning if another program changed the file meanwhile.

**Keep** (Enter or K) carries on with the recovered script, unsaved, so you can look it over before saving. **Discard** (D) goes back to the file as it was saved, and keeps the recovered work aside in Recovered versions, in case you change your mind.

**Work from another computer.** A journal that came from another computer, through a synced folder say, is never applied by itself. Cold Open asks first, naming the computer: **Recover them** or **Set them aside**.

## Saving is safe too

Every save writes a new file next to the old one, makes sure it's on the disk, then puts it in the old one's place in one step. A crash in the middle of a save leaves either the old file or the new one, never half of each. Saving happens in the background, so typing never waits for it.

## Recovered versions

Cold Open never throws work away. When it sets something aside, it keeps it in **Recovered versions** (palette, or the start screen):

- unsaved changes you chose not to save ("Don't save" or "Discard");
- the version with tracked changes or revision marks, when a save to Fountain or .fdx took them out;
- a damaged file, as it was before Cold Open repaired it;
- a journal that couldn't all be replayed.

The screen lists them newest first, with a preview. **Restore** (Enter or R) opens one as a new untitled script, with nothing overwritten. **Delete** removes one, after asking.

A copy is cleared away 30 days after you first saw it in the list. One you've never seen is kept, however old.

## Crash reports

If Cold Open crashes, it also saves a short **crash report** on your computer. The report holds no script text, file names or user names. At the next start it offers to show it to you: **Look at it** shows the whole report, and you can send it to us if you like, or not. Nothing is sent unless you send it yourself. See [Privacy](../privacy.md).

**Crash on purpose (to test recovery)** in the palette lets you try all of this safely: it closes Cold Open as a crash would. Open it again and your work comes back, with a crash report to look at.
