# Layout fixtures

Short scenes written by us, one pagination rule per case, each checked
against a golden page view. The rules are in
[`docs/specs/layout.md`](../../../docs/specs/layout.md); the page view format is
its §11. No script text is copied from anywhere.

## Files per case

| File | Purpose |
| --- | --- |
| `<case>.fountain` | The input. |
| `<case>.format.json` | Optional. A JSON merge patch over the default page format. `"extends": "small"` first applies `_small.format.json`, a 12-line test page that keeps the rule cases short. |
| `<case>.options.json` | Optional. Layout options, such as `{"scene_numbers": true}`, and `"decides"` or `"records"` (below). An unknown key fails the test, so a typo is never ignored. |
| `<case>.pages.txt` | The golden page view, reviewed by hand. |
| `<case>.pdf`, `<case>.pdf.txt` | For a few cases: the golden PDF bytes, and a readable dump of it with its streams decompressed. |

## Case prefixes

| Prefix | Area | Spec |
| --- | --- | --- |
| `sp` | Spacing, and space dropped at the top of a page | §6 |
| `k` | Keeps: headings, names, transitions | §7.3 rules 1, 2, 6 |
| `s` | Splits at sentence ends, mid-line and at line ends; lyrics; centered text | §7.3 rules 3, 5; §7.4 |
| `i` | (MORE) and continued names | §7.5 |
| `p` | Parentheticals | §7.3 rule 4 |
| `dd` | Dual dialogue | §4.4, §7.3 rule 7 |
| `x` | Explicit page breaks | §4.5 |
| `f` | Fallbacks when no rule-abiding break exists | §7.6 |
| `d` | The default US Letter format: title page, styles, non-ASCII text, scene numbers, custom margins, several pages | §3.2, §9 |
| `t` | Title pages: a title that continues on a second page, bottom blocks in source order | §9.3 |

## Cases that decide a rule

A case that is there to show a page-break rule says so in its options:
`{"decides": ["heading"]}`. The test then lays the case out again with each
named rule turned off, and fails unless some line moves or a page ends
elsewhere (page headers are left out of that comparison). That catches a
golden that passes for some other reason, such as a (MORE) line with no room,
and so would keep passing if the rule broke.

A level-1 fallback breaks only the sentence rule, so turning that rule off
moves nothing; only the page's recorded break cause changes. Such a case says
`{"records": ["sentences"]}` instead, and the test checks that the recorded
cause changes while every line stays put (f01, f02).

| Name | Turned off | Spec |
| --- | --- | --- |
| `heading` | Rule 1, headings keep two lines | §7.3 rule 1 |
| `name` | Rule 2, the character-name keep | §7.3 rule 2 |
| `parenthetical` | Rule 4, parentheticals | §7.3 rule 4 |
| `transition` | Rule 6, no break before a transition | §7.3 rule 6 |
| `sentences` | Splits only at sentence ends | §7.4 |
| `min_lines` | The two-line minimum of rules 1, 2, 3 and 5, lowered to one line | §7.3 |

These are the switches in `coldopen_layout::Rules`. Some rules overlap, and a
case can only decide a rule where no other rule makes the same choice:
- `name` decides, with lines moving, only for a name with nothing after it (k05). Wherever a
  group has dialogue, rule 3's two lines on each side are at least as strict
  as rule 2, so k06 shows rule 2's two lines with `min_lines`.
- A break before a trailing parenthetical always leaves no dialogue after
  it, so rule 3 refuses it as well as rule 4. p02 decides `parenthetical`
  through rule 4's other half: the page cannot end right after it either.

A case that shows where a rule stops applying cannot decide it, and leaves the
key out. k03 is one: a heading with two lines after it stays put.

## Updating goldens

`crates/layout/tests/fixtures.rs` lays out every case, checks that layout is
deterministic and that each page can be laid out again from its own start,
then compares the page view with the golden. After a deliberate change, review
the new output (a failing run writes it under Cargo's target directory) and
rerun with `COLDOPEN_BLESS=1`. Blessing is refused when the `CI` variable is
set, and CI checks the same goldens on Windows, macOS and Linux.

Bytes are exact: `tests/fixtures/**` is `-text` in `.gitattributes`.
