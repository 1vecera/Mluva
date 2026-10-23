//! Painting the watch with GtkSnapshot: a steel case, a sunray dial, applied indices, a faceted
//! hand and one moving light.
//!
//! Everything that never moves while the choice swings (case, dial texture, minute track,
//! indices, cap, crystal) is recorded once into a layer, baked into a texture when a renderer
//! is at hand, and merely appended each frame. Only the sunburst bar, the light, the bloom and
//! the hand are drawn per frame.

use super::palette::{BLACK, Palette};
use gtk::gdk::RGBA;
use gtk::prelude::*;
use gtk::{gdk, graphene, gsk};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Ready,
    Working,
    Blocked,
    /// Present, but nothing can be sent: recording keeps an editable draft.
    Draft,
    Gone,
}

/// What the watch shows: its size, one index per session and the choice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Face {
    pub dial: f64,
    pub marks: Vec<(f64, Kind)>,
    pub chosen: Option<usize>,
    /// The choice cannot be recorded, so the light turns to the warning colour.
    pub warn: bool,
}

impl Face {
    pub fn rim(&self) -> f64 {
        self.dial + 6.
    }
    pub fn case(&self) -> f64 {
        self.dial + CASE_BAND
    }
    pub fn inner(&self) -> f64 {
        self.dial - 52.
    }
    pub fn outer(&self) -> f64 {
        self.dial - 16.
    }
    fn width(&self) -> f64 {
        if self.marks.len() > 14 { 7. } else { 8. }
    }
    fn hand_length(&self) -> f64 {
        self.inner() - 10.
    }
    /// Where the light lands on the dial at `degrees`.
    fn landing(&self, degrees: f64) -> (f64, f64) {
        point(degrees, (self.inner() + self.outer()) / 2.)
    }
}

/// The steel between the dial and the room.
pub const CASE_BAND: f64 = 30.;

