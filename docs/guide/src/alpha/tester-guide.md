# Tester guide

Two tracks: one for anyone, and a deeper one for screenwriters. Do either, or both, or part of one. Note anything that surprised you, confused you or got in your way. That's the most useful thing you can send.

**Keys:** Mod means Ctrl on Windows and Linux, Cmd on a Mac. **The palette** (Mod+K) lists everything, so if a step names something you can't find, type a word of it there.

## For anyone (about 20 minutes)

### 1. Install (5 minutes)

[Download](../download/) the alpha and follow [Install](../guide/install.md) for your system. Note anything about the warnings or the install that was unclear.

### 2. Write a short scene (5 minutes)

Cold Open starts on an empty scene heading. Type a scene of your own, or this one, using Enter and Tab to move between elements (the bar at the top says what each will do):

- a scene heading: `int. kitchen - morning`, then Enter;
- some action: `The kettle screams.`, then Enter, then Tab;
- a character: `sam`, then Enter;
- dialogue: `Who left this on?`, then Enter;
- another character: `alex`, then Enter, then Tab for a parenthetical: `from upstairs`, then Enter;
- dialogue: `Not me.`

Did Enter and Tab do what you expected? Did the suggestions (names, `INT.`, times of day) help or get in the way?

### 3. Save it (1 minute)

Press Mod+S and save it as `kitchen.fountain` somewhere you'll find it.

### 4. Crash it on purpose, and get your work back (3 minutes)

1. Type one more line of action: `The kettle stops.`. Don't save.
2. Open the palette, choose **Crash on purpose (to test recovery)**, and press Enter. Cold Open closes at once, as if it had crashed.
3. Start Cold Open again, and open `kitchen.fountain`: it's in the recent files, and in the bar offering your unsaved work.
4. A dialog says what came back. Is your last line there? Choose **Keep**.
5. A notice at the top says Cold Open closed unexpectedly and saved a crash report. Click **Look at it**. Read it: it should hold nothing from your script. If you're happy to, click **Send on GitHub** or **Send with the form** and send it to us, so we can check the whole path works.

### 5. Track a friend's changes (5 minutes)

You'll play both people.

1. In the palette, choose **Track my changes**. Give your name when it asks.
2. Change a word or two in the scene. The new text shows in color, and anything deleted is struck through.
3. In the palette, choose **Change my name for changes…** and type a friend's name. Change another line, as them.
4. Press Mod+Shift+R for the changes panel. It lists each change with its author.
5. Accept one change (A) and reject another (R). Mod+Z undoes either.

Was it clear whose change was whose? Could you find the changes again?

### 6. Export a PDF (2 minutes)

Press Mod+Shift+E. Tick **Scene numbers**, type `DRAFT` as the watermark, press Enter, and save it. Open the PDF. Does it look like a script page should?

### 7. Open an .fdx (2 minutes)

1. In the palette, choose **Save a copy as .fdx** and save `kitchen.fdx`.
2. Press Mod+O and open `kitchen.fdx`. It should look the same.

If you have an .fdx of your own, open a copy of that too.

### 8. Tell us

[Tell us how it went](join.md#tell-us): the Alpha testing discussion, the form, or Report a problem in the app.

## For screenwriters (an hour or two)

Use **copies** of scripts you own, never the only copy, and never someone else's script. Don't attach or paste them anywhere public.

### 1. Bring in a draft

Open a copy of a recent draft, as `.fdx` or `.fountain`.

- Did it open without complaint? If it says it repaired the file, what did the report say?
- **The page count:** is it what you expect from your usual app, or within a page? Are scene starts on the right pages?
- **The title page:** is everything there?
- Click through the navigator. Are the scenes, numbers and lengths right?

### 2. Edit and save it back

Rewrite a scene the way you normally would. Then save it back to its own format.

- If it was an .fdx, open the saved file in the app you usually use. Is anything missing or moved?
- Open it again in Cold Open. Is it as you left it?

### 3. Production revisions

1. In the palette, choose **Start revision…**, keep Blue, and press Enter.
2. Make several changes across a few scenes.
3. Export a PDF with **Colored pages** (Mod+4 in the export form).

Are the `*` marks in the margin where you'd expect? Do the revised pages' headers say "Blue Rev." and the date? Would a production be happy with these pages?

### 4. Compare drafts

If you have two drafts of the same script, open one and choose **Compare with…** in the palette, then pick the other.

- Are the changed scenes the ones you know changed? Are moved scenes shown as moved?
- **Save as PDF…** and look at the marked pages.

Then try **Review a draft as changes…** with the same draft. Do the changes it brings in match what you'd expect?

### 5. Reports

Press Mod+Shift+I. Look at Scenes, Characters and Locations.

- Do the counts look right for your script?
- **Copy as CSV** (Mod+C) and paste it into a spreadsheet.

### 6. An AI agent (optional)

If you use Claude or another MCP client, connect it ([AI agents](../guide/agents.md)) and ask it to tighten a scene's action. Its edits should arrive as tracked changes under its name, for you to accept or reject.

### 7. Measure performance

Choose **Measure performance** in the palette. It takes about two minutes, and its windows come and go. Send us the two files it writes to your Downloads folder, especially from a Mac or Linux.

### 8. Tell us

[Tell us how it went](join.md#tell-us), including what you'd need before you'd write your next draft in Cold Open.
