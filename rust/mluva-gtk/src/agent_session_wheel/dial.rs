//! Painting the dial with GtkSnapshot: a hairline ring, one tick per session, a hand and one light
//! on the choice. Everything is drawn live in the theme's colours; nothing is baked, so the whole
//! dial is a few strokes per frame even on the cairo renderer.

use super::palette::Palette;
use gtk::gdk::RGBA;
use gtk::prelude::*;
use gtk::{graphene, gsk};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Ready,
    Working,
    Blocked,
    /// Present, but nothing can be sent: recording keeps an editable draft.
    Draft,
    Gone,
}

/// How the light falls on the choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Light {
    /// A soft glow on the chosen tick and the hand.
    #[default]
    Glow,
    /// A light trace along the ring at the chosen bearing.
    Trace,
    /// A faint phosphor wedge from the centre toward the choice.
    Wedge,
}

/// What the dial shows: its size, one tick per session and the choice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Face {
    /// The ring's radius.
    pub dial: f64,
    pub marks: Vec<(f64, Kind)>,
    pub chosen: Option<usize>,
    /// The choice cannot be recorded, so the light turns to the warning colour.
    pub warn: bool,
}

/// A tick's length inside the ring, and the chosen tick's.
const TICK: f64 = 12.;
const CHOSEN_TICK: f64 = 20.;
/// Room the light takes beyond the ring, which names keep clear of.
const HALO: f64 = 8.;
/// The hand stops this far short of the chosen tick.
const HAND_GAP: f64 = 14.;
/// The hand starts on an ellipse around the hub text, so it never runs through the words: half
/// the ellipse's width and half its height.
const HUB_CLEAR: (f64, f64) = (70., 22.);
/// Stroke widths: the ring, a tick, the chosen tick and the hand.
const RING_WIDTH: f32 = 1.;
const TICK_WIDTH: f32 = 1.5;
const CHOSEN_WIDTH: f32 = 2.5;
const HAND_WIDTH: f32 = 1.75;

/// Names keep clear of this radius around the centre.
pub fn halo_radius(dial: f64) -> f64 {
    dial + HALO
}

impl Face {
    /// The band where a tick can be clicked.
    pub fn inner(&self) -> f64 {
        self.dial - CHOSEN_TICK
    }
    pub fn outer(&self) -> f64 {
        self.dial
    }
}

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
fn square(radius: f64) -> graphene::Rect {
    rect(-radius, -radius, 2. * radius, 2. * radius)
}
fn stop(offset: f64, color: RGBA) -> gsk::ColorStop {
    gsk::ColorStop::new(offset as f32, color)
}
fn clear() -> RGBA {
    RGBA::new(0., 0., 0., 0.)
}
fn round(width: f32) -> gsk::Stroke {
    let stroke = gsk::Stroke::new(width);
    stroke.set_line_cap(gsk::LineCap::Round);
    stroke
}

/// A straight line along a bearing between two radii.
fn radial(angle: f64, from: f64, to: f64) -> gsk::Path {
    let path = gsk::PathBuilder::new();
    let (x0, y0) = point(angle, from);
    let (x1, y1) = point(angle, to);
    path.move_to(x0 as f32, y0 as f32);
    path.line_to(x1 as f32, y1 as f32);
    path.to_path()
}

fn dot(snapshot: &gtk::Snapshot, (x, y): (f64, f64), radius: f64, color: &RGBA) {
    let path = gsk::PathBuilder::new();
    path.add_circle(&at(x, y), radius as f32);
    snapshot.append_fill(&path.to_path(), gsk::FillRule::Winding, color);
}

fn circle(radius: f64) -> gsk::Path {
    let path = gsk::PathBuilder::new();
    path.add_circle(&at(0., 0.), radius as f32);
    path.to_path()
}

/// The hairline ring the ticks hang from.
pub fn ring(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    snapshot.append_stroke(
        &circle(face.dial),
        &gsk::Stroke::new(RING_WIDTH),
        &p.ink.alpha(0.24),
    );
}

