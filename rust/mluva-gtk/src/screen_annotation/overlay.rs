//! One transparent layer per monitor. It never takes the keyboard, lets every pointer event
//! through until Shift is held, and shows ink that fades shortly after release.
use glib::translate::IntoGlib;
use gtk::{cairo, gdk, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

/// Ink stays fully visible this long after release, then fades over `FADE`.
const HOLD: Duration = Duration::from_millis(700);
const FADE: Duration = Duration::from_millis(600);
/// A pin shows as a brief ring; the pin itself stays data.
const PULSE: Duration = Duration::from_millis(450);
const CSS: &str = "window.ml-ink-overlay, window.ml-ink-overlay > * { background: transparent; box-shadow: none; }";

struct Line {
    id: u64,
    points: Vec<(f64, f64)>,
    released: Option<Instant>,
    /// Hidden until its clean frame has been captured.
    held: bool,
}

#[derive(Default)]
struct Painting {
    lines: Vec<Line>,
    pulses: Vec<(f64, f64, Instant)>,
    next: u64,
}
impl Painting {
    fn expire(&mut self, now: Instant) {
        self.lines.retain(|line| {
            line.released
                .is_none_or(|released| now.duration_since(released) < HOLD + FADE)
        });
        self.pulses
            .retain(|(_, _, started)| now.duration_since(*started) < PULSE);
    }
    fn visible(&self) -> bool {
        self.lines.iter().any(|line| !line.held) || !self.pulses.is_empty()
    }
}

/// Pointer motion over one screen, monitor-local logical pixels.
pub type Motion = Rc<dyn Fn(&str, f64, f64)>;

pub struct Overlay {
    pub screen: String,
    window: gtk::Window,
    area: gtk::DrawingArea,
    painting: Rc<RefCell<Painting>>,
    drawing: Cell<bool>,
    ticking: Rc<Cell<bool>>,
    /// Where the pointer last moved over this overlay, monitor-local.
    pub pointer: Cell<Option<(f64, f64)>>,
}

fn install_style(display: &gdk::Display) {
    thread_local!(static INSTALLED: Cell<bool> = const { Cell::new(false) });
    if INSTALLED.replace(true) {
        return;
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn draw(painting: &Painting, cr: &cairo::Context, now: Instant) {
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    for line in painting.lines.iter().filter(|line| !line.held) {
        let alpha = line.released.map_or(1.0, |released| {
            let age = now.duration_since(released);
            if age <= HOLD {
                1.0
            } else {
                1.0 - ((age - HOLD).as_secs_f64() / FADE.as_secs_f64()).min(1.0)
            }
        });
        let Some(&(x, y)) = line.points.first() else {
            continue;
        };
        for (rgba, width) in [
            ((1.0, 1.0, 1.0, 0.85 * alpha), 7.0),
            ((1.0, 0.231, 0.188, alpha), 4.5),
        ] {
            cr.move_to(x, y);
            for &(x, y) in &line.points[1..] {
                cr.line_to(x, y);
            }
            if line.points.len() == 1 {
                cr.line_to(x + 0.1, y);
            }
            cr.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
            cr.set_line_width(width);
            let _ = cr.stroke();
        }
    }
    for &(x, y, started) in &painting.pulses {
        let progress = now.duration_since(started).as_secs_f64() / PULSE.as_secs_f64();
        cr.arc(x, y, 10.0 + 18.0 * progress, 0.0, std::f64::consts::TAU);
        cr.set_source_rgba(1.0, 0.231, 0.188, 1.0 - progress.min(1.0));
        cr.set_line_width(3.0);
        let _ = cr.stroke();
    }
}

impl Overlay {
    /// A hidden-until-mapped layer over `monitor`, reporting pointer motion in monitor-local
    /// logical pixels. None where the compositor has no layer-shell.
    pub fn new(monitor: &gdk::Monitor, screen: &str, motion: Motion) -> Option<Rc<Self>> {
        let window = gtk::Window::builder()
            .decorated(false)
            .can_focus(false)
            .focusable(false)
            .title("Mluva screen annotation")
            .build();
        if !crate::layer_shell::ink(&window, "mluva-annotation", monitor) {
            return None;
        }
        install_style(&WidgetExt::display(&window));
        window.add_css_class("ml-ink-overlay");
        let area = gtk::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_can_target(true);
        window.set_child(Some(&area));
        let painting = Rc::new(RefCell::new(Painting::default()));
        let overlay = Rc::new(Self {
            screen: screen.into(),
            window,
            area,
            painting,
            drawing: Cell::new(false),
            ticking: Rc::new(Cell::new(false)),
            pointer: Cell::new(None),
        });
        let painting = overlay.painting.clone();
        overlay.area.set_draw_func(move |_, cr, _, _| {
            draw(&painting.borrow(), cr, Instant::now());
        });
        // Buttons have no handler: a click landing here while ink is up is dropped, never
        // forwarded. Pins come from the compositor's Shift-click binding instead.
        let controller = gtk::EventControllerMotion::new();
        let weak = Rc::downgrade(&overlay);
        let report = move |controller: &gtk::EventControllerMotion, x: f64, y: f64| {
            let Some(overlay) = weak.upgrade() else {
                return;
            };
            // GTK merges motion into one event per frame; its history keeps the full-rate path.
            if let Some(event) = controller.current_event() {
                for sample in event.history() {
                    let axes = sample.axes();
                    let (hx, hy) = (
                        axes[gdk::AxisUse::X.into_glib() as usize],
                        axes[gdk::AxisUse::Y.into_glib() as usize],
                    );
                    if overlay.pointer.get() != Some((hx, hy)) && (hx, hy) != (x, y) {
                        overlay.pointer.set(Some((hx, hy)));
                        motion(&overlay.screen, hx, hy);
                    }
                }
            }
            if overlay.pointer.get() != Some((x, y)) {
                overlay.pointer.set(Some((x, y)));
                motion(&overlay.screen, x, y);
            }
        };
        let entered = report.clone();
        controller.connect_enter(move |controller, x, y| entered(controller, x, y));
        controller.connect_motion(move |controller, x, y| report(controller, x, y));
        overlay.area.add_controller(controller);
        // GTK may recompute a window's input region on allocation; re-apply ours after it.
        let weak = Rc::downgrade(&overlay);
        overlay.area.connect_resize(move |_, _, _| {
            if let Some(overlay) = weak.upgrade() {
                overlay.apply_input_region();
            }
        });
        let weak = Rc::downgrade(&overlay);
        overlay.window.connect_map(move |_| {
            if let Some(overlay) = weak.upgrade() {
                overlay.apply_input_region();
            }
        });
        overlay.window.present();
        overlay.apply_input_region();
        Some(overlay)
    }

    fn apply_input_region(&self) {
        let Some(surface) = self.window.surface() else {
            return;
        };
        let region = if self.drawing.get() {
            let (w, h) = (self.window.width().max(1), self.window.height().max(1));
            cairo::Region::create_rectangle(&cairo::RectangleInt::new(0, 0, w, h))
        } else {
            cairo::Region::create()
        };
        surface.set_input_region(&region);
        // GDK sends a new input region only with the next commit; an idle overlay never
        // commits, so force one.
        self.area.queue_draw();
    }

    /// Take the pointer while Shift is held; let everything through otherwise.
    pub fn set_drawing(&self, drawing: bool) {
        if self.drawing.replace(drawing) == drawing {
            return;
        }
        self.window
            .set_cursor_from_name(drawing.then_some("crosshair"));
        self.apply_input_region();
    }

    /// Hide ink still fading, so the next clean frame shows the screen alone.
    pub fn hide_ink(&self) {
        self.painting
            .borrow_mut()
            .lines
            .retain(|line| line.released.is_none());
        self.area.queue_draw();
    }

    pub fn has_visible_ink(&self) -> bool {
        let mut painting = self.painting.borrow_mut();
        painting.expire(Instant::now());
        painting.visible()
    }

    /// Start a hidden line; `reveal` with the returned id shows it.
    pub fn begin(&self) -> u64 {
        let mut painting = self.painting.borrow_mut();
        painting.next += 1;
        let id = painting.next;
        painting.lines.push(Line {
            id,
            points: vec![],
            released: None,
            held: true,
        });
        id
    }

    pub fn point(&self, x: f64, y: f64) {
        if let Some(line) = self
            .painting
            .borrow_mut()
            .lines
            .iter_mut()
            .rev()
            .find(|line| line.released.is_none())
        {
            line.points.push((x, y));
        }
        self.area.queue_draw();
    }

    /// Show one line once its own clean frame exists; other held lines stay hidden.
    pub fn reveal(&self, id: u64) {
        for line in self
            .painting
            .borrow_mut()
            .lines
            .iter_mut()
            .filter(|line| line.id == id)
        {
            line.held = false;
        }
        self.area.queue_draw();
    }

    pub fn end(self: &Rc<Self>) {
        let now = Instant::now();
        for line in self.painting.borrow_mut().lines.iter_mut() {
            line.released.get_or_insert(now);
        }
        self.animate();
    }

    pub fn pulse(self: &Rc<Self>, x: f64, y: f64) {
        self.painting
            .borrow_mut()
            .pulses
            .push((x, y, Instant::now()));
        self.animate();
    }

    fn animate(self: &Rc<Self>) {
        if self.ticking.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        self.area.add_tick_callback(move |area, _| {
            let Some(overlay) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            area.queue_draw();
            let mut painting = overlay.painting.borrow_mut();
            painting.expire(Instant::now());
            let fading = painting.lines.iter().any(|line| line.released.is_some())
                || !painting.pulses.is_empty();
            if fading {
                glib::ControlFlow::Continue
            } else {
                overlay.ticking.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    pub fn close(&self) {
        self.window.destroy();
    }
}
