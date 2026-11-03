# Install

Download the file for your computer from the [download page](../download.md). Every download is under 15 MB. Cold Open needs no account and no internet connection, before or after you install it.

**What it runs on:**

- Windows 10 or 11, on x64 or ARM;
- macOS 12 or later, on an Apple chip or Intel;
- Linux on x86-64 or ARM64, with Wayland or X11, and glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later).

The builds aren't signed by Microsoft or notarized by Apple yet ([why](../code-signing.md)), so the first start shows a warning. Here's what to do on each system.

## Windows

1. Run `ColdOpen-…-windows-x64-setup.exe` (or `arm64` on Windows on ARM).
2. If SmartScreen says **"Windows protected your PC"**, click **More info**, then **Run anyway**. It shows for programs that aren't signed yet, not because something is wrong.
3. The installer needs no administrator rights. It installs for you alone, in `%LOCALAPPDATA%\Programs\Cold Open`, and adds a Start menu entry.

It also:

- puts the command-line tool `coldopen` on your PATH (a box you can untick). Open a new terminal to use it;
- offers Cold Open in **Open with** for `.fountain` and `.fdx` files, and opens `.coldopen` projects. Windows lets only you choose the default app for a type: right-click a file ▸ **Open with** ▸ **Choose another app**.

**Without installing:** unzip `ColdOpen-…-windows-x64.zip` anywhere and run `Cold Open.exe`. `coldopen.exe` is beside it.

**Upgrading:** run the new installer. If Cold Open is open, it asks to close it first. Nothing you typed is lost: it's all in the crash journal, and the next start brings it back.

**Uninstalling:** Settings ▸ Apps ▸ Cold Open ▸ Uninstall. Your scripts and settings are left alone.

## macOS

1. Open the `.dmg` and drag **Cold Open** to **Applications**.
2. Open Cold Open. The first time, macOS refuses, because the app isn't notarized.
3. Open **System Settings ▸ Privacy & Security**, scroll down to the message about Cold Open, and click **Open Anyway**. Then open Cold Open again and confirm.

   On macOS 12 to 14 you can instead Control-click the app in Applications and choose **Open**.
4. If macOS says the app **is damaged**, it's the quarantine flag on an unnotarized download. Run this in Terminal, then open it again:

   ```
   xattr -dr com.apple.quarantine "/Applications/Cold Open.app"
   ```

**The command-line tool:** in Cold Open, open the palette (Cmd+K) and choose **Install the command-line tool**. It puts a `coldopen` link in `/usr/local/bin`, or in `~/.local/bin` when that's the one you can write, and says which. It never asks for your password.

**A known issue:** double-clicking a script in Finder starts Cold Open but doesn't open the script yet. Use **Open** (Cmd+O) in the app.

**Uninstalling:** drag Cold Open from Applications to the Trash.

## Linux

Pick one of the three packages.

**.deb** (Debian, Ubuntu, Mint, Pop!\_OS):

```
sudo apt install ./cold-open_*_amd64.deb
```

This puts `coldopen-app` and `coldopen` in `/usr/bin`, with a menu entry and file types. Remove it with `sudo apt remove cold-open`.

**AppImage** (any distribution):

```
chmod +x ColdOpen-*-x86_64.AppImage
./ColdOpen-*-x86_64.AppImage
```

The command-line tool is inside: `./ColdOpen-*.AppImage cli --help`, or make a link named `coldopen` to the AppImage. An AppImage doesn't add a menu entry by itself; a tool such as AppImageLauncher can. To remove it, delete the file.

**Flatpak** (any distribution with Flatpak):

```
flatpak install --user ColdOpen-*-x86_64.flatpak
flatpak run io.github.theyungstump.coldopen
```

- It runs on the freedesktop 25.08 runtime. If Flatpak can't find it, add Flathub first: `flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo`.
- The Flatpak can open scripts in your home folder. For another drive, run `flatpak override --user --filesystem=/path/to/drive io.github.theyungstump.coldopen`.
- The command-line tool is `flatpak run --command=coldopen io.github.theyungstump.coldopen`. You can add `alias coldopen='flatpak run --command=coldopen io.github.theyungstump.coldopen'` to your shell's startup file.
- Remove it with `flatpak uninstall io.github.theyungstump.coldopen`.

On ARM64, the files say `arm64` or `aarch64` in place of `amd64` and `x86_64`.

## Check a download

Each release has `SHA256SUMS.txt`, which lists every file's SHA-256 checksum. A file that matches is exactly the one the release built. Put the checksum file beside your download, then:

- **Linux:** `sha256sum -c SHA256SUMS.txt --ignore-missing`
- **macOS:** `shasum -a 256 -c SHA256SUMS.txt --ignore-missing`
- **Windows** (PowerShell): `Get-FileHash .\ColdOpen-…-setup.exe`, then compare the hash it prints with that file's line in `SHA256SUMS.txt`.

"OK" (or matching hashes) means the download is good. Anything else: download it again, and if it still doesn't match, please [tell us](../alpha/feedback.md).

## Where Cold Open keeps things

Uninstalling never touches these:

- **Settings** (`settings.json`, `keymap.json`, `flow.json`): `%APPDATA%\Cold Open` on Windows, `~/Library/Application Support/Cold Open` on macOS, `~/.config/coldopen` on Linux.
- **Untitled scripts, crash reports and other app data:** `%LOCALAPPDATA%\Cold Open` on Windows, the same folder as the settings on macOS, `~/.local/share/coldopen` on Linux.
- **Each script's crash journal** sits beside the script while it's open (see [Recovery](recovery.md)).
