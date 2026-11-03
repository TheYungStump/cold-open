# Setting up the feedback form

For the project owner (phase 6b). The form is how testers without a GitHub account send a problem, a crash report or an idea. Cold Open's **Report a problem…** and the crash report's **Send with the form** open it with the version, OS and renderer filled in. Answers come to you privately.

It takes about 15 minutes. When it's done, send back the two things in the last step, and I'll wire them into the app and the website.

**Done 2026-11-03.** The form is wired into the app (`crates/app/src/feedback.rs`), the website's feedback page and the issue chooser. This page stays as the record of how it was made, and for making it again.

## 1. Use a project Google account

Make the form from a Google account for the project, not your personal one: for example a new account named "Cold Open project". Respondents don't see the owner's address, but anything later shared from the form (a linked Sheet, a collaborator invite, an email notification forwarded) would show it. A separate account keeps your name out of all of it.

## 2. Create the form

At [forms.google.com](https://forms.google.com), signed in to the project account, start a **Blank form**.

- **Title:** Cold Open feedback
- **Description:**
  > Tell us about a problem, a crash, or an idea. Your answers come to the Cold Open project privately. Please don't paste a script you don't own: a short scene of your own that shows the problem is best.

## 3. The questions

Add these seven, in this order, with these exact titles and options. The app fills in four of them, and it matches the choices **Bug** and **Crash** by their exact text.

| # | Question title | Type | Required | Options or help text |
| --- | --- | --- | --- | --- |
| 1 | What is it about? | Multiple choice | Yes | Bug · A file won't open or saves wrong · Crash · Idea · Something else. The first and third must read exactly **Bug** and **Crash**. |
| 2 | What happened? | Paragraph | Yes | Help text: "What were you trying to do, what did you expect, and what happened instead?" |
| 3 | Version | Short answer | No | Help text: "About Cold Open shows it. Report a problem fills it in." |
| 4 | Operating system | Short answer | No | |
| 5 | Renderer | Short answer | No | Help text: "CPU or GPU, from About Cold Open." |
| 6 | Crash report | Paragraph | No | Help text: "If Cold Open crashed, it copied the whole report to your clipboard: paste it here." |
| 7 | How can we reach you? (optional) | Short answer | No | Help text: "Only if you'd like a reply. Leave it empty to stay anonymous." |

Don't add a **File upload** question. It makes every respondent sign in to Google.

## 4. Settings

In the form's **Settings** tab:

- **Responses ▸ Collect email addresses:** *Do not collect*.
- **Responses ▸ Limit to 1 response:** *off* (it needs sign-in).
- **Responses ▸ Allow response editing:** off.
- **Presentation ▸ Confirmation message:** "Thank you. We read every answer."
- **Defaults:** leave "Make questions required by default" off.

In the **Responses** tab:

- ⋮ ▸ **Get email notifications for new responses**, so you know when one arrives.
- Optionally, **Link to Sheets** to keep responses in a spreadsheet. Keep the Sheet unshared.

## 5. The prefilled link

1. Click ⋮ (top right) ▸ **Get pre-filled link**.
2. Fill in the questions with these placeholders, so I can tell which id is which:
   - What is it about?: choose **Bug**
   - What happened?: `DETAILS`
   - Version: `VERSION`
   - Operating system: `OS`
   - Renderer: `RENDERER`
   - Crash report: `REPORT`
   - How can we reach you?: leave it empty
3. Click **Get link**, then **Copy link**.

The link looks like `https://docs.google.com/forms/d/e/1FAIpQL…/viewform?usp=pp_url&entry.123456789=Bug&entry.987654321=DETAILS&…`. Each `entry.<number>` is one question's id.

## 6. Check it

Open the copied link in a private browser window, where you're signed out of Google. The form should open with the placeholders filled in, and submit without asking you to sign in. Delete that test response afterwards (Responses ▸ ⋮ ▸ Delete all responses).

## 7. Send back

- **The prefilled link** from step 5, whole.
- **The form's public link** (Send ▸ the link icon ▸ Copy), if you'd like a short one for the website. Otherwise I'll use the address in the prefilled link.

With those, I'll:

- set `FORM_URL` and the question ids in `crates/app/src/feedback.rs`, which shows the **Use the form** and **Send with the form** buttons;
- link the form from the website's "Send feedback without an account" page and the issue chooser;
- add a test that the prefilled address holds only the version, OS, renderer and kind.
