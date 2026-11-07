#!/usr/bin/env python3
"""Looks for personal details in what Cold Open publishes (docs/specs/site.md §8).

Generic checks, which CI runs on every pull request:
  - email addresses, other than GitHub's noreply addresses and example domains,
    and other than a Windows DLL's function as Inno Setup names it
    (GetTickCount@kernel32.dll);
  - home folders with a user name in them (C:\\Users\\x, /Users/x, /home/x),
    other than the made-up names the tests use.

Private checks, run on the project owner's computer: one pattern a line
(a case-insensitive regular expression) in a file outside the repository,
so the patterns themselves are never published. The file is
$COLDOPEN_PRIVACY_PATTERNS, or else coldopen-privacy.txt in %APPDATA%
(Windows) or ~/.config (elsewhere). Lines starting with # are comments.

Usage:
  privacy_scan.py                 every tracked file in the working tree
  privacy_scan.py --history       every commit's author, committer and message
  privacy_scan.py --dir PATH ...  every file under PATH (a built site, notes)
  privacy_scan.py --github        pull request, issue and release texts (gh)
  privacy_scan.py --staged        the staged changes' added lines

Exit status 1 when anything is found.
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
ALLOWED_EMAIL = re.compile(
    r"(?:@users\.noreply\.github\.com|^noreply@github\.com|^noreply@anthropic\.com"
    r"|@example\.(?:com|org|net)|\.example$|\.invalid$|\.test$|@[A-Za-z0-9_-]+\.dll)$",
    re.IGNORECASE,
)
HOME = re.compile(r"(?:[A-Za-z]:[\\/]+Users[\\/]+|/Users/|/home/)([A-Za-z0-9._-]+)", re.IGNORECASE)
# Made-up people in the tests and docs, CI's own accounts, and placeholders.
ALLOWED_HOME = {
    "dana", "maya", "kim", "alice", "bob", "jo", "ada", "fuzz", "you", "me", "user", "username",
    "name", "x", "runner", "runneradmin", "public", "default", "shared",
}


def private_patterns():
    path = os.environ.get("COLDOPEN_PRIVACY_PATTERNS")
    if not path:
        base = os.environ.get("APPDATA") if os.name == "nt" else None
        base = Path(base) if base else Path.home() / ".config"
        path = base / "coldopen-privacy.txt"
    path = Path(path)
    if not path.is_file():
        return [], None
    pats = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            pats.append(re.compile(line, re.IGNORECASE))
    return pats, path


def findings(text, private):
    """Yields (line number, what was found) for one text."""
    for n, line in enumerate(text.splitlines(), 1):
        for m in EMAIL.finditer(line):
            # A template's placeholder before a file name, such as
            # ColdOpen-@VERSION@-windows-x64-setup.exe, isn't an address.
            if re.fullmatch(r"[A-Z0-9_]+", m.group(0).split("@")[0]) and line[m.start() - 1:m.start()] == "@":
                continue
            if not ALLOWED_EMAIL.search(m.group(0)):
                yield n, f"email address {m.group(0)}"
        for m in HOME.finditer(line):
            if m.group(1).rstrip(".").lower() not in ALLOWED_HOME:
                yield n, f"home folder {m.group(0)}"
        for p in private:
            if p.search(line):
                yield n, f"private pattern {p.pattern!r}"


def git(*args):
    return subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, check=True
    ).stdout.decode("utf-8", "replace")


def read_text(path):
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data[:8192]:
        return None  # binary
    return data.decode("utf-8", "replace")


def scan_tree(private):
    for name in git("ls-files", "-z").split("\0"):
        if not name or name == "Cargo.lock":
            continue
        text = read_text(ROOT / name)
        if text is not None:
            for n, what in findings(text, private):
                yield f"{name}:{n}", what


def scan_staged(private):
    diff = git("diff", "--cached", "-U0", "--no-color")
    file = "?"
    for line in diff.splitlines():
        if line.startswith("+++ "):
            file = line[6:] if line.startswith("+++ b/") else line[4:]
        elif line.startswith("+") and not line.startswith("+++"):
            for _, what in findings(line[1:], private):
                yield f"{file} (staged)", what


def scan_history(private):
    log = git("log", "--all", "-z", "--format=%H%n%an <%ae>%n%cn <%ce>%n%B")
    for entry in log.split("\0"):
        if not entry.strip():
            continue
        sha, _, rest = entry.strip("\n").partition("\n")
        for _, what in findings(rest, private):
            yield f"commit {sha[:12]}", what


def scan_dir(path, private):
    for p in sorted(Path(path).rglob("*")):
        if p.is_file():
            text = read_text(p)
            if text is not None:
                for n, what in findings(text, private):
                    yield f"{p}:{n}", what


def scan_github(private):
    def gh(*args):
        out = subprocess.run(["gh", *args], cwd=ROOT, capture_output=True, check=True)
        return json.loads(out.stdout or b"[]")

    for kind in ("pr", "issue"):
        for item in gh(kind, "list", "--state", "all", "--limit", "500",
                       "--json", "number,title,body,comments"):
            texts = [item.get("title") or "", item.get("body") or ""]
            texts += [c.get("body") or "" for c in item.get("comments") or []]
            for t in texts:
                for _, what in findings(t, private):
                    yield f"{kind} #{item['number']}", what
    for rel in gh("release", "list", "--limit", "100", "--json", "tagName"):
        view = gh("release", "view", rel["tagName"], "--json", "body,name")
        for t in (view.get("name") or "", view.get("body") or ""):
            for _, what in findings(t, private):
                yield f"release {rel['tagName']}", what


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--history", action="store_true")
    ap.add_argument("--github", action="store_true")
    ap.add_argument("--staged", action="store_true")
    ap.add_argument("--dir", action="append", default=[])
    ap.add_argument("--no-private", action="store_true",
                    help="generic checks only, as CI runs them")
    args = ap.parse_args()

    private, source = ([], None) if args.no_private else private_patterns()
    print(f"private patterns: {len(private)}" + (f" (from {source})" if source else ""),
          file=sys.stderr)

    scans = []
    if args.history:
        scans.append(scan_history(private))
    if args.github:
        scans.append(scan_github(private))
    if args.staged:
        scans.append(scan_staged(private))
    for d in args.dir:
        scans.append(scan_dir(d, private))
    if not scans:
        scans.append(scan_tree(private))

    found = 0
    for scan in scans:
        for where, what in scan:
            print(f"{where}: {what}")
            found += 1
    print(f"{found} finding(s)", file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
