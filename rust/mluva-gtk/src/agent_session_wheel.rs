//! Native, keyboard-first session wheel; routing remains in the application owner.
//!
//! The wheel is a dial drawn in a few thin lines over the desktop: one tick per session on a
//! hairline ring, a hand that turns to the chosen one and a soft light that follows it. Sessions
//! sit clockwise from one o'clock in discovery order: Herdr's panes, then sessions outside Herdr in
//! the order they started. Each full name stands just outside the ring on its tick's bearing, an
//! uppercase key first; nothing is shortened, and the dial shrinks, never the names, when the
//! screen is small. The wheel reads only the target model: what a session is, its state and how a
//! recording reaches it.
//!
//! Geometry depends only on the number of sessions and on their names, never on status captions
//! or on which session is chosen, so a refresh that changes a status moves nothing.
use adw::prelude::*;
use mluva_core::agent_voice::{AgentKind, AgentStatus, Delivery, Target};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

mod dial;
mod palette;
mod stage;

use dial::{Face, Kind, halo_radius};
use stage::{Area, Hit, Scene, Stage};

/// Selection keys clockwise from one o'clock: the home row, then the top and bottom rows.
pub const KEYS: [char; 26] = [
    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', 'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p',
    'z', 'x', 'c', 'v', 'b', 'n', 'm',
];
/// The first session in Herdr order sits at one o'clock.
const START_DEGREES: f64 = 30.;
/// Where a name's anchor lies beyond the ring's halo, and the least a name may come to it.
const NAME_GAP: f64 = 16.;
const NAME_CLEARANCE: f64 = 8.;
const STACK_GAP: f64 = 4.;
/// Boxes closer than this to touching count as clear of each other.
const EPSILON: f64 = 1e-6;
const STAGE_MARGIN: f64 = 28.;
/// The smallest dial; a smaller screen than this fits is out of scope for a wheel.
const MIN_DIAL: f64 = 120.;
/// Name measures in characters, tried in turn until the wheel fits the screen. Most titles,
/// with room for a caption after them, fit one comfortable line, and a long legacy name wraps to
/// two lines rather than three, so it stays near its tick; wider lines trade width for height,
/// then narrower ones the reverse.
const NAME_MEASURES: [i32; 5] = [58, 72, 44, 34, 26];
/// Room kept for the legend and the compositor's gaps around the dialog.
const SCREEN_RESERVE: (f64, f64) = (96., 168.);
/// The furthest a key may sit from its own tick, centre to the tick's outer end, in pixels; a
/// one-line name's key sits about 40 px out.
pub const KEY_REACH: f64 = 72.;
/// The longest status caption, in characters, which every name reserves so that a status change
/// can never resize a name.
const CAPTION_CHARS: i32 = 10;
/// Room between a key and its name: two cells of 14 px JetBrains Mono.
const KEY_GAP: i32 = 17;
/// The hub's width, room for its widest text, `F12 → draft only` or `choose a session`, at 16 px.
const HUB_WIDTH: i32 = 190;
/// The hub and the status line cross-fade over this many milliseconds when their text changes.
const HUB_FADE_MS: u32 = 160;
/// The status line's width in characters: the longest delivery hint fits on one line.
const HINT_CHARS: i32 = 68;

/// A name's box beside the dial, relative to the ring centre; y grows downward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Clockwise from twelve o'clock, in degrees.
    pub angle: f64,
    /// Right-hand names grow rightward from the dial; left-hand names mirror them.
    pub right: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// The outer end of the name's tick on the ring.
    pub tick: (f64, f64),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
    /// The dial's radius after fitting the screen.
    pub dial: f64,
    pub labels: Vec<Placement>,
    /// False when even the smallest dial leaves the names beyond the room.
    pub fits: bool,
}

/// Clockwise degrees from twelve o'clock for the session at `index` of `count`.
pub fn clock_angle(index: usize, count: usize) -> f64 {
    (START_DEGREES + 360. * index as f64 / count.max(1) as f64).rem_euclid(360.)
}

/// The ring radius a count of sessions asks for: small for a few, large for twenty.
pub fn preferred_dial(count: usize) -> f64 {
    match count {
        0..=1 => 150.,
        2..=6 => 166. + count as f64 * 8.,
        // It grows up to twenty sessions, then holds.
        _ => 214. + (count.min(20) as f64 - 6.) * 5.4,
    }
}

/// Place names of the given sizes clockwise from one o'clock around a dial that fits `room`.
///
/// Each name stands on its own tick's bearing, its nearest edge on the anchor point just
/// beyond the ring. Names that would touch are pushed apart toward their pole, the one nearest
/// the equator keeping its place, and the dial shrinks until the whole wheel fits `room`.
pub fn arrange(sizes: &[(f64, f64)], room: (f64, f64)) -> Arrangement {
    let mut dial = preferred_dial(sizes.len());
    loop {
        let labels = layout(sizes, dial);
        let (width, height, _) = stage_bounds(dial, &labels);
        let fits = width <= room.0 && height <= room.1;
        if fits || dial <= MIN_DIAL {
            return Arrangement { dial, labels, fits };
        }
        dial = (dial * 0.95).max(MIN_DIAL);
    }
}