pub fn point(degrees: f64, radius: f64) -> (f64, f64) {
    let angle = degrees.to_radians();
    (radius * angle.sin(), -radius * angle.cos())
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> graphene::Rect {
    graphene::Rect::new(x as f32, y as f32, w as f32, h as f32)
}
fn at(x: f64, y: f64) -> graphene::Point {
    graphene::Point::new(x as f32, y as f32)
}
fn disc(radius: f64) -> gsk::RoundedRect {
    gsk::RoundedRect::from_rect(
        rect(-radius, -radius, 2. * radius, 2. * radius),
        radius as f32,
    )
}
fn square(radius: f64) -> graphene::Rect {
    rect(-radius, -radius, 2. * radius, 2. * radius)
}
fn stop(offset: f32, color: RGBA) -> gsk::ColorStop {
    gsk::ColorStop::new(offset, color)
}
fn clear() -> RGBA {
    RGBA::new(0., 0., 0., 0.)
}
fn sweep(stops: &[(f32, RGBA)]) -> Vec<gsk::ColorStop> {
    stops.iter().map(|(a, c)| stop(a / 360., *c)).collect()
}

/// A closed polygon in the local frame.
fn polygon(points: &[(f64, f64)]) -> gsk::Path {
    let builder = gsk::PathBuilder::new();
    for (index, (x, y)) in points.iter().enumerate() {
        if index == 0 {
            builder.move_to(*x as f32, *y as f32);
        } else {
            builder.line_to(*x as f32, *y as f32);
        }
    }
    builder.close();
    builder.to_path()
}

fn fill(snapshot: &gtk::Snapshot, path: &gsk::Path, color: &RGBA) {
    snapshot.append_fill(path, gsk::FillRule::Winding, color);
}

fn clipped_to(snapshot: &gtk::Snapshot, radius: f64, paint: impl FnOnce(&gtk::Snapshot)) {
    snapshot.push_rounded_clip(&disc(radius));
    paint(snapshot);
    snapshot.pop();
}

/// Thin wedges around the centre: the radial brushing of a sunray dial, or the knurled bezel.
fn wedges(snapshot: &gtk::Snapshot, radius: f64, period: f64, width: f64, color: &RGBA) {
    let count = (360. / period).round() as usize;
    for step in 0..count {
        let from = (step as f64 * period).to_radians();
        let to = from + width.to_radians();
        let path = polygon(&[
            (0., 0.),
            (radius * from.sin(), -radius * from.cos()),
            (radius * to.sin(), -radius * to.cos()),
        ]);
        fill(snapshot, &path, color);
    }
}

/// Everything under the sunburst bar: shadow, steel case, knurled bezel, gasket, dial base.
pub fn under(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    let case = face.case();
    let black = |alpha| BLACK.alpha(alpha);
    snapshot.append_outset_shadow(&disc(case), &black(0.72), 18., 34., 0., 70.);
    snapshot.append_outset_shadow(&disc(case), &black(0.55), 4., 8., 0., 16.);
    clipped_to(snapshot, case, |s| {
        s.append_conic_gradient(&square(case), &at(0., 0.), 0., &sweep(&p.case_sweep()));
    });
    // A fine knurled band on the case top.
    let bezel = case - 5.;
    clipped_to(snapshot, bezel, |s| {
        s.append_conic_gradient(&square(bezel), &at(0., 0.), 0., &sweep(&p.bezel_sweep()));
        wedges(s, bezel, 1.4, 0.7, &p.steel.alpha(0.09));
    });
    snapshot.append_inset_shadow(&disc(bezel), &p.steel.alpha(0.4), 0., 1., 0., 1.);
    // The dark gasket under the crystal.
    let rim = face.rim();
    clipped_to(snapshot, rim, |s| {
        s.append_color(&p.darker.solid(), &square(rim))
    });
    snapshot.append_inset_shadow(&disc(rim), &black(0.8), 2., 3., 0., 6.);
    // The dial: a radial base, lighter in the middle.
    let dial = face.dial;
    clipped_to(snapshot, dial, |s| {
        s.append_radial_gradient(
            &square(dial),
            &at(0., 0.),
            dial as f32,
            dial as f32,
            0.,
            1.,
            &[
                stop(0., p.metal.over(p.steel, 0.92).solid()),
                stop(0.72, p.dark.solid()),
                stop(1., p.darker.solid()),
            ],
        );
    });
    snapshot.append_inset_shadow(&disc(dial), &black(0.4), 0., 0., 1., 0.);
    snapshot.append_inset_shadow(&disc(dial), &black(0.45), 0., 10., 0., 30.);
}

/// The sunburst bar: radial brushing catches the light as a bright bar through the centre that
/// turns to face the light, with a weaker one opposite.
pub fn sunburst(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, sun: f64, glow: f64) {
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let glow = glow as f32;
    let edge = p.dark.over(p.steel, 0.82).solid();
    let rim = lit.over(p.dark, 0.22 * glow).solid();
    let core = lit.over(p.metal, 0.34 * glow).solid();
    let start = (sun - 180.).rem_euclid(360.) as f32;
    clipped_to(snapshot, face.dial, |s| {
        s.append_conic_gradient(
            &square(face.dial),
            &at(0., 0.),
            start,
            &sweep(&[
                (0., edge),
                (34., clear()),
                (146., clear()),
                (172., rim),
                (180., core),
                (188., rim),
                (214., clear()),
                (326., clear()),
                (360., edge),
            ]),
        );
    });
}

/// Brushing that fades toward the centre, then the matte chapter ring at the dial's edge.
pub fn over(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    let dial = face.dial;
    clipped_to(snapshot, dial, |s| {
        s.push_mask(gsk::MaskMode::Alpha);
        let reach = (dial * std::f64::consts::SQRT_2) as f32;
        s.append_radial_gradient(
            &square(dial),
            &at(0., 0.),
            reach,
            reach,
            0.,
            1.,
            &[
                stop(0., clear()),
                stop(0.18, clear()),
                stop(0.52, RGBA::new(0., 0., 0., 1.)),
                stop(1., RGBA::new(0., 0., 0., 1.)),
            ],
        );
        s.pop();
        wedges(s, dial, 0.9, 0.45, &p.steel.alpha(0.05));
        s.pop();
    });
    // The chapter ring: a matte step a few tenths of a millimetre below the dial.
    let step = dial - 25.;
    clipped_to(snapshot, dial, |s| {
        s.push_mask(gsk::MaskMode::InvertedAlpha);
        clipped_to(s, step, |s| {
            s.append_color(&RGBA::new(0., 0., 0., 1.), &square(step));
        });
        s.pop();
        s.append_color(&p.darker.alpha(0.4), &square(dial));
        s.pop();
    });
    snapshot.append_outset_shadow(&disc(step), &p.steel.alpha(0.12), 0., -1., 0., 0.);
    snapshot.append_outset_shadow(&disc(step), &BLACK.alpha(0.5), 0., 1., 0., 0.);
    snapshot.append_inset_shadow(&disc(dial), &BLACK.alpha(0.3), 0., 0., 1., 0.);
}

/// The light: a soft wedge from the centre toward the choice and a pool where it lands.
pub fn light(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, sun: f64, glow: f64) {
    if glow <= 0.01 {
        return;
    }
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let glow = glow as f32;
    let dial = face.dial;
    clipped_to(snapshot, dial, |s| {
        s.push_opacity(f64::from(glow));
        s.push_mask(gsk::MaskMode::Alpha);
        let reach = (dial * std::f64::consts::SQRT_2) as f32;
        s.append_radial_gradient(
            &square(dial),
            &at(0., 0.),
            reach,
            reach,
            0.,
            1.,
            &[
                stop(0., clear()),
                stop(0.14, clear()),
                stop(0.46, RGBA::new(0., 0., 0., 1.)),
                stop(0.84, RGBA::new(0., 0., 0., 1.)),
                stop(1., clear()),
            ],
        );
        s.pop();
        let tint = |alpha: f32| lit.alpha(alpha);
        s.append_conic_gradient(
            &square(dial),
            &at(0., 0.),
            (sun - 16.).rem_euclid(360.) as f32,
            &sweep(&[
                (0., clear()),
                (10., tint(0.16)),
                (16., tint(0.34)),
                (22., tint(0.16)),
                (32., clear()),
                (360., clear()),
            ]),
        );
        s.pop();
        let (x, y) = face.landing(sun);
        s.append_radial_gradient(
            &rect(x - 75., y - 75., 150., 150.),
            &at(x, y),
            75.,
            75.,
            0.,
            1.,
            &[
                stop(0., tint(0.5)),
                stop(0.34, tint(0.18)),
                stop(0.68, clear()),
                stop(1., clear()),
            ],
        );
        s.pop();
    });
}

/// A light source blooming: a wide halo, a near one and the source itself.
pub fn bloom(snapshot: &gtk::Snapshot, strength: f64, paint: impl Fn(&gtk::Snapshot)) {
    if strength <= 0.01 {
        return;
    }
    snapshot.push_opacity(strength.min(1.));
    for radius in [20., 20., 6.] {
        snapshot.push_blur(radius);
        paint(snapshot);
        snapshot.pop();
    }
    paint(snapshot);
    snapshot.pop();
}

fn rounded(x: f64, y: f64, w: f64, h: f64, radius: f64) -> gsk::RoundedRect {
    gsk::RoundedRect::from_rect(rect(x, y, w, h), radius as f32)
}

/// One applied index, in its own frame with twelve o'clock up and the lit edge to the left.
fn applied_index(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, kind: Kind) {
    let (inner, outer, width) = (face.inner(), face.outer(), face.width());
    let length = outer - inner;
    let body = rounded(-width / 2., -outer, width, length, (width / 2.).min(1.6));
    let stops = [
        stop(0., p.fg(96.).solid()),
        stop(0.48, p.fg(74.).solid()),
        stop(0.52, p.fg_dark(46.).solid()),
        stop(1., p.fg_dark(30.).solid()),
    ];
    let gradient = |s: &gtk::Snapshot| {
        s.append_linear_gradient(
            &rect(-width / 2., -outer, width, length),
            &at(-width / 2., 0.),
            &at(width / 2., 0.),
            &stops,
        );
    };
    match kind {
        Kind::Gone => {
            // An empty setting: two tiny feet where the index was.
            for y in [-outer + 5., -inner - 5.] {
                let path = gsk::PathBuilder::new();
                path.add_circle(&at(0., y), 1.6);
                fill(snapshot, &path.to_path(), &p.darker.solid());
            }
        }
        Kind::Draft => {
            // Hollow: the facets drawn as an outline only.
            snapshot.push_rounded_clip(&body);
            snapshot.push_mask(gsk::MaskMode::InvertedAlpha);
            snapshot.push_rounded_clip(&rounded(
                -width / 2. + 1.6,
                -outer + 1.6,
                width - 3.2,
                length - 3.2,
                0.6,
            ));
            snapshot.append_color(
                &RGBA::new(0., 0., 0., 1.),
                &rect(-width, -outer, 2. * width, length),
            );
            snapshot.pop();
            snapshot.pop();
            gradient(snapshot);
            snapshot.pop();
            snapshot.pop();
        }
        Kind::Ready | Kind::Working | Kind::Blocked => {
            snapshot.push_rounded_clip(&body);
            gradient(snapshot);
            snapshot.pop();
            // A lume channel down the middle of every index.
            let channel = if kind == Kind::Blocked {
                p.warning.solid()
            } else {
                p.fg(82.).solid()
            };
            snapshot.push_rounded_clip(&rounded(-1.1, -outer + 5., 2.2, length - 10., 1.1));
            snapshot.append_color(&channel, &rect(-2., -outer + 5., 4., length - 10.));
            snapshot.pop();
            if kind == Kind::Working {
                let dot = gsk::PathBuilder::new();
                dot.add_circle(&at(0., -inner + 9.), 3.);
                fill(snapshot, &dot.to_path(), &p.working.solid());
            }
        }
    }
}

/// The minute track and the applied indices, each with a drop shadow to the lower right.
pub fn marks(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    let dial = face.dial;
    snapshot.push_opacity(0.5);
    for minute in 0..60 {
        let five = minute % 5 == 0;
        let (from, to) = (dial - 9., dial - if five { 2. } else { 4. });
        snapshot.save();
        snapshot.rotate((f64::from(minute) * 6.) as f32);
        let alpha = if five { 0.8 } else { 0.55 };
        let width = if five { 1.2 } else { 0.8 };
        snapshot.append_color(
            &p.print.alpha(alpha),
            &rect(-width / 2., -to, width, to - from),
        );
        snapshot.restore();
    }
    snapshot.append_border(
        &disc(dial - 9.),
        &[0.8; 4],
        &[
            p.print.alpha(0.28),
            p.print.alpha(0.28),
            p.print.alpha(0.28),
            p.print.alpha(0.28),
        ],
    );
    snapshot.pop();
    snapshot.push_shadow(&[gsk::Shadow::new(BLACK.alpha(0.7), 1.2, 2.2, 1.2)]);
    for (angle, kind) in &face.marks {
        snapshot.save();
        snapshot.rotate(*angle as f32);
        applied_index(snapshot, p, face, *kind);
        snapshot.restore();
    }
    snapshot.pop();
}

/// The chosen index lit by the light: a filled body and a bright lume core.
pub fn lit_index(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64, kind: Kind) {
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let (inner, outer, width) = (face.inner(), face.outer(), face.width());
    snapshot.save();
    snapshot.rotate(angle as f32);
    if kind == Kind::Gone {
        let ring = gsk::PathBuilder::new();
        ring.add_circle(&at(0., -(inner + outer) / 2.), 4.);
        snapshot.append_stroke(&ring.to_path(), &gsk::Stroke::new(1.5), &lit.solid());
    } else {
        snapshot.push_rounded_clip(&rounded(-width / 2., -outer, width, outer - inner, 1.6));
        snapshot.append_color(
            &lit.alpha(0.55),
            &rect(-width / 2., -outer, width, outer - inner),
        );
        snapshot.pop();
        snapshot.push_rounded_clip(&rounded(-1.7, -outer + 4., 3.4, outer - inner - 8., 1.7));
        snapshot.append_color(
            &lit.solid(),
            &rect(-2., -outer + 4., 4., outer - inner - 8.),
        );
        snapshot.pop();
    }
    snapshot.restore();
}

/// The faceted dauphine hand: a lit half and a shaded half, with its shadow on the dial.
pub fn hand(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64) {
    let (length, half) = (face.hand_length(), 10.);
    snapshot.push_shadow(&[gsk::Shadow::new(BLACK.alpha(0.55), 4., 7., 3.5)]);
    snapshot.save();
    snapshot.rotate(angle as f32);
    fill(
        snapshot,
        &polygon(&[(0., -length), (-half, 0.), (0., 14.)]),
        &p.fg(94.).solid(),
    );
    fill(
        snapshot,
        &polygon(&[(0., -length), (half, 0.), (0., 14.)]),
        &p.fg_dark(48.).solid(),
    );
    snapshot.restore();
    snapshot.pop();
}

/// The lume channel down the hand, glowing.
pub fn hand_lume(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64) {
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let length = face.hand_length();
    bloom(snapshot, 0.9, |s| {
        s.save();
        s.rotate(angle as f32);
        fill(
            s,
            &polygon(&[
                (0., -length + 22.),
                (-2.4, -length * 0.32),
                (0., -length * 0.28),
                (2.4, -length * 0.32),
            ]),
            &lit.solid(),
        );
        s.restore();
    });
}

/// A polished, domed pin where the hand turns.
pub fn cap(snapshot: &gtk::Snapshot, p: &Palette) {
    snapshot.push_shadow(&[gsk::Shadow::new(BLACK.alpha(0.7), 1.2, 2.2, 1.2)]);
    snapshot.push_rounded_clip(&disc(11.));
    snapshot.append_radial_gradient(
        &square(11.),
        &at(-11. * 0.24, -11. * 0.32),
        11. * 1.4,
        11. * 1.4,
        0.,
        1.,
        &[
            stop(0., p.steel.solid()),
            stop(0.55, p.fg(55.).solid()),
            stop(1., p.fg_dark(22.).solid()),
        ],
    );
    snapshot.pop();
    snapshot.pop();
    let pin = gsk::PathBuilder::new();
    pin.add_circle(&at(0., 0.), 2.6);
    fill(snapshot, &pin.to_path(), &p.darker.solid());
}

/// The domed crystal: a reflection across the upper left and a thin bright edge.
pub fn crystal(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    let rim = face.rim();
    let size = 2. * rim;
    clipped_to(snapshot, rim, |s| {
        s.append_radial_gradient(
            &square(rim),
            &at(-rim + size * 0.34, -rim + size * 0.22),
            (size * 0.72) as f32,
            (size * 0.46) as f32,
            0.,
            1.,
            &[
                stop(0., p.steel.alpha(0.09)),
                stop(0.7, clear()),
                stop(1., clear()),
            ],
        );
        s.append_radial_gradient(
            &square(rim),
            &at(-rim + size * 0.7, -rim + size * 0.85),
            (size * 1.2) as f32,
            (size * 1.2) as f32,
            0.,
            1.,
            &[
                stop(0., clear()),
                stop(0.62, clear()),
                stop(0.7, p.steel.alpha(0.05)),
                stop(0.74, clear()),
                stop(1., clear()),
            ],
        );
    });
    snapshot.append_inset_shadow(&disc(rim), &p.steel.alpha(0.1), 0., 0., 1., 0.);
}

/// A tile of faint noise that dithers the room's lift, so it never bands.
pub fn grain() -> gdk::Texture {
    const SIDE: usize = 128;
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut pixels = Vec::with_capacity(SIDE * SIDE * 4);
    for _ in 0..SIDE * SIDE {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let level = (state >> 56) as u8;
        // Premultiplied grey at a low alpha: half the tile lightens, half darkens.
        let alpha = 9u8;
        let grey = if level > 127 { alpha } else { 0 };
        pixels.extend_from_slice(&[grey, grey, grey, alpha]);
    }
    gdk::MemoryTexture::new(
        SIDE as i32,
        SIDE as i32,
        gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &glib::Bytes::from_owned(pixels),
        SIDE * 4,
    )
    .upcast()
}

/// The opaque room: one ground, a faint lift around the watch and grain over both.
pub fn room(
    snapshot: &gtk::Snapshot,
    p: &Palette,
    (width, height): (f64, f64),
    centre: (f64, f64),
    grain: &gdk::Texture,
) {
    let bounds = rect(0., 0., width, height);
    snapshot.append_color(&p.room.solid(), &bounds);
    let reach = (height.max(width) * 0.46) as f32;
    snapshot.append_radial_gradient(
        &bounds,
        &at(centre.0, centre.1),
        reach,
        reach,
        0.,
        1.,
        &[stop(0., p.room_lift.solid()), stop(1., p.room.alpha(0.))],
    );
    snapshot.push_repeat(&bounds, None);
    snapshot.append_texture(grain, &rect(0., 0., 128., 128.));
    snapshot.pop();
}

/// Smallest angle between two bearings.
pub fn separation(a: f64, b: f64) -> f64 {
    ((a - b + 540.).rem_euclid(360.) - 180.).abs()
}

/// How strongly the light on `sun` shows on an index at `angle`: full on it, none 40° away.
pub fn proximity(sun: f64, angle: f64) -> f64 {
    let t = (1. - separation(sun, angle) / 40.).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
