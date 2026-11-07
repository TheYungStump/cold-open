# Flathub

**Read this first: these files can't be what you submit.** Flathub's requirements (<https://docs.flathub.org/docs/for-app-authors/requirements>, read 2026-11-06) say "Flathub manifests must not contain AI-generated or AI-assisted content", and that "AI tools or agents must not open or automate Flathub submission pull requests". This manifest was written with an AI assistant. It stays here as **CI's test that Cold Open builds offline from source** in Flathub's own container (`ci.yml`, store-flathub), which is what a submission needs to work. The manifest you submit has to be written by you.

Two more things make Flathub a later step rather than now:

- **Stable only:** "The Flathub stable repository is dedicated to hosting only stable software." Cold Open is an alpha. Submit once there's a stable release.
- **The app ID and the repository:** for an `io.github.` ID, Flathub takes the repository's address from the ID's last part "as-is": `io.github.theyungstump.coldopen` points at `github.com/TheYungStump/coldopen`, but the repository is `cold-open`. Either ask the reviewers for an exception in the pull request, or change the ID to `io.github.theyungstump.cold-open`. The ID is in every Linux package, so changing it is a change of its own, best made before Flathub.

## What a submission needs (from Flathub's pages)

- **Built entirely from source,** offline: "There is no network access during the build process." Every crate is a source with a public address, which `flatpak-cargo-generator` makes from `Cargo.lock` (the release workflow does this: the `store-files` artifact holds a `cargo-sources.json`).
- **No binaries in the pull request:** "Binary or precompiled files must not be present."
- **A valid MetaInfo file** with the license "correctly specified", a desktop file, and a 256x256 PNG or an SVG icon. The ones here pass Flathub's linter in CI.
- **Permissions kept to "an absolute minimum"**, and the pull request should "explain why the app needs its permissions". Cold Open asks for the home folder (`--filesystem=home`) because a script's crash journal and atomic save are written beside it, in its folder, which a file chooser portal doesn't give. Expect the reviewers to ask about it.
- **AI disclosure:** Flathub asks apps whose code or docs are AI-generated to say so. Cold Open's README already does.
- **Submitting:** fork `flathub/flathub` with "Copy the master branch only" **unticked**, branch from `new-pr`, add your manifest, and open the pull request against `new-pr`, titled "Add io.github.theyungstump.coldopen". After it's accepted, verify the app in Flathub's developer portal while signed in as the repository's owner.

## Files here

| File | What it is |
| --- | --- |
| `io.github.theyungstump.coldopen.yml` | CI's from-source build test, and a reference for what yours needs: the runtime, the Rust extension, the permissions, the install steps |
| `io.github.theyungstump.coldopen.metainfo.xml` | AppStream data with the fields Flathub's guidelines look for |
| `ci_manifest.py` | Points the manifest at the checkout, for CI |
