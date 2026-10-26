//! Painting the dial with GtkSnapshot: a hairline ring, one tick per session and a hand, with one
//! light: a trace along the ring at the hand's bearing. Everything is drawn live in the theme's
//! colours; nothing is baked, so the whole dial is a few strokes per frame even on the cairo
//! renderer.

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
/// The trace reaches this share of the tick spacing either side of the hand, and never more than
/// `TRACE_SPREAD` degrees.
const TRACE_SHARE: f64 = 0.55;
const TRACE_SPREAD: f64 = 34.;
/// The trace's tail reaches as far behind the hand as the hand turns in this many seconds, and
/// never more than `TAIL_MAX` degrees.
const TAIL_SECONDS: f64 = 0.045;
const TAIL_MAX: f64 = 40.;
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

/// The hairline ring the ticks hang from, `drawn` of it clockwise from `from` degrees: the whole
/// ring at rest, and growing from one o'clock while the wheel opens. The growing ring is a path of
/// arcs, not a masked circle, so drawing it costs no more than the ring itself.
pub fn ring(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, drawn: f64, from: f64) {
    if drawn <= 0. {
        return;
    }
    let path = if drawn >= 1. {
        circle(face.dial)
    } else {
        let path = gsk::PathBuilder::new();
        let (x, y) = point(from, face.dial);
        path.move_to(x as f32, y as f32);
        let (sweep, radius) = (360. * drawn, face.dial as f32);
        let mut done = 0.;
        // Clockwise arcs of at most half a turn each.
        while done < sweep - 1e-9 {
            done += (sweep - done).min(180.);
            let (x, y) = point(from + done, face.dial);
            path.svg_arc_to(radius, radius, 0., false, true, x as f32, y as f32);
        }
        path.to_path()
    };
    snapshot.append_stroke(&path, &round(RING_WIDTH), &p.ink.alpha(0.24));
}

/// One tick per session, except the chosen one: a line inside the ring, in the warning colour
/// when blocked, with a dot when working, hollow for draft only and a faint point when gone. Each
/// fades in by `appear` and lifts by `lift`, longer, wider and brighter, while its name is hovered
/// or focused.
pub fn ticks(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, appear: &[f64], lift: &[f64]) {
    let r = face.dial;
    for (index, (angle, kind)) in face.marks.iter().enumerate() {
        let shown = appear.get(index).copied().unwrap_or(1.) as f32;
        if face.chosen == Some(index) || shown <= 0. {
            continue;
        }
        let lifted = lift.get(index).copied().unwrap_or(0.);
        let ink = p.ink.alpha((0.72 + 0.28 * lifted as f32) * shown);
        let width = TICK_WIDTH + 0.5 * lifted as f32;
        let length = TICK + 4. * lifted;
        match kind {
            Kind::Gone => dot(
                snapshot,
                point(*angle, r - TICK / 2.),
                1.5,
                &p.ink.alpha(0.4 * shown),
            ),
            Kind::Draft => {
                snapshot.save();
                snapshot.rotate(*angle as f32);
                let outline = gsk::PathBuilder::new();
                outline.add_rect(&rect(-3., -r, 6., length));
                snapshot.append_stroke(&outline.to_path(), &gsk::Stroke::new(1.), &ink);
                snapshot.restore();
            }
            Kind::Blocked => snapshot.append_stroke(
                &radial(*angle, r - length, r),
                &round(width),
                &p.warning.alpha(0.95 * shown),
            ),
            Kind::Ready | Kind::Working => {
                snapshot.append_stroke(&radial(*angle, r - length, r), &round(width), &ink);
                if *kind == Kind::Working {
                    dot(
                        snapshot,
                        point(*angle, r - TICK - 7.),
                        2.,
                        &p.working.alpha(0.95 * shown),
                    );
                }
            }
        }
    }
}

