#!/usr/bin/env python3
"""Checks the writers' path for AI words (docs/specs/site.md §11): the home
page, the download page, the started pages and the search pages, with the
scripts and styles they load, must not mention AI or agents, in their text,
their attributes, their links' text or their addresses.

Usage: site_words.py BOOK
Exit status 1 when a word is found.
"""

import pathlib
import re
import sys

# Pages of the writers' path, under the book.
PAGES = [
    "index.html",
    "download/index.html",
    "final-draft-alternative/index.html",
    "open-fdx-file/index.html",
    "screenwriting-on-linux/index.html",
    "fountain-screenplay-format/index.html",
    "screenwriting-apps-compared/index.html",
]
STARTED = "download/started"

WORDS = re.compile(
    r"\bAI\b|\bA\.I\.|artificial intelligence|\bagents?\b|\bagentic\b|\bMCP\b|"
    r"model context protocol|\bLLMs?\b|machine learning|\bClaude\b|\bChatGPT\b|\bGPT\b|"
    r"\bCopilot\b|\bGemini\b",
    re.IGNORECASE,
)
# "AI" only as a capitalized word: "ai" alone in other words is fine, but
# not in an address (the AI page's path).
AI_PATH = re.compile(r"(?:href|src)=\"[^\"]*\bai/", re.IGNORECASE)
# Where AI words may appear on these pages without being shown: the
# structured data's license URL and the like have none, so nothing is
# exempt.


def visible(text):
    """Everything a reader or a screen reader meets: text, and the values
    of alt, title, aria-label and content attributes."""
    attrs = " ".join(re.findall(r'(?:alt|title|aria-label|content)="([^"]*)"', text))
    body = re.sub(r"<script\b(?![^>]*\bsrc=)[^>]*>.*?</script>", " ", text, flags=re.S)
    return attrs + " " + re.sub(r"<[^>]+>", " ", body)


def check(path, text, label):
    found = []
    for m in WORDS.finditer(visible(text)):
        word = m.group(0)
        # "AI" must be capitals to count as the word; "ai" in lowercase is
        # only ever part of an address here, which AI_PATH checks.
        if word.lower() == "ai" and word != "AI":
            continue
        found.append(word)
    for m in AI_PATH.finditer(text):
        found.append(m.group(0))
    for w in found:
        print(f"{label}: {w!r}")
    return len(found)


def main():
    book = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "docs/guide/book")
    files = [book / p for p in PAGES]
    # The started pages a visitor reaches from the writers' pages: not those
    # under …/ai/, which the AI page links to.
    files += sorted(f for f in (book / STARTED).rglob("index.html") if f.parent.name != "ai")
    assets = set()
    bad = 0
    for f in files:
        if not f.exists():
            print(f"{f.relative_to(book)}: missing")
            bad += 1
            continue
        text = f.read_text(encoding="utf-8")
        bad += check(f, text, f.relative_to(book).as_posix())
        for src in re.findall(r'(?:src|href)="([^"]+\.(?:js|css))"', text):
            if not re.match(r"^[a-z]+:", src):
                assets.add((f.parent / src).resolve())
    for a in sorted(assets):
        if a.exists():
            bad += check(a, a.read_text(encoding="utf-8"), a.relative_to(book.resolve()).as_posix())
    print(f"{len(files)} pages and {len(assets)} files of the writers' path, {bad} AI word(s)",
          file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
