# The files for theyungstump.github.io's root

Crawlers read `robots.txt` only at a host's root, and `llms.txt` is expected
there too, but Cold Open's site lives at `https://theyungstump.github.io/cold-open/`
(docs/specs/site.md §9). The root is served by a user-site repository. These
are its files; `tools/site_pages.py` makes the same `robots.txt` and `llms.txt`
for `/cold-open/`, so keep them in step when either changes.

| File | What it does |
| --- | --- |
| `robots.txt` | Allows everything and names Cold Open's sitemap |
| `llms.txt` | Cold Open's summary and links, for AI answer tools |
| `index.html` | Sends a visitor to `/cold-open/`, with a plain link |
| `.nojekyll` | Tells GitHub Pages to serve the files as they are |

## The project owner's steps

1. On GitHub, create a public repository named exactly `TheYungStump.github.io`,
   with no README.
2. In it, Settings ▸ Pages ▸ Source: **Deploy from a branch**, branch `main`,
   folder `/ (root)`.
3. Copy the four files above into it and push them to `main`:

   ```
   git clone https://github.com/TheYungStump/TheYungStump.github.io.git
   cp docs/site-root/{robots.txt,llms.txt,index.html,.nojekyll} TheYungStump.github.io/
   cd TheYungStump.github.io && git add -A && git commit -m "Root files for Cold Open's site" && git push
   ```

   Use the noreply address as the commit's author, as in Cold Open's own
   repository.
4. After a minute, `https://theyungstump.github.io/robots.txt` should show the
   sitemap line. Then, if you like, add the site in Google Search Console and
   Bing Webmaster Tools (both free) and submit
   `https://theyungstump.github.io/cold-open/sitemap.xml`. Bing's index also
   feeds several AI answer tools.

Nothing here costs anything, and the repository holds nothing personal.
