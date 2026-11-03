# r/linux

## Notes for the owner (not part of the post)

**What's known about the rules** (from search results, 2026-11-03; not read on Reddit itself):

- r/linux keeps its rules in a public GitLab project, "Rules for /r/Linux", but the search results didn't show the rule text.
- An archived snapshot of the subreddit shows a **proposal to refuse personal projects under 3 months old**, judged by a public repository's commit history. Whether it was adopted is unknown. If it was, Cold Open's public repository dates from 2026-10-15, so it qualifies from **2027-01-15**.
- Reddit's general guidance on self-promotion (about 10% of your posts linking your own work) applies everywhere. Subreddits enforce it in their own ways.

**Check in the sidebar before posting** (Reddit ▸ r/linux ▸ About ▸ Rules), and paste what you find here:

- [ ] The rule on posting your own project:
- [ ] Any minimum project age:
- [ ] Rules on AI-assisted projects, if any:
- [ ] Flair required:

**If r/linux doesn't fit,** look for smaller communities about Linux apps (search Reddit for ones that take app announcements, and check each one's rules), Flathub's forum once the app is on Flathub, and the Linux communities on Lemmy or Mastodon. None of these was checked here.

**Angle:** Linux users who write, and Linux users whose screenwriting friends ask "what do I use on Linux?". The point is that the professional format, .fdx, works on Linux. Packaging (deb, AppImage, Flatpak), sizes and checksums matter to this audience. Don't oversell: they'll test the claims.

## The post

**Title:** Cold Open: a free, open-source screenwriting app for Linux (and Windows and macOS) that opens and saves .fdx. Alpha testers wanted

**Body:**

> Screenwriting has long been a gap on Linux: the industry exchanges `.fdx` files, and the app that defined the format doesn't run here. **Cold Open** is a free, GPL-3.0 screenwriting app that runs natively on Linux and opens and saves `.fdx` files as they came, with Fountain as its own format.
>
> **For Linux specifically**
>
> - x86-64 and ARM64; Wayland and X11.
> - A `.deb`, an AppImage and a Flatpak, each under 15 MB, with SHA-256 checksums. Built by CI from tagged commits, on Ubuntu 22.04 (glibc 2.35).
> - Pages lay out the same on every OS, byte for byte: same page breaks, same PDF.
> - No network code at all: no telemetry, no update check. CI checks that no HTTP client ever gets into the build.
> - A CLI (`coldopen`) for converting, exporting PDFs, comparing drafts and repairing damaged `.fdx` files, plus JSON over stdio and an MCP server.
>
> **For writing:** smart element flow and autocomplete, a crash journal that saves every keystroke before it's drawn, track changes, production revisions, scene-by-scene compare, reports, title page and PDF export.
>
> **Status:** alpha. It's written in Rust with iced. Much of the code is written with AI coding assistants under my direction, and I review every change. The app has no AI built in.
>
> **What would help:** install it on your distro and tell me what broke: packaging, Wayland or X11, fonts, HiDPI, the Flatpak's file access. **Measure performance** in the app writes a small report with model names and timings, which helps a lot.
>
> - Site and downloads: https://theyungstump.github.io/cold-open/
> - Install notes per package: https://theyungstump.github.io/cold-open/guide/install.html#linux
> - Source: https://github.com/TheYungStump/cold-open
