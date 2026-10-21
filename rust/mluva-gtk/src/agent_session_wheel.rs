//! Native, keyboard-first session wheel; routing remains in the application owner.
//!
//! Sessions sit clockwise from one o'clock in Herdr pane order, each name whole beside its
//! own mark. The ring grows, up to the screen, before a name drifts from its mark or wraps
//! more tightly; nothing is shortened to keep a fixed geometry.
use adw::prelude::*;
use mluva_core::agent_voice::{AgentStatus, Recipient};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::rc::Rc;

/// Selection keys clockwise from one o'clock: the home row, then the top and bottom rows.
pub const KEYS: [char; 26] = [
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p',
    'z', 'x', 'c', 'v', 'b', 'n', 'm',
];
/// The first session in Herdr order sits at one o'clock.
const START_DEGREES: f64 = 30.;
/// Space between the ring and the nearest edge of a name.
const RING_GAP: f64 = 22.;
/// A leader leaves its mark radially for this long when its name sits at the mark's height.
const STUB: f64 = 12.;
/// The longest a radial stub stretches toward its name's centre line.
const MAX_STUB: f64 = 3. * RING_GAP;
/// The longest vertical jog a leader may need before the ring grows instead.
const MAX_SLIP: f64 = 14.;
/// Each growth step while names still drift from their marks.
const GROWTH: f64 = 1.06;
/// Space between the hub and the ring.
const HUB_CLEARANCE: f64 = 34.;
/// Minimum arc between neighbouring session marks.
const MARK_SPACING: f64 = 28.;
const STACK_GAP: f64 = 4.;
const STAGE_MARGIN: f64 = 28.;
/// Name measures in characters, tried in turn until the wheel fits the screen. Most titles
/// fit one comfortable line; longer lines trade width for height, then narrower ones the reverse.
const NAME_MEASURES: [i32; 5] = [44, 58, 72, 34, 26];
const HUB_MIN_RADIUS: f64 = 84.;
/// Room kept for the footer and the compositor's gaps around the dialog.
const SCREEN_RESERVE: (f64, f64) = (96., 168.);

/// A name's box beside the ring, relative to the ring centre; y grows downward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Clockwise from twelve o'clock, in degrees.
    pub angle: f64,
    /// Right-hand names grow rightward from the ring; left-hand names mirror them.
    pub right: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
    pub radius: f64,
    pub labels: Vec<Placement>,
}

/// Clockwise degrees from twelve o'clock for the session at `index` of `count`.
pub fn clock_angle(index: usize, count: usize) -> f64 {
    (START_DEGREES + 360. * index as f64 / count.max(1) as f64).rem_euclid(360.)
}

/// Place names of the given sizes clockwise from one o'clock around a ring that clears the hub.
///
/// Every name sits beside its own mark: marks right of the twelve-to-six axis take names on
/// the right, the rest on the left. Names that would touch are pushed apart as a group; the
/// ring then grows, while the wheel still fits `room`, until no name drifts noticeably from
/// its mark.
pub fn arrange(sizes: &[(f64, f64)], hub_radius: f64, room: (f64, f64)) -> Arrangement {
    let mut radius = (hub_radius + HUB_CLEARANCE).max(sizes.len() as f64 * MARK_SPACING / TAU);
    let mut arrangement = layout(sizes, radius);
    for _ in 0..48 {
        if slip(&arrangement) <= MAX_SLIP {
            break;
        }
        radius *= GROWTH;
        let grown = layout(sizes, radius);
        let (width, height, _) = stage_bounds(&grown);
        if width > room.0 || height > room.1 {
            break;
        }
        arrangement = grown;
    }
    arrangement
}

/// Where a leader would turn level for a name exactly at its mark's height.
fn elbow(angle: f64, radius: f64) -> (f64, f64) {
    stub_end(angle, radius, STUB)
}

fn stub_end(angle: f64, radius: f64, length: f64) -> (f64, f64) {
    let angle = angle.to_radians();
    (
        (radius + length) * angle.sin(),
        -(radius + length) * angle.cos(),
    )
}

/// A leader's turning points to a name centred at height `level`: a radial stub stretched
/// toward that height, then, if the stub cannot reach, a vertical jog further the same way.
/// Names only ever sit level with or poleward of their stub, so a leader never doubles back,
/// and its final run is always level.
pub fn leader(angle: f64, radius: f64, level: f64) -> [(f64, f64); 2] {
    let reach = -angle.to_radians().cos();
    let length = if reach.abs() > 0.2 {
        (level / reach - radius).clamp(STUB, MAX_STUB)
    } else {
        STUB
    };
    let end = stub_end(angle, radius, length);
    [end, (end.0, level)]
}

/// The longest vertical jog any leader needs after its stub.
fn slip(arrangement: &Arrangement) -> f64 {
    arrangement
        .labels
        .iter()
        .map(|l| {
            let level = l.y + l.height / 2.;
            let [end, _] = leader(l.angle, arrangement.radius, level);
            (level - end.1).abs()
        })
        .fold(0., f64::max)
}

