//! Load the optional WebKitGTK 6.0 renderer only when a sketch needs it.
//! GObject types keep their implementation for the process lifetime, so a
//! successfully loaded library must never be unloaded while GTK is running.

use glib::translate::{ToGlibPtr, from_glib, from_glib_full, from_glib_none};
use gtk::prelude::*;
use std::ffi::{CStr, c_char, c_void};
use std::ptr;
use std::sync::OnceLock;

type Pointer = *mut c_void;

// These signatures are the stable WebKitGTK 6.0 / JavaScriptCore C ABI. Opaque
// WebKit pointers are used only during their owning GObject's lifetime.
struct Api {
    web_type: unsafe extern "C" fn() -> glib::ffi::GType,
    manager_new: unsafe extern "C" fn() -> Pointer,
    manager_register: unsafe extern "C" fn(Pointer, *const c_char, *const c_char) -> i32,
    session_new: unsafe extern "C" fn() -> Pointer,
    settings: unsafe extern "C" fn(Pointer) -> Pointer,
    set_background: unsafe extern "C" fn(Pointer, *const gtk::gdk::ffi::GdkRGBA),
    load_html: unsafe extern "C" fn(Pointer, *const c_char, *const c_char),
    evaluate: unsafe extern "C" fn(
        Pointer,
        *const c_char,
        isize,
        *const c_char,
        *const c_char,
        *mut gio::ffi::GCancellable,
        gio::ffi::GAsyncReadyCallback,
        Pointer,
    ),
    terminate: unsafe extern "C" fn(Pointer),
    value_string: unsafe extern "C" fn(Pointer) -> *mut c_char,
    navigation_action: unsafe extern "C" fn(Pointer) -> Pointer,
    navigation_request: unsafe extern "C" fn(Pointer) -> Pointer,
    request_uri: unsafe extern "C" fn(Pointer) -> *const c_char,
    ignore_decision: unsafe extern "C" fn(Pointer),
    _library: libloading::Library,
}

impl Api {
    fn load() -> Result<Self, libloading::Error> {
        // SAFETY: Load the system ABI, resolving every required symbol before
        // registering any GObject type. The retained library outlives all views.
        unsafe {
            let library = libloading::Library::new("libwebkitgtk-6.0.so.4")?;
            Ok(Self {
                web_type: *library.get(c"webkit_web_view_get_type".to_bytes_with_nul())?,
                manager_new: *library
                    .get(c"webkit_user_content_manager_new".to_bytes_with_nul())?,
                manager_register: *library.get(
                    c"webkit_user_content_manager_register_script_message_handler"
                        .to_bytes_with_nul(),
                )?,
                session_new: *library
                    .get(c"webkit_network_session_new_ephemeral".to_bytes_with_nul())?,
                settings: *library.get(c"webkit_web_view_get_settings".to_bytes_with_nul())?,
                set_background: *library
                    .get(c"webkit_web_view_set_background_color".to_bytes_with_nul())?,
                load_html: *library.get(c"webkit_web_view_load_html".to_bytes_with_nul())?,
                evaluate: *library
                    .get(c"webkit_web_view_evaluate_javascript".to_bytes_with_nul())?,
                terminate: *library
                    .get(c"webkit_web_view_terminate_web_process".to_bytes_with_nul())?,
                value_string: *library.get(c"jsc_value_to_string".to_bytes_with_nul())?,
                navigation_action: *library.get(
                    c"webkit_navigation_policy_decision_get_navigation_action".to_bytes_with_nul(),
                )?,
                navigation_request: *library
                    .get(c"webkit_navigation_action_get_request".to_bytes_with_nul())?,
                request_uri: *library.get(c"webkit_uri_request_get_uri".to_bytes_with_nul())?,
                ignore_decision: *library
                    .get(c"webkit_policy_decision_ignore".to_bytes_with_nul())?,
                _library: library,
            })
        }
    }
}

#[derive(Clone)]
pub(crate) struct WebView {
    pub widget: gtk::Widget,
    manager: glib::Object,
    api: &'static Api,
}

