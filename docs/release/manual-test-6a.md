# Manual test: phase 6a, the alpha's packages

**Results of the first run (2026-10-31, the project owner, 83b44f7):**
- **The bench (§5) passed every budget**, quick and full. These two reports are the Windows gate, in `docs/progress/phase-6a-bench/`.
- **Four findings, all fixed:**
  - `coldopen bench` printed nothing in a terminal (§5);
  - the installer couldn't close a running Cold Open (§8);
  - the zip's folder had a space in its name (§9);
  - this list's own PowerShell commands were wrong (§9).
- **OneDrive (§7) is untested:** OneDrive is off and full on the test PC. The retest is §5 step 6, §8 and §9.

For the project owner, on the Windows PC. About 45 minutes.

**Before you start**

- Take the packages from the PR's latest CI run: Summary ▸ Artifacts ▸ `packages-windows-x64`. Or take them from the rehearsal draft release, once there is one.
- Unzip them in `C:\ColdOpenTest\packages`, outside the repository.
- Use copies of scripts in `C:\ColdOpenTest`, never the files in `corpus/`. For example, copy `corpus\fountain-io\Brick-&-Steel.fdx` to `C:\ColdOpenTest\Brick-&-Steel.fdx`.
- If an earlier test build is installed, note where. This test installs over nothing.

Record each result as **pass** or **fail**, with what you saw. A screenshot helps for anything that looks wrong.

## 1. Install

1. Double-click `ColdOpen-0.1.0-alpha.1-windows-x64-setup.exe`.
2. **Expected:**
   - SmartScreen may warn that the app is unrecognised (the build is unsigned). "More info" ▸ "Run anyway" gets past it.
   - No administrator (UAC) prompt.
   - The installer offers two tasks: "Add the coldopen command to PATH" ticked, and "Create a desktop shortcut" not ticked.
   - It installs to `%LOCALAPPDATA%\Programs\Cold Open`.
3. Leave "Start Cold Open" ticked and finish.
4. **Expected:** the app opens on a new script, a scene heading, with the stand-in icon in the taskbar and the window.

## 2. First run

1. Before step 1, if you can, rename `%APPDATA%\Cold Open` and `%LOCALAPPDATA%\Cold Open` to something else, so this really is a first run. Rename them back after the test.
2. **Expected:**
   - The start is quick (under a second).
   - No message about settings or leftover work.
   - `%APPDATA%\Cold Open` and `%LOCALAPPDATA%\Cold Open` exist afterwards, and nothing new appears anywhere else in your user folder.
3. Palette (Ctrl+K) ▸ "About Cold Open and licenses".
4. **Expected:**
   - The sheet shows "Cold Open 0.1.0-alpha.1", a commit, `x86_64-pc-windows-msvc`, and "drawn with CPU".
   - All four license tabs have text, the third-party one included (a long list of crates).
   - "Source code" opens the repository at that commit in your browser.
   - "Copy version info" puts four lines on the clipboard.

## 3. The command-line tool

1. Open a **new** terminal (PowerShell or cmd) and run `coldopen --version`.
2. **Expected:** `coldopen 0.1.0-alpha.1 (<commit>)`, the same commit as in About.
3. Run `coldopen export "C:\ColdOpenTest\Brick-&-Steel.fdx" C:\ColdOpenTest\bs.pdf`.
4. **Expected:** a PDF that opens.

## 4. File associations

1. In Explorer, right-click `C:\ColdOpenTest\Brick-&-Steel.fdx` ▸ Open with.
2. **Expected:** Cold Open is in the list. Choosing it opens the script in Cold Open.
3. Make a copy of any script with the extension `.fountain`, and right-click ▸ Open with.
4. **Expected:** Cold Open is listed.
5. In the app, Save as… your script as a project (`.coldopen`) in `C:\ColdOpenTest`, close the app, then double-click the `.coldopen` file.
6. **Expected:**
   - It opens in Cold Open without asking which app.
   - Its icon in Explorer is the stand-in.

## 5. The bench

1. Palette ▸ "Measure performance". Read the sheet, then Start.
2. **Expected:**
   - Windows open and close by themselves for about two minutes.
   - The sheet counts the steps, then shows the summary.
   - "Show the report" opens your Downloads folder, holding `coldopen-bench-windows-x86_64-<date>.json` and `.txt`.
3. Open the `.txt`.
4. **Expected:**
   - Every budget reads PASS.
   - Nothing in either file names you, this PC, a folder or a script.
5. **Send me both files.** They go into the final report as the Windows gate.
6. Also try `coldopen bench --quick` in a terminal.
7. **Expected:** the steps and the same summary printed in the terminal (fixed 2026-10-31: before, nothing was printed), and two more files in Downloads.

## 6. A folder that can't be written

1. Make the folder `C:\ColdOpenTest\locked`, and copy a script into it.
2. Right-click the folder ▸ Properties ▸ Security ▸ Edit… ▸ your user. Under **Deny**, tick **Write**, then OK, and Yes to the warning.
3. Open the script in Cold Open (Ctrl+O).
4. **Expected:**
   - A banner says the folder can't be written, so this is a copy.
   - The title shows "<name> (copy)".
   - Typing works.
   - Ctrl+S asks where to save.
   - The original and the locked folder are unchanged.
5. Remove the Deny entry afterwards.

## 7. A synced folder (OneDrive)

1. Copy a script into your OneDrive folder, and wait until OneDrive shows it as synced.
2. Open it in Cold Open, type a line, and save with Ctrl+S. Repeat a few times quickly.
3. **Expected:** every save works ("Saved"), with no error, and the file on OneDrive's website has the line after it syncs.
4. Right-click the script in Explorer ▸ "Free up space", so it is online only, then open it in Cold Open.
5. **Expected:** it opens; Windows downloads it first.

## 8. Upgrade and uninstall

1. Run the same installer again.
2. **Expected:**
   - It installs over the first one without asking for a folder.
   - Your settings (theme, recent files) are still there afterwards.
   - If Cold Open was open, the installer offers to close it, and does: it asks the app first, and ends it if that fails. Type a line in an untitled script before you run the installer. When Cold Open starts again, it offers that script back, line included.
3. Settings ▸ Apps ▸ Installed apps ▸ Cold Open ▸ Uninstall.
4. **Expected:**
   - `%LOCALAPPDATA%\Programs\Cold Open` is gone.
   - A new terminal says `coldopen` isn't recognised.
   - Cold Open is gone from "Open with".
   - `%APPDATA%\Cold Open` and `%LOCALAPPDATA%\Cold Open\Unsaved` are still there (your settings and unsaved scripts are never removed).

## 9. The zip

1. Unzip `ColdOpen-0.1.0-alpha.1-windows-x64.zip` to `C:\ColdOpenTest\zip`. It holds one folder, `ColdOpen` (no space since 2026-10-31).
2. Double-click `Cold Open.exe` in it.
3. **Expected:** it starts like the installed one.
4. In PowerShell, run `C:\ColdOpenTest\zip\ColdOpen\coldopen.exe --version`.
5. **Expected:** the version and commit.
6. `cd C:\ColdOpenTest\zip\ColdOpen`, then run `.\coldopen bench --quick`. PowerShell needs the `.\` for a program in the current folder.
7. **Expected:** it finds the app beside it, and prints the steps and the summary in the terminal.