fn layout(sizes: &[(f64, f64)], dial: f64) -> Vec<Placement> {
    let count = sizes.len();
    let halo = halo_radius(dial);
    let reach = halo + NAME_GAP;
    let clear = halo + NAME_CLEARANCE;
    // A name's first line, which holds its key, is as tall as the shortest name.
    let line = sizes
        .iter()
        .map(|(_, height)| *height)
        .fold(f64::MAX, f64::min);
    let mut labels: Vec<Placement> = (0..count)
        .map(|i| {
            let angle = clock_angle(i, count);
            let (px, py) = dial::point(angle, reach);
            let (width, height) = sizes[i];
            // The box hangs off its anchor on the side away from the dial: wholly to the right
            // on the right and wholly to the left on the left, even beside the poles, so every
            // key sits at its own tick's bearing; and its first line wholly above or below the
            // anchor near the top and bottom. A name that wraps grows toward the equator, where
            // the ticks are further apart, instead of pushing its neighbours toward the pole:
            // above the equator its further lines hang below its first, which stays where a
            // one-line name's would; below the equator it rises so that its last line ends where
            // a one-line name's would.
            let right = angle < 180.;
            let upright = (angle.to_radians().cos() * 2.6).clamp(-1., 1.);
            let more = (height - line).max(0.);
            Placement {
                angle,
                right,
                x: if right { px } else { px - width },
                y: py - (1. + upright) / 2. * line - (1. - upright) / 2. * more,
                width,
                height,
                tick: dial::point(angle, dial),
            }
        })
        .collect();
    // How far from the centre a name at this height must start to keep clear of the ring.
    let needed = |label: &Placement| {
        let nearest = if label.y <= 0. && label.y + label.height >= 0. {
            0.
        } else {
            label.y.abs().min((label.y + label.height).abs())
        };
        (clear * clear - nearest * nearest).max(0.).sqrt()
    };
    // Where each name would start sideways if nothing were in its way.
    let anchors: Vec<f64> = labels.iter().map(|label| label.x).collect();
    // Keep every name clear of the ring, sliding it outward along its own side. This comes
    // first: pushing a name poleward later only moves it further from the ring.
    for label in &mut labels {
        let needed = needed(label);
        if label.right {
            label.x = label.x.max(needed);
        } else {
            label.x = label.x.min(-needed - label.width);
        }
    }
    // Start from the equator and move outward, so a name that must give way goes poleward.
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|a, b| {
        let height = |i: &usize| dial::point(labels[*i].angle, reach).1.abs();
        height(a).total_cmp(&height(b))
    });
    let mut placed: Vec<usize> = Vec::with_capacity(count);
    for index in order {
        let poleward = if labels[index].y + labels[index].height / 2. < 0. {
            -1.
        } else {
            1.
        };
        // Every pass moves the name strictly poleward past one name, so this ends.
        while let Some(other) = placed.iter().map(|j| labels[*j]).find(|other| {
            let me = labels[index];
            me.x < other.x + other.width - EPSILON
                && other.x < me.x + me.width - EPSILON
                && me.y < other.y + other.height + STACK_GAP - EPSILON
                && other.y < me.y + me.height + STACK_GAP - EPSILON
        }) {
            labels[index].y = if poleward < 0. {
                other.y - STACK_GAP - labels[index].height
            } else {
                other.y + other.height + STACK_GAP
            };
        }
        placed.push(index);
    }
    // A name pushed poleward may now clear the ring nearer its anchor: slide it back toward the
    // anchor, sideways only, as far as the ring and every other name allow, so its key stays near
    // its tick.
    let overlaps = |me: &Placement, other: &Placement| {
        me.x < other.x + other.width - EPSILON
            && other.x < me.x + me.width - EPSILON
            && me.y < other.y + other.height + STACK_GAP - EPSILON
            && other.y < me.y + me.height + STACK_GAP - EPSILON
    };
    for index in 0..count {
        let label = labels[index];
        let home = if label.right {
            anchors[index].max(needed(&label))
        } else {
            anchors[index].min(-needed(&label) - label.width)
        };
        if (home - label.x).abs() < EPSILON {
            continue;
        }
        let fits = |x: f64| {
            let moved = Placement { x, ..label };
            labels
                .iter()
                .enumerate()
                .all(|(other, them)| other == index || !overlaps(&moved, them))
        };
        // The nearest free position between where the name is and its home.
        let (mut free, mut blocked) = (label.x, home);
        if fits(home) {
            free = home;
        } else {
            for _ in 0..24 {
                let middle = (free + blocked) / 2.;
                if fits(middle) {
                    free = middle;
                } else {
                    blocked = middle;
                }
            }
        }
        labels[index].x = free;
    }
    labels
}

#[derive(Clone, Copy, PartialEq)]
enum Availability {
    /// Recording reaches the session now, at this level.
    Ready(Delivery),
    /// As Ready, while the agent is in a turn.
    Working(Delivery),
    /// As Ready, while the session waits for the person; the message waits behind the prompt
    /// and never answers it.
    Waiting(Delivery),
    Blocked,
    Unavailable,
    /// Present, but nothing can deliver to it: recording keeps an editable draft in Mluva.
    Draft,
}
impl Availability {
    fn of(entry: &Target) -> Self {
        if entry.draft_only() {
            return Self::Draft;
        }
        match (entry.can_receive(), &entry.status) {
            (true, AgentStatus::Working) => Self::Working(entry.delivery()),
            (true, AgentStatus::Blocked) => Self::Waiting(entry.delivery()),
            (true, _) => Self::Ready(entry.delivery()),
            (false, AgentStatus::Blocked) => Self::Blocked,
            (false, _) => Self::Unavailable,
        }
    }
    fn title(self) -> &'static str {
        match self {
            // Waiting for the person matters more than how the message will wait.
            Self::Ready(delivery) | Self::Working(delivery) if delivery != Delivery::Send => {
                delivery.verb()
            }
            Self::Ready(_) => "Ready",
            Self::Working(_) => "Working",
            Self::Waiting(_) => "Waiting",
            Self::Blocked => "Blocked",
            Self::Unavailable => "Gone",
            Self::Draft => "Draft only",
        }
    }
    /// The caption under a name: a state or a delivery level worth knowing, never a plain send
    /// to an idle session. A level other than Send outranks `working`: it already says the
    /// message waits for the agent.
    fn caption(self) -> Option<&'static str> {
        (self != Self::Ready(Delivery::Send)).then(|| self.title())
    }
    fn warns(self) -> bool {
        matches!(self, Self::Blocked | Self::Unavailable)
    }
    /// How the session's tick is drawn on the dial.
    fn kind(self) -> Kind {
        match self {
            Self::Ready(_) => Kind::Ready,
            Self::Working(_) => Kind::Working,
            Self::Waiting(_) | Self::Blocked => Kind::Blocked,
            Self::Unavailable => Kind::Gone,
            Self::Draft => Kind::Draft,
        }
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

/// A flat legend entry: its key, then what it does.
fn legend_item(key: &gtk::Label, text: &str) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    content.append(key);
    content.append(&label(text, &["ml-wheel-legend-text"]));
    gtk::Button::builder()
        .child(&content)
        .css_classes(["ml-wheel-legend"])
        .build()
}

