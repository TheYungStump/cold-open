# The website's own pages

The home page, the AI page, the download page, the started pages and the search pages, built by `tools/site_pages.py` after mdBook builds the docs (`docs/specs/site.md`).

| Path | What it is |
| --- | --- |
| `site.toml` | The site's address, the analytics switch, and the docs' descriptions |
| `layout.html` | The frame every standalone page shares |
| `pages/*.html` | One fragment per page, its title and description in the leading comment. Links are written from the site's root |
| `static/` | The stylesheet, the scripts, the mark, the wordmark's font, the screenshots and the share image |

## Build it here

```
mdbook build docs/guide -d book
python tools/site_download.py release.json
python tools/site_pages.py book --release release.json
python tools/site_links.py book
python tools/site_seo.py book
python tools/site_words.py book
```

`tools/site_release_fixture.json` stands in for a release with every build, as CI uses it.

## Turning on Cloudflare Web Analytics

Free, with no cookies and no change to the privacy statement, which already describes it.

1. Sign in to Cloudflare (a free account), then **Analytics & Logs ▸ Web Analytics ▸ Add a site**.
2. Enter the hostname `theyungstump.github.io`, and choose the JavaScript snippet (the site isn't proxied through Cloudflare).
3. Copy the snippet's **token**, the value after `"token":`. It's public: every page that carries the beacon shows it.
4. Put it in `site.toml` as `cloudflare_token = "…"`, and merge that change to `main`. The next deploy adds the beacon to every page.

Then the dashboard counts page views per path. Downloads show as `/cold-open/download/started/<build>/` (no landing page known), `…/writers/` or `…/ai/`.

## The share image

`static/share.png` is rendered from `tools/brand/card.html` by `node tools/brand/render.mjs share`, with Playwright. CI's site job renders it again on every pull request and keeps it as the `brand-images` artifact.
