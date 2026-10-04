//! Released recovery handlers against native widgets, stores and real HTTP/clipboard boundaries.
use adw::prelude::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mluva_core::{
    config::{AppConfig, AppPaths},
    history::HistoryInput,
    scratchpad::ScratchpadDraft,
};
use mluva_gtk::{
    async_runtime::DesktopRuntime,
    capture_preferences::{CapturePreferences, PreferenceActivity},
    capture_view::CapturePage,
    document_layout::DocumentResources,
    history_controller::{HistoryController, HistoryControllerCallbacks},
    theme::ThemeController,
};
use mluva_providers::{
    Secret, elevenlabs::ElevenLabsClient, rewriting::RewriteClient, speech::SpeechClient,
};
use mluva_workflows::{dictation::DictationWorkflow, services::ApplicationServices};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[path = "support/capture_graph.rs"]
mod capture_graph;
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
#[path = "support/text_transport.rs"]
#[allow(dead_code)]
mod text_transport;
const ID: &str = "11111111-1111-4111-8111-111111111111";
const DATE: &str = "2026-09-28T12:34:56.123456Z";
fn wait(predicate: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(Instant::now() < end, "History recovery did not settle");
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn settle() {
    let until = Instant::now() + Duration::from_millis(60);
    wait(|| Instant::now() >= until);
}
fn await_shutdown(runtime: &Rc<DesktopRuntime>, shutdown: glib::JoinHandle<()>) {
    let finished = Rc::new(std::cell::Cell::new(false));
    let flag = finished.clone();
    runtime.spawn(async move {
        shutdown.await.unwrap();
        flag.set(true);
    });
    wait(|| finished.get());
}
fn normalize(value: Value, root: &Path) -> Value {
    match value {
        Value::String(s) => json!(
            regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                .unwrap()
                .replace_all(&s.replace(root.to_str().unwrap(), "$ROOT"), "generated")
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(|v| normalize(v, root)).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, normalize(v, root)))
                .collect(),
        ),
        other => other,
    }
}
fn observe(
    owner: &HistoryController,
    services: &ApplicationServices,
    page: &CapturePage,
    prefs: &CapturePreferences,
    events: &[Value],
    root: &Path,
) -> Value {
    let mut records = serde_json::to_value(services.history.recent(100).unwrap()).unwrap();
    for entry in records.as_array_mut().unwrap() {
        for key in ["recognition_ms", "enhancement_ms", "delivery_ms"] {
            if !entry[key].is_null() {
                entry[key] = json!("measured");
            }
        }
    }
    let b = page.output_view.buffer();
    normalize(
        json!({"status":page.status.label().to_string(),"output":b.text(&b.start_iter(),&b.end_iter(),true).to_string(),
 "controls":{"mode":prefs.mode.is_sensitive(),"key":prefs.recording_key.is_sensitive(),"incognito":prefs.incognito.is_sensitive(),"language":prefs.language.is_sensitive(),"microphone":prefs.microphone.is_sensitive(),"system":prefs.system_audio.is_sensitive(),"refresh":prefs.refresh_audio.is_sensitive(),"audio_retention":prefs.audio_retention.is_sensitive(),"history_retention":prefs.history_retention.is_sensitive(),"spoken":prefs.spoken_commands.is_sensitive(),"remember":prefs.remember_application.is_sensitive()},
 "capture_sensitive":page.record_button.is_sensitive(),"retry":owner.retry_identifier(),"retrying":owner.retry_identifier().is_some(),"records":records,"events":events,"draft":services.scratchpad.borrow().draft,"draft_exists":services.scratchpad.borrow().path.exists(),"audio":services.paths.data.join(format!("recordings/{ID}.wav")).exists(),"history_count":owner.page.count.label().to_string(),"scratchpad_visible":page.scratchpad_actions.get_visible()}),
        root,
    )
}
fn compare(actual: Value, expected: &Value, label: &str, private: &Path) {
    if &actual != expected {
        fs::write(
            private.join("history-owner-actual.json"),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        fs::write(
            private.join("history-owner-expected.json"),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        let fields = actual
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, v)| expected.get(*k) != Some(*v))
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>();
        panic!("{label}: changed {fields:?}; synthetic observations saved in private evidence");
    }
}
fn configure(owner: &HistoryController, services: &Rc<ApplicationServices>, address: &str) {
    let services = services.clone();
    let endpoint = format!("{address}/speech-to-text");
    assert!(owner.set_workflow_factory(Some(Rc::new(move || {
        let config = services.config();
        let speech = SpeechClient::ElevenLabs(ElevenLabsClient::new(
            Secret::new("synthetic-recovery-key"),
            &endpoint,
            Duration::from_secs(3),
        )?);
        let rewrite = RewriteClient::new(&config, None, None)?;
        let mut workflow = DictationWorkflow::new(
            config,
            speech,
            rewrite,
            services.history.clone(),
            services.cwd.clone(),
        );
        workflow.personalization = Some(services.personalization.borrow().clone());
        Ok(Rc::new(workflow))
    }))));
}
#[test]
#[ignore = "Requires the guarded private GTK/session/accessibility/network/PID/device environment"]
fn released_history_recovery_and_owned_shutdown() {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&private));
    }
    for node in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(node).exists());
    }
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
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
        ThemeController::apply(private.join("theme"), resources.font.parent().unwrap()).unwrap();
    let runtime = DesktopRuntime::new().unwrap();
    let cache_app = gtk::Application::new(
        Some("org.example.Mluva.HistoryComparison"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    cache_app.register(gio::Cancellable::NONE).unwrap();
    let cache_window = gtk::ApplicationWindow::builder()
        .application(&cache_app)
        .build();
    cache_window.set_child(Some(&gtk::Entry::new()));
    cache_window.present();
    settle();
    cache_window.set_visible(false);
    settle();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-history-controller.json")).unwrap();
    let mut states = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir_in(&private).unwrap();
        let root = directory.path();
        let paths = AppPaths {
            config: root.join("config"),
            data: root.join("data"),
            runtime: root.join("runtime"),
        };
        let config = AppConfig {
            language_code: "ces".into(),
            rewrite_provider: "none".into(),
            transcription_provider: "elevenlabs".into(),
            history_retention_days: 0.into(),
            spoken_commands_enabled: true,
            auto_paste: case["auto_paste"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        config.save(&paths.config.join("config.json")).unwrap();
        let services = ApplicationServices::open(paths).unwrap();
        let audio = services.paths.data.join(format!("recordings/{ID}.wav"));
        if case["retained"] == true {
            fs::create_dir_all(audio.parent().unwrap()).unwrap();
            fs::write(&audio, b"synthetic retained PCM").unwrap();
        }
        let added = services
            .history
            .add(HistoryInput {
                language_code: "ces".into(),
                delivery_outcome: "failed".into(),
                retained_audio_path: (case["retained"] == true)
                    .then(|| audio.to_str().unwrap().into()),
                audio_retention_policy: Some("failures".into()),
                ..HistoryInput::dictation(
                    case["raw"].as_str().unwrap_or(""),
                    case["delivered"].as_str().unwrap_or(""),
                )
            })
            .unwrap();
        rusqlite::Connection::open(&services.history.database.path)
            .unwrap()
            .execute(
                "UPDATE transcription_history SET identifier=?,created_at=? WHERE identifier=?",
                rusqlite::params![ID, DATE, added.identifier],
            )
            .unwrap();
        let entry = services.history.find(ID).unwrap();
        if case["images"] == true {
            services.screenshots.add(ID,&STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAAD0lEQVR4nGOQS7nzoUkOAAoRAu8yvJYrAAAAAElFTkSuQmCC").unwrap(),false,None).unwrap();
        }
        let (capture, prefs) = capture_graph::graph(services.clone(), resources.clone());
        let events = Rc::new(RefCell::new(Vec::<Value>::new()));
        let activity = Rc::new(RefCell::new(PreferenceActivity::default()));
        let command = Rc::new(RefCell::new(None::<String>));
        let owner = HistoryController::new(
            services.clone(),
            runtime.clone(),
            capture.clone(),
            prefs.clone(),
            HistoryControllerCallbacks {
                activity: {
                    let activity = activity.clone();
                    let command = command.clone();
                    let services = services.clone();
                    Rc::new(move || {
                        let mut a = activity.borrow().clone();
                        a.pending_review = command.borrow().is_some()
                            || services.scratchpad.borrow().draft.is_some();
                        a
                    })
                },
                pending_command: {
                    let command = command.clone();
                    Rc::new(move || command.borrow().clone())
                },
                changed: {
                    let events = events.clone();
                    let workspace = capture.workspace.clone();
                    Rc::new(move || {
                        workspace.refresh_history().unwrap();
                        events.borrow_mut().push(json!(["changed"]));
                    })
                },
                idle: {
                    let events = events.clone();
                    Rc::new(move || events.borrow_mut().push(json!(["idle"])))
                },
                queue_title: {
                    let events = events.clone();
                    Rc::new(move |entry| {
                        events.borrow_mut().push(json!(["title", entry.identifier]))
                    })
                },
                close_screenshot: {
                    let events = events.clone();
                    Rc::new(move |id| {
                        events.borrow_mut().push(json!(["close-image", id]));
                        Ok(())
                    })
                },
                copy: Rc::new(|_| {}),
            },
        )
        .unwrap();
        let mut responses = case["responses"].as_array().cloned().unwrap_or_default();
        for response in &mut responses {
            response["delay_ms"] = json!(300);
        }
        let mut peer = http::Peer::new(&responses);
        if case["configured"] == true {
            configure(&owner, &services, &peer.address);
        }
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .content(&capture.widget)
            .build();
        window.present();
        settle();
        let mut target_peer = (case["target"] == true)
            .then(|| text_transport::Peer::new(root.join("peer"), "text_target_peer"));
        let mut tracker = None;
        let mut target = None;
        for stage in case["stages"].as_array().unwrap() {
            let action = stage["action"].as_str().unwrap();
            match action {
                "target" => {
                    let peer = target_peer.as_mut().unwrap();
                    peer.request(json!({"operation":"button"}));
                    let owner_tracker =
                        mluva_gtk::text_target::FocusedTextTargetTracker::new().unwrap();
                    peer.request(json!({"operation":"setup","kind":"entry","text":"Before 🐎 after","start":7,"end":8,"editable":true}));
                    let snapshot = owner_tracker.capture_delivery_target().unwrap();
                    owner.remember_target(ID, &snapshot);
                    tracker = Some(owner_tracker);
                    target = Some(snapshot);
                }
                "exit_target" => {
                    target_peer.as_mut().unwrap().stop();
                    settle();
                }
                "evict" => {
                    for index in 0..32 {
                        owner.remember_target(&format!("cache-{index}"), target.as_ref().unwrap());
                    }
                }
                "initial" => {}
                "meeting_processing" | "meeting_retry" | "meeting_recording" | "preparing"
                | "processing" | "recording" | "idle" => {
                    let mut a = activity.borrow_mut();
                    *a = PreferenceActivity {
                        meeting_processing: action == "meeting_processing",
                        meeting_retrying: action == "meeting_retry",
                        meeting_recording: action == "meeting_recording",
                        preparing: action == "preparing",
                        processing: action == "processing",
                        recording: action == "recording",
                        ..Default::default()
                    };
                    command.borrow_mut().take();
                }
                "command" => {
                    command.replace(Some(ID.into()));
                }
                "draft" => {
                    services
                        .scratchpad
                        .borrow_mut()
                        .save(
                            ScratchpadDraft {
                                identifier: ID.into(),
                                history_identifier: Some(ID.into()),
                                created_at: DATE.into(),
                                raw_text: "Original raw.".into(),
                                text: "Unresolved Notes".into(),
                                audio_path: None,
                                incognito: false,
                                audio_retention_policy: "failures".into(),
                                session_identifier: None,
                            },
                            true,
                        )
                        .unwrap();
                    capture.scratchpad_actions.set_visible(true);
                }
                "retry" => owner.retry_recognition(&entry),
                "reprocess" => owner.reprocess(&entry),
                "paste" => owner.retry_delivery(&entry),
                "delete" => {
                    let deleted = owner.delete(&entry);
                    events.borrow_mut().push(json!(["deleted", deleted]));
                }
                "wait" => wait(|| owner.retry_identifier().is_none()),
                other => panic!("unexpected action {other}"),
            }
            if action == "paste" && target_peer.is_some() {
                settle();
            }
            if action == "paste" && !entry.delivered_text.is_empty() {
                let clipboard = std::process::Command::new("xclip")
                    .args(["-selection", "clipboard", "-o"])
                    .output()
                    .unwrap();
                assert!(clipboard.status.success());
                assert_eq!(clipboard.stdout, entry.delivered_text.as_bytes());
            }
            let mut actual = observe(&owner, &services, &capture, &prefs, &events.borrow(), root);
            if let Some(peer) = &target_peer {
                let mut observed = peer.observed();
                observed.as_object_mut().unwrap().remove("serial");
                observed.as_object_mut().unwrap().remove("pid");
                actual["target"] = observed;
                actual["can_paste"] = json!(owner.can_retry_delivery(&entry));
            }
            compare(
                actual,
                &stage["observed"],
                &format!("{} {action}", case["name"]),
                &private,
            );
            states += 1;
        }
        assert_eq!(peer.finish(), case["requests"].as_array().unwrap().clone());
        // Closing during a real pending response must never publish a retry result.
        if case["name"] == "recognition-success" {
            let mut response = case["responses"][0].clone();
            response["delay_ms"] = json!(400);
            let peer = http::Peer::new(&[response]);
            configure(&owner, &services, &peer.address);
            let before = services.history.find(ID).unwrap();
            let callbacks = events.borrow().clone();
            let output = capture.output_view.buffer();
            let before_output = output
                .text(&output.start_iter(), &output.end_iter(), true)
                .to_string();
            owner.retry_recognition(&before);
            wait(|| !peer.observed.lock().unwrap().is_empty());
            let closed = owner.shutdown();
            await_shutdown(&runtime, closed);
            // Wait for the peer's delayed response attempt before checking for late writes.
            drop(peer);
            settle();
            assert_eq!(services.history.find(ID).unwrap(), before);
            assert_eq!(*events.borrow(), callbacks);
            assert_eq!(
                output
                    .text(&output.start_iter(), &output.end_iter(), true)
                    .as_str(),
                before_output
            );
            assert!(audio.exists());
            assert!(!owner.set_workflow_factory(None));
        }
        let closed = owner.shutdown();
        await_shutdown(&runtime, closed);
        if let Some(tracker) = &mut tracker {
            tracker.close();
        }
        drop(target_peer);
        window.destroy();
        settle();
    }
    println!(
        "Matched {} released History recovery workflows/{states} GTK/store states plus pending-response shutdown",
        fixture["cases"].as_array().unwrap().len()
    );
}
