# Manual test: phase 6c (0.1.0-alpha.3)

For the project owner, on Windows, with the alpha.3 rehearsal's installer (the draft release `rehearsal-<run>`, or the CI artifacts). About 20 minutes. Note anything that surprises you; "pass" or "fail" with a line is enough for each step.

Mod is Ctrl on Windows and Linux, Cmd on a Mac.

## 1. Install over alpha.2

1. Install `ColdOpen-0.1.0-alpha.3-windows-x64-setup.exe` over your alpha.2. Your settings and recent files are still there.
2. The taskbar and Start menu show the **new icon** (the blue C around three lines of a script).

## 2. Settings, by keyboard alone

1. Open a script. Press **Mod+Comma** (Ctrl and the comma key, just right of M; not the plus key, which zooms). The **Settings** sheet opens, the Theme row outlined.
2. **Right** changes the theme at once; **Left** puts it back.
3. **Down** to Interface size, **Right**: the whole window grows. **Left** puts it back.
4. **Down** to Crash reports, **Space**: it switches. **Space** again: back.
5. **Down** to Usage summary: **Space** turns counting off and on. **Tab**, then **Enter**: the usage summary opens.
6. **Esc** closes each sheet. Nothing needed the mouse.

## 3. The usage summary

1. Type a few lines, save, then open **View usage summary** (palette, or About ▸ Usage summary…).
2. It reads in plain words: since today, the app version, sessions, what drew the window, the formats you opened and saved, and the commands you used with counts.
3. **Check that it holds none of your text, no character or scene names, and no file names or folders.**
4. **C** copies it. Paste it somewhere: it's the same text.
5. **R**, then **Esc**: nothing is reset. **R**, then **Enter**: it's reset; viewing it again says there's no summary yet, or only what you've done since.
6. **O** turns counting off; the sheet says so. Type and save; view it again: nothing changed. **O** turns it back on.

## 4. Share feedback, with the summary: the end-to-end test

1. Palette ▸ **Share feedback…**. The text box has the keyboard. Type "Test from the 6c manual test".
2. **Esc** leaves the box. **U** ticks **Include my usage summary**: the whole summary appears under it.
3. **F** (or the **Open the form** button): your browser opens the form with:
   - your text in the details box;
   - the version, OS and renderer filled in;
   - the usage summary in the report box, **shown before anything is sent**, and with no text, names or paths.
4. **Submit the form.** Then check it **arrived** in the form's responses (Google Forms ▸ Responses), with the summary as it was shown.
5. Back in Cold Open, Share feedback… again: the box is empty. Type something, **Esc**, **G**: a GitHub issue opens with the feedback template, your text and, if ticked, the summary. Close it without submitting.
6. **Report a problem…**: it has the same tick (**U**); **C** copies the details, and the summary when ticked.

## 5. Agents' settings still work

1. Settings ▸ **Agents can attach**: switching it says agent access is on or off, as the palette's command does.

## 6. The website on real devices (optional, any time)

1. On this PC, open <https://theyungstump.github.io/cold-open/download/>: **Recommended for your computer** names the Windows installer for your processor.
2. On a phone: no recommendation, and a note that Cold Open runs on computers.
3. On a Mac, if you can borrow one, in Safari: the universal disk image is recommended.

## 7. Report

Add the results to `docs/progress/phase-6c.md` under "Manual test", or tell me and I'll record them.
