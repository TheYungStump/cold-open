#!/usr/bin/env python3
"""Fills the store files (docs/specs/release.md §11) for one release:

    stores.py VERSION COMMIT DATE DIST OUT

DIST holds the release's files; their SHA-256 go into the store files.
OUT gets:
- flathub/: the manifest and the metainfo (cargo-sources.json is made beside
  them by CI, from Cargo.lock);
- winget/manifests/t/TheYungStump/ColdOpen/VERSION/: the three manifests;
- homebrew/Casks/cold-open.rb.

A file a template needs and DIST lacks fails the run, naming it.
"""

import hashlib
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    if len(sys.argv) != 6:
        raise SystemExit(__doc__)
    version, commit, date, dist, out = sys.argv[1:]
    dist, out = pathlib.Path(dist), pathlib.Path(out)
    files = {
        "SHA_WINDOWS_X64": f"ColdOpen-{version}-windows-x64-setup.exe",
        "SHA_WINDOWS_ARM64": f"ColdOpen-{version}-windows-arm64-setup.exe",
        "SHA_MACOS_UNIVERSAL": f"ColdOpen-{version}-macos-universal.dmg",
        "SHA_THIRD_PARTY_X86_64": f"ColdOpen-{version}-third-party-licenses-linux-x86_64.txt",
        "SHA_THIRD_PARTY_AARCH64": f"ColdOpen-{version}-third-party-licenses-linux-aarch64.txt",
    }
    values = {"VERSION": version, "COMMIT": commit, "DATE": date}
    for key, name in files.items():
        p = dist / name
        if not p.exists():
            raise SystemExit(f"stores.py: {name} isn't in {dist}")
        values[key] = sha256(p)

    def fill(src, dst):
        text = (HERE / src).read_text(encoding="utf-8")
        text = re.sub(r"@([A-Z0-9_]+)@", lambda m: values.get(m.group(1), m.group(0)), text)
        left = re.findall(r"@[A-Z0-9_]+@", text)
        if left:
            raise SystemExit(f"stores.py: {src} still has {left}")
        dst = out / dst
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_text(text, encoding="utf-8", newline="\n")
        print(dst)

    app = "io.github.theyungstump.coldopen"
    fill(f"flathub/{app}.yml", f"flathub/{app}.yml")
    fill(f"flathub/{app}.metainfo.xml", f"flathub/{app}.metainfo.xml")
    winget = f"winget/manifests/t/TheYungStump/ColdOpen/{version}"
    for name in ["TheYungStump.ColdOpen.yaml", "TheYungStump.ColdOpen.installer.yaml",
                 "TheYungStump.ColdOpen.locale.en-US.yaml"]:
        fill(f"winget/{name}", f"{winget}/{name}")
    fill("homebrew/cold-open.rb", "homebrew/Casks/cold-open.rb")
    return 0


if __name__ == "__main__":
    sys.exit(main())
