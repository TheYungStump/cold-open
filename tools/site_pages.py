#!/usr/bin/env python3
"""Builds the website's pages outside the docs, and the tags every page
carries (docs/specs/site.md §2, §3, §5, §8, §10), into a book mdBook built:

    site_pages.py BOOK [--release release.json] [--token TOKEN]

- the writers' page (index.html), the AI page, the download page, the
  started pages and the search pages, from docs/site/pages/ and
  docs/site/layout.html;
- download.html, a stub that sends the browser to download/;
- on every page mdBook made: the title, description, canonical link, Open
  Graph and Twitter tags and the structured data; `noindex` on print.html;
- the Cloudflare Web Analytics beacon on every page, when site.toml's
  cloudflare_token (or --token) is set;
- sitemap.xml, robots.txt and llms.txt;
- docs/site/static/ copied to static/.

Python's core modules only (this PC's Python lacks html.entities, so no
`html` module either).
"""

import json
import pathlib
import re
import shutil
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "docs" / "site"
GUIDE = ROOT / "docs" / "guide" / "src"

FROMS = ["writers", "ai"]

# Build id, label, OS, the file's name for a version (site.md §5.1).
BUILDS = [
    ("windows-x64", "Windows installer, x64", "windows", "ColdOpen-{v}-windows-x64-setup.exe"),
    ("windows-arm64", "Windows installer, ARM64", "windows", "ColdOpen-{v}-windows-arm64-setup.exe"),
    ("windows-x64-zip", "Windows .zip, x64 (no install)", "windows-zip", "ColdOpen-{v}-windows-x64.zip"),
    ("windows-arm64-zip", "Windows .zip, ARM64 (no install)", "windows-zip", "ColdOpen-{v}-windows-arm64.zip"),
    ("macos-universal", "macOS, universal (Apple chip and Intel)", "macos", "ColdOpen-{v}-macos-universal.dmg"),
    ("macos-arm64", "macOS, Apple chip only", "macos", "ColdOpen-{v}-macos-arm64.dmg"),
    ("macos-x64", "macOS, Intel only", "macos", "ColdOpen-{v}-macos-x64.dmg"),
    ("linux-x64-appimage", "Linux AppImage, x86-64", "appimage", "ColdOpen-{v}-x86_64.AppImage"),
    ("linux-x64-deb", "Debian and Ubuntu .deb, x86-64", "deb", "cold-open_{v}_amd64.deb"),
    ("linux-x64-flatpak", "Flatpak, x86-64", "flatpak", "ColdOpen-{v}-x86_64.flatpak"),
    ("linux-arm64-appimage", "Linux AppImage, ARM64", "appimage", "ColdOpen-{v}-aarch64.AppImage"),
    ("linux-arm64-deb", "Debian and Ubuntu .deb, ARM64", "deb", "cold-open_{v}_arm64.deb"),
    ("linux-arm64-flatpak", "Flatpak, ARM64", "flatpak", "ColdOpen-{v}-aarch64.flatpak"),
]
GROUPS = [
    ("Windows", ["windows-x64", "windows-arm64", "windows-x64-zip", "windows-arm64-zip"]),
    ("macOS", ["macos-universal", "macos-arm64", "macos-x64"]),
    ("Linux", ["linux-x64-appimage", "linux-x64-deb", "linux-x64-flatpak",
               "linux-arm64-appimage", "linux-arm64-deb", "linux-arm64-flatpak"]),
]

