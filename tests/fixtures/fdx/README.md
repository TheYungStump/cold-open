# .fdx fixtures

These are short scenes written by us, one area per file, for the rules in [`docs/specs/fdx.md`](../../../docs/specs/fdx.md). Each file holds only the elements its case needs. No script text or settings block is copied from any sample file.

## Files per case

| File | Purpose |
| --- | --- |
| `<case>.fdx` | The input. |
| `<case>.expected.json` | What `coldopen_fdx::read` must return: `script` (the serde form of `Script`), `overrides`, `title_lines`, `scenes`, the `report` codes in order, and `format` as a JSON merge patch (RFC 7386) against `PageFormat::default()`. |

## The harness

`crates/fdx/tests/fixtures.rs` reads every `<case>.fdx`, compares each field with the expectation, and also runs the fuzz checks (`coldopen_fdx::fuzzing::read`) on it.

To regenerate the expectations after a deliberate change, run

```
COLDOPEN_BLESS=1 cargo test -p coldopen-fdx --test fixtures
```

and review the diff line by line before committing. Blessing is refused when `CI` is set.

## Bytes are exact

`.gitattributes` marks this directory `-text`. `utf16le-bom.fdx` (UTF-16 little-endian with a byte-order mark) and `windows-1252.fdx` are binary on purpose; create such files with a script, not an editor.

## Cases

| Case | Covers | Spec |
| --- | --- | --- |
| `basic` | The nine paragraph types, a line break, untrimmed spaces | §6.1, §6.2 |
| `styles` | Style tokens, `+` combinations, `AllCaps`, unknown tokens, run attributes, a tab | §6.2 |
| `dual-dialogue` | Two speakers, a group that does not fit, text in the outer paragraph | §6.6 |
| `scene-numbers` | `Number`, trimming, an empty number, `SceneProperties` | §6.3 |
| `starts-new-page` | Page breaks, the first paragraph, an act break's own settings, an inline `No` | §6.5 |
| `inline-overrides` | Centered action, `SpaceBefore` overrides, ignored indents and spacing | §6.4 |
| `settings` | Custom margins (81/54 pt), sentence breaking off, all nine kinds' settings, rounding, `Full`, `Spacing`, fonts, `PaginateAs`, scene-number positions, the header | §5 |
| `mores` | Lower-case insert strings, scene continueds on | §5.1 |
| `mores-off` | Inserts off, a hidden header, `StartingPage` | §5.1, §5.3 |
| `title-page` | Title lines, `AllCaps`, `Full`, a bad column, missing attributes | §7 |
| `entities` | Predefined entities, character references, CDATA, an unknown entity, a duplicate attribute | §2.3, §2.4 |
| `truncated` | A file cut off inside a tag | §2.2 |
| `mismatched-end-tag` | An end tag that closes too much, a stray end tag | §2.2 |
| `utf16le-bom` | UTF-16 little-endian with a byte-order mark | §2.1 |
| `windows-1252` | windows-1252 bytes with a matching declaration | §2.1 |
| `unknown-type` | Types we don't model, a missing `Type`, case-insensitive types, an unknown child | §6.1, §6.2 |
| `trailing-empty` | Interior empty paragraphs kept, trailing empty general dropped with its break | §6.7 |
| `invalid-format` | Margins larger than the page | §5.4 |
| `out-of-range` | Paragraph and title-page `SpaceBefore` below 0 or above the body height, a `Spacing` whose product does not fit a length | §5.2, §6.4, §7 |
| `long-inserts` | (MORE), (CONT'D) and page-number suffix strings longer than the format allows, one of them over two lines | §5.1, §5.3 |
