//! A private, synthetic recorder preview shared by setup and preferences.
use crate::{direct_choices::DirectChoices, theme};
use adw::prelude::*;
use mluva_core::config::AppConfig;
use serde_json::{Map, Value, json};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

const POSITIONS: [&str; 3] = ["bottom-left", "bottom-center", "bottom-right"];
const SAMPLE_LINES: [&str; 10] = [
    "A small idea becomes a clear thought.",
    "Speak naturally and keep your rhythm.",
    "Your words appear while you talk.",
    "The original transcript stays yours.",
    "Five lines leave room for the desktop.",
    "New words gently move into view.",
    "Choose more space for longer thoughts.",
    "Or keep the recorder small and quiet.",
    "Adjust the background to suit your eyes.",
    "This is the last of ten sample lines.",
];
pub struct AppearanceSettings {
    pub widget: adw::PreferencesGroup,
    pub preview: gtk::DrawingArea,
    pub caption: gtk::Label,
    pub position: Rc<DirectChoices>,
    pub lines: gtk::Scale,
    pub line_label: gtk::Label,
    pub opacity: gtk::Scale,
    pub opacity_label: gtk::Label,
    pub paste: adw::SwitchRow,
    changed: Rc<dyn Fn()>,
    timer: RefCell<Option<glib::SourceId>>,
}
impl AppearanceSettings {
    pub fn new(config: &AppConfig, changed: Rc<dyn Fn()>) -> Rc<Self> {
        let widget = adw::PreferencesGroup::builder()
            .title("Your recorder")
            .description("A little space for your voice.")
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let preview_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        preview_box.add_css_class("card");
        let header = gtk::Box::builder()
            .margin_start(14)
            .margin_end(14)
            .margin_top(12)
            .build();
        let label = gtk::Label::builder()
            .label("DESKTOP PREVIEW")
            .xalign(0.0)
            .hexpand(true)
            .build();
        label.add_css_class("caption");
        label.add_css_class("dim-label");
        header.append(&label);
        let label = gtk::Label::new(Some("● Live"));
        label.add_css_class("caption");
        header.append(&label);
        preview_box.append(&header);
        let preview = gtk::DrawingArea::builder()
            .content_height(180)
            .hexpand(true)
            .build();
        preview.update_property(&[gtk::accessible::Property::Label(
            "Recorder appearance preview over a sample desktop",
        )]);
        preview_box.append(&preview);
        let caption = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .margin_start(14)
            .margin_end(14)
            .margin_bottom(12)
            .build();
        caption.add_css_class("caption");
        caption.add_css_class("dim-label");
        preview_box.append(&caption);
        content.append(&preview_box);
        let heading = gtk::Label::builder()
            .label("Where should it sit?")
            .xalign(0.0)
            .build();
        heading.add_css_class("heading");
        content.append(&heading);
        let owner = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let weak = weak.clone();
            let position = DirectChoices::new(
                &["↙  Left", "↓  Center", "↘  Right"],
                Rc::new(move |_| {
                    if let Some(owner) = weak.upgrade() {
                        owner.changed();
                    }
                }),
            );
            position.set_selected(
                POSITIONS
                    .iter()
                    .position(|value| *value == config.widget_position)
                    .unwrap(),
            );
            content.append(&position.widget);
            let lines = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 10.0, 1.0);
            lines.set_round_digits(0);
            lines.set_draw_value(false);
            lines.set_value(config.widget_lines as f64);
            lines.update_property(&[gtk::accessible::Property::Label("Visible transcript lines")]);
            for value in [1, 3, 5, 10] {
                lines.add_mark(
                    f64::from(value),
                    gtk::PositionType::Bottom,
                    Some(&value.to_string()),
                );
            }
            let line_label = gtk::Label::builder().xalign(0.0).build();
            line_label.add_css_class("heading");
            content.append(&line_label);
            content.append(&lines);
            let opacity = gtk::Scale::with_range(gtk::Orientation::Horizontal, 10.0, 100.0, 1.0);
            opacity.set_draw_value(false);
            opacity.set_value(config.widget_opacity as f64);
            opacity.update_property(&[gtk::accessible::Property::Label(
                "Recorder background opacity",
            )]);
            let opacity_label = gtk::Label::builder().xalign(0.0).build();
            opacity_label.add_css_class("heading");
            content.append(&opacity_label);
            content.append(&opacity);
            let ends = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            for (text, x, expand) in [("More desktop", 0.0, true), ("More contrast", 1.0, false)] {
                let label = gtk::Label::builder()
                    .label(text)
                    .xalign(x)
                    .hexpand(expand)
                    .build();
                label.add_css_class("caption");
                label.add_css_class("dim-label");
                ends.append(&label);
            }
            content.append(&ends);
            let paste = adw::SwitchRow::builder()
                .title("Paste when I stop")
                .subtitle("Off recommended · copy when you are ready")
                .active(config.auto_paste)
                .build();
            let switches = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .build();
            switches.add_css_class("boxed-list");
            switches.append(&paste);
            content.append(&switches);
            widget.add(&content);
            Self {
                widget,
                preview,
                caption,
                position,
                lines,
                line_label,
                opacity,
                opacity_label,
                paste,
                changed,
                timer: RefCell::new(None),
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.lines.connect_value_changed(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.changed();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.opacity.connect_value_changed(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.changed();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.paste.connect_active_notify(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.changed();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner
            .preview
            .set_draw_func(move |_, context, width, height| {
                if let Some(owner) = weak.upgrade() {
                    owner.draw(context, width, height);
                }
            });
        let weak = Rc::downgrade(&owner);
        owner.preview.connect_map(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.animate();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.preview.connect_unmap(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.stop_animation();
            }
        });
        owner.changed();
        owner
    }
    pub fn values(&self) -> Map<String, Value> {
        json!({"widget_position":POSITIONS[self.position.selected()],"widget_lines":self.lines.value() as i64,"widget_opacity":self.opacity.value() as i64,"auto_paste":self.paste.is_active()}).as_object().unwrap().clone()
    }
    pub fn refresh_config(&self, config: &AppConfig) {
        self.position.set_selected(
            POSITIONS
                .iter()
                .position(|value| *value == config.widget_position)
                .unwrap(),
        );
        self.lines.set_value(config.widget_lines as f64);
        self.opacity.set_value(config.widget_opacity as f64);
        self.paste.set_active(config.auto_paste);
    }
    fn changed(&self) {
        self.preview.queue_draw();
        self.line_label.set_label(&format!(
            "{} visible lines · 5 recommended",
            self.lines.value() as i64
        ));
        self.opacity_label
            .set_label(&format!("Background · {}%", self.opacity.value() as i64));
        self.caption.set_label(&format!(
            "Showing the last {} of 10 lines · {}% background opacity",
            self.lines.value() as i64,
            self.opacity.value() as i64
        ));
        (self.changed)();
    }
    fn animate(self: &Rc<Self>) {
        if self.preview.settings().is_gtk_enable_animations() && self.timer.borrow().is_none() {
            let weak = Rc::downgrade(self);
            self.timer.replace(Some(glib::timeout_add_local(
                Duration::from_millis(66),
                move || {
                    if let Some(owner) = weak.upgrade() {
                        owner.preview.queue_draw();
                        glib::ControlFlow::Continue
                    } else {
                        glib::ControlFlow::Break
                    }
                },
            )));
        }
    }
    fn stop_animation(&self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
    }
    fn draw(&self, cr: &gtk::cairo::Context, width: i32, height: i32) {
        let state = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
            });
        let tokens = theme::read_omarchy_palette(&state.join("omarchy/current/theme/colors.toml"))
            .map(|(palette, _)| palette)
            .unwrap_or_else(|| theme::default_palette(adw::StyleManager::default().is_dark()));
        let color = |name: &str, alpha: f64| {
            let value = &tokens[name];
            let values = [1, 3, 5].map(|index| {
                f64::from(
                    u8::from_str_radix(&value[index..index + 2], 16).expect("validated palette"),
                ) / 255.0
            });
            cr.set_source_rgba(values[0], values[1], values[2], alpha);
        };
        let text = |value: &str, x: f64, y: f64, size: f64| {
            let layout = pangocairo::functions::create_layout(cr);
            let mut font = gtk::pango::FontDescription::from_string("JetBrains Mono");
            font.set_absolute_size(size * f64::from(gtk::pango::SCALE));
            layout.set_font_description(Some(&font));
            layout.set_text(value);
            cr.move_to(x, y);
            pangocairo::functions::show_layout(cr, &layout);
        };
        let width = f64::from(width);
        let height = f64::from(height);
        color("canvas", 1.0);
        let _ = cr.paint();
        color("ink", 0.06);
        cr.rectangle(20.0, 12.0, width - 40.0, height - 28.0);
        let _ = cr.fill();
        color("action", 0.09);
        cr.rectangle(20.0, 12.0, 76.0, height - 28.0);
        let _ = cr.fill();
        color("ink", 0.35);
        text("Launch notes", 112.0, 25.0, 12.0);
        color("ink", 0.10);
        for index in 0..7 {
            cr.rectangle(
                112.0,
                56.0 + f64::from(index) * 22.0,
                ((width - 148.0) * if index % 3 == 0 { 0.55 } else { 0.8 }).max(20.0),
                2.0,
            );
        }
        let _ = cr.fill();
        let lines = self.lines.value() as usize;
        let panel_width = 500.0;
        let panel_height = 40.0 + lines as f64 * 22.0;
        let scale = 1_f64
            .min((width - 56.0) / panel_width)
            .min((height - 32.0) / panel_height);
        let rendered_width = panel_width * scale;
        let x = match self.position.selected() {
            0 => 12.0,
            2 => width - rendered_width - 12.0,
            _ => (width - rendered_width) / 2.0,
        };
        let _ = cr.save();
        cr.translate(x, height - panel_height * scale - 12.0);
        cr.scale(scale, scale);
        let phase = if self.timer.borrow().is_some() {
            (glib::monotonic_time() as f64 / 1_000_000.0 % 3.4) / 3.4 * std::f64::consts::TAU
        } else {
            0.0
        };
        let radius = 6.4 - 1.6 * phase.cos();
        let ink = &tokens["danger"];
        let rgb = [1, 3, 5].map(|index| {
            f64::from(u8::from_str_radix(&ink[index..index + 2], 16).unwrap()) / 255.0
        });
        let gradient = gtk::cairo::RadialGradient::new(6.4, 8.0, 0.0, 8.0, 10.0, radius * 1.1);
        for (stop, alpha) in [(0.0, 0.84), (0.64, 0.66), (1.0, 0.12)] {
            gradient.add_color_stop_rgba(stop, rgb[0], rgb[1], rgb[2], alpha);
        }
        let _ = cr.set_source(&gradient);
        let points: Vec<_> = (0..65)
            .map(|index| {
                let angle = f64::from(index) / 64.0 * std::f64::consts::TAU;
                let wobble = phase.sin().powi(2)
                    * (0.07 * (3.0 * angle + phase).sin()
                        + 0.035 * (5.0 * angle - 2.0 * phase).sin());
                ((1.0 + wobble) * angle.cos(), (1.0 + wobble) * angle.sin())
            })
            .collect();
        let left = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let right = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let top = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let bottom = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        for (index, (px, py)) in points.iter().enumerate() {
            let x = 8.0 + radius * (2.0 * (px - left) / (right - left) - 1.0);
            let y = 10.0 + radius * (2.0 * (py - top) / (bottom - top) - 1.0);
            if index == 0 {
                cr.move_to(x, y);
            } else {
                cr.line_to(x, y);
            }
        }
        cr.close_path();
        let _ = cr.fill();
        color("ink", 1.0);
        text("00:12", panel_width - 42.0, 0.0, 14.0);
        cr.rectangle(0.0, 20.0, panel_width, panel_height - 20.0);
        color("surface", self.opacity.value() as i64 as f64 / 100.0);
        let _ = cr.fill_preserve();
        color("outline", 1.0);
        cr.set_line_width(1.0);
        let _ = cr.stroke();
        cr.rectangle(10.0, 30.0, panel_width - 20.0, lines as f64 * 22.0);
        cr.clip();
        color("ink", 1.0);
        for (index, line) in SAMPLE_LINES[10 - lines..].iter().enumerate() {
            text(line, 10.0, 30.0 + index as f64 * 22.0, 14.0);
        }
        let _ = cr.restore();
    }
}
impl Drop for AppearanceSettings {
    fn drop(&mut self) {
        self.stop_animation();
    }
}
