//! The wheel's one custom widget: it lays out the names and the legend around the dial, paints
//! the wash and the dial behind them, and runs the dial's motion: the opening, the hand's spring
//! with the light riding it, hover lifts, page turns and the turn to amber. Motion is paint and
//! transform only; it never moves a name's allocation, and nothing waits for it.

use super::dial::{self, Face};
use super::palette::{Palette, Rgb, Tokens};
use adw::prelude::*;
use glib::subclass::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, graphene};
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;

/// The least room between the legend and the bottom edge, and the room between the names and
/// the status line above the legend.
const FOOTER_BOTTOM: f64 = 40.;
const FOOTER_GAP: f64 = 16.;
/// The flat wash between the desktop and the overlay, the theme's canvas deepened: about a
/// quarter of the desktop shows through, and with each glyph's halo in the same colour names
/// keep their contrast over bright pages. A window, which cannot be see-through everywhere, gets
/// an opaque ground instead.
const WASH: f32 = 0.72;

/// The opening, in seconds: the wash and the text fade in over `WASH_IN`; the ring draws itself
/// clockwise from one o'clock over `SWEEP`, decelerating; each tick fades in over `TICK_IN` as
/// the draw passes it and each name settles in over `NAME_IN`, `SETTLE` pixels from further
/// out. The hand springs from one o'clock to the remembered choice meanwhile.
const WASH_IN: f64 = 0.12;
const SWEEP: f64 = 0.22;
const TICK_IN: f64 = 0.05;
const NAME_IN: f64 = 0.08;
const SETTLE: f64 = 10.;
/// When the opening has run its course.
const OPEN_DONE: f64 = SWEEP + NAME_IN;
/// A page turn: the new page's ticks and names turn `TURN_DEGREES` into place and fade in.
const TURN_SECONDS: f64 = 0.18;
const TURN_DEGREES: f64 = 8.;
/// How quickly the light, the amber and a hover lift follow their targets: the time to cover
/// about two thirds of the way. The light rises softly when the hand arrives and dips quickly
/// when it leaves.
const LIGHT_RISE: f64 = 0.1;
const LIGHT_DIP: f64 = 0.04;
const WARN_EASE: f64 = 0.09;
const LIFT_EASE: f64 = 0.05;
/// While the hand turns faster than this, in degrees a second, the light dims to `TRAVEL_LIGHT`.
const TRAVELLING: f64 = 60.;
const TRAVEL_LIGHT: f64 = 0.7;

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

/// A damped spring for the hand, tuned to the length of the hop it makes: one small overshoot
/// that grows with the hop but never past a few degrees, a quick settle for a short hop and a
/// slightly longer one across the dial, and no second swing worth seeing.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spring {
    stiffness: f64,
    damping: f64,
}

impl Spring {
    fn for_hop(degrees: f64) -> Self {
        let distance = degrees.abs().max(1.);
        // The overshoot past the tick in degrees, as a share of the hop, gives the damping ratio.
        let overshoot = (0.07 * distance).clamp(0.5, 3.);
        let share = (overshoot / distance).min(0.12);
        let zeta = -share.ln() / (PI * PI + share.ln().powi(2)).sqrt();
        // The hand reaches its peak in 0.11 s for the shortest hop and 0.25 s across the dial.
        let peak = 0.11 + 0.0008 * distance.min(180.);
        let omega = PI / (peak * (1. - zeta * zeta).sqrt());
        Self {
            stiffness: omega * omega,
            damping: 2. * zeta * omega,
        }
    }
}

impl Default for Spring {
    fn default() -> Self {
        Self::for_hop(60.)
    }
}

/// Smooth deceleration for arrivals, close to `cubic-bezier(0.16, 1, 0.3, 1)`.
fn ease_out(t: f64) -> f64 {
    1. - (1. - t.clamp(0., 1.)).powi(4)
}

/// Move `value` toward `target` as a first-order lag with time constant `tau`.
fn follow(value: &mut f64, target: f64, tau: f64, seconds: f64) {
    *value += (target - *value) * (1. - (-seconds / tau).exp());
    // Closer than this the difference cannot be seen.
    if (*value - target).abs() < 4e-3 {
        *value = target;
    }
}