fn legend_key(text: &str) -> gtk::Label {
    label(text, &["ml-wheel-legend-key"])
}

/// What follows a name to tell it apart, if anything. A shared name gets the session's pane or
/// handle, led by its product when the sessions sharing it are different agents; a desktop app
/// gets its product, which says where it lives.
fn tag(entry: &Target, sharing: Option<&HashSet<AgentKind>>) -> Option<String> {
    match sharing {
        Some(kinds) if kinds.len() > 1 => {
            Some(format!("{} {}", entry.kind.label(), entry.locator()))
        }
        Some(_) => Some(entry.locator()),
        None if entry.kind.is_desktop() => Some(entry.kind.label().to_owned()),
        None => None,
    }
}

fn natural_size(widget: &impl IsA<gtk::Widget>) -> (f64, f64) {
    let (_, width, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
    let (_, height, _, _) = widget.measure(gtk::Orientation::Vertical, width);
    (f64::from(width), f64::from(height))
}

/// One session's name: its uppercase key beside the full title, with any tag and state caption
/// flowing after the title's last word.
struct Slot {
    button: gtk::Button,
    body: gtk::Box,
    key: gtk::Label,
    name: gtk::Label,
    /// The title and tag as markup.
    title: RefCell<String>,
    /// The caption as markup, if any.
    caption: RefCell<Option<String>>,
    /// The markup last set, so an unchanged name is not laid out again.
    markup: RefCell<String>,
}

/// A caption in markup: small and quiet, or in the warning colour. A no-break space keeps
/// `draft only` one unbreakable word.
fn caption_markup(text: &str, warning: Option<&str>) -> String {
    let text = glib::markup_escape_text(&text.replace(' ', "\u{a0}"));
    match warning {
        Some(colour) => format!("<span size=\"small\" foreground=\"{colour}\">{text}</span>"),
        None => format!("<span size=\"small\" alpha=\"80%\">{text}</span>"),
    }
}

/// Type attributes for a name: it wraps at spaces, or inside a long unbroken token, and is never
/// ellipsized or hyphenated. The chosen name keeps the same weight, since a change of weight
/// cannot ease; it is carried by brightness, which can.
fn type_attributes() -> gtk::pango::AttrList {
    let attributes = gtk::pango::AttrList::new();
    attributes.insert(gtk::pango::AttrInt::new_insert_hyphens(false));
    attributes
}

impl Slot {
    fn new() -> Self {
        let key = label("", &["ml-wheel-key"]);
        key.set_valign(gtk::Align::Baseline);
        let name = gtk::Label::builder()
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .attributes(&type_attributes())
            .css_classes(["ml-wheel-name"])
            .build();
        name.set_valign(gtk::Align::Baseline);
        // The name takes the room reserved for the longest caption, so the key stays at the
        // dial's end of the box whatever caption follows the name.
        name.set_hexpand(true);
        // Two cells between the key and the name, so the key reads as its own column.
        let body = gtk::Box::new(gtk::Orientation::Horizontal, KEY_GAP);
        body.set_baseline_position(gtk::BaselinePosition::Top);
        body.append(&key);
        body.append(&name);
        let button = gtk::Button::builder()
            .child(&body)
            .css_classes(["ml-wheel-slot"])
            .visible(false)
            .build();
        Self {
            button,
            body,
            key,
            name,
            title: RefCell::new(String::new()),
            caption: RefCell::new(None),
            markup: RefCell::new(String::new()),
        }
    }
    /// The name's markup with `caption` after the title, if any. The caption joins the last line
    /// of the title and never moves a word of it, because lines break greedily.
    fn render(&self, caption: Option<&str>) {
        let markup = match caption {
            Some(caption) => format!("{}  {caption}", self.title.borrow()),
            None => self.title.borrow().clone(),
        };
        if *self.markup.borrow() != markup {
            self.name.set_markup(&markup);
            self.markup.replace(markup);
        }
    }
    fn show(
        &self,
        letter: char,
        entry: &Target,
        selected: bool,
        sharing: Option<&HashSet<AgentKind>>,
        warning: &str,
    ) {
        let availability = Availability::of(entry);
        let name = entry.display_name();
        let note = availability.caption().map(str::to_lowercase);
        // Names lead; a tag appears only to tell a name apart, a caption only for state or a
        // delivery level other than a plain send.
        let place = entry.locator();
        let tag = tag(entry, sharing).filter(|tag| tag != name);
        let mut title = glib::markup_escape_text(name).to_string();
        if let Some(tag) = &tag {
            title = format!(
                "{title}  <span size=\"small\" alpha=\"70%\">{}</span>",
                glib::markup_escape_text(tag)
            );
        }
        self.title.replace(title);
        // A caption is amber when the session needs the person, as its tick is.
        let amber = availability.warns() || availability.kind() == Kind::Blocked;
        self.caption.replace(
            note.as_deref()
                .map(|note| caption_markup(note, amber.then_some(warning))),
        );
        self.render(self.caption.borrow().as_deref());
        self.key.set_label(&letter.to_ascii_uppercase().to_string());
        toggle_class(&self.button, "ml-wheel-warn", availability.warns());
        // A session Record cannot reach rests dimmer, unless it is the choice; the colour eases.
        toggle_class(
            &self.button,
            "ml-wheel-dim",
            availability.warns() && !selected,
        );
        toggle_class(&self.button, "ml-wheel-selected", selected);
        self.button.set_tooltip_text(Some(&format!(
            "{name}\n{}\n{}",
            entry.whereabouts(),
            what_recording_does(entry)
        )));
        self.button
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "{} · {name} · {place}{}",
                letter.to_ascii_uppercase(),
                note.map(|note| format!(" · {note}")).unwrap_or_default()
            ))]);
        self.button.set_visible(true);
    }
    /// Keys face the dial; names read away from it.
    fn face(&self, right: bool) {
        if right {
            self.body.reorder_child_after(&self.name, Some(&self.key));
        } else {
            self.body.reorder_child_after(&self.key, Some(&self.name));
        }
        let (xalign, justify) = if right {
            (0., gtk::Justification::Left)
        } else {
            (1., gtk::Justification::Right)
        };
        self.name.set_xalign(xalign);
        self.name.set_justify(justify);
    }
    /// The size this name takes at a measure with the longest caption, so that no status change
    /// ever changes any size.
    fn measured(&self, measure: i32) -> (f64, f64) {
        self.render(Some(&caption_markup(
            &"x".repeat(CAPTION_CHARS as usize),
            None,
        )));
        self.name.set_max_width_chars(measure);
        self.button.set_size_request(-1, -1);
        let size = natural_size(&self.button);
        self.render(self.caption.borrow().as_deref());
        size
    }
}

