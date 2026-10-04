//! The distribution's libatspi, confined to its GLib thread with owned GObject/boxed values.

use glib::translate::{from_glib_full, from_glib_none};
use glib::{Object, prelude::*};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_uint, c_void},
    marker::PhantomData,
    rc::Rc,
    sync::OnceLock,
    thread::ThreadId,
};

pub(crate) const ACTIVE: c_int = 1;
pub(crate) const EDITABLE: c_int = 7;
pub(crate) const FOCUSED: c_int = 12;
pub(crate) const PASSWORD_TEXT: c_int = 40;
const FOCUS_EVENT: &CStr = c"object:state-changed:focused";

/// libatspi maintains global proxy/cache/listener state without synchronization.
pub(crate) fn initialize() -> bool {
    static OWNER: OnceLock<ThreadId> = OnceLock::new();
    if *OWNER.get_or_init(|| std::thread::current().id()) != std::thread::current().id() {
        return false;
    }
    // Match the released binding: init's status does not substitute for actual query results.
    unsafe { ffi::atspi_init() };
    true
}

#[derive(Clone, PartialEq, Eq)]
struct Proxy(Object, PhantomData<Rc<()>>);

impl Proxy {
    // All get_*_iface methods return a referenced Accessible cast to its GType interface.
    unsafe fn full(pointer: *mut c_void) -> Option<Self> {
        (!pointer.is_null()).then(|| Self(unsafe { from_glib_full(pointer.cast()) }, PhantomData))
    }
    unsafe fn borrowed(pointer: *mut c_void) -> Option<Self> {
        (!pointer.is_null()).then(|| Self(unsafe { from_glib_none(pointer.cast()) }, PhantomData))
    }
    fn pointer(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Node(Proxy);
#[derive(Clone)]
pub(crate) struct Text(Proxy);
#[derive(Clone)]
pub(crate) struct EditableText(Proxy);

/// An error may accompany an owned result; acquire that result before checking this slot.
#[derive(Default)]
struct ErrorSlot(*mut glib::ffi::GError);
impl ErrorSlot {
    fn out(&mut self) -> *mut *mut glib::ffi::GError {
        &mut self.0
    }
    fn result<T>(&self, value: T) -> Option<T> {
        self.0.is_null().then_some(value)
    }
}
impl Drop for ErrorSlot {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { glib::ffi::g_error_free(self.0) };
        }
    }
}

impl Node {
    pub(crate) fn desktop() -> Option<Self> {
        unsafe { Proxy::full(ffi::atspi_get_desktop(0)) }.map(Self)
    }
    pub(crate) fn child_count(&self) -> Option<i32> {
        let mut error = ErrorSlot::default();
        let count = unsafe { ffi::atspi_accessible_get_child_count(self.0.pointer(), error.out()) };
        error.result(count)
    }
    pub(crate) fn child(&self, index: i32) -> Option<Self> {
        let mut error = ErrorSlot::default();
        let child = unsafe {
            Proxy::full(ffi::atspi_accessible_get_child_at_index(
                self.0.pointer(),
                index,
                error.out(),
            ))
        };
        error.result(child).flatten().map(Self)
    }
    pub(crate) fn process_id(&self) -> Option<u32> {
        let mut error = ErrorSlot::default();
        let pid = unsafe { ffi::atspi_accessible_get_process_id(self.0.pointer(), error.out()) };
        error.result(pid)
    }
    pub(crate) fn role(&self) -> Option<i32> {
        let mut error = ErrorSlot::default();
        let role = unsafe { ffi::atspi_accessible_get_role(self.0.pointer(), error.out()) };
        error.result(role)
    }
    pub(crate) fn has_state(&self, state: i32) -> Option<bool> {
        let states = unsafe { Proxy::full(ffi::atspi_accessible_get_state_set(self.0.pointer())) }?;
        Some(unsafe { ffi::atspi_state_set_contains(states.pointer(), state) } != 0)
    }
    pub(crate) fn text(&self) -> Option<Text> {
        unsafe { Proxy::full(ffi::atspi_accessible_get_text_iface(self.0.pointer())) }.map(Text)
    }
    pub(crate) fn editable_text(&self) -> Option<EditableText> {
        unsafe {
            Proxy::full(ffi::atspi_accessible_get_editable_text_iface(
                self.0.pointer(),
            ))
        }
        .map(EditableText)
    }
    pub(crate) fn grab_focus(&self) -> Option<bool> {
        let component =
            unsafe { Proxy::full(ffi::atspi_accessible_get_component_iface(self.0.pointer())) }?;
        let mut error = ErrorSlot::default();
        let focused = unsafe { ffi::atspi_component_grab_focus(component.pointer(), error.out()) };
        error.result(focused != 0)
    }
}

