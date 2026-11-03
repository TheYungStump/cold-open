# Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

Cold Open is applying to SignPath Foundation's program for open-source projects. Until the project is accepted, the Windows builds are unsigned, and Windows warns about them the first time they run (see [Install](guide/install.md#windows)). The macOS builds are signed ad hoc and not notarized; notarization needs a paid Apple account.

## Team roles

| Role | Who | What they do |
| --- | --- | --- |
| **Authors** (committers) | [TheYungStump](https://github.com/TheYungStump) | May change the source without another review |
| **Reviewers** | [TheYungStump](https://github.com/TheYungStump) | Review every change from anyone else, such as a pull request, before it's merged |
| **Approvers** | [TheYungStump](https://github.com/TheYungStump) | Approve each signing request by hand |

Every team member uses multi-factor authentication for GitHub and for SignPath.

Much of Cold Open's code is written with AI coding agents, working under the author's direction and GitHub account. The author reviews every change before it's merged.

## How a release is signed

- **Each release is built from source** by GitHub Actions, from a tagged commit of the [repository](https://github.com/TheYungStump/cold-open), by the [release workflow](https://github.com/TheYungStump/cold-open/blob/main/.github/workflows/release.yml). Nothing built elsewhere goes in.
- **Only Cold Open's own programs are signed:** the desktop app, the command-line tool and the Windows installer.
- **Every file carries Cold Open's name and the release's version** in its file details.
- **An approver approves each signing request by hand.**
- **Each release lists its files' SHA-256 checksums** in `SHA256SUMS.txt`, so anyone can check a download (see [Install](guide/install.md#check-a-download)).

## What the programs do to your system

- The Windows installer installs for you alone, needs no administrator rights, and asks before putting the command-line tool on your PATH. Settings ▸ Apps removes everything it installed.
- Cold Open makes no network connections. See [Privacy](privacy.md).
