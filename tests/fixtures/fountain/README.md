# Fountain conformance fixtures

These are short scenes written by us, one rule per file. Each case checks a rule from the public syntax page at fountain.io, as restated in [`docs/specs/fountain.md`](../../../docs/specs/fountain.md). No script text is copied from anywhere.

## Files per case

| File | Purpose |
| --- | --- |
| `<case>.fountain` | The input. |
| `<case>.expected.jsonl` | The expected parse, **written by hand**. Each line is one JSON object, in the same shape the command API uses (without ids). An optional first line `{"title_page": …}` holds the title page. Lines starting with `//` are comments. |
| `<case>.out.fountain` | The canonical text the writer must produce. Present only when it differs from the input. |
| `<case>.diagnostics` | The diagnostic codes the parser must report, one per line. Present only when there are any. |

## What the harness checks

The harness is `crates/fountain/tests/conformance.rs`. For each case it checks that:

- parsing gives exactly the expectation;
- writing the expectation gives the canonical text, with no losses;
- the canonical text parses back to the expectation;
- writing is idempotent.

## Bytes are exact

`.gitattributes` marks this directory `-text`. That keeps the CRLF, CR and BOM cases byte-exact on every OS. Create such files with `printf`, not an editor.

## Case prefixes

| Prefix | Area | Spec |
| --- | --- | --- |
| `sh` | Scene headings | §6.2 |
| `a` | Action | §6.4, §7 |
| `c` | Character | §6.2 |
| `d` | Dialogue | §6.3 |
| `p` | Parentheticals | §6.3 |
| `dd` | Dual dialogue | §6.2 |
| `l` | Lyrics | §6.1, §6.3 |
| `t` | Transitions | §6.2 |
| `ct` | Centered text | §6.1 |
| `e` | Emphasis and escapes | §8 |
| `tp` | Title page | §4 |
| `pb` | Page breaks | §6.1 |
| `pu` | Punctuation | §8 |
| `i` | Indenting | §6 |
| `n` | Notes | §5 |
| `b` | Boneyard | §5 |
| `s` | Sections and synopses | §6.1 |
| `r` | Robustness: line endings, BOM, empty files, malformed input | §3 |
| `z` | A short scene using most of the syntax | all |