/// One tick per session, except the chosen one: a line inside the ring, in the warning colour
/// when blocked, with a dot when working, hollow for draft only and a faint point when gone. The
/// hovered tick brightens.
pub fn ticks(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, hover: Option<usize>) {
    let r = face.dial;
    for (index, (angle, kind)) in face.marks.iter().enumerate() {
        if face.chosen == Some(index) {
            continue;
        }
        let hovered = hover == Some(index);
        let ink = p.ink.alpha(if hovered { 1. } else { 0.72 });
        let width = if hovered { 2. } else { TICK_WIDTH };
        let length = if hovered { TICK + 4. } else { TICK };
        match kind {
            Kind::Gone => dot(
                snapshot,
                point(*angle, r - TICK / 2.),
                1.5,
                &p.ink.alpha(0.4),
            ),
            Kind::Draft => {
                snapshot.save();
                snapshot.rotate(*angle as f32);
                let outline = gsk::PathBuilder::new();
                outline.add_rect(&rect(-2., -r, 4., length));
                snapshot.append_stroke(&outline.to_path(), &gsk::Stroke::new(1.), &ink);
                snapshot.restore();
            }
            Kind::Blocked => snapshot.append_stroke(
                &radial(*angle, r - length, r),
                &round(width),
                &p.warning.alpha(0.95),
            ),
            Kind::Ready | Kind::Working => {
                snapshot.append_stroke(&radial(*angle, r - length, r), &round(width), &ink);
                if *kind == Kind::Working {
                    dot(
                        snapshot,
                        point(*angle, r - TICK - 7.),
                        2.,
                        &p.working.alpha(0.95),
                    );
                }
            }
        }
    }
}

/// The chosen tick: longer, wider and in the light's colour, its strokes `weight` times their
/// width. A gone choice keeps a small ring where its tick was.
pub fn chosen_tick(
    snapshot: &gtk::Snapshot,
    p: &Palette,
    face: &Face,
    angle: f64,
    kind: Kind,
    weight: f32,
) {
    let lit = p.light(face.warn).alpha(1.);
    let r = face.dial;
    match kind {
        Kind::Gone => {
            let (x, y) = point(angle, r - TICK / 2.);
            let ring = gsk::PathBuilder::new();
            ring.add_circle(&at(x, y), 4.);
            snapshot.append_stroke(&ring.to_path(), &gsk::Stroke::new(1.5 * weight), &lit);
        }
        Kind::Draft => {
            snapshot.save();
            snapshot.rotate(angle as f32);
            let outline = gsk::PathBuilder::new();
            outline.add_rect(&rect(-2.5, -r, 5., CHOSEN_TICK));
            snapshot.append_stroke(&outline.to_path(), &gsk::Stroke::new(1.5 * weight), &lit);
            snapshot.restore();
        }
        _ => snapshot.append_stroke(
            &radial(angle, r - CHOSEN_TICK, r),
            &round(CHOSEN_WIDTH * weight),
            &lit,
        ),
    }
}

/// Where the hand starts at `angle`: on the ellipse around the hub text.
fn hand_start(angle: f64) -> f64 {
    let (sin, cos) = angle.to_radians().sin_cos();
    let (a, b) = HUB_CLEAR;
    1. / ((sin / a).powi(2) + (cos / b).powi(2)).sqrt()
}

/// The hand: a thin line from the hub text to just short of the chosen tick, `weight` times its
/// width.
pub fn hand(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64, weight: f32) {
    let from = hand_start(angle);
    let to = face.dial - CHOSEN_TICK - HAND_GAP;
    if to <= from {
        return;
    }
    snapshot.append_stroke(
        &radial(angle, from, to),
        &round(HAND_WIDTH * weight),
        &p.light(face.warn).alpha(1.),
    );
}

/// A soft glow under a line: a wide halo and a near one, each a blurred copy of `paint` drawn at
/// the stroke weight it is given.
pub fn glow(snapshot: &gtk::Snapshot, strength: f64, paint: impl Fn(&gtk::Snapshot, f32)) {
    if strength <= 0.01 {
        return;
    }
    snapshot.push_opacity(strength.min(1.));
    for (radius, weight, alpha) in [(12., 5., 0.45), (4., 2.5, 0.6)] {
        snapshot.push_opacity(alpha);
        snapshot.push_blur(radius);
        paint(snapshot, weight);
        snapshot.pop();
        snapshot.pop();
    }
    snapshot.pop();
}

/// A faint pool of light where the hand meets the chosen tick.
pub fn pool(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64, strength: f64) {
    if strength <= 0.01 {
        return;
    }
    let (x, y) = point(angle, face.dial - CHOSEN_TICK / 2.);
    let reach = 38.;
    let lit = p.light(face.warn);
    snapshot.append_radial_gradient(
        &rect(x - reach, y - reach, 2. * reach, 2. * reach),
        &at(x, y),
        reach as f32,
        reach as f32,
        0.,
        1.,
        &[
            stop(0., lit.alpha((0.3 * strength) as f32)),
            stop(0.45, lit.alpha((0.1 * strength) as f32)),
            stop(1., clear()),
        ],
    );
}

