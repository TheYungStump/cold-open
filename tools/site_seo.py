#!/usr/bin/env python3
"""Checks the tags of docs/specs/site.md §10 on the built website:

- every page in the sitemap has a title of at most 70 characters naming
  "Cold Open", a description of 50 to 170 characters, a canonical link to
  its own address, the Open Graph and Twitter tags, and one JSON-LD
  SoftwareApplication whose fields are present and well formed;
- the share image exists and is 1200x630;
- sitemap.xml parses, holds absolute addresses under the site's address,
  each naming a page that exists, and holds every page it should;
- robots.txt names the sitemap; llms.txt starts with its title and summary.

Usage: site_seo.py BOOK
Exit status 1 on any problem.
"""

import json
import pathlib
import re
import struct
import sys
import tomllib
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
SM = "{http://www.sitemaps.org/schemas/sitemap/0.9}"
REQUIRED_LD = {
    "@context": str, "@type": str, "name": str, "description": str,
    "applicationCategory": str, "operatingSystem": str, "softwareVersion": str,
    "url": str, "downloadUrl": str, "offers": dict, "license": str, "image": str,
}
OG = ["og:type", "og:site_name", "og:title", "og:description", "og:url", "og:image",
      "og:image:width", "og:image:height", "og:image:alt"]
TW = ["twitter:card", "twitter:title", "twitter:description", "twitter:image"]


def unesc(s):
    return (s.replace("&quot;", '"').replace("&#39;", "'").replace("&lt;", "<")
            .replace("&gt;", ">").replace("&amp;", "&"))


def meta(text, attr, name):
    m = re.search(rf'<meta {attr}="{re.escape(name)}" content="([^"]*)"', text)
    return unesc(m.group(1)) if m else None


def png_size(path):
    with open(path, "rb") as f:
        head = f.read(24)
    if head[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    return struct.unpack(">II", head[16:24])


def page_problems(book, base, url):
    rel = url[len(base):]
    f = book / (rel + "index.html" if rel == "" or rel.endswith("/") else rel)
    if not f.exists():
        return [f"{url}: in the sitemap, but no page"]
    text = f.read_text(encoding="utf-8")
    out = []
    t = re.search(r"<title>(.*?)</title>", text, re.S)
    title = unesc(t.group(1)).strip() if t else ""
    if not title or len(title) > 70 or "Cold Open" not in title:
        out.append(f"title {title!r} ({len(title)} chars)")
    d = meta(text, "name", "description") or ""
    if not 50 <= len(d) <= 170:
        out.append(f"description {len(d)} chars: {d!r}")
    c = re.search(r'<link rel="canonical" href="([^"]*)"', text)
    if not c or unesc(c.group(1)) != url:
        out.append(f"canonical {c.group(1) if c else None!r}")
    for p in OG:
        if not meta(text, "property", p):
            out.append(f"no {p}")
    for n in TW:
        if not meta(text, "name", n):
            out.append(f"no {n}")
    if meta(text, "property", "og:url") != url:
        out.append("og:url isn't the page's address")
    blocks = re.findall(r'<script type="application/ld\+json">(.*?)</script>', text, re.S)
    apps = []
    for b in blocks:
        try:
            data = json.loads(b)
        except json.JSONDecodeError as e:
            out.append(f"JSON-LD doesn't parse: {e}")
            continue
        if data.get("@type") == "SoftwareApplication":
            apps.append(data)
    if len(apps) != 1:
        out.append(f"{len(apps)} SoftwareApplication blocks")
    for data in apps:
        if data.get("@context") != "https://schema.org":
            out.append("JSON-LD @context")
        for k, kind in REQUIRED_LD.items():
            if not isinstance(data.get(k), kind) or not data.get(k):
                out.append(f"JSON-LD {k} missing or wrong")
        offer = data.get("offers", {})
        if offer.get("@type") != "Offer" or offer.get("price") != "0" or not offer.get("priceCurrency"):
            out.append("JSON-LD offers")
        for k in ("url", "downloadUrl", "image", "license"):
            if not str(data.get(k, "")).startswith("https://"):
                out.append(f"JSON-LD {k} isn't an https address")
        if "aggregateRating" in data or "review" in data:
            out.append("JSON-LD claims ratings or reviews, which don't exist")
    return [f"{rel or '(home)'}: {p}" for p in out]


def main():
    book = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "docs/guide/book")
    base = tomllib.loads((ROOT / "docs/site/site.toml").read_text(encoding="utf-8"))["base_url"]
    problems = []
    try:
        tree = ET.parse(book / "sitemap.xml")
    except (OSError, ET.ParseError) as e:
        print(f"sitemap.xml: {e}")
        return 1
    urls = []
    for u in tree.getroot().findall(f"{SM}url"):
        loc = u.find(f"{SM}loc")
        if loc is None or not (loc.text or "").startswith(base):
            problems.append(f"sitemap: an address outside {base}")
            continue
        lastmod = u.find(f"{SM}lastmod")
        if lastmod is not None and not re.fullmatch(r"\d{4}-\d{2}-\d{2}", lastmod.text or ""):
            problems.append(f"sitemap: lastmod {lastmod.text!r}")
        urls.append(loc.text)
    if len(urls) != len(set(urls)):
        problems.append("sitemap: an address twice")
    for want in ["", "ai/", "download/", "guide/index.html", "privacy.html",
                 "final-draft-alternative/", "screenwriting-apps-compared/"]:
        if base + want not in urls:
            problems.append(f"sitemap: {want or '(home)'} missing")
    for bad in ["download.html", "print.html", "toc.html", "404.html", "download/started/"]:
        if any(u.startswith(base + bad) for u in urls):
            problems.append(f"sitemap: holds {bad}")
    for url in urls:
        problems += page_problems(book, base, url)
    size = png_size(book / "static" / "share.png") if (book / "static" / "share.png").exists() else None
    if size != (1200, 630):
        problems.append(f"static/share.png: {size}, not 1200x630")
    robots = (book / "robots.txt").read_text(encoding="utf-8") if (book / "robots.txt").exists() else ""
    if f"Sitemap: {base}sitemap.xml" not in robots:
        problems.append("robots.txt doesn't name the sitemap")
    llms = (book / "llms.txt").read_text(encoding="utf-8") if (book / "llms.txt").exists() else ""
    if not llms.startswith("# Cold Open\n\n> "):
        problems.append("llms.txt: no title and summary")
    for p in problems:
        print(p)
    print(f"{len(urls)} pages checked, {len(problems)} problem(s)", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
