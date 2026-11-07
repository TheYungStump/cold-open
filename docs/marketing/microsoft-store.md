# The Microsoft Store: steps and listing

For the project owner. Nothing has been submitted. The package is built and tested in CI (`ci.yml`, store-winget-msix); the release workflow builds the Store's own upload once the identity variables are set (step 4).

**Before you start:** Microsoft's documentation says an individual account publishes "under your own name" (`docs/progress/phase-6c.md`, "Found before the work"). Look at the name Partner Center will show before you finish step 2. If it insists on your legal name and you don't want it public, stop there: a company account avoids it, but costs $99.

## 1. The developer account (yours alone)

1. Go to <https://storedeveloper.microsoft.com> and choose to register as an **individual**. It's free.
2. Sign in with a **personal** Microsoft account. A new one just for the project keeps your own inbox out of it.
3. Do the identity check: a government ID and a selfie.
4. When it asks for a **publisher display name**, read what will show. Try `The Cold Open contributors` or `TheYungStump`; the installer, winget and the website already use the first. If neither is allowed, see the note above.

## 2. Reserve the name

1. In Partner Center: **Apps and games ▸ New product ▸ MSIX or PWA app**.
2. Reserve **Cold Open**. If it's taken, `Cold Open Screenwriter` keeps the search words.

## 3. Read off the identity

In the new app: **Product management ▸ Product identity**. Send me these three values (none is secret):

| Partner Center shows | It goes in |
| --- | --- |
| **Package/Identity/Name** (like `12345TheYungStump.ColdOpen`) | `MSIX_IDENTITY_NAME` |
| **Package/Identity/Publisher** (like `CN=1A2B3C4D-…`) | `MSIX_PUBLISHER` |
| **Package/Properties/PublisherDisplayName** | `MSIX_PUBLISHER_DISPLAY` |

## 4. The package

Put the three values in the repository's **Settings ▸ Secrets and variables ▸ Actions ▸ Variables** (variables, not secrets), under the names above, or send them to me and I'll set them with your OK.

From then on, every release run (and every rehearsal) makes the `store-msix` artifact: `ColdOpen-<version>.msixbundle`, holding x64 and ARM64, plus the two `.msix` files. The Store signs it when you upload it, so nothing needs signing on our side.

## 5. The submission

In the app's page in Partner Center, **Start your submission**:

- **Pricing and availability:** Free; all markets; visibility public (or "hidden, link only" for a quiet first release).
- **Properties:**
  - Category **Productivity**.
  - Privacy policy URL: `https://theyungstump.github.io/cold-open/privacy.html` (Win32 apps always need one, policy 10.5.1).
  - Website: `https://theyungstump.github.io/cold-open/`
  - Support contact info: `https://github.com/TheYungStump/cold-open/issues`
- **Age ratings:** the questionnaire. Cold Open has no user-to-user features, no purchases, no location and no network use. Answer accordingly; it comes out for everyone.
- **Packages:** upload the `.msixbundle` from step 4.
- **The restricted capability (`runFullTrust`):** Partner Center asks why. Use the text below.
- **Store listings (English):** the text below, and the screenshots in `docs/site/static/` (`shot-light.png`, `shot-dark.png`, `shot-changes-light.png`; each is 2560x1720, which the Store accepts).
- **Submission options ▸ Notes for certification:** the text below.

Then **Submit to the Store**. Certification usually takes a few days, and Partner Center emails the result.

## Listing text

**Product name:** Cold Open

**Short description:** A free, open-source screenwriting app that never loses your work.

**Description:**

Cold Open is a free, open-source screenwriting app.

Write in standard screenplay format: Enter and Tab move from scene heading to action to character to dialogue, and names, places and times of day complete themselves.

Never lose work. Every keystroke goes into a crash journal before it shows on screen, and the next start brings it back after a crash or a power cut.

Exchange scripts with anyone. Open and save Final Draft-compatible .fdx files; saving one back writes it as it came. A damaged .fdx file opens with a report of what was repaired. Fountain, a plain-text screenplay format, is Cold Open's own format.

Revise like a production: track changes and accept or reject each one, mark production revisions in color, and compare two drafts scene by scene. Export PDFs laid out the same way on every computer, with a title page and reports.

No account, no subscription, no ads. Cold Open makes no network connections.

Cold Open is in alpha: careful with your work, but new.

"Final Draft" is a trademark of Final Draft, Inc. Cold Open isn't made by, or connected with, Final Draft, Inc.

**Features** (one per line):

- Standard screenplay format with smart element flow
- A crash journal: every keystroke saved before it shows
- Opens and saves .fdx files
- Fountain as its own format
- Track changes, accept or reject
- Production revision colors
- Compare two drafts scene by scene
- PDF export with the same pages everywhere
- Title page editor and reports
- Dark and high-contrast themes
- No account, works offline

**Search terms:** screenwriting, screenplay, script, fdx, fountain, final draft alternative, writing

## Why `runFullTrust` (for the restricted capability)

Cold Open is a desktop (Win32) screenwriting app. It reads and writes the script files a writer opens wherever they keep them, and keeps a crash journal beside each open script so a crash or power cut loses nothing. That needs the full trust of a desktop app. It makes no network connections.

## Notes for certification

Cold Open is a free, open-source screenwriting app (GPL-3.0-or-later; source at https://github.com/TheYungStump/cold-open). This package is the same build as the project's installer and zip on GitHub. No account or sign-in is needed. To try it: start Cold Open, type a scene heading such as "INT. KITCHEN - DAY", press Enter and type action, then Tab for a character name and Enter for dialogue; Ctrl+S saves. The command-line tool is available as `coldopen` from a terminal.
