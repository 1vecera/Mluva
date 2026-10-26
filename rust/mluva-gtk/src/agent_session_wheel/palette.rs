//! The dial's colours: the active theme's tokens and a few mixes of them.
//!
//! Every colour is a role of the theme, never a fixed hex value. The wash is the theme's canvas,
//! so a light theme gets a light wash with dark lines and a dark theme the reverse.

use gtk::gdk::RGBA;

/// A colour without alpha, mixed in sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub f32, pub f32, pub f32);

impl Rgb {
    pub fn of(color: RGBA) -> Self {
        Self(color.red(), color.green(), color.blue())
    }
    /// `weight` of `self` over the rest of `other`.
    pub fn over(self, other: Self, weight: f32) -> Self {
        let mix = |a: f32, b: f32| a * weight + b * (1. - weight);
        Self(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }
    pub fn alpha(self, alpha: f32) -> RGBA {
        RGBA::new(self.0, self.1, self.2, alpha)
    }
    /// Perceived brightness, enough to tell a light theme from a dark one.
    pub fn luma(self) -> f32 {
        0.2126 * self.0 + 0.7152 * self.1 + 0.0722 * self.2
    }
}

/// The theme roles the wheel reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub canvas: Rgb,
    pub ink: Rgb,
    pub action: Rgb,
    pub warning: Rgb,
    pub success: Rgb,
    pub ink_secondary: Rgb,
    pub ink_muted: Rgb,
}

impl Tokens {
    /// Nord, the reference theme, for a display whose stylesheet has not loaded.
    pub const NORD: Self = Self {
        canvas: Rgb(0.180, 0.204, 0.251),
        ink: Rgb(0.847, 0.871, 0.914),
        action: Rgb(0.506, 0.631, 0.757),
        warning: Rgb(0.922, 0.796, 0.545),
        success: Rgb(0.639, 0.745, 0.549),
        ink_secondary: Rgb(0.78, 0.81, 0.86),
        ink_muted: Rgb(0.68, 0.71, 0.77),
    };
}

pub struct Palette {
    /// The flat wash between the desktop and the dial: the theme's canvas.
    pub wash: Rgb,
    /// Lines at rest: the ring and the ticks.
    pub ink: Rgb,
    /// The light's colour, and its colour for a blocked or missing choice.
    pub lit: Rgb,
    pub warn_lit: Rgb,
    pub warning: Rgb,
    pub working: Rgb,
}

impl Palette {
    pub fn new(t: Tokens) -> Self {
        Self {
            wash: t.canvas,
            ink: t.ink,
            lit: t.action.over(t.ink, 0.72),
            warn_lit: t.warning.over(t.ink, 0.85),
            warning: t.warning,
            working: t.success,
        }
    }
    /// The light for the current choice.
    pub fn light(&self, warn: bool) -> Rgb {
        if warn { self.warn_lit } else { self.lit }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_weight_the_first_colour() {
        let mixed = Rgb(1., 0., 0.).over(Rgb(0., 0., 1.), 0.25);
        assert_eq!(mixed, Rgb(0.25, 0., 0.75));
    }

    #[test]
    fn the_wash_follows_the_canvas_and_the_lines_its_ink() {
        let light = Palette::new(Tokens {
            canvas: Rgb(0.97, 0.98, 0.97),
            ink: Rgb(0.1, 0.2, 0.18),
            ..Tokens::NORD
        });
        assert!(light.wash.luma() > light.ink.luma());
        let nord = Palette::new(Tokens::NORD);
        assert!(nord.wash.luma() < nord.ink.luma());
    }
}
