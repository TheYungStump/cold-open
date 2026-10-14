# coldopen-fonts

The fonts Cold Open bundles for layout and PDF output: **Courier Prime** in
four faces. Nothing else lives in this crate, so the layout code that uses the
fonts stays MIT OR Apache-2.0.

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

## Source

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
