# The website and the user guide

- **Date:** 2026-11-02, updated 2026-11-05 (phase 6c: two landing pages, the download page and its recommendation, the started pages, analytics behind a switch, search and AI answer tools, the site root).
- **Status:** phase 6b, implemented; the phase 6c changes are being implemented.
- **Sources:**
  - The phase 6b brief and the project owner's decisions of 2026-11-02 (`docs/progress/phase-6b.md`): no cost, nothing personal in anything public, a Google Form for people without a GitHub account.
  - The phase 6c brief, items 2, 3 and 5, and the project owner's decisions of 2026-11-05 (`docs/progress/phase-6c.md`): a writers' page and an AI page, the download page's recommendation, Cloudflare Web Analytics only, download counts by path, the site root in a user-site repository.
  - SignPath Foundation's terms (signpath.org/terms, read 2026-11-02 and again 2026-11-05): the words "Code signing policy" on the home page and on download and release pages, the policy's contents, and a privacy statement.
  - The mdBook user guide for version 0.5 (`book.toml`, `SUMMARY.md`, themes, `{{#include}}`, search). GitHub's documentation for Pages deployments from Actions (`configure-pages`, `upload-pages-artifact`, `deploy-pages`), for user and project sites, and for the releases API.
  - The Web Content Accessibility Guidelines 2.2, level AA: contrast, keyboard operation, visible focus, text alternatives, page language.
  - Cloudflare's Web Analytics documentation and FAQ (read 2026-11-05): a JavaScript beacon with a site token, no cookies, and no query strings recorded.
  - MDN's documentation of `navigator.userAgentData` and `getHighEntropyValues`, the User-Agent Client Hints draft, and `<noscript>` and `<meta http-equiv="refresh">` in the HTML standard.
  - For search: sitemaps.org's protocol, the robots exclusion standard (RFC 9309), the Open Graph protocol, schema.org's `SoftwareApplication`, Google Search Central's documentation of titles, snippets and structured data, and llmstxt.org's proposal.
  - `docs/specs/editor.md` §10 and §23, `docs/specs/commands.md`, `docs/specs/release.md`, `docs/specs/crash-reports.md`, `docs/specs/usage-summary.md`.
- No Final Draft material: the site describes Cold Open in our own words. The comparison page (§10.2) uses other apps' public pages only, each cited with the date it was read. A choice where the sources leave room is marked **Resolution**.

## 1. Goals

1. **Two front doors.** Writers land on a page about writing (§3.1). People who use AI agents land on a page about agents (§3.2). Both share the download page, the docs and the release.
2. **The right download, without asking.** The download page recommends the build for the visitor's computer when the browser can tell, and lists every build below (§5).
3. **One source for the docs:** Markdown in the repository, versioned with the code. The docs are built from it with mdBook.
4. **References that can't drift:** the CLI, command and keyboard references are generated from the code (§4).
5. **Free and plain:** GitHub Pages, static files only, with relative links, so a custom domain can point at it later (§9).
6. **Accessible:** WCAG 2.2 AA in light and dark mode, checked in CI (§6).
7. **Private:** no cookies, no third-party fonts or scripts, nothing personal. The one exception is Cloudflare Web Analytics, which counts visits without cookies, and only once its switch is on (§8).
8. **Findable:** every page answers a real search in plain words, and says what it is to search engines and AI answer tools (§10).

## 2. Where it lives

```text
docs/guide/                the docs, built by mdBook (as in 6b)
  book.toml
  theme/                   coldopen.css, a11y.js
  src/                     SUMMARY.md, the guide, the references, the policy pages
docs/site/                 the pages outside the docs (§3), built by tools/site_pages.py
  site.toml                the site's settings: its address, the analytics switch (§8)
  layout.html              the frame every standalone page shares
  pages/                   one HTML fragment per page, with its title and description
  static/                  site.css, download.js, started.js, the mark, the share image
docs/site-root/            the files for the host's root (§9), for the user-site repository
tools/site_*.py            the build steps and checks (§7)
```

