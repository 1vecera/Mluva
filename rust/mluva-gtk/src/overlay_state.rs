//! Bounded recording/review projections on the existing application bus.
use glib::variant::ToVariant;
use mluva_core::text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const OBJECT_PATH: &str = "/com/mluva/Linux/RecordingStatus";
pub const INTERFACE: &str = "com.mluva.Linux.RecordingStatus";
const REVIEW_PHASES: &[&str] = &["ready", "rewriting", "review-error"];
const VISIBLE_PHASES: &[&str] = &["preparing", "recording", "processing", "copied", "error"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayState {
    pub phase: String,
    pub detail: String,
    pub elapsed_seconds: i64,
    pub mode: String,
    pub route: String,
    pub level: f64,
    pub preview: String,
    pub delivery: String,
    pub review_identifier: String,
    pub review_options: Vec<(String, String)>,
    pub message: String,
    pub review_timeout_seconds: i64,
    pub show_copy_action: bool,
    pub smooth_scrolling: bool,
    pub scroll_duration_ms: i64,
    pub scroll_lookahead_lines: i64,
    pub widget_position: String,
    pub widget_lines: i64,
    pub widget_opacity: i64,
    pub rewrite_enabled: bool,
}
impl Default for OverlayState {
    fn default() -> Self {
        Self {
            phase: "hidden".into(),
            detail: String::new(),
            elapsed_seconds: 0,
            mode: String::new(),
            route: String::new(),
            level: 0.0,
            preview: String::new(),
            delivery: String::new(),
            review_identifier: String::new(),
            review_options: vec![],
            message: String::new(),
            review_timeout_seconds: 4,
            show_copy_action: true,
            smooth_scrolling: true,
            scroll_duration_ms: 800,
            scroll_lookahead_lines: 2,
            widget_position: "bottom-center".into(),
            widget_lines: 5,
            widget_opacity: 82,
            rewrite_enabled: true,
        }
    }
}
fn compact(value: &str) -> String {
    value
        .split(text::whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
fn prefix(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}
fn line(value: &str, limit: usize) -> String {
    prefix(&compact(value), limit)
}
impl OverlayState {
    pub fn signal(&self) -> glib::Variant {
        let phase = caseless::default_case_fold_str(&self.phase);
        let visible = VISIBLE_PHASES.contains(&phase.as_str());
        if !visible {
            return (false, "hidden", "", 0_u32, "", "", 0.0_f64, "", "").to_variant();
        }
        let preview = compact(&self.preview);
        let start = preview.chars().count().saturating_sub(180);
        (
            true,
            phase,
            line(&self.detail, 80),
            self.elapsed_seconds.clamp(0, 86_400) as u32,
            line(&self.mode, 32),
            line(&self.route, 48),
            self.bounded_level(),
            preview.chars().skip(start).collect::<String>(),
            line(&self.delivery, 48),
        )
            .to_variant()
    }
    fn bounded_level(&self) -> f64 {
        if self.level.is_finite() {
            self.level.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
    pub fn shell_values(&self) -> BTreeMap<String, glib::Variant> {
        let phase = if REVIEW_PHASES.contains(&self.phase.as_str())
            || ["preparing", "recording", "processing", "error"].contains(&self.phase.as_str())
        {
            self.phase.as_str()
        } else {
            "idle"
        };
        let mut values = BTreeMap::from([
            ("phase".into(), phase.to_variant()),
            ("elapsed".into(), 0_u32.to_variant()),
        ]);
        if phase == "idle" {
            return values;
        }
        let preview = compact(&self.preview);
        let characters = preview.chars().collect::<Vec<_>>();
        let mut start = characters.len().saturating_sub(4096);
        if start != 0
            && let Some(boundary) = characters[start - 1..]
                .iter()
                .position(|character| *character == ' ')
        {
            start += boundary;
        }
        values.extend([
            ("rewrite_enabled".into(), self.rewrite_enabled.to_variant()),
            (
                "widget_lines".into(),
                (self.widget_lines.clamp(1, 10) as u32).to_variant(),
            ),
            (
                "widget_opacity".into(),
                (self.widget_opacity.clamp(10, 100) as u32).to_variant(),
            ),
            (
                "review_timeout".into(),
                (self.review_timeout_seconds.clamp(1, 60) as u32).to_variant(),
            ),
            ("show_copy".into(), self.show_copy_action.to_variant()),
            (
                "smooth_scrolling".into(),
                self.smooth_scrolling.to_variant(),
            ),
            (
                "scroll_duration".into(),
                (self.scroll_duration_ms.clamp(0, 2000) as u32).to_variant(),
            ),
            (
                "scroll_lookahead".into(),
                (self.scroll_lookahead_lines.clamp(0, 6) as u32).to_variant(),
            ),
            (
                "widget_position".into(),
                if ["bottom-left", "bottom-center", "bottom-right"]
                    .contains(&self.widget_position.as_str())
                {
                    self.widget_position.as_str()
                } else {
                    "bottom-center"
                }
                .to_variant(),
            ),
            (
                "elapsed".into(),
                (self.elapsed_seconds.clamp(0, 86_400) as u32).to_variant(),
            ),
            ("level".into(), self.bounded_level().to_variant()),
            (
                "preview".into(),
                characters[start..].iter().collect::<String>().to_variant(),
            ),
            ("preview_start".into(), (start as u32).to_variant()),
        ]);
        if REVIEW_PHASES.contains(&phase) {
            values.extend([
                (
                    "identifier".into(),
                    prefix(&self.review_identifier, 36).to_variant(),
                ),
                (
                    "options".into(),
                    self.review_options
                        .iter()
                        .take(128)
                        .map(|(id, name)| (prefix(id, 36), line(name, 64)))
                        .collect::<Vec<_>>()
                        .to_variant(),
                ),
                ("message".into(), line(&self.message, 96).to_variant()),
            ]);
        }
        values
    }
    pub fn shell(&self) -> glib::Variant {
        (self.shell_values(),).to_variant()
    }
}

pub struct OverlayPublisher {
    connection: gio::DBusConnection,
    signal: glib::Variant,
    shell: glib::Variant,
}
impl OverlayPublisher {
    pub fn new(connection: gio::DBusConnection) -> Self {
        let hidden = OverlayState::default();
        Self {
            connection,
            signal: hidden.signal(),
            shell: hidden.shell(),
        }
    }
    pub fn publish(&mut self, state: &OverlayState) -> bool {
        self.signal = state.signal();
        self.shell = state.shell();
        self.replay()
    }
    pub fn replay(&self) -> bool {
        self.connection
            .emit_signal(
                None,
                OBJECT_PATH,
                INTERFACE,
                "StateChanged",
                Some(&self.signal),
            )
            .is_ok()
            && self
                .connection
                .emit_signal(
                    None,
                    OBJECT_PATH,
                    INTERFACE,
                    "ShellStateChanged",
                    Some(&self.shell),
                )
                .is_ok()
    }
    pub fn clear(&mut self) -> bool {
        self.publish(&OverlayState::default())
    }
}
