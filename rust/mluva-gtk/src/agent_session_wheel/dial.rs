//! Painting the dial with GtkSnapshot: a hairline ring, one tick per session and a hand, with one
//! light: a trace along the ring at the hand's bearing. Everything is drawn live in the theme's
//! colours; nothing is baked, so the whole dial is a few strokes per frame even on the cairo
//! renderer. Every quantity that changes with the choice arrives here as an eased level, so the
//! painting itself never snaps.

use super::palette::{Palette, Rgb};
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

/// How far one tick has come, each from 0 to 1: faded in, lifted by hover or focus, and lit as
/// the choice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tick {
    pub shown: f64,
    pub lift: f64,
    pub chosen: f64,
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
const HUB_CLEAR: (f64, f64) = (88., 22.);
/// The trace's bright core reaches this share of the tick spacing either side of the hand, and
/// never more than `TRACE_SPREAD` degrees, so it stays one light and never a highlighter.
const TRACE_SHARE: f64 = 0.55;
const TRACE_SPREAD: f64 = 16.;
/// Under the core, a low fade of at most `SHOULDER_LEVEL` reaches the same share of the spacing,
/// at least `SHOULDER` and at most `SHOULDER_MAX` degrees either side, so the light reads as
/// light at any count.
const SHOULDER: f64 = 12.;
const SHOULDER_MAX: f64 = 34.;
const SHOULDER_LEVEL: f64 = 0.3;
/// The trace's tail reaches as far behind the hand as the hand turns in this many seconds, and
/// never more than `TAIL_MAX` degrees.
const TAIL_SECONDS: f64 = 0.045;
const TAIL_MAX: f64 = 40.;
/// Stroke widths: the ring, a tick, the chosen tick, the hand, the trace's core and its halo.
const RING_WIDTH: f32 = 1.;
const TICK_WIDTH: f64 = 1.5;
const CHOSEN_WIDTH: f64 = 2.5;
const HAND_WIDTH: f32 = 1.75;
const CORE_WIDTH: f32 = 2.;
const HALO_WIDTH: f32 = 10.;

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
fn mix(a: RGBA, b: RGBA, t: f64) -> RGBA {
    let t = t.clamp(0., 1.) as f32;
    let m = |x: f32, y: f32| x + (y - x) * t;
    RGBA::new(
        m(a.red(), b.red()),
        m(a.green(), b.green()),
        m(a.blue(), b.blue()),
        m(a.alpha(), b.alpha()),
    )
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

/// One tick per session, a line inside the ring in the warning colour when blocked, with a dot
/// when working, hollow for draft only and a faint point when gone. Each fades in by `shown`,
/// lifts by `lift` while its name is hovered or focused, and grows by `chosen` into the chosen
/// tick: longer, wider and in the light's colour, `warn` of the way to amber. The tick that
/// stops being chosen shrinks back the same way, so the two trade places smoothly.
pub fn ticks(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, levels: &[Tick], warn: f64) {
    let r = face.dial;
    let lit = p.light(warn).alpha(1.);
    // The most chosen tick is drawn last, above its neighbours.
    let mut order: Vec<usize> = (0..face.marks.len()).collect();
    order.sort_by(|a, b| {
        let level = |i: &usize| levels.get(*i).map_or(0., |t| t.chosen);
        level(a).total_cmp(&level(b))
    });
    for index in order {
        let (angle, kind) = face.marks[index];
        let Tick {
            shown,
            lift,
            chosen,
        } = levels.get(index).copied().unwrap_or(Tick {
            shown: 1.,
            ..Tick::default()
        });
        if shown <= 0. {
            continue;
        }
        let lift = lift * (1. - chosen);
        let length = TICK + (CHOSEN_TICK - TICK) * chosen + 4. * lift;
        let width = (TICK_WIDTH + (CHOSEN_WIDTH - TICK_WIDTH) * chosen + 0.5 * lift) as f32;
        let ink = p.ink.alpha((0.72 + 0.28 * lift) as f32);
        let fade = |color: RGBA| {
            RGBA::new(
                color.red(),
                color.green(),
                color.blue(),
                color.alpha() * shown as f32,
            )
        };
        match kind {
            Kind::Gone => {
                dot(
                    snapshot,
                    point(angle, r - TICK / 2.),
                    1.5,
                    &fade(p.ink.alpha((0.4 * (1. - chosen)) as f32)),
                );
                if chosen > 0. {
                    let (x, y) = point(angle, r - TICK / 2.);
                    let ring = gsk::PathBuilder::new();
                    ring.add_circle(&at(x, y), (2. + 2. * chosen) as f32);
                    let color = fade(RGBA::new(lit.red(), lit.green(), lit.blue(), chosen as f32));
                    snapshot.append_stroke(&ring.to_path(), &gsk::Stroke::new(1.5), &color);
                }
            }
            Kind::Draft => {
                snapshot.save();
                snapshot.rotate(angle as f32);
                let half = 3. + 0.5 * chosen;
                let outline = gsk::PathBuilder::new();
                outline.add_rect(&rect(-half, -r, 2. * half, length));
                let stroke = gsk::Stroke::new((1. + 0.5 * chosen) as f32);
                snapshot.append_stroke(&outline.to_path(), &stroke, &fade(mix(ink, lit, chosen)));
                snapshot.restore();
            }
            Kind::Blocked => snapshot.append_stroke(
                &radial(angle, r - length, r),
                &round(width),
                &fade(mix(p.warning.alpha(0.95), lit, chosen)),
            ),
            Kind::Ready | Kind::Working => {
                snapshot.append_stroke(
                    &radial(angle, r - length, r),
                    &round(width),
                    &fade(mix(ink, lit, chosen)),
                );
                if kind == Kind::Working {
                    dot(
                        snapshot,
                        point(angle, r - length - 7.),
                        2.,
                        &fade(p.working.alpha(0.95)),
                    );
                }
            }
        }
    }
}

/// Where the hand starts at `angle`: on the ellipse around the hub text.
fn hand_start(angle: f64) -> f64 {
    let (sin, cos) = angle.to_radians().sin_cos();
    let (a, b) = HUB_CLEAR;
    1. / ((sin / a).powi(2) + (cos / b).powi(2)).sqrt()
}

/// The hand: a thin line from the hub text to just short of the chosen tick, the precise pointer.
/// It is in the light's colour on a dark wash and neutral ink on a light one, where a coloured
/// hand would read as a second light; either way it eases to amber by `warn`.
pub fn hand(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64, warn: f64) {
    let from = hand_start(angle);
    let to = face.dial - CHOSEN_TICK - HAND_GAP;
    if to <= from {
        return;
    }
    let color = if p.light_theme {
        p.warning.over(p.ink, warn.clamp(0., 1.) as f32)
    } else {
        p.light(warn)
    };
    snapshot.append_stroke(
        &radial(angle, from, to),
        &round(HAND_WIDTH),
        &color.alpha(1.),
    );
}

/// How far the trace's bright core reaches on each side of the hand, in degrees: a little over
/// half the spacing of the ticks, so it is almost dark halfway to a neighbour and out well before
/// it, and at most `TRACE_SPREAD` when there are only a few.
pub fn trace_spread(count: usize) -> f64 {
    (TRACE_SHARE * 360. / count.max(1) as f64).min(TRACE_SPREAD)
}

/// How far behind the hand the trace's tail reaches, in degrees, while the hand turns at
/// `speed` degrees a second: a comet's tail that grows with speed and is gone at rest.
pub fn trace_tail(speed: f64) -> f64 {
    (speed.abs() * TAIL_SECONDS).min(TAIL_MAX)
}

/// How far the trace's low shoulder reaches on each side of the hand, in degrees.
pub fn shoulder_reach(count: usize) -> f64 {
    (TRACE_SHARE * 360. / count.max(1) as f64).clamp(SHOULDER, SHOULDER_MAX)
}

/// A smooth fall from 1 at `distance` 0 to 0 at `reach`.
fn fall(distance: f64, reach: f64) -> f64 {
    let t = (1. - distance / reach).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// How bright the trace is `offset` degrees from the hand, clockwise positive, with a core that
/// reaches `spread` degrees, a shoulder that reaches `shoulder` and a tail of `tail` degrees
/// behind a hand turning `clockwise` or not. The core stays at full for its inner third and falls
/// smoothly to nothing; the shoulder under it is never brighter than `SHOULDER_LEVEL`; behind the
/// hand both stretch by the tail.
pub fn trace_level(offset: f64, spread: f64, shoulder: f64, tail: f64, clockwise: bool) -> f64 {
    let behind = if clockwise { offset < 0. } else { offset > 0. };
    let distance = offset.abs();
    let stretch = if behind { tail } else { 0. };
    let flat = spread / 3.;
    let core = if distance <= flat {
        1.
    } else {
        fall(distance - flat, spread - flat + stretch)
    };
    let low = SHOULDER_LEVEL * fall(distance, shoulder.max(spread) + stretch);
    core.max(low)
}

/// The light: a trace along the ring at the hand's bearing. On a dark wash a near-white core lies
/// over a soft halo in the light's colour; on a light wash a saturated accent core lies over a
/// faint bloom of the accent, so it reads as coloured light on paper. While the hand turns a tail
/// trails behind it. `strength` dims it in travel and brightens it as the hand settles; `warn`
/// turns it amber.
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
    let spread = trace_spread(face.marks.len());
    let shoulder = shoulder_reach(face.marks.len());
    let tail = trace_tail(speed);
    let clockwise = speed >= 0.;
    let reach = shoulder.max(spread);
    // The arc runs counterclockwise-most first, as a conic gradient does.
    let (before, after) = if clockwise {
        (reach + tail, reach)
    } else {
        (reach, reach + tail)
    };
    let span = before + after;
    // Everything the trace draws, its halo included, lies within the box around its arc; keeping
    // to that box keeps the blur small, which matters on the cairo renderer.
    let (mut left, mut top, mut right, mut bottom) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for k in 0..=32 {
        let bearing = hand - before + span * f64::from(k) / 32.;
        for radius in [face.dial - 8., face.dial + 8.] {
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
    let arc = |s: &gtk::Snapshot, width: f32, color: Rgb, alpha: f64| {
        s.push_mask(gsk::MaskMode::Alpha);
        s.append_stroke(&circle(face.dial), &gsk::Stroke::new(width), &RGBA::BLACK);
        s.pop();
        let stops: Vec<gsk::ColorStop> = (0..=32)
            .map(|k| {
                let along = span * f64::from(k) / 32.;
                let level = trace_level(along - before, spread, shoulder, tail, clockwise);
                stop(along / 360., color.alpha((alpha * strength * level) as f32))
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
    let (halo, halo_alpha) = if p.light_theme {
        (p.accent(warn), 0.25)
    } else {
        (p.light(warn), 0.55)
    };
    snapshot.push_blur(5.);
    arc(snapshot, HALO_WIDTH, halo, halo_alpha);
    snapshot.pop();
    arc(snapshot, CORE_WIDTH, p.core(warn), 1.);
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
        // Turning clockwise, light lingers behind the hand further than ahead of it.
        let (spread, shoulder, tail) = (10., SHOULDER, 20.);
        assert_eq!(trace_level(0., spread, shoulder, tail, true), 1.);
        assert_eq!(trace_level(SHOULDER, spread, shoulder, tail, true), 0.);
        assert!(trace_level(-SHOULDER, spread, shoulder, tail, true) > 0.);
        // Turning back, the tail is on the other side.
        assert!(trace_level(SHOULDER, spread, shoulder, tail, false) > 0.);
        assert_eq!(trace_level(-SHOULDER, spread, shoulder, tail, false), 0.);
    }

    #[test]
    fn the_trace_is_bright_on_the_chosen_tick_and_low_at_a_neighbour() {
        for count in 1..=26 {
            let spacing = 360. / count as f64;
            let (spread, shoulder) = (trace_spread(count), shoulder_reach(count));
            assert!(spread <= TRACE_SPREAD && shoulder >= SHOULDER);
            // Full brightness around the chosen tick.
            assert_eq!(trace_level(spread / 4., spread, shoulder, 0., true), 1.);
            // No more than a low glow on a neighbour.
            let neighbour = trace_level(spacing, spread, shoulder, 0., true);
            assert!(
                neighbour <= SHOULDER_LEVEL,
                "{count}: {neighbour} at a neighbour"
            );
        }
        // A light that reads as light at any count: the low fade spans at least 24°.
        assert!(
            trace_level(
                SHOULDER * 0.8,
                trace_spread(26),
                shoulder_reach(26),
                0.,
                true
            ) > 0.
        );
        // With few sessions the bright core stays short and only the low fade sweeps wide.
        assert!(trace_level(20., trace_spread(1), shoulder_reach(1), 0., true) <= SHOULDER_LEVEL);
        assert!((trace_spread(20) - 9.9).abs() < 1e-9);
    }
}
