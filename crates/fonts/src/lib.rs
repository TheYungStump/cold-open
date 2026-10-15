//! The fonts Cold Open bundles, under the SIL Open Font License 1.1:
//!
//! - Courier Prime Regular, Bold, Italic and Bold Italic, for the page
//!   ([`LICENSE`]);
//! - Atkinson Hyperlegible Next Regular and Bold, subset to Latin, for the
//!   desktop app's menus and panels ([`UI_LICENSE`]). Only the `ui` feature
//!   includes them, so the CLI and the layout library never carry them.
//!
//! Every copy of a program that contains these fonts must carry their
//! license text; `coldopen licenses` and the app's About screen print it.
//! Documents made with the fonts, such as exported PDFs, are not covered by
//! the license.
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

/// The SIL Open Font License 1.1 with the Atkinson Hyperlegible Next
/// copyright notice, exactly as distributed with that font.
pub const UI_LICENSE: &str = include_str!("../fonts/OFL-AtkinsonHyperlegibleNext.txt");

/// The desktop app's interface font, Atkinson Hyperlegible Next, subset to
/// Latin as `crates/fonts/README.md` records.
#[cfg(feature = "ui")]
pub mod ui {
    /// The family name the faces declare.
    pub const FAMILY: &str = "Atkinson Hyperlegible Next";
    pub static REGULAR: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegibleNext-Regular.ttf");
    pub static BOLD: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegibleNext-Bold.ttf");
}

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
        assert!(
            UI_LICENSE
                .starts_with("Copyright 2020-2024 The Atkinson Hyperlegible Next Project Authors")
        );
        assert!(UI_LICENSE.contains("SIL OPEN FONT LICENSE Version 1.1"));
        // No Reserved Font Name, so the subset may keep the font's name.
        assert!(!UI_LICENSE.contains("Reserved Font Name \""));
    }

    #[cfg(feature = "ui")]
    #[test]
    fn interface_faces_are_the_recorded_subsets() {
        for (data, len, hash) in [
            (ui::REGULAR, 36_612, 0xeede_f7a8_cb25_e666),
            (ui::BOLD, 37_128, 0x8c52_c588_de5d_6765),
        ] {
            assert_eq!(data.len() as u64, len);
            assert_eq!(fnv1a64(data), hash);
            assert_eq!(&data[..4], b"\0\x01\0\0");
        }
    }
}
