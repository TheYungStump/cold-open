# Release builds and packages

- **Date:** 2026-10-30
- **Status:** phase 6a, being implemented.
- **Sources:**
  - `docs/spec.md`, "Platforms, size and performance" (targets, the 15 MB installer budget) and "Open questions" (code signing: ship unsigned, apply to SignPath Foundation, pay Apple later).
  - The phase 6a brief and the project owner's decisions of 2026-10-30, recorded in `docs/progress/phase-6a.md`.
  - `docs/specs/editor.md` §11 (the app's folders), `docs/specs/journal.md` (journals next to scripts), `docs/specs/mcp.md` §5 (the agent link's run folder).
  - Public packaging documentation, read for how each tool works, never copied: the Inno Setup documentation (per-user installs, `PrivilegesRequired`, architecture identifiers); Apple's documentation for `Info.plist` document types and uniform type identifiers, `codesign`, `hdiutil` and `lipo`; the freedesktop.org Desktop Entry, Shared MIME-info and AppStream specifications; the Debian Policy Manual (binary package layout, `dpkg-shlibdeps`); the AppImage and Flatpak documentation; SignPath's documentation for its GitHub action; the `cargo-about` README.
- Nothing here comes from Final Draft's software, installers or help. The file extension `.fdx` is a public file format name. A choice where the sources leave room is marked **Resolution**.

## 1. Goals

1. **Every target in the spec** installs and runs: Windows 10 and 11 on x86-64 and ARM64, macOS 12 and later on Apple Silicon and Intel, Linux on x86-64 and ARM64 (Wayland and X11) as AppImage, Flatpak and .deb.
2. **Small.** Every file a writer downloads to install is under 15,000,000 bytes, fonts included. CI measures each one and fails the build when one is over (§7).
3. **No cost and no account.** Builds are unsigned or signed ad hoc until a free or paid signature is switched on (§8). No installer asks for administrator rights except the .deb, which the system's package manager installs.
4. **One version** from `Cargo.toml` in every program, file and report (§9).
5. **Licenses travel with every build** (§10).
6. **The command-line tool comes with the app** on every OS, where a writer can run it (§4).

## 2. Targets and files

A release `v<version>` holds these files. `<v>` is the version without the `v`, such as `0.1.0-alpha.1`.

| OS | Architecture | File | What it is |
| --- | --- | --- | --- |
| Windows | x86-64 | `ColdOpen-<v>-windows-x64-setup.exe` | Per-user installer (§3.1) |
| Windows | x86-64 | `ColdOpen-<v>-windows-x64.zip` | The same files, unzipped anywhere (§3.2) |
| Windows | ARM64 | `ColdOpen-<v>-windows-arm64-setup.exe` | Per-user installer |
| Windows | ARM64 | `ColdOpen-<v>-windows-arm64.zip` | Zip |
| macOS | Apple Silicon and Intel | `ColdOpen-<v>-macos-universal.dmg` | Disk image holding `Cold Open.app` (§3.3) |
| Linux | x86-64 | `cold-open_<v>_amd64.deb` | Debian and Ubuntu package (§3.4) |
| Linux | x86-64 | `ColdOpen-<v>-x86_64.AppImage` | Single-file app (§3.5) |
| Linux | x86-64 | `ColdOpen-<v>-x86_64.flatpak` | Flatpak bundle (§3.6) |
| Linux | ARM64 | `cold-open_<v>_arm64.deb`, `ColdOpen-<v>-aarch64.AppImage`, `ColdOpen-<v>-aarch64.flatpak` | The same three |
| All | | `SHA256SUMS.txt` | One line per file above, `<sha256>  <name>`, sorted by name |

**Resolution (macOS):** one universal disk image if it stays under the size budget. If it doesn't, the release holds `ColdOpen-<v>-macos-arm64.dmg` and `ColdOpen-<v>-macos-x64.dmg` instead. The packaging step measures the universal image and decides; the release notes name what was shipped.

