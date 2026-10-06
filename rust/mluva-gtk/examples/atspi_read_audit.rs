//! Unshipped outgoing-request observer for private browser privacy checks.
//! Forward libdbus calls unchanged; record no replies or text content.
use std::{
    cell::RefCell,
    ffi::{CStr, c_char, c_int, c_void},
    fs::OpenOptions,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    sync::OnceLock,
};

type Send = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut u32) -> u32;
type Pending = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut *mut c_void, c_int) -> u32;
type Blocking = unsafe extern "C" fn(*mut c_void, *mut c_void, c_int, *mut c_void) -> *mut c_void;

#[link(name = "dbus-1")]
unsafe extern "C" {
    fn dbus_message_is_method_call(
        message: *mut c_void,
        interface: *const c_char,
        member: *const c_char,
    ) -> u32;
    fn dbus_message_get_args(message: *mut c_void, error: *mut c_void, ...) -> u32;
    fn dbus_bus_get_unique_name(connection: *mut c_void) -> *const c_char;
}

thread_local! {
    static ACTIVE: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

struct Forward;
impl Forward {
    fn observe(connection: *mut c_void, message: *mut c_void) -> Self {
        let nested = ACTIVE.with(|stack| {
            let mut stack = stack.borrow_mut();
            let nested = stack.contains(&(message as usize));
            stack.push(message as usize);
            nested
        });
        if !nested && !connection.is_null() && !message.is_null() {
            record(connection, message);
        }
        Self
    }
}
impl Drop for Forward {
    fn drop(&mut self) {
        ACTIVE.with(|stack| stack.borrow_mut().pop().unwrap());
    }
}

fn record(connection: *mut c_void, message: *mut c_void) {
    let Some(path) = std::env::var_os("MLUVA_TEXT_READ_AUDIT") else {
        return;
    };
    if unsafe {
        dbus_message_is_method_call(
            message,
            c"org.a11y.atspi.Text".as_ptr(),
            c"GetText".as_ptr(),
        )
    } == 0
    {
        return;
    }
    let (mut start, mut end) = (0_i32, 0_i32);
    let valid = unsafe {
        dbus_message_get_args(
            message,
            std::ptr::null_mut(),
            i32::from(b'i'),
            &mut start,
            i32::from(b'i'),
            &mut end,
            0,
        )
    } != 0;
    let peer = unsafe { dbus_bus_get_unique_name(connection).is_null() };
    let row = format!(
        "{{\"pid\":{},\"peer\":{peer},\"valid\":{valid},\"start\":{start},\"end\":{end}}}\n",
        std::process::id()
    );
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .unwrap()
        .write_all(row.as_bytes())
        .unwrap();
}

unsafe fn next<T: Copy>(name: &CStr) -> T {
    let symbol = unsafe { libc::dlsym(libc::RTLD_NEXT, name.as_ptr()) };
    assert!(!symbol.is_null(), "original libdbus symbol is missing");
    unsafe { std::mem::transmute_copy(&symbol) }
}

/// # Safety
/// The caller supplies valid libdbus connection, message and result pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_send(
    connection: *mut c_void,
    message: *mut c_void,
    serial: *mut u32,
) -> u32 {
    static NEXT: OnceLock<Send> = OnceLock::new();
    let next = NEXT.get_or_init(|| unsafe { next(c"dbus_connection_send") });
    let _forward = Forward::observe(connection, message);
    unsafe { next(connection, message, serial) }
}

/// # Safety
/// The caller supplies valid libdbus connection, message and pending-call pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_send_with_reply(
    connection: *mut c_void,
    message: *mut c_void,
    pending: *mut *mut c_void,
    timeout: c_int,
) -> u32 {
    static NEXT: OnceLock<Pending> = OnceLock::new();
    let next = NEXT.get_or_init(|| unsafe { next(c"dbus_connection_send_with_reply") });
    let _forward = Forward::observe(connection, message);
    unsafe { next(connection, message, pending, timeout) }
}

/// # Safety
/// The caller supplies valid libdbus connection, message and optional error pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dbus_connection_send_with_reply_and_block(
    connection: *mut c_void,
    message: *mut c_void,
    timeout: c_int,
    error: *mut c_void,
) -> *mut c_void {
    static NEXT: OnceLock<Blocking> = OnceLock::new();
    let next = NEXT.get_or_init(|| unsafe { next(c"dbus_connection_send_with_reply_and_block") });
    let _forward = Forward::observe(connection, message);
    unsafe { next(connection, message, timeout, error) }
}
