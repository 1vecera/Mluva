//! Native, keyboard-first session wheel; routing remains in the application owner.
use adw::prelude::*;
use mluva_core::agent_voice::{AgentStatus, Recipient};
use std::cell::{Cell, RefCell};
use std::f64::consts::TAU;
use std::rc::Rc;

pub const KEYS: [char; 6] = ['a', 's', 'd', 'j', 'k', 'l'];
/// Clockwise key positions in degrees from twelve o'clock. Left-hand keys rise
/// up the left side and right-hand keys descend the right, so the home row
/// reads as one arch around the hub.
const ANGLES: [f64; 6] = [210., 270., 330., 30., 90., 150.];
// Keys sit far enough out that a diagonal pill corner clears the needle by 8px.
const HUB_RADIUS: i32 = 78;
const TRACK_RADIUS: f64 = 88.;
const KEY_RADIUS: f64 = 124.;
const KEY_SIZE: i32 = 30;
/// Slot border and padding between a slot edge and its key.
const KEY_INSET: f64 = 6.;
const SLOT_WIDTH: i32 = 200;
const SLOT_HEIGHT: i32 = 68;
const DIAL_WIDTH: i32 = 612;
const DIAL_HEIGHT: i32 = 284;
pub const WIDTH: i32 = DIAL_WIDTH + 40;
pub const HEIGHT: i32 = 430;

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

/// Session names break at spaces or hyphens, never mid-word with an inserted hyphen.
fn name_label(max_chars: i32, xalign: f32, justify: gtk::Justification, class: &str) -> gtk::Label {
    let attributes = gtk::pango::AttrList::new();
    attributes.insert(gtk::pango::AttrInt::new_insert_hyphens(false));
    gtk::Label::builder()
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::Word)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .lines(2)
        .max_width_chars(max_chars)
        .xalign(xalign)
        .justify(justify)
        .attributes(&attributes)
        .css_classes([class])
        .build()
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

fn key_hint(key: &gtk::Label, text: &str) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    content.append(key);
    content.append(&label(text, &["ml-wheel-button-label"]));
    gtk::Button::builder().child(&content).build()
}

/// A point on a ring around the dial centre, measured clockwise from twelve o'clock.
fn ring_point(degrees: f64, radius: f64) -> (f64, f64) {
    let angle = degrees.to_radians();
    (
        f64::from(DIAL_WIDTH) / 2. + radius * angle.sin(),
        f64::from(DIAL_HEIGHT) / 2. - radius * angle.cos(),
    )
}

fn source(context: &gtk::cairo::Context, color: gtk::gdk::RGBA, alpha: f64) {
    context.set_source_rgba(
        color.red().into(),
        color.green().into(),
        color.blue().into(),
        alpha,
    );
}

fn canvas(class: &str) -> gtk::DrawingArea {
    gtk::DrawingArea::builder()
        .content_width(DIAL_WIDTH)
        .content_height(DIAL_HEIGHT)
        .can_target(false)
        .css_classes([class])
        .build()
}

/// The faint track with one tick per key; occupied keys get stronger ticks.
fn dial(occupied: Rc<Cell<usize>>) -> gtk::DrawingArea {
    let dial = canvas("ml-wheel-dial");
    dial.set_draw_func(move |area, context, _, _| {
        let ink = area.color();
        let (x, y) = ring_point(0., 0.);
        context.set_line_width(1.5);
        source(context, ink, 0.13);
        context.arc(x, y, TRACK_RADIUS, 0., TAU);
        let _ = context.stroke();
        for (slot, degrees) in ANGLES.iter().enumerate() {
            let (x, y) = ring_point(*degrees, TRACK_RADIUS);
            source(
                context,
                ink,
                if slot < occupied.get() { 0.42 } else { 0.16 },
            );
            context.arc(x, y, 2., 0., TAU);
            let _ = context.fill();
        }
    });
    dial
}

