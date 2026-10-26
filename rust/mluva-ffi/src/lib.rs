//! C ABI over the shared Mluva core for native desktop shells.
//!
//! Every call takes one UTF-8 JSON request, `{"method": …, "params": {…}}`, and returns one
//! JSON response, `{"ok": …}` or `{"error": "…"}`. Long work runs on an owned Tokio runtime
//! and reports through the single event handler. Credentials arrive per call, are held only
//! as `Secret` and never appear in a response or event.
mod bridge;

pub use bridge::{Bridge, Emitter};

use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    sync::{Arc, Mutex, OnceLock},
};

type EventHandler = extern "C" fn(context: *mut c_void, event: *const c_char);

/// The registered handler and its opaque context, stored as an address so it can cross threads.
static HANDLER: Mutex<Option<(EventHandler, usize)>> = Mutex::new(None);
static BRIDGE: OnceLock<Bridge> = OnceLock::new();

fn bridge() -> &'static Bridge {
    BRIDGE.get_or_init(|| {
        Bridge::new(Arc::new(|event: Value| {
            let Ok(text) = CString::new(event.to_string()) else {
                return;
            };
            if let Some((handler, context)) = *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) {
                handler(context as *mut c_void, text.as_ptr());
            }
        }))
    })
}

fn respond(value: Value) -> *mut c_char {
    CString::new(value.to_string())
        .unwrap_or_else(|_| c"{\"error\":\"The response contained a NUL byte.\"}".to_owned())
        .into_raw()
}

/// Run one request. The caller frees the returned string with `mluva_string_free`.
///
/// # Safety
/// `request` must be a valid NUL-terminated string or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mluva_call(request: *const c_char) -> *mut c_char {
    if request.is_null() {
        return respond(json!({"error": "No request was supplied."}));
    }
    // SAFETY: the caller guarantees a valid NUL-terminated string.
    let request = unsafe { CStr::from_ptr(request) };
    let parsed = request
        .to_str()
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let Some(request) = parsed else {
        return respond(json!({"error": "The request is not valid JSON."}));
    };
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    respond(match bridge().call(method, &params) {
        Ok(value) => json!({"ok": value}),
        Err(error) => json!({"error": error}),
    })
}

/// # Safety
/// `value` must come from `mluva_call` and be freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mluva_string_free(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: the pointer came from CString::into_raw in this library.
        drop(unsafe { CString::from_raw(value) });
    }
}

/// Register the event handler. It is called from runtime threads; the caller dispatches.
#[unsafe(no_mangle)]
pub extern "C" fn mluva_set_event_handler(handler: Option<EventHandler>, context: *mut c_void) {
    *HANDLER.lock().unwrap_or_else(|e| e.into_inner()) =
        handler.map(|handler| (handler, context as usize));
}

/// Queue 16 kHz, 16-bit little-endian mono PCM for a realtime session. Returns false once the
/// session can no longer take audio; the recording continues and recognition falls back.
///
/// # Safety
/// `session` must be a NUL-terminated string; `bytes` must point to `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mluva_realtime_audio(
    session: *const c_char,
    bytes: *const u8,
    length: usize,
) -> bool {
    if session.is_null() || (bytes.is_null() && length > 0) {
        return false;
    }
    // SAFETY: the caller guarantees a valid NUL-terminated string.
    let session = unsafe { CStr::from_ptr(session) };
    let frames: &[u8] = if length == 0 {
        &[]
    } else {
        // SAFETY: the caller guarantees `length` readable bytes at `bytes`.
        unsafe { std::slice::from_raw_parts(bytes, length) }
    };
    session
        .to_str()
        .is_ok_and(|session| bridge().realtime_audio(session, frames))
}