/// One line of the hub: the agent key in the light's colour, an arrow and what then happens.
struct HubLine {
    root: gtk::Box,
    key: gtk::Label,
    lead: gtk::Label,
    action: gtk::Label,
}
impl HubLine {
    fn new() -> Self {
        let key = label("", &["ml-wheel-dial-key"]);
        let lead = label("→", &["ml-wheel-dial-lead"]);
        let action = label("", &["ml-wheel-dial-action"]);
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.set_halign(gtk::Align::Center);
        root.add_css_class("ml-wheel-dial-text");
        root.append(&key);
        root.append(&lead);
        root.append(&action);
        Self {
            root,
            key,
            lead,
            action,
        }
    }
}

/// What the hub and the status line say.
#[derive(Clone, Debug, Default, PartialEq)]
struct HubText {
    /// The agent key, when the key goes on to record; empty when Record is withdrawn.
    key: String,
    action: String,
    hint: String,
    warn: bool,
    muted: bool,
}

/// The hub at the dial's centre: the agent key and what then happens to the words, `F12 → send`,
/// or why Record is withdrawn. The status line above the legend explains a level, a draft or a
/// page. Each holds two pages that cross-fade when the text changes, at a fixed size, so a new
/// choice never moves anything and no text snaps.
struct DialText {
    hub: gtk::Stack,
    lines: [HubLine; 2],
    hint: gtk::Stack,
    hints: [gtk::Label; 2],
    shown: RefCell<(usize, HubText)>,
}
impl DialText {
    fn new() -> Self {
        let crossfade = |stack: &gtk::Stack| {
            stack.set_transition_type(gtk::StackTransitionType::Crossfade);
            stack.set_transition_duration(HUB_FADE_MS);
            stack.set_hhomogeneous(true);
            stack.set_vhomogeneous(true);
            stack.set_can_target(false);
        };
        let hub = gtk::Stack::new();
        crossfade(&hub);
        // As wide as the widest hub, so a new text is centred without moving the hub.
        hub.set_size_request(HUB_WIDTH, -1);
        let lines = [HubLine::new(), HubLine::new()];
        for line in &lines {
            hub.add_child(&line.root);
        }
        let hint = gtk::Stack::new();
        crossfade(&hint);
        let hints = [0, 1].map(|_| {
            let label = gtk::Label::builder()
                .label("\u{a0}")
                .width_chars(HINT_CHARS)
                .css_classes(["ml-wheel-hint"])
                .build();
            hint.add_child(&label);
            label
        });
        Self {
            hub,
            lines,
            hint,
            hints,
            shown: RefCell::new((0, HubText::default())),
        }
    }
    fn show(
        &self,
        target: Option<&Target>,
        other_page: Option<usize>,
        empty: bool,
        agent_key: &str,
    ) {
        let key = agent_key.to_uppercase();
        let text = match target {
            None if empty => HubText {
                action: "no sessions".to_owned(),
                // No-break spaces keep each product name on one line.
                hint: "Start Claude\u{a0}Code or Codex".to_owned(),
                ..HubText::default()
            },
            None => HubText {
                action: "choose a session".to_owned(),
                muted: true,
                ..HubText::default()
            },
            Some(target) => match Availability::of(target) {
                // Record is withdrawn; the status line says why and what to do.
                Availability::Blocked => HubText {
                    action: "blocked".to_owned(),
                    hint: "Answer its prompt where it runs, or choose another session".to_owned(),
                    warn: true,
                    ..HubText::default()
                },
                Availability::Unavailable => HubText {
                    action: "gone".to_owned(),
                    hint: "Mluva can't find it now; choose another session".to_owned(),
                    warn: true,
                    ..HubText::default()
                },
                // The hub already says draft only; the status line says why.
                Availability::Draft => HubText {
                    key,
                    action: "draft only".to_owned(),
                    hint: sentence(target.draft_hint()),
                    ..HubText::default()
                },
                Availability::Ready(delivery)
                | Availability::Working(delivery)
                | Availability::Waiting(delivery) => HubText {
                    key,
                    action: delivery.verb().to_lowercase(),
                    // The remembered session stays chosen while another page is shown.
                    hint: other_page
                        .map(|page| format!("Chosen on page {}", page + 1))
                        .or_else(|| target.delivery_hint().map(str::to_owned))
                        .unwrap_or_default(),
                    ..HubText::default()
                },
            },
        };
        let mut shown = self.shown.borrow_mut();
        if shown.1 == text {
            return;
        }
        // Write the new text into the hidden page and fade across to it.
        let next = 1 - shown.0;
        let line = &self.lines[next];
        line.key.set_label(&text.key);
        line.key.set_visible(!text.key.is_empty());
        line.lead.set_visible(!text.key.is_empty());
        line.action.set_label(&text.action);
        toggle_class(&line.root, "ml-wheel-dial-warn", text.warn);
        toggle_class(&line.root, "ml-wheel-dial-muted", text.muted);
        self.hub.set_visible_child(&line.root);
        // Both stacks turn to the same page; an unchanged status line fades into itself.
        let hint = &self.hints[next];
        hint.set_label(if text.hint.is_empty() {
            "\u{a0}"
        } else {
            &text.hint
        });
        self.hint.set_visible_child(hint);
        *shown = (next, text);
    }
}

