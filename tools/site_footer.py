#!/usr/bin/env python3
"""An mdBook preprocessor: the footer on every page of the website
(docs/specs/site.md §3). SignPath's terms ask for the words "Code signing
policy" on the home and download pages; the footer puts them, and the
privacy statement, on every page.

mdBook runs it as `site_footer.py supports <renderer>`, then with the book
as JSON on standard input, and reads the book back on standard output.
"""

import json
import posixpath
import sys

FOOTER = (
    '\n\n<footer class="site-footer">\n\n'
    "[Cold Open home]({root}index.html) · [Download]({root}download/) · "
    "[Code signing policy]({root}code-signing.md) · [Privacy]({root}privacy.md) · "
    "[Source code](https://github.com/TheYungStump/cold-open) · "
    "Cold Open is free software under the GPL-3.0-or-later.\n\n"
    "</footer>\n"
)


def add_footer(item):
    chapter = item.get("Chapter") if isinstance(item, dict) else None
    if not chapter:
        return
    path = chapter.get("path") or ""
    depth = posixpath.dirname(path.replace("\\", "/")).count("/") + (1 if "/" in path.replace("\\", "/") else 0)
    root = "../" * depth
    chapter["content"] = chapter.get("content", "").rstrip("\n") + FOOTER.format(root=root)
    for sub in chapter.get("sub_items", []):
        add_footer(sub)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "supports":
        sys.exit(0)
    # mdBook speaks UTF-8; Python's stdin and stdout follow the system's
    # code page on Windows, which turned "–" into "â€“" (2026-11-03).
    context, book = json.loads(sys.stdin.buffer.read().decode("utf-8"))
    # mdBook 0.5 calls the list "items"; 0.4 called it "sections".
    for key in ("items", "sections"):
        for item in book.get(key, []):
            add_footer(item)
    sys.stdout.buffer.write(json.dumps(book).encode("utf-8"))


if __name__ == "__main__":
    main()
