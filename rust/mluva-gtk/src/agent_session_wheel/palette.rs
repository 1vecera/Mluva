//! The watch's colours: the active theme's tokens plus derived mixes for the darker grounds.
//!
//! Every colour is a role of the theme, never a fixed hex value. A theme whose canvas is lighter
//! than its ink (the default light palette) swaps the two for the watch itself, so the steel is
//! always the light metal and the dial the dark ground, while the room around it and the names on
//! it keep following the theme's canvas and ink.

use gtk::gdk::RGBA;

/// A colour without alpha, mixed in sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub f32, pub f32, pub f32);

pub const BLACK: Rgb = Rgb(0., 0., 0.);

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
    pub fn solid(self) -> RGBA {
        self.alpha(1.)
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

/// A multi-stop sweep: angle in degrees from twelve o'clock and its colour.
pub type Sweep = Vec<(f32, RGBA)>;

pub struct Palette {
    pub light_theme: bool,
    /// The opaque ground, and the faint lift around the watch.
    pub room: Rgb,
    pub room_lift: Rgb,
    /// Steel highlights and the dark metal under the crystal.
    pub steel: Rgb,
    pub metal: Rgb,
    pub dark: Rgb,
    pub darker: Rgb,
    /// The light's colour, and its colour for a blocked or missing choice.
    pub lit: Rgb,
    pub warn_lit: Rgb,
    pub warning: Rgb,
    pub working: Rgb,
    /// Dial printing and the minute track.
    pub print: Rgb,
}

impl Palette {
    pub fn new(t: Tokens) -> Self {
        let light_theme = t.canvas.luma() > t.ink.luma();
        let (steel, metal) = if light_theme {
            (t.canvas, t.ink)
        } else {
            (t.ink, t.canvas)
        };
        let dark = metal.over(BLACK, 0.74);
        let darker = metal.over(BLACK, 0.54);
        let room_base = if light_theme {
            t.canvas.over(t.ink, 0.93)
        } else {
            darker
        };
        Self {
            light_theme,
            room: room_base,
            room_lift: room_base.over(t.action, 0.86),
            steel,
            metal,
            dark,
            darker,
            lit: t.action.over(steel, 0.7),
            warn_lit: t.warning.over(steel, 0.8),
            warning: t.warning,
            working: t.success.over(steel, 0.7),
            print: steel.over(metal, 0.7),
        }
    }

    /// `percent` of steel over the metal, as the prototype's `fg N% bg`.
    pub fn fg(&self, percent: f32) -> Rgb {
        self.steel.over(self.metal, percent / 100.)
    }
    pub fn fg_dark(&self, percent: f32) -> Rgb {
        self.steel.over(self.darker, percent / 100.)
    }

    /// Polished steel: a conic sweep, brightest at the upper left.
    pub fn case_sweep(&self) -> Sweep {
        let stops: [(f32, Rgb); 9] = [
            (0., self.fg(58.)),
            (50., self.fg(22.)),
            (120., self.fg_dark(10.)),
            (165., self.fg(34.)),
            (215., self.fg_dark(6.)),
            (305., self.fg(76.)),
            (318., self.fg(92.)),
            (335., self.fg(50.)),
            (360., self.fg(58.)),
        ];
        stops.iter().map(|(a, c)| (*a, c.solid())).collect()
    }
    /// The bezel band under the knurling, a little darker than the case.
    pub fn bezel_sweep(&self) -> Sweep {
        let over_dark = |p: f32| self.steel.over(self.dark, p / 100.);
        let over_darker = |p: f32| self.steel.over(self.darker, p / 100.);
        let stops: [(f32, Rgb); 5] = [
            (0., over_dark(30.)),
            (130., over_darker(12.)),
            (220., over_darker(8.)),
            (315., over_dark(44.)),
            (360., over_dark(30.)),
        ];
        stops.iter().map(|(a, c)| (*a, c.solid())).collect()
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
    fn a_light_theme_keeps_light_steel_and_a_dark_dial() {
        let light = Palette::new(Tokens {
            canvas: Rgb(0.97, 0.98, 0.97),
            ink: Rgb(0.1, 0.2, 0.18),
            ..Tokens::NORD
        });
        assert!(light.steel.luma() > light.metal.luma());
        assert!(light.dark.luma() < light.steel.luma());
        // The room follows the canvas, so a light desktop keeps a light room.
        assert!(light.room.luma() > 0.8);
        let nord = Palette::new(Tokens::NORD);
        assert!(nord.steel.luma() > nord.metal.luma());
        assert!(nord.room.luma() < 0.15);
    }
}
