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

## 5. Send the file

Reply to the message you got with **both files attached** (the `.json` file alone is enough if that's easier). If anything above didn't go as described, say which step and what you saw; a screenshot helps.

Thank you!
