# coldopen-fonts

The fonts Cold Open bundles:

- **Courier Prime** in four faces, for layout and PDF output;
- **Atkinson Hyperlegible Next**, Regular and Bold, subset to Latin, for the
  desktop app's menus, navigator and dialogs. Only the `ui` cargo feature
  includes these, so the CLI and the layout library never carry them.

Nothing else lives in this crate, so the layout code that uses the fonts
stays MIT OR Apache-2.0.

## License

The font files in `fonts/` are licensed under the SIL Open Font License,
Version 1.1. The license text, with the Courier Prime copyright notice, is in
[`fonts/OFL.txt`](fonts/OFL.txt). The license has no Reserved Font Name, so
subsets embedded in PDFs keep the name Courier Prime. Documents made with the
fonts are not covered by the license.

Every copy of a program that contains the fonts must carry the license text.
`coldopen licenses` prints it, and release packages must include `OFL.txt`.

The Rust source in `src/` is MIT OR Apache-2.0, like the rest of the format
libraries.

## Atkinson Hyperlegible Next (interface font)

Added 2026-10-15 for the desktop app (phase 3).

- **License.** SIL Open Font License 1.1, in
  [`fonts/OFL-AtkinsonHyperlegibleNext.txt`](fonts/OFL-AtkinsonHyperlegibleNext.txt),
  next to Courier Prime's. It declares no Reserved Font Name, so the subsets
  below may keep the name Atkinson Hyperlegible Next.
- **Source.** `https://github.com/googlefonts/atkinson-hyperlegible-next`,
  commit `7925f50f649b3813257faf2f4c0b381011f434f1`, files
  `fonts/ttf/AtkinsonHyperlegibleNext-Regular.ttf`,
  `fonts/ttf/AtkinsonHyperlegibleNext-Bold.ttf` and `OFL.txt`, downloaded
  2026-10-15.
- **Subset.** Each face was subset with fontTools 4.67.0 (Python 3.13):

  ```bash
  python -m fontTools.subset AtkinsonHyperlegibleNext-Regular.ttf \
    --unicodes="U+0020-007E,U+00A0-024F,U+02B0-02FF,U+2000-206F,U+20A0-20CF,U+2190-21FF,U+2318,U+2325,U+2326,U+232B,U+23CE" \
    --layout-features='*' --name-IDs='*' --no-hinting \
    --output-file=AtkinsonHyperlegibleNext-Regular.ttf
  ```

  and the same for Bold. That keeps ASCII, Latin-1, Latin Extended-A and -B,
  spacing modifiers, general punctuation, currency signs and whatever arrows
  and key symbols the font has (it has none of the arrows or key symbols, so
  the app spells keys out as text). Characters outside the subset, such as a
  Greek scene heading in the navigator, fall back to a system font. The page
  itself is always Courier Prime.

| File | Bytes | SHA-256 |
| --- | --- | --- |
| upstream `AtkinsonHyperlegibleNext-Regular.ttf` | 65068 | `88ed5c31a71584c7772963b02d04bef1eb7e3d2e9c8b9cb204339b1f82cf432c` |
| upstream `AtkinsonHyperlegibleNext-Bold.ttf` | 66792 | `994414047df66bb4998d01c1cb1eeb4a2ddc4622d1aa56bbb8adbeca7645b041` |
| `AtkinsonHyperlegibleNext-Regular.ttf` (subset) | 36612 | `fc0bc738a162aa3e4901da4df7c696597991acebd31fabd01ee228e7bb2ed77e` |
| `AtkinsonHyperlegibleNext-Bold.ttf` (subset) | 37128 | `daed73089214e525d0aa38fe52c24d61f2e0db613fd6c216537aeb53bec1d269` |
| `OFL-AtkinsonHyperlegibleNext.txt` (upstream `OFL.txt`) | 4431 | `aca6a428580965d2297d1b718042dd427c2a9443ece3b0d02d758e161e0c4030` |

Subsetting saves 57 KB: the two subsets add 73,740 bytes to the app, about
34.5 KB compressed. A unit test (with `--features ui`) checks each subset's
length and hash.

## Courier Prime source

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `CourierPrime-Regular.ttf` | 71188 | `72f793376f8e2841656bf21d77a5de010f2929bd6956a22ee848ad0c7eb978af` |
| `CourierPrime-Bold.ttf` | 72856 | `ff1f38786c849d1c41fa8e447960abdb2bd75fdfb0cfcdeb524fad65a5af3638` |
| `CourierPrime-Italic.ttf` | 79980 | `f1b9a5829789f7e56432a9f3bc7665ef4531dbba1c112639e48bef39621a006b` |
| `CourierPrime-BoldItalic.ttf` | 80872 | `3355273f3ea6d6362658f9564f4e83d59e156cf1a182e3c1592b5c5943627dcc` |
| `OFL.txt` | 4403 | `9a755af092b494944c99f471be6fddd19b006a448fefdc4717e4ee0aa09a97b0` |

- Downloaded 2026-10-13 from the Google Fonts repository,
  `https://raw.githubusercontent.com/google/fonts/main/ofl/courierprime/`.
- That directory's `METADATA.pb` names the upstream source as
  `https://github.com/quoteunquoteapps/CourierPrime`, commit
  `7fd585a2dd4c1612c79b3308e300923d1c13df93`.
- The files are unchanged. A unit test checks each face's length and hash, so
  a damaged checkout fails loudly.
- Courier Prime covers ASCII, Latin-1, Latin Extended-A and the usual smart
  punctuation. Its advance width is 1228/2048 em; layout places glyphs on a
  7.2 pt grid regardless (`docs/specs/layout.md` §12).