/// The chosen tick: longer, wider and in the light's colour, `warn` of the way to amber. A gone
/// choice keeps a small ring where its tick was.
pub fn chosen_tick(
    snapshot: &gtk::Snapshot,
    p: &Palette,
    face: &Face,
    angle: f64,
    kind: Kind,
    warn: f64,
) {
    let lit = p.light(warn).alpha(1.);
    let r = face.dial;
    match kind {
        Kind::Gone => {
            let (x, y) = point(angle, r - TICK / 2.);
            let ring = gsk::PathBuilder::new();
            ring.add_circle(&at(x, y), 4.);
            snapshot.append_stroke(&ring.to_path(), &gsk::Stroke::new(1.5), &lit);
        }
        Kind::Draft => {
            snapshot.save();
            snapshot.rotate(angle as f32);
            let outline = gsk::PathBuilder::new();
            outline.add_rect(&rect(-3.5, -r, 7., CHOSEN_TICK));
            snapshot.append_stroke(&outline.to_path(), &gsk::Stroke::new(1.5), &lit);
            snapshot.restore();
        }
        _ => snapshot.append_stroke(
            &radial(angle, r - CHOSEN_TICK, r),
            &round(CHOSEN_WIDTH),
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

/// The hand: a thin line from the hub text to just short of the chosen tick, the precise pointer.
pub fn hand(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64, warn: f64) {
    let from = hand_start(angle);
    let to = face.dial - CHOSEN_TICK - HAND_GAP;
    if to <= from {
        return;
    }
    snapshot.append_stroke(
        &radial(angle, from, to),
        &round(HAND_WIDTH),
        &p.light(warn).alpha(1.),
    );
}

/// How far the trace reaches on each side of the hand, in degrees: a little over half the
/// spacing of the ticks, so it is almost dark halfway to a neighbour and out well before it, and
/// at most `TRACE_SPREAD` when there are only a few.
pub fn trace_spread(count: usize) -> f64 {
    (TRACE_SHARE * 360. / count.max(1) as f64).min(TRACE_SPREAD)
}

/// How far behind the hand the trace's tail reaches, in degrees, while the hand turns at
/// `speed` degrees a second: a comet's tail that grows with speed and is gone at rest.
pub fn trace_tail(speed: f64) -> f64 {
    (speed.abs() * TAIL_SECONDS).min(TAIL_MAX)
}

/// How bright the trace is `offset` degrees from the hand (clockwise positive) when it reaches
/// `ahead` degrees on the side the hand turns toward and `behind` on the other: a smooth bump
/// ahead and a longer, softer fall behind.
pub fn trace_level(offset: f64, ahead: f64, behind: f64, clockwise: bool) -> f64 {
    let (toward, away) = if clockwise {
        (offset, -offset)
    } else {
        (-offset, offset)
    };
    if toward >= 0. {
        let t = (1. - toward / ahead).clamp(0., 1.);
        t * t * (3. - 2. * t)
    } else {
        (1. - away / behind).clamp(0., 1.).powf(1.6)
    }
}

/// The light: a trace along the ring, brightest at the hand's bearing, fading smoothly to nothing
/// `trace_spread` degrees ahead and trailing a tail behind while the hand turns. On a dark wash a
/// fine bright line lies over a soft halo; on a light one it is a crisp arc. `strength` dims it
/// while the hand travels and brightens it as it settles; `warn` turns it amber.
pub fn trace(
    snapshot: &gtk::Snapshot,
    p: &Palette,
    face: &Face,
    hand: f64,
    speed: f64,
    strength: f64,
    warn: f64,
) {
    if strength <= 0.01 {
        return;
    }
    let lit = p.light(warn);
    let ahead = trace_spread(face.marks.len());
    let behind = ahead + trace_tail(speed);
    let clockwise = speed >= 0.;
    // The arc runs counterclockwise-most first, as a conic gradient does.
    let (before, after) = if clockwise {
        (behind, ahead)
    } else {
        (ahead, behind)
    };
    let span = before + after;
    // Everything the trace draws, its halo included, lies within the box around its arc; keeping
    // to that box keeps the blur small, which matters on the cairo renderer.
    let (mut left, mut top, mut right, mut bottom) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for k in 0..=32 {
        let bearing = hand - before + span * f64::from(k) / 32.;
        for radius in [face.dial - 6., face.dial + 6.] {
            let (x, y) = point(bearing, radius);
            (left, top) = (left.min(x), top.min(y));
            (right, bottom) = (right.max(x), bottom.max(y));
        }
    }
    let margin = 16.;
    snapshot.push_clip(&rect(
        left - margin,
        top - margin,
        right - left + 2. * margin,
        bottom - top + 2. * margin,
    ));
    let arc = |s: &gtk::Snapshot, width: f32, alpha: f64| {
        s.push_mask(gsk::MaskMode::Alpha);
        s.append_stroke(&circle(face.dial), &gsk::Stroke::new(width), &RGBA::BLACK);
        s.pop();
        let stops: Vec<gsk::ColorStop> = (0..=24)
            .map(|k| {
                let along = span * f64::from(k) / 24.;
                let level = trace_level(along - before, ahead, behind, clockwise);
                stop(along / 360., lit.alpha((alpha * strength * level) as f32))
            })
            .chain([stop(1., clear())])
            .collect();
        let reach = face.dial + 12.;
        s.append_conic_gradient(
            &rect(-reach, -reach, 2. * reach, 2. * reach),
            &at(0., 0.),
            (hand - before).rem_euclid(360.) as f32,
            &stops,
        );
        s.pop();
    };
    if p.light_theme {
        // A halo of a dark accent on a light wash reads as a smudge, not light: a crisp arc only.
        arc(snapshot, 2.5, 0.75);
    } else {
        snapshot.push_blur(5.);
        arc(snapshot, 6., 0.85);
        snapshot.pop();
        arc(snapshot, 1.75, 1.);
    }
    snapshot.pop();
}

/// Smallest angle between two bearings.
pub fn separation(a: f64, b: f64) -> f64 {
    ((a - b + 540.).rem_euclid(360.) - 180.).abs()
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
    fn the_trace_trails_a_tail_only_while_the_hand_turns() {
        assert_eq!(trace_tail(0.), 0.);
        assert!(trace_tail(400.) > 10. && trace_tail(-400.) == trace_tail(400.));
        assert_eq!(trace_tail(1e6), TAIL_MAX);
        // Turning clockwise, the light ahead ends at the spread and the tail lingers behind it.
        let (ahead, behind) = (10., 30.);
        assert_eq!(trace_level(0., ahead, behind, true), 1.);
        assert_eq!(trace_level(10., ahead, behind, true), 0.);
        assert!(trace_level(-15., ahead, behind, true) > 0.);
        assert_eq!(trace_level(-30., ahead, behind, true), 0.);
        // Turning back, the tail is on the other side.
        assert!(trace_level(15., ahead, behind, false) > 0.);
        assert_eq!(trace_level(-10., ahead, behind, false), 0.);
    }

    #[test]
    fn the_trace_lights_the_chosen_tick_and_never_a_neighbour() {
        for count in 1..=26 {
            let spacing = 360. / count as f64;
            let spread = trace_spread(count);
            // Out well before the next tick, and almost dark halfway there.
            assert!(
                spread <= 0.55 * spacing + 1e-9,
                "{count}: {spread}° of {spacing}°"
            );
            let t = (1. - (spacing / 2.) / spread).max(0.);
            let halfway = t * t * (3. - 2. * t);
            assert!(
                halfway < 0.05,
                "{count}: {halfway} of the light left halfway"
            );
            assert!(spread <= TRACE_SPREAD);
        }
        // Few sessions keep a long sweep of light; twenty keep it to the chosen tick.
        assert_eq!(trace_spread(1), TRACE_SPREAD);
        assert_eq!(trace_spread(3), TRACE_SPREAD);
        assert!((trace_spread(6) - 33.).abs() < 1e-9);
        assert!((trace_spread(20) - 9.9).abs() < 1e-9);
    }
}