/// A light trace along the ring, brightest on `sun` and fading out within `spread` degrees.
pub fn trace(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, sun: f64, strength: f64) {
    if strength <= 0.01 {
        return;
    }
    let lit = p.light(face.warn);
    let spread = 34.;
    let arc = |s: &gtk::Snapshot, width: f32, alpha: f64| {
        s.push_mask(gsk::MaskMode::Alpha);
        s.append_stroke(&circle(face.dial), &gsk::Stroke::new(width), &RGBA::BLACK);
        s.pop();
        let stops: Vec<gsk::ColorStop> = (0..=16)
            .map(|k| {
                let offset = f64::from(k) / 16.;
                // A smooth bump across the spread, nothing outside it.
                let t = 1. - (offset * 2. - 1.).abs();
                let level = t * t * (3. - 2. * t);
                stop(
                    offset * 2. * spread / 360.,
                    lit.alpha((alpha * strength * level) as f32),
                )
            })
            .chain([stop(1., clear())])
            .collect();
        s.append_conic_gradient(
            &square(face.dial + 12.),
            &at(0., 0.),
            (sun - spread).rem_euclid(360.) as f32,
            &stops,
        );
        s.pop();
    };
    snapshot.push_blur(5.);
    arc(snapshot, 6., 0.8);
    snapshot.pop();
    arc(snapshot, 1.5, 1.);
}

/// A faint phosphor wedge from the hub toward `sun`, fading in from the centre and out at the
/// ring, with feathered sides.
pub fn wedge(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, sun: f64, strength: f64) {
    if strength <= 0.01 {
        return;
    }
    let lit = p.light(face.warn);
    let r = face.dial;
    let spread = 12.;
    snapshot.push_mask(gsk::MaskMode::Alpha);
    snapshot.append_radial_gradient(
        &square(r),
        &at(0., 0.),
        r as f32,
        r as f32,
        0.,
        1.,
        &[
            stop(0., clear()),
            stop(0.12, clear()),
            stop(0.88, RGBA::BLACK),
            stop(0.985, RGBA::BLACK),
            stop(1., clear()),
        ],
    );
    snapshot.pop();
    let stops: Vec<gsk::ColorStop> = (0..=16)
        .map(|k| {
            let offset = f64::from(k) / 16.;
            let t = 1. - (offset * 2. - 1.).abs();
            let level = t * t * (3. - 2. * t);
            stop(
                offset * 2. * spread / 360.,
                lit.alpha((0.34 * strength * level) as f32),
            )
        })
        .chain([stop(1., clear())])
        .collect();
    snapshot.append_conic_gradient(
        &square(r),
        &at(0., 0.),
        (sun - spread).rem_euclid(360.) as f32,
        &stops,
    );
    snapshot.pop();
}

/// Smallest angle between two bearings.
pub fn separation(a: f64, b: f64) -> f64 {
    ((a - b + 540.).rem_euclid(360.) - 180.).abs()
}

/// How strongly the light on `sun` shows on a tick at `angle`: full on it, none 40° away.
pub fn proximity(sun: f64, angle: f64) -> f64 {
    let t = (1. - separation(sun, angle) / 40.).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hand_starts_clear_of_the_hub_text() {
        // Sideways the hand starts beyond the text's half width; up and down beyond its half height.
        assert!((hand_start(90.) - HUB_CLEAR.0).abs() < 1e-9);
        assert!((hand_start(0.) - HUB_CLEAR.1).abs() < 1e-9);
        assert!((hand_start(180.) - HUB_CLEAR.1).abs() < 1e-9);
        for degrees in 0..360 {
            let r = hand_start(f64::from(degrees));
            assert!((HUB_CLEAR.1 - 1e-9..=HUB_CLEAR.0 + 1e-9).contains(&r));
        }
    }

    #[test]
    fn light_falls_off_with_distance_from_the_choice() {
        assert_eq!(proximity(30., 30.), 1.);
        assert_eq!(proximity(30., 70.), 0.);
        assert!(proximity(350., 10.) > 0. && proximity(350., 10.) < 1.);
    }
}
