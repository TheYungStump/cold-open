//! The bundled faces, parsed once, for glyph coverage and metrics.
#![deny(clippy::float_arithmetic)]

use ttf_parser::{Face, GlyphId};
use unicode_width::UnicodeWidthChar;

use crate::page::Coverage;
use crate::text::Look;

/// The four Courier Prime faces, parsed.
pub struct Fonts {
    faces: [Face<'static>; 4],
}

impl Fonts {
    /// Parses the bundled faces. Fails only if the bundled files are damaged,
    /// which the fonts crate's own tests rule out.
    pub fn new() -> Result<Fonts, String> {
        let parse = |f: &'static coldopen_fonts::Face| {
            Face::parse(f.data, 0).map_err(|e| format!("{}: {e}", f.postscript_name))
        };
        Ok(Fonts {
            faces: [
                parse(&coldopen_fonts::REGULAR)?,
                parse(&coldopen_fonts::BOLD)?,
                parse(&coldopen_fonts::ITALIC)?,
                parse(&coldopen_fonts::BOLD_ITALIC)?,
            ],
        })
    }

    /// The face for a look: 0 regular, 1 bold, 2 italic, 3 bold italic.
    pub(crate) fn index(look: Look) -> usize {
        usize::from(look.bold) + 2 * usize::from(look.italic)
    }

    pub(crate) fn face(&self, look: Look) -> &Face<'static> {
        &self.faces[Fonts::index(look)]
    }

    pub(crate) fn faces(&self) -> &[Face<'static>; 4] {
        &self.faces
    }

    /// True when a glyph of face `face` has no advance. The PDF declares
    /// such glyphs zero wide; combining marks are drawn this way
    /// (layout.md §12).
    pub(crate) fn zero_advance(&self, face: usize, gid: u16) -> bool {
        self.faces
            .get(face)
            .is_some_and(|f| f.glyph_hor_advance(GlyphId(gid)) == Some(0))
    }

    /// True for a character of zero display width whose glyph has an
    /// advance anyway, such as the soft hyphen U+00AD. It takes no cell, so
    /// the PDF draws nothing for it (layout.md §12).
    pub(crate) fn draws_nothing(&self, c: char, look: Look) -> bool {
        UnicodeWidthChar::width(c) == Some(0)
            && self
                .face(look)
                .glyph_index(c)
                .is_some_and(|g| g.0 != 0 && !self.zero_advance(Fonts::index(look), g.0))
    }
}

impl Coverage for Fonts {
    fn has_glyph(&self, c: char, look: Look) -> bool {
        self.face(look).glyph_index(c).is_some_and(|g| g.0 != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_matches_the_font() {
        let f = Fonts::new().unwrap();
        let plain = Look::default();
        for c in ['A', 'z', '0', 'é', '’', '“', '—', '…', 'Ł'] {
            assert!(f.has_glyph(c, plain), "{c}");
        }
        for c in ['日', 'Ж', '\u{1F600}'] {
            assert!(!f.has_glyph(c, plain), "{c}");
        }
        let bold_italic = Look {
            bold: true,
            italic: true,
            underline: false,
            ..Look::default()
        };
        assert!(f.has_glyph('A', bold_italic));
    }

    /// The soft hyphen is the only zero-width character in the bundled
    /// faces whose glyph has an advance. Any other would be dropped from the
    /// PDF too, so a font update that adds one needs a look (§12).
    #[test]
    fn only_the_soft_hyphen_draws_nothing() {
        let f = Fonts::new().unwrap();
        for (i, face) in f.faces().iter().enumerate() {
            let look = Look {
                bold: i & 1 == 1,
                italic: i & 2 == 2,
                underline: false,
                ..Look::default()
            };
            assert_eq!(Fonts::index(look), i);
            let mut found = std::collections::BTreeSet::new();
            for st in face.tables().cmap.unwrap().subtables {
                if st.is_unicode() {
                    st.codepoints(|cp| {
                        if let Some(c) = char::from_u32(cp)
                            && f.draws_nothing(c, look)
                        {
                            found.insert(c);
                        }
                    });
                }
            }
            assert_eq!(found, ['\u{AD}'].into(), "face {i}");
            // Combining marks the faces have are zero wide, not dropped.
            let g = face.glyph_index('\u{326}').unwrap();
            assert!(f.zero_advance(i, g.0));
            assert!(!f.draws_nothing('\u{326}', look));
            assert!(!f.draws_nothing('A', look));
        }
    }
}
