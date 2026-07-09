//! Design tokens. Both themes are deliberately selected palettes (not an
//! automatic flip). Category colors come from a CVD-validated 8-slot
//! categorical palette in fixed slot order — do not shuffle; `Other` and the
//! structural categories stay neutral. Identity is never color-alone: every
//! node also carries an icon glyph and text labels, and edge kinds are
//! dash-pattern-coded.

use crate::model::ResourceCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Light,
    Dark,
}

/// Plain sRGB color; converted to the GUI/SVG layer's own type at the edge,
/// so the core stays free of GUI dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// Blends `a` over `b` with weight `t` (0.0 = all `b`, 1.0 = all `a`).
/// Used for the subtle category tints on container fills.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let lerp = |x: u8, y: u8| (x as f32 * t + y as f32 * (1.0 - t)).round() as u8;
    Rgb(lerp(a.0, b.0), lerp(a.1, b.1), lerp(a.2, b.2))
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub kind: ThemeKind,
    /// Chart surface (containers, panels).
    pub surface: Rgb,
    /// Page plane behind everything.
    pub plane: Rgb,
    /// Resource card fill.
    pub card: Rgb,
    pub ink: Rgb,
    pub ink_2: Rgb,
    pub ink_3: Rgb,
    pub hairline: Rgb,
    pub edge: Rgb,
    pub accent: Rgb,
    pub warn: Rgb,
}

pub const LIGHT: Theme = Theme {
    kind: ThemeKind::Light,
    surface: Rgb(0xfc, 0xfc, 0xfb),
    plane: Rgb(0xf9, 0xf9, 0xf7),
    card: Rgb(0xff, 0xff, 0xff),
    ink: Rgb(0x0b, 0x0b, 0x0b),
    ink_2: Rgb(0x52, 0x51, 0x4e),
    ink_3: Rgb(0x89, 0x87, 0x81),
    hairline: Rgb(0xe1, 0xe0, 0xd9),
    edge: Rgb(0xa9, 0xa8, 0x9f),
    accent: Rgb(0x2a, 0x78, 0xd6),
    warn: Rgb(0xd0, 0x3b, 0x3b),
};

pub const DARK: Theme = Theme {
    kind: ThemeKind::Dark,
    surface: Rgb(0x1a, 0x1a, 0x19),
    plane: Rgb(0x0d, 0x0d, 0x0d),
    card: Rgb(0x23, 0x23, 0x22),
    ink: Rgb(0xff, 0xff, 0xff),
    ink_2: Rgb(0xc3, 0xc2, 0xb7),
    ink_3: Rgb(0x89, 0x87, 0x81),
    hairline: Rgb(0x2c, 0x2c, 0x2a),
    edge: Rgb(0x56, 0x55, 0x50),
    accent: Rgb(0x39, 0x87, 0xe5),
    warn: Rgb(0xe6, 0x67, 0x67),
};

impl Theme {
    pub fn category_color(&self, category: ResourceCategory) -> Rgb {
        use ResourceCategory::*;
        match self.kind {
            ThemeKind::Light => match category {
                Compute => Rgb(0x2a, 0x78, 0xd6),
                Network => Rgb(0x1b, 0xaf, 0x7a),
                Storage => Rgb(0xed, 0xa1, 0x00),
                Database => Rgb(0x00, 0x83, 0x00),
                Containers => Rgb(0x4a, 0x3a, 0xa7),
                Security => Rgb(0xe3, 0x49, 0x48),
                Integration => Rgb(0xe8, 0x7b, 0xa4),
                Web => Rgb(0xeb, 0x68, 0x34),
                Other => Rgb(0x89, 0x87, 0x81),
                Scope | Group => Rgb(0x52, 0x51, 0x4e),
            },
            ThemeKind::Dark => match category {
                Compute => Rgb(0x39, 0x87, 0xe5),
                Network => Rgb(0x19, 0x9e, 0x70),
                Storage => Rgb(0xc9, 0x85, 0x00),
                Database => Rgb(0x00, 0x83, 0x00),
                Containers => Rgb(0x90, 0x85, 0xe9),
                Security => Rgb(0xe6, 0x67, 0x67),
                Integration => Rgb(0xd5, 0x51, 0x81),
                Web => Rgb(0xd9, 0x59, 0x26),
                Other => Rgb(0x89, 0x87, 0x81),
                Scope | Group => Rgb(0xc3, 0xc2, 0xb7),
            },
        }
    }
}
