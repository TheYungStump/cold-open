#!/usr/bin/env python3
"""Writes the download page's file table (docs/specs/site.md §5) from the
newest release that isn't a draft, pre-releases included: GitHub's
/releases/latest address skips pre-releases, and every alpha is one.

Usage: site_download.py OUT.md
Reads the releases with `gh api` (GH_TOKEN in CI). With no release yet, it
leaves OUT.md as it is: the committed page points at the releases page.
"""

import json
import subprocess
import sys

REPO = "TheYungStump/cold-open"


def size(n):
    return f"{n / 1_000_000:.1f} MB"


def main():
    out = sys.argv[1]
    raw = subprocess.run(
        ["gh", "api", f"repos/{REPO}/releases?per_page=30"],
        capture_output=True, text=True, check=True,
    ).stdout
    releases = [r for r in json.loads(raw) if not r.get("draft")]
    releases.sort(key=lambda r: r.get("published_at") or "", reverse=True)
    if not releases:
        print("no published release: the page keeps its link to the releases page")
        return 0
    r = releases[0]
    version = r["tag_name"].lstrip("v")
    date = (r.get("published_at") or "")[:10]
    kind = "a pre-release" if r.get("prerelease") else "a release"
    lines = [
        "<!-- Written at deploy by tools/site_download.py from the newest release. -->",
        "",
        f"**Cold Open {version}**, {kind} of {date}. "
        f"[Release notes]({r['html_url']}).",
        "",
        "| File | Size |",
        "| --- | --- |",
    ]
    sums = None
    for a in sorted(r.get("assets", []), key=lambda a: a["name"]):
        if a["name"] == "SHA256SUMS.txt":
            sums = a["browser_download_url"]
            continue
        lines.append(f"| [`{a['name']}`]({a['browser_download_url']}) | {size(a['size'])} |")
    lines.append("")
    if sums:
        lines.append(
            f"Checksums: [`SHA256SUMS.txt`]({sums}). "
            "[Install](guide/install.md#check-a-download) says how to check a download."
        )
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"{out}: {version} ({len(r.get('assets', []))} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
