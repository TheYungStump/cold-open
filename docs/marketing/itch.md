# itch.io page (draft) and steps

For the project owner. Nothing has been created or uploaded.

## Steps

1. **An itch.io account** (free), signed up with the project's address rather than a personal one if you can.
2. **Create a new project** (Dashboard ▸ Create new project):
   - **Title:** Cold Open. **Project URL:** `cold-open`.
   - **Kind of project:** Downloadable. **Classification:** Tools.
   - **Pricing:** "No payments", or "$0 or donate" if you'd like the option; itch.io's revenue share only applies to money paid.
   - **Uploads:** leave empty; the release workflow fills them (step 4).
   - **Genre/tags, description, screenshots:** below.
   - **AI generation disclosure:** itch.io asks projects to "accurately tag your project if it contains materials produced by generative AI". Cold Open's code is AI-assisted, as its README says, so answer that question yes and describe it as below.
   - **Visibility:** Draft until the first upload is in, then Public.
3. **An API key:** <https://itch.io/user/settings/api-keys> ▸ Generate new API key. In the cold-open repository, add it as an Actions **secret** named `BUTLER_API_KEY`, and add a **variable** named `ITCH_PROJECT` holding `<your itch username>/cold-open`.
4. **The uploads:** the next tagged release's `itch` job pushes each build with butler to its own channel. The channel names tag the platforms automatically: `windows-x64`, `windows-arm64` (Windows), `mac-universal` (macOS), `linux-x64`, `linux-arm64` (Linux, the AppImages). itch.io asks that you only mark platforms you've tested; all five are installed and started in CI.

## Page text

**Short description:** A free, open-source screenwriting app that never loses your work.

**Description:**

Cold Open is a free, open-source screenwriting app for Windows, macOS and Linux.

- Write in standard screenplay format. Enter and Tab move from scene heading to action to character to dialogue, and names, places and times of day complete themselves.
- Never lose work. Every keystroke is saved to a crash journal before it shows on screen.
- Open and save Final Draft-compatible .fdx files. Fountain, a plain-text screenplay format, is its own format.
- Track changes, production revision colors, draft comparison, a title page editor, reports and PDF export.
- No account, no subscription. Works offline.

It's in alpha: careful with your work, but new. Website and docs: https://theyungstump.github.io/cold-open/ · Source: https://github.com/TheYungStump/cold-open

*About AI:* much of Cold Open's code was written with AI coding assistants under the developer's direction, and every change was reviewed. The app itself uses AI only if you connect an agent yourself.

**Tags:** screenwriting, writing, screenplay, open-source, linux, productivity

**Screenshots:** `docs/site/static/shot-light.png`, `shot-dark.png`, `shot-changes-light.png`. **Cover image** (630x500): crop the share image, `docs/site/static/share.png`.
