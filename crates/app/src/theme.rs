//! Light, dark and high-contrast themes (docs/specs/editor.md §3). Colors
//! are our own; the page never uses them when printing.

use iced::{Color, Theme};
use serde::{Deserialize, Serialize};

/// The theme the writer chose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// Light or dark, following the operating system.
    #[default]
    System,
    Light,
    Dark,
    HighContrast,
}

/// Every color the app draws with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub high_contrast: bool,
    /// Which ink a revision color takes in this theme: 0 light, 1 dark, 2
    /// high contrast (docs/specs/revisions.md §2.3).
    pub shade: usize,
    /// Behind the pages.
    pub desk: Color,
    pub page: Color,
    pub page_edge: Color,
    /// Script text on the page.
    pub ink: Color,
    /// Page numbers and other quiet text on the page.
    pub ink_quiet: Color,
    pub caret: Color,
    pub selection: Color,
    pub find: Color,
    /// Margin bars on blocks an agent changed (editor.md §12).
    pub agent: Color,
    /// Tracked changes' authors, in slot order (editor.md §17.1).
    pub changes: [Color; 6],
    /// Markers for notes and other text that does not print.
    pub hidden: Color,
    /// Panels: navigator, hint bar, status bar.
    pub panel: Color,
    pub panel_alt: Color,
    pub text: Color,
    pub text_quiet: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub accent_text: Color,
    pub border: Color,
    pub focus: Color,
    pub danger: Color,
    pub success: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const fn rgba(hex: u32, a: f32) -> Color {
    Color::from_rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, a)
}

impl Palette {
    pub const LIGHT: Palette = Palette {
        high_contrast: false,
        shade: 0,
        desk: rgb(0xECEAE4),
        page: rgb(0xFFFFFF),
        page_edge: rgb(0xD3D1C7),
        ink: rgb(0x1A1A1A),
        ink_quiet: rgb(0x5F5E5A),
        caret: rgb(0x185FA5),
        selection: rgba(0x85B7EB, 0.45),
        find: rgba(0xEF9F27, 0.40),
        agent: rgb(0x7F77DD),
        changes: [
            rgb(0x0072B2),
            rgb(0xD55E00),
            rgb(0x009E73),
            rgb(0xCC79A7),
            rgb(0x3C3C99),
            rgb(0x8F5A00),
        ],
        hidden: rgb(0xB4B2A9),
        panel: rgb(0xF7F6F3),
        panel_alt: rgb(0xFFFFFF),
        text: rgb(0x2C2C2A),
        text_quiet: rgb(0x5F5E5A),
        accent: rgb(0x185FA5),
        accent_soft: rgb(0xE6F1FB),
        accent_text: rgb(0x0C447C),
        border: rgb(0xD3D1C7),
        focus: rgb(0x185FA5),
        danger: rgb(0xA32D2D),
        success: rgb(0x3B6D11),
    };

    pub const DARK: Palette = Palette {
        high_contrast: false,
        shade: 1,
        desk: rgb(0x1C1C1E),
        page: rgb(0x2A2A2D),
        page_edge: rgb(0x444441),
        ink: rgb(0xDEDDD8),
        ink_quiet: rgb(0x9A9A9E),
        caret: rgb(0x85B7EB),
        selection: rgba(0x185FA5, 0.55),
        find: rgba(0xBA7517, 0.55),
        agent: rgb(0xAFA9EC),
        changes: [
            rgb(0x56B4E9),
            rgb(0xE69F00),
            rgb(0x3DD6A4),
            rgb(0xE8A0C8),
            rgb(0xA8A8FF),
            rgb(0xD9B26A),
        ],
        hidden: rgb(0x5F5E5A),
        panel: rgb(0x232326),
        panel_alt: rgb(0x2A2A2D),
        text: rgb(0xE6E6E2),
        text_quiet: rgb(0xA3A29C),
        accent: rgb(0x85B7EB),
        accent_soft: rgb(0x0C447C),
        accent_text: rgb(0xE6F1FB),
        border: rgb(0x3A3A3E),
        focus: rgb(0x85B7EB),
        danger: rgb(0xF09595),
        success: rgb(0x97C459),
    };

    pub const HIGH_CONTRAST: Palette = Palette {
        high_contrast: true,
        shade: 2,
        desk: rgb(0x000000),
        page: rgb(0x000000),
        page_edge: rgb(0xFFFFFF),
        ink: rgb(0xFFFFFF),
        ink_quiet: rgb(0xFFFFFF),
        caret: rgb(0xFFD400),
        selection: rgb(0x1A4DFF),
        find: rgb(0x7A5C00),
        agent: rgb(0xC9A6FF),
        changes: [
            rgb(0x00E5FF),
            rgb(0xFF9E40),
            rgb(0x7CFF7C),
            rgb(0xFF8CFF),
            rgb(0xB8B8FF),
            rgb(0xFFD9A0),
        ],
        hidden: rgb(0xFFFFFF),
        panel: rgb(0x000000),
        panel_alt: rgb(0x000000),
        text: rgb(0xFFFFFF),
        text_quiet: rgb(0xFFFFFF),
        accent: rgb(0xFFD400),
        accent_soft: rgb(0x1A4DFF),
        accent_text: rgb(0xFFFFFF),
        border: rgb(0xFFFFFF),
        focus: rgb(0xFFD400),
        danger: rgb(0xFF8080),
        success: rgb(0x9CFF9C),
    };

    /// The palette for a choice; `System` follows `dark`.
    pub fn of(choice: Choice, system_dark: bool) -> Palette {
        match choice {
            Choice::Light => Palette::LIGHT,
            Choice::Dark => Palette::DARK,
            Choice::HighContrast => Palette::HIGH_CONTRAST,
            Choice::System if system_dark => Palette::DARK,
            Choice::System => Palette::LIGHT,
        }
    }

    /// An iced theme for the built-in widgets, from this palette.
    pub fn theme(&self) -> Theme {
        let name = if self.high_contrast {
            "Cold Open High Contrast"
        } else if self.page == Palette::DARK.page {
            "Cold Open Dark"
        } else {
            "Cold Open Light"
        };
        Theme::custom(
            name,
            iced::theme::Palette {
                background: self.panel,
                text: self.text,
                primary: self.accent,
                success: self.success,
                warning: rgb(0xBA7517),
                danger: self.danger,
            },
        )
    }
}