# The next steps on each started page, from guide/install.md.
STEPS = {
    "windows": "<ol><li>Run the installer. If SmartScreen says <strong>Windows protected your PC</strong>, click <strong>More info</strong>, then <strong>Run anyway</strong>: the build isn't signed yet.</li><li>It needs no administrator rights, and adds Cold Open to the Start menu.</li></ol>",
    "windows-zip": "<ol><li>Unzip it anywhere.</li><li>Run <code>Cold Open.exe</code>. If SmartScreen warns you, click <strong>More info</strong>, then <strong>Run anyway</strong>.</li></ol>",
    "macos": "<ol><li>Open the disk image and drag <strong>Cold Open</strong> to <strong>Applications</strong>.</li><li>The first time you open it, macOS refuses, because it isn't notarized yet. Open <strong>System Settings ▸ Privacy &amp; Security</strong> and click <strong>Open Anyway</strong>.</li></ol>",
    "appimage": "<ol><li>Make it runnable: <code>chmod +x ColdOpen-*.AppImage</code></li><li>Run it: <code>./ColdOpen-*.AppImage</code></li></ol>",
    "deb": "<ol><li>Install it: <code>sudo apt install ./cold-open_*.deb</code></li><li>Cold Open is in your applications menu.</li></ol>",
    "flatpak": "<ol><li>Install it: <code>flatpak install --user ColdOpen-*.flatpak</code></li><li>Run it: <code>flatpak run io.github.theyungstump.coldopen</code></li></ol>",
}

# The mark (docs/site/static/mark.svg, the project owner's brand of
# 2026-11-06), inline so its colors follow the page's theme (site.css).
RING = ("M30 0H70A30 30 0 0 1 100 30V70A30 30 0 0 1 70 100H30A30 30 0 0 1 0 70V30A30 30 0 0 1 30 0Z"
        "M32 13H68A18 18 0 0 1 86 31V69A18 18 0 0 1 68 87H32A18 18 0 0 1 14 69V31A18 18 0 0 1 32 13Z")
MARK = ('<svg viewBox="0 0 100 100" aria-hidden="true" focusable="false"><defs>'
        '<clipPath id="co-arc"><polygon points="42.6,50 112.6,-20 130,-20 130,120 112.6,120"/></clipPath>'
        '<mask id="co-gaps" maskUnits="userSpaceOnUse" x="-10" y="-10" width="120" height="120">'
        '<rect x="-10" y="-10" width="120" height="120" fill="#fff"/>'
        '<path d="M66 26.6 104 -11.4M66 73.4 104 111.4" stroke="#000" stroke-width="3.6"/></mask></defs>'
        f'<g mask="url(#co-gaps)"><path class="ring" fill-rule="evenodd" d="{RING}"/>'
        f'<path class="arc" fill-rule="evenodd" clip-path="url(#co-arc)" d="{RING}"/></g>'
        '<g class="lines"><rect x="34" y="39.5" width="33" height="4.2"/>'
        '<rect x="39.5" y="48" width="31" height="4.2"/><rect x="34" y="56.5" width="27.5" height="4.2"/></g></svg>')

TAGLINE = ("Cold Open is a free, open-source screenwriting app for Windows, macOS and "
           "Linux. It works offline, never loses your work, and opens and saves .fdx files.")


def esc(s):
    return (str(s).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
            .replace('"', "&quot;"))


def unesc(s):
    return (s.replace("&quot;", '"').replace("&#39;", "'").replace("&lt;", "<")
            .replace("&gt;", ">").replace("&nbsp;", " ").replace("&amp;", "&"))


def front(text):
    """A fragment's settings (its leading comment) and its body."""
    m = re.match(r"\s*<!--(.*?)-->\s*", text, re.S)
    meta = {}
    if m:
        for line in m.group(1).strip().splitlines():
            k, _, v = line.partition(":")
            meta[k.strip()] = v.strip()
        text = text[m.end():]
    return meta, text


def root_of(path):
    """The relative way back to the site's root from `path` ("ai/", "x.html")."""
    return "../" * path.count("/")


LINK = re.compile(r'(\s(?:href|src|srcset)=")(?![a-zA-Z][a-zA-Z0-9+.-]*:|#|/)([^"]*)"')


def rooted(body, root):
    """Links in a fragment are written from the site's root; this makes
    them relative to the page."""
    return LINK.sub(lambda m: f'{m.group(1)}{root}{m.group(2)}"', body)