- **Resolution (two builders):** the docs need mdBook's sidebar, search and themes. The landing and download pages must not show the docs' sidebar, since it lists the AI agents page (§11). A small Python script writes those pages from fragments and one layout, then adds the tags of §10 to every page mdBook made. Python's core modules only, as for the other site tools (phase 6b, decision 18).

## 3. Pages

| Page | Address under `/cold-open/` | Built by | What it holds |
| --- | --- | --- | --- |
| For writers | `` (the home page) | `site_pages.py` | §3.1 |
| For AI users | `ai/` | `site_pages.py` | §3.2 |
| Download | `download/` | `site_pages.py` | §5 |
| Download started | `download/started/<build>/` and `…/<build>/<from>/` | `site_pages.py` | §5.3 |
| The search pages | `final-draft-alternative/`, `open-fdx-file/`, `screenwriting-on-linux/`, `fountain-screenplay-format/`, `screenwriting-apps-compared/` | `site_pages.py` | §10.2 |
| The docs | `guide/…`, `reference/…`, `alpha/…` | mdBook | Install, getting started, a Fountain primer, the features, recovery, known limitations, FAQ; **Advanced:** AI agents, the command line, the commands (§11) |
| Code signing policy | `code-signing.html` | mdBook | SignPath's credit, the team's roles, how a release is built and approved |
| Privacy | `privacy.html` | mdBook | §8 |
| Contribute | `contribute.html` | mdBook | How to help during the alpha |
| The old download page | `download.html` | `site_pages.py` | A stub that sends the browser to `download/` (a `meta` refresh and a link), so the alpha.1 and alpha.2 notes' links keep working |

- **Every page's footer** links to the code signing policy and the privacy statement. SignPath asks for the words "Code signing policy" on the home and download pages; the footer puts them on every page.
- **The addresses mdBook made in 6b don't change.** Releases, the app (`DOCS_URL`, About's links) and the SignPath application link to them.
- **Links to the source,** such as a spec or a workflow, are full GitHub addresses, since the site doesn't hold the repository's other files.

### 3.1 The writers' page

It leads with writing, in this order:

1. **Free and open source.** No account, no activation, no subscription.
2. **Works offline.** Nothing to sign into; the app needs no internet.
3. **Never loses your work.** Every keystroke goes into a crash journal before it shows on screen, and the next start brings it back. Saves are atomic. Said in these terms, which the tests back (§12).
4. **.fdx exchange, byte for byte.** Open the `.fdx` files producers send and save them back as they came (phase 4's gate: every corpus file writes back byte for byte).
5. **Windows, macOS and Linux,** including Linux, where Final Draft doesn't run.

Then a screenshot, the download button, and links to the docs and the source. It never mentions AI or agents (§11).

### 3.2 The AI page

It leads with agents:

1. **Connect your agent through MCP:** setup for common clients, each from that client's own public docs, cited.
2. **Agents' edits arrive as tracked changes** under the agent's name, for the writer to accept or reject.
3. **What agents can't do:** accept any change, discard the writer's or another agent's (an agent may withdraw only its own), edit the title page while tracking is on, or open, close, revert or rename the writer's script while attached (`docs/specs/mcp.md` §3, §5.4 and §6.1).
4. **The CLI and `coldopen mcp`:** every command the app has, from a script or an agent.
5. **Nothing connects by default,** and the app sends nothing anywhere.

Then the same download button and links.

## 4. The generated references

Each is a golden test. The test renders the reference from the code and compares it with the file in `docs/guide/src/reference/`. `COLDOPEN_BLESS=1` rewrites the file; on CI, a bless is refused and a difference fails the test.

| File | Made from | The test |
| --- | --- | --- |
| `cli.md` | The CLI's clap definitions: every subcommand, its usage line, its description and each argument's help | A `#[cfg(test)]` renderer inside the `coldopen` binary, which can reach its private definitions |
| `commands.md` | `Command::NAMES`, each command's description (the MCP tool table, `crates/mcp/src/tools.rs`) and its parameters (`Command::params_schema`) | `crates/mcp/tests/reference.rs` |
| `keyboard.md` | Every app action and command the palette lists, its title, and its default keys on Windows and Linux and on macOS (`keymap::defaults_for`) | `crates/app/tests/reference.rs` |

- **Resolution (two key columns):** a shortcut differs by OS (Mod is Ctrl or Cmd; a few bindings exist on one OS only), and a test runs on one OS, so the key map takes the OS as a parameter, and each column is rendered from its own map.
- **Order:** the order of the code's own lists, which is stable, never a hash map's.

## 5. The download page

### 5.1 The files

- **At deploy,** `tools/site_download.py` asks GitHub's API for the repository's releases. It takes the newest one that isn't a draft, pre-releases included, and writes `release.json` for `site_pages.py`: the version, the date, the notes' address, and each file with its size, its address and its build id.
- **Resolution:** GitHub's `/releases/latest` address skips pre-releases, and every alpha is one, so the page names the files itself.
- **Build ids** are plain words, the same in the page, the started pages' addresses and the analytics:

  | Build id | File |
  | --- | --- |
  | `windows-x64`, `windows-arm64` | `ColdOpen-<v>-windows-<arch>-setup.exe` |
  | `windows-x64-zip`, `windows-arm64-zip` | `ColdOpen-<v>-windows-<arch>.zip` |
  | `macos-universal` | `ColdOpen-<v>-macos-universal.dmg` |
  | `macos-arm64`, `macos-x64` | `ColdOpen-<v>-macos-<arch>.dmg` |
  | `linux-x64-appimage`, `linux-arm64-appimage` | `ColdOpen-<v>-<x86_64 or aarch64>.AppImage` |
  | `linux-x64-deb`, `linux-arm64-deb` | `cold-open_<v>_<amd64 or arm64>.deb` |
  | `linux-x64-flatpak`, `linux-arm64-flatpak` | `ColdOpen-<v>-<x86_64 or aarch64>.flatpak` |

  A file the release doesn't have isn't listed, and has no started page. `SHA256SUMS.txt` is linked under the list. The symbols file isn't a download.
- **With no release yet** (a local build), the page lists nothing and links to the releases page.

### 5.2 The recommendation

The page as served lists every build, grouped by OS, with no recommendation. With JavaScript, `download.js` reads what the browser says about the computer, and, when it's sure, shows one large **Recommended for your computer** button above the list. It never redirects.

| The browser says | Recommended | Else |
| --- | --- | --- |
| macOS, not a touch device | `macos-universal`: it runs on both Apple chips and Intel (the owner's decision, 2026-11-05) | — |
| Windows, with `userAgentData` high-entropy values: `architecture` `x86`, `bitness` `64` | `windows-x64` | — |
| Windows, with high-entropy values: `architecture` `arm` | `windows-arm64` | — |
| Windows without high-entropy values (Firefox, Safari, old Edge) | none | The Windows group comes first, with how to tell: Settings ▸ System ▸ About ▸ System type |
| Linux, not Android, with an architecture from the high-entropy values or from `navigator.platform` (`x86_64`, `aarch64`) | `linux-<arch>-appimage`: it runs on any distribution without installing | The Linux group comes first |
| A phone or tablet: Android, iPhone, an iPad (an iPad's Safari says it's a Mac, so a "Mac" with more than one touch point is an iPad), or ChromeOS | none | "Cold Open runs on Windows, macOS and Linux computers." |
| Anything else | none | — |

- **Resolution (Windows without client hints):** Firefox and Safari report `Win64; x64` on ARM computers too, and an x64 build doesn't run on Windows 10 on ARM. The brief says to recommend nothing when the architecture can't be determined, so the page doesn't guess.
- **Resolution (Safari on a Mac):** Safari reports an Intel Mac on Apple chips too. The universal build makes that question moot.
- **Resolution (Linux's package):** the AppImage needs no install and runs on any distribution. The .deb and the Flatpak stay one line below, in the list.
- **Nothing is stored or sent.** The script reads `navigator` in the visitor's own browser and changes the page. It sets no cookie and writes no storage.

### 5.3 The started pages and download counts

- **Every download link** points at a page on the site, never straight at GitHub: `download/started/<build>/`. Each such page starts its file's download from GitHub. That makes each download a page view the analytics can count (§8), per build.
- **Per landing page:** the writers' and AI pages link to `download/?from=writers` and `download/?from=ai`. `download.js` reads `from`, checks it against that list of two plain words, and rewrites the page's links to `download/started/<build>/<from>/`. A visitor who comes some other way, or has JavaScript off, gets `download/started/<build>/`.
  - **Resolution (the path, not a query):** Cloudflare Web Analytics doesn't record query strings, so a query would count every download as one page (the owner's decision 1, 2026-11-05). `from` is a word naming the page, never an identifier, and the `?from=` query itself is never counted.
- **Each started page** says "Your download is starting", names the file and its size, gives a plain link to it, and then the next steps for its OS (the warnings an unsigned build gets, from `guide/install.md`) and the checksum.
- **The download starts in two ways:**
  - **JavaScript off:** a `<meta http-equiv="refresh" content="1;url=…">` inside `<noscript>` in the page's head.
  - **JavaScript on:** `started.js` sets `location.href` to the file a moment after the page loads. The `<noscript>` refresh is ignored.

  Neither waits on the analytics beacon, so an ad blocker that stops the beacon doesn't stop the download. GitHub serves the file as an attachment, so the page stays in place.
- **The pages are made at deploy** for the newest release (§7), so a new release updates them.

## 6. Look, accessibility and themes

- **The docs:** mdBook's light theme by default, and its dark theme (`navy`) when the system prefers dark (`preferred-dark-theme`). mdBook's theme menu offers the rest.
- **The standalone pages** follow the system's light or dark setting (`prefers-color-scheme`), with the same colors as the docs and the stand-in brand: navy, paper and a red accent (the owner's decision 5, 2026-11-05). They use the system's own sans-serif fonts, so nothing is fetched.
- **`theme/coldopen.css` and `static/site.css`** set:
  - link and text colors at a contrast of 4.5:1 or more (3:1 for large text and interface parts);
  - a focus outline at least 2 pixels wide on every link and button;
  - a "Skip to the content" link, first in the page and visible when focused.
- **Images** have alt text that says what they show. Screenshots are made by `crates/app/examples/screenshots.rs` from our own sample scene.
- **The page's language** is `en`.
- **Checks in CI** (the `site` job, §7):
  - axe-core runs on every built page, once in light mode and once in dark, through Playwright's Chromium, for the WCAG 2.2 A and AA rules. Any violation fails the job.
  - Keyboard operation and focus order are checked by hand, in the manual test.

## 7. Building and deploying

- **`.github/workflows/pages.yml`** runs on a push to `main` that changes `docs/guide/`, `docs/site/` or the tools they use, when a release is published, and by hand. It:
  1. writes `release.json` from the newest release (§5.1);
  2. builds the docs with the pinned mdBook;
  3. runs `tools/site_pages.py`: the standalone pages, the started pages, the tags of §10 on every page, `sitemap.xml`, `robots.txt` and `llms.txt`, and the beacon when its switch is on (§8);
  4. checks the built site: links (`site_links.py`), search tags (`site_seo.py`), the writers' words (`site_words.py`, §11), and personal details (`privacy_scan.py`);
  5. publishes it with `configure-pages`, `upload-pages-artifact` and `deploy-pages`.
- **On pull requests,** CI's `site` job builds the site and runs every check without publishing:
  - the checks of step 4;
  - axe-core, in light and dark (§6);
  - Playwright (`tools/site_browser.mjs`): every started page with JavaScript off reaches its file's address; the recommendation for Windows x64 and ARM, macOS in Chrome and in Safari on both chips, Linux x64 and ARM, Android, an iPhone and an iPad, through user-agent and client-hints overrides; and the `from` links;
  - the beacon switch, both ways: no page names Cloudflare when it's off, and every page carries exactly one beacon when a test token is set.
- **The address** is `https://theyungstump.github.io/cold-open/`. The app's About sheet and "Open the user guide" use it (`editor.md` §23).
- **Pages must be turned on** once, in the repository's settings (Pages ▸ Source: GitHub Actions). Done 2026-11-03.

## 8. Privacy and analytics

- **The site sets no cookies and stores nothing in the visitor's browser.** The docs' fonts, icons and search are bundled. There is no comment widget and no embedded media. Downloads come from GitHub's release files.
- **Cloudflare Web Analytics** is the only measurement, and only once its switch is on:
  - `docs/site/site.toml` holds `cloudflare_token`. While it's empty, no page has the beacon. When the project owner sets the site token, every page gets Cloudflare's beacon script, `static.cloudflareinsights.com/beacon.min.js`, with the token.
  - It counts page views and visits in aggregate, without cookies, and doesn't record query strings (Cloudflare's FAQ). Nothing else measures visitors.
  - The started pages (§5.3) make downloads per build and per landing page countable as page views.
