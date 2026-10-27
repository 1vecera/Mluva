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

/// The least room between the legend and the bottom edge, and the room between the names and
/// the status line above the legend.
const FOOTER_BOTTOM: f64 = 40.;
const FOOTER_GAP: f64 = 16.;
/// The flat wash between the desktop and the overlay, the theme's canvas deepened: about a
/// quarter of the desktop shows through. A window, which cannot be see-through everywhere, gets
/// an opaque ground instead.
const WASH: f32 = 0.72;
/// Under every label the overlay clears the desktop, as a terminal overwrites the cells it prints
/// in, in two layers of the wash's colour. Close behind the glyphs and pager arrows, out to
/// `GLYPH_REACH` pixels and softened over `GLYPH_SOFT`, it clears at the full strength, so desktop
/// text never runs through a letter. Around them a faint cell reaches `CELL_PAD` beyond the text
/// sideways and up and down and feathers out over `FEATHER` pixels: it quiets desktop text between
/// the letters and lines, yet over empty ground it differs from the wash by about one step of
/// lightness (CIE L*), too little to read as a panel. Each strength is `(glyphs, cell)`. A light
/// wash over a dark desktop moves furthest from the ground, about seven times as far as a dark
/// wash, while desktop text through it shows lighter than the ground and the labels darker; so the
/// light theme clears more gently and its labels still win.
const CLEAR_DARK: (f32, f32) = (0.9, 0.3);
const CLEAR_LIGHT: (f32, f32) = (0.6, 0.05);
const GLYPH_REACH: f32 = 1.5;
const GLYPH_SOFT: f64 = 2.;
const CELL_PAD: (f64, f64) = (8., 4.);
const FEATHER: f32 = 20.;
/// The focused name's underline: its thickness and its distance below the baseline.
const UNDERLINE: (f64, f64) = (2., 3.);

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
    /// Left, top, right and bottom of the names and the ring around the ring centre.
    pub bounds: (f64, f64, f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Index(usize),
    Dial,
    Outside,
}

/// The opening, about 0.3 s on the strong ease-out: the wash, the hub and the legend fade in over
/// `WASH_IN`; the ring draws itself clockwise from one o'clock over `SWEEP`; each tick fades in
/// over `TICK_IN` as the draw reaches it and each name settles in over `NAME_IN` from `SETTLE`
/// pixels further out, so the dial arrives in reading order. The hand springs from one o'clock
/// to the remembered choice meanwhile. Nothing waits for it.
const WASH_IN: f64 = 0.12;
const SWEEP: f64 = 0.2;
const TICK_IN: f64 = 0.08;
const NAME_IN: f64 = 0.1;
const SETTLE: f64 = 8.;
/// When the opening has run its course.
const OPEN_DONE: f64 = SWEEP + NAME_IN;
/// A page turn: the new page's ticks and names turn `TURN_DEGREES` into place and fade in.
const TURN_SECONDS: f64 = 0.18;
const TURN_DEGREES: f64 = 8.;
/// How quickly eased values follow their targets: the time to cover about two thirds of the way,
/// on an exponential that never overshoots. The light rises softly as the hand arrives and dips
/// quickly when it leaves; the chosen tick grows while the previous one shrinks back.
const LIGHT_RISE: f64 = 0.08;
const LIGHT_DIP: f64 = 0.04;
const WARN_EASE: f64 = 0.08;
const LIFT_EASE: f64 = 0.05;
const CHOSEN_EASE: f64 = 0.06;
/// While the hand turns faster than this, in degrees a second, the light dims to `TRAVEL_LIGHT`.
const TRAVELLING: f64 = 60.;
const TRAVEL_LIGHT: f64 = 0.75;
/// The hand's spring is critically damped, so it never passes its tick. Its natural frequency,
/// in radians a second, falls from `BRISK` for a neighbour to `ACROSS` for a hop across the dial:
/// 99% of the way in about 0.19 s and 0.25 s.
const BRISK: f64 = 36.;
const ACROSS: f64 = 26.;
/// The strong ease-out for things arriving, as a CSS cubic-bezier.
const EASE_OUT: [f64; 4] = [0.23, 1., 0.32, 1.];

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing function at `x`, solved for the curve's parameter.
fn bezier([x1, y1, x2, y2]: [f64; 4], x: f64) -> f64 {
    if x <= 0. {
        return 0.;
    }
    if x >= 1. {
        return 1.;
    }
    let curve = |a: f64, b: f64, s: f64| {
        3. * a * s * (1. - s).powi(2) + 3. * b * s * s * (1. - s) + s.powi(3)
    };
    let (mut low, mut high) = (0., 1.);
    for _ in 0..40 {
        let middle = (low + high) / 2.;
        if curve(x1, x2, middle) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    curve(y1, y2, (low + high) / 2.)
}

/// The strong ease-out at `t`, 0 to 1.
fn ease_out(t: f64) -> f64 {
    bezier(EASE_OUT, t)
}

/// Move `value` toward `target` as a first-order lag with time constant `tau`: an ease-out that
/// never overshoots and retargets smoothly.
fn follow(value: &mut f64, target: f64, tau: f64, seconds: f64) {
    *value += (target - *value) * (1. - (-seconds / tau).exp());
    // Closer than this the difference cannot be seen.
    if (*value - target).abs() < 4e-3 {
        *value = target;
    }
}

/// The hand's natural frequency for a hop of `degrees`.
fn frequency_for_hop(degrees: f64) -> f64 {
    let across = ((degrees.abs() - 15.) / 165.).clamp(0., 1.);
    BRISK + (ACROSS - BRISK) * across
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
    /// The spring's natural frequency for the present hop.
    frequency: f64,
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
    /// How far each tick has grown into the chosen tick, and which tick is chosen.
    chosen: Vec<f64>,
    chosen_target: Option<usize>,
    /// The frame clock's time at the last step, in microseconds.
    last: Option<i64>,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            hand: super::START_DEGREES,
            speed: 0.,
            target: None,
            frequency: BRISK,
            light: 0.,
            warn: 0.,
            warn_target: 0.,
            opened: OPEN_DONE,
            turned: TURN_SECONDS,
            turn_direction: 1.,
            lift: Vec::new(),
            hover: None,
            chosen: Vec::new(),
            chosen_target: None,
            last: None,
        }
    }
}

