//! Shared gutters, transcript following and native document resources.

use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct DocumentResources {
    pub brand: PathBuf,
    pub mermaid: PathBuf,
    pub font: PathBuf,
}

impl DocumentResources {
    pub fn from_directory(directory: &Path) -> Self {
        Self {
            brand: directory.join("mluva-symbolic.svg"),
            mermaid: directory.join("mermaid"),
            font: directory.join("fonts/JetBrainsMono-Regular.ttf"),
        }
    }

    pub fn brand_mark(&self, size: i32) -> gtk::Image {
        let image = gtk::Image::from_gicon(&gio::FileIcon::new(&gio::File::for_path(&self.brand)));
        image.set_pixel_size(size);
        image.add_css_class("ml-brand-mark");
        image
    }
}

pub fn margins(widget: &impl IsA<gtk::Widget>, amount: i32) {
    widget.set_margin_top(amount);
    widget.set_margin_bottom(amount);
    widget.set_margin_start(amount);
    widget.set_margin_end(amount);
}

pub fn document_scroll(child: &impl IsA<gtk::Widget>, expand: bool) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Always)
        .overlay_scrolling(false)
        .vexpand(expand)
        .child(child)
        .build();
    scroll.add_css_class("ml-scroll-gutter");
    let bar = scroll.vscrollbar().downgrade();
    scroll.vadjustment().connect_changed(move |adjustment| {
        if let Some(bar) = bar.upgrade() {
            bar.set_opacity(if adjustment.upper() > adjustment.page_size() + 1.0 {
                1.0
            } else {
                0.0
            });
        }
    });
    let adjustment = scroll.vadjustment();
    scroll
        .vscrollbar()
        .set_opacity(if adjustment.upper() > adjustment.page_size() + 1.0 {
            1.0
        } else {
            0.0
        });
    scroll
}

#[derive(Default)]
pub struct SpeechScrollForecast {
    samples: VecDeque<(f64, usize)>,
    high_water: usize,
}

impl SpeechScrollForecast {
    pub fn observe(&mut self, characters: usize, now: f64) {
        self.high_water = self.high_water.max(characters);
        self.samples.push_back((now, self.high_water));
        while self.samples.len() > 2 && self.samples[1].0 < now - 4.0 {
            self.samples.pop_front();
        }
    }

    pub fn reserve(&self, columns: f64, fill: f64, horizon: f64, limit: i64) -> f64 {
        let mut rate = 12.0;
        if let (Some(first), Some(last)) = (self.samples.front(), self.samples.back()) {
            let elapsed = last.0 - first.0;
            if self.samples.len() > 1 && elapsed >= 0.25 {
                rate = ((last.1 - first.1) as f64 / elapsed).clamp(0.0, 80.0);
            }
        }
        let projected = fill + rate * horizon.max(0.2) / columns.max(1.0);
        (projected - 1.0).max(0.0).min(limit.max(0) as f64)
    }
}

pub struct TailFollower {
    scroll: gtk::ScrolledWindow,
    view: Option<gtk::TextView>,
    pub following: Cell<bool>,
    pub writing: Cell<bool>,
    pending: RefCell<Option<glib::SourceId>>,
    snap_next: Cell<bool>,
    animation: RefCell<Option<adw::TimedAnimation>>,
    destination: Cell<f64>,
    pub duration_ms: Cell<u32>,
    pub smooth: Cell<bool>,
    pub lookahead_lines: Cell<i64>,
    forecast: RefCell<SpeechScrollForecast>,
    retained_bottom: Cell<f64>,
    revision_inset: Cell<i32>,
    base_top: i32,
}