- **The privacy statement** says, on the site, in `README.md` and in the code signing policy's privacy part: "The website counts visits anonymously with Cloudflare Web Analytics, without cookies; the app sends nothing." The app's usage summary has its own line (`usage-summary.md`): it counts which features you use, on this computer only, and is never sent unless you include it in feedback.
- **`tools/privacy_scan.py`** checks the built site, the release notes and the repository:
  - for email addresses other than GitHub's noreply ones and example domains;
  - for home folders named after someone;
  - on the owner's computer only, for the owner's own name and address. Those patterns are kept in a file outside the repository (`%APPDATA%\coldopen-privacy.txt`, or `~/.config/coldopen-privacy.txt`), so the scan never publishes what it looks for.
- **The contact paths** are GitHub (issues and Discussions, public) and the Google Form (private to the project). No personal address appears anywhere.

## 9. The host's root and a custom domain

- **Crawlers read `robots.txt` only at a host's root,** and `llms.txt` is expected there too, but this site lives under `/cold-open/`. The root of `theyungstump.github.io` is served by a user-site repository, `TheYungStump/TheYungStump.github.io`, which the project owner creates (decision 3, 2026-11-05). Its files are kept in `docs/site-root/`:
  - `robots.txt`, allowing everything and naming the sitemap;
  - `llms.txt`, the same as the site's;
  - `index.html`, a short page that links to Cold Open and sends the browser there;
  - `.nojekyll`, and a README with the owner's steps.