def shorten(text, limit=158):
    text = re.sub(r"\s+", " ", text).strip()
    if len(text) <= limit:
        return text
    cut = text[:limit].rsplit(" ", 1)[0].rstrip(",;:")
    return cut + "…"


class Site:
    def __init__(self, book, release, token):
        self.book = book
        self.conf = tomllib.loads((SITE / "site.toml").read_text(encoding="utf-8"))
        self.base = self.conf["base_url"]
        self.token = token if token is not None else self.conf.get("cloudflare_token", "")
        self.release = release
        self.version = release["version"] if release else cargo_version()
        self.pages = []  # (address under base, source file for lastmod)
        self.written = set()  # the files this script wrote, relative to the book

    # The tags of site.md §10.1.
    def head(self, path, title, description):
        url = self.base + path
        image = self.base + "static/share.png"
        data = {
            "@context": "https://schema.org",
            "@type": "SoftwareApplication",
            "name": "Cold Open",
            "alternateName": "Cold Open screenwriting app",
            "description": TAGLINE,
            "applicationCategory": "DesignApplication",
            "applicationSubCategory": "Screenwriting",
            "operatingSystem": "Windows 10, Windows 11, macOS 12 or later, Linux",
            "softwareVersion": self.version,
            "url": self.base,
            "downloadUrl": self.base + "download/",
            "installUrl": self.base + "download/",
            "screenshot": self.base + "static/shot-light.png",
            "image": image,
            "license": "https://www.gnu.org/licenses/gpl-3.0.html",
            "isAccessibleForFree": True,
            "offers": {"@type": "Offer", "price": "0", "priceCurrency": "USD"},
            "author": {"@type": "Organization", "name": "The Cold Open contributors",
                       "url": self.conf["repo"]},
            "sameAs": [self.conf["repo"]],
        }
        tags = [
            f'<link rel="canonical" href="{esc(url)}">',
            '<meta property="og:type" content="website">',
            '<meta property="og:site_name" content="Cold Open">',
            f'<meta property="og:title" content="{esc(title)}">',
            f'<meta property="og:description" content="{esc(description)}">',
            f'<meta property="og:url" content="{esc(url)}">',
            f'<meta property="og:image" content="{esc(image)}">',
            '<meta property="og:image:width" content="1200">',
            '<meta property="og:image:height" content="630">',
            '<meta property="og:image:alt" content="Cold Open, a free, open-source screenwriting app, beside a screenplay page.">',
            '<meta name="twitter:card" content="summary_large_image">',
            f'<meta name="twitter:title" content="{esc(title)}">',
            f'<meta name="twitter:description" content="{esc(description)}">',
            f'<meta name="twitter:image" content="{esc(image)}">',
            '<script type="application/ld+json">'
            + json.dumps(data, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
            + "</script>",
        ]
        return "\n".join(tags) + "\n"

    def beacon(self):
        """Cloudflare Web Analytics, when its switch is on (site.md §8)."""
        if not self.token:
            return ""
        cfg = json.dumps({"token": self.token})
        return ("<script defer src=\"https://static.cloudflareinsights.com/beacon.min.js\" "
                f"data-cf-beacon='{esc(cfg)}'></script>\n")

    def write(self, path, html_text):
        rel = path + "index.html" if path == "" or path.endswith("/") else path
        self.written.add(rel)
        out = self.book / rel
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(html_text, encoding="utf-8", newline="\n")

    def render(self, path, meta, body, scripts="", extra_head=""):
        root = root_of(path)
        layout = (SITE / "layout.html").read_text(encoding="utf-8")
        title, description = meta["title"], meta["description"]
        frm = meta.get("from", "")
        values = {
            "title": esc(title),
            "description": esc(description),
            "head": extra_head + self.head(path, title, description),
            "root": root,
            "home": root or "./",
            "mark": MARK,
            "audience": esc(meta.get("audience", "")),
            "from": f"?from={frm}" if frm else "",
            "body": rooted(body, root),
            "scripts": scripts + self.beacon(),
        }
        out = re.sub(r"\{\{(\w+)\}\}", lambda m: values.get(m.group(1), m.group(0)), layout)
        return out.replace("{{version}}", esc(self.version))

    # The standalone pages.
    def fragments(self):
        for f in sorted((SITE / "pages").glob("*.html")):
            meta, body = front(f.read_text(encoding="utf-8"))
            if meta.get("template"):
                continue
            path = meta["path"]
            scripts = ""
            if path == "download/":
                body = body.replace("{{groups}}", self.groups()).replace("{{release}}", self.release_line())
                scripts = f'<script src="{root_of(path)}static/download.js"></script>\n'
            body = body.replace("{{version}}", esc(self.version))
            self.write(path, self.render(path, meta, body, scripts))
            self.pages.append((path, f))

    def assets(self):
        """The release's files by build id (site.md §5.1)."""
        if not self.release:
            return {}
        by_name = {a["name"]: a for a in self.release["assets"]}
        found = {}
        for bid, label, kind, pattern in BUILDS:
            a = by_name.get(pattern.format(v=self.version))
            if a:
                found[bid] = (label, kind, a)
        return found

    def release_line(self):
        if not self.release:
            return ('<p>The files are on the <a href="https://github.com/TheYungStump/cold-open/releases">'
                    "releases page</a>.</p>")
        r = self.release
        kind = "a pre-release" if r.get("prerelease") else "a release"
        sums = f' · <a href="{esc(r["sums"])}">SHA256SUMS.txt</a>' if r.get("sums") else ""
        return (f'<p class="quiet">Cold Open {esc(r["version"])}, {kind} of {esc(r["date"])}. '
                f'<a href="{esc(r["notes"])}">Release notes</a>{sums}</p>')

    def groups(self):
        found = self.assets()
        out = []
        for name, ids in GROUPS:
            items = []
            for bid in ids:
                if bid not in found:
                    continue
                label, _, a = found[bid]
                items.append(
                    f'<li><a href="download/started/{bid}/" data-build="{bid}">{esc(label)}</a> '
                    f'<span class="size">{a["size"] / 1_000_000:.1f} MB</span>'
                    f'<span class="file">{esc(a["name"])}</span></li>')
            if items:
                slug = name.lower()
                out.append(f'<section id="{slug}" data-os="{slug}"><h2>{name}</h2><ul>'
                           + "".join(items) + "</ul></section>")
        if not out:
            out.append('<p>No release yet: see the <a href="https://github.com/TheYungStump/cold-open/releases">'
                       "releases page</a>.</p>")
        # The files, by build, for the recommendation (site.md §5.2).
        data = {bid: {"label": v[0], "name": v[2]["name"],
                      "size": f'{v[2]["size"] / 1_000_000:.1f} MB'} for bid, v in found.items()}
        out.append('<script type="application/json" id="builds">'
                   + json.dumps(data).replace("</", "<\\/") + "</script>")
        return "\n".join(out)

    def started(self):
        """download/started/<build>/ and …/<build>/<from>/ (site.md §5.3)."""
        meta, body = front((SITE / "pages" / "started.html").read_text(encoding="utf-8"))
        sums = (self.release or {}).get("sums") or "https://github.com/TheYungStump/cold-open/releases"
        for bid, (label, kind, a) in self.assets().items():
            filled = (body.replace("{{label}}", esc(label)).replace("{{file}}", esc(a["name"]))
                      .replace("{{size}}", f'{a["size"] / 1_000_000:.1f} MB')
                      .replace("{{url}}", esc(a["url"])).replace("{{steps}}", STEPS[kind])
                      .replace("{{sums}}", esc(sums)))
            refresh = (f'<noscript><meta http-equiv="refresh" content="1;url={esc(a["url"])}"></noscript>\n'
                       '<meta name="robots" content="noindex">\n')
            page_meta = dict(meta, title=f"Downloading {label}: Cold Open screenwriting app",
                             description=f"Your Cold Open download is starting: {label}.")
            for frm in [""] + FROMS:
                path = f"download/started/{bid}/" + (f"{frm}/" if frm else "")
                scripts = f'<script src="{root_of(path)}static/started.js"></script>\n'
                html_text = self.render(path, page_meta, filled, scripts, extra_head=refresh)
                self.write(path, html_text)

    def stub(self):
        """The old download page, for the alpha.1 and alpha.2 notes' links."""
        page = ('<!doctype html>\n<html lang="en"><head><meta charset="utf-8">'
                '<title>Download Cold Open</title>'
                '<meta http-equiv="refresh" content="0;url=download/">'
                f'<link rel="canonical" href="{esc(self.base)}download/">'
                '<meta name="robots" content="noindex"></head>'
                '<body><p>The download page moved: <a href="download/">Download Cold Open</a>.</p>'
                + self.beacon() + "</body></html>\n")
        (self.book / "download.html").write_text(page, encoding="utf-8", newline="\n")

    # mdBook's pages.
    def docs(self):
        descriptions = self.conf.get("descriptions", {})
        for f in sorted(self.book.rglob("*.html")):
            rel = f.relative_to(self.book).as_posix()
            if rel in self.written or rel in ("download.html", "404.html") or rel.startswith("static/"):
                continue
            text = f.read_text(encoding="utf-8")
            m = re.search(r"<title>(.*?)</title>", text, re.S)
            chapter = unesc(m.group(1)).rsplit(" - ", 1)[0].strip() if m else "Cold Open"
            title = f"{chapter} · Cold Open screenwriting app"
            if rel in ("print.html", "toc.html"):
                title = "Cold Open screenwriting app: the whole guide"
            description = descriptions.get(rel) or self.first_paragraph(text) or TAGLINE
            if m:
                text = text[:m.start()] + f"<title>{esc(title)}</title>" + text[m.end():]
            text = re.sub(r'<meta name="description" content="[^"]*">',
                          f'<meta name="description" content="{esc(description)}">', text, count=1)
            extra = self.head(rel, title, description)
            if rel in ("print.html", "toc.html"):
                extra = '<meta name="robots" content="noindex">\n' + extra
            text = text.replace("</head>", extra + "</head>", 1)
            if self.token:
                text = text.replace("</body>", self.beacon() + "</body>", 1)
            f.write_text(text, encoding="utf-8", newline="\n")
            if rel not in ("print.html", "toc.html"):
                self.pages.append((rel, self.source_of(rel)))

    @staticmethod
    def first_paragraph(text):
        main = re.search(r"<main>(.*?)</main>", text, re.S)
        if not main:
            return ""
        for p in re.findall(r"<p>(.*?)</p>", main.group(1), re.S):
            words = unesc(re.sub(r"<[^>]+>", "", p)).strip()
            if len(words) > 40:
                return shorten(words)
        return ""

    @staticmethod
    def source_of(rel):
        src = GUIDE / rel.replace(".html", ".md")
        return src if src.exists() else None

    # Site-wide files (site.md §10.3).
    def sitemap(self):
        rows = []
        for path, src in sorted(set(self.pages), key=lambda x: x[0]):
            lastmod = git_date(src) if src else None
            row = f"  <url><loc>{esc(self.base + path)}</loc>"
            if lastmod:
                row += f"<lastmod>{lastmod}</lastmod>"
            rows.append(row + "</url>")
        xml = ('<?xml version="1.0" encoding="UTF-8"?>\n'
               '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
               + "\n".join(rows) + "\n</urlset>\n")
        (self.book / "sitemap.xml").write_text(xml, encoding="utf-8", newline="\n")

    def robots(self):
        text = robots_txt(self.base)
        (self.book / "robots.txt").write_text(text, encoding="utf-8", newline="\n")

    def llms(self):
        (self.book / "llms.txt").write_text(llms_txt(self.base), encoding="utf-8", newline="\n")

    def static(self):
        out = self.book / "static"
        out.mkdir(parents=True, exist_ok=True)
        for f in (SITE / "static").iterdir():
            if f.is_file():
                shutil.copyfile(f, out / f.name)


def robots_txt(base):
    return f"User-agent: *\nAllow: /\n\nSitemap: {base}sitemap.xml\n"


def llms_txt(base):
    """llmstxt.org's format: a title, a summary, then links with notes."""
    links = [
        ("Download", "download/", "Windows, macOS and Linux builds, each under 15 MB"),
        ("User guide", "guide/index.html", "install, getting started, and every feature"),
        ("Install", "guide/install.html", "per OS, including the unsigned-build warnings"),
        ("A Fountain primer", "guide/fountain.html", "the plain-text screenplay format Cold Open writes natively"),
        (".fdx exchange and repair", "guide/fdx.html", "opening, saving and repairing Final Draft-compatible files"),
        ("Recovery", "guide/recovery.html", "the crash journal and atomic saves"),
        ("FAQ", "guide/faq.html", "price, offline use, AI, platforms"),
        ("Known limitations", "guide/limitations.html", "what the alpha doesn't do yet"),
        ("For AI agents", "ai/", "connecting an MCP client; agents' edits arrive as tracked changes"),
        ("AI agents (MCP) guide", "guide/agents.html", "setup, what agents can and can't do"),
        ("Command line", "reference/cli.html", "every coldopen command"),
        ("Commands", "reference/commands.html", "the command set shared by the app, CLI, JSON and MCP"),
        ("Privacy", "privacy.html", "the app sends nothing; the site counts visits without cookies"),
    ]
    lines = [
        "# Cold Open",
        "",
        "> Cold Open is a free, open-source screenwriting app for Windows, macOS and Linux. "
        "It reads and writes Fountain and Final Draft-compatible .fdx files, lays out pages the "
        "same way on every computer, journals every keystroke so a crash never loses work, and "
        "exposes every action as a command through a CLI, JSON over stdio and an MCP server "
        "(`coldopen mcp`). The app is GPL-3.0-or-later; its format and layout libraries are "
        "MIT or Apache-2.0. It is in alpha.",
        "",
        "## Docs",
        "",
    ]
    lines += [f"- [{t}]({base}{p}): {n}" for t, p, n in links]
    lines += ["", "## Optional", "",
              "- [Source code](https://github.com/TheYungStump/cold-open): the repository, with the specs in docs/specs/",
              "- [Releases](https://github.com/TheYungStump/cold-open/releases): every version and its notes",
              ""]
    return "\n".join(lines)


def git_date(path):
    try:
        out = subprocess.run(["git", "log", "-1", "--format=%cs", "--", str(path)],
                             capture_output=True, text=True, check=True, cwd=ROOT).stdout.strip()
        return out or None
    except (OSError, subprocess.CalledProcessError):
        return None


def cargo_version():
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^version = "([^"]+)"', text, re.M)
    return m.group(1) if m else "unknown"


def main():
    args = sys.argv[1:]
    if not args:
        raise SystemExit(__doc__)
    book = pathlib.Path(args[0])
    release, token = None, None
    if "--release" in args:
        p = pathlib.Path(args[args.index("--release") + 1])
        if p.exists():
            release = json.loads(p.read_text(encoding="utf-8"))
    if "--token" in args:
        token = args[args.index("--token") + 1]
    site = Site(book, release, token)
    site.static()
    site.fragments()
    site.started()
    site.stub()
    site.docs()
    site.sitemap()
    site.robots()
    site.llms()
    print(f"{book}: {len(site.pages)} pages in the sitemap; beacon {'on' if site.token else 'off'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