fn layout(sizes: &[(f64, f64)], radius: f64) -> Arrangement {
    let count = sizes.len();
    let reach = radius + RING_GAP;
    let mut labels: Vec<Placement> = (0..count)
        .map(|i| {
            let angle = clock_angle(i, count);
            Placement {
                angle,
                right: angle < 180.,
                x: 0.,
                y: elbow(angle, radius).1 - sizes[i].1 / 2.,
                width: sizes[i].0,
                height: sizes[i].1,
            }
        })
        .collect();
    for side in [true, false] {
        let mut column: Vec<usize> = (0..count).filter(|i| labels[*i].right == side).collect();
        // Order by mark height; tops alone can flip neighbours of different heights.
        column.sort_by(|a, b| {
            let centre = |i: &usize| labels[*i].y + labels[*i].height / 2.;
            centre(a).total_cmp(&centre(b))
        });
        stack_from_equator(&mut labels, &column);
    }
    for label in &mut labels {
        let nearest = if label.y <= 0. && label.y + label.height >= 0. {
            0.
        } else {
            label.y.abs().min((label.y + label.height).abs())
        };
        // Clear the ring, and start beyond the leader's turn so every level run points outward.
        let [turn, _] = leader(label.angle, radius, label.y + label.height / 2.);
        let clear = (reach * reach - nearest * nearest)
            .max(0.)
            .sqrt()
            .max(turn.0.abs() + 6.);
        label.x = if label.right {
            clear
        } else {
            -clear - label.width
        };
    }
    Arrangement { radius, labels }
}