impl Motion {
    /// Head for `angle`, or put the hand away. A swing keeps the hand's place and tunes the spring
    /// for what is left of the way, so the hand never moves further from the tick it heads for:
    /// speed toward the new tick is kept, held a little firmer when the hand rushes so that it
    /// still arrives without passing the tick, and speed away from it is dropped, so the hand
    /// turns where it is instead of carrying on and coming back. Anything else places the hand at
    /// once.
    fn aim(&mut self, angle: Option<f64>, swing: bool) {
        let Some(angle) = angle else {
            self.target = None;
            self.light = 0.;
            return;
        };
        let target = shortest(self.hand, angle);
        if swing && self.target.is_some() {
            let gap = target - self.hand;
            let mut frequency = frequency_for_hop(gap);
            if self.speed * gap > 0. {
                // A critically damped hand passes its target only if it moves toward it faster
                // than the frequency times the gap; held at least that firm, its distance to
                // the target only shrinks.
                frequency = frequency.max(self.speed.abs() / gap.abs());
            } else {
                // From rest a critically damped hand closes on its target without turning, so
                // dropping the speed away from the new tick is all it takes never to move
                // further from it.
                self.speed = 0.;
            }
            self.frequency = frequency;
        } else {
            self.hand = target;
            self.speed = 0.;
            self.light = 1.;
        }
        self.target = Some(target);
    }

