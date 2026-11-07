# Privacy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

**In short: the website counts visits anonymously with Cloudflare Web Analytics, without cookies; the app sends nothing.**

## In practice

- **Cold Open makes no network connections.** It doesn't check for updates or contact any server. The app, the command-line tool and the agent server hold no network code at all, and the project's automated checks make sure none gets in.
- **A usage summary, on your computer only** (from 0.1.0-alpha.3). Cold Open counts which features you use, on this computer only, and it's never sent unless you include it in feedback. It holds counts, never your text, names or file paths. **Settings** (Ctrl+Comma, or Cmd+Comma on a Mac) shows it in full, resets it, or turns counting off.
- **No account, and nothing about you.** Cold Open never asks who you are. The one name it keeps is the one you give your tracked changes, in your own settings file.
- **Links open in your browser,** and only when you click them.

## What stays on your computer

Cold Open writes these files, and nothing else, and none of them leaves your computer unless you send it:

- your scripts, where you save them;
- each open script's crash journal, beside it (see [Recovery](guide/recovery.md));
- your settings, recent files and untitled scripts, in Cold Open's own folders (see [Install](guide/install.md#where-cold-open-keeps-things));
- crash reports, in Cold Open's own folder (below);
- the usage summary, `usage.json`, in Cold Open's own folder (below);
- the two report files **Measure performance** writes to your Downloads folder, holding your computer's model names and timings.

## AI agents

The app and `coldopen mcp` talk through a socket or a pipe that only your user account can open. There's no network endpoint. **An AI agent you connect is a separate program:** what it reads from your script, and where it sends it, is up to that program and its own privacy policy. See [AI agents](guide/agents.md).

## Report a problem

**Report a problem…** (in the palette and in About Cold Open) opens a page in your browser, ready to fill in: a GitHub issue, or our feedback form for people without a GitHub account. Cold Open fills in:

- Cold Open's version and commit, and the build's target (such as `x86_64-pc-windows-msvc`);
- your operating system's name and version;
- the renderer drawing the window (CPU or GPU, and the GPU's model).

It adds no script text, no file names or paths, and no user or computer names. **Nothing is sent until you submit the page yourself,** and you can change or remove anything on it first.

## Crash reports

If Cold Open crashes, it saves a report on your computer, in `crash-reports` in Cold Open's own folder. It keeps the newest five. A report holds:

- Cold Open's version, the build's target, your operating system and version, and the renderer;
- the time of the crash, in UTC;
- the name of the thread that crashed, and the name of the last action you ran, such as `text.insert` (never what you typed);
- where in Cold Open's code it happened;
- the error's message, with anything in quotes taken out, and any file path, your home folder and your user name replaced;
- a backtrace: code addresses within Cold Open's own files, with no folder names.

It holds no script text, file names or user names.

At the next start, Cold Open says a report was saved, and offers **Look at it** and **Never ask again**:

- **Look at it** shows you the whole report, exactly as it is on disk. From there you can **Send on GitHub** or **Send with the form**, which copy the report to your clipboard and open a page to paste it into, or **Copy** it, or **Delete** it. Nothing is sent until you submit that page.
- **Never ask again** turns crash reports off: Cold Open stops writing them at all. "Turn crash reports on or off" in the palette turns them back on.

## The usage summary

From 0.1.0-alpha.3, Cold Open keeps a small summary of how it's used, in `usage.json` in its own folder, so that feedback can say which features someone relies on. It's on from the first start, and it never leaves your computer by itself. It holds only counts:

- which commands and actions you used, by their names in the [command list](reference/commands.md) and the [keyboard reference](reference/keyboard.md), and how often;
- how many times you started Cold Open, and how long each session lasted, rounded to under 5 minutes, 5 to 30 minutes, 30 minutes to 2 hours, or longer;
- which formats you opened and saved (Fountain, .fdx or a project);
- whether the CPU or the GPU drew the window, and Cold Open's version;
- the day counting started.

It never holds script text, names of characters, scenes or anyone, file names or folders, or any time finer than a day.

- **View it:** Settings ▸ Usage summary ▸ **View…**, About ▸ **Usage summary…**, or "View usage summary" in the palette. It shows exactly what's in the file, in plain words.
- **Reset it:** **Reset…** deletes the file. Counting starts again from that day.
- **Turn it off:** Settings ▸ Usage summary ▸ **Counting off**. Then nothing is counted or written.
- **Share it, if you want to:** **Share feedback…** and **Report a problem…** have a tick, **Include my usage summary**, which is off each time. Ticked, the sheet shows the whole summary first, and it goes only into the page that opens in your browser, which you submit yourself.

## Where what you send goes

- **GitHub issues and Discussions are public,** under [GitHub's privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
- **The feedback form's answers come privately to the project,** through Google Forms, under [Google's privacy policy](https://policies.google.com/privacy). The form asks for nothing personal; a way to reach you is optional.

## This website

The website counts visits anonymously with Cloudflare Web Analytics, without cookies. It counts page views in aggregate, such as how many people opened the download page, and how many downloads each build had, by the page that started each download. It sets no cookie, stores nothing in your browser, and doesn't record the addresses' query strings ([Cloudflare's Web Analytics FAQ](https://developers.cloudflare.com/web-analytics/faq/); [Cloudflare's privacy policy](https://www.cloudflare.com/privacypolicy/)). Nothing else measures visitors: no other analytics, no fonts or scripts from elsewhere, and the site's search runs in your browser.

The download page's recommendation reads what your browser says about your computer, in your browser only, and sends nothing. Downloads come from GitHub's release files. The site is served by GitHub Pages, under GitHub's privacy statement.

## Questions

Ask in [Discussions](https://github.com/TheYungStump/cold-open/discussions), or through the [form](alpha/feedback.md).
