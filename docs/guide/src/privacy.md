# Privacy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

## In practice

- **Cold Open makes no network connections.** It doesn't check for updates, count anything, or contact any server. The app, the command-line tool and the agent server hold no network code at all, and the project's automated checks make sure none gets in.
- **No account, and nothing about you.** Cold Open never asks who you are. The one name it keeps is the one you give your tracked changes, in your own settings file.
- **Links open in your browser,** and only when you click them.

## What stays on your computer

Cold Open writes these files, and nothing else, and none of them leaves your computer unless you send it:

- your scripts, where you save them;
- each open script's crash journal, beside it (see [Recovery](guide/recovery.md));
- your settings, recent files and untitled scripts, in Cold Open's own folders (see [Install](guide/install.md#where-cold-open-keeps-things));
- crash reports, in Cold Open's own folder (below);
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

## Where what you send goes

- **GitHub issues and Discussions are public,** under [GitHub's privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
- **The feedback form's answers come privately to the project,** through Google Forms, under [Google's privacy policy](https://policies.google.com/privacy). The form asks for nothing personal; a way to reach you is optional.

## This website

This site makes no request to any other site: no analytics, no cookies, no fonts or scripts from elsewhere. Its search runs in your browser. It's served by GitHub Pages, under GitHub's privacy statement.

## Questions

Ask in [Discussions](https://github.com/TheYungStump/cold-open/discussions), or through the [form](alpha/feedback.md).
