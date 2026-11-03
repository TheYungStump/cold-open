# Manual test: phase 6b, the alpha

For the project owner, on the Windows PC. About 40 minutes.

**Before you start**

- **The packages:** take them from the PR's latest CI run (Summary ▸ Artifacts ▸ `packages-windows-x64`). Put them in `C:\ColdOpenTest\packages`, outside the repository. The rehearsal draft ("Rehearsal: Cold Open 0.1.0-alpha.2") was built from 128a74a, before the feedback form was wired in, so its app has no **Use the form** button: use the draft for §6 only.
- **Scripts:** use copies in `C:\ColdOpenTest`, never the files in `corpus/`.
- **The website:** if GitHub Pages isn't on yet, §1 uses the local preview instead: run `mdbook serve docs/guide`, or ask Claude to start it, and open <http://localhost:3000>.

Record each result as **pass** or **fail**, with what you saw. A screenshot helps for anything that looks wrong.

## 1. The website

1. Open the site: <https://theyungstump.github.io/cold-open/>, or the local preview.
2. **Expected:**
   - Home, Download, the user guide, Join the alpha, Code signing policy, Privacy and Contribute are all in the sidebar, and each opens.
   - Every page's footer links to the code signing policy and privacy.
   - Download names the latest release and its files. Locally, or before any release is published, it points to the releases page instead.
3. **Keyboard only:** reload a page and press Tab once.
   - **Expected:** "Skip to the content" appears at the top left. Enter on it moves focus past the sidebar. Every link and button shows a clear focus ring as you Tab on.
   - On a narrow window, Tab reaches the sidebar's ☰ toggle, and Enter opens and closes the sidebar.
4. **Light and dark:** switch Windows between light and dark mode (Settings ▸ Personalization ▸ Colors), then reload with a fresh tab.
   - **Expected:** the site follows, with everything readable in both, code blocks included.
5. **Search:** press S, type "recovery".
   - **Expected:** results, and Enter opens one.
6. Skim one page you know well, such as Getting started or Track changes. Note anything wrong or unclear: these pages are what testers will read.

## 2. Install alpha.2

1. Run `ColdOpen-0.1.0-alpha.2-windows-x64-setup.exe` over the alpha.1 test build, if one is installed.
2. **Expected:** as in 6a: SmartScreen's "More info" ▸ "Run anyway", no administrator prompt, and Cold Open opens.
3. **The file details:** in `%LOCALAPPDATA%\Programs\Cold Open\bin`, right-click `coldopen.exe` ▸ Properties ▸ Details.
   - **Expected:** Product name "Cold Open", product version `0.1.0-alpha.2`. The same for `Cold Open.exe` one folder up.
4. Palette (Ctrl+K) ▸ **About Cold Open and licenses**.
   - **Expected:** "Cold Open 0.1.0-alpha.2", with the commit of the CI run you took the packages from.

## 3. The docs links in the app

1. The start screen (close any open script with Ctrl+W).
   - **Expected:** a quiet row under the buttons: **User guide**, **Report a problem…**, **About Cold Open**.
2. Click **User guide**.
   - **Expected:** your browser opens the guide's first page, "About this guide". Before Pages is on, it opens GitHub's 404 page: that's expected, so note it and go on.
3. About Cold Open:
   - **Expected:** the row shows **Source code**, **User guide**, **Report a problem…** and **Copy version info**, and each does what it says.
4. Palette ▸ "Open the user guide".
   - **Expected:** the same page as step 2.
5. Palette ▸ **Make the interface larger**, twice.
   - **Expected:** "Interface size: 125%." The whole window grows: menus, dialogs and the page. Open Report a problem: everything is still inside its box.
6. Close and start Cold Open again.
   - **Expected:** still at 125%. Palette ▸ **Interface at normal size** brings it back to 100%.

## 4. Report a problem

1. Palette ▸ **Report a problem…**.
2. **Expected:** a sheet showing the version and commit, target, OS and renderer, and saying nothing is sent until you submit. No script text, no paths, no names. The buttons are **Open a GitHub issue**, **Use the form (no account)**, **Copy the details** and **Close**, all inside the box (Close ran out of it before 2026-11-03).
3. Press **C**.
   - **Expected:** "The details were copied." Paste into Notepad: the four lines, nothing else.
