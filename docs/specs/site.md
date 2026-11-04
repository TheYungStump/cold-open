# The website and the user guide

- **Date:** 2026-11-02
- **Status:** phase 6b, being implemented.
- **Sources:**
  - The phase 6b brief and the project owner's decisions of 2026-11-02 (`docs/progress/phase-6b.md`): no cost, nothing personal in anything public, a Google Form for people without a GitHub account.
  - SignPath Foundation's terms (signpath.org/terms, read 2026-11-02): the words "Code signing policy" on the home page and on download and release pages, the policy's contents, and a privacy statement.
  - The mdBook user guide for version 0.5 (`book.toml`, `SUMMARY.md`, themes, `{{#include}}`, search). GitHub's documentation for Pages deployments from Actions (`configure-pages`, `upload-pages-artifact`, `deploy-pages`) and for the releases API.
  - The Web Content Accessibility Guidelines 2.2, level AA: contrast, keyboard operation, visible focus, text alternatives, page language.
  - `docs/specs/editor.md` §10 and §23, `docs/specs/commands.md`, `docs/specs/release.md`, `docs/specs/crash-reports.md`.
- No Final Draft material: the guide describes Cold Open in our own words. A choice where the sources leave room is marked **Resolution**.

## 1. Goals

1. **One source for the docs:** Markdown in the repository, versioned with the code. The website is built from it.
2. **References that can't drift:** the CLI, command and keyboard references are generated from the code (§4).
3. **Free and plain:** GitHub Pages, static files only, with relative links, so a custom domain can point at it later (§9).
4. **Accessible:** WCAG 2.2 AA in light and dark mode, checked in CI (§6).
5. **Private:** no analytics, no third-party fonts or scripts, nothing personal (§8).

## 2. Where it lives

```text
docs/guide/
  book.toml            mdBook's settings
  theme/coldopen.css   our additions to mdBook's theme (§6)
  src/
    SUMMARY.md         the table of contents
    index.md           Home
    download.md        Download, with download-files.md included
    download-files.md  the latest release's files, written at deploy (§5)
    …                  the guide, the tester pages, the policy pages
    reference/         generated (§4)
```

**Resolution (the generator):** mdBook 0.5. It is a single program, written in Rust, used for the Rust project's own books. It has a light and a dark theme that follow the system, local search, keyboard navigation between pages, and no outside requests. CI downloads a pinned release and checks its SHA-256. A writer reading the docs needs nothing: the Markdown reads on GitHub too.

## 3. Pages

| Page | File | What it holds |
| --- | --- | --- |
| Home | `index.md` | What Cold Open is, the alpha's status, and links to Download, the guide, Join the alpha, and the code signing policy |
| Download | `download.md` | The latest release's files (§5), which file is whose, the install notes, checking checksums, and the code signing policy |
| User guide | `guide/…` | Install, getting started, a Fountain primer, the features, recovery, agents, known limitations, FAQ |
| References | `reference/…` | The CLI, the commands, the keyboard (§4) |
| Join the alpha | `alpha/join.md` | What the alpha is, how to take part, the tester guide's two tracks, what feedback helps, where to send it |
| Code signing policy | `code-signing.md` | SignPath's credit, the team's roles, how a release is built and approved |
| Privacy | `privacy.md` | What Cold Open sends (nothing unless asked), what Report a problem and crash reports hold, where they go |
| Contribute | `contribute.md` | How to help during the alpha: test, report, discuss. Pull requests wait for a contributor agreement |

- **Every page's footer** links to the code signing policy and the privacy statement (SignPath asks for the words "Code signing policy" on the home and download pages, and the footer puts them on every page).
- **Links to the source**, such as a spec or a workflow, are full GitHub addresses, since the site doesn't hold the repository's other files.

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

- **At deploy,** `tools/site_download.py` asks GitHub's API for the repository's releases. It takes the newest one that isn't a draft, pre-releases included, and writes `download-files.md`:
  - the version and date;
  - a table of each file with its size, linking to the file;
  - a link to `SHA256SUMS.txt`, and the command to check a file on each OS.
- **Resolution:** GitHub's `/releases/latest` address skips pre-releases, and every alpha is one, so the page names the files itself.
- **The committed `download-files.md`** says that the files are on the releases page, with a link. A local build shows that, and so does a deploy made before any release exists.
- **When a release is published,** the deploy runs again (§7), so the page follows each release.

## 6. Look, accessibility and themes

- **Themes:** mdBook's light theme by default, and its dark theme (`navy`) when the system prefers dark (`preferred-dark-theme`). mdBook's theme menu offers the rest.
- **`theme/coldopen.css`** sets:
  - link and text colors where a theme falls under a contrast of 4.5:1 (3:1 for large text and interface parts);
  - a focus outline at least 2 pixels wide on every link and button;
  - a "Skip to the content" link, first in the page and visible when focused.
- **Images** have alt text that says what they show. Screenshots, where used, are made by `crates/app/examples/screenshots.rs` from our own sample scene.
- **The page's language** is `en` (`book.toml`).
- **Checks in CI** (the `site` job, §7):
  - axe-core runs on every built page, once in light mode and once in dark, through Playwright's Chromium, for the WCAG 2.2 A and AA rules. Any violation fails the job.
  - Keyboard operation and focus order are checked by hand, in the manual test.

## 7. Building and deploying

- **`.github/workflows/pages.yml`** runs on a push to `main` that changes `docs/guide/` or the tools it uses, when a release is published, and by hand. It:
  1. writes the download page (§5);
  2. builds the book with the pinned mdBook;
  3. scans the built site for personal details (§8);
  4. publishes it with `configure-pages`, `upload-pages-artifact` and `deploy-pages`.
- **On pull requests,** CI's `site` job builds the book and runs every check without publishing:
  - lychee, offline, checks every link between pages and every anchor;
  - axe-core, in light and dark (§6);
  - the privacy scan.
- **The address** is `https://theyungstump.github.io/cold-open/`. The app's About sheet and "Open the user guide" use it (`editor.md` §23).
- **Pages must be turned on** once, in the repository's settings (Pages ▸ Source: GitHub Actions). That is the project owner's to do.

## 8. Privacy

- **The site makes no request to another site.** mdBook's fonts, icons and search are bundled. There is no analytics, no comment widget, and no embedded media.
- **`tools/privacy_scan.py`** checks the built site, the release notes and the repository:
  - for email addresses other than GitHub's noreply ones and example domains;
  - for home folders named after someone;
  - on the owner's computer only, for the owner's own name and address. Those patterns are kept in a file outside the repository (`%APPDATA%\coldopen-privacy.txt`, or `~/.config/coldopen-privacy.txt`), so the scan never publishes what it looks for.
- **The contact paths** are GitHub (issues and Discussions, public) and the Google Form (private to the project). No personal address appears anywhere.

## 9. A custom domain later

Every link inside the site is relative, and nothing depends on the `/cold-open/` path. Pointing a domain at it later means a `CNAME` file and a DNS record, with no change to the pages. The app's address constant (`DOCS_URL`) is the one thing to change.
