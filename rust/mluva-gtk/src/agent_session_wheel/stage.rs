//! The wheel's one custom widget: it lays out the names and the legend around the dial, paints
//! the wash and the dial behind them, and runs the hand and the light as springs.

use super::dial::{self, Face, Light};
use super::palette::{Palette, Rgb, Tokens};
use adw::prelude::*;
use glib::subclass::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, graphene};
use std::cell::{Cell, RefCell};

/// Room between the legend and the bottom edge, and between the legend and the dial.
const FOOTER_BOTTOM: f64 = 40.;
const FOOTER_GAP: f64 = 16.;
/// The flat wash between the desktop and the overlay, in the theme's canvas: strong enough that
/// names stay legible over busy terminals and bright pages, light enough that the desktop shows.
/// A window, which cannot be see-through everywhere, gets an opaque ground instead.
const WASH: f32 = 0.88;
/// A damped spring with a small mechanical overshoot for the hand; a softer one for the light,
/// so the glow trails the hand and settles on the tick.
const HAND: (f64, f64) = (260., 20.);
const SUN: (f64, f64) = (120., 19.);
/// Where the hand starts when the wheel opens.
const OPEN_FROM: f64 = -40.;
const OPEN_SECONDS: f64 = 0.16;

/// A box relative to the ring centre; y grows downward.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Everything the stage needs to lay out and paint one arrangement of the wheel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub face: Face,
    pub names: Vec<Area>,
    /// Left, top, right and bottom of the names and the case around the ring centre.
    pub bounds: (f64, f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Index(usize),
    Dial,
    Outside,
}

#[derive(Default)]
struct Motion {
    hand: f64,
    hand_speed: f64,
    sun: f64,
    sun_speed: f64,
    /// Where the hand and light head, unwrapped so they take the short way round.
    target: Option<f64>,
    /// The previous choice, still blooming while the light leaves it.
    afterglow: Option<f64>,
    /// 0 with no choice, 1 with one: how much light falls on the dial.
    glow: f64,
    open: f64,
    last: Option<i64>,
}