/// Keep the name nearest the equator on its mark and push every other name only toward its
/// pole, so that each stub stretches outward and never has to turn back.
fn stack_from_equator(labels: &mut [Placement], column: &[usize]) {
    let centre = |label: &Placement| label.y + label.height / 2.;
    let Some(anchor) = (0..column.len()).min_by(|a, b| {
        centre(&labels[column[*a]])
            .abs()
            .total_cmp(&centre(&labels[column[*b]]).abs())
    }) else {
        return;
    };
    for position in (0..anchor).rev() {
        let limit = labels[column[position + 1]].y - STACK_GAP;
        let label = &mut labels[column[position]];
        label.y = label.y.min(limit - label.height);
    }
    for position in anchor + 1..column.len() {
        let previous = labels[column[position - 1]];
        let label = &mut labels[column[position]];
        label.y = label.y.max(previous.y + previous.height + STACK_GAP);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Availability {
    Ready,
    Working,
    Blocked,
    Unavailable,
}
impl Availability {
    fn of(entry: &Recipient) -> Self {
        match (entry.can_receive(), &entry.status) {
            (true, AgentStatus::Working) => Self::Working,
            (true, _) => Self::Ready,
            (false, AgentStatus::Blocked) => Self::Blocked,
            (false, _) => Self::Unavailable,
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Working => "Working",
            Self::Blocked => "Blocked",
            Self::Unavailable => "Unavailable",
        }
    }
    fn warns(self) -> bool {
        matches!(self, Self::Blocked | Self::Unavailable)
    }
}

fn install_style() {
    thread_local!(static INSTALLED: Cell<bool> = const { Cell::new(false) });
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    if INSTALLED.replace(true) {
        return;
    }
    // Scoped to this window; the shared stylesheet keeps its existing contract.
    let style = gtk::CssProvider::new();
    style.load_from_string(include_str!("../resources/session-wheel.css"));
    gtk::style_context_add_provider_for_display(
        &display,
        &style,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
}

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    for class in classes {
        label.add_css_class(class);
    }
    label
}

fn toggle_class(widget: &impl IsA<gtk::Widget>, class: &str, enabled: bool) {
    if enabled {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

fn key_cap(text: &str) -> gtk::Label {
    let key = label(text, &["ml-wheel-key"]);
    key.set_valign(gtk::Align::Center);
    key
}

fn key_hint(key: &gtk::Label, text: &str) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.append(key);
    content.append(&label(text, &["ml-wheel-hint-label"]));
    gtk::Button::builder().child(&content).build()
}

/// Agent product in the user's words; Claude Code reports either kind.
fn product(kind: &str) -> &str {
    match kind {
        "claude" | "claude_code" => "claude",
        other => other,
    }
}

fn natural_size(widget: &impl IsA<gtk::Widget>) -> (f64, f64) {
    let (_, width, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
    let (_, height, _, _) = widget.measure(gtk::Orientation::Vertical, width);
    (f64::from(width), f64::from(height))
}

fn source(context: &gtk::cairo::Context, color: gtk::gdk::RGBA, alpha: f64) {
    context.set_source_rgba(
        color.red().into(),
        color.green().into(),
        color.blue().into(),
        alpha * f64::from(color.alpha()),
    );
}

/// A session's mark on the ring, where its leader turns level and where its name begins.
#[derive(Clone, Copy)]
struct Mark {
    angle: f64,
    /// The end of the radial stub and the foot of any vertical jog.
    turns: [(f64, f64); 2],
    anchor: (f64, f64),
    availability: Availability,
}
impl Mark {
    fn leader(&self, context: &gtk::cairo::Context, from: (f64, f64)) {
        context.move_to(from.0, from.1);
        for (x, y) in self.turns.into_iter().chain([self.anchor]) {
            context.line_to(x, y);
        }
        let _ = context.stroke();
    }
}

/// Ring geometry in stage coordinates, shared by the drawing layers.
#[derive(Default)]
struct Geometry {
    centre: (f64, f64),
    radius: f64,
    marks: Vec<Mark>,
    selected: Option<usize>,
}
impl Geometry {
    fn point(&self, degrees: f64, radius: f64) -> (f64, f64) {
        let angle = degrees.to_radians();
        (
            self.centre.0 + radius * angle.sin(),
            self.centre.1 - radius * angle.cos(),
        )
    }
}

fn layer(class: &str) -> gtk::DrawingArea {
    gtk::DrawingArea::builder()
        .can_target(false)
        .css_classes([class])
        .build()
}

/// The track, the hairline from each mark to its name, and the marks in the ink colour.
fn dial(geometry: Rc<RefCell<Geometry>>) -> gtk::DrawingArea {
    let area = layer("ml-wheel-dial");
    area.set_draw_func(move |area, context, _, _| {
        let ink = area.color();
        let geometry = geometry.borrow();
        let (x, y) = geometry.centre;
        context.set_line_width(1.);
        source(context, ink, 0.18);
        context.arc(x, y, geometry.radius, 0., TAU);
        let _ = context.stroke();
        for (index, mark) in geometry.marks.iter().enumerate() {
            if geometry.selected == Some(index) {
                continue;
            }
            let (mx, my) = geometry.point(mark.angle, geometry.radius);
            source(context, ink, 0.22);
            mark.leader(context, (mx, my));
            match mark.availability {
                Availability::Ready => {
                    source(context, ink, 0.72);
                    context.arc(mx, my, 3., 0., TAU);
                    let _ = context.fill();
                }
                // Working is a hollow ring, distinct from Ready in every palette.
                Availability::Working => {
                    source(context, ink, 0.9);
                    context.set_line_width(1.5);
                    context.arc(mx, my, 3.5, 0., TAU);
                    let _ = context.stroke();
                    context.set_line_width(1.);
                }
                Availability::Blocked | Availability::Unavailable => {}
            }
        }
    });
    area
}

/// Blocked and missing sessions in the theme's warning colour.
fn alerts(geometry: Rc<RefCell<Geometry>>) -> gtk::DrawingArea {
    let area = layer("ml-wheel-alert");
    area.set_draw_func(move |area, context, _, _| {
        let warning = area.color();
        let geometry = geometry.borrow();
        for (index, mark) in geometry.marks.iter().enumerate() {
            if mark.availability.warns() && geometry.selected != Some(index) {
                let (mx, my) = geometry.point(mark.angle, geometry.radius);
                source(context, warning, 1.);
                context.arc(mx, my, 3.5, 0., TAU);
                let _ = context.fill();
            }
        }
    });
    area
}

/// The accent arc that swings along the track to the chosen session, with its mark and line.
struct Needle {
    area: gtk::DrawingArea,
    angle: Rc<Cell<f64>>,
    motion: adw::TimedAnimation,
}
impl Needle {
    fn new(geometry: Rc<RefCell<Geometry>>) -> Self {
        let area = layer("ml-wheel-needle");
        let angle = Rc::new(Cell::new(f64::NAN));
        let drawn = angle.clone();
        area.set_draw_func(move |area, context, _, _| {
            let geometry = geometry.borrow();
            let (Some(index), false) = (geometry.selected, drawn.get().is_nan()) else {
                return;
            };
            let Some(mark) = geometry.marks.get(index) else {
                return;
            };
            let accent = area.color();
            let (x, y) = geometry.centre;
            let spacing = 360. / geometry.marks.len().max(1) as f64;
            let spread = (spacing * 0.36).min(15.).to_radians();
            let middle = (drawn.get() - 90.).to_radians();
            source(context, accent, 1.);
            context.set_line_cap(gtk::cairo::LineCap::Round);
            context.set_line_width(2.5);
            context.arc(x, y, geometry.radius, middle - spread, middle + spread);
            let _ = context.stroke();
            let (mx, my) = geometry.point(mark.angle, geometry.radius);
            context.set_line_width(1.5);
            context.set_line_join(gtk::cairo::LineJoin::Round);
            mark.leader(context, (mx, my));
            context.arc(mx, my, 4.5, 0., TAU);
            let _ = context.fill();
        });
        let target = adw::CallbackAnimationTarget::new({
            let angle = angle.clone();
            let area = area.clone();
            move |value| {
                angle.set(value.rem_euclid(360.));
                area.queue_draw();
            }
        });
        let motion = adw::TimedAnimation::builder()
            .widget(&area)
            .duration(200)
            .easing(adw::Easing::EaseOutCubic)
            .target(&target)
            .build();
        Self {
            area,
            angle,
            motion,
        }
    }
    /// Swing the shortest way to a new choice; hide or place without motion otherwise.
    fn point(&self, degrees: Option<f64>, warning: bool, swing: bool) {
        toggle_class(&self.area, "ml-wheel-unavailable", warning);
        let current = self.angle.get();
        // Skipping a running swing reports its end value, so settle it first.
        self.motion.skip();
        match degrees {
            Some(target) if swing && !current.is_nan() => {
                let turn = (target - current + 540.).rem_euclid(360.) - 180.;
                self.angle.set(current);
                if turn.abs() > f64::EPSILON {
                    self.motion.set_value_from(current);
                    self.motion.set_value_to(current + turn);
                    self.motion.play();
                }
            }
            Some(target) => self.angle.set(target),
            None => self.angle.set(f64::NAN),
        }
        self.area.queue_draw();
    }
}

/// One session's name with its key cap on the side facing the ring.
struct Slot {
    button: gtk::Button,
    body: gtk::Box,
    key: gtk::Label,
    text: gtk::Box,
    name: gtk::Label,
    meta: gtk::Label,
}
impl Slot {
    fn new() -> Self {
        let key = key_cap("");
        // Names wrap at spaces, or inside a long unbroken token, and are never ellipsized.
        let attributes = gtk::pango::AttrList::new();
        attributes.insert(gtk::pango::AttrInt::new_insert_hyphens(false));
        let name = gtk::Label::builder()
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .attributes(&attributes)
            .css_classes(["ml-wheel-name"])
            .build();
        let meta = label("", &["ml-wheel-meta"]);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_valign(gtk::Align::Center);
        text.append(&name);
        text.append(&meta);
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 9);
        body.append(&key);
        body.append(&text);
        let button = gtk::Button::builder()
            .child(&body)
            .css_classes(["ml-wheel-slot"])
            .visible(false)
            .build();
        Self {
            button,
            body,
            key,
            text,
            name,
            meta,
        }
    }
    fn show(&self, letter: char, entry: &Recipient, selected: bool, duplicate: bool) {
        let availability = Availability::of(entry);
        let name = entry.display_name();
        let note =
            (availability != Availability::Ready).then(|| availability.title().to_lowercase());
        // Names lead; a second line appears only to disambiguate a shared name or flag state.
        let pane = (duplicate && name != entry.pane_id).then_some(entry.pane_id.as_str());
        let meta: Vec<&str> = [pane, note.as_deref()].into_iter().flatten().collect();
        self.key.set_label(&letter.to_ascii_uppercase().to_string());
        self.name.set_label(name);
        self.meta.set_label(&meta.join(" · "));
        self.meta.set_visible(!meta.is_empty());
        toggle_class(&self.meta, "ml-wheel-warning", availability.warns());
        toggle_class(&self.button, "ml-wheel-unavailable", availability.warns());
        toggle_class(&self.button, "ml-wheel-selected", selected);
        let project = std::path::Path::new(&entry.project)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Local");
        self.button.set_tooltip_text(Some(&format!(
            "{name}\n{} · {} · Herdr {} · {project}",
            product(&entry.kind),
            entry.pane_id,
            entry.name
        )));
        self.button
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "{} · {name} · {}{}",
                letter.to_ascii_uppercase(),
                entry.pane_id,
                note.map(|note| format!(" · {note}")).unwrap_or_default()
            ))]);
        self.button.set_visible(true);
    }
    /// Key caps face the ring; names read away from it.
    fn face(&self, right: bool) {
        if right {
            self.body.reorder_child_after(&self.text, Some(&self.key));
        } else {
            self.body.reorder_child_after(&self.key, Some(&self.text));
        }
        let (xalign, justify) = if right {
            (0., gtk::Justification::Left)
        } else {
            (1., gtk::Justification::Right)
        };
        for label in [&self.name, &self.meta] {
            label.set_xalign(xalign);
            label.set_justify(justify);
        }
    }
}