impl Text {
    pub(crate) fn caret(&self) -> Option<i32> {
        let mut error = ErrorSlot::default();
        let caret = unsafe { ffi::atspi_text_get_caret_offset(self.0.pointer(), error.out()) };
        error.result(caret)
    }
    pub(crate) fn set_caret(&self, offset: i32) -> Option<bool> {
        let mut error = ErrorSlot::default();
        let set =
            unsafe { ffi::atspi_text_set_caret_offset(self.0.pointer(), offset, error.out()) };
        error.result(set != 0)
    }
    pub(crate) fn selection_count(&self) -> Option<i32> {
        let mut error = ErrorSlot::default();
        let count = unsafe { ffi::atspi_text_get_n_selections(self.0.pointer(), error.out()) };
        error.result(count)
    }
    pub(crate) fn selection(&self) -> Option<(i32, i32)> {
        let mut error = ErrorSlot::default();
        let range = unsafe { ffi::atspi_text_get_selection(self.0.pointer(), 0, error.out()) };
        let offsets = if range.is_null() {
            None
        } else {
            let offsets = unsafe { ((*range).start_offset, (*range).end_offset) };
            unsafe { glib::gobject_ffi::g_boxed_free(ffi::atspi_range_get_type(), range.cast()) };
            Some(offsets)
        };
        error.result(offsets).flatten()
    }
    pub(crate) fn get_text(&self, start: i32, end: i32) -> Option<String> {
        let mut error = ErrorSlot::default();
        let text = unsafe { ffi::atspi_text_get_text(self.0.pointer(), start, end, error.out()) };
        let value = if text.is_null() {
            None
        } else {
            let value = unsafe { CStr::from_ptr(text) }
                .to_str()
                .ok()
                .map(str::to_owned);
            unsafe { glib::ffi::g_free(text.cast()) };
            value
        };
        error.result(value).flatten()
    }
    pub(crate) fn add_selection(&self, start: i32, end: i32) -> Option<bool> {
        let mut error = ErrorSlot::default();
        let added =
            unsafe { ffi::atspi_text_add_selection(self.0.pointer(), start, end, error.out()) };
        error.result(added != 0)
    }
    pub(crate) fn set_selection(&self, start: i32, end: i32) -> Option<bool> {
        let mut error = ErrorSlot::default();
        let set =
            unsafe { ffi::atspi_text_set_selection(self.0.pointer(), 0, start, end, error.out()) };
        error.result(set != 0)
    }
}

impl EditableText {
    pub(crate) fn delete(&self, start: i32, end: i32) -> Option<bool> {
        let mut error = ErrorSlot::default();
        let deleted = unsafe {
            ffi::atspi_editable_text_delete_text(self.0.pointer(), start, end, error.out())
        };
        error.result(deleted != 0)
    }
    pub(crate) fn insert(&self, position: i32, text: &str) -> Option<bool> {
        let bytes = i32::try_from(text.len()).ok()?;
        // The released C binding sends the NUL-terminated prefix, while retaining the original
        // UTF-8 byte length and its caller's character-based caret update.
        let text = CString::new(text.split('\0').next()?).ok()?;
        let mut error = ErrorSlot::default();
        let inserted = unsafe {
            ffi::atspi_editable_text_insert_text(
                self.0.pointer(),
                position,
                text.as_ptr(),
                bytes,
                error.out(),
            )
        };
        error.result(inserted != 0)
    }
}

type FocusCallback = Rc<dyn Fn(Option<Node>, i32)>;
type CallbackData = glib::thread_guard::ThreadGuard<FocusCallback>;
pub(crate) struct FocusListener {
    object: Proxy,
    // libatspi 2.60.6 disables its destroy callback. Own the stable userdata ourselves.
    _callback: Box<CallbackData>,
}

impl FocusListener {
    pub(crate) fn new(callback: FocusCallback) -> Option<Self> {
        let mut callback = Box::new(CallbackData::new(callback));
        let object = unsafe {
            Proxy::full(ffi::atspi_event_listener_new(
                focus_event,
                (&mut *callback as *mut CallbackData).cast(),
                None,
            ))
        }?;
        let listener = Self {
            object,
            _callback: callback,
        };
        let mut error = ErrorSlot::default();
        let registered = unsafe {
            ffi::atspi_event_listener_register(
                listener.object.pointer(),
                FOCUS_EVENT.as_ptr(),
                error.out(),
            )
        };
        error
            .result(registered != 0)
            .filter(|registered| *registered)
            .map(|_| listener)
    }
}

impl Drop for FocusListener {
    fn drop(&mut self) {
        let mut error = ErrorSlot::default();
        // Deregistration removes the local entry before making the remote call. During dispatch
        // it is marked for removal and skipped; a running callback already owns an Rc clone.
        unsafe {
            ffi::atspi_event_listener_deregister(
                self.object.pointer(),
                FOCUS_EVENT.as_ptr(),
                error.out(),
            );
        }
        // The owned userdata is dropped only after this method returns.
    }
}