fn shortest(from: f64, to: f64) -> f64 {
    from + ((to - from + 540.).rem_euclid(360.) - 180.)
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Stage {
        pub names: RefCell<Vec<gtk::Widget>>,
        pub dial_text: RefCell<Option<gtk::Widget>>,
        pub footer: RefCell<Option<gtk::Widget>>,
        pub arrangement: RefCell<Scene>,
        pub centre: Cell<(f64, f64)>,
        pub palette: RefCell<Option<Palette>>,
        pub tokens: Cell<Option<Tokens>>,
        pub light: Cell<Light>,
        pub(super) motion: RefCell<Motion>,
        pub hover: Cell<Option<usize>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Stage {
        const NAME: &'static str = "MluvaSessionWheelStage";
        type Type = super::Stage;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Stage {
        fn constructed(&self) {
            self.parent_constructed();
            let stage = self.obj();
            stage.set_hexpand(true);
            stage.set_vexpand(true);
            stage.add_css_class("ml-wheel-stage");
        }
        fn dispose(&self) {
            if let Some(tick) = self.tick.take() {
                tick.remove();
            }
            for child in self.names.take() {
                child.unparent();
            }
            if let Some(child) = self.dial_text.take() {
                child.unparent();
            }
            if let Some(child) = self.footer.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Stage {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (left, top, right, bottom) = self.arrangement.borrow().bounds;
            let footer = self.footer.borrow().as_ref().map_or((0, 0), |footer| {
                let (_, width, _, _) = footer.measure(gtk::Orientation::Horizontal, -1);
                let (_, height, _, _) = footer.measure(gtk::Orientation::Vertical, width);
                (width, height)
            });
            let size = if orientation == gtk::Orientation::Horizontal {
                (right - left).max(f64::from(footer.0) + 48.)
            } else {
                bottom - top + f64::from(footer.1) + FOOTER_BOTTOM + FOOTER_GAP
            };
            let size = size.ceil() as i32;
            (size, size, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let (left, top, right, bottom) = self.arrangement.borrow().bounds;
            let footer = self.footer.borrow();
            let footer_size = footer.as_ref().map_or((0, 0), |footer| {
                let (_, w, _, _) = footer.measure(gtk::Orientation::Horizontal, -1);
                let (_, h, _, _) = footer.measure(gtk::Orientation::Vertical, w);
                (w, h)
            });
            let zone = f64::from(footer_size.1) + FOOTER_BOTTOM + FOOTER_GAP;
            let centre = (
                f64::from(width) / 2. - (left + right) / 2.,
                (f64::from(height) - zone) / 2. - (top + bottom) / 2.,
            );
            self.centre.set(centre);
            let arrangement = self.arrangement.borrow();
            for (child, area) in self.names.borrow().iter().zip(&arrangement.names) {
                child.size_allocate(
                    &gdk::Rectangle::new(
                        (centre.0 + area.x).round() as i32,
                        (centre.1 + area.y).round() as i32,
                        area.width.ceil() as i32,
                        area.height.ceil() as i32,
                    ),
                    -1,
                );
            }
            // The hub text sits on the centre; the hand starts clear of it.
            if let Some(text) = self.dial_text.borrow().as_ref() {
                let (_, w, _, _) = text.measure(gtk::Orientation::Horizontal, -1);
                let (_, h, _, _) = text.measure(gtk::Orientation::Vertical, w);
                text.size_allocate(
                    &gdk::Rectangle::new(
                        (centre.0 - f64::from(w) / 2.).round() as i32,
                        (centre.1 - f64::from(h) / 2.).round() as i32,
                        w,
                        h,
                    ),
                    -1,
                );
            }
            if let Some(footer) = footer.as_ref() {
                footer.size_allocate(
                    &gdk::Rectangle::new(
                        (width - footer_size.0) / 2,
                        height - footer_size.1 - FOOTER_BOTTOM as i32,
                        footer_size.0,
                        footer_size.1,
                    ),
                    -1,
                );
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let stage = self.obj();
            let size = (stage.width() as f32, stage.height() as f32);
            if self.palette.borrow().is_none() {
                let tokens = super::read_tokens(stage.upcast_ref());
                self.tokens.set(Some(tokens));
                self.palette.replace(Some(Palette::new(tokens)));
            }
            let palette = self.palette.borrow();
            let palette = palette.as_ref().expect("palette set above");
            let centre = self.centre.get();
            let motion = self.motion.borrow();
            snapshot.push_opacity(motion.open);
            // One flat wash: no gradient, so nothing can band. An overlay lets the desktop show
            // through it; a window is opaque.
            let see_through = stage
                .root()
                .is_some_and(|root| root.has_css_class("ml-wheel-overlay"));
            snapshot.append_color(
                &palette.wash.alpha(if see_through { WASH } else { 1. }),
                &graphene::Rect::new(0., 0., size.0, size.1),
            );
            let arrangement = self.arrangement.borrow();
            let face = &arrangement.face;
            if face.dial > 0. {
                snapshot.save();
                snapshot.translate(&graphene::Point::new(centre.0 as f32, centre.1 as f32));
                paint_dial(
                    snapshot,
                    palette,
                    face,
                    &motion,
                    self.hover.get(),
                    self.light.get(),
                );
                snapshot.restore();
            }
            drop(motion);
            drop(arrangement);
            self.parent_snapshot(snapshot);
            snapshot.pop();
        }
    }
}

/// The ring, the ticks, the hand and the light on the choice.
fn paint_dial(
    snapshot: &gtk::Snapshot,
    palette: &Palette,
    face: &Face,
    motion: &Motion,
    hover: Option<usize>,
    light: Light,
) {
    let chosen = face
        .chosen
        .and_then(|index| face.marks.get(index).map(|(angle, kind)| (*angle, *kind)));
    match light {
        Light::Trace => dial::trace(snapshot, palette, face, motion.sun, motion.glow),
        Light::Wedge => dial::wedge(snapshot, palette, face, motion.sun, motion.glow),
        Light::Glow => {}
    }
    dial::ring(snapshot, palette, face);
    dial::ticks(
        snapshot,
        palette,
        face,
        hover.filter(|h| face.chosen != Some(*h)),
    );
    let Some((angle, kind)) = chosen else {
        return;
    };
    if light == Light::Glow {
        // The glow arrives with the light and leaves the previous choice as it goes.
        let previous = motion
            .afterglow
            .and_then(|a| face.marks.iter().find(|(m, _)| *m == a).copied());
        for (at, was) in previous.into_iter().chain([(angle, kind)]) {
            let strength = dial::proximity(motion.sun, at);
            dial::pool(snapshot, palette, face, at, strength);
            dial::glow(snapshot, strength, |s, weight| {
                dial::chosen_tick(s, palette, face, at, was, weight)
            });
        }
        dial::glow(snapshot, motion.glow, |s, weight| {
            dial::hand(s, palette, face, motion.hand, weight)
        });
    }
    dial::chosen_tick(snapshot, palette, face, angle, kind, 1.);
    dial::hand(snapshot, palette, face, motion.hand, 1.);
}

/// The theme's colour roles for the dial, with Nord for any that the stylesheet lacks.
#[allow(deprecated)]
fn read_tokens(widget: &gtk::Widget) -> Tokens {
    let context = widget.style_context();
    let get = |name: &str, fallback: Rgb| context.lookup_color(name).map_or(fallback, Rgb::of);
    let nord = Tokens::NORD;
    Tokens {
        canvas: get("vs_canvas", nord.canvas),
        ink: get("vs_ink", nord.ink),
        action: get("vs_action", nord.action),
        warning: get("vs_warning", nord.warning),
        success: get("vs_success", nord.success),
        ink_secondary: get("vs_ink_secondary", nord.ink_secondary),
        ink_muted: get("vs_ink_muted", nord.ink_muted),
    }
}

glib::wrapper! {
    pub struct Stage(ObjectSubclass<imp::Stage>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Stage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Add a name, drawn and focused in the order added.
    pub fn put_name(&self, child: &impl IsA<gtk::Widget>) {
        child.set_parent(self);
        self.imp().names.borrow_mut().push(child.clone().upcast());
    }
    pub fn set_dial_text(&self, child: &impl IsA<gtk::Widget>) {
        child.set_parent(self);
        self.imp().dial_text.replace(Some(child.clone().upcast()));
    }
    pub fn set_footer(&self, child: &impl IsA<gtk::Widget>) {
        child.set_parent(self);
        self.imp().footer.replace(Some(child.clone().upcast()));
    }

    /// Repaint in the new palette when the theme's tokens have changed since the last paint.
    fn follow_theme(&self) {
        let tokens = read_tokens(self.upcast_ref());
        if self.imp().tokens.get() != Some(tokens) {
            self.imp().palette.take();
        }
    }

    /// Place the names and size the dial.
    pub fn arrange(&self, arrangement: Scene) {
        self.follow_theme();
        self.imp().arrangement.replace(arrangement);
        self.queue_resize();
        self.queue_draw();
    }

    /// How the light falls on the choice.
    pub fn set_light(&self, light: Light) {
        self.imp().light.set(light);
        self.queue_draw();
    }

    pub fn hover(&self) -> Option<usize> {
        self.imp().hover.get()
    }

    /// The tick whose name is under the pointer brightens; no ring, no box.
    pub fn set_hover(&self, index: Option<usize>) {
        if self.imp().hover.replace(index) != index {
            self.queue_draw();
        }
    }

    /// The dial, or a tick on it, under a point in the stage's coordinates.
    pub fn hit(&self, x: f64, y: f64) -> Hit {
        let arrangement = self.imp().arrangement.borrow();
        let face = &arrangement.face;
        let centre = self.imp().centre.get();
        let (dx, dy) = (x - centre.0, y - centre.1);
        let radius = dx.hypot(dy);
        if face.dial <= 0. || radius > face.outer() + 12. {
            return Hit::Outside;
        }
        let bearing = dx.atan2(-dy).to_degrees();
        let best = face
            .marks
            .iter()
            .enumerate()
            .filter_map(|(index, (angle, _))| {
                let apart = dial::separation(bearing, *angle).to_radians();
                // Along the tick and across it, in the tick's own frame.
                let (along, across) = (radius * apart.cos(), radius * apart.sin());
                let near = along >= face.inner() - 12.
                    && along <= face.outer() + 12.
                    && across.abs() <= 20.;
                near.then_some((index, across.abs()))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((index, _)) => Hit::Index(index),
            None if radius <= face.dial => Hit::Dial,
            None => Hit::Outside,
        }
    }

    fn animated(&self) -> bool {
        WidgetExt::settings(self).is_gtk_enable_animations()
    }

    /// Turn the hand and the light to `angle`, or put them away when nothing is chosen.
    pub fn aim(&self, angle: Option<f64>, swing: bool) {
        let swing = swing && self.animated();
        {
            let mut motion = self.imp().motion.borrow_mut();
            match angle {
                None => {
                    motion.target = None;
                    motion.afterglow = None;
                    motion.glow = 0.;
                }
                Some(angle) => {
                    let first = motion.target.is_none();
                    let previous = motion.target.map(|t| t.rem_euclid(360.));
                    let target = shortest(motion.hand, angle);
                    if swing && !first && previous.is_some_and(|p| (p - angle).abs() > 1e-6) {
                        motion.afterglow = previous;
                    } else {
                        motion.afterglow = None;
                    }
                    motion.target = Some(target);
                    motion.glow = 1.;
                    if !swing || first {
                        motion.hand = target;
                        motion.sun = target;
                        motion.hand_speed = 0.;
                        motion.sun_speed = 0.;
                    }
                }
            }
        }
        if swing {
            self.run();
        }
        self.queue_draw();
    }

    /// Fade the dial in and sweep the hand in from the side.
    pub fn open(&self) {
        self.follow_theme();
        let animated = self.animated();
        {
            let mut motion = self.imp().motion.borrow_mut();
            motion.afterglow = None;
            if animated {
                motion.open = 0.;
                if let Some(target) = motion.target {
                    motion.hand = shortest(target, OPEN_FROM);
                    motion.sun = motion.hand;
                    motion.hand_speed = 0.;
                    motion.sun_speed = 0.;
                }
            } else {
                motion.open = 1.;
                if let Some(target) = motion.target {
                    motion.hand = target;
                    motion.sun = target;
                }
            }
        }
        if animated {
            self.run();
        }
        self.queue_draw();
    }

    fn run(&self) {
        if self.imp().tick.borrow().is_some() {
            return;
        }
        self.imp().motion.borrow_mut().last = None;
        let id = self.add_tick_callback(|stage, clock| {
            let now = clock.frame_time();
            let at_rest = {
                let mut motion = stage.imp().motion.borrow_mut();
                let step = motion
                    .last
                    .map_or(1. / 60., |last| ((now - last) as f64 / 1e6).clamp(0., 0.05));
                motion.last = Some(now);
                advance(&mut motion, step)
            };
            stage.queue_draw();
            if at_rest {
                stage.imp().tick.take();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        self.imp().tick.replace(Some(id));
    }
}

/// Integrate the springs for `seconds` in small steps; true once everything has settled.
fn advance(motion: &mut Motion, seconds: f64) -> bool {
    let steps = (seconds / 0.004).ceil().max(1.) as usize;
    let dt = seconds / steps as f64;
    motion.open = (motion.open + seconds / OPEN_SECONDS).min(1.);
    let Some(target) = motion.target else {
        return motion.open >= 1.;
    };
    for _ in 0..steps {
        motion.hand_speed += (-HAND.0 * (motion.hand - target) - HAND.1 * motion.hand_speed) * dt;
        motion.hand += motion.hand_speed * dt;
        motion.sun_speed += (-SUN.0 * (motion.sun - target) - SUN.1 * motion.sun_speed) * dt;
        motion.sun += motion.sun_speed * dt;
    }
    let settled = (motion.hand - target).abs()
        + motion.hand_speed.abs()
        + (motion.sun - target).abs()
        + motion.sun_speed.abs()
        < 0.02;
    if settled {
        motion.hand = target;
        motion.sun = target;
        motion.hand_speed = 0.;
        motion.sun_speed = 0.;
        motion.afterglow = None;
    }
    settled && motion.open >= 1.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hand_overshoots_a_little_then_settles_on_the_index() {
        let mut motion = Motion {
            target: Some(90.),
            open: 1.,
            ..Motion::default()
        };
        let mut peak = 0_f64;
        let mut frames = 0;
        while !advance(&mut motion, 1. / 60.) {
            peak = peak.max(motion.hand);
            frames += 1;
            assert!(frames < 600, "the hand never settles");
        }
        assert!(peak > 90. && peak < 90. + 90. * 0.08, "overshoot {peak}");
        assert_eq!(motion.hand, 90.);
        assert_eq!(motion.sun, 90.);
    }

    #[test]
    fn the_light_trails_the_hand() {
        let mut motion = Motion {
            target: Some(120.),
            open: 1.,
            ..Motion::default()
        };
        for _ in 0..6 {
            advance(&mut motion, 1. / 60.);
        }
        assert!(
            motion.hand > motion.sun,
            "{} vs {}",
            motion.hand,
            motion.sun
        );
    }

    #[test]
    fn turns_take_the_short_way_round() {
        assert_eq!(shortest(350., 10.), 370.);
        assert_eq!(shortest(10., 350.), -10.);
        assert_eq!(shortest(0., 180.).abs(), 180.);
    }
}