4. Open it again and press **G**.
   - **Expected:** your browser opens a new GitHub issue from the "Something doesn't work" template, with Version, Operating system and Renderer filled in.
5. **Submit a test issue** (this is the end-to-end check): title it "Test: Report a problem (please ignore)", write "Manual test of 6b" in What happened, tick the box and submit.
   - **Expected:** the issue appears, and holds no script text, no path and no name beyond what you typed.
   - Close it afterwards.
6. Open it again and press **F**, in a private browser window if you can, signed out of Google.
   - **Expected:** the feedback form, with **Bug** chosen and Version, Operating System and Renderer filled in. It doesn't ask you to sign in.
7. **Submit a test answer:** write "Manual test of 6b" in What happened and submit.
   - **Expected:** it appears under the form's Responses, holding only what the page showed. Delete it afterwards (Responses ▸ ⋮ ▸ Delete all responses).

## 5. A crash report

1. Open a copy of a script, or start a new one, and type a line with a word you'd recognise, such as "Zorvanta".
2. Palette ▸ **Crash on purpose (to test recovery)**.
   - **Expected:** a question saying Cold Open will close at once, as one sentence with no gap in it. **Esc** cancels and nothing happens.
3. Do it again and press **Enter**.
   - **Expected:** Cold Open closes at once.
4. Start Cold Open again and open the script (from the recent files, or the bar offering unsaved work).
   - **Expected:**
     - the recovery dialog brings back your line, "Zorvanta" included; choose **Keep**;
     - a notice: "Cold Open closed unexpectedly. A crash report was saved, with no script text.", with **Look at it** and **Never ask again**.
5. Click **Look at it**.
   - **Expected:** the whole report:
     - Version, Target, OS, Renderer, Time, Thread, `Last action: app.crash_test`;
     - `Location: crates/app/src/crash.rs:…`;
     - `Message: Crash on purpose, from the palette`;
     - a backtrace of lines like `Cold Open.exe+0x…`.

     Search it for "Zorvanta", your user name and your folders: none should be there.
6. Press **G** (Send on GitHub).
   - **Expected:** "The whole report is on your clipboard…" and a new issue from the crash template, with Version, OS, Renderer and "Where it crashed" filled in.
7. Paste the report into "The whole report" and submit, titled "Test: crash report (please ignore)".
   - **Expected:** the issue holds the report, and nothing private. Close it afterwards.
8. Back in Cold Open, press **F** (Send with the form).
   - **Expected:** the form, with **Crash** chosen, the details filled in, and the crash report box holding the one-line summary. Paste the whole report over it, write "Manual test of 6b" in What happened, and submit. Delete the response afterwards, as in §4.
9. Close the sheet, then palette ▸ **Look at crash reports**, then **Delete** and Enter.
   - **Expected:** the report goes.
10. Crash on purpose again, start again, and click **Never ask again** on the notice.
    - **Expected:** "Crash reports are off…".
11. Crash on purpose once more and start again.
    - **Expected:** no notice, and Look at crash reports shows nothing new.
12. Turn them back on: palette ▸ **Turn crash reports on or off**.
    - **Expected:** "Crash reports are on…".

## 6. The rehearsal release

In the draft release (just its page, and one download to check: don't install from it):

- **Expected:**
  - every package from 6a, now named `0.1.0-alpha.2`;
  - `ColdOpen-0.1.0-alpha.2-symbols.zip` and `SHA256SUMS.txt`;
  - in the notes: What's new, the file table with sizes, the line saying the symbols file isn't for installing, and links to the code signing policy and privacy.
- **Check a download** as the guide says: in PowerShell, `Get-FileHash .\ColdOpen-0.1.0-alpha.2-windows-x64-setup.exe`. **Expected:** the hash matches its line in `SHA256SUMS.txt`.

Please send the Mac tester the updated `docs/release/mac-tester.md`. It now has the crash report and Report a problem steps.

## Results

| § | Result | Notes |
| --- | --- | --- |
| 1 The website | | |
| 2 Install alpha.2 | | |
| 3 The docs links | | |
| 4 Report a problem | | |
| 5 A crash report | | |
| 6 The rehearsal release | | |
