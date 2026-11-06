#!/usr/bin/env python3
"""Writes release.json for the download page and the started pages
(docs/specs/site.md §5.1) from the newest release that isn't a draft,
pre-releases included: GitHub's /releases/latest address skips
pre-releases, and every alpha is one.

Usage: site_download.py OUT.json
Reads the releases with `gh api` (GH_TOKEN in CI). With no release yet, it
writes nothing, and the pages point at the releases page.
"""

import json
import subprocess
import sys

REPO = "TheYungStump/cold-open"


def main():
    out = sys.argv[1]
    raw = subprocess.run(
        ["gh", "api", f"repos/{REPO}/releases?per_page=30"],
        capture_output=True, text=True, check=True,
    ).stdout
    releases = [r for r in json.loads(raw) if not r.get("draft")]
    releases.sort(key=lambda r: r.get("published_at") or "", reverse=True)
    if not releases:
        print("no published release: the pages point at the releases page")
        return 0
    r = releases[0]
    assets = sorted(r.get("assets", []), key=lambda a: a["name"])
    data = {
        "version": r["tag_name"].lstrip("v"),
        "date": (r.get("published_at") or "")[:10],
        "notes": r["html_url"],
        "prerelease": bool(r.get("prerelease")),
        "assets": [
            {"name": a["name"], "size": a["size"], "url": a["browser_download_url"]}
            for a in assets
            if a["name"] != "SHA256SUMS.txt"
        ],
        "sums": next(
            (a["browser_download_url"] for a in assets if a["name"] == "SHA256SUMS.txt"), None
        ),
    }
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    print(f"{out}: {data['version']} ({len(data['assets'])} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
