//! The dial's colours: the active theme's tokens and a few mixes of them.
//!
//! Every colour is a role of the theme, never a fixed hex value. The wash is the theme's canvas
//! deepened away from its ink, like a terminal's background: near black on a dark theme and near
//! white on a light one, so names keep their contrast while more of the desktop shows through.

use gtk::gdk::RGBA;

/// How much of the canvas stays in the wash; the rest is black on a dark theme, white on a
/// light one.
const WASH_CANVAS: f32 = 0.4;

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
    /// `#rrggbb`, for Pango markup.
    pub fn hex(self) -> String {
        let byte = |c: f32| (c.clamp(0., 1.) * 255.).round() as u8;
        format!(
            "#{:02x}{:02x}{:02x}",
            byte(self.0),
            byte(self.1),
            byte(self.2)
        )
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
    /// The canvas is lighter than the ink.
    pub light_theme: bool,
    /// The flat wash between the desktop and the dial: the theme's canvas, deepened.
    pub wash: Rgb,
    /// Lines at rest: the ring and the ticks.
    pub ink: Rgb,
    /// The light's colour, and its colour for a blocked or missing choice.
    pub lit: Rgb,
    pub warn_lit: Rgb,
    /// The theme's own accent, saturated, for the light on a light wash.
    pub accent: Rgb,
    pub warning: Rgb,
    pub working: Rgb,
}

impl Palette {
    pub fn new(t: Tokens) -> Self {
        let light_theme = t.canvas.luma() > t.ink.luma();
        let deep = if light_theme {
            Rgb(1., 1., 1.)
        } else {
            Rgb(0., 0., 0.)
        };
        Self {
            light_theme,
            wash: t.canvas.over(deep, WASH_CANVAS),
            ink: t.ink,
            lit: t.action.over(t.ink, 0.72),
            warn_lit: t.warning.over(t.ink, 0.85),
            accent: t.action,
            warning: t.warning,
            working: t.success,
        }
    }
    /// The light for the current choice, `warn` of the way from its own colour to amber.
    pub fn light(&self, warn: f64) -> Rgb {
        self.warn_lit.over(self.lit, warn.clamp(0., 1.) as f32)
    }
    /// The saturated accent, `warn` of the way to amber.
    pub fn accent(&self, warn: f64) -> Rgb {
        self.warning.over(self.accent, warn.clamp(0., 1.) as f32)
    }
    /// The trace's bright core: near white with a tint of the light on a dark wash, so it reads as
    /// light over a bright page too; the saturated accent on a light wash.
    pub fn core(&self, warn: f64) -> Rgb {
        if self.light_theme {
            self.accent(warn)
        } else {
            self.light(warn).over(Rgb(1., 1., 1.), 0.45)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_print_as_markup_hex() {
        assert_eq!(Tokens::NORD.warning.hex(), "#ebcb8b");
    }

    #[test]
    fn mixes_weight_the_first_colour() {
        let mixed = Rgb(1., 0., 0.).over(Rgb(0., 0., 1.), 0.25);
        assert_eq!(mixed, Rgb(0.25, 0., 0.75));
    }

    #[test]
    fn the_wash_deepens_the_canvas_away_from_the_ink() {
        let light = Palette::new(Tokens {
            canvas: Rgb(0.97, 0.98, 0.97),
            ink: Rgb(0.1, 0.2, 0.18),
            ..Tokens::NORD
        });
        assert!(light.light_theme && light.wash.luma() >= Tokens::NORD.canvas.luma());
        assert!(light.wash.luma() > light.ink.luma());
        let nord = Palette::new(Tokens::NORD);
        assert!(!nord.light_theme);
        // Nord's wash is its canvas at 40%: #12151a, a terminal's near black.
        assert!(nord.wash.luma() < 0.4 * Tokens::NORD.canvas.luma() + 1e-6);
        assert!(nord.wash.luma() < nord.ink.luma());
    }
}
