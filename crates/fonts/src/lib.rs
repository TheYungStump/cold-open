//! The fonts Cold Open bundles: Courier Prime Regular, Bold, Italic and Bold
//! Italic, under the SIL Open Font License 1.1 ([`LICENSE`]).
//!
//! Every copy of a program that contains these fonts must carry the license
//! text; the `coldopen licenses` command prints it. Documents made with the
//! fonts, such as exported PDFs, are not covered by the license.
//!
//! Where the files come from, and their hashes, is recorded in
//! `crates/fonts/README.md`.
#![forbid(unsafe_code)]

/// One bundled face.
#[derive(Debug)]
pub struct Face {
    /// The face's PostScript name.
    pub postscript_name: &'static str,
    pub bold: bool,
    pub italic: bool,
    /// The TrueType file, unchanged.
    pub data: &'static [u8],
}

pub static REGULAR: Face = Face {
    postscript_name: "CourierPrime-Regular",
    bold: false,
    italic: false,
    data: include_bytes!("../fonts/CourierPrime-Regular.ttf"),
};

pub static BOLD: Face = Face {
    postscript_name: "CourierPrime-Bold",
    bold: true,
    italic: false,
    data: include_bytes!("../fonts/CourierPrime-Bold.ttf"),
};

pub static ITALIC: Face = Face {
    postscript_name: "CourierPrime-Italic",
    bold: false,
    italic: true,
    data: include_bytes!("../fonts/CourierPrime-Italic.ttf"),
};

pub static BOLD_ITALIC: Face = Face {
    postscript_name: "CourierPrime-BoldItalic",
    bold: true,
    italic: true,
    data: include_bytes!("../fonts/CourierPrime-BoldItalic.ttf"),
};

/// All four faces, in a fixed order: regular, bold, italic, bold italic.
pub static FACES: [&Face; 4] = [&REGULAR, &BOLD, &ITALIC, &BOLD_ITALIC];

/// The face for a style.
pub fn face(bold: bool, italic: bool) -> &'static Face {
    match (bold, italic) {
        (false, false) => &REGULAR,
        (true, false) => &BOLD,
        (false, true) => &ITALIC,
        (true, true) => &BOLD_ITALIC,
    }
}

/// The full text of the SIL Open Font License 1.1 with the Courier Prime
/// copyright notice, exactly as distributed with the fonts.
pub const LICENSE: &str = include_str!("../fonts/OFL.txt");

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// A font damaged by a line-ending conversion, a Git LFS pointer or a
    /// symlink checkout fails here instead of as a puzzling golden diff.
    #[test]
    fn bundled_bytes_are_the_recorded_files() {
        let expected: [(u64, u64); 4] = [
            (71_188, 0x3fc2_0dfc_a605_7892),
            (72_856, 0x0816_a527_426e_40d9),
            (79_980, 0x0cb4_f44d_620d_44ed),
            (80_872, 0x51db_d631_927d_c7e2),
        ];
        for (face, (len, hash)) in FACES.iter().zip(expected) {
            assert_eq!(face.data.len() as u64, len, "{}", face.postscript_name);
            assert_eq!(fnv1a64(face.data), hash, "{}", face.postscript_name);
            assert_eq!(
                &face.data[..4],
                b"\0\x01\0\0",
                "{} is not TrueType",
                face.postscript_name
            );
        }
    }

    #[test]
    fn faces_by_style() {
        assert_eq!(face(false, false).postscript_name, "CourierPrime-Regular");
        assert_eq!(face(true, true).postscript_name, "CourierPrime-BoldItalic");
        for f in FACES {
            assert!(std::ptr::eq(face(f.bold, f.italic), f));
        }
    }

    #[test]
    fn license_text_is_the_ofl() {
        assert!(LICENSE.starts_with("Copyright 2015 The Courier Prime Project Authors"));
        assert!(LICENSE.contains("SIL OPEN FONT LICENSE Version 1.1"));
    }
}