/// The centre is the primary action: the remembered session and its record key.
struct Hub {
    button: gtk::Button,
    content: gtk::Box,
    key: gtk::Label,
    dot: gtk::Box,
    name: gtk::Label,
    status: gtk::Box,
    meta: gtk::Label,
    action: gtk::Box,
    hint: gtk::Label,
}
impl Hub {
    fn new() -> Self {
        let key = key_cap("");
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("ml-wheel-dot");
        dot.set_valign(gtk::Align::Center);
        let name = gtk::Label::builder()
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .justify(gtk::Justification::Center)
            .max_width_chars(24)
            // The hub grows with the chosen name; only runaway metadata is shortened.
            .lines(8)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["ml-wheel-hub-name"])
            .build();
        // The name leads; its state, agent and pane follow on one quiet line.
        let meta = label("", &["ml-wheel-meta"]);
        let status = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        status.set_halign(gtk::Align::Center);
        status.append(&dot);
        status.append(&meta);
        let action = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        action.add_css_class("ml-wheel-action");
        action.set_halign(gtk::Align::Center);
        action.set_margin_top(10);
        action.append(&key);
        action.append(&label("Record", &["ml-wheel-action-label"]));
        let hint = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(24)
            .margin_top(6)
            .css_classes(["ml-wheel-hint"])
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 3);
        content.set_valign(gtk::Align::Center);
        content.set_halign(gtk::Align::Center);
        for child in [
            name.upcast_ref::<gtk::Widget>(),
            status.upcast_ref(),
            action.upcast_ref(),
            hint.upcast_ref(),
        ] {
            content.append(child);
        }
        let button = gtk::Button::builder()
            .child(&content)
            .css_classes(["ml-wheel-hub"])
            .build();
        Self {
            button,
            content,
            key,
            dot,
            name,
            status,
            meta,
            action,
            hint,
        }
    }
    fn show(
        &self,
        target: Option<&Recipient>,
        other_page: Option<usize>,
        empty: bool,
        agent_key: &str,
    ) {
        self.key.set_label(agent_key);
        let Some(target) = target else {
            self.status.set_visible(false);
            self.action.set_visible(false);
            self.button.set_sensitive(false);
            self.name.set_label(if empty {
                "No sessions"
            } else {
                "No session chosen"
            });
            self.hint.set_label(if empty {
                // A no-break space keeps the product name on one line.
                "Start Claude\u{a0}Code or Codex in Herdr"
            } else {
                "Press a letter to choose"
            });
            self.hint.set_visible(true);
            self.button.set_tooltip_text(None);
            self.button
                .update_property(&[gtk::accessible::Property::Label("No session selected")]);
            return;
        };
        let availability = Availability::of(target);
        let ready = target.can_receive();
        let name = target.display_name();
        self.button.set_sensitive(ready);
        self.action.set_visible(ready);
        self.status.set_visible(true);
        toggle_class(
            &self.dot,
            "ml-wheel-busy",
            availability == Availability::Working,
        );
        toggle_class(&self.dot, "ml-wheel-warning", availability.warns());
        self.name.set_label(name);
        let state = availability.title().to_lowercase();
        let place = format!("{state} · {} · {}", product(&target.kind), target.pane_id);
        // The remembered session stays named while another page is shown.
        self.meta.set_label(&match other_page {
            Some(page) => format!("{place} · page {}", page + 1),
            None => place,
        });
        self.hint.set_label("Press a letter to switch");
        self.hint.set_visible(!ready);
        self.button.set_tooltip_text(Some(&format!(
            "{name}\n{} · {} · Herdr {}",
            product(&target.kind),
            target.pane_id,
            target.name
        )));
        self.button
            .update_property(&[gtk::accessible::Property::Label(&if ready {
                format!("{agent_key} · Record to {name} · {}", target.pane_id)
            } else {
                format!("{name} · {} · {}", target.pane_id, availability.title())
            })]);
    }
    /// The smallest circle around the content, so a long chosen name widens the hub.
    fn radius(&self) -> f64 {
        let (width, height) = natural_size(&self.content);
        (width.hypot(height) / 2. + 10.).max(HUB_MIN_RADIUS).ceil()
    }
}

