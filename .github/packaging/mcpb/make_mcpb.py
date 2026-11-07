#!/usr/bin/env python3
"""Packs the release's MCP bundle (docs/specs/release.md §11.6):

    make_mcpb.py VERSION WINDOWS_EXE MACOS_BIN LINUX_BIN OUT.mcpb

An MCPB is a zip: manifest.json (from this folder, with the version), the
CLI for each OS under server/ (Windows x64, which ARM64 Windows 11 runs
too; macOS universal; Linux x86-64), the icon and the licenses. MCP
clients that install bundles run `coldopen mcp` from it.
"""

import os
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))


def add(z, src, name, executable=False):
    info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    # Unix permissions in the zip, so the binaries stay runnable.
    info.external_attr = ((0o755 if executable else 0o644) << 16)
    with open(src, "rb") as f:
        z.writestr(info, f.read())


def main():
    if len(sys.argv) != 6:
        raise SystemExit(__doc__)
    version, win, mac, linux, out = sys.argv[1:]
    manifest = open(os.path.join(HERE, "manifest.json"), encoding="utf-8").read()
    manifest = manifest.replace("@VERSION@", version)
    if "@" in manifest.replace("${__dirname}", ""):
        raise SystemExit("make_mcpb.py: a placeholder is left in manifest.json")
    with zipfile.ZipFile(out, "w") as z:
        info = zipfile.ZipInfo("manifest.json", date_time=(2026, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        info.external_attr = 0o644 << 16
        z.writestr(info, manifest)
        add(z, win, "server/windows/coldopen.exe", executable=True)
        add(z, mac, "server/macos/coldopen", executable=True)
        add(z, linux, "server/linux/coldopen", executable=True)
        add(z, os.path.join(ROOT, ".github/packaging/icon/png/256.png"), "icon.png")
        add(z, os.path.join(ROOT, "LICENSE"), "LICENSE")
    print(f"{out}: {os.path.getsize(out)} bytes")


if __name__ == "__main__":
    main()
