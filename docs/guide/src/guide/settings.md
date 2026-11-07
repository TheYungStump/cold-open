# Settings and the usage summary

*New in 0.1.0-alpha.3.*

**Settings** (Ctrl+, on Windows and Linux, Cmd+, on a Mac, also in the palette, on the start screen and in About) gathers Cold Open's settings in one place. On a Mac it's also **Cold Open ▸ Settings…** in the menu bar. Each change applies at once and is kept for next time.

| Setting | What it does |
| --- | --- |
| **Theme** | Follow the system, light, dark, or high contrast |
| **Interface size** | 100% to 200%, for the whole window, pages included |
| **Renderer** | Automatic, CPU or GPU. It changes at the next start. Automatic uses the GPU on a Mac and the CPU elsewhere |
| **Your name on tracked changes** | The name your tracked changes carry ([Track changes](track-changes.md)) |
| **Agents can attach** | Whether an agent you connect yourself may attach to scripts open here ([AI agents](agents.md)) |
| **Crash reports** | Ask after a crash, or off ([Your work is safe](recovery.md)) |
| **Usage summary** | Counting on or off, **View…** and **Reset…** (below) |

**From the keyboard:** Up and Down choose a row, Left and Right change it, Space or Enter switches an on-or-off row, and Esc closes. On the Usage summary row, Tab moves between its switch, View… and Reset….

## The usage summary

Cold Open keeps a small summary of how it's used, on your computer only, so that feedback can say which features you rely on. **It never sends it.** It goes anywhere only if you tick **Include my usage summary** when you share feedback, and then you see all of it first.

**What it counts:**

- which commands and actions you use, by name (such as `text.insert` or `changes.accept`), and how often;
- how many times you start Cold Open, and roughly how long each session lasts (under 5 minutes, up to 30, up to 2 hours, or longer);
- which formats you open and save: Fountain, .fdx or a Cold Open project;
- whether the CPU or the GPU draws the window, and Cold Open's version;
- the day counting started.

**What it never holds:** your text, the names of characters, scenes or anyone, file names or folders, or any time finer than a day.

- **View it:** Settings ▸ Usage summary ▸ **View…**, or **Usage summary…** in About. It shows exactly what's in the file, in plain words. **Copy** copies it.
- **Reset it:** **Reset…** deletes it, after asking. Counting starts again from that day.
- **Turn it off:** Settings ▸ Usage summary ▸ **Counting off**. Then nothing is counted or written.

It's kept in `usage.json` in Cold Open's own folder (see [Install](install.md#where-cold-open-keeps-things)). See also [Privacy](../privacy.md).

## Share feedback

**Share feedback…** (in the palette, About and the start screen) opens a box for what works, what doesn't and what you'd like.

1. Type your feedback.
2. If you'd like, tick **Include my usage summary**. The whole summary appears under the tick, exactly as it will go.
3. Choose **Open the form** (no account needed), **Open a GitHub issue**, or **Copy all**.

Cold Open opens the page in your browser with your text filled in, along with Cold Open's version and your system. **Nothing is sent until you submit that page yourself.** If it's too long for the page's address, Cold Open copies it all to your clipboard and says so: paste it into the page.

**Keys:** while you're typing, Ctrl+Enter (Cmd+Enter on a Mac) opens the form. Esc or Tab leaves the text box; then U ticks the summary, F opens the form, G the GitHub issue, C copies all, T goes back to typing, and Esc closes.

**Report a problem…** has the same **Include my usage summary** tick.
