# Known limitations of the alpha

Cold Open is an alpha. Everything in this guide works, and it's careful with your work. But it's new, and some things aren't there yet. Here they are, honestly.

## Back up your scripts

**Keep copies of anything that matters,** as you would with any new program: in another folder, on another drive, or in version control. Cold Open's journal and atomic saves protect against crashes and power cuts, but an alpha can have bugs nobody has found yet. Your scripts are ordinary Fountain and .fdx files, so a copy opens anywhere.

## Installing

- **The builds aren't signed** by Microsoft or notarized by Apple yet, so Windows and macOS warn the first time (see [Install](install.md)). Windows signing comes with SignPath Foundation's approval; Apple's notarization needs a paid account.
- **On macOS, double-clicking a script** in Finder starts Cold Open but doesn't open the script. Use Open (Cmd+O).
- **On Windows, Cold Open isn't made the default app** for `.fountain` and `.fdx` files. It's offered in "Open with", and Windows lets only you choose a default.
- **No automatic updates.** Cold Open never checks the internet. New releases are on the [download page](../download/).
- **The Flatpak** isn't on Flathub yet, and sees only your home folder unless you give it more (see [Install](install.md#linux)).
- **The icon** is a stand-in.

## Files

- **.fdx files can't carry notes, boneyard, sections or synopses** from Cold Open, nor tracked changes (they're written accepted) or revision marks (they're left out). No public description of how .fdx stores them exists, so Cold Open asks before such a save and offers a project, which keeps everything. See [.fdx exchange](fdx.md).
- **No PDF import.** Cold Open exports PDFs, but can't read one back.
- **Templates:** feature film only. One-hour drama, multi-camera sitcom, stage play and others come later.
- **A4** isn't selectable yet: a new script is US Letter. An .fdx file keeps its own page settings.

## Writing

- **No spellchecker** yet.
- **Dual dialogue** is set with the palette ("Toggle dual dialogue" on the second character), not with a button.
- **Notes and boneyard inside a line** are kept and shown in the margin, but you can't edit them inside the line yet. Edit them in the Fountain file, or with an agent.
- **No screen-reader support** yet, and **no input methods for Chinese, Japanese or Korean.** Accented letters, dead keys and AltGr work.
- **One window** at a time, and no native macOS menu bar beyond **Settings…** in the app menu: everything else is in the palette and on the keys.
- **Shortcuts** are changed in `keymap.json`, not in Settings (Mod+Comma), which holds the theme, sizes, the renderer and the privacy choices.

## Production

- **Locked pages, A and B pages, and locked scene numbers** (12A, 12B) aren't there yet. Scene numbers can be set by hand.
- **Omitted scenes and production breakdown tags** come later.

## Compare and import

- **Bringing in a draft as changes** can't bring styles, notes, boneyard, element settings (centering, blank lines) or the title page. They're listed for you to apply by hand.
- **A section just above a scene heading** belongs to the scene before it when comparing, so moving a section with its scene reads as the section moving.

## Crash reports

A crash report is written when Cold Open itself stops on an error. It isn't written when Cold Open is closed from outside, when the computer loses power, or when a graphics driver or the system fails underneath it. Your typing is safe in each case: the crash journal covers all of them.

## Found something else?

Please [tell us](../alpha/join.md#tell-us). That's what the alpha is for.