- **Copies stay under `/cold-open/`,** so a custom domain later needs no change to them.
- **A custom domain later:** every link inside the site is relative, and nothing depends on the `/cold-open/` path except `site.toml`'s address, which the canonical links, the sitemap and the share image's address are made from. Pointing a domain at it means a `CNAME` file, a DNS record and that one setting. The app's address constant (`DOCS_URL`) is the one other thing to change.

## 10. Search and AI answer tools

### 10.1 On every page

- **A title** of 60 characters or fewer, naming the page and then "Cold Open screenwriting app", since "cold open" alone is a TV term. The home page's: "Cold Open: free, open-source screenwriting app".
- **A description** of 70 to 160 characters, in plain words. The standalone pages set their own. The docs' pages take theirs from a table in `site.toml`, or else from their first paragraph.
- **A canonical link** to the page's own address under `site.toml`'s address.
- **Open Graph and Twitter tags:** `og:title`, `og:description`, `og:type` (`website`), `og:url`, `og:site_name`, `og:image` (the share image, 1200×630, by its full address), `og:image:alt`, and `twitter:card` (`summary_large_image`).
- **Structured data:** one JSON-LD `SoftwareApplication` with the name, "Cold Open screenwriting app" as `alternateName`, the description, `applicationCategory` `DesignApplication` with `applicationSubCategory` "Screenwriting", the operating systems, the version, the download page as `downloadUrl`, a free `Offer` (price 0), the license, the source, and a screenshot.
  - **Resolution (category):** Google's list of application categories has no writing category, and schema.org leaves it free. `DesignApplication` is the nearest of Google's, and the sub-category says what it is.
  - **Resolution (no ratings):** Google shows an app's rich result only with ratings or reviews. Cold Open has none, so none are given: the data still says what the app is, to every reader of it.