**Resolution (Debian versions):** a `-` in a Cargo pre-release becomes `~` in the .deb's own version field (`0.1.0~alpha.1`), so that apt sorts the alpha before `0.1.0`. The file name keeps `<v>`.

**Building.** Every file is built by CI on GitHub's hosted runners, from the commit the tag names:

- **Windows:** x86-64 on `windows-latest`; ARM64 natively on `windows-11-arm`.
- **macOS:** both architectures on `macos-latest`, joined with `lipo`.
- **Linux:** x86-64 on `ubuntu-22.04`; ARM64 natively on `ubuntu-22.04-arm`.

**Resolution:** Linux builds use the oldest supported Ubuntu runner, so the binaries need glibc 2.35 or newer and run on current Debian, Ubuntu, Fedora and Arch. Every build is `cargo build --release --locked` with the release profile in the root `Cargo.toml`.

## 3. Packages

Each package holds the desktop app, the command-line tool `coldopen` (which also runs `coldopen mcp`), the license folder (§10) and a short readme.

The app's identifier is `io.github.theyungstump.coldopen` everywhere an OS asks for one (macOS bundle id, Flatpak app id, desktop file and AppStream id). The stand-in icon is replaced by the project owner's artwork when it arrives. Until then the packages carry a plain stand-in in the same places.

### 3.1 Windows installer

