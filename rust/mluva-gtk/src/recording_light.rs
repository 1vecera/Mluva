//! A fixed layout slot with reduced-motion-aware native recording animation.

use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct RecordingLight {
    pub widget: gtk::DrawingArea,
    recording: Cell<bool>,
    breath: Cell<f64>,
    phase: Cell<f64>,
    started_at: Cell<i64>,
    tick: RefCell<Option<gtk::TickCallbackId>>,
    settings: RefCell<Option<(gtk::Settings, glib::SignalHandlerId)>>,
}

impl RecordingLight {
    pub fn new() -> Rc<Self> {
        let widget = gtk::DrawingArea::builder()
            .content_width(20)
            .content_height(20)
            .valign(gtk::Align::Center)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        widget.add_css_class("ml-recording-light");
        let light = Rc::new(Self {
            widget,
            recording: Cell::new(false),
            breath: Cell::new(0.0),
            phase: Cell::new(0.0),
            started_at: Cell::new(0),
            tick: RefCell::new(None),
            settings: RefCell::new(None),
        });
        let weak = Rc::downgrade(&light);
        light
            .widget
            .set_draw_func(move |area, context, width, height| {
                if let Some(l) = weak.upgrade() {
                    l.draw(area, context, width, height);
                }
            });
        let weak = Rc::downgrade(&light);
        light.widget.connect_map(move |area| {
            if let Some(l) = weak.upgrade() {
                let settings = area.settings();
                let weak = Rc::downgrade(&l);
                let handler = settings.connect_gtk_enable_animations_notify(move |_| {
                    if let Some(l) = weak.upgrade() {
                        l.sync_motion();
                    }
                });
                l.settings.replace(Some((settings, handler)));
                l.sync_motion();
            }
        });
        let weak = Rc::downgrade(&light);
        light.widget.connect_unmap(move |_| {
            if let Some(l) = weak.upgrade() {
                l.disconnect_settings();
                l.stop_motion();
            }
        });
        light.set_recording(false);
        light
    }

    pub fn set_recording(self: &Rc<Self>, recording: bool) {
        self.recording.set(recording);
        let label = if recording {
            "Recording"
        } else {
            "Dictation status"
        };
        self.widget.set_tooltip_text(Some(label));
        self.widget
            .update_property(&[gtk::accessible::Property::Label(label)]);
        self.sync_motion();
    }

    fn sync_motion(self: &Rc<Self>) {
        let moving = self.recording.get()
            && self.widget.is_mapped()
            && self.widget.settings().is_gtk_enable_animations();
        if moving && self.tick.borrow().is_none() {
            self.started_at.set(0);
            let weak = Rc::downgrade(self);
            let tick = self.widget.add_tick_callback(move |area, clock| {
                let Some(l) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if l.started_at.get() == 0 {
                    l.started_at.set(clock.frame_time());
                }
                let phase = (clock.frame_time() - l.started_at.get()) as f64 / 3_400_000.0
                    * std::f64::consts::TAU;
                l.phase.set(phase);
                l.breath.set(-phase.cos());
                area.queue_draw();
                glib::ControlFlow::Continue
            });
            self.tick.replace(Some(tick));
        } else if !moving {
            self.stop_motion();
        }
        self.widget.queue_draw();
    }

    fn stop_motion(&self) {
        if let Some(tick) = self.tick.borrow_mut().take() {
            tick.remove();
        }
        self.breath.set(0.0);
        self.phase.set(0.0);
        self.widget.queue_draw();
    }

    fn disconnect_settings(&self) {
        if let Some((settings, handler)) = self.settings.borrow_mut().take() {
            settings.disconnect(handler);
        }
    }

    fn draw(
        &self,
        area: &gtk::DrawingArea,
        context: &gtk::cairo::Context,
        width: i32,
        height: i32,
    ) {
        let color = area.color();
        let radius = if self.recording.get() {
            6.4 + 1.6 * self.breath.get()
        } else {
            3.0
        };
        let phase = self.phase.get();
        let points = (0..=64)
            .map(|index| {
                let angle = f64::from(index) / 64.0 * std::f64::consts::TAU;
                let wobble = if self.recording.get() {
                    phase.sin().powi(2)
                        * (0.07 * (3.0 * angle + phase).sin()
                            + 0.035 * (5.0 * angle - 2.0 * phase).sin())
                } else {
                    0.0
                };
                ((1.0 + wobble) * angle.cos(), (1.0 + wobble) * angle.sin())
            })
            .collect::<Vec<_>>();
        let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        for (index, (x, y)) in points.into_iter().enumerate() {
            let x = f64::from(width) / 2.0 + radius * (2.0 * (x - min_x) / (max_x - min_x) - 1.0);
            let y = f64::from(height) / 2.0 + radius * (2.0 * (y - min_y) / (max_y - min_y) - 1.0);
            if index == 0 {
                context.move_to(x, y);
            } else {
                context.line_to(x, y);
            }
        }
        context.close_path();
        let gradient = gtk::cairo::RadialGradient::new(
            f64::from(width) * 0.42,
            f64::from(height) * 0.4,
            0.0,
            f64::from(width) / 2.0,
            f64::from(height) / 2.0,
            radius * 1.1,
        );
        for (offset, alpha) in [(0.0, 0.84), (0.64, 0.66), (1.0, 0.12)] {
            gradient.add_color_stop_rgba(
                offset,
                color.red().into(),
                color.green().into(),
                color.blue().into(),
                alpha,
            );
        }
        if context.set_source(&gradient).is_ok() {
            let _ = context.fill();
        }
    }
}

impl Drop for RecordingLight {
    fn drop(&mut self) {
        self.disconnect_settings();
        self.stop_motion();
    }
}