impl TailFollower {
    pub fn new(scroll: &gtk::ScrolledWindow, view: Option<&gtk::TextView>) -> Rc<Self> {
        let view = view
            .cloned()
            .or_else(|| scroll.child().and_downcast::<gtk::TextView>());
        let follower = Rc::new(Self {
            scroll: scroll.clone(),
            base_top: view.as_ref().map_or(4, gtk::TextView::top_margin),
            view,
            following: Cell::new(true),
            writing: Cell::new(false),
            pending: RefCell::new(None),
            snap_next: Cell::new(true),
            animation: RefCell::new(None),
            destination: Cell::new(0.0),
            duration_ms: Cell::new(800),
            smooth: Cell::new(true),
            lookahead_lines: Cell::new(2),
            forecast: RefCell::new(SpeechScrollForecast::default()),
            retained_bottom: Cell::new(0.0),
            revision_inset: Cell::new(0),
        });
        let weak = Rc::downgrade(&follower);
        scroll.vadjustment().connect_changed(move |_| {
            if let Some(f) = weak.upgrade() {
                f.queue();
            }
        });
        let weak = Rc::downgrade(&follower);
        scroll.vadjustment().connect_value_changed(move |a| {
            if let Some(f) = weak.upgrade()
                && !f.writing.get()
                && f.pending.borrow().is_none()
            {
                f.following
                    .set(a.value() + a.page_size() >= a.upper() - 24.0);
                if !f.following.get() {
                    f.pause();
                }
            }
        });
        let navigation = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        navigation.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&follower);
        navigation.connect_scroll(move |_, _, dy| {
            if dy < 0.0
                && let Some(f) = weak.upgrade()
            {
                f.reader_navigation();
            }
            glib::Propagation::Proceed
        });
        scroll.add_controller(navigation);
        let drag = gtk::GestureClick::new();
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&follower);
        drag.connect_pressed(move |_, _, _, _| {
            if let Some(f) = weak.upgrade() {
                f.reader_navigation();
            }
        });
        scroll.vscrollbar().add_controller(drag);
        follower
    }

    pub fn follow(self: &Rc<Self>, snap: bool) {
        self.following.set(true);
        if snap {
            self.pause();
            self.destination.set(0.0);
            self.retained_bottom.set(0.0);
            self.forecast.replace(SpeechScrollForecast::default());
            self.revision_inset.set(0);
            if let Some(view) = &self.view {
                view.set_top_margin(self.base_top);
                view.set_bottom_margin(4);
            }
            self.write(0.0);
        }
        self.snap_next.set(self.snap_next.get() || snap);
        self.queue();
    }

    pub fn queue(self: &Rc<Self>) {
        if self.following.get() && self.pending.borrow().is_none() {
            let weak = Rc::downgrade(self);
            let source = glib::idle_add_local_once(move || {
                if let Some(f) = weak.upgrade() {
                    f.pending.borrow_mut().take();
                    f.reveal();
                }
            });
            self.pending.replace(Some(source));
        }
    }

    pub fn write(&self, value: f64) {
        self.writing.set(true);
        self.scroll.vadjustment().set_value(value);
        self.writing.set(false);
    }

    pub fn prepare_update(self: &Rc<Self>, text: &str, reset: bool) {
        if reset {
            self.follow(true);
        }
        if let Some(view) = &self.view {
            let buffer = view.buffer();
            if text
                == buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), true)
                    .as_str()
            {
                return;
            }
        }
        self.forecast.borrow_mut().observe(
            text.chars().count(),
            glib::monotonic_time() as f64 / 1_000_000.0,
        );
        if self.following.get()
            && let Some(view) = &self.view
        {
            let a = self.scroll.vadjustment();
            self.retained_bottom.set(if self.destination.get() > 0.0 {
                self.destination.get() + a.page_size()
            } else {
                0.0
            });
            self.writing.set(true);
            if !reset {
                view.set_bottom_margin(
                    view.bottom_margin()
                        .max(a.page_size().round_ties_even() as i32),
                );
            }
        }
    }

    pub fn updated(self: &Rc<Self>) {
        self.writing.set(false);
        self.queue();
    }
    fn pause(&self) {
        if let Some(animation) = self.animation.borrow_mut().take()
            && animation.state() == adw::AnimationState::Playing
        {
            animation.pause();
        }
    }
    pub fn stop(&self) {
        self.following.set(false);
        self.pause();
    }

    fn reveal(self: &Rc<Self>) {
        if !self.following.get() {
            return;
        }
        let a = self.scroll.vadjustment();
        let direct = self
            .view
            .as_ref()
            .is_some_and(|v| self.scroll.child().as_ref() == Some(v.upcast_ref()));
        if direct && (!self.scroll.is_mapped() || a.page_size() <= 0.0) {
            return;
        }
        self.anticipate_wrap();
        let mut destination = (a.upper() - a.page_size()).max(0.0);
        if direct && let Some(view) = &self.view {
            let (y, height) = view.line_yrange(&view.buffer().end_iter());
            destination = (f64::from(y + height + view.top_margin() + view.bottom_margin())
                - a.page_size())
            .max(0.0);
        }
        if !self.snap_next.get() {
            destination = destination.max(a.value()).max(self.destination.get());
        }
        if !self.snap_next.get()
            && self.smooth.get()
            && self
                .animation
                .borrow()
                .as_ref()
                .is_some_and(|a| a.state() == adw::AnimationState::Playing)
            && (destination - self.destination.get()).abs() < 1.0
        {
            return;
        }
        self.pause();
        self.destination.set(destination);
        if self.snap_next.get()
            || !self.smooth.get()
            || !self.scroll.is_mapped()
            || (destination - a.value()).abs() < 1.0
        {
            self.write(destination);
        } else {
            let weak = Rc::downgrade(self);
            let target = adw::CallbackAnimationTarget::new(move |value| {
                if let Some(f) = weak.upgrade() {
                    f.write(value);
                }
            });
            let animation = adw::TimedAnimation::new(
                &self.scroll,
                a.value(),
                destination,
                self.duration_ms.get(),
                target,
            );
            animation.set_easing(adw::Easing::EaseOutCubic);
            animation.play();
            self.animation.replace(Some(animation));
        }
        self.snap_next.set(false);
    }

    fn anticipate_wrap(&self) {
        let Some(view) = &self.view else {
            return;
        };
        if !view.is_mapped() {
            return;
        }
        let ending = view.iter_location(&view.buffer().end_iter());
        let width = (view.width() - view.left_margin() - view.right_margin()).max(1);
        let fill = f64::from(ending.x() - view.left_margin()) / f64::from(width);
        let char_width = view.create_pango_layout(Some("M")).pixel_size().0.max(1);
        let reserve = self.forecast.borrow().reserve(
            f64::from(width) / f64::from(char_width),
            fill,
            f64::from(self.duration_ms.get()) / 1000.0 + 0.25,
            self.lookahead_lines.get(),
        );
        let extra = if f64::from(ending.y() + ending.height() + 4)
            >= self.scroll.vadjustment().page_size()
        {
            (f64::from(ending.height()) * reserve).round_ties_even() as i32
        } else {
            0
        };
        let retained = (self.retained_bottom.get()
            - f64::from(4 + extra + ending.y() + ending.height()))
        .max(0.0);
        let inset = if self.destination.get() > 0.0 {
            retained.round_ties_even() as i32
        } else {
            0
        };
        self.writing.set(true);
        if inset != self.revision_inset.get() {
            self.revision_inset.set(inset);
            view.set_top_margin(self.base_top + inset);
        }
        if view.bottom_margin() != 4 + extra {
            view.set_bottom_margin(4 + extra);
        }
        self.writing.set(false);
    }

    fn reader_navigation(&self) {
        self.stop();
        if self.revision_inset.get() != 0
            && let Some(view) = &self.view
        {
            let position =
                (self.scroll.vadjustment().value() - f64::from(self.revision_inset.get())).max(0.0);
            self.writing.set(true);
            view.set_top_margin(self.base_top);
            self.revision_inset.set(0);
            self.retained_bottom.set(0.0);
            self.destination.set(position);
            self.scroll.vadjustment().set_value(position);
            self.writing.set(false);
        }
    }
}

impl Drop for TailFollower {
    fn drop(&mut self) {
        if let Some(source) = self.pending.get_mut().take() {
            source.remove();
        }
        self.pause();
    }
}
