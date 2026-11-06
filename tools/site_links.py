#!/usr/bin/env python3
"""Checks every link between the built website's pages (docs/specs/site.md §7):
each relative link must name a page or file that exists, and each #anchor an
id on that page. Links to other sites aren't fetched.

mdBook writes regular HTML, so links and ids are read with two patterns,
with nothing beyond Python's core modules.

Usage: site_links.py docs/guide/book
Exit status 1 when a link is broken.
"""

import pathlib
import re
import sys
import urllib.parse

HREF = re.compile(r"<a\s[^>]*?href=\"([^\"]*)\"", re.IGNORECASE)
ID = re.compile(r"\sid=\"([^\"]+)\"", re.IGNORECASE)


def unescape(s):
    return (s.replace("&quot;", '"').replace("&#39;", "'").replace("&lt;", "<")
            .replace("&gt;", ">").replace("&amp;", "&"))


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "docs/guide/book").resolve()
    pages = {}
    for f in sorted(root.rglob("*.html")):
        text = f.read_text(encoding="utf-8")
        pages[f] = (
            [unescape(h) for h in HREF.findall(text)],
            {unescape(i) for i in ID.findall(text)},
        )
    bad = 0
    for f, (links, _) in pages.items():
        # The print page and the sidebar's table repeat every link.
        if f.name in ("print.html", "toc.html"):
            continue
        for href in links:
            if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", href) or href.startswith("//"):
                continue
            path, _, frag = href.partition("#")
            # A query (?from=…) names no other page.
            path = path.partition("?")[0]
            target = (f.parent / urllib.parse.unquote(path)).resolve() if path else f
            if target.is_dir():
                target = target / "index.html"
            if target not in pages:
                if not target.exists():
                    print(f"{f.relative_to(root)}: no such page: {href}")
                    bad += 1
                continue
            if frag and urllib.parse.unquote(frag) not in pages[target][1]:
                print(f"{f.relative_to(root)}: no such anchor: {href}")
                bad += 1
    print(f"{len(pages)} pages, {bad} broken link(s)", file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