/// Everything that moves on the dial, as plain numbers: the stage paints from it, and it advances
/// by time alone, so tests and stepped captures drive it exactly.
#[derive(Clone, Debug)]
struct Motion {
    hand: f64,
    /// Degrees a second; positive is clockwise.
    speed: f64,
    /// Where the hand heads, unwrapped so it takes the short way round.
    target: Option<f64>,
    spring: Spring,
    /// How bright the light is: 0 with no choice, dimmer while the hand travels, 1 at rest.
    light: f64,
    /// 0 to 1, from the light's own colour to amber, and where it heads.
    warn: f64,
    warn_target: f64,
    /// Seconds into the opening and into a page turn, and the turn's direction.
    opened: f64,
    turned: f64,
    turn_direction: f64,
    /// Each tick's lift, and the tick whose name is hovered or focused.
    lift: Vec<f64>,
    hover: Option<usize>,
    /// The frame clock's time at the last step, in microseconds.
    last: Option<i64>,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            hand: super::START_DEGREES,
            speed: 0.,
            target: None,
            spring: Spring::default(),
            light: 0.,
            warn: 0.,
            warn_target: 0.,
            opened: OPEN_DONE,
            turned: TURN_SECONDS,
            turn_direction: 1.,
            lift: Vec::new(),
            hover: None,
            last: None,
        }
    }
}

impl Motion {
    /// Head for `angle`, or put the hand away. A swing keeps the hand's place and speed and
    /// retunes the spring for what is left of the way, so a new choice mid-swing turns smoothly;
    /// otherwise the hand is placed at once.
    fn aim(&mut self, angle: Option<f64>, swing: bool) {
        let Some(angle) = angle else {
            self.target = None;
            self.light = 0.;
            return;
        };
        let target = shortest(self.hand, angle);
        if swing && self.target.is_some() {
            self.spring = Spring::for_hop(target - self.hand);
        } else {
            self.hand = target;
            self.speed = 0.;
            self.light = 1.;
        }
        self.target = Some(target);
    }

    /// Begin the opening: the hand springs from one o'clock to the choice while the ring draws.
    fn open(&mut self) {
        self.opened = 0.;
        self.turned = TURN_SECONDS;
        self.lift.iter_mut().for_each(|lift| *lift = 0.);
        self.warn = self.warn_target;
        if let Some(target) = self.target {
            self.hand = shortest(target, super::START_DEGREES);
            self.target = Some(shortest(self.hand, target));
            self.speed = 0.;
            self.spring = Spring::for_hop(target - self.hand);
            self.light = 0.;
        }
    }

    /// Place everything at rest, as with animations disabled.
    fn rest(&mut self) {
        self.opened = OPEN_DONE;
        self.turned = TURN_SECONDS;
        if let Some(target) = self.target {
            self.hand = target;
            self.light = 1.;
        }
        self.speed = 0.;
        self.warn = self.warn_target;
        let hover = self.hover;
        for (index, lift) in self.lift.iter_mut().enumerate() {
            *lift = if hover == Some(index) { 1. } else { 0. };
        }
    }

    /// Advance by `seconds`; true once everything has settled.
    fn advance(&mut self, seconds: f64) -> bool {
        self.opened = (self.opened + seconds).min(OPEN_DONE);
        self.turned = (self.turned + seconds).min(TURN_SECONDS);
        let mut settled = true;
        if let Some(target) = self.target {
            let steps = (seconds / 0.002).ceil().max(1.) as usize;
            let dt = seconds / steps as f64;
            let Spring { stiffness, damping } = self.spring;
            for _ in 0..steps {
                self.speed += (-stiffness * (self.hand - target) - damping * self.speed) * dt;
                self.hand += self.speed * dt;
            }
            // A twentieth of a degree is a quarter of a pixel on the largest ring.
            if (self.hand - target).abs() < 0.05 && self.speed.abs() < 2. {
                self.hand = target;
                self.speed = 0.;
            } else {
                settled = false;
            }
        }
        let light = match self.target {
            None => 0.,
            Some(_) if self.speed.abs() > TRAVELLING => TRAVEL_LIGHT,
            Some(_) => 1.,
        };
        let tau = if light > self.light {
            LIGHT_RISE
        } else {
            LIGHT_DIP
        };
        follow(&mut self.light, light, tau, seconds);
        follow(&mut self.warn, self.warn_target, WARN_EASE, seconds);
        let hover = self.hover;
        for (index, lift) in self.lift.iter_mut().enumerate() {
            let target = if hover == Some(index) { 1. } else { 0. };
            follow(lift, target, LIFT_EASE, seconds);
            settled &= *lift == target;
        }
        settled
            && self.light == light
            && self.warn == self.warn_target
            && self.opened >= OPEN_DONE
            && self.turned >= TURN_SECONDS
    }