/// A draft hint without its leading "Draft only: ", as a sentence.
fn sentence(hint: &str) -> String {
    let rest = hint.strip_prefix("Draft only: ").unwrap_or(hint);
    let mut chars = rest.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// What an arrangement depends on: each shown name's markup and the room.
type LayoutKey = (Vec<String>, (f64, f64));

pub struct SessionWheel {
    pub window: adw::Window,
    /// True where the wheel floats over the desktop as a layer-shell overlay, not a window.
    pub overlay: bool,
    pub choices: Vec<gtk::Button>,
    slots: Vec<Slot>,
    stage: Stage,
    dial: DialText,
    pub pager: gtk::Box,
    pub previous: gtk::Button,
    pub next: gtk::Button,
    pub page_label: gtk::Label,
    /// The legend's Record entry; clicking the dial presses it too.
    pub record: gtk::Button,
    pub record_key: gtk::Label,
    pub transcript: gtk::Button,
    pub transcript_key: gtk::Label,
    pub cancel: gtk::Button,
    pub page: Cell<usize>,
    rendered_page: Cell<usize>,
    rendered_keys: RefCell<(String, String)>,
    rendered_room: Cell<(f64, f64)>,
    pub entries: RefCell<Vec<Target>>,
    pub selected: RefCell<Option<Target>>,
    placements: RefCell<Vec<Placement>>,
    /// The last arrangement and what it was measured from: each shown name's markup and the room.
    layout: RefCell<Option<(LayoutKey, Arrangement)>>,
}
impl SessionWheel {
    pub fn new() -> Self {
        install_style();
        let window = adw::Window::builder()
            .title("Choose a session")
            .resizable(false)
            .modal(true)
            .hide_on_close(true)
            .build();
        window.add_css_class("ml-session-wheel");
        // Like Omarchy's launcher: the wheel floats over the desktop behind one flat translucent
        // wash, with no window frame, wherever the compositor supports layer-shell.
        let overlay = crate::layer_shell::overlay(&window, "mluva-session-wheel");
        if overlay {
            // The compositor sizes an overlay to its output; a fixed size would pin it small.
            window.set_resizable(true);
            window.add_css_class("ml-wheel-overlay");
        }

        let stage = Stage::new();
        // Clockwise insertion keeps Tab order the same as the letters.
        let slots: Vec<_> = KEYS.iter().map(|_| Slot::new()).collect();
        for slot in &slots {
            stage.put_name(&slot.button);
        }
        let dial = DialText::new();
        stage.set_dial_text(&dial.hub);

        let record_key = legend_key("");
        let record = legend_item(&record_key, "record");
        let transcript_key = legend_key("");
        let transcript = legend_item(&transcript_key, "transcript only");
        transcript.set_tooltip_text(Some(
            "Keep an editable transcript in Mluva without copying or sending",
        ));
        let cancel = legend_item(&legend_key("Esc"), "cancel");
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
        let legend = gtk::Box::new(gtk::Orientation::Horizontal, 32);
        legend.set_halign(gtk::Align::Center);
        legend.append(&record);
        legend.append(&transcript);
        legend.append(&cancel);
        legend.append(&pager);
        let footer = gtk::Box::new(gtk::Orientation::Vertical, 12);
        footer.add_css_class("ml-wheel-footer");
        footer.append(&dial.hint);
        footer.append(&legend);
        stage.set_footer(&footer);
        window.set_content(Some(&stage));

        let wheel = Self {
            window,
            overlay,
            choices: slots.iter().map(|s| s.button.clone()).collect(),
            slots,
            stage,
            dial,
            pager,
            previous,
            next,
            page_label,
            record,
            record_key,
            transcript,
            transcript_key,
            cancel,
            page: Cell::new(0),
            rendered_page: Cell::new(usize::MAX),
            rendered_keys: RefCell::new((String::new(), String::new())),
            rendered_room: Cell::new((0., 0.)),
            entries: RefCell::new(vec![]),
            selected: RefCell::new(None),
            placements: RefCell::new(vec![]),
            layout: RefCell::new(None),
        };
        wheel.wire_pointer();
        wheel.wire_hover();
        wheel
    }

    /// A click on a tick chooses it; a click elsewhere on the dial records, as the legend's
    /// Record does. The pointer shows a hand over both.
    fn wire_pointer(&self) {
        let click = gtk::GestureClick::new();
        let choices = self.choices.clone();
        let record = self.record.clone();
        let stage = self.stage.clone();
        click.connect_released(move |gesture, _, x, y| match stage.hit(x, y) {
            Hit::Index(index) => {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                choices[index].emit_clicked();
            }
            Hit::Dial => {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                if record.is_sensitive() {
                    record.emit_clicked();
                }
            }
            Hit::Outside => {}
        });
        self.stage.add_controller(click);
        let motion = gtk::EventControllerMotion::new();
        let stage = self.stage.clone();
        motion.connect_motion(move |_, x, y| {
            let over = stage.hit(x, y) != Hit::Outside;
            stage.set_cursor_from_name(over.then_some("pointer"));
        });
        self.stage.add_controller(motion);
        // The dial fades in and its hand sweeps in each time the wheel opens.
        let stage = self.stage.clone();
        self.window.connect_map(move |_| stage.open());
    }

    /// Hovering a name, or focusing it from the keyboard, brightens its tick on the dial.
    fn wire_hover(&self) {
        for (index, choice) in self.choices.iter().enumerate() {
            let stage = self.stage.clone();
            choice.connect_state_flags_changed(move |button, _| {
                let flags = button.state_flags();
                let hovered = flags.contains(gtk::StateFlags::PRELIGHT)
                    || flags.contains(gtk::StateFlags::FOCUS_VISIBLE);
                if hovered {
                    stage.set_hover(Some(index));
                } else if stage.hover() == Some(index) {
                    stage.set_hover(None);
                }
            });
        }
    }

    /// True for a point, in window coordinates, that belongs to a control: a name, the legend or
    /// the dial. Clicks anywhere else dismiss the overlay.
    pub fn on_control(&self, x: f64, y: f64) -> bool {
        let Some(point) = self
            .window
            .compute_point(&self.stage, &gtk::graphene::Point::new(x as f32, y as f32))
        else {
            return false;
        };
        self.stage.hit(f64::from(point.x()), f64::from(point.y())) != Hit::Outside
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
    /// The logical size of the screen showing the wheel, less room for the legend and gaps.
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

    /// Every shown name's box and bearing, relative to the ring centre, for review captures.
    pub fn placements(&self) -> Vec<Placement> {
        self.placements.borrow().clone()
    }

    pub fn update(
        &self,
        entries: &[Target],
        selected: Option<Target>,
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
        // Text halos take the wash's colour: near white on a light theme, near black otherwise.
        toggle_class(&self.window, "ml-wheel-on-light", self.stage.light_theme());
        let previous_page = self.rendered_page.get();
        let same_page = previous_page == self.page.get();
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
        self.record_key.set_label(&agent_key.to_uppercase());
        self.transcript_key
            .set_label(&transcript_key.to_uppercase());
        self.transcript
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "{transcript_key} · Transcript only"
            ))]);

        let position = selected
            .as_ref()
            .and_then(|s| entries.iter().position(|e| e.same_instance(s)));
        let other_page = position.map(|i| i / KEYS.len()).filter(|p| *p != page);
        self.show_record(selected.as_ref(), other_page, entries.is_empty(), agent_key);
        // The products sharing each name, for names that more than one session has.
        let mut names: HashMap<&str, (usize, HashSet<AgentKind>)> = HashMap::new();
        for entry in entries {
            let (count, kinds) = names.entry(entry.display_name()).or_default();
            *count += 1;
            kinds.insert(entry.kind);
        }
        let shown: Vec<&Target> = entries
            .iter()
            .skip(page * KEYS.len())
            .take(KEYS.len())
            .collect();
        let chosen = position
            .filter(|_| other_page.is_none())
            .map(|i| i % KEYS.len());
        let warning = self.stage.warning_hex();
        for (index, slot) in self.slots.iter().enumerate() {
            match shown.get(index) {
                Some(entry) => slot.show(
                    KEYS[index],
                    entry,
                    chosen == Some(index),
                    names
                        .get(entry.display_name())
                        .filter(|(count, _)| *count > 1)
                        .map(|(_, kinds)| kinds),
                    &warning,
                ),
                None => slot.button.set_visible(false),
            }
        }
        // Geometry depends only on the names, their tags and the room, so a new choice or a
        // status change reuses it and the swing's first frame is not held up measuring names.
        let key = (
            self.slots[..shown.len()]
                .iter()
                .map(|slot| slot.title.borrow().clone())
                .collect::<Vec<_>>(),
            room,
        );
        let cached = self
            .layout
            .borrow()
            .as_ref()
            .filter(|(known, _)| *known == key)
            .map(|(_, arrangement)| arrangement.clone());
        let arrangement = cached.unwrap_or_else(|| {
            let arrangement = self.arrange(shown.len());
            self.layout.replace(Some((key, arrangement.clone())));
            arrangement
        });
        let previous_marks = self.stage_marks();
        self.place(&arrangement, &shown, chosen, &selected);
        // Only a choice changing on an unchanged wheel swings; opening or reshaping places.
        let swing = self.window.is_mapped() && same_page && previous_marks == shown.len();
        self.stage
            .aim(chosen.map(|index| clock_angle(index, shown.len())), swing);
        // Paging turns the dial a few degrees instead of swapping it.
        if self.window.is_mapped() && previous_page != usize::MAX && !same_page {
            self.stage.turn(self.page.get() > previous_page);
        }
    }

    /// Advance the wheel's motion by `seconds` on a stepped clock instead of the frame clock, so
    /// review captures can render it frame by frame. Returns the hand's bearing, if it shows.
    pub fn step_motion(&self, seconds: f64) -> Option<f64> {
        self.stage.step(seconds)
    }

    fn stage_marks(&self) -> usize {
        self.placements.borrow().len()
    }

    /// The Record legend and the dial text, from the remembered session.
    fn show_record(
        &self,
        target: Option<&Target>,
        other_page: Option<usize>,
        empty: bool,
        agent_key: &str,
    ) {
        self.dial.show(target, other_page, empty, agent_key);
        let Some(target) = target else {
            self.record.set_sensitive(false);
            self.record.set_tooltip_text(None);
            self.record
                .update_property(&[gtk::accessible::Property::Label("No session selected")]);
            return;
        };
        let availability = Availability::of(target);
        let ready = target.can_receive();
        let draft = availability == Availability::Draft;
        let name = target.display_name();
        let place = target.locator();
        self.record.set_sensitive(ready || draft);
        self.record.set_tooltip_text(Some(&format!(
            "{name}\n{}\n{}",
            target.whereabouts(),
            what_recording_does(target)
        )));
        self.record
            .update_property(&[gtk::accessible::Property::Label(&if ready || draft {
                format!("{agent_key} · {} · {place}", what_recording_does(target))
            } else {
                format!("{name} · {place} · {}", availability.title())
            })]);
    }

    /// The narrowest name measure whose wheel fits the screen, else the widest.
    fn arrange(&self, count: usize) -> Arrangement {
        let room = self.available();
        let mut arrangement = Arrangement::default();
        for measure in NAME_MEASURES {
            let sizes: Vec<(f64, f64)> = self.slots[..count]
                .iter()
                .map(|slot| slot.measured(measure))
                .collect();
            arrangement = arrange(&sizes, room);
            if arrangement.fits {
                break;
            }
        }
        arrangement
    }

    fn place(
        &self,
        arrangement: &Arrangement,
        shown: &[&Target],
        chosen: Option<usize>,
        selected: &Option<Target>,
    ) {
        let (width, height, (left, top)) = stage_bounds(arrangement.dial, &arrangement.labels);
        let marks = shown
            .iter()
            .zip(&arrangement.labels)
            .map(|(entry, placement)| (placement.angle, Availability::of(entry).kind()))
            .collect();
        for ((slot, placement), _) in self.slots.iter().zip(&arrangement.labels).zip(shown) {
            slot.face(placement.right);
            // Fixed allocation: hold each name at the size it was placed at.
            slot.button.set_size_request(
                placement.width.ceil() as i32,
                placement.height.ceil() as i32,
            );
        }
        let warn = selected
            .as_ref()
            .is_some_and(|s| Availability::of(s).warns());
        self.stage.arrange(Scene {
            face: Face {
                dial: arrangement.dial,
                marks,
                chosen,
                warn,
            },
            names: arrangement
                .labels
                .iter()
                .map(|p| Area {
                    x: p.x,
                    y: p.y,
                    width: p.width,
                    height: p.height,
                })
                .collect(),
            bounds: (left, top, left + width, top + height),
        });
        self.placements.replace(arrangement.labels.clone());
    }
}
impl Default for SessionWheel {
    fn default() -> Self {
        Self::new()
    }
}