pub struct SessionWheel {
    pub window: adw::Window,
    /// True where the wheel floats over the desktop as a layer-shell overlay, not a window.
    pub overlay: bool,
    pub choices: Vec<gtk::Button>,
    slots: Vec<Slot>,
    stage: gtk::Fixed,
    layers: [gtk::DrawingArea; 2],
    geometry: Rc<RefCell<Geometry>>,
    needle: Needle,
    hub: Hub,
    pub pager: gtk::Box,
    pub previous: gtk::Button,
    pub next: gtk::Button,
    pub page_label: gtk::Label,
    pub record: gtk::Button,
    pub record_key: gtk::Label,
    pub transcript: gtk::Button,
    pub transcript_key: gtk::Label,
    pub cancel: gtk::Button,
    pub page: Cell<usize>,
    rendered_page: Cell<usize>,
    rendered_keys: RefCell<(String, String)>,
    rendered_room: Cell<(f64, f64)>,
    pub entries: RefCell<Vec<Recipient>>,
    pub selected: RefCell<Option<Recipient>>,
}
impl SessionWheel {
    pub fn new() -> Self {
        install_style();
        let window = adw::Window::builder()
            .title("Choose a Herdr session")
            .resizable(false)
            .modal(true)
            .hide_on_close(true)
            .build();
        window.add_css_class("ml-session-wheel");
        // Like Omarchy's launcher: the wheel floats on a scrim over the desktop, with no window
        // frame, wherever the compositor supports layer-shell.
        let overlay = crate::layer_shell::overlay(&window, "mluva-session-wheel");
        if overlay {
            // The compositor sizes an overlay to its output; a fixed size would pin it small.
            window.set_resizable(true);
            window.add_css_class("ml-wheel-overlay");
        }
        // The wheel and its key legend stay together at the centre of a full-screen overlay.
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_valign(gtk::Align::Center);

        let geometry = Rc::new(RefCell::new(Geometry::default()));
        let stage = gtk::Fixed::builder().halign(gtk::Align::Center).build();
        let layers = [dial(geometry.clone()), alerts(geometry.clone())];
        for layer in &layers {
            stage.put(layer, 0., 0.);
        }
        let needle = Needle::new(geometry.clone());
        stage.put(&needle.area, 0., 0.);
        let hub = Hub::new();
        stage.put(&hub.button, 0., 0.);
        // Clockwise insertion keeps Tab order the same as the letters.
        let slots: Vec<_> = KEYS.iter().map(|_| Slot::new()).collect();
        for slot in &slots {
            stage.put(&slot.button, 0., 0.);
        }
        content.append(&stage);

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 28);
        footer.set_halign(gtk::Align::Center);
        footer.add_css_class("ml-wheel-footer");
        let transcript_key = key_cap("");
        let transcript = key_hint(&transcript_key, "Transcript only");
        transcript.set_tooltip_text(Some(
            "Keep an editable transcript in Mluva without copying or sending",
        ));
        let cancel = key_hint(&key_cap("Esc"), "Cancel");
        let pager = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        pager.add_css_class("ml-wheel-pager");
        let previous = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Previous sessions (Left)")
            .build();
        let next = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .tooltip_text("Next sessions (Right)")
            .build();
        let page_label = label("", &["ml-wheel-page"]);
        page_label.set_width_chars(5);
        pager.append(&previous);
        pager.append(&page_label);
        pager.append(&next);
        footer.append(&transcript);
        footer.append(&pager);
        footer.append(&cancel);
        content.append(&footer);
        window.set_content(Some(&content));
        Self {
            window,
            overlay,
            choices: slots.iter().map(|s| s.button.clone()).collect(),
            slots,
            stage,
            layers,
            geometry,
            needle,
            record: hub.button.clone(),
            record_key: hub.key.clone(),
            hub,
            pager,
            previous,
            next,
            page_label,
            transcript,
            transcript_key,
            cancel,
            page: Cell::new(0),
            rendered_page: Cell::new(usize::MAX),
            rendered_keys: RefCell::new((String::new(), String::new())),
            rendered_room: Cell::new((0., 0.)),
            entries: RefCell::new(vec![]),
            selected: RefCell::new(None),
        }
    }
    /// Run `resized` whenever the overlay's output size changes, so the wheel can refit it.
    pub fn connect_resized(&self, resized: impl Fn() + Clone + 'static) {
        self.window.connect_realize(move |window| {
            if let Some(surface) = window.surface() {
                let resized = resized.clone();
                surface.connect_layout(move |_, _, _| resized());
            }
        });
    }
    /// The logical size of the screen showing the wheel, less room for the footer and gaps.
    pub fn available(&self) -> (f64, f64) {
        // A configured overlay covers exactly its output.
        if let Some(surface) = self.window.surface().filter(|_| self.overlay)
            && surface.width() > 0
        {
            return (
                f64::from(surface.width()) - SCREEN_RESERVE.0,
                f64::from(surface.height()) - SCREEN_RESERVE.1,
            );
        }
        let display = WidgetExt::display(&self.window);
        let monitor = self
            .window
            .transient_for()
            .and_then(|parent| parent.surface())
            .and_then(|surface| display.monitor_at_surface(&surface))
            .or_else(|| {
                display
                    .monitors()
                    .item(0)
                    .and_downcast::<gtk::gdk::Monitor>()
            });
        let (width, height) = monitor.map_or((1280, 800), |monitor| {
            let area = monitor.geometry();
            (area.width(), area.height())
        });
        (
            f64::from(width) - SCREEN_RESERVE.0,
            f64::from(height) - SCREEN_RESERVE.1,
        )
    }
    pub fn update(
        &self,
        entries: &[Recipient],
        selected: Option<Recipient>,
        agent_key: &str,
        transcript_key: &str,
    ) {
        let room = self.available();
        if self.entries.borrow().as_slice() == entries
            && *self.selected.borrow() == selected
            && self.rendered_page.get() == self.page.get()
            && self.rendered_keys.borrow().0 == agent_key
            && self.rendered_keys.borrow().1 == transcript_key
            && self.rendered_room.get() == room
        {
            return;
        }
        self.rendered_room.set(room);
        let same_page = self.rendered_page.get() == self.page.get();
        self.rendered_page.set(self.page.get());
        self.rendered_keys
            .replace((agent_key.to_owned(), transcript_key.to_owned()));
        self.entries.replace(entries.to_vec());
        self.selected.replace(selected.clone());
        let pages = entries.len().div_ceil(KEYS.len()).max(1);
        self.page.set(self.page.get().min(pages - 1));
        let page = self.page.get();
        self.pager.set_visible(pages > 1);
        self.page_label
            .set_label(&format!("{} / {pages}", page + 1));
        self.previous.set_sensitive(page > 0);
        self.next.set_sensitive(page + 1 < pages);
        self.transcript_key.set_label(transcript_key);
        self.transcript
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "{transcript_key} · Transcript only"
            ))]);

        let position = selected
            .as_ref()
            .and_then(|s| entries.iter().position(|e| e.same_instance(s)));
        let other_page = position.map(|i| i / KEYS.len()).filter(|p| *p != page);
        self.hub
            .show(selected.as_ref(), other_page, entries.is_empty(), agent_key);
        let mut names: HashMap<&str, usize> = HashMap::new();
        for entry in entries {
            *names.entry(entry.display_name()).or_default() += 1;
        }
        let shown: Vec<&Recipient> = entries
            .iter()
            .skip(page * KEYS.len())
            .take(KEYS.len())
            .collect();
        let chosen = position
            .filter(|_| other_page.is_none())
            .map(|i| i % KEYS.len());
        for (index, slot) in self.slots.iter().enumerate() {
            match shown.get(index) {
                Some(entry) => slot.show(
                    KEYS[index],
                    entry,
                    chosen == Some(index),
                    names[entry.display_name()] > 1,
                ),
                None => slot.button.set_visible(false),
            }
        }
        let arrangement = self.arrange(shown.len());
        let previous_marks = self.geometry.borrow().marks.len();
        self.place(&arrangement, &shown, chosen);
        // Only a choice changing on an unchanged wheel swings; opening or reshaping places.
        let swing = self.window.is_mapped() && same_page && previous_marks == shown.len();
        let warning = selected.as_ref().is_some_and(|s| !s.can_receive());
        self.needle.point(
            chosen.map(|index| clock_angle(index, shown.len())),
            warning,
            swing,
        );
    }
    /// The narrowest name measure whose wheel fits the screen, else the widest.
    fn arrange(&self, count: usize) -> Arrangement {
        let hub_radius = self.hub.radius();
        let (width, height) = self.available();
        let mut arrangement = Arrangement::default();
        for measure in NAME_MEASURES {
            let sizes: Vec<(f64, f64)> = self.slots[..count]
                .iter()
                .map(|slot| {
                    slot.name.set_max_width_chars(measure);
                    slot.button.set_size_request(-1, -1);
                    natural_size(&slot.button)
                })
                .collect();
            arrangement = arrange(&sizes, hub_radius, (width, height));
            let (stage_width, stage_height, _) = stage_bounds(&arrangement);
            if stage_width <= width && stage_height <= height {
                break;
            }
        }
        arrangement
    }
    fn place(&self, arrangement: &Arrangement, shown: &[&Recipient], chosen: Option<usize>) {
        let (width, height, (left, top)) = stage_bounds(arrangement);
        let centre = (-left, -top);
        for layer in self.layers.iter().chain([&self.needle.area]) {
            layer.set_content_width(width.ceil() as i32);
            layer.set_content_height(height.ceil() as i32);
        }
        let hub_radius = self.hub.radius();
        self.hub
            .button
            .set_size_request((2. * hub_radius) as i32, (2. * hub_radius) as i32);
        self.stage.move_(
            &self.hub.button,
            centre.0 - hub_radius,
            centre.1 - hub_radius,
        );
        let mut marks = Vec::with_capacity(shown.len());
        for ((slot, placement), entry) in self.slots.iter().zip(&arrangement.labels).zip(shown) {
            slot.face(placement.right);
            let (x, y) = (centre.0 + placement.x, centre.1 + placement.y);
            // Fixed allocates its minimum size, so hold each name at the size it was placed at.
            slot.button.set_size_request(
                placement.width.ceil() as i32,
                placement.height.ceil() as i32,
            );
            self.stage.move_(&slot.button, x, y);
            let turns = leader(
                placement.angle,
                arrangement.radius,
                placement.y + placement.height / 2.,
            )
            .map(|(tx, ty)| (centre.0 + tx, centre.1 + ty));
            marks.push(Mark {
                angle: placement.angle,
                turns,
                anchor: (
                    if placement.right {
                        x
                    } else {
                        x + placement.width
                    },
                    y + placement.height / 2.,
                ),
                availability: Availability::of(entry),
            });
        }
        self.geometry.replace(Geometry {
            centre,
            radius: arrangement.radius,
            marks,
            selected: chosen,
        });
        for layer in self.layers.iter().chain([&self.needle.area]) {
            layer.queue_draw();
        }
    }
}
impl Default for SessionWheel {
    fn default() -> Self {
        Self::new()
    }
}

