//! External input for disposable X11 GUI checks. Never open a live desktop display.

use std::path::PathBuf;

#[link(name = "X11")]
unsafe extern "C" {
    fn XOpenDisplay(name: *const libc::c_char) -> *mut libc::c_void;
    fn XCloseDisplay(display: *mut libc::c_void) -> libc::c_int;
    fn XDefaultRootWindow(display: *mut libc::c_void) -> libc::c_ulong;
    fn XQueryPointer(
        display: *mut libc::c_void,
        window: libc::c_ulong,
        root: *mut libc::c_ulong,
        child: *mut libc::c_ulong,
        root_x: *mut libc::c_int,
        root_y: *mut libc::c_int,
        window_x: *mut libc::c_int,
        window_y: *mut libc::c_int,
        mask: *mut libc::c_uint,
    ) -> libc::c_int;
    fn XSync(display: *mut libc::c_void, discard: libc::c_int) -> libc::c_int;
}
#[link(name = "Xtst")]
unsafe extern "C" {
    fn XTestFakeMotionEvent(
        display: *mut libc::c_void,
        screen: libc::c_int,
        x: libc::c_int,
        y: libc::c_int,
        delay: libc::c_ulong,
    ) -> libc::c_int;
    fn XTestFakeButtonEvent(
        display: *mut libc::c_void,
        button: libc::c_uint,
        pressed: libc::c_int,
        delay: libc::c_ulong,
    ) -> libc::c_int;
}

fn main() {
    let root = std::env::var_os("OFFSCREEN_SESSION_ROOT")
        .map(PathBuf::from)
        .expect("private desktop helper required")
        .canonicalize()
        .unwrap();
    assert!(
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&root)
    );
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(
        std::env::var("AT_SPI_BUS_ADDRESS")
            .unwrap()
            .starts_with("unix:abstract=offscreen-atspi-")
    );
    let display_name = std::env::var("DISPLAY").unwrap();
    assert!(display_name.starts_with(':'));
    // The helper's Xauthority cookie proves ownership of the ephemeral Xvfb display.
    let authority = std::env::var_os("XAUTHORITY")
        .map(PathBuf::from)
        .expect("private Xvfb authority required");
    assert!(authority.is_file() && authority.canonicalize().unwrap().starts_with(&root));
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(args.len(), 2);
    let x: i32 = args[0].parse().unwrap();
    let y: i32 = args[1].parse().unwrap();
    assert!((0..8192).contains(&x) && (0..8192).contains(&y));
    // SAFETY: only the validated private display is opened; all pointers reference initialized stack storage.
    unsafe {
        let display = XOpenDisplay(std::ptr::null());
        assert!(!display.is_null());
        assert_ne!(XTestFakeMotionEvent(display, -1, x, y, 0), 0);
        XSync(display, 0);
        assert_ne!(XTestFakeButtonEvent(display, 1, 1, 0), 0);
        assert_ne!(XTestFakeButtonEvent(display, 1, 0, 25), 0);
        XSync(display, 0);
        let (
            mut root_window,
            mut child,
            mut actual_x,
            mut actual_y,
            mut window_x,
            mut window_y,
            mut mask,
        ) = (0, 0, 0, 0, 0, 0, 0);
        assert_ne!(
            XQueryPointer(
                display,
                XDefaultRootWindow(display),
                &mut root_window,
                &mut child,
                &mut actual_x,
                &mut actual_y,
                &mut window_x,
                &mut window_y,
                &mut mask
            ),
            0
        );
        assert_eq!((actual_x, actual_y), (x, y));
        XCloseDisplay(display);
        println!("{{\"private_pointer\":[{x},{y}]}}");
    }
}