- **mdBook's `print.html`** carries `noindex`, since it repeats every page.

### 10.2 The search pages

Pages that answer real searches, in plain words, with no AI words (§11):

| Address | Answers |
| --- | --- |
| `final-draft-alternative/` | A free Final Draft alternative: what Cold Open does that a writer moving over needs, and what it doesn't do yet (`guide/limitations.md`). |
| `open-fdx-file/` | How to open an .fdx file for free, on each OS, and how to repair a damaged one. |
| `screenwriting-on-linux/` | Screenwriting on Linux: the AppImage, the .deb, the Flatpak, and Flathub when it's there. |
| `fountain-screenplay-format/` | What Fountain is, why a plain-text script helps, and how Cold Open reads and writes it; links to the primer. |
| `screenwriting-apps-compared/` | An honest comparison with other screenwriting apps (Final Draft, Fade In, WriterDuet, Highland, Trelby and others): price, platforms, .fdx, Fountain, offline use, license. Each fact is from that app's public pages, with the address and the date read. Nothing is said about how Final Draft works inside. Where we couldn't confirm a fact, the cell says so. |

- **"Final Draft" is named** only to say what Cold Open is compatible with or an alternative to, with the trademark notice on the page (`docs/spec.md`, Naming).

### 10.3 Site-wide

- **`sitemap.xml`:** every page's canonical address, with `lastmod` from the last commit that changed the page's source. Not the redirect stub, `print.html`, `toc.html` or `404.html`.
- **`robots.txt`:** everything allowed, and the sitemap's address. Under `/cold-open/` it's a copy; the root's is the one crawlers read (§9).
- **`llms.txt`,** in llmstxt.org's format: a title, a one-paragraph summary of the app, and links to the docs' main pages, the download page, the AI page and the source, each with a short note.
- **The GitHub repository** says the same in its own places: a description, topics and a social preview image (prepared in `docs/marketing/github.md`; the owner applies them).

`tools/site_seo.py` checks every built page for the tags above, parses each JSON-LD block and checks its fields, checks the share image's size, and checks that the sitemap lists exactly the pages it should, each of them existing.

## 11. The writers' words

- **The writers' path** is the home page, the download page, the started pages and the search pages, with the CSS and scripts they load. None of it mentions AI or agents, as the brief asks.
- **`tools/site_words.py`** fails the build if any of these appear in their text, their attributes (`alt`, `title`, `aria-label`, `content`), their links' text or addresses, or their scripts: `AI` (as a word), `A.I.`, "artificial intelligence", "agent" and "agents", "agentic", `MCP`, "Model Context Protocol", `LLM`, "machine learning", and the names of AI products (Claude, ChatGPT, GPT, Copilot, Gemini). The writers' pages don't link to `ai/`.
- **The docs are shared,** so they may say it. They keep agents under **Advanced** in the sidebar.
- **The FAQ** answers "Does Cold Open use AI?" honestly: only if you connect an agent yourself; nothing connects by default; every agent edit is a tracked change you approve. **Nothing anywhere says the app has no AI.**

## 12. Claims

Every claim on a page can be backed by something in the repository: a test, a measurement in `docs/progress/`, a release file's size, or a spec. Where a claim is about another app, it cites that app's public page. `docs/marketing/claims.md` lists each claim the site and the ads make, beside its evidence.
