# Testing Cold Open on your Mac

Thank you for helping. This takes about ten minutes, two of them waiting. You need macOS 12 (Monterey) or later, on any Mac.

## 1. Download

Open the release page you were sent. Under **Assets**, download the file ending in **`.dmg`**:

- `ColdOpen-…-macos-universal.dmg` if there is one; it works on every Mac;
- otherwise `…-macos-arm64.dmg` for an Apple chip (M1, M2, M3, M4…), or `…-macos-x64.dmg` for an Intel Mac. To check which you have:  ▸ About This Mac shows **Chip** for Apple, **Processor** for Intel.

## 2. Install

1. Double-click the `.dmg`. A window opens with **Cold Open** and **Applications**.
2. Drag **Cold Open** onto **Applications**. Then close the window, and eject the disk ("Cold Open") in Finder's sidebar.

## 3. Allow it to open

Cold Open isn't notarized by Apple yet, so macOS stops it the first time. This is expected.

1. Open **Applications** and double-click **Cold Open**. macOS says it can't check it for malicious software. Click **Done** (or **OK**). Don't move it to the Bin.
2. Open  ▸ **System Settings** ▸ **Privacy & Security**. Scroll down to **Security**. You'll see a line about "Cold Open" being blocked. Click **Open Anyway**, then confirm with your password or Touch ID.
3. Double-click **Cold Open** again, and click **Open** if asked. It opens on an empty page.

On macOS 12, 13 or 14 you can do this instead: Control-click **Cold Open** ▸ **Open** ▸ **Open**.

If macOS says Cold Open **is damaged**, open **Terminal** (Applications ▸ Utilities), paste this line, press Return, then try again:

```
xattr -dr com.apple.quarantine "/Applications/Cold Open.app"
```

## 4. Run the measurement

1. In Cold Open, press **⌘K**, type **measure**, and press Return.
2. Read the window, then click **Start**. For about two minutes, windows will open and close by themselves. Please don't type or click meanwhile.
3. When it says **Performance measured**, click **Show the report**. Finder opens your **Downloads** folder at two files, named like `coldopen-bench-macos-aarch64-20261102-141500.json` and `.txt`.

The files hold only your Mac's model names (macOS version, chip, graphics), its screen size and the times. They have no name, no folder paths and nothing you wrote.

## 5. A crash, on purpose, and its report (alpha.2 and later)

This checks that a crash loses nothing, and that the report Cold Open saves holds nothing private.

1. In Cold Open, type a line of your own on the empty page, such as `Zorvanta walks in.`
2. Press **⌘K**, type **crash**, choose **Crash on purpose (to test recovery)**, and press Return. Cold Open closes at once. That's expected.
3. Open Cold Open again. A bar offers your unsaved work: open it, and choose **Keep**. Is your line back?
4. A notice says Cold Open closed unexpectedly and saved a crash report. Click **Look at it** and read the report. It should hold no words from your line, no name, and no folders.
5. Click **Copy**, and paste the report into your reply (step 7).

## 6. Report a problem (alpha.2 and later)

1. Press **⌘K**, type **report**, and choose **Report a problem…**.
2. The sheet lists the version, your macOS, and the renderer, and nothing else. Press **G**. Your browser should open a GitHub page with those filled in. You don't need to submit it: just say whether it opened, and close it.
3. Open **Report a problem…** again and press **F**. A feedback form should open, with **Bug** chosen and the same details filled in. Again, just say whether it did.

## 7. Send the files

Reply to the message you got with **both measurement files attached** (the `.json` file alone is enough if that's easier), and the crash report you copied in step 5. If anything above didn't go as described, say which step and what you saw; a screenshot helps.

Thank you!
