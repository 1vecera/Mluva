//! Present a window as a full-screen overlay through wlr-layer-shell, as Omarchy's own launcher
//! and menus do, where the compositor supports it.
use glib::translate::ToGlibPtr;
use gtk::prelude::*;
use std::ffi::{CString, c_char, c_int};

// gtk4-layer-shell must load before libwayland-client, so it is linked rather than opened later.
#[link(name = "gtk4-layer-shell")]
unsafe extern "C" {
    fn gtk_layer_is_supported() -> glib::ffi::gboolean;
    fn gtk_layer_init_for_window(window: *mut gtk::ffi::GtkWindow);
    fn gtk_layer_set_namespace(window: *mut gtk::ffi::GtkWindow, name_space: *const c_char);
    fn gtk_layer_set_layer(window: *mut gtk::ffi::GtkWindow, layer: c_int);
    fn gtk_layer_set_anchor(
        window: *mut gtk::ffi::GtkWindow,
        edge: c_int,
        anchor_to_edge: glib::ffi::gboolean,
    );
    fn gtk_layer_set_exclusive_zone(window: *mut gtk::ffi::GtkWindow, exclusive_zone: c_int);
    fn gtk_layer_set_keyboard_mode(window: *mut gtk::ffi::GtkWindow, mode: c_int);
    fn gtk_layer_set_monitor(window: *mut gtk::ffi::GtkWindow, monitor: *mut gdk_ffi::GdkMonitor);
}
use gtk::gdk::ffi as gdk_ffi;

const LAYER_OVERLAY: c_int = 3;
const KEYBOARD_NONE: c_int = 0;
const KEYBOARD_EXCLUSIVE: c_int = 1;
/// Left, right, top and bottom.
const EDGES: [c_int; 4] = [0, 1, 2, 3];

fn supported(window: &impl IsA<gtk::Window>) -> bool {
    // gtk4-layer-shell asserts a Wayland display, so X11 sessions never reach it.
    let wayland = WidgetExt::display(window.as_ref())
        .type_()
        .name()
        .starts_with("GdkWayland");
    // SAFETY: only queries the Wayland display's advertised globals.
    wayland && unsafe { gtk_layer_is_supported() } != glib::ffi::GFALSE
}

/// Make an unrealized window a transparent layer over `monitor` that never takes the keyboard,
/// for drawing on the screen while the user keeps typing elsewhere. Returns false where the
/// compositor has no layer-shell.
pub fn ink(window: &impl IsA<gtk::Window>, name_space: &str, monitor: &gtk::gdk::Monitor) -> bool {
    if !supported(window) {
        return false;
    }
    let Ok(name_space) = CString::new(name_space) else {
        return false;
    };
    let pointer: *mut gtk::ffi::GtkWindow = window.as_ref().to_glib_none().0;
    // SAFETY: the window and monitor are alive, the window not yet realized; the namespace is
    // copied during the call.
    unsafe {
        gtk_layer_init_for_window(pointer);
        gtk_layer_set_namespace(pointer, name_space.as_ptr());
        gtk_layer_set_layer(pointer, LAYER_OVERLAY);
        gtk_layer_set_monitor(pointer, monitor.to_glib_none().0);
        for edge in EDGES {
            gtk_layer_set_anchor(pointer, edge, glib::ffi::GTRUE);
        }
        gtk_layer_set_exclusive_zone(pointer, -1);
        gtk_layer_set_keyboard_mode(pointer, KEYBOARD_NONE);
    }
    true
}

/// Make an unrealized window cover the focused monitor above every other surface, panels
/// included, and take the keyboard while shown. Returns false and leaves an ordinary window on
/// X11 or on compositors without layer-shell.
pub fn overlay(window: &impl IsA<gtk::Window>, name_space: &str) -> bool {
    if !supported(window) {
        return false;
    }
    let Ok(name_space) = CString::new(name_space) else {
        return false;
    };
    let window: *mut gtk::ffi::GtkWindow = window.as_ref().to_glib_none().0;
    // SAFETY: the window is alive and not yet realized, as gtk4-layer-shell requires; the
    // namespace is copied during the call.
    unsafe {
        gtk_layer_init_for_window(window);
        gtk_layer_set_namespace(window, name_space.as_ptr());
        gtk_layer_set_layer(window, LAYER_OVERLAY);
        for edge in EDGES {
            gtk_layer_set_anchor(window, edge, glib::ffi::GTRUE);
        }
        gtk_layer_set_exclusive_zone(window, -1);
        gtk_layer_set_keyboard_mode(window, KEYBOARD_EXCLUSIVE);
    }
    true
}
