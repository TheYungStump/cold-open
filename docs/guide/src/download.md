# Download

Cold Open is free. It needs no account and no activation, and you can install it on every computer you have.

{{#include download-files.md}}

## Which file is yours

| Your computer | File |
| --- | --- |
| Windows 10 or 11 (most PCs) | `…-windows-x64-setup.exe` |
| Windows on ARM (Snapdragon laptops, Surface Pro X) | `…-windows-arm64-setup.exe` |
| Windows, without installing | the `.zip` for your processor |
| macOS 12 or later, Apple chip (M1, M2, M3…) | `…-macos-arm64.dmg` |
| macOS 12 or later, Intel | `…-macos-x64.dmg` |
| Debian, Ubuntu, Mint, Pop!\_OS | the `.deb` for your processor (`amd64` for most PCs) |
| Any Linux | the `.AppImage` or the `.flatpak` |

Not sure which Mac you have? Apple menu ▸ **About This Mac** shows **Chip** for an Apple chip and **Processor** for Intel.

`…-symbols.zip` isn't for installing. It lets the project read crash reports from that release.

## Installing

The builds aren't signed by Microsoft or notarized by Apple yet, so your system warns you the first time you start Cold Open. [Install](guide/install.md) says what to click on each system, and how to check a download against its checksum.

## Code signing policy

Windows builds are to be signed through SignPath Foundation's free program for open-source projects. Until the project is accepted, they're unsigned. See the [code signing policy](code-signing.md).

## Older versions and the source

Every release, with its notes, is on the [releases page](https://github.com/TheYungStump/cold-open/releases). The source code is on [GitHub](https://github.com/TheYungStump/cold-open). To build it yourself, see the repository's README.