/// The accent arc on the track that points from the hub to the selected key.
struct Needle {
    area: gtk::DrawingArea,
    angle: Rc<Cell<f64>>,
    motion: adw::TimedAnimation,
}
impl Needle {
    fn new() -> Self {
        let area = canvas("ml-wheel-needle");
        let angle = Rc::new(Cell::new(f64::NAN));
        let drawn = angle.clone();
        area.set_draw_func(move |area, context, _, _| {
            if drawn.get().is_nan() {
                return;
            }
            let (x, y) = ring_point(0., 0.);
            let middle = (drawn.get() - 90.).to_radians();
            let spread = 19_f64.to_radians();
            source(context, area.color(), 1.);
            context.set_line_width(3.);
            context.set_line_cap(gtk::cairo::LineCap::Round);
            context.arc(x, y, TRACK_RADIUS, middle - spread, middle + spread);
            let _ = context.stroke();
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
            .duration(220)
            .easing(adw::Easing::EaseOutCubic)
            .target(&target)
            .build();
        Self {
            area,
            angle,
            motion,
        }
    }
    /// Swing the shortest way to a new key; hide or place without motion otherwise.
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

struct Slot {
    button: gtk::Button,
    key: gtk::Label,
    name: gtk::Label,
    pane: gtk::Label,
    state: gtk::Label,
    ghost: gtk::Label,
}
impl Slot {
    /// Anchor the key on its ring position; the name grows outward from the hub.
    fn new(index: usize, wheel: &gtk::Fixed) -> Self {
        let left = ANGLES[index] > 180.;
        let outward = if left {
            gtk::Align::End
        } else {
            gtk::Align::Start
        };
        let (x, y) = ring_point(ANGLES[index], KEY_RADIUS);
        let letter = KEYS[index].to_ascii_uppercase().to_string();
        let ghost = label(&letter, &["ml-wheel-key", "ml-wheel-ghost"]);
        ghost.update_state(&[gtk::accessible::State::Hidden(true)]);
        ghost.set_size_request(KEY_SIZE, KEY_SIZE);
        let half = f64::from(KEY_SIZE) / 2.;
        wheel.put(&ghost, x - half, y - half);
        let key = label(&letter, &["ml-wheel-key"]);
        key.set_size_request(KEY_SIZE, KEY_SIZE);
        key.set_valign(gtk::Align::Center);
        let name = name_label(
            18,
            if left { 1. } else { 0. },
            if left {
                gtk::Justification::Right
            } else {
                gtk::Justification::Left
            },
            "ml-wheel-name",
        );
        name.set_halign(outward);
        let pane = label("", &["ml-wheel-meta"]);
        let state = label("", &["ml-wheel-meta"]);
        let meta = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        meta.set_halign(outward);
        meta.append(&pane);
        meta.append(&state);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_valign(gtk::Align::Center);
        text.append(&name);
        text.append(&meta);
        let body = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        if left {
            body.append(&text);
            body.append(&key);
        } else {
            body.append(&key);
            body.append(&text);
        }
        let button = gtk::Button::builder()
            .child(&body)
            .halign(outward)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .css_classes([
                "ml-wheel-slot",
                if left {
                    "ml-wheel-left"
                } else {
                    "ml-wheel-right"
                },
            ])
            .build();
        let frame = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        frame.set_size_request(SLOT_WIDTH, SLOT_HEIGHT);
        frame.append(&button);
        let edge = half + KEY_INSET;
        let frame_x = if left {
            x + edge - f64::from(SLOT_WIDTH)
        } else {
            x - edge
        };
        wheel.put(&frame, frame_x, y - f64::from(SLOT_HEIGHT) / 2.);
        Self {
            button,
            key,
            name,
            pane,
            state,
            ghost,
        }
    }
    fn show(&self, index: usize, entry: Option<&Recipient>, selected: bool) {
        self.button.set_visible(entry.is_some());
        self.ghost.set_visible(entry.is_none());
        let Some(entry) = entry else {
            return;
        };
        let letter = KEYS[index].to_ascii_uppercase();
        let availability = Availability::of(entry);
        let note =
            (availability != Availability::Ready).then(|| availability.title().to_lowercase());
        self.key.set_label(&letter.to_string());
        self.name.set_label(&entry.name);
        self.pane.set_label(&entry.pane_id);
        self.state.set_label(
            &note
                .as_ref()
                .map(|note| format!("· {note}"))
                .unwrap_or_default(),
        );
        self.state.set_visible(note.is_some());
        toggle_class(&self.state, "ml-wheel-warning", availability.warns());
        toggle_class(&self.button, "ml-wheel-unavailable", availability.warns());
        toggle_class(&self.button, "ml-wheel-selected", selected);
        let project = std::path::Path::new(&entry.project)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Local");
        self.button.set_tooltip_text(Some(&format!(
            "{} · {} · {} · {project}",
            entry.name, entry.pane_id, entry.kind
        )));
        self.button
            .update_property(&[gtk::accessible::Property::Label(&format!(
                "{letter} · {} · {}{}",
                entry.name,
                entry.pane_id,
                note.map(|note| format!(" · {note}")).unwrap_or_default()
            ))]);
    }
}

/// The centre is the primary action: the remembered target and its record key.
struct Hub {
    button: gtk::Button,
    key: gtk::Label,
    dot: gtk::Box,
    state: gtk::Label,
    name: gtk::Label,
    pane: gtk::Label,
    action: gtk::Box,
    hint: gtk::Label,
}
impl Hub {
    fn new() -> Self {
        let key = label("", &["ml-wheel-chip"]);
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("ml-wheel-dot");
        dot.set_valign(gtk::Align::Center);
        let state = label("", &["ml-wheel-state"]);
        let state_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        state_row.set_halign(gtk::Align::Center);
        state_row.append(&dot);
        state_row.append(&state);
        let name = name_label(13, 0.5, gtk::Justification::Center, "ml-wheel-hub-name");
        let pane = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(16)
            .css_classes(["ml-wheel-hub-pane"])
            .build();
        let action = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        action.add_css_class("ml-wheel-action");
        action.set_halign(gtk::Align::Center);
        action.set_margin_top(10);
        action.append(&key);
        action.append(&label("Record", &["ml-wheel-action-label"]));
        let hint = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(14)
            .margin_top(8)
            .css_classes(["ml-wheel-hint"])
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
        content.set_valign(gtk::Align::Center);
        content.set_halign(gtk::Align::Center);
        for child in [
            state_row.upcast_ref::<gtk::Widget>(),
            name.upcast_ref(),
            pane.upcast_ref(),
            action.upcast_ref(),
            hint.upcast_ref(),
        ] {
            content.append(child);
        }
        let button = gtk::Button::builder()
            .child(&content)
            .width_request(2 * HUB_RADIUS)
            .height_request(2 * HUB_RADIUS)
            .css_classes(["ml-wheel-hub"])
            .build();
        Self {
            button,
            key,
            dot,
            state,
            name,
            pane,
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
            for widget in [
                self.dot.upcast_ref::<gtk::Widget>(),
                self.state.upcast_ref(),
                self.pane.upcast_ref(),
                self.action.upcast_ref(),
            ] {
                widget.set_visible(false);
            }
            self.button.set_sensitive(false);
            self.name
                .set_label(if empty { "No sessions" } else { "No session" });
            self.hint.set_label(if empty {
                "Start an agent\nin Herdr"
            } else {
                "Press a letter\nto choose"
            });
            self.hint.set_visible(true);
            self.button.set_tooltip_text(None);
            self.button
                .update_property(&[gtk::accessible::Property::Label("No session selected")]);
            return;
        };
        let availability = Availability::of(target);
        let ready = target.can_receive();
        self.button.set_sensitive(ready);
        self.action.set_visible(ready);
        self.dot.set_visible(true);
        self.state.set_visible(true);
        self.state.set_label(availability.title());
        toggle_class(
            &self.dot,
            "ml-wheel-busy",
            availability == Availability::Working,
        );
        toggle_class(&self.dot, "ml-wheel-warning", availability.warns());
        self.name.set_label(&target.name);
        toggle_class(
            &self.name,
            "ml-wheel-long",
            target.name.chars().count() > 16,
        );
        // The remembered target stays visible while another page is shown.
        self.pane.set_label(&match other_page {
            Some(page) => format!("{} · page {}", target.pane_id, page + 1),
            None => target.pane_id.clone(),
        });
        self.pane.set_visible(true);
        self.hint.set_label("Press a letter\nto switch");
        self.hint.set_visible(!ready);
        self.button.set_tooltip_text(Some(&format!(
            "{} · {} · {}",
            target.name, target.pane_id, target.kind
        )));
        self.button
            .update_property(&[gtk::accessible::Property::Label(&if ready {
                format!(
                    "{agent_key} · Record to {} · {}",
                    target.name, target.pane_id
                )
            } else {
                format!(
                    "{} · {} · {}",
                    target.name,
                    target.pane_id,
                    availability.title()
                )
            })]);
    }
}

pub struct SessionWheel {
    pub window: adw::Window,
    pub choices: Vec<gtk::Button>,
    slots: Vec<Slot>,
    dial: gtk::DrawingArea,
    occupied: Rc<Cell<usize>>,
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
    pub entries: RefCell<Vec<Recipient>>,
    pub selected: RefCell<Option<Recipient>>,
}
impl SessionWheel {
    pub fn new() -> Self {
        install_style();
        let window = adw::Window::builder()
            .title("Choose a Herdr session")
            .default_width(WIDTH)
            .default_height(HEIGHT)
            .resizable(false)
            .modal(true)
            .hide_on_close(true)
            .build();
        window.add_css_class("ml-session-wheel");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.set_margin_top(18);
        content.set_margin_bottom(16);
        content.set_margin_start(20);
        content.set_margin_end(20);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        // Reserve the pager's height so paging never shifts the dial.
        header.set_size_request(-1, 28);
        let heading = label("Choose a session", &["ml-wheel-title"]);
        heading.set_hexpand(true);
        heading.set_xalign(0.);
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
        header.append(&heading);
        header.append(&pager);
        content.append(&header);

        let wheel = gtk::Fixed::builder()
            .width_request(DIAL_WIDTH)
            .height_request(DIAL_HEIGHT)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .vexpand(true)
            .build();
        let occupied = Rc::new(Cell::new(0));
        let dial = dial(occupied.clone());
        wheel.put(&dial, 0., 0.);
        let needle = Needle::new();
        wheel.put(&needle.area, 0., 0.);
        let slots: Vec<_> = (0..KEYS.len()).map(|i| Slot::new(i, &wheel)).collect();
        let hub = Hub::new();
        wheel.put(
            &hub.button,
            f64::from(DIAL_WIDTH / 2 - HUB_RADIUS),
            f64::from(DIAL_HEIGHT / 2 - HUB_RADIUS),
        );
        content.append(&wheel);

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        footer.add_css_class("ml-wheel-footer");
        let transcript_key = label("", &["ml-wheel-chip"]);
        let transcript = key_hint(&transcript_key, "Transcript only");
        transcript.set_tooltip_text(Some(
            "Keep an editable transcript in Mluva without copying or sending",
        ));
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        let cancel = key_hint(&label("Esc", &["ml-wheel-chip"]), "Cancel");
        footer.append(&transcript);
        footer.append(&spacer);
        footer.append(&cancel);
        content.append(&footer);
        window.set_content(Some(&content));
        Self {
            window,
            choices: slots.iter().map(|s| s.button.clone()).collect(),
            slots,
            dial,
            occupied,
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
            entries: RefCell::new(vec![]),
            selected: RefCell::new(None),
        }
    }
    pub fn update(
        &self,
        entries: &[Recipient],
        selected: Option<Recipient>,
        agent_key: &str,
        transcript_key: &str,
    ) {
        if self.entries.borrow().as_slice() == entries
            && *self.selected.borrow() == selected
            && self.rendered_page.get() == self.page.get()
            && self.rendered_keys.borrow().0 == agent_key
            && self.rendered_keys.borrow().1 == transcript_key
        {
            return;
        }
        // Only a choice changing on a shown page swings; opening or paging places.
        let swing = self.window.is_mapped() && self.rendered_page.get() == self.page.get();
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
        let shown = entries.iter().skip(page * KEYS.len()).take(KEYS.len());
        self.occupied.set(shown.len());
        self.dial.queue_draw();
        for (index, slot) in self.slots.iter().enumerate() {
            let entry = entries.get(page * KEYS.len() + index);
            let chosen =
                entry.is_some_and(|e| selected.as_ref().is_some_and(|s| s.same_instance(e)));
            slot.show(index, entry, chosen);
        }
        let pointed = position
            .filter(|_| other_page.is_none())
            .map(|i| ANGLES[i % KEYS.len()]);
        let warning = selected.as_ref().is_some_and(|s| !s.can_receive());
        self.needle.point(pointed, warning, swing);
    }
}
impl Default for SessionWheel {
    fn default() -> Self {
        Self::new()
    }
}