    /// Begin the opening: the hand springs from one o'clock to the choice while the ring draws,
    /// and the chosen tick grows as it arrives.
    fn open(&mut self) {
        self.opened = 0.;
        self.turned = TURN_SECONDS;
        self.lift.iter_mut().for_each(|lift| *lift = 0.);
        self.chosen.iter_mut().for_each(|level| *level = 0.);
        self.warn = self.warn_target;
        if let Some(target) = self.target {
            self.hand = shortest(target, super::START_DEGREES);
            self.target = Some(shortest(self.hand, target));
            self.speed = 0.;
            self.frequency = frequency_for_hop(target - self.hand);
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
        let (hover, chosen) = (self.hover, self.chosen_target);
        for (index, lift) in self.lift.iter_mut().enumerate() {
            *lift = if hover == Some(index) { 1. } else { 0. };
        }
        for (index, level) in self.chosen.iter_mut().enumerate() {
            *level = if chosen == Some(index) { 1. } else { 0. };
        }
    }

    /// Fit the per-tick levels to `count` ticks with `chosen` lit. A new set of ticks, on opening
    /// or a new page, starts at rest; the same set keeps its levels so they can ease.
    fn fit(&mut self, count: usize, chosen: Option<usize>) {
        if self.chosen.len() != count {
            self.lift = vec![0.; count];
            self.chosen = (0..count)
                .map(|index| if chosen == Some(index) { 1. } else { 0. })
                .collect();
        }
        self.chosen_target = chosen;
    }

    /// Advance by `seconds`; true once everything has settled.
    fn advance(&mut self, seconds: f64) -> bool {
        self.opened = (self.opened + seconds).min(OPEN_DONE);
        self.turned = (self.turned + seconds).min(TURN_SECONDS);
        let mut settled = true;
        if let Some(target) = self.target {
            // The exact critically damped step: x(t) = (x0 + (v0 + w x0) t) e^(-w t).
            let (gap, w) = (self.hand - target, self.frequency);
            let decay = (-w * seconds).exp();
            let pull = self.speed + w * gap;
            self.hand = target + (gap + pull * seconds) * decay;
            self.speed = (self.speed - w * pull * seconds) * decay;
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
        let (hover, chosen) = (self.hover, self.chosen_target);
        for (index, lift) in self.lift.iter_mut().enumerate() {
            let target = if hover == Some(index) { 1. } else { 0. };
            follow(lift, target, LIFT_EASE, seconds);
            settled &= *lift == target;
        }
        for (index, level) in self.chosen.iter_mut().enumerate() {
            let target = if chosen == Some(index) { 1. } else { 0. };
            follow(level, target, CHOSEN_EASE, seconds);
            settled &= *level == target;
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
    /// How much of the ring has been drawn.
    fn drawn(&self) -> f64 {
        ease_out(self.opened / SWEEP)
    }
    /// When, in seconds into the opening, the drawing ring reaches `angle`.
    fn reached(angle: f64) -> f64 {
        let share = (angle - super::START_DEGREES).rem_euclid(360.) / 360.;
        let (mut low, mut high) = (0., 1.);
        for _ in 0..40 {
            let middle = (low + high) / 2.;
            if ease_out(middle) < share {
                low = middle;
            } else {
                high = middle;
            }
        }
        SWEEP * (low + high) / 2.
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
        /// The cleared cells, baked into a texture, and the boxes they were baked from: they
        /// stay put while the hand swings, so they are drawn once and reused.
        pub(super) cells: RefCell<Option<BakedCells>>,
    }

    impl Stage {
        /// Clear the cells under every line, from a texture baked the last time the lines were
        /// the same, or baked now.
        fn clear_cells(
            &self,
            snapshot: &gtk::Snapshot,
            color: gtk::gdk::RGBA,
            strength: (f32, f32),
            lines: &[(Line, f64, (f64, f64))],
        ) {
            let Some((boxes, bounds)) = cell_boxes(lines) else {
                return;
            };
            let glyphs = glyph_runs(lines);
            let stage = self.obj();
            let scale = stage.scale_factor().max(1) as f32;
            let key = CellKey {
                boxes: boxes
                    .iter()
                    .map(|(r, shown)| [r.x(), r.y(), r.width(), r.height(), *shown])
                    .collect(),
                glyphs: glyphs
                    .iter()
                    .map(|(ink, at, shown)| (ink.text(), [at.x(), at.y(), *shown]))
                    .collect(),
                paint: [
                    color.red(),
                    color.green(),
                    color.blue(),
                    strength.0,
                    strength.1,
                    scale,
                ],
            };
            if let Some((known, texture, area)) = self.cells.borrow().as_ref()
                && *known == key
            {
                snapshot.append_scaled_texture(texture, gtk::gsk::ScalingFilter::Linear, area);
                return;
            }
            let record = gtk::Snapshot::new();
            paint_cells(&record, color, strength, &boxes, &glyphs, &bounds);
            let Some(node) = record.to_node() else {
                return;
            };
            let Some(renderer) = stage.native().and_then(|native| native.renderer()) else {
                snapshot.append_node(&node);
                return;
            };
            let baking = gtk::Snapshot::new();
            baking.scale(scale, scale);
            baking.translate(&graphene::Point::new(-bounds.x(), -bounds.y()));
            baking.append_node(&node);
            let Some(baked) = baking.to_node() else {
                return;
            };
            let texture = renderer.render_texture(
                &baked,
                Some(&graphene::Rect::new(
                    0.,
                    0.,
                    bounds.width() * scale,
                    bounds.height() * scale,
                )),
            );
            snapshot.append_scaled_texture(&texture, gtk::gsk::ScalingFilter::Linear, &bounds);
            self.cells.replace(Some((key, texture, bounds)));
        }
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
            // Each name settles in along its bearing as the ring reaches it, and turns with the
            // page; only its paint moves, never its allocation.
            let turn = motion.turn_offset().to_radians();
            let names: Vec<(gtk::Widget, f64, (f64, f64))> = self
                .names
                .borrow()
                .iter()
                .zip(&arrangement.names)
                .zip(&face.marks)
                .map(|((child, area), (angle, _))| {
                    let shown = motion.name_shown(*angle);
                    let (cx, cy) = (area.x + area.width / 2., area.y + area.height / 2.);
                    let out = cx.hypot(cy).max(1.);
                    let settle = SETTLE * (1. - shown);
                    let (sin, cos) = turn.sin_cos();
                    let dx = cx * cos - cy * sin - cx + settle * cx / out;
                    let dy = cx * sin + cy * cos - cy + settle * cy / out;
                    (child.clone(), shown, (dx, dy))
                })
                .collect();
            let others: Vec<gtk::Widget> = [
                self.dial_text.borrow().clone(),
                self.footer.borrow().clone(),
            ]
            .into_iter()
            .flatten()
            .collect();
            // Every line of text, with how much it shows and how far its paint has moved.
            let mut lines: Vec<(Line, f64, (f64, f64))> = vec![];
            // How far each line's underline has faded in: a name's follows its tick's eased lift,
            // which keyboard focus raises; the legend's is whole.
            let mut underlines: Vec<f64> = vec![];
            for (index, (child, shown, offset)) in names.iter().enumerate() {
                if *shown > 0. && child.is_drawable() {
                    let lift = motion.lift.get(index).copied().unwrap_or(1.);
                    text_lines(&stage, child, &mut |line| {
                        lines.push((line, *shown, *offset));
                        underlines.push(lift);
                    });
                }
            }
            for child in &others {
                text_lines(&stage, child, &mut |line| {
                    underlines.push(1.);
                    lines.push((line, wash, (0., 0.)))
                });
            }
            if see_through {
                let strength = if palette.light_theme {
                    CLEAR_LIGHT
                } else {
                    CLEAR_DARK
                };
                self.clear_cells(snapshot, palette.wash.alpha(1.), strength, &lines);
            }
            if face.dial > 0. {
                snapshot.save();
                snapshot.translate(&graphene::Point::new(centre.0 as f32, centre.1 as f32));
                paint_dial(snapshot, palette, face, &motion);
                snapshot.restore();
            }
            for (child, shown, (dx, dy)) in &names {
                if *shown <= 0. {
                    continue;
                }
                snapshot.save();
                snapshot.translate(&graphene::Point::new(*dx as f32, *dy as f32));
                snapshot.push_opacity(*shown);
                stage.snapshot_child(child, snapshot);
                snapshot.pop();
                snapshot.restore();
            }
            snapshot.push_opacity(wash);
            for child in &others {
                stage.snapshot_child(child, snapshot);
            }
            snapshot.pop();
            // Keyboard focus underlines the focused name or legend entry in the light's colour.
            let underline = palette.lit.alpha(1.);
            for ((line, shown, (dx, dy)), level) in lines.iter().zip(&underlines) {
                if line.focused {
                    let r = &line.rect;
                    snapshot.append_color(
                        &gtk::gdk::RGBA::new(
                            underline.red(),
                            underline.green(),
                            underline.blue(),
                            (shown * level) as f32,
                        ),
                        &graphene::Rect::new(
                            (f64::from(r.x()) + dx) as f32,
                            (line.baseline + UNDERLINE.1 + dy) as f32,
                            r.width(),
                            UNDERLINE.0 as f32,
                        ),
                    );
                }
            }
        }
    }
}

/// What a bake of the cleared cells depends on: each cell with how much it shows, each label's
/// text with where it sits and how much it shows, and the colour, strengths and scale.
#[derive(PartialEq)]
pub(super) struct CellKey {
    boxes: Vec<[f32; 5]>,
    glyphs: Vec<(glib::GString, [f32; 3])>,
    paint: [f32; 6],
}

/// Cleared cells baked into a texture: what they were baked from, the texture and the area it
/// covers.
type BakedCells = (CellKey, gdk::Texture, graphene::Rect);

/// What the stage paints for a label or an image: a label's whole layout, or a named icon such
/// as a pager arrow with its name and its side in pixels.
#[derive(Clone)]
enum Ink {
    Text(gtk::pango::Layout),
    Icon(glib::GString, gdk::Paintable, f32),
}

impl Ink {
    /// What it shows, so a bake can tell when it changes.
    fn text(&self) -> glib::GString {
        match self {
            Self::Text(layout) => layout.text(),
            Self::Icon(name, ..) => name.clone(),
        }
    }
}

/// One laid-out line of a label, in the stage's coordinates: its logical box, its baseline,
/// whether it is the text of a name or legend entry that has keyboard focus, and on the first
/// line of each label or for an image, what is painted there with where its origin sits.
struct Line {
    rect: graphene::Rect,
    baseline: f64,
    focused: bool,
    ink: Option<(Ink, graphene::Point)>,
}

/// Every line of text in `widget` and the widgets below it, as the stage will paint it, with
/// pager arrows counted as a line each.
fn text_lines(stage: &Stage, widget: &gtk::Widget, found: &mut dyn FnMut(Line)) {
    if !widget.is_drawable() {
        return;
    }
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        if label.text().trim().is_empty() {
            return;
        }
        // The text of a focused name or legend entry is underlined; its key is not.
        let focused = (label.has_css_class("ml-wheel-name")
            || label.has_css_class("ml-wheel-legend-text"))
            && label
                .ancestor(gtk::Button::static_type())
                .is_some_and(|button| {
                    button
                        .state_flags()
                        .contains(gtk::StateFlags::FOCUS_VISIBLE)
                });
        let layout = label.layout();
        let (ox, oy) = label.layout_offsets();
        let mut whole = label
            .compute_point(stage, &graphene::Point::new(ox as f32, oy as f32))
            .map(|origin| (Ink::Text(layout.clone()), origin));
        let scale = f64::from(gtk::pango::SCALE);
        let mut iter = layout.iter();
        loop {
            let (_, logical) = iter.line_extents();
            let x = f64::from(ox) + f64::from(logical.x()) / scale;
            let y = f64::from(oy) + f64::from(logical.y()) / scale;
            let baseline = f64::from(oy) + f64::from(iter.baseline()) / scale;
            if let Some(origin) =
                label.compute_point(stage, &graphene::Point::new(x as f32, y as f32))
            {
                let lift = f64::from(origin.y()) - y;
                found(Line {
                    rect: graphene::Rect::new(
                        origin.x(),
                        origin.y(),
                        (f64::from(logical.width()) / scale) as f32,
                        (f64::from(logical.height()) / scale) as f32,
                    ),
                    baseline: baseline + lift,
                    focused,
                    ink: whole.take(),
                });
            }
            if !iter.next_line() {
                break;
            }
        }
        return;
    }
    if let Some(image) = widget.downcast_ref::<gtk::Image>() {
        if let Some(bounds) = widget.compute_bounds(stage) {
            // The icon as GTK draws it: at its pixel size, centred in the image's box.
            let ink = image.icon_name().map(|name| {
                let side = match image.pixel_size() {
                    side if side > 0 => side,
                    _ if image.icon_size() == gtk::IconSize::Large => 32,
                    _ => 16,
                };
                let paintable = gtk::IconTheme::for_display(&image.display()).lookup_icon(
                    &name,
                    &[],
                    side,
                    image.scale_factor(),
                    gtk::TextDirection::None,
                    gtk::IconLookupFlags::empty(),
                );
                let side = side as f32;
                let origin = graphene::Point::new(
                    bounds.x() + (bounds.width() - side) / 2.,
                    bounds.y() + (bounds.height() - side) / 2.,
                );
                (Ink::Icon(name, paintable.upcast(), side), origin)
            });
            found(Line {
                rect: bounds,
                baseline: f64::from(bounds.y() + bounds.height()),
                focused: false,
                ink,
            });
        }
        return;
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        text_lines(stage, &widget, found);
        child = widget.next_sibling();
    }
}

/// The boxes to clear under every line, padded, with how much each shows, and the area they
/// cover with their feather. Lines that share a row and an offset, such as a key and the first
/// line of its name, become one span.
fn cell_boxes(
    lines: &[(Line, f64, (f64, f64))],
) -> Option<(Vec<(graphene::Rect, f32)>, graphene::Rect)> {
    let mut rows: Vec<(graphene::Rect, f64, (f64, f64))> = vec![];
    for (line, shown, offset) in lines.iter().filter(|(_, shown, _)| *shown > 0.) {
        let r = line.rect;
        let middle = r.y() + r.height() / 2.;
        let row = rows.iter_mut().find(|(other, their_shown, their_offset)| {
            their_offset == offset
                && their_shown == shown
                && (other.y() + other.height() / 2. - middle).abs()
                    < r.height().min(other.height()) / 2.
                && (other.x() - (r.x() + r.width())).max(r.x() - (other.x() + other.width()))
                    < 3. * CELL_PAD.0 as f32
        });
        match row {
            Some((other, _, _)) => *other = other.union(&r),
            None => rows.push((r, *shown, *offset)),
        }
    }
    let mut bounds: Option<graphene::Rect> = None;
    let boxes: Vec<(graphene::Rect, f32)> = rows
        .iter()
        .map(|(r, shown, (dx, dy))| {
            let cell = graphene::Rect::new(
                (f64::from(r.x()) + dx - CELL_PAD.0) as f32,
                (f64::from(r.y()) + dy - CELL_PAD.1) as f32,
                (f64::from(r.width()) + 2. * CELL_PAD.0) as f32,
                (f64::from(r.height()) + 2. * CELL_PAD.1) as f32,
            );
            let reach = cell.inset_r(-FEATHER, -FEATHER);
            bounds = Some(bounds.map_or(reach, |b| b.union(&reach)));
            (cell, *shown as f32)
        })
        .collect();
    bounds.map(|bounds| (boxes, bounds))
}

/// What each shown label or image paints, with where its paint sits and how much it shows.
fn glyph_runs(lines: &[(Line, f64, (f64, f64))]) -> Vec<(Ink, graphene::Point, f32)> {
    lines
        .iter()
        .filter(|(_, shown, _)| *shown > 0.)
        .filter_map(|(line, shown, (dx, dy))| {
            let (ink, origin) = line.ink.as_ref()?;
            let at = graphene::Point::new(
                (f64::from(origin.x()) + dx) as f32,
                (f64::from(origin.y()) + dy) as f32,
            );
            Some((ink.clone(), at, *shown as f32))
        })
        .collect()
}

/// Paint the cleared cells: one mask, so layers that meet or overlap never darken twice, filled
/// with `color` over `bounds`. The mask holds the faint feathered cells and, over them, every
/// label's glyphs and every image spread a little in each direction and softened, at the full
/// strength.
fn paint_cells(
    snapshot: &gtk::Snapshot,
    color: gtk::gdk::RGBA,
    (glyph_strength, cell_strength): (f32, f32),
    boxes: &[(graphene::Rect, f32)],
    glyphs: &[(Ink, graphene::Point, f32)],
    bounds: &graphene::Rect,
) {
    snapshot.push_mask(gtk::gsk::MaskMode::Alpha);
    for (cell, shown) in boxes {
        // A solid core inset by half the feather, never thinner than a pixel, and a blurred edge
        // around it.
        let inset = (FEATHER / 2.).min(cell.height() / 2. - 0.5).max(0.);
        let core = cell.inset_r(inset, inset);
        let black = gtk::gdk::RGBA::new(0., 0., 0., shown * cell_strength);
        snapshot.append_color(&black, &core);
        snapshot.append_outset_shadow(
            &gtk::gsk::RoundedRect::from_rect(core, 0.),
            &black,
            0.,
            0.,
            0.,
            FEATHER,
        );
    }
    snapshot.push_blur(GLYPH_SOFT);
    let black = gtk::gdk::RGBA::new(0., 0., 0., 1.);
    for (ink, at, shown) in glyphs {
        snapshot.push_opacity(f64::from(shown * glyph_strength));
        // The glyphs or the icon and eight copies around them, as one shape at the strength.
        for step in 0..9 {
            let (dx, dy) = if step == 0 {
                (0., 0.)
            } else {
                let turn = std::f32::consts::FRAC_PI_4 * step as f32;
                (GLYPH_REACH * turn.cos(), GLYPH_REACH * turn.sin())
            };
            snapshot.save();
            snapshot.translate(&graphene::Point::new(at.x() + dx, at.y() + dy));
            match ink {
                Ink::Text(layout) => snapshot.append_layout(layout, &black),
                Ink::Icon(_, paintable, side) => {
                    paintable.snapshot(snapshot, f64::from(*side), f64::from(*side))
                }
            }
            snapshot.restore();
        }
        snapshot.pop();
    }
    snapshot.pop();
    snapshot.pop();
    snapshot.append_color(&color, bounds);
    snapshot.pop();
}

/// The ring, the light along it at the hand's bearing, the ticks and the hand, as the motion
/// stands. The light is drawn at the hand itself, so it rides the hand through a swing and settles
/// with it; ticks and the hand turn with a page turn.
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
    let levels: Vec<dial::Tick> = face
        .marks
        .iter()
        .enumerate()
        .map(|(index, (angle, _))| dial::Tick {
            shown: motion.tick_shown(*angle),
            lift: motion.lift.get(index).copied().unwrap_or(0.),
            chosen: motion.chosen.get(index).copied().unwrap_or(0.),
        })
        .collect();
    let chosen = motion.target.is_some() && face.chosen.is_some();
    snapshot.push_opacity(motion.wash());
    if chosen {
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
    dial::ticks(snapshot, palette, face, &levels, motion.warn);
    if chosen {
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
        let moving = self.moving();
        {
            let mut motion = self.imp().motion.borrow_mut();
            motion.warn_target = if arrangement.face.warn { 1. } else { 0. };
            motion.fit(arrangement.face.marks.len(), arrangement.face.chosen);
            if !moving {
                motion.rest();
            }
        }
        self.imp().arrangement.replace(arrangement);
        self.queue_resize();
        self.queue_draw();
        if moving {
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
    /// wherever the hand is, keeping its speed toward the new tick and dropping any away from it;
    /// anything else places it at once.
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
    /// clock, so captures can be rendered frame by frame. Returns the hand's bearing, if it shows.
    pub fn step(&self, seconds: f64) -> Option<f64> {
        self.imp().manual.set(true);
        if let Some(tick) = self.imp().tick.take() {
            tick.remove();
        }
        let hand = {
            let mut motion = self.imp().motion.borrow_mut();
            motion.advance(seconds);
            motion.target.map(|_| motion.hand)
        };
        self.queue_draw();
        hand
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

    fn swinging(from: f64) -> Motion {
        Motion {
            hand: from,
            target: Some(from),
            light: 1.,
            ..Motion::default()
        }
    }

    /// Swing from `from` to `to` frame by frame: the furthest the hand passes the target, and the
    /// seconds until it stays within a quarter of a degree, which is as good as still.
    fn hop(from: f64, to: f64) -> (f64, f64) {
        let mut motion = swinging(from);
        motion.aim(Some(to), true);
        let direction = (to - from).signum();
        let (mut past, mut seconds, mut still) = (0_f64, 0., 0.);
        while !(motion.hand == to && motion.speed == 0.) {
            motion.advance(FRAME);
            seconds += FRAME;
            past = past.max((motion.hand - to) * direction);
            if (motion.hand - to).abs() > 0.25 {
                still = seconds;
            }
            assert!(seconds < 2., "the hand never settles");
        }
        (past, still)
    }

    #[test]
    fn the_hand_never_passes_its_tick_on_a_short_hop_or_across_the_dial() {
        let (past, still) = hop(30., 48.);
        assert!(past <= 1e-9, "short hop passes by {past}");
        assert!(still < 0.2, "short hop still after {still} s");
        // 170° clockwise: exactly 180° would be the short way either side.
        let (past, still) = hop(30., 200.);
        assert!(past <= 1e-9, "long hop passes by {past}");
        assert!(still < 0.36, "long hop still after {still} s");
        assert!(
            still > 0.2,
            "a hop across the dial takes a little longer: {still} s"
        );
    }

    /// Swing from 30° toward 200°, then after `frames` choose `to`. Checks that the retarget keeps
    /// the hand's place, that no frame jumps and that the hand settles on `to`; returns the most
    /// the hand's distance to `to` grows from one frame to the next, and the furthest it passes
    /// `to` from the side it was on when `to` was chosen.
    fn retarget(frames: usize, to: f64) -> (f64, f64) {
        let mut motion = swinging(30.);
        motion.aim(Some(200.), true);
        for _ in 0..frames {
            motion.advance(FRAME);
        }
        let (hand, speed) = (motion.hand, motion.speed);
        motion.aim(Some(to), true);
        assert_eq!(motion.hand, hand);
        let side = (hand - to).signum();
        let (mut previous, mut grows, mut past, mut count) = (hand, 0_f64, 0_f64, 0);
        while !motion.advance(FRAME) {
            let moved = (motion.hand - previous).abs();
            assert!(moved <= speed.abs() * FRAME + 30., "jump of {moved}°");
            grows = grows.max((motion.hand - to).abs() - (previous - to).abs());
            past = past.max((to - motion.hand) * side);
            previous = motion.hand;
            count += 1;
            assert!(count < 120, "the hand never settles");
        }
        grows = grows.max((motion.hand - to).abs() - (previous - to).abs());
        assert!((motion.hand - to).abs() < 1e-9, "{} vs {to}", motion.hand);
        (grows, past)
    }

    #[test]
    fn after_a_new_choice_the_hand_never_moves_further_from_its_tick_nor_passes_it() {
        for (frames, to) in [
            // Ahead of the hand swinging from 30° to 200°: just in front of it, halfway to the
            // first tick and a degree short of it.
            (1, 60.),
            (2, 120.),
            (5, 199.),
            // Behind it: just swept past, swept past long ago, back beyond where the swing
            // started, and passed mid-swing.
            (2, 60.),
            (6, 60.),
            (4, 20.),
            (8, 100.),
            // Further on, beyond the first tick.
            (3, 250.),
        ] {
            let (grows, past) = retarget(frames, to);
            assert!(
                grows <= 1e-9,
                "{frames} frames, {to}°: moves {grows}° further away"
            );
            assert!(past <= 1e-9, "{frames} frames, {to}°: passes it by {past}°");
        }
    }

    #[test]
    fn the_chosen_tick_grows_while_the_previous_one_shrinks_back() {
        let mut motion = swinging(30.);
        motion.fit(6, Some(0));
        motion.fit(6, Some(3));
        motion.aim(Some(210.), true);
        let mut previous = (motion.chosen[0], motion.chosen[3]);
        assert_eq!(previous, (1., 0.));
        let mut frames = 0;
        while !motion.advance(FRAME) {
            let now = (motion.chosen[0], motion.chosen[3]);
            assert!(now.0 <= previous.0 && now.1 >= previous.1, "{now:?}");
            assert!(
                previous.0 - now.0 < 0.3 && now.1 - previous.1 < 0.3,
                "a level jumps"
            );
            previous = now;
            frames += 1;
        }
        assert_eq!(previous, (0., 1.));
        assert!(frames < 40);
    }

    #[test]
    fn the_opening_draws_the_ring_from_one_oclock_and_brings_every_tick_and_name_in_time() {
        let mut motion = Motion {
            target: Some(150.),
            ..Motion::default()
        };
        motion.fit(20, Some(6));
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
        assert!((motion.drawn() - 1.).abs() < 1e-9 && (motion.wash() - 1.).abs() < 1e-9);
        for angle in &angles {
            assert!((motion.tick_shown(*angle) - 1.).abs() < 1e-9);
            assert!((motion.name_shown(*angle) - 1.).abs() < 1e-9);
        }
        const { assert!(OPEN_DONE <= 0.3 + 1e-9) };
    }

    #[test]
    fn the_strong_ease_out_starts_fast_and_lands_softly() {
        assert_eq!(ease_out(0.), 0.);
        assert!((ease_out(1.) - 1.).abs() < 1e-9);
        // Most of the way in the first third, then a long soft landing.
        assert!(ease_out(0.33) > 0.75);
        let steps: Vec<f64> = (0..=10).map(|k| ease_out(f64::from(k) / 10.)).collect();
        assert!(steps.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn the_light_dims_while_the_hand_travels_and_brightens_as_it_settles() {
        let mut motion = swinging(30.);
        motion.aim(Some(200.), true);
        for _ in 0..6 {
            motion.advance(FRAME);
        }
        assert!(
            motion.light < 0.9,
            "light while travelling {}",
            motion.light
        );
        let mut previous = motion.light;
        while !motion.advance(FRAME) {
            // It eases; it never jumps.
            assert!((motion.light - previous).abs() < 0.25);
            previous = motion.light;
        }
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
            turned: 0.,
            ..Motion::default()
        };
        motion.fit(6, None);
        motion.hover = Some(2);
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
            warn_target: 1.,
            ..Motion::default()
        };
        motion.fit(6, Some(2));
        motion.hover = Some(1);
        motion.open();
        motion.rest();
        assert_eq!(motion.hand, 150.);
        assert_eq!((motion.light, motion.warn, motion.lift[1]), (1., 1., 1.));
        assert_eq!((motion.chosen[2], motion.chosen[1]), (1., 0.));
        assert!((motion.drawn() - 1.).abs() < 1e-9 && motion.turn_offset() == 0.);
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
