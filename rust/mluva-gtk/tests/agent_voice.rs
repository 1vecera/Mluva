//! Full desktop graph with private PCM, HTTP and Herdr peers. No host input or audio.
use adw::prelude::*;
use mluva_core::{
    agent_voice::{RoutePhase, VoiceSettings},
    config::{AppConfig, AppPaths},
};
use mluva_gtk::{
    application::{ApplicationDesktop, ApplicationPlatform},
    async_runtime::DesktopRuntime,
    document_layout::DocumentResources,
    theme::ThemeController,
};
use mluva_workflows::{
    capture::CapturePhase,
    services::{ApplicationServices, NativeBinaries},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
#[track_caller]
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "private application did not settle"
        );
        drain();
        thread::sleep(Duration::from_millis(5));
    }
    drain();
}
fn settle(ms: u64) {
    let start = Instant::now();
    while start.elapsed().as_millis() < ms as u128 {
        drain();
        thread::sleep(Duration::from_millis(5));
    }
}
struct Peer(Child);
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wire(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn sent(path: &Path) -> Vec<Value> {
    wire(path)
        .into_iter()
        .filter(|r| r["method"] == "agent.prompt_guarded")
        .collect()
}
fn state(path: &Path, value: &Value) {
    let tmp = path.with_extension("next");
    fs::write(&tmp, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(tmp, path).unwrap();
}
fn screenshot(owner: &ApplicationDesktop, root: &Path, name: &str, width: i32) {
    owner.shell.window.set_default_size(width, 760);
    settle(100);
    let output = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", "^Mluva$"])
        .output()
        .unwrap();
    let windows = String::from_utf8(output.stdout).unwrap();
    let id = windows.lines().next().unwrap();
    Command::new("xdotool")
        .args(["windowsize", id, &width.to_string(), "760"])
        .status()
        .unwrap();
    settle(130);
    assert!(
        Command::new("import")
            .args(["-window", id])
            .arg(root.join(name))
            .status()
            .unwrap()
            .success()
    );
    assert!(owner.agent_voice.view.widget.width() <= owner.shell.window.width());
}
#[test]
#[ignore = "requires private GTK, network, devices and fixture peers"]
fn toggle_route_cancel_edit_replace_and_uncertain_delivery_in_the_real_application() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    assert!(PathBuf::from(std::env::var_os("HOME").unwrap()).starts_with(&root));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/snd", "/dev/input", "/dev/uinput"] {
        assert!(!Path::new(path).exists());
    }
    let tools = root.join("application-tools");
    fs::create_dir(&tools).unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    symlink(target.join("audio-fixture-peer"), tools.join("pw-record")).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-application.json")).unwrap();
    fs::write(
        tools.join("test-config.json"),
        serde_json::to_vec(&fixture["cases"][0]["pcm"]).unwrap(),
    )
    .unwrap();
    let text = "Synthetic message: verify the selected agent only.";
    let responses: Vec<_> = (0..9).map(|_| json!({"route":"speech", "status":200, "delay_ms":160, "payload":{"text":text, "language_code":"eng"}})).collect();
    let mut speech = http::Peer::new(&responses);
    let paths = AppPaths {
        config: root.join("config/mluva"),
        data: root.join("data/mluva"),
        runtime: root.join("runtime/mluva"),
    };
    let config = AppConfig {
        transcription_provider: "litellm".into(),
        transcription_base_url: format!("{}/v1", speech.address),
        transcription_api_key_env: "AGENT_VOICE_SYNTHETIC_KEY".into(),
        transcription_remote_model: Some("whisper".into()),
        welcome_completed: true,
        ..Default::default()
    };
    config.save(&paths.config.join("config.json")).unwrap();
    let socket = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap()).join("voice.sock");
    let peer_state = root.join("herdr-state.json");
    let audit = root.join("herdr-wire.jsonl");
    let mut data = json!({"agents": [
        {"guard": "a".repeat(64), "agent": {"name":"Chief of staff", "pane_id":"w1:p1", "agent":"codex", "agent_status":"working", "cwd":"/synthetic/product"}},
        {"guard": "b".repeat(64), "agent": {"name":"Chief of staff", "pane_id":"w1:p2", "agent":"claude", "agent_status":"idle", "cwd":"/synthetic/research"}}
    ]});
    state(&peer_state, &data);
    let mut peer = Peer(
        Command::new(env!("CARGO_BIN_EXE_herdr-voice-fixture-peer"))
            .arg(&socket)
            .arg(&peer_state)
            .arg(&audit)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut ready = String::new();
    BufReader::new(peer.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "herdr-voice-peer-ready");
    VoiceSettings {
        enabled: true,
        socket: Some(socket.to_string_lossy().into()),
        ..Default::default()
    }
    .save(&paths.config.join("agent-voice.json"), "F9")
    .unwrap();
    gtk::init().unwrap();
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme =
        ThemeController::apply(root.join("theme"), resources.font.parent().unwrap()).unwrap();
    let app = adw::Application::builder()
        .application_id("com.mluva.AgentVoiceVerification")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let runtime = DesktopRuntime::new().unwrap();
    let services = ApplicationServices::open(paths.clone()).unwrap();
    let owner = ApplicationDesktop::new(
        &app,
        services.clone(),
        runtime,
        resources,
        NativeBinaries {
            asr_worker: target.join("mluva-asr-worker"),
            audio_cleanup: target.join("mluva-audio-cleanup"),
            screenshot_editor: target.join("mluva-screenshot-editor"),
        },
        ApplicationPlatform {
            compact_recording: Rc::new(|_| {}),
            close: Rc::new(|| {}),
        },
    )
    .unwrap();
    owner.shell.present();
    until(|| {
        owner.capture.page.record_button.is_sensitive()
            && owner
                .agent_voice
                .view
                .recipients
                .model()
                .is_some_and(|m| m.n_items() == 3)
    });
    owner.agent_voice.view.recipients.set_selected(1);
    settle(50);
    gtk::gdk::Display::default()
        .unwrap()
        .clipboard()
        .set_text("Clipboard must remain unchanged");
    screenshot(&owner, &root, "agent-ready-laptop.png", 1100);
    let window_ids = Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", "^Mluva$"])
        .output()
        .unwrap();
    let window_ids = String::from_utf8(window_ids.stdout).unwrap();
    assert!(
        Command::new("xdotool")
            .args([
                "windowactivate",
                "--sync",
                window_ids.lines().next().unwrap()
            ])
            .status()
            .unwrap()
            .success()
    );
    settle(100);
    owner.settings.show();
    assert!(owner.settings.view.set_visible_page_name("capture"));
    screenshot(&owner, &root, "agent-settings-laptop.png", 1100);
    owner.shell.navigate("capture");
    // Saved opt-in controls both the native panel and the private persisted settings.
    owner.agent_voice.view.enabled.set_active(false);
    assert!(!owner.agent_voice.view.widget.is_visible());
    assert!(
        !VoiceSettings::load(&paths.config.join("agent-voice.json"))
            .unwrap()
            .enabled
    );
    owner.agent_voice.view.enabled.set_active(true);
    assert!(owner.agent_voice.view.widget.is_visible());
    assert!(
        VoiceSettings::load(&paths.config.join("agent-voice.json"))
            .unwrap()
            .enabled
    );
    // Settings save themselves; shortcuts share the normal Capture page.
    owner.agent_voice.view.hold_key.set_selected(11);
    assert_eq!(
        VoiceSettings::load(&paths.config.join("agent-voice.json"))
            .unwrap()
            .hold_key,
        "F12"
    );
    owner.settings.capture.recording_key.set_selected(11);
    assert_eq!(
        services.config().global_recording_key,
        "F9",
        "Duplicate shortcuts must be rejected"
    );
    owner.settings.workspace.appearance.opacity.set_value(71.0);
    until(|| services.config().widget_opacity == 71);
    assert_eq!(
        AppConfig::load(&paths.config.join("config.json"))
            .unwrap()
            .widget_opacity,
        71
    );
    owner.settings.capture.language.set_selected(2);
    assert_eq!(services.config().language_code, "ces");
    owner.settings.providers.speech.chunk_row.set_value(15.0);
    until(|| services.config().transcription_chunk_seconds == 15);
    assert_eq!(
        services.config().language_code,
        "ces",
        "Autosaving a provider field must preserve changes made elsewhere"
    );
    assert_eq!(
        AppConfig::load(&paths.config.join("config.json"))
            .unwrap()
            .transcription_chunk_seconds,
        15
    );
    owner
        .settings
        .providers
        .speech
        .endpoint_entry
        .set_text("invalid URL");
    settle(500);
    assert_eq!(
        services.config().transcription_base_url,
        speech.address.to_string() + "/v1"
    );
    owner
        .settings
        .providers
        .speech
        .endpoint_entry
        .set_text(&services.config().transcription_base_url);
    until(|| owner.capture.page.record_button.is_sensitive());
    let record = || {
        let _ = fs::remove_file(tools.join("raw.ready.json"));
        owner.agent_voice.view.record.emit_clicked();
        until(|| owner.capture.phase() == Some(CapturePhase::Recording));
        settle(60);
    };
    let finish = || {
        owner.agent_voice.view.record.emit_clicked();
        until(|| owner.capture.phase().is_none());
    };
    // Full capture uses the selected provider, no clipboard delivery, and one guarded send.
    record();
    // Normal main-thread work must not be confused with losing a held shortcut.
    thread::sleep(Duration::from_millis(1250));
    drain();
    assert_eq!(
        owner.capture.phase(),
        Some(CapturePhase::Recording),
        "A brief UI stall interrupted agent dictation"
    );
    screenshot(&owner, &root, "agent-recording-laptop.png", 1100);
    finish();
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Grace { .. }
    ));
    screenshot(&owner, &root, "agent-countdown-laptop.png", 1100);
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Sent
    });
    assert_eq!(sent(&audit).len(), 1);
    assert_eq!(sent(&audit)[0]["params"]["text"], text);
    assert_eq!(sent(&audit)[0]["params"]["target"], "w1:p1");
    let attempt = owner.agent_voice.route.borrow().attempt.clone().unwrap();
    assert!(attempt.submitted_ms.unwrap() - attempt.final_ms.unwrap() >= 2000);
    assert_eq!(services.config().transcription_provider, "litellm");
    // Keep draft during recording; editing and saving do not alter immutable recognition.
    record();
    owner.agent_voice.view.keep.emit_clicked();
    until(|| owner.capture.phase().is_none());
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Draft(_)
    ));
    owner.capture.page.workspace.documents()[0]
        .buffer()
        .set_text("Edited synthetic request.");
    until(|| {
        !owner
            .capture
            .page
            .workspace
            .has_unsaved_edits(&owner.capture.page.workspace.entry().unwrap().identifier)
    });
    let history_id = owner
        .agent_voice
        .route
        .borrow()
        .attempt
        .as_ref()
        .unwrap()
        .history_id
        .clone()
        .unwrap();
    let entry = services.history.find(&history_id).unwrap();
    assert_eq!(entry.raw_text, text);
    assert_eq!(
        services.conversations.source_text(&entry, false).unwrap(),
        "Edited synthetic request."
    );
    screenshot(&owner, &root, "agent-draft-narrow.png", 420);
    settle(2200);
    assert_eq!(sent(&audit).len(), 1);
    // Explicit sending re-arms this editor, so a subsequent edit cancels that new attempt too.
    owner.agent_voice.view.send_draft.emit_clicked();
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Grace { .. }
    ));
    owner.capture.page.workspace.documents()[0]
        .buffer()
        .set_text("Second edited synthetic request.");
    settle(2200);
    assert_eq!(sent(&audit).len(), 1);
    owner.agent_voice.view.send_draft.emit_clicked();
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Sent
    });
    assert_eq!(sent(&audit).len(), 2);
    assert_eq!(
        sent(&audit)[1]["params"]["text"],
        "Second edited synthetic request."
    );
    let edited_entry = services.history.find(&history_id).unwrap();
    assert_eq!(edited_entry.raw_text, text);
    assert_eq!(
        services
            .conversations
            .source_text(&edited_entry, false)
            .unwrap(),
        "Second edited synthetic request."
    );
    // Editing during grace and a CLI recipient switch each disarm immediately.
    record();
    finish();
    owner.capture.page.workspace.documents()[0]
        .buffer()
        .set_text("Do not send this edit automatically.");
    settle(2200);
    assert_eq!(sent(&audit).len(), 2);
    record();
    finish();
    let mut switch = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            app.dbus_connection()
                .unwrap()
                .unique_name()
                .unwrap()
                .as_str(),
            "--object-path",
            mluva_gtk::herdr_cli::PATH,
            "--method",
            "com.mluva.Linux.Herdr.Execute",
            &json!({"command":"select","target":"w1:p2","socket":socket}).to_string(),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    until(|| switch.try_wait().unwrap().is_some());
    let switched = switch.wait_with_output().unwrap();
    assert!(switched.status.success());
    assert!(
        String::from_utf8(switched.stdout)
            .unwrap()
            .contains("\"connected\":true")
    );
    assert_eq!(owner.agent_voice.view.recipients.selected(), 2);
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Draft(_)
    ));
    let buffer = owner.capture.page.workspace.documents()[0].buffer();
    assert_eq!(
        buffer.text(&buffer.start_iter(), &buffer.end_iter(), true),
        text
    );
    settle(2200);
    assert_eq!(sent(&audit).len(), 2);
    // A renamed/replaced occupant is rejected by the actual transport even before the next poll.
    owner.agent_voice.view.recipients.set_selected(1);
    record();
    finish();
    data["agents"][0]["guard"] = json!("c".repeat(64));
    state(&peer_state, &data);
    owner.agent_voice.refresh();
    settle(2300);
    assert_eq!(sent(&audit).len(), 2);
    screenshot(&owner, &root, "agent-unavailable-narrow.png", 420);
    // Select the replacement explicitly. Lost acknowledgement reads a receipt and never resends.
    owner.agent_voice.view.recipients.set_selected(1);
    data["mode"] = json!("drop-ack");
    state(&peer_state, &data);
    record();
    finish();
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Sent
    });
    assert_eq!(sent(&audit).len(), 3);
    data["mode"] = json!("uncertain");
    state(&peer_state, &data);
    record();
    finish();
    until(|| {
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase
            == RoutePhase::Unconfirmed
    });
    assert!(!owner.agent_voice.view.send_draft.is_sensitive());
    screenshot(&owner, &root, "agent-unconfirmed-laptop.png", 1100);
    settle(2300);
    assert_eq!(sent(&audit).len(), 4);
    // The normal Stop control also finishes an agent capture. Cancel during grace wins.
    record();
    owner.capture.page.record_button.emit_clicked();
    until(|| owner.capture.phase().is_none());
    assert!(matches!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .phase,
        RoutePhase::Grace { .. }
    ));
    owner.agent_voice.view.cancel.emit_clicked();
    settle(2200);
    assert_eq!(sent(&audit).len(), 4);
    // Incognito uses the same explicit recipient but never creates local History.
    let saved = services.history.recent(100).unwrap().len();
    owner.settings.capture.incognito.set_active(true);
    assert!(services.config().incognito_mode);
    record();
    finish();
    owner.agent_voice.keep_draft();
    assert!(
        owner
            .agent_voice
            .route
            .borrow()
            .attempt
            .as_ref()
            .unwrap()
            .history_id
            .is_none()
    );
    assert_eq!(services.history.recent(100).unwrap().len(), saved);
    settle(2200);
    assert_eq!(sent(&audit).len(), 4);
    // Cancellation during capture does not finalize or submit its audio.
    record();
    owner.agent_voice.view.cancel.emit_clicked();
    until(|| owner.capture.phase().is_none());
    assert_eq!(sent(&audit).len(), 4);
    let clipboard = gtk::gdk::Display::default().unwrap().clipboard();
    assert_eq!(
        glib::MainContext::default()
            .block_on(clipboard.read_text_future())
            .unwrap()
            .as_deref(),
        Some("Clipboard must remain unchanged")
    );
    let ended = owner.shutdown();
    glib::MainContext::default().block_on(ended).unwrap();
    let observed = speech.finish();
    assert_eq!(observed.len(), 9);
    assert!(wire(&audit).iter().all(|r| {
        [
            "agent.guarded_list",
            "agent.prompt_guarded",
            "agent.prompt_receipt",
        ]
        .contains(&r["method"].as_str().unwrap())
    }));
    fs::write(root.join("voice-result.json"), serde_json::to_vec_pretty(&json!({"captures":10,"recognized":9,"guarded_submissions":4,"cancelled_or_disarmed_captures":7,"edited_draft_send":true,"editing_rearmed_draft_cancels":true,"settings_opt_in_persisted":true,"receipt_reconciliation":true,"recognition_immutable":true,"incognito_not_saved":true,"clipboard_unchanged":true,"selected_provider_unchanged":true,"timing_sample_size":1,"release_to_final_ms":attempt.final_ms.unwrap()-attempt.released_ms.unwrap(),"intentional_grace_ms":attempt.submitted_ms.unwrap()-attempt.final_ms.unwrap(),"submit_to_receipt_ms":attempt.received_ms.unwrap()-attempt.submitted_ms.unwrap()})).unwrap()).unwrap();
}