    /// How much of the wash, the hub and the legend has faded in.
    fn wash(&self) -> f64 {
        ease_out(self.opened / WASH_IN)
    }
    /// How much of the ring has been drawn: a cubic deceleration, gentler than the other
    /// arrivals, so the draw itself can be seen.
    fn drawn(&self) -> f64 {
        1. - (1. - (self.opened / SWEEP).clamp(0., 1.)).powi(3)
    }
    /// When, in seconds into the opening, the drawing ring reaches `angle`.
    fn reached(angle: f64) -> f64 {
        let share = (angle - super::START_DEGREES).rem_euclid(360.) / 360.;
        SWEEP * (1. - (1. - share).cbrt())
    }
    /// How far a page turn has come, eased.
    fn turning(&self) -> f64 {
        ease_out(self.turned / TURN_SECONDS)
    }
    /// The degrees the page is still turned by.
    fn turn_offset(&self) -> f64 {
        self.turn_direction * TURN_DEGREES * (1. - self.turning())
    }
    /// How far the tick at `angle` has faded in.
    fn tick_shown(&self, angle: f64) -> f64 {
        ease_out((self.opened - Self::reached(angle)) / TICK_IN) * self.turning()
    }
    /// How far the name at `angle` has settled in.
    fn name_shown(&self, angle: f64) -> f64 {
        ease_out((self.opened - Self::reached(angle)) / NAME_IN) * self.turning()
    }
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
        pub(super) motion: RefCell<Motion>,
        pub hover: Cell<Option<usize>>,
        pub tick: RefCell<Option<gtk::TickCallbackId>>,
        /// A stepped clock drives the motion instead of the frame clock, for review captures.
        pub manual: Cell<bool>,
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
            // The names, then the status line and the legend just under them, centred together
            // on the screen, so the line that explains the hub stays near it.
            let group = bottom - top + FOOTER_GAP + f64::from(footer_size.1);
            let group_top = ((f64::from(height) - FOOTER_BOTTOM - group) / 2.).max(0.);
            let centre = (f64::from(width) / 2. - (left + right) / 2., group_top - top);
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
                        (centre.1 + bottom + FOOTER_GAP).round() as i32,
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
            let wash = motion.wash();
            // One flat wash: no gradient, so nothing can band. An overlay lets the desktop show
            // through it; a window is opaque.
            let see_through = stage
                .root()
                .is_some_and(|root| root.has_css_class("ml-wheel-overlay"));
            let ground = if see_through { WASH } else { 1. };
            snapshot.append_color(
                &palette.wash.alpha(ground * wash as f32),
                &graphene::Rect::new(0., 0., size.0, size.1),
            );
            let arrangement = self.arrangement.borrow();
            let face = &arrangement.face;
            if face.dial > 0. {
                snapshot.save();
                snapshot.translate(&graphene::Point::new(centre.0 as f32, centre.1 as f32));
                paint_dial(snapshot, palette, face, &motion);
                snapshot.restore();
            }
            // Each name settles in along its bearing as the ring reaches it, and turns with the
            // page; only its paint moves, never its allocation.
            let turn = motion.turn_offset().to_radians();
            for ((child, area), (angle, _)) in self
                .names
                .borrow()
                .iter()
                .zip(&arrangement.names)
                .zip(&face.marks)
            {
                let shown = motion.name_shown(*angle);
                if shown <= 0. {
                    continue;
                }
                let (cx, cy) = (area.x + area.width / 2., area.y + area.height / 2.);
                let out = cx.hypot(cy).max(1.);
                let settle = SETTLE * (1. - shown);
                let (sin, cos) = turn.sin_cos();
                let dx = cx * cos - cy * sin - cx + settle * cx / out;
                let dy = cx * sin + cy * cos - cy + settle * cy / out;
                snapshot.save();
                snapshot.translate(&graphene::Point::new(dx as f32, dy as f32));
                snapshot.push_opacity(shown);
                stage.snapshot_child(child, snapshot);
                snapshot.pop();
                snapshot.restore();
            }
            let others = [
                self.dial_text.borrow().clone(),
                self.footer.borrow().clone(),
            ];
            snapshot.push_opacity(wash);
            for child in others.iter().flatten() {
                stage.snapshot_child(child, snapshot);
            }
            snapshot.pop();
        }
    }
}