struct OwnedEvent(*mut ffi::Event);
impl Drop for OwnedEvent {
    fn drop(&mut self) {
        unsafe { glib::gobject_ffi::g_boxed_free(ffi::atspi_event_get_type(), self.0.cast()) };
    }
}

unsafe extern "C" fn focus_event(event: *mut ffi::Event, userdata: *mut c_void) {
    if event.is_null() {
        return;
    }
    let event = OwnedEvent(event);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Check the dispatch thread before touching an Rc, just as GTK's native callbacks do.
        // Clone before any libatspi call: a nested GLib dispatch may close the listener.
        let callback = unsafe { &*userdata.cast::<CallbackData>() }
            .get_ref()
            .clone();
        let source = unsafe { Proxy::borrowed((*event.0).source) }.map(Node);
        let detail = unsafe { (*event.0).detail1 };
        callback(source, detail);
    }));
}

mod ffi {
    use super::*;
    #[repr(C)]
    pub(super) struct Range {
        pub start_offset: c_int,
        pub end_offset: c_int,
    }
    #[repr(C)]
    pub(super) struct Event {
        pub event_type: *mut c_char,
        pub source: *mut c_void,
        pub detail1: c_int,
        pub detail2: c_int,
        pub any_data: glib::gobject_ffi::GValue,
        pub sender: *mut c_void,
    }
    type Error = *mut *mut glib::ffi::GError;
    #[link(name = "atspi")]
    unsafe extern "C" {
        pub fn atspi_init() -> c_int;
        pub fn atspi_get_desktop(index: c_int) -> *mut c_void;
        pub fn atspi_accessible_get_child_count(obj: *mut c_void, error: Error) -> c_int;
        pub fn atspi_accessible_get_child_at_index(
            obj: *mut c_void,
            index: c_int,
            error: Error,
        ) -> *mut c_void;
        pub fn atspi_accessible_get_process_id(obj: *mut c_void, error: Error) -> c_uint;
        pub fn atspi_accessible_get_role(obj: *mut c_void, error: Error) -> c_int;
        pub fn atspi_accessible_get_state_set(obj: *mut c_void) -> *mut c_void;
        pub fn atspi_state_set_contains(obj: *mut c_void, state: c_int) -> c_int;
        pub fn atspi_accessible_get_text_iface(obj: *mut c_void) -> *mut c_void;
        pub fn atspi_accessible_get_editable_text_iface(obj: *mut c_void) -> *mut c_void;
        pub fn atspi_accessible_get_component_iface(obj: *mut c_void) -> *mut c_void;
        pub fn atspi_component_grab_focus(obj: *mut c_void, error: Error) -> c_int;
        pub fn atspi_text_get_caret_offset(obj: *mut c_void, error: Error) -> c_int;
        pub fn atspi_text_set_caret_offset(obj: *mut c_void, offset: c_int, error: Error) -> c_int;
        pub fn atspi_text_get_n_selections(obj: *mut c_void, error: Error) -> c_int;
        pub fn atspi_text_get_selection(obj: *mut c_void, index: c_int, error: Error)
        -> *mut Range;
        pub fn atspi_range_get_type() -> glib::ffi::GType;
        pub fn atspi_text_get_text(
            obj: *mut c_void,
            start: c_int,
            end: c_int,
            error: Error,
        ) -> *mut c_char;
        pub fn atspi_text_add_selection(
            obj: *mut c_void,
            start: c_int,
            end: c_int,
            error: Error,
        ) -> c_int;
        pub fn atspi_text_set_selection(
            obj: *mut c_void,
            index: c_int,
            start: c_int,
            end: c_int,
            error: Error,
        ) -> c_int;
        pub fn atspi_editable_text_delete_text(
            obj: *mut c_void,
            start: c_int,
            end: c_int,
            error: Error,
        ) -> c_int;
        pub fn atspi_editable_text_insert_text(
            obj: *mut c_void,
            position: c_int,
            text: *const c_char,
            bytes: c_int,
            error: Error,
        ) -> c_int;
        pub fn atspi_event_listener_new(
            callback: unsafe extern "C" fn(*mut Event, *mut c_void),
            userdata: *mut c_void,
            destroy: Option<unsafe extern "C" fn(*mut c_void)>,
        ) -> *mut c_void;
        pub fn atspi_event_listener_register(
            obj: *mut c_void,
            event: *const c_char,
            error: Error,
        ) -> c_int;
        pub fn atspi_event_listener_deregister(
            obj: *mut c_void,
            event: *const c_char,
            error: Error,
        ) -> c_int;
        pub fn atspi_event_get_type() -> glib::ffi::GType;
    }
}
