//! The bundled fonts: Courier Prime for the page, Atkinson Hyperlegible
//! Next for the interface (docs/specs/editor.md §3). The app uses no
//! system font of its own choosing, so it looks the same everywhere.

use iced::Font;
use iced::font::{Style, Weight};

/// Courier Prime, as the page prints it.
pub const PAGE: Font = Font::with_name("Courier Prime");

/// The interface face.
pub const UI: Font = Font::with_name(coldopen_fonts::ui::FAMILY);

pub const UI_BOLD: Font = Font {
    weight: Weight::Bold,
    ..UI
};

/// Courier Prime in a style.
pub fn page(bold: bool, italic: bool) -> Font {
    Font {
        weight: if bold { Weight::Bold } else { Weight::Normal },
        style: if italic { Style::Italic } else { Style::Normal },
        ..PAGE
    }
}

/// Every bundled font file, to load at start-up.
pub fn files() -> Vec<&'static [u8]> {
    let mut v: Vec<&'static [u8]> = coldopen_fonts::FACES.iter().map(|f| f.data).collect();
    v.push(coldopen_fonts::ui::REGULAR);
    v.push(coldopen_fonts::ui::BOLD);
    v
}