/// Stage size and its top-left corner relative to the ring centre, margin included.
fn stage_bounds(arrangement: &Arrangement) -> (f64, f64, (f64, f64)) {
    let ring = arrangement.radius + 6.;
    let (mut left, mut top, mut right, mut bottom) = (-ring, -ring, ring, ring);
    for label in &arrangement.labels {
        left = left.min(label.x);
        top = top.min(label.y);
        right = right.max(label.x + label.width);
        bottom = bottom.max(label.y + label.height);
    }
    (
        right - left + 2. * STAGE_MARGIN,
        bottom - top + 2. * STAGE_MARGIN,
        (left - STAGE_MARGIN, top - STAGE_MARGIN),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Daniel's laptop screen in logical pixels, less the footer and gaps.
    const LAPTOP: (f64, f64) = (1920. - SCREEN_RESERVE.0, 1200. - SCREEN_RESERVE.1);

    /// Mixed one- to three-line names of varied width.
    fn sizes(count: usize) -> Vec<(f64, f64)> {
        (0..count)
            .map(|i| (120. + (i * 37 % 180) as f64, 38. + (i % 3) as f64 * 17.))
            .collect()
    }

    #[test]
    fn the_first_session_sits_at_one_oclock_and_the_rest_follow_clockwise() {
        assert_eq!(clock_angle(0, 1), 30.);
        assert_eq!(
            (0..6).map(|i| clock_angle(i, 6)).collect::<Vec<_>>(),
            [30., 90., 150., 210., 270., 330.]
        );
        let twenty: Vec<f64> = (0..20).map(|i| clock_angle(i, 20)).collect();
        assert_eq!(twenty[0], 30.);
        assert_eq!(twenty[1], 48.);
        assert_eq!(twenty[19], 12.);
        let arrangement = arrange(&sizes(6), HUB_MIN_RADIUS, LAPTOP);
        let sides: Vec<bool> = arrangement.labels.iter().map(|l| l.right).collect();
        assert_eq!(sides, [true, true, true, false, false, false]);
    }

    #[test]
    fn names_never_overlap_each_other_the_ring_or_the_hub() {
        for count in 1..=KEYS.len() {
            for hub in [HUB_MIN_RADIUS, 118.] {
                let arrangement = arrange(&sizes(count), hub, LAPTOP);
                assert!(arrangement.radius >= hub + HUB_CLEARANCE);
                let reach = arrangement.radius + RING_GAP;
                for (index, label) in arrangement.labels.iter().enumerate() {
                    // Each name sits on its own mark's side of the twelve-to-six axis.
                    assert_eq!(label.right, label.angle < 180., "{count}: {index} side");
                    // The nearest point of every box stays outside the ring.
                    let dx = 0_f64.clamp(label.x, label.x + label.width);
                    let dy = 0_f64.clamp(label.y, label.y + label.height);
                    assert!(dx.hypot(dy) >= reach - 0.01, "{count}: {index} inside ring");
                    // The level run from the elbow points away from the ring.
                    let [(elbow_x, _), _] =
                        leader(label.angle, arrangement.radius, label.y + label.height / 2.);
                    let edge = if label.right {
                        label.x
                    } else {
                        -(label.x + label.width)
                    };
                    assert!(edge > elbow_x.abs(), "{count}: {index} leader turns back");
                    for other in &arrangement.labels[index + 1..] {
                        let apart = label.y + label.height + STACK_GAP <= other.y + 0.01
                            || other.y + other.height + STACK_GAP <= label.y + 0.01
                            || label.right != other.right;
                        assert!(apart, "{count}: overlapping names");
                    }
                }
                // Down the right and up the left, names follow their marks clockwise.
                for side in [true, false] {
                    let mut column: Vec<&Placement> = arrangement
                        .labels
                        .iter()
                        .filter(|l| l.right == side)
                        .collect();
                    column.sort_by(|a, b| a.angle.total_cmp(&b.angle));
                    let ordered = column.windows(2).all(|w| {
                        if side {
                            w[0].y < w[1].y
                        } else {
                            w[0].y > w[1].y
                        }
                    });
                    assert!(ordered, "{count}: clockwise order lost");
                }
            }
        }
    }

    #[test]
    fn every_name_sits_beside_its_own_mark_up_to_twenty_six_sessions() {
        let one_line = |count| vec![(180., 30.); count];
        // Mostly short titles with a long legacy name wrapping to two lines every fourth.
        let mixed = |count: usize| -> Vec<(f64, f64)> {
            (0..count)
                .map(|i| if i % 4 == 3 { (340., 47.) } else { (180., 30.) })
                .collect()
        };
        for count in 1..=KEYS.len() {
            for arrangement in [
                arrange(&one_line(count), HUB_MIN_RADIUS, LAPTOP),
                arrange(&mixed(count), HUB_MIN_RADIUS, LAPTOP),
            ] {
                assert!(
                    slip(&arrangement) <= MAX_SLIP,
                    "{count}: {}",
                    slip(&arrangement)
                );
                let (width, height, _) = stage_bounds(&arrangement);
                assert!(width <= LAPTOP.0 && height <= LAPTOP.1, "{count}");
                // Every leader ends in a level run at its name's centre line, never changes
                // vertical direction, and keeps clear of the ring after leaving its mark.
                let radius = arrangement.radius;
                for label in &arrangement.labels {
                    let level = label.y + label.height / 2.;
                    let [turn, foot] = leader(label.angle, radius, level);
                    assert_eq!(foot.1, level);
                    let angle = label.angle.to_radians();
                    let mark = (radius * angle.sin(), -radius * angle.cos());
                    let anchor = (
                        if label.right {
                            label.x
                        } else {
                            label.x + label.width
                        },
                        level,
                    );
                    let steps = [mark.1, turn.1, foot.1, anchor.1];
                    let rising = steps.windows(2).all(|w| w[1] <= w[0] + 0.01);
                    let falling = steps.windows(2).all(|w| w[1] >= w[0] - 0.01);
                    assert!(rising || falling, "{count}: leader doubles back");
                    for (from, to) in [(turn, foot), (foot, anchor)] {
                        for step in 0..=20 {
                            let t = f64::from(step) / 20.;
                            let (x, y) =
                                (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
                            assert!(x.hypot(y) >= radius + 2., "{count}: leader touches ring");
                        }
                    }
                }
            }
        }
        for count in 1..=20 {
            let arrangement = arrange(&one_line(count), HUB_MIN_RADIUS, LAPTOP);
            assert!(
                slip(&arrangement) <= MAX_SLIP,
                "{count}: {}",
                slip(&arrangement)
            );
            let (width, height, _) = stage_bounds(&arrangement);
            assert!(width <= LAPTOP.0 && height <= LAPTOP.1, "{count}");
        }
        // Twenty: the last session, just past twelve, is named on the right; the tenth,
        // just past six, on the left.
        let twenty = arrange(&one_line(20), HUB_MIN_RADIUS, LAPTOP);
        assert!(twenty.labels[19].right && !twenty.labels[9].right);
    }

    #[test]
    fn small_wheels_stay_compact_and_large_ones_fit_the_screen() {
        let one = arrange(&sizes(1), HUB_MIN_RADIUS, LAPTOP);
        assert_eq!(one.radius, HUB_MIN_RADIUS + HUB_CLEARANCE);
        for count in [20, KEYS.len()] {
            let many = arrange(&sizes(count), HUB_MIN_RADIUS, LAPTOP);
            assert!(many.radius > one.radius);
            let (width, height, _) = stage_bounds(&many);
            assert!(
                width <= LAPTOP.0 && height <= LAPTOP.1,
                "{count}: {width}x{height}"
            );
        }
    }
}
