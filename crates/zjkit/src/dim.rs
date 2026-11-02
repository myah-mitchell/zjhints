//! Fading Zellij palette colors, for a bar whose session is not where the
//! keyboard goes. The blend itself is `crate::format::dim_rgb`, which also
//! fades colors a format string names, so both dim by the same amount.

use crate::format;
use zellij_tile::prelude::{PaletteColor, StyleDeclaration};

/// Fades a `PaletteColor` toward a dark, desaturated gray by `strength`
/// (`0.0` is unchanged, `1.0` fully dimmed), mirroring the zjstatus fork's
/// `dim_color`. `EightBit` entries pass through unchanged: they're indices
/// into a terminal-defined palette, not RGB triples, so there's no
/// well-defined dimmed version of one to compute without also assuming a
/// specific palette.
pub fn color(color: PaletteColor, strength: f32) -> PaletteColor {
    match color {
        PaletteColor::Rgb((r, g, b)) => {
            let (r, g, b) = format::dim_rgb(r, g, b, strength);
            PaletteColor::Rgb((r, g, b))
        }
        other => other,
    }
}

/// Every color of one `Styling` group, faded by `strength`.
pub fn style_declaration(decl: &StyleDeclaration, strength: f32) -> StyleDeclaration {
    StyleDeclaration {
        base: color(decl.base, strength),
        background: color(decl.background, strength),
        emphasis_0: color(decl.emphasis_0, strength),
        emphasis_1: color(decl.emphasis_1, strength),
        emphasis_2: color(decl.emphasis_2, strength),
        emphasis_3: color(decl.emphasis_3, strength),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_blends_rgb_toward_gray() {
        // Pure red's BT.601 luminance is 0.299*255 = 76.245; the dim target
        // is 30% of that (~22.9), so full strength converges there on all
        // three channels: darker than the color's own brightness, not just
        // desaturated to it.
        let red = PaletteColor::Rgb((255, 0, 0));
        assert_eq!(color(red, 0.0), red);
        assert_eq!(color(red, 1.0), PaletteColor::Rgb((23, 23, 23)));
        assert_eq!(color(red, 0.5), PaletteColor::Rgb((139, 11, 11)));

        // A color that's already gray has nothing to desaturate, but still
        // darkens: its luminance equals every channel already, but the dim
        // target is a fraction of that.
        let white = PaletteColor::Rgb((255, 255, 255));
        assert_eq!(color(white, 1.0), PaletteColor::Rgb((77, 77, 77)));
    }

    #[test]
    fn color_passes_through_indexed_colors() {
        let indexed = PaletteColor::EightBit(200);
        assert_eq!(color(indexed, 0.8), indexed);
    }
}