/// The ring, the light along it at the hand's bearing, the ticks, the chosen tick and the hand,
/// as the motion stands. The light is drawn at the hand itself, so it rides the hand through a
/// swing and settles with it; ticks and the hand turn with a page turn.
fn paint_dial(snapshot: &gtk::Snapshot, palette: &Palette, face: &Face, motion: &Motion) {
    dial::ring(
        snapshot,
        palette,
        face,
        motion.drawn(),
        super::START_DEGREES,
    );
    snapshot.save();
    snapshot.rotate(motion.turn_offset() as f32);
    let shown: Vec<f64> = face
        .marks
        .iter()
        .map(|(angle, _)| motion.tick_shown(*angle))
        .collect();
    let chosen = face
        .chosen
        .and_then(|index| face.marks.get(index).map(|mark| (index, *mark)));
    snapshot.push_opacity(motion.wash());
    if chosen.is_some() {
        dial::trace(
            snapshot,
            palette,
            face,
            motion.hand,
            motion.speed,
            motion.light,
            motion.warn,
        );
    }
    snapshot.pop();
    dial::ticks(snapshot, palette, face, &shown, &motion.lift);
    if let Some((index, (angle, kind))) = chosen {
        snapshot.push_opacity(shown[index].max(motion.wash()));
        dial::chosen_tick(snapshot, palette, face, angle, kind, motion.warn);
        snapshot.pop();
        snapshot.push_opacity(motion.wash());
        dial::hand(snapshot, palette, face, motion.hand, motion.warn);
        snapshot.pop();
    }
    snapshot.restore();
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

    /// Place the names and size the dial. A choice that becomes blocked or missing turns amber
    /// gently.
    pub fn arrange(&self, arrangement: Scene) {
        self.follow_theme();
        {
            let mut motion = self.imp().motion.borrow_mut();
            motion.warn_target = if arrangement.face.warn { 1. } else { 0. };
            motion.lift.resize(arrangement.face.marks.len(), 0.);
            if !self.moving() {
                motion.warn = motion.warn_target;
            }
        }
        self.imp().arrangement.replace(arrangement);
        self.queue_resize();
        self.queue_draw();
        if self.moving() {
            self.run();
        }
    }

    pub fn hover(&self) -> Option<usize> {
        self.imp().hover.get()
    }

    /// The active theme's warning colour as `#rrggbb`, for a name's caption.
    pub fn warning_hex(&self) -> String {
        read_tokens(self.upcast_ref()).warning.hex()
    }

    /// True when the active theme's canvas is lighter than its ink.
    pub fn light_theme(&self) -> bool {
        Palette::new(read_tokens(self.upcast_ref())).light_theme
    }

    /// The tick whose name is under the pointer, or focused, lifts a little; no ring, no box.
    pub fn set_hover(&self, index: Option<usize>) {
        if self.imp().hover.replace(index) == index {
            return;
        }
        let moving = self.moving();
        {
            let mut motion = self.imp().motion.borrow_mut();
            motion.hover = index;
            if !moving {
                motion.rest();
            }
        }
        self.queue_draw();
        if moving {
            self.run();
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

    /// True when motion should play: animations are on and the wheel is on screen, or a stepped
    /// clock drives it.
    fn moving(&self) -> bool {
        self.imp().manual.get() || (self.animated() && self.is_mapped())
    }

    /// Turn the hand to `angle`, or put it away when nothing is chosen. A swing springs from
    /// wherever the hand is, at its present speed; anything else places it at once.
    pub fn aim(&self, angle: Option<f64>, swing: bool) {
        let swing = swing && (self.animated() || self.imp().manual.get());
        self.imp().motion.borrow_mut().aim(angle, swing);
        if swing {
            self.run();
        }
        self.queue_draw();
    }

    /// Turn the page: the new page's ticks and names turn a few degrees into place, clockwise
    /// for the next page and back for the previous one.
    pub fn turn(&self, forward: bool) {
        if !self.moving() {
            return;
        }
        {
            let mut motion = self.imp().motion.borrow_mut();
            motion.turned = 0.;
            motion.turn_direction = if forward { -1. } else { 1. };
        }
        self.run();
        self.queue_draw();
    }

    /// Open the wheel: the ring draws itself from one o'clock, ticks and names arrive as it
    /// passes them, and the hand springs to the remembered choice. With animations off,
    /// everything is placed at rest.
    pub fn open(&self) {
        self.follow_theme();
        let animated = self.animated() || self.imp().manual.get();
        {
            let mut motion = self.imp().motion.borrow_mut();
            if animated {
                motion.open();
            } else {
                motion.rest();
            }
        }
        if animated {
            self.run();
        }
        self.queue_draw();
    }

    /// Advance the motion by `seconds` on a stepped clock, from now on instead of the frame
    /// clock, so captures can be rendered frame by frame. True once everything has settled.
    pub fn step(&self, seconds: f64) -> bool {
        self.imp().manual.set(true);
        if let Some(tick) = self.imp().tick.take() {
            tick.remove();
        }
        let at_rest = self.imp().motion.borrow_mut().advance(seconds);
        self.queue_draw();
        at_rest
    }

    fn run(&self) {
        if self.imp().manual.get() || self.imp().tick.borrow().is_some() {
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
                motion.advance(step)
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

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f64 = 1. / 60.;

    /// Swing from `from` to `to` frame by frame: the furthest past the target, the deepest swing
    /// back before it after that, and the seconds until the hand stays within a quarter of a
    /// degree, which is as good as still.
    fn hop(from: f64, to: f64) -> (f64, f64, f64) {
        let mut motion = Motion {
            hand: from,
            target: Some(from),
            light: 1.,
            ..Motion::default()
        };
        motion.aim(Some(to), true);
        let direction = (to - from).signum();
        let (mut past, mut back, mut peaked) = (0_f64, 0_f64, false);
        let (mut seconds, mut still) = (0., 0.);
        while !(motion.hand == to && motion.speed == 0.) {
            motion.advance(FRAME);
            seconds += FRAME;
            if (motion.hand - to).abs() > 0.25 {
                still = seconds;
            }
            let beyond = (motion.hand - to) * direction;
            if beyond > 0. {
                peaked = true;
                past = past.max(beyond);
            } else if peaked {
                back = back.max(-beyond);
            }
            assert!(seconds < 2., "the hand never settles");
        }
        assert_eq!(motion.hand, to);
        assert!(seconds < 0.7, "full rest after {seconds} s");
        (past, back, still)
    }

    #[test]
    fn a_short_hop_and_one_across_the_dial_each_overshoot_once_a_little_then_settle() {
        let (past, back, seconds) = hop(30., 48.);
        assert!(past > 0.3 && past < 1.8, "short hop overshoot {past}");
        assert!(back < 0.15, "short hop swings back {back}");
        assert!(seconds < 0.25, "short hop takes {seconds} s");
        // 170° clockwise: exactly 180° would be the short way either side.
        let (past, back, seconds) = hop(30., 200.);
        assert!(past > 1.5 && past < 3.5, "long hop overshoot {past}");
        assert!(back < 0.15, "long hop swings back {back}");
        assert!(seconds < 0.4, "long hop takes {seconds} s");
    }

    #[test]
    fn a_new_choice_mid_swing_turns_smoothly_from_where_the_hand_is() {
        let mut motion = Motion {
            hand: 30.,
            target: Some(30.),
            light: 1.,
            ..Motion::default()
        };
        motion.aim(Some(200.), true);
        for _ in 0..5 {
            motion.advance(FRAME);
        }
        let (hand, speed) = (motion.hand, motion.speed);
        // The new choice lies behind the hand: it must turn back without a jump.
        assert!(hand > 60., "the hand has passed the new choice: {hand}");
        assert!(speed.abs() > 100., "still swinging fast: {speed}");
        motion.aim(Some(60.), true);
        // Retargeting moves nothing by itself: the hand keeps its place and speed.
        assert_eq!((motion.hand, motion.speed), (hand, speed));
        let mut previous = motion.hand;
        let mut past = 0_f64;
        let mut frames = 0;
        while !motion.advance(FRAME) {
            // The first frame carries on at the speed the hand already had, give or take what
            // the new pull adds in one frame, and no frame jumps.
            let moved = (motion.hand - previous).abs();
            if frames == 0 {
                let pull = motion.spring.stiffness * (60. - hand).abs() * FRAME * FRAME / 2.;
                assert!(
                    moved <= speed.abs() * FRAME + pull + 0.5,
                    "jump of {moved}°"
                );
            }
            assert!(moved < 30., "jump of {moved}° at frame {frames}");
            previous = motion.hand;
            past = past.max((motion.hand - 60.) * (60. - hand).signum());
            frames += 1;
            assert!(frames < 120, "the hand never settles");
        }
        assert_eq!(motion.hand, 60.);
        assert!(past < 6., "overshoot after the retarget {past}");
    }

    #[test]
    fn the_opening_draws_the_ring_from_one_oclock_and_brings_every_tick_and_name_in_time() {
        let mut motion = Motion {
            target: Some(150.),
            ..Motion::default()
        };
        motion.lift = vec![0.; 20];
        motion.open();
        assert_eq!(motion.hand, START_DEGREES_FOR_TEST);
        assert_eq!((motion.drawn(), motion.wash()), (0., 0.));
        let angles: Vec<f64> = (0..20).map(|i| 30. + 18. * f64::from(i)).collect();
        // Ticks arrive in clockwise order from one o'clock.
        let reached: Vec<f64> = angles.iter().map(|a| Motion::reached(*a)).collect();
        assert!(reached.windows(2).all(|w| w[0] < w[1]));
        let mut seconds = 0.;
        while seconds < OPEN_DONE - 1e-9 {
            motion.advance(FRAME);
            seconds += FRAME;
        }
        assert_eq!((motion.drawn(), motion.wash()), (1., 1.));
        for angle in &angles {
            assert_eq!(motion.tick_shown(*angle), 1.);
            assert_eq!(motion.name_shown(*angle), 1.);
        }
        const { assert!(OPEN_DONE <= 0.3 + 1e-9) };
    }

    #[test]
    fn the_light_dims_while_the_hand_travels_and_brightens_as_it_settles() {
        let mut motion = Motion {
            hand: 30.,
            target: Some(30.),
            light: 1.,
            ..Motion::default()
        };
        motion.aim(Some(210.), true);
        for _ in 0..8 {
            motion.advance(FRAME);
        }
        assert!(
            motion.light < 0.85,
            "light while travelling {}",
            motion.light
        );
        while !motion.advance(FRAME) {}
        assert_eq!(motion.light, 1.);
    }

    #[test]
    fn amber_settles_in_gently_rather_than_flashing() {
        let mut motion = Motion {
            warn_target: 1.,
            ..Motion::default()
        };
        motion.advance(FRAME);
        assert!(motion.warn > 0. && motion.warn < 0.3, "{}", motion.warn);
        let mut seconds = FRAME;
        while motion.warn < 1. {
            motion.advance(FRAME);
            seconds += FRAME;
        }
        assert!(seconds < 0.8 && seconds > 0.15, "{seconds} s");
    }

    #[test]
    fn a_page_turn_and_a_hover_lift_finish_quickly() {
        let mut motion = Motion {
            lift: vec![0.; 6],
            turned: 0.,
            hover: Some(2),
            ..Motion::default()
        };
        assert!(motion.turn_offset().abs() > 0.);
        let mut seconds = 0.;
        while !motion.advance(FRAME) {
            seconds += FRAME;
        }
        assert!(seconds <= 0.4, "{seconds} s");
        assert_eq!(motion.turn_offset(), 0.);
        assert_eq!(motion.lift[2], 1.);
    }

    #[test]
    fn with_animations_off_everything_is_placed_at_rest() {
        let mut motion = Motion {
            target: Some(150.),
            ..Motion::default()
        };
        motion.lift = vec![0.; 6];
        motion.hover = Some(1);
        motion.warn_target = 1.;
        motion.open();
        motion.rest();
        assert_eq!(motion.hand, 150.);
        assert_eq!((motion.light, motion.warn, motion.lift[1]), (1., 1., 1.));
        assert_eq!(
            (motion.drawn(), motion.wash(), motion.turn_offset()),
            (1., 1., 0.)
        );
        assert!(motion.advance(FRAME));
    }

    #[test]
    fn turns_take_the_short_way_round() {
        assert_eq!(shortest(350., 10.), 370.);
        assert_eq!(shortest(10., 350.), -10.);
        assert_eq!(shortest(0., 180.).abs(), 180.);
    }

    const START_DEGREES_FOR_TEST: f64 = super::super::START_DEGREES;
}
