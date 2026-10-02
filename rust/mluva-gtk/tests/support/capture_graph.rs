//! Actual capture/workspace/preferences with unrelated outer actions left at their boundary.
use mluva_audio::catalog::PipeWireDeviceCatalog;
use mluva_gtk::{
    capture_preferences::{CapturePreferenceCallbacks, CapturePreferences},
    capture_view::{CaptureCallbacks, CapturePage},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    rewrite_settings::RewriteSettings,
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
            apply_live_settings: Rc::new(|_| false),
            toast: Rc::new(|_| {}),
            open_prompt: Rc::new(|_| {}),
            retry_initialization: Rc::new(|| {}),
            accept_command: Rc::new(|| {}),
            discard_command: Rc::new(|| {}),
            copy_scratchpad: Rc::new(|| {}),
            delete_scratchpad: Rc::new(|| {}),
            output_changed: Rc::new(|_| {}),
            live_draft_edited: Rc::new(|| {}),
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
            rewrite: Rc::new(|_| {}),
            paste: Rc::new(|_| {}),
            open_archive: Rc::new(|| {}),
            save_prompt: Rc::new(|_| {}),
            cancel_rewrite: Rc::new(|| {}),
            rename: Rc::new(|_, _| false),
            delete: Rc::new(|_| false),
            merge: Rc::new(|_, _| false),
            continue_recording: Rc::new(|_| {}),
            capture_screenshot: None,
            edit_screenshot: Rc::new(|_| {}),
            remove_screenshot: Rc::new(|_| {}),
            edit_prompt: Rc::new(|_| {}),
        },
        resources,
    )
    .unwrap();
    let config = services.config();
    workspace.set_config(config.clone()).unwrap();
    let rewrite = RewriteSettings::new(config.clone(), Rc::new(|| {}), Rc::new(|_, _, _| {}));
    let page = CapturePage::new(workspace, rewrite, config, callbacks).unwrap();
    let preferences = CapturePreferences::new(
        services,
        PipeWireDeviceCatalog::default(),
        None,
        false,
        CapturePreferenceCallbacks {
            changed: Rc::new(|_| {}),
            summary_changed: Rc::new(|| {}),
            routes_changed: Rc::new(|_, _| {}),
            history_changed: Rc::new(|| {}),
            status: Rc::new(|_| {}),
            toast: Rc::new(|_| {}),
            manage_styles: Rc::new(|| {}),
            export_diagnostics: Rc::new(|| {}),
        },
    );
    (page, preferences)
}
