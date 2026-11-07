# winget

The three manifests for Cold Open in the Windows Package Manager's community repository (docs/specs/release.md §11.2). The release workflow fills them with each release's addresses and hashes: download the **store-files** artifact from the release run, and the manifests are in `winget/manifests/t/TheYungStump/ColdOpen/<version>/`. CI checks them with `winget validate` on every pull request.

## The rules that apply (Microsoft's pages, read 2026-11-06)

From <https://learn.microsoft.com/en-us/windows/package-manager/package/repository> and the policies page, <https://learn.microsoft.com/en-us/windows/package-manager/package/windows-package-manager-policies>:

- **One package version per pull request:** "more than one package or version in the pull request" is refused.
- **The installer must support "non-interactive modes"** (winget passes Inno Setup's silent switches itself, from the manifests' `InstallerType: inno`), and come "directly from the publisher's website": the GitHub release.
- **It must "install and uninstall correctly for both administrators and non-administrators."** Cold Open's installer is per-user and needs no administrator rights.
- **Every installer is scanned by Microsoft Defender.** A false positive can be sent to Microsoft for analysis. Microsoft's pages don't say whether unsigned installers are accepted; many unsigned packages are in the repository, but a reviewer may ask. SmartScreen's warning doesn't stop installation through winget.
- **Pre-release versions:** the pages don't say. Submit alpha.3 and see; if it's refused, wait for a stable release.

## Submitting (your step)

**With `wingetcreate`** (Microsoft's tool, `winget install wingetcreate`):

```
wingetcreate submit --token <a GitHub token with public_repo> stores\winget\manifests\t\TheYungStump\ColdOpen\0.1.0-alpha.3
```

**Or by hand:** fork `microsoft/winget-pkgs`, copy the version folder to `manifests/t/TheYungStump/ColdOpen/<version>/`, and open a pull request. Microsoft's bots validate, install and scan it; a moderator merges it. After that, `winget install TheYungStump.ColdOpen` works for everyone.

Each new release is one more pull request with that release's folder. Use the noreply address as the commit author, as in Cold Open's own repository.
