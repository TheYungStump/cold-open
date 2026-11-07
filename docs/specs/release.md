# Release builds and packages

- **Date:** 2026-10-30, updated 2026-11-02 (phase 6b: the symbols file in §2 and §9, the code signing policy and file metadata in §8), and 2026-11-05 (phase 6c: the universal disk image in §2, §3.3 and §7, and the stores in §11)
- **Status:** phase 6a and 6b, implemented; the 6c additions are being implemented.
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
| All | | `ColdOpen-<v>-symbols.zip` | The debug symbols of every target's app, for reading crash reports (§9; added 2026-11-02). Not an install file, so not under the size budget |
| All | | `SHA256SUMS.txt` | One line per file above, `<sha256>  <name>`, sorted by name |

**Resolution (macOS):** one universal disk image if it stays under the size budget. If it doesn't, the release holds `ColdOpen-<v>-macos-arm64.dmg` and `ColdOpen-<v>-macos-x64.dmg` instead. The packaging step measures the universal image and decides; the release notes name what was shipped.

**Since 2026-11-05 (phase 6c, the project owner's decision):** every release holds the universal disk image, which the website recommends for every Mac, since a browser can't always tell an Apple chip from Intel (Safari reports Intel on both). The release profile uses fat link-time optimization to make it smaller (§7). The per-architecture images `ColdOpen-<v>-macos-arm64.dmg` and `ColdOpen-<v>-macos-x64.dmg` are shipped beside it, as smaller downloads for writers who know their Mac.

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
- **Uninstall** (Settings ▸ Apps) removes the installed files, shortcuts, PATH entry and associations. It never touches the writer's folders (§6), scripts, journals or set-aside copies. If the installed app is running, the uninstaller first closes it as an upgrade does (below; 2026-11-02).
- **Upgrades:** a newer installer over an older one replaces the files in place and keeps the settings. If the app is running, the installer offers to close it. **Resolution (2026-10-31, the project owner's manual test):** iced's window doesn't answer the request Windows' Restart Manager sends, so the installer ends the app when asking fails (`CloseApplications=force`). Nothing is lost: every keystroke is already in the crash journal, and the next start recovers it as after any crash. CI installs over a running app on every pull request.
  - **Closing it quickly** (2026-11-02, the project owner's retest: the Restart Manager's close took 20 to 30 seconds, waiting out Windows' time limit, and the uninstaller had no close at all). The installer and the uninstaller close the app themselves, before any file is replaced or removed, and the Restart Manager is no longer used (`CloseApplications=no`):
    1. Find the copies of `Cold Open.exe` running from the install folder. Copies run from anywhere else, such as a zip, hold none of its files and are left alone.
    2. Say that Cold Open will be closed and that nothing typed is lost, with OK and Cancel. Cancel stops the install or uninstall and changes nothing. A silent run takes OK.
    3. Ask each to close, as its window's close button does. A script with nothing unsaved closes cleanly.
    4. After 3 seconds, end any still running (one asking whether to save, say). Its work is in the crash journal, and the next start offers it back.
    5. If one is still running 3 seconds later, say so and stop, changing nothing.
    - **The 3 seconds are by the clock** (2026-11-04). Each check for a running copy asks WMI, and on a busy computer a check can take most of a second. The waits used to count only their quarter-second pauses, so "3 seconds" could take 10 or more. CI's uninstall took 11.1 and 15.4 seconds on two runs, against about 2 on most. One WMI connection now serves every check, since connecting is the slow part, and the log notes how long each step took.
  - CI starts the app, then installs over it and uninstalls it while it runs, and fails if either leaves it running or takes over 15 seconds. It prints the installer's and the uninstaller's logs of the close on every run, so a slow one shows where the time went.

### 3.2 Windows zip

The same files as the installer, in one folder named `ColdOpen` (no space, so a terminal path needs no quoting; 2026-10-31): `Cold Open.exe`, `coldopen.exe` (next to it, not in `bin`), `licenses\` and `README.txt`. It has no shortcuts, associations or PATH entry. The readme says how to add the folder to PATH. The app keeps its settings in the usual folders (§6). **Resolution:** a portable mode that keeps settings beside the exe is not in 6a; `COLDOPEN_HOME` already gives one for those who want it.

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

- **A bundle file** (`flatpak build-bundle`), on the `org.freedesktop.Platform` 25.08 runtime, made with Flatpak's own build commands (`build-init`, `build-finish`, `build-export`) in `package.sh`.
- **Resolution:** it holds the same release binaries the other Linux packages hold, so all three run identical code. `flatpak-builder` builds from a manifest, and the builder adds nothing for prebuilt binaries. Also, Ubuntu 22.04's builder calls a tool the 25.08 SDK no longer has, and Flathub's builder can't see the runner's SDK from its sandbox (CI, 2026-10-30). Flathub requires a manifest and a build from source; that is for a later phase. (2026-11-05: phase 6c prepares it, §11.1.)
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
- **The universal disk image** (2026-11-05, the project owner's decision). The release profile uses `lto = "fat"`, with `codegen-units = 1` and `strip = true`, to make every binary smaller. `opt-level` stays at 3 unless a budget improves, and `panic` stays `unwind`, since the crash journal and crash reports depend on it. If the universal image is still over 15,000,000 bytes, it ships anyway: **the budget applies per architecture**, and the universal image holds two. CI checks the per-architecture images against the budget, and reports the universal one's size without failing on it.

## 8. Signing

- **macOS:** ad hoc only (`codesign --force --sign -`), never notarized, by the project owner's no-cost rule. A downloaded app is quarantined, so the first start needs System Settings ▸ Privacy & Security ▸ **Open Anyway**. On macOS 12 to 14, Control-click ▸ Open works too. The release notes and `docs/release/mac-tester.md` give both, and the `xattr -dr com.apple.quarantine` fallback.
- **Windows:** unsigned until SignPath Foundation approves the project. Signing is one switch:
  - with the repository variable `WINDOWS_SIGNING` set to `signpath`, and the secret `SIGNPATH_API_TOKEN` with the organization and project ids in repository variables, the release workflow submits `Cold Open.exe` and `coldopen.exe` for signing before packaging, then the installer after it;
  - unset, it skips those steps, and the release notes say the build is unsigned and how to get past SmartScreen ("More info" ▸ "Run anyway").
  - Nothing else changes between a signed and an unsigned build.
- **Linux:** packages are not signed. `SHA256SUMS.txt` is the check.
- **The code signing policy** (2026-11-02, SignPath Foundation's terms): the README and the website's "Code signing policy" page carry SignPath's credit, the team's roles, and how a release is built and approved (`docs/specs/site.md` §3). SignPath asks for the words "Code signing policy" on release pages too. The release workflow ends every release's notes with links to the policy and the privacy statement.
- **File metadata** (the same terms): every signed Windows program names the product "Cold Open" and carries the release's version. The app has had it since 6a (`crates/app/build.rs`). The CLI gets the same resource in 6b, and so does the installer, through Inno Setup's version fields.

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
- **Pre-releases:** `0.1.0-alpha.1`, `0.1.0-alpha.2`, and so on, then `0.1.0-beta.1`. A tag is `v` plus the version. The release workflow fails when the tag and `Cargo.toml` disagree, and marks any version with a `-` as a pre-release. Once a tag's release exists, it starts the website's deploy, so the download page names the new files (`site.md` §7; 2026-11-07).
- **Commit:** the commit a build came from, 12 hex digits, appears in the About sheet, `coldopen --version` and the bench report. It comes from `COLDOPEN_COMMIT` at build time (CI sets it), else from `git` when the build runs in a clone, else `unknown`. It is information only: nothing reads it back, and no file Cold Open writes depends on it. **Resolution:** the journal header and `meta.json` keep the version alone, so a script saved by two builds of one version reads the same.
- **Symbols** (2026-11-02, the project owner's decision for crash reports, `docs/specs/crash-reports.md` §5). A crash report's frames are offsets into the build's binaries, so each release keeps the symbols that name them:
  - CI's release builds add line tables (`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`) and keep the symbols out of the binary:
    - Linux: `objcopy --only-keep-debug` copies them to `coldopen-app.debug`, then the binary is stripped;
    - macOS: `split-debuginfo=packed` writes `coldopen-app.dSYM`, then the binary is stripped;
    - Windows: the linker writes `coldopen_app.pdb` beside the exe.
  - Each shipped binary is the stripped form of the same link as its symbols, so the offsets match. The packages are the same size as before, and CI's size check still applies.
  - The release's `ColdOpen-<v>-symbols.zip` holds one folder per target, named by its target triple, with the app's symbols.
  - `tools/symbolicate.py <report> <folder>` resolves a report with `llvm-symbolizer` (rustup's `llvm-tools` component).
  - **Resolution:** the app only, since crash reports are the app's (`crash-reports.md` §2).

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

## 11. Stores

Added 2026-11-05 (phase 6c). Each store is free to list in, and each has rules of its own. `docs/progress/phase-6c.md` quotes the rules that apply, with their addresses and the date read. Nothing is submitted, uploaded or published without the project owner, who holds every store account. The files live in `.github/packaging/<store>/`, and each folder's `README.md` gives the owner's steps.

- **Every store gets the same build** as the release: the same binaries, version and licenses. No store build adds a network connection, an update checker or a store SDK.
- **Personal details:** each store's listing names the publisher as the project owner chooses. The files here use `TheYungStump`, the GitHub account, and the project's addresses, never a person's name or email.

### 11.1 Flathub

- **A manifest that builds from source** (`io.github.theyungstump.coldopen.yml`), as Flathub's requirements ask. It uses the `org.freedesktop.Platform` and `Sdk` 25.08 runtimes, with the `rust-stable` SDK extension, and builds the tagged commit with `cargo build --release --offline --locked`.
- **Crates come from `cargo-sources.json`,** generated from `Cargo.lock` by `flatpak-cargo-generator` (from `flatpak-builder-tools`, pinned by commit), since Flathub's builders have no network.
- **The same AppStream metainfo** as the other Linux packages, extended for Flathub:
  - screenshots by their full addresses at the tag;
  - a `<releases>` entry per version;
  - OARS content rating (none);
  - `<developer id="io.github.theyungstump">`;
  - `<url>` for the homepage, bugs, help and the source;
  - `<branding>` colors.
- **The desktop entry and MIME types** are the .deb's (§3.4, §5).
- **The permissions** are those of §3.6. **Resolution:** Flathub's reviewers may ask why `--filesystem=home` is needed. The answer is §3.6's: a script's crash journal and atomic save need its folder, which a portal doesn't give. The owner's decision of 2026-10-30 stands unless the reviewers refuse it.
- **CI builds it:** `flatpak-builder` in Flathub's own container (`flatpak/flatpak-github-actions`), and `flatpak-builder-lint` on the manifest, the metainfo and the build.
- **The owner's step:** a pull request to `flathub/flathub` with the manifest, `cargo-sources.json` and the metainfo, following Flathub's submission page. Flathub then makes a repository for the app, where updates go.

### 11.2 winget

- **Three manifests** (version, installer, default locale), in the community repository's layout: `manifests/t/TheYungStump/ColdOpen/<v>/`.
- **The installer manifest:**
  - `InstallerType: inno`, `Scope: user`;
  - the x64 and arm64 setup files, by their release addresses, with their SHA-256;
  - the `.fountain`, `.fdx` and `.coldopen` file extensions;
  - `Commands: [coldopen]`.
- **The release workflow writes them** for each release, from the published files' hashes, as a workflow artifact. CI checks them with `winget validate` on Windows.
- **The owner's step:** a pull request to `microsoft/winget-pkgs` with the three files. The `wingetcreate` tool or a fork both work; the README gives both.

### 11.3 Homebrew

- **Our own tap,** `TheYungStump/homebrew-tap` (the owner's decision, 2026-11-05), since Homebrew's official cask repository deprecates and disables casks whose apps don't pass Gatekeeper, and Cold Open isn't notarized (§8).
- **The cask** (`Casks/cold-open.rb`):
  - the universal disk image, by its release address, with its SHA-256;
  - `app "Cold Open.app"`;
  - `binary` for `coldopen`, so the CLI is on the PATH;
  - `depends_on macos: ">= :monterey"`;
  - `zap` for the app's folder in `~/Library/Application Support/Cold Open`;
  - a caveat on opening an app that isn't notarized (§8).
- **The release workflow writes it** for each release, as a workflow artifact. CI checks it with `brew style` and `brew audit --cask` on macOS, in a temporary tap.
- **The owner's steps:** create the tap repository, then copy each release's cask into it. Writers then run `brew tap theyungstump/tap`, `brew trust theyungstump/tap` (current Homebrew loads casks only from taps the user trusts) and `brew install --cask cold-open`.

### 11.4 Microsoft Store

- **An MSIX package,** which the Store signs for free when it's submitted. Built with `makeappx`, from the Windows SDK on the runner, from the release's x64 and arm64 binaries: one package per architecture, joined in a bundle (`.msixbundle`).
- **The manifest** (`AppxManifest.xml`):
  - `Identity` and `PublisherDisplayName`: the values Partner Center gives the owner's app reservation. Until then, placeholders, which CI fills with a test identity;
  - the `runFullTrust` capability, since Cold Open is a desktop app;
  - file type associations for `.fountain`, `.fdx` and `.coldopen`;
  - an app execution alias, `coldopen.exe`, so the CLI runs from a terminal;
  - the visual assets, made from the icon (§12), at the sizes the Store asks for.
- **What changes inside a package:** Windows redirects an MSIX app's writes to its own `AppData` folders into the package's private storage. Cold Open's settings, `Unsaved/` and crash journals for untitled scripts then live there, and the agent link's run folder too. The CLI, run through the alias, is in the same package, so it finds the app. CI installs the package (signed with a throwaway certificate made for that run), starts the app, and runs `coldopen --version` through the alias.
- **The publisher name** the Store shows is chosen in Partner Center. `docs/progress/phase-6c.md` says what Microsoft's documentation requires of an individual account's name, and the owner decides before anything is submitted.
- **The owner's steps:** the developer account and its identity check, the app's name reservation, the identity values for the manifest, the listing (from `docs/marketing/`), and the submission.

### 11.5 itch.io

- **A page in itch.io's tools section,** made by the owner, with its text drafted in `docs/marketing/itch.md`.
- **Uploads by `butler`,** itch.io's free command-line uploader, from the release workflow:
  - one channel per build: `windows-x64`, `windows-arm64`, `mac-universal`, `linux-x64` (the AppImage), `linux-arm64`;
  - butler pinned by version and checked by SHA-256;
  - the job runs only when the repository has a `BUTLER_API_KEY` secret, so a release without it isn't held up.
- **The owner's steps:** the itch.io account and page, an API key, and the secret.

### 11.6 MCP directories

Added 2026-11-06. MCP clients and directories find servers through the Official MCP Registry (registry.modelcontextprotocol.io), which lists a server by its package. Cold Open's MCP server is its CLI, `coldopen mcp`, which isn't on npm or PyPI, so its package is an **MCP bundle** on each release:

- **`ColdOpen-<v>-mcp.mcpb`**, made by `.github/packaging/mcpb/make_mcpb.py`: a zip holding `manifest.json` (MCPB manifest version 0.3, server type `binary`), the CLI for Windows (x64, which Windows 11 on ARM also runs), macOS (universal) and Linux (x86-64) under `server/`, the icon and the GPL. `platform_overrides` names each OS's binary; the bundle's manifest can't tell architectures apart, so Linux on ARM64 isn't covered by it.
- **`server.json`** for the registry (`.github/packaging/mcp-registry/`): the name `io.github.TheYungStump/cold-open`, which the registry ties to the GitHub account that publishes it, and the bundle's address and SHA-256, filled by `stores.py` into the **store-files** artifact.
- **The owner publishes it** with the registry's `mcp-publisher` (`login github`, then `publish`). Directories such as Glama read the registry; `docs/marketing/mcp-directories.md` has the steps.

## 12. Brand assets

Added 2026-11-05 (phase 6c, the owner's decision 5), with the stand-in **01 / Opening Frame**. Since 2026-11-06 it's the owner's final brand: a rounded C in blue `#0C447C`, whose open right side is a dark `#232326` arc cut off by two diagonal gaps, around three lines of a script, with COLD OPEN in the bundled Courier Prime Bold. Every asset is made by a script, so a later change means rerunning them:

- **`.github/packaging/icon/make_icons.py`** draws the mark from its geometry, at every size, and writes the `.ico`, the `.icns`, the PNGs and an SVG. `--from <folder>` takes artwork's PNGs instead.
- **The `.icns` holds PNG only in the slots macOS reads as PNG inside an app** (2026-11-07): `ic07` to `ic10` (128 to 1024) and `ic11` to `ic14` (the @2x sizes, 32 to 1024). `icp4`, `icp5` and `icp6` (16, 32, 48) are raw-pixel slots in an app bundle. A PNG in them displays for a loose `.icns` file but not for an app's icon. alpha.3 had them, and a tester's macOS launcher showed a garbled purple tile in place of the icon. macOS scales the small sizes down from the @2x slots. CI's Mac asks macOS to draw the installed app's icon at 16, 32 and 128 points and checks its colors (`smoke-macos.sh`). Source: the icns analysis by relikd, <https://relikd.github.io/icns-analysis/>, read 2026-11-07.
- **`tools/brand/`** renders, from one HTML card (`card.html`) and the list in `images.json`, with Playwright in CI's `site` job (artifact `brand-images`):
  - the share image;
  - the GitHub social preview;
  - itch.io's cover, 630×500 (2026-11-07);
  - the ads.

  The fonts are the app's own: Courier Prime and Atkinson Hyperlegible Next, both SIL OFL.
- **Swapping the brand later** means new geometry or artwork, then rerunning both scripts.
