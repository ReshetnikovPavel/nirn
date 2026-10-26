//! A custom [Rosé Pine] theme, built from the official palette.
//!
//! [Rosé Pine]: https://rose-pine.dev

use iced::theme::Palette;
use iced::{Color, Theme};

/// The color roles of a Rosé Pine variant, as published by the
/// [Rosé Pine palette] project.
///
/// [Rosé Pine palette]: https://github.com/rose-pine/palette
#[derive(Debug, Clone, Copy)]
pub struct RosePine {
    /// The base background.
    pub base: Color,
    /// A raised background, used for surfaces.
    pub surface: Color,
    /// A background for overlays and highlights.
    pub overlay: Color,
    /// De-emphasized text.
    pub muted: Color,
    /// Secondary text.
    pub subtle: Color,
    /// The main text color.
    pub text: Color,
    /// The love accent: an eye-catching rose red.
    pub love: Color,
    /// The gold accent: a warm yellow.
    pub gold: Color,
    /// The rose accent: a soft pink.
    pub rose: Color,
    /// The pine accent: a deep teal.
    pub pine: Color,
    /// The foam accent: a pale aqua.
    pub foam: Color,
    /// The iris accent: a muted purple.
    pub iris: Color,
}

impl RosePine {
    /// The default dark variant.
    pub const MAIN: Self = Self {
        base: Color::from_rgb8(0x19, 0x17, 0x24),
        surface: Color::from_rgb8(0x1f, 0x1d, 0x2e),
        overlay: Color::from_rgb8(0x26, 0x23, 0x3a),
        muted: Color::from_rgb8(0x6e, 0x6a, 0x86),
        subtle: Color::from_rgb8(0x90, 0x8c, 0xaa),
        text: Color::from_rgb8(0xe0, 0xde, 0xf4),
        love: Color::from_rgb8(0xeb, 0x6f, 0x92),
        gold: Color::from_rgb8(0xf6, 0xc1, 0x77),
        rose: Color::from_rgb8(0xeb, 0xbc, 0xba),
        pine: Color::from_rgb8(0x31, 0x74, 0x8f),
        foam: Color::from_rgb8(0x9c, 0xcf, 0xd8),
        iris: Color::from_rgb8(0xc4, 0xa7, 0xe7),
    };

    /// A darker variant of [`Self::MAIN`].
    pub const MOON: Self = Self {
        base: Color::from_rgb8(0x23, 0x21, 0x36),
        surface: Color::from_rgb8(0x2a, 0x27, 0x3f),
        overlay: Color::from_rgb8(0x39, 0x35, 0x52),
        muted: Color::from_rgb8(0x6e, 0x6a, 0x86),
        subtle: Color::from_rgb8(0x90, 0x8c, 0xaa),
        text: Color::from_rgb8(0xe0, 0xde, 0xf4),
        love: Color::from_rgb8(0xeb, 0x6f, 0x92),
        gold: Color::from_rgb8(0xf6, 0xc1, 0x77),
        rose: Color::from_rgb8(0xea, 0x9a, 0x97),
        pine: Color::from_rgb8(0x3e, 0x8f, 0xb0),
        foam: Color::from_rgb8(0x9c, 0xcf, 0xd8),
        iris: Color::from_rgb8(0xc4, 0xa7, 0xe7),
    };

    /// The light variant.
    pub const DAWN: Self = Self {
        base: Color::from_rgb8(0xfa, 0xf4, 0xed),
        surface: Color::from_rgb8(0xff, 0xfa, 0xf3),
        overlay: Color::from_rgb8(0xf2, 0xe9, 0xe1),
        muted: Color::from_rgb8(0x98, 0x93, 0xa5),
        subtle: Color::from_rgb8(0x79, 0x75, 0x93),
        text: Color::from_rgb8(0x46, 0x42, 0x61),
        love: Color::from_rgb8(0xb4, 0x63, 0x7a),
        gold: Color::from_rgb8(0xea, 0x9d, 0x34),
        rose: Color::from_rgb8(0xd7, 0x82, 0x7e),
        pine: Color::from_rgb8(0x28, 0x69, 0x83),
        foam: Color::from_rgb8(0x56, 0x94, 0x9f),
        iris: Color::from_rgb8(0x90, 0x7a, 0xa9),
    };

    /// Maps the variant onto the roles of an iced [`Palette`].
    ///
    /// Rosé Pine has no green of its own, so `foam` doubles as the
    /// success color and `iris` as the primary one.
    fn palette(self) -> Palette {
        Palette {
            background: self.base,
            text: self.text,
            primary: self.iris,
            success: self.foam,
            warning: self.gold,
            danger: self.love,
        }
    }
}

/// The theme used by the bar: [Rosé Pine].
///
/// [Rosé Pine]: https://rose-pine.dev
pub fn rose_pine() -> Theme {
    Theme::custom("Rosé Pine", RosePine::MAIN.palette())
}

/// The dimmer [Rosé Pine Moon] variant.
///
/// [Rosé Pine Moon]: https://rose-pine.dev
pub fn rose_pine_moon() -> Theme {
    Theme::custom("Rosé Pine Moon", RosePine::MOON.palette())
}

/// The light [Rosé Pine Dawn] variant.
///
/// [Rosé Pine Dawn]: https://rose-pine.dev
pub fn rose_pine_dawn() -> Theme {
    Theme::custom("Rosé Pine Dawn", RosePine::DAWN.palette())
}
