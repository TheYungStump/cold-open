# Ad drafts: writers and AI users

Drafts for the project owner to review and run. Nothing here has been posted or submitted. The images are in `images/`, rendered by `tools/brand/render.mjs` from `tools/brand/images.json` (CI's site job keeps the latest as the `brand-images` artifact).

- **Writers' ads** go to `https://theyungstump.github.io/cold-open/` and never mention AI.
- **AI ads** go to `https://theyungstump.github.io/cold-open/ai/`.
- **Every claim is in the ledger** at the end, with what backs it.

Add a plain `?utm_…` tag if the ad platform wants one: Cloudflare Web Analytics doesn't record query strings, so it won't show in our counts, but the platforms use it.

## Writers (Meta and Reddit) → `/cold-open/`

### 1. Never loses your work

- **Images:** `ad-writers-safe-1080x1080.png` (feed), `-1080x1920.png` (Stories, Reels), `-1200x628.png` (Reddit, link ads)
- **Meta primary text** (≤125 characters): Free screenwriting software that saves every keystroke before it shows on screen. A crash can't cost you a scene.
- **Meta headline** (≤40): Free screenwriting app, no account
- **Meta description** (≤30): Windows, macOS and Linux
- **Reddit headline:** Cold Open: a free, open-source screenwriting app that writes every keystroke to a crash journal, so a crash never costs you a scene. Windows, macOS, Linux.
- **Button:** Download

### 2. Open .fdx for free

- **Images:** `ad-writers-fdx-1080x1080.png`, `-1080x1920.png`, `-1200x628.png`
- **Meta primary text:** Got an .fdx script and no Final Draft? Open, edit and save it free. It saves back as it came.
- **Meta headline:** Open .fdx scripts for free
- **Meta description:** No account, no subscription
- **Reddit headline:** Open, edit and save .fdx screenplays for free, on Windows, macOS or Linux. Cold Open is free and open source.
- **Button:** Download

### 3. Linux too

- **Images:** `ad-writers-linux-1080x1080.png`, `-1200x628.png`
- **Meta primary text:** A free screenwriting app that runs the same on Linux, Windows and macOS, with the same page breaks on all three.
- **Meta headline:** Screenwriting on Linux, too
- **Meta description:** Free and open source
- **Reddit headline** (good for r/linux-adjacent targeting): A free, open-source screenwriting app for Linux, as an AppImage, .deb or Flatpak. Opens and saves .fdx.
- **Button:** Download

## AI users → `/cold-open/ai/`

### 4. You approve every edit

- **Images:** `ad-ai-review-1080x1080.png`, `-1080x1920.png`, `-1200x628.png`
- **Meta primary text:** Let your AI agent work on your screenplay, and approve every edit. Its changes arrive as tracked changes.
- **Meta headline:** Your agent writes. You approve.
- **Meta description:** Free, open source, MCP
- **Reddit headline:** Cold Open is a free screenwriting app with an MCP server built in. Your agent's edits arrive as tracked changes you accept or reject.
- **Button:** Learn more

### 5. MCP built in

- **Images:** `ad-ai-mcp-1080x1080.png`, `-1200x628.png`
- **Meta primary text:** A real screenplay editor your AI can drive: connect Claude, Cursor or any MCP client. Nothing connects by default.
- **Meta headline:** Screenwriting app with MCP
- **Meta description:** Free and open source
- **Reddit headline:** A screenplay editor with an MCP server built in: connect Claude, Cursor or any MCP client, and review its edits as tracked changes.
- **Button:** Learn more

## Before running them

- **Platform rules:** Meta's Advertising Standards and Reddit's Advertising Policy apply to both campaigns. Check each in your ad account's current version before launch: nothing here makes health, financial or "before and after" claims, but both platforms review software ads for misleading claims, and Reddit's targeting by community has its own rules.
- **Third-party names:** "Final Draft" appears only to say Cold Open opens .fdx files and is an alternative (nominative use; the site carries the trademark notice). "Claude" and "Cursor" are named only as MCP clients that work with it. If a platform's reviewer objects, drop the names: "any MCP client" carries the meaning.
- **Analytics:** Cloudflare Web Analytics counts landing-page visits and downloads per build once `cloudflare_token` is set in `docs/site/site.toml` (`docs/site/README.md`).

## Claims ledger

| Claim | What backs it |
| --- | --- |
| Free, open source, no account, no subscription | GPL-3.0-or-later (`LICENSE`), no sign-in anywhere in the app (`docs/spec.md`, "Yours") |
| Saves every keystroke before it shows on screen; a crash can't cost a scene | The crash journal (`docs/specs/journal.md`), and the crash gate, which kills the app at every step and loses no acknowledged key (`crates/app/tests/crash_gate.rs`, run five times in a row in phase 6c) |
| Windows, macOS and Linux | Every release's files (`docs/specs/release.md` §2) |
| Same page breaks on all three | The layout goldens, identical on all three OSes in CI (phase 2's gate) |
| Opens and saves .fdx; saves back as it came | Phase 4's gate: every corpus .fdx writes back byte for byte (`docs/progress/phase-4.md`) |
| AppImage, .deb or Flatpak | `docs/specs/release.md` §3 |
| MCP server built in; works with Claude, Cursor or any MCP client | `coldopen mcp` (`docs/specs/mcp.md`), a standard stdio MCP server; setups on `/cold-open/ai/` from each client's docs |
| Agents' edits arrive as tracked changes you accept or reject | `docs/specs/mcp.md` §6.1, `docs/specs/changes.md` §6 |
| Nothing connects by default | No network code in the app (CI's privacy job); the agent link is local and only for your user (`docs/specs/mcp.md` §7) |
