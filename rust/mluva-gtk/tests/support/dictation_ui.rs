//! Private GTK driver shared by current dictation UI checks.
#![allow(dead_code)]
use adw::prelude::*;
use std::{
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
pub fn isolated() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("Run through the private desktop runner"),
    )
    .canonicalize()
    .unwrap();
    for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(name).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
    for device in ["/dev/snd", "/dev/input", "/dev/uinput", "/dev/dri"] {
        assert!(!Path::new(device).exists());
    }
    assert_ne!(
        std::fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_string_lossy(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    root
}
pub fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
pub fn settle(seconds: f64) {
    let until = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < until {
        drain();
        thread::sleep(Duration::from_millis(5));
    }
}
#[track_caller]
pub fn until(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(12);
    while !ready() {
        assert!(Instant::now() < end, "Native UI did not settle");
        drain();
        thread::sleep(Duration::from_millis(5));
    }
    drain();
}
pub fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut result = vec![widget.as_ref().clone()];
    let mut child = widget.as_ref().first_child();
    while let Some(current) = child {
        result.extend(widgets(&current));
        child = current.next_sibling();
    }
    result
}
pub fn button(widget: &impl IsA<gtk::Widget>, tip: &str) -> gtk::Button {
    widgets(widget)
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::Button>().ok())
        .find(|b| {
            b.tooltip_text().as_deref() == Some(tip)
                || b.label().as_deref() == Some(tip)
                || (tip == "Copy" && b.tooltip_text().is_some_and(|t| t.starts_with("Copy ")))
        })
        .unwrap_or_else(|| panic!("Button {tip} missing"))
}
pub fn paint(window: &impl IsA<gtk::Window>, name: &str) {
    settle(0.15);
    let window = window.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let mut node = None;
    window.queue_draw();
    until(|| {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            &snapshot,
            f64::from(window.width()),
            f64::from(window.height()),
        );
        node = snapshot.to_node();
        node.is_some()
    });
    let viewport =
        gtk::graphene::Rect::new(0.0, 0.0, window.width() as f32, window.height() as f32);
    let texture = window
        .renderer()
        .unwrap()
        .render_texture(node.unwrap(), Some(&viewport));
    let directory = PathBuf::from(
        std::env::var_os("OFFSCREEN_ARTIFACT_DIR")
            .or_else(|| std::env::var_os("OFFSCREEN_SESSION_ROOT"))
            .unwrap(),
    );
    texture
        .save_to_png(directory.join(format!("{name}.png")))
        .unwrap();
}
