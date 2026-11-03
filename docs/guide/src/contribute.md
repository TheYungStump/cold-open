# Contribute

Thank you for wanting to help. During the alpha, the most valuable help isn't code: it's using Cold Open and telling us how it went.

## Ways to help now

- **Test it.** Follow the [tester guide](alpha/tester-guide.md), on any system, and [tell us how it went](alpha/join.md#tell-us).
- **Report bugs.** [GitHub issues](https://github.com/TheYungStump/cold-open/issues/new/choose), **Report a problem…** in the app, or the [form](alpha/feedback.md). [What feedback helps](alpha/what-helps.md) says what makes a report easy to fix.
- **Send crash reports.** If Cold Open crashes, the next start offers to show you the report, and you can send it.
- **Measure it.** **Measure performance** in the palette, on a Mac or Linux especially, and send us the two small files it writes.
- **Share .fdx problems** with a short scene of your own, or the output of `coldopen repair --json`. Never a script you don't own.
- **Improve the docs.** Something unclear or wrong on this site? Open an issue, or start a discussion. Every page has an edit link (the pencil at the top), which shows its source.
- **Talk ideas through** in [Discussions](https://github.com/TheYungStump/cold-open/discussions).
- **Tell people** who might like it: screenwriters, Linux users, Fountain fans.

## Code contributions

**Pull requests aren't accepted yet.** Before they are, Cold Open needs a contributor license agreement, which a lawyer will check first. It will let the project publish signed builds in app stores, whose terms don't fit outside contributions under the GPL alone. Until then, please open an issue or a discussion instead: a well-described bug or idea is as useful as a patch.

When pull requests open, this page will say how. Meanwhile, the code and its design are all public:

- the [repository](https://github.com/TheYungStump/cold-open), with build and test instructions in its README;
- the [product spec](https://github.com/TheYungStump/cold-open/blob/main/docs/spec.md) and the behavior [specs](https://github.com/TheYungStump/cold-open/tree/main/docs/specs);
- a record of how each phase was built, in [docs/progress](https://github.com/TheYungStump/cold-open/tree/main/docs/progress).

## Licenses

- The app, the command-line tool and the agent server are under the **GPL-3.0-or-later**.
- The Fountain and .fdx libraries, the layout engine, the diff library and the font crate are also available under **MIT or Apache-2.0**, so other apps can use them.
- The bundled fonts, Courier Prime and Atkinson Hyperlegible Next, are under the **SIL Open Font License 1.1**.

## A clean room

Cold Open matches what screenwriters need from other apps without using any of their code, files or text. Its knowledge of `.fdx` comes from publicly shared sample files, written up in the project's own specs. So please don't send material from another app's software, manuals or help pages, even to help.

## Be kind

Be considerate in issues and discussions. Everyone here is trying to make a good tool.