- **Tool:** Inno Setup 6, compiled in CI. The script is `.github/packaging/windows/coldopen.iss`.
- **No administrator rights:** `PrivilegesRequired=lowest`, with the dialog that offers an all-users install turned off.
- **Where it installs:** `%LOCALAPPDATA%\Programs\Cold Open`, which is the per-user default:
  - `Cold Open.exe`
  - `bin\coldopen.exe`
  - `licenses\`
  - `README.txt`
- **Architecture:** the x64 installer runs on x64 Windows. The ARM64 installer runs only on ARM64 Windows. **Resolution:** an x64 build on ARM64 Windows would run under emulation, and the ARM64 build is better there, so each installer refuses the other architecture.
- **Shortcuts:** a Start menu entry, and an optional desktop shortcut, off by default.
- **The CLI on PATH:** a task, ticked by default, adds `<install>\bin` to the user's `Path` (HKCU\Environment). Uninstalling takes it out again. Programs started after the install see it.
- **File associations** (§5), under `HKCU\Software\Classes`.
- **Uninstall** (Settings ▸ Apps) removes the installed files, shortcuts, PATH entry and associations. It never touches the writer's folders (§6), scripts, journals or set-aside copies.
- **Upgrades:** a newer installer over an older one replaces the files in place and keeps the settings. If the app is running, the installer asks the writer to close it first.

### 3.2 Windows zip

The same files as the installer, in one folder: `Cold Open.exe`, `coldopen.exe` (next to it, not in `bin`), `licenses\` and `README.txt`. It has no shortcuts, associations or PATH entry. The readme says how to add the folder to PATH. The app keeps its settings in the usual folders (§6). **Resolution:** a portable mode that keeps settings beside the exe is not in 6a; `COLDOPEN_HOME` already gives one for those who want it.

### 3.3 macOS disk image

- `Cold Open.app`, with an alias to `/Applications` and `Read me first.txt`, compressed with `hdiutil` in the `ULMO` format, which macOS 10.15 and later open.
- **The bundle:**
  - `Contents/MacOS/coldopen-app`, the app;
  - `Contents/MacOS/coldopen`, the CLI;
  - `Contents/Resources/licenses/`;
  - `Contents/Info.plist`, with `LSMinimumSystemVersion` 12.0, the version (§9), document types (§5) and the icon.
- **Signature:** both binaries, then the bundle, are signed ad hoc (`codesign --sign -`). Without that, Apple Silicon refuses to run the app at all. The image is not notarized (§8). The release notes, and `docs/release/mac-tester.md`, say how to allow it.
- **The CLI:**
  - The app's palette has **Install the command-line tool** (macOS only). It makes a symbolic link named `coldopen`, pointing at the bundle's CLI, in `/usr/local/bin` if that folder exists and the writer can write it, otherwise in `~/.local/bin`. It then says where it put it, and, if that folder isn't on the writer's `PATH`, the line to add to `~/.zprofile`.
  - It never asks for a password and never overwrites a `coldopen` that isn't already a link to a Cold Open bundle.
  - The readme gives the same `ln -s` by hand.

### 3.4 Debian package

- **Built with `dpkg-deb`**, with dependencies computed by `dpkg-shlibdeps` from the binaries. The .deb is the one package that installs system-wide, through the system's package manager.
- **Layout:**
  - `/usr/bin/coldopen-app`
  - `/usr/bin/coldopen`
  - `/usr/share/applications/io.github.theyungstump.coldopen.desktop`
  - `/usr/share/mime/packages/io.github.theyungstump.coldopen.xml`
  - `/usr/share/metainfo/io.github.theyungstump.coldopen.metainfo.xml`
  - `/usr/share/icons/hicolor/<size>/apps/io.github.theyungstump.coldopen.png`
  - `/usr/share/doc/cold-open/copyright`, in machine-readable form, naming the licenses
  - `/usr/share/doc/cold-open/licenses/`
- **Desktop entry:** `Exec=coldopen-app %f`, the MIME types of §5, and `Categories=Office;WordProcessor;`.

### 3.5 AppImage

- **Built with `appimagetool`**, a release pinned by its SHA-256, from an AppDir holding:
  - `usr/bin/coldopen-app`
  - `usr/bin/coldopen`
  - the desktop entry, icon and metainfo
  - `AppRun`
  - the licenses
- **`AppRun`:**
  - runs the CLI when the AppImage is started through a link named `coldopen`, or with `cli` as its first argument (`ColdOpen.AppImage cli --version`);
  - otherwise runs the app.
- The readme gives the `ln -s` line.
- **Resolution:** the AppImage doesn't install a desktop entry or file associations itself. Tools such as AppImageLauncher do that, and the readme says so.

### 3.6 Flatpak

- **A bundle file** (`flatpak build-bundle`), built with `flatpak-builder` from `.github/packaging/linux/io.github.theyungstump.coldopen.yml`, on the `org.freedesktop.Platform` runtime.
- **Resolution:** the manifest installs the same release binaries the other Linux packages hold, so all three run identical code. Flathub requires a build from source; that is for a later phase.
- **Permissions:**

  | Permission | Why |
  | --- | --- |
  | `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc` | The window. |
  | `--device=dri` | The GPU renderer. |
  | `--filesystem=home` | The project owner's decision (2026-10-30). A script's crash journal and atomic save need its folder, not just the file a portal hands over (`docs/specs/journal.md`). Scripts outside the home folder need an override (Flatseal, or `flatpak override --user --filesystem=/path`), and the release notes say so. |
  | `--filesystem=xdg-run/coldopen:create` | The agent link's run folder (`docs/specs/mcp.md` §5), so a `coldopen mcp` outside the sandbox can reach the app inside it. |

- **The CLI** is `flatpak run --command=coldopen io.github.theyungstump.coldopen`. The release notes give an alias line.

## 4. The command-line tool on each OS

| Package | Where `coldopen` is | How a writer runs it |
| --- | --- | --- |
| Windows installer | `<install>\bin\coldopen.exe` | On the user's PATH (§3.1); a new terminal finds it |
| Windows zip | Next to `Cold Open.exe` | By its path, or after adding the folder to PATH |
| macOS | `Cold Open.app/Contents/MacOS/coldopen` | **Install the command-line tool** in the app, or `ln -s` (§3.3) |
| .deb | `/usr/bin/coldopen` | On PATH |
| AppImage | Inside the image | A `coldopen` link to the AppImage, or `ColdOpen.AppImage cli` |
| Flatpak | Inside the sandbox | `flatpak run --command=coldopen io.github.theyungstump.coldopen` |

`coldopen --version` prints `coldopen <v> (<commit>)` on every one (§9).

## 5. File associations

| Extension | Type | Description | Linux MIME type | macOS type |
| --- | --- | --- | --- | --- |
| `.fountain` | Fountain screenplay | Plain text | `text/x-fountain` (a subclass of `text/plain`) | imported `io.fountain.fountain`, conforming to `public.plain-text` |
| `.fdx` | Final Draft–compatible XML | XML | `application/x-fdx` (a subclass of `application/xml`) | imported `com.finaldraft.fdx`, conforming to `public.xml` |
| `.coldopen` | Cold Open project | Zip | `application/x-coldopen` (a subclass of `application/zip`) | exported `io.github.theyungstump.coldopen.project`, conforming to `public.zip-archive` |

- **Resolution:** on macOS, the identifiers for the two formats we don't own are declared as *imported* types, the form an app uses for formats another party defines. The identifier strings are the conventional ones for those extensions. A Mac that already knows the type keeps the definition it has.
- **Windows:**
  - Each extension's `OpenWithProgids` gains `ColdOpen.fountain`, `ColdOpen.fdx` or `ColdOpen.coldopen`.
  - Each ProgId opens with `"<install>\Cold Open.exe" "%1"`.
  - **Resolution:** Windows 10 and 11 let only the writer choose a default app. So the installer offers Cold Open in "Open with" for all three, and makes it the default for `.coldopen` only, which no other program claims.
- **How the file arrives:**
  - **Windows and Linux** pass the file as the program's first argument, which the app already opens (`coldopen-app [script]`).
  - **macOS** sends a document a writer opens from Finder to the running app as an "open documents" event, not as an argument. The app must handle that event. If the windowing library doesn't expose it, it is a known issue in the release notes: the app starts but doesn't open the file. It isn't fixed with unsafe code without the project owner's approval.

## 6. First run

**Folders.** A first start creates only the folders in `docs/specs/editor.md` §11, under the user's own folders, and nothing else:

- Windows: `%APPDATA%\Cold Open`, `%LOCALAPPDATA%\Cold Open`, and `run` in the latter.
- macOS: `~/Library/Application Support/Cold Open`, with `run`.
- Linux: `$XDG_CONFIG_HOME/coldopen`, `$XDG_DATA_HOME/coldopen`, and `$XDG_RUNTIME_DIR/coldopen`.

No package writes a setting at install time, so a new install over nothing starts with the defaults, and a reinstall keeps the writer's settings.

**Renderer.**
- The default (`auto`) is the CPU renderer on Windows and Linux and the GPU renderer on macOS (`docs/specs/editor.md`).
- A writer who picks the GPU renderer on a computer whose GPU or driver can't start it gets the CPU renderer instead: the app asks iced for `wgpu,tiny-skia`, which falls back in order.
- The About sheet and the bench report say which renderer is drawing.

**Folders the app can't write** (decided by the project owner, 2026-10-30; `docs/specs/journal.md`, "A folder that can't be written").
- `doc.open` refuses a script whose crash journal can't be created beside it, with `folder_read_only`.
- The app then opens the script as an untitled copy, `<name> (copy).<ext>` in `Unsaved/`, journaled there like any new script.
- A banner says the folder can't be written and that Save as… keeps the copy anywhere else.
- The original is never written to.

**Synced folders** (OneDrive, iCloud Drive, Dropbox).
- Journals and saves work there as in any folder, with three protections:
  - an atomic save's rename already retries for about a second on Windows when a sync client or antivirus holds the file (`crates/core/src/io.rs`);
  - a journal synced from another computer is offered, never applied on its own (`docs/specs/journal.md`, "A journal from somewhere else");
  - a file changed by the sync client while it's open fails the save with `file_changed` and loses nothing.
- **Resolution:** a file the sync client hasn't downloaded yet ("online only") is read through the OS like any other file, which downloads it first. The manual test covers all three services on the machines we have.

## 7. Size

- The budget is 15,000,000 bytes for each installer or package a writer downloads: `setup.exe`, `.zip`, `.dmg`, `.deb`, `.AppImage` and `.flatpak`. **Resolution:** decimal megabytes, the stricter of the two readings of "15 MB".
- A Flatpak bundle doesn't include its runtime, which Flatpak downloads once for every app that uses it. The budget is the bundle file.
- CI measures every file on every pull request, writes the sizes to the job summary, and fails the job when one is over.

## 8. Signing

- **macOS:** ad hoc only (`codesign --force --sign -`), never notarized, by the project owner's no-cost rule. A downloaded app is quarantined, so the first start needs System Settings ▸ Privacy & Security ▸ **Open Anyway**. On macOS 12 to 14, Control-click ▸ Open works too. The release notes and `docs/release/mac-tester.md` give both, and the `xattr -dr com.apple.quarantine` fallback.
- **Windows:** unsigned until SignPath Foundation approves the project. Signing is one switch:
  - with the repository variable `WINDOWS_SIGNING` set to `signpath`, and the secret `SIGNPATH_API_TOKEN` with the organization and project ids in repository variables, the release workflow submits `Cold Open.exe` and `coldopen.exe` for signing before packaging, then the installer after it;
  - unset, it skips those steps, and the release notes say the build is unsigned and how to get past SmartScreen ("More info" ▸ "Run anyway").
  - Nothing else changes between a signed and an unsigned build.
- **Linux:** packages are not signed. `SHA256SUMS.txt` is the check.

## 9. Version and commit

- **One version.** The workspace's `version` in the root `Cargo.toml` is the version everywhere:
  - the app's About sheet and window title;
  - `coldopen --version`;
  - the MCP server's `serverInfo.version`;
  - the agent link's handshake;
  - the journal header's `app` (`docs/specs/journal.md`) and the bundle's `meta.json` `app` (`docs/specs/bundle.md`);
  - the bench report (`docs/specs/bench.md`);
  - each package's own version field;
  - the release's tag.
  A test checks the programs agree.
- **Pre-releases:** `0.1.0-alpha.1`, `0.1.0-alpha.2`, and so on, then `0.1.0-beta.1`. A tag is `v` plus the version. The release workflow fails when the tag and `Cargo.toml` disagree, and marks any version with a `-` as a pre-release.
- **Commit:** the commit a build came from, 12 hex digits, appears in the About sheet, `coldopen --version` and the bench report. It comes from `COLDOPEN_COMMIT` at build time (CI sets it), else from `git` when the build runs in a clone, else `unknown`. It is information only: nothing reads it back, and no file Cold Open writes depends on it. **Resolution:** the journal header and `meta.json` keep the version alone, so a script saved by two builds of one version reads the same.

## 10. Licenses

Every package holds a `licenses` folder:

| File | What |
| --- | --- |
| `COPYING.txt` | GPL-3.0, for the app, the CLI and the MCP server, with a line naming the source: `https://github.com/TheYungStump/cold-open` at the commit of §9 |
| `LICENSE-MIT.txt`, `LICENSE-APACHE.txt` | The licenses of our format, layout and font crates, which are also available under MIT or Apache-2.0 |
| `Courier-Prime-OFL.txt`, `Atkinson-Hyperlegible-Next-OFL.txt` | The fonts' SIL Open Font License 1.1 |
| `THIRD-PARTY-LICENSES.txt` | Every third-party crate compiled into that target's binaries, grouped by license, with each license's text and the crates under it |

- **The third-party file** is generated at build time by `cargo-about`, for the target being built, from `about.toml` at the root. That file lists the licenses we accept. A crate under any other license fails the build, which also guards the license boundary. CI runs the same check on every pull request.
- **The app's About sheet** shows all of these. The third-party text is compiled into the app from the generated file. A build without it (a developer's `cargo build`) says where to find it instead.