impl WebView {
    pub fn new() -> Option<Self> {
        assert!(gtk::is_initialized_main_thread());
        static API: OnceLock<Option<Api>> = OnceLock::new();
        let api = API.get_or_init(|| Api::load().ok()).as_ref()?;
        // SAFETY: Constructors return owned GObjects of the resolved ABI. GTK
        // objects and their signal callbacks stay on this main thread.
        let (manager, session, web_type): (glib::Object, glib::Object, glib::Type) = unsafe {
            (
                from_glib_full((api.manager_new)().cast::<glib::gobject_ffi::GObject>()),
                from_glib_full((api.session_new)().cast::<glib::gobject_ffi::GObject>()),
                from_glib((api.web_type)()),
            )
        };
        // SAFETY: Manager is retained locally and the handler names live for
        // the call; NULL selects the default JavaScript world.
        if unsafe {
            (api.manager_register)(manager.as_ptr().cast(), c"rendered".as_ptr(), ptr::null())
        } == 0
        {
            return None;
        }
        let widget = glib::Object::builder_with_type(web_type)
            .property("network-session", &session)
            .property("user-content-manager", &manager)
            .build()
            .downcast::<gtk::Widget>()
            .ok()?;
        // SAFETY: WebKit's settings property is write-only. Its C getter lends
        // the live settings object; from_glib_none retains it for these writes.
        let settings: Option<glib::Object> = unsafe {
            from_glib_none(
                (api.settings)(widget.as_ptr().cast()).cast::<glib::gobject_ffi::GObject>(),
            )
        };
        if let Some(settings) = settings {
            settings.set_property("enable-html5-local-storage", false);
            settings.set_property("enable-page-cache", false);
        }
        let color = gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0);
        // SAFETY: This widget has the WebKitWebView GType; color is borrowed for
        // the duration of a synchronous call.
        unsafe { (api.set_background)(widget.as_ptr().cast(), color.to_glib_none().0) };
        Some(Self {
            widget,
            manager,
            api,
        })
    }

    pub fn connect_rendered(&self, callback: impl Fn(&str) + 'static) {
        let api = self.api;
        self.manager
            .connect_local("script-message-received::rendered", false, move |values| {
                let value = values[1].get::<glib::Object>().ok()?;
                // SAFETY: This signal supplies a live JSCValue. Its UTF-8
                // conversion is newly allocated and freed by GString.
                let text: Option<glib::GString> =
                    unsafe { from_glib_full((api.value_string)(value.as_ptr().cast())) };
                if let Some(text) = text {
                    callback(&text);
                }
                None
            });
    }

    pub fn connect_loaded(&self, callback: impl Fn() + 'static) {
        self.widget
            .connect_local("load-changed", false, move |values| {
                // WEBKIT_LOAD_FINISHED = 3 in the WebKitGTK 6.0 ABI.
                if glib::EnumValue::from_value(&values[1])
                    .is_some_and(|(_, value)| value.value() == 3)
                {
                    callback();
                }
                None
            });
    }

    pub fn connect_terminated(&self, callback: impl Fn() + 'static) {
        self.widget
            .connect_local("web-process-terminated", false, move |_| {
                callback();
                None
            });
    }

    pub fn restrict_navigation(&self, base: String) {
        let api = self.api;
        self.widget
            .connect_local("decide-policy", false, move |values| {
                // NavigationAction = 0; NewWindowAction = 1. Resource-response
                // decisions keep their normal behavior, as in the released view.
                if !glib::EnumValue::from_value(&values[2])
                    .is_some_and(|(_, value)| matches!(value.value(), 0 | 1))
                {
                    return Some(false.to_value());
                }
                let decision = values[1]
                    .get::<glib::Object>()
                    .expect("WebKit policy decision");
                // SAFETY: The signal owns the navigation decision and its borrowed
                // action/request/URI until this synchronous callback returns.
                let blocked = unsafe {
                    let action = (api.navigation_action)(decision.as_ptr().cast());
                    let request = if action.is_null() {
                        ptr::null_mut()
                    } else {
                        (api.navigation_request)(action)
                    };
                    let uri = if request.is_null() {
                        ptr::null()
                    } else {
                        (api.request_uri)(request)
                    };
                    let allowed = !uri.is_null() && {
                        let bytes = CStr::from_ptr(uri).to_bytes();
                        bytes == b"about:blank" || bytes == base.as_bytes()
                    };
                    if !allowed {
                        (api.ignore_decision)(decision.as_ptr().cast());
                    }
                    !allowed
                };
                Some(blocked.to_value())
            });
    }

    pub fn load_html(&self, html: &str, base: &str) {
        // SAFETY: WebView and UTF-8 strings remain valid for this call.
        unsafe {
            (self.api.load_html)(
                self.widget.as_ptr().cast(),
                html.to_glib_none().0,
                base.to_glib_none().0,
            );
        }
    }

    pub fn evaluate(&self, script: &str) {
        // SAFETY: WebView and script remain valid for the call. WebKit owns the
        // asynchronous work. Like the reference, no result callback is needed;
        // the registered message signal supplies the render result.
        unsafe {
            (self.api.evaluate)(
                self.widget.as_ptr().cast(),
                script.to_glib_none().0,
                -1,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                None,
                ptr::null_mut(),
            );
        }
    }

    pub fn terminate(&self) {
        // SAFETY: WebView is a retained live GObject of the resolved type.
        unsafe { (self.api.terminate)(self.widget.as_ptr().cast()) };
    }
}
