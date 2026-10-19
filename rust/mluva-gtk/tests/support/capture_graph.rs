//! Actual capture/workspace/preferences with unrelated outer actions left at their boundary.
use mluva_audio::catalog::PipeWireDeviceCatalog;
use mluva_gtk::{
    capture_preferences::{CapturePreferenceCallbacks, CapturePreferences},
    capture_view::{CaptureCallbacks, CapturePage},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
};
use mluva_workflows::services::ApplicationServices;
use std::rc::Rc;
pub fn graph(
    services: Rc<ApplicationServices>,
    resources: DocumentResources,
) -> (Rc<CapturePage>, Rc<CapturePreferences>) {
    graph_with_callbacks(
        services,
        resources,
        CaptureCallbacks {
            toggle_recording: Rc::new(|| {}),
            toast: Rc::new(|_| {}),
            retry_initialization: Rc::new(|| {}),
            announce: Rc::new(|_| {}),
        },
    )
}
pub fn graph_with_callbacks(
    services: Rc<ApplicationServices>,
    resources: DocumentResources,
    callbacks: CaptureCallbacks,
) -> (Rc<CapturePage>, Rc<CapturePreferences>) {
    let workspace = ConversationWorkspace::new(
        services.conversations.clone(),
        ConversationCallbacks {
            copy: Rc::new(|_| {}),
            open_archive: Rc::new(|| {}),
            rename: Rc::new(|_, _| false),
            delete: Rc::new(|_| false),
            merge: Rc::new(|_, _| false),
            continue_recording: Rc::new(|_| {}),
            capture_screenshot: None,
            edit_screenshot: Rc::new(|_| {}),
            remove_screenshot: Rc::new(|_| {}),
        },
        resources,
    )
    .unwrap();
    let config = services.config();
    workspace.set_config(config.clone()).unwrap();
    let page = CapturePage::new(workspace, config, callbacks).unwrap();
    let preferences = CapturePreferences::new(
        services,
        PipeWireDeviceCatalog::default(),
        None,
        CapturePreferenceCallbacks {
            changed: Rc::new(|_| {}),
            routes_changed: Rc::new(|_, _| {}),
            history_changed: Rc::new(|| {}),
            status: Rc::new(|_| {}),
            toast: Rc::new(|_| {}),
            export_diagnostics: Rc::new(|| {}),
        },
    );
    (page, preferences)
}
