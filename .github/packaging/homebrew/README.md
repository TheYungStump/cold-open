# Homebrew

Cold Open's cask, for our own tap, `TheYungStump/homebrew-tap` (docs/specs/release.md §11.3). The release workflow fills it with each release's universal disk image and its hash: download the **store-files** artifact from the release run, and the cask is `homebrew/Casks/cold-open.rb`. CI runs `brew style` and `brew audit --cask --strict` on it on every pull request.

## Why our own tap (Homebrew's pages, read 2026-11-06)

- Homebrew's official casks must "pass Homebrew's Gatekeeper checks and must not require System Integrity Protection or Gatekeeper to be disabled" (<https://docs.brew.sh/Acceptable-Casks>). Cold Open isn't notarized by Apple yet, so it would fail them. Its own tap has no such rule.
- A tap's repository name should "start with `homebrew-`" so the short command works (<https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap>).
- **Since Homebrew 6.0.0, "non-official taps require explicit trust by default"** (<https://docs.brew.sh/Tap-Trust>): writers run `brew trust` once.

## The tap

<https://github.com/TheYungStump/homebrew-tap>, set up on 2026-11-07 with alpha.3's cask and a README for writers.

For each new release, replace `Casks/cold-open.rb` in the tap with that release's, and commit it with the noreply address as the author.

## What writers run

```
brew tap theyungstump/tap
brew trust theyungstump/tap
brew install --cask cold-open
```

`brew upgrade --cask cold-open` picks up a new release once its cask is in the tap. The cask's caveat tells them how to open an app that isn't notarized.