/// What recording for a session does, in one sentence for tooltips and screen readers.
fn what_recording_does(target: &Target) -> String {
    let name = target.display_name();
    if target.draft_only() {
        return format!("Record a draft for {name} · {}", target.draft_hint());
    }
    match target.delivery() {
        Delivery::Send => format!("Record and send to {name}"),
        Delivery::Queue => format!("Record and queue for {name}"),
        Delivery::Prefill => format!("Record into {name}'s composer; you press Enter"),
        Delivery::DraftOnly => format!("Record a draft for {name}"),
    }
}

/// Stage size and its top-left corner relative to the ring centre, margin included. The dial
/// stays centred, so the stage reaches as far on each side as the names reach on the wider one.
fn stage_bounds(dial: f64, labels: &[Placement]) -> (f64, f64, (f64, f64)) {
    let ring = halo_radius(dial) + 8.;
    let (mut across, mut down) = (ring, ring);
    for label in labels {
        across = across.max(-label.x).max(label.x + label.width);
        down = down.max(-label.y).max(label.y + label.height);
    }
    let (across, down) = (across + STAGE_MARGIN, down + STAGE_MARGIN);
    (2. * across, 2. * down, (-across, -down))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Daniel's laptop screen in logical pixels, less the legend and gaps.
    const LAPTOP: (f64, f64) = (1920. - SCREEN_RESERVE.0, 1200. - SCREEN_RESERVE.1);

    /// Mixed one- to three-line names of varied width.
    fn sizes(count: usize) -> Vec<(f64, f64)> {
        (0..count)
            .map(|i| (120. + (i * 37 % 180) as f64, 38. + (i % 3) as f64 * 17.))
            .collect()
    }

    fn overlap(a: &Placement, b: &Placement) -> bool {
        a.x < b.x + b.width - 0.01
            && b.x < a.x + a.width - 0.01
            && a.y < b.y + b.height - 0.01
            && b.y < a.y + a.height - 0.01
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
        let arrangement = arrange(&sizes(6), LAPTOP);
        let sides: Vec<bool> = arrangement.labels.iter().map(|l| l.right).collect();
        assert_eq!(sides, [true, true, true, false, false, false]);
    }

    #[test]
    fn names_never_overlap_each_other_or_the_ring() {
        for count in 1..=KEYS.len() {
            let arrangement = arrange(&sizes(count), LAPTOP);
            let clear = halo_radius(arrangement.dial) + NAME_CLEARANCE;
            for (index, label) in arrangement.labels.iter().enumerate() {
                // The nearest point of every box stays outside the ring and its halo.
                let dx = 0_f64.clamp(label.x, label.x + label.width);
                let dy = 0_f64.clamp(label.y, label.y + label.height);
                assert!(
                    dx.hypot(dy) >= clear - 0.01,
                    "{count}: {index} inside the ring"
                );
                for other in &arrangement.labels[index + 1..] {
                    assert!(!overlap(label, other), "{count}: overlapping names");
                }
            }
        }
    }

    #[test]
    fn every_name_is_on_screen_and_in_clockwise_order_up_to_twenty_six_sessions() {
        let one_line = |count| vec![(180., 30.); count];
        // Mostly short titles with a long legacy name wrapping to two lines every fourth.
        let mixed = |count: usize| -> Vec<(f64, f64)> {
            (0..count)
                .map(|i| if i % 4 == 3 { (340., 47.) } else { (180., 30.) })
                .collect()
        };
        for count in 1..=KEYS.len() {
            for names in [one_line(count), mixed(count), sizes(count)] {
                let arrangement = arrange(&names, LAPTOP);
                assert!(arrangement.fits, "{count}: does not fit the laptop screen");
                let (width, height, (left, top)) =
                    stage_bounds(arrangement.dial, &arrangement.labels);
                assert!(width <= LAPTOP.0 && height <= LAPTOP.1, "{count}");
                for label in &arrangement.labels {
                    // Inside the stage, which is inside the screen.
                    assert!(label.x >= left && label.x + label.width <= left + width);
                    assert!(label.y >= top && label.y + label.height <= top + height);
                }
                // Down the right and up the left, names follow their indices clockwise.
                for side in [true, false] {
                    let mut column: Vec<&Placement> = arrangement
                        .labels
                        .iter()
                        .filter(|l| l.right == side)
                        .collect();
                    column.sort_by(|a, b| a.angle.total_cmp(&b.angle));
                    let centre = |l: &Placement| l.y + l.height / 2.;
                    let ordered = column.windows(2).all(|w| {
                        if side {
                            centre(w[0]) < centre(w[1])
                        } else {
                            centre(w[0]) > centre(w[1])
                        }
                    });
                    assert!(ordered, "{count}: clockwise order lost");
                }
            }
        }
    }

    #[test]
    fn a_name_stays_near_its_own_index() {
        let one_line = vec![(220., 52.); 20];
        let arrangement = arrange(&one_line, LAPTOP);
        let reach = halo_radius(arrangement.dial) + NAME_GAP;
        for label in &arrangement.labels {
            let (_, anchor_y) = dial::point(label.angle, reach);
            let centre = label.y + label.height / 2.;
            assert!(
                (centre - anchor_y).abs() < 80.,
                "{}° drifts {}",
                label.angle,
                centre - anchor_y
            );
        }
    }

    #[test]
    fn every_key_sits_near_its_own_tick_even_beside_a_wrapped_name() {
        // A stress mix: one-line names, with a long legacy name wrapping to two lines every
        // fourth, up to six of them around one dial.
        let (line, key) = (25., 15.);
        for count in [6, 20, 26] {
            let names: Vec<(f64, f64)> = (0..count)
                .map(|i| {
                    if i % 4 == 3 {
                        (495., 2. * line - 1.)
                    } else {
                        (245., line)
                    }
                })
                .collect();
            let arrangement = arrange(&names, LAPTOP);
            for label in &arrangement.labels {
                // The key is the first glyph cell on the first line, on the dial's side.
                let kx = if label.right {
                    label.x + key / 2.
                } else {
                    label.x + label.width - key / 2.
                };
                let ky = label.y + line / 2.;
                let distance = (kx - label.tick.0).hypot(ky - label.tick.1);
                assert!(
                    distance < KEY_REACH,
                    "{count}: the key at {}° is {distance:.0} px from its tick",
                    label.angle
                );
            }
        }
    }

    #[test]
    fn geometry_depends_only_on_the_count_and_the_names() {
        // The widget measures every name at a fixed caption width and in its chosen style, so a
        // status or a choice changing leaves these inputs, and so the placement, unchanged.
        let names = sizes(20);
        assert_eq!(arrange(&names, LAPTOP), arrange(&names, LAPTOP));
        assert_eq!(CAPTION_CHARS as usize, "DRAFT ONLY".len());
        let every = [Delivery::Send, Delivery::Queue, Delivery::Prefill]
            .into_iter()
            .flat_map(|d| {
                [
                    Availability::Ready(d),
                    Availability::Working(d),
                    Availability::Waiting(d),
                ]
            })
            .chain([
                Availability::Blocked,
                Availability::Unavailable,
                Availability::Draft,
            ]);
        for availability in every {
            let caption = availability.caption().unwrap_or_default();
            assert!(caption.len() <= CAPTION_CHARS as usize, "{caption}");
        }
    }

    #[test]
    fn small_wheels_stay_compact_and_the_dial_shrinks_on_a_small_screen() {
        assert!(preferred_dial(1) < preferred_dial(6));
        assert!(preferred_dial(6) < preferred_dial(20));
        assert_eq!(preferred_dial(26), preferred_dial(20));
        for count in [20, KEYS.len()] {
            let many = arrange(&vec![(200., 30.); count], LAPTOP);
            assert_eq!(many.dial, preferred_dial(count), "{count}");
            let (width, height, _) = stage_bounds(many.dial, &many.labels);
            assert!(width <= LAPTOP.0 && height <= LAPTOP.1, "{count}");
            // A smaller screen keeps every name and gives the dial up instead.
            let small = (1280. - SCREEN_RESERVE.0, 800. - SCREEN_RESERVE.1);
            let fitted = arrange(&vec![(200., 30.); count], small);
            assert!(fitted.fits && fitted.dial < many.dial, "{count}");
        }
    }
    #[test]
    fn arbitrary_names_never_overlap_or_leave_the_screen_when_they_fit() {
        // A deterministic spread of counts and name sizes, narrow to very wide and one to five lines.
        let mut state = 12345_u64;
        let mut next = move |low: f64, high: f64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            low + (state % 10_000) as f64 / 10_000. * (high - low)
        };
        for _ in 0..400 {
            let count = next(1., 27.) as usize;
            let names: Vec<(f64, f64)> = (0..count)
                .map(|_| (next(60., 420.), next(30., 110.)))
                .collect();
            let arrangement = arrange(&names, LAPTOP);
            for (index, label) in arrangement.labels.iter().enumerate() {
                for other in &arrangement.labels[index + 1..] {
                    assert!(!overlap(label, other), "{count} names overlap");
                }
            }
            if arrangement.fits {
                let (width, height, _) = stage_bounds(arrangement.dial, &arrangement.labels);
                assert!(width <= LAPTOP.0 && height <= LAPTOP.1);
            }
        }
    }
}
