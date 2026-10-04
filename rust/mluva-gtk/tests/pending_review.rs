//! Actual Command/Notes decisions compared with the unchanged released application.
use adw::prelude::*;
use mluva_core::{
    config::{AppConfig, AppPaths, AudioRetentionPolicy},
    delivery::DeliveryReceipt,
    history::HistoryInput,
    scratchpad::ScratchpadDraft,
};
use mluva_gtk::{
    async_runtime::DesktopRuntime,
    capture_preferences::{CapturePreferences, PreferenceActivity},
    capture_view::{CaptureCallbacks, CapturePage},
    document_layout::DocumentResources,
    history_controller::{HistoryController, HistoryControllerCallbacks},
    pending_review::{PendingReview, PendingReviewCallbacks},
    text_target::FocusedTextTargetTracker,
    theme::ThemeController,
};
use mluva_providers::TranscriptionResult;
use mluva_workflows::{dictation::WorkflowResult, services::ApplicationServices};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::{Rc, Weak},
    thread,
    time::{Duration, Instant},
};
#[path = "support/capture_graph.rs"]
#[allow(dead_code)]
mod capture_graph;
#[path = "support/text_transport.rs"]
#[allow(dead_code)]
mod text_transport;
const ID: &str = "11111111-1111-4111-8111-111111111111";
const SESSION: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const DATE: &str = "2026-09-28T12:34:56.123456Z";

fn wait(predicate: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(Instant::now() < end, "Pending review did not settle");
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
fn normalize(value: Value, root: &Path) -> Value {
    match value {
        Value::String(s) => json!(s.replace(root.to_str().unwrap(), "$ROOT")),
        Value::Array(a) => Value::Array(a.into_iter().map(|v| normalize(v, root)).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, normalize(v, root)))
                .collect(),
        ),
        other => other,
    }
}
fn draft_value(mut value: Value) -> Value {
    if !value.is_null() {
        assert_eq!(value["identifier"].as_str().unwrap().len(), 36);
        value["identifier"] = json!("generated-draft");
        if value["history_identifier"].is_null() {
            value["created_at"] = json!("generated-time");
        }
    }
    value
}
struct Observation<'a> {
    review: &'a PendingReview,
    history: &'a HistoryController,
    services: &'a ApplicationServices,
    capture: &'a CapturePage,
    prefs: &'a CapturePreferences,
    window: &'a adw::Window,
}
impl Observation<'_> {
    fn snapshot(
        &self,
        events: &[Value],
        root: &Path,
        peer: Option<&text_transport::Peer>,
    ) -> Value {
        let Self {
            review,
            history,
            services,
            capture: page,
            prefs,
            window,
        } = self;
        let mut records = serde_json::to_value(services.history.recent(100).unwrap()).unwrap();
        for entry in records.as_array_mut().unwrap() {
            for key in ["recognition_ms", "enhancement_ms", "delivery_ms"] {
                if !entry[key].is_null() {
                    entry[key] = json!("measured");
                }
            }
        }
        let connection = rusqlite::Connection::open(&services.diagnostics.path).unwrap();
        let diagnostics = connection.prepare("SELECT session_identifier,mode,stage,provider,outcome FROM diagnostic_events ORDER BY sequence").unwrap()
            .query_map([], |row| Ok((0..5).map(|i| row.get::<_,String>(i).unwrap()).collect::<Vec<_>>())).unwrap()
            .collect::<Result<Vec<_>,_>>().unwrap();
        let draft = services.scratchpad.borrow();
        let disk = if draft.path.exists() {
            serde_json::from_slice(&fs::read(&draft.path).unwrap())
                .map(draft_value)
                .unwrap_or(json!("preserved malformed document"))
        } else {
            Value::Null
        };
        let dialog = window.visible_dialog().map(|dialog| {
            let dialog = dialog.downcast::<adw::AlertDialog>().unwrap();
            json!({"heading":dialog.heading().map(|s|s.to_string()),"body":dialog.body().to_string(),"default":dialog.default_response().map(|s|s.to_string()),"close":dialog.close_response().to_string()})
        });
        let clipboard = Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
            .unwrap();
        assert!(clipboard.status.success());
        let buffer = page.output_view.buffer();
        let mut observed = json!({
            "status": page.status.label().to_string(), "output": buffer.text(&buffer.start_iter(), &buffer.end_iter(), true).to_string(),
            "editable": page.output_view.is_editable(), "command": review.has_command(), "command_id": review.command_identifier(),
            "command_label": page.accept_command.label().map(|s|s.to_string()), "command_source": page.command_source.label().to_string(),
            "command_source_visible": page.command_source.get_visible(), "command_visible": page.command_actions.get_visible(),
            "notes_visible": page.scratchpad_actions.get_visible(), "action_bar_visible": page.action_bar.get_visible(),
            "controls":{"mode":prefs.mode.is_sensitive(),"key":prefs.recording_key.is_sensitive(),"incognito":prefs.incognito.is_sensitive(),"language":prefs.language.is_sensitive(),"microphone":prefs.microphone.is_sensitive(),"system":prefs.system_audio.is_sensitive(),"refresh":prefs.refresh_audio.is_sensitive(),"audio_retention":prefs.audio_retention.is_sensitive(),"history_retention":prefs.history_retention.is_sensitive(),"spoken":prefs.spoken_commands.is_sensitive(),"remember":prefs.remember_application.is_sensitive(),"cleanup":prefs.cleanup.is_sensitive(),"style":prefs.output_style.is_sensitive()},
            "capture_sensitive": page.record_button.is_sensitive(), "mode": prefs.mode.selected(),
            "history_count": history.page.count.label().to_string(), "records": records, "events": events, "diagnostics":diagnostics,
            "draft": draft_value(serde_json::to_value(&draft.draft).unwrap()), "disk": disk,
            "audio": services.paths.data.join(format!("recordings/{ID}.wav")).exists(), "clipboard":String::from_utf8(clipboard.stdout).unwrap(), "dialog":dialog,
        });
        if let Some(peer) = peer {
            let mut target = peer.observed();
            target.as_object_mut().unwrap().remove("serial");
            target.as_object_mut().unwrap().remove("pid");
            observed["target"] = target;
        }
        normalize(observed, root)
    }
}
fn compare(actual: Value, expected: &Value, label: &str, private: &Path) {
    if &actual != expected {
        fs::write(
            private.join("pending-review-actual.json"),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        fs::write(
            private.join("pending-review-expected.json"),
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
fn action(
    slot: &Rc<RefCell<Weak<PendingReview>>>,
    operation: impl Fn(&Rc<PendingReview>) + 'static,
) -> Rc<dyn Fn()> {
    let slot = slot.clone();
    Rc::new(move || {
        if let Some(review) = slot.borrow().upgrade() {
            operation(&review);
        }
    })
}

#[test]
#[ignore = "Requires the guarded private GTK/session/accessibility/network/PID/device environment"]
fn released_command_and_notes_decisions() {
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
        Some("org.example.Mluva.ReviewComparison"),
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
        serde_json::from_str(include_str!("fixtures/released-pending-review.json")).unwrap();
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
            rewrite_provider: case["rewrite_provider"].as_str().unwrap().into(),
            history_retention_days: 0.into(),
            auto_paste: case["auto_paste"].as_bool().unwrap_or(false),
            incognito_mode: case["private"].as_bool().unwrap_or(false),
            ..Default::default()
        };
        config.save(&paths.config.join("config.json")).unwrap();
        if case["malformed"] == true {
            fs::create_dir_all(&paths.data).unwrap();
            fs::write(
                paths.data.join("scratchpad-draft.json"),
                case["malformed_text"]
                    .as_str()
                    .unwrap_or("{ preserved malformed document"),
            )
            .unwrap();
        }
        let services = ApplicationServices::open(paths).unwrap();
        let mode = case["mode"].as_str().unwrap();
        let retention = case["retention"].as_str().unwrap_or("failures");
        let policy = match retention {
            "always" => AudioRetentionPolicy::Always,
            "never" => AudioRetentionPolicy::Never,
            _ => AudioRetentionPolicy::Failures,
        };
        let audio = services.paths.data.join(format!("recordings/{ID}.wav"));
        if case["retained"] == true {
            fs::create_dir_all(audio.parent().unwrap()).unwrap();
            fs::write(&audio, b"synthetic retained PCM").unwrap();
        }
        let output = case["text"].as_str().unwrap_or("Reviewed output.");
        let entry = if case["private"] == true {
            None
        } else {
            let added = services
                .history
                .add(HistoryInput {
                    mode: mode.into(),
                    language_code: "ces".into(),
                    delivery_outcome: if mode == "scratchpad" {
                        "draft"
                    } else {
                        "pending-preview"
                    }
                    .into(),
                    retained_audio_path: (case["retained"] == true)
                        .then(|| audio.to_str().unwrap().into()),
                    audio_retention_policy: Some(retention.into()),
                    ..HistoryInput::dictation("Original raw.", output)
                })
                .unwrap();
            rusqlite::Connection::open(&services.history.database.path)
                .unwrap()
                .execute(
                    "UPDATE transcription_history SET identifier=?,created_at=? WHERE identifier=?",
                    rusqlite::params![ID, DATE, added.identifier],
                )
                .unwrap();
            Some(services.history.find(ID).unwrap())
        };
        if case["draft"] == true {
            services
                .scratchpad
                .borrow_mut()
                .save(
                    ScratchpadDraft {
                        identifier: SESSION.into(),
                        history_identifier: Some(ID.into()),
                        created_at: DATE.into(),
                        raw_text: "Original raw.".into(),
                        text: "Recovered Notes".into(),
                        audio_path: Some(audio.to_str().unwrap().into()),
                        incognito: false,
                        audio_retention_policy: retention.into(),
                        session_identifier: Some(SESSION.into()),
                    },
                    true,
                )
                .unwrap();
        }
        let slot = Rc::new(RefCell::new(Weak::<PendingReview>::new()));
        let (capture, prefs) = capture_graph::graph_with_callbacks(
            services.clone(),
            resources.clone(),
            CaptureCallbacks {
                toggle_recording: Rc::new(|| {}),
                apply_live_settings: Rc::new(|_| false),
                toast: Rc::new(|_| {}),
                open_prompt: Rc::new(|_| {}),
                retry_initialization: Rc::new(|| {}),
                accept_command: action(&slot, |review| review.accept_command()),
                discard_command: action(&slot, |review| review.discard_command()),
                copy_scratchpad: action(&slot, |review| review.copy_notes()),
                delete_scratchpad: action(&slot, PendingReview::confirm_delete_notes),
                output_changed: {
                    let slot = slot.clone();
                    Rc::new(move |text| {
                        if let Some(review) = slot.borrow().upgrade() {
                            review.edit_notes(text);
                        }
                    })
                },
                live_draft_edited: Rc::new(|| {}),
                announce: Rc::new(|_| {}),
            },
        );
        capture.set_pending_mode(mode);
        let events = Rc::new(RefCell::new(Vec::<Value>::new()));
        let changed: Rc<dyn Fn()> = {
            let events = events.clone();
            let workspace = capture.workspace.clone();
            Rc::new(move || {
                workspace.refresh_history().unwrap();
                events.borrow_mut().push(json!(["changed"]));
            })
        };
        let history = HistoryController::new(
            services.clone(),
            runtime.clone(),
            capture.clone(),
            prefs.clone(),
            HistoryControllerCallbacks {
                activity: {
                    let slot = slot.clone();
                    Rc::new(move || PreferenceActivity {
                        pending_review: slot.borrow().upgrade().is_some_and(|review| review.busy()),
                        ..Default::default()
                    })
                },
                pending_command: {
                    let slot = slot.clone();
                    Rc::new(move || {
                        slot.borrow()
                            .upgrade()
                            .and_then(|review| review.command_identifier())
                    })
                },
                changed: changed.clone(),
                idle: {
                    let events = events.clone();
                    Rc::new(move || events.borrow_mut().push(json!(["idle"])))
                },
                queue_title: Rc::new(|_| {}),
                close_screenshot: Rc::new(|_| Ok(())),
                copy: Rc::new(|_| {}),
            },
        )
        .unwrap();
        let review = PendingReview::new(
            services.clone(),
            capture.clone(),
            prefs.clone(),
            history.clone(),
            PendingReviewCallbacks {
                changed,
                excluded_history: Rc::new(Default::default),
            },
        );
        slot.replace(Rc::downgrade(&review));
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .content(&capture.widget)
            .build();
        window.present();
        settle();
        let mut peer = (case["target"] == true)
            .then(|| text_transport::Peer::new(root.join("peer"), "text_target_peer"));
        let mut tracker = None;
        let mut target = None;
        let mut clipboard = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        clipboard
            .stdin
            .take()
            .unwrap()
            .write_all(b"private clipboard canary")
            .unwrap();
        assert!(clipboard.wait().unwrap().success());
        let observation = Observation {
            review: &review,
            history: &history,
            services: &services,
            capture: &capture,
            prefs: &prefs,
            window: &window,
        };
        for stage in case["stages"].as_array().unwrap() {
            let action = stage["action"].as_str().unwrap();
            match action {
                "initial" => {}
                "target" => {
                    let peer = peer.as_mut().unwrap();
                    peer.request(json!({"operation":"button"}));
                    let tracking = FocusedTextTargetTracker::new().unwrap();
                    peer.request(json!({"operation":"setup","kind":"entry","text":"Before 🐎 after","start":7,"end":if case["caret"]==true {7}else{8},"editable":true}));
                    target = Some(tracking.capture_text_target(2000).unwrap().unwrap());
                    tracker = Some(tracking);
                }
                "exit_target" => {
                    peer.as_mut().unwrap().stop();
                    settle();
                }
                "show" => {
                    let result = WorkflowResult {
                        transcription: TranscriptionResult {
                            text: "Original raw.".into(),
                            language_code: "ces".into(),
                            language_probability: None,
                            transcription_id: None,
                            speaker_segments: vec![],
                            audio_duration_seconds: None,
                        },
                        output_text: output.into(),
                        delivery: DeliveryReceipt {
                            copied: false,
                            pasted: false,
                            guidance: "Review this result.".into(),
                            paste_dispatched: false,
                            paste_confirmed: None,
                        },
                        history_entry: entry.clone(),
                        retained_audio_path: (case["retained"] == true).then(|| audio.clone()),
                        requires_acceptance: true,
                        incognito: case["private"] == true,
                        mode: mode.into(),
                        recognition_ms: 0,
                        enhancement_ms: 0,
                        delivery_ms: 0,
                        session_identifier: SESSION.into(),
                        recognition_fallback: false,
                        recognition_route: "scribe-v2-batch".into(),
                        recognition_fallback_reason: None,
                    };
                    if mode == "command" {
                        review.show_command(result, target.take(), policy);
                    } else {
                        review.show_notes(result, policy);
                    }
                }
                "accept" => capture.accept_command.emit_clicked(),
                "discard" => capture.discard_command.emit_clicked(),
                "copy" => capture.copy_scratchpad.emit_clicked(),
                "edit" => capture
                    .output_view
                    .buffer()
                    .set_text(" \u{1c} Human edited Notes. "),
                "empty" => capture.output_view.buffer().set_text(" \u{1c} "),
                "confirm_delete" => capture.delete_scratchpad.emit_clicked(),
                "cancel" | "confirm" => window
                    .visible_dialog()
                    .unwrap()
                    .downcast::<adw::AlertDialog>()
                    .unwrap()
                    .emit_by_name::<()>(
                        "response",
                        &[&if action == "cancel" {
                            "cancel"
                        } else {
                            "delete"
                        }],
                    ),
                "restore" => review.restore_notes(),
                "lose_history" => services.history.delete(ID).unwrap(),
                other => panic!("unexpected action {other}"),
            }
            settle();
            compare(
                observation.snapshot(&events.borrow(), root, peer.as_ref()),
                &stage["observed"],
                &format!("{} {action}", case["name"]),
                &private,
            );
            states += 1;
        }
        if case["malformed"] == true {
            assert_eq!(
                fs::read_to_string(&services.scratchpad.borrow().path).unwrap(),
                case["malformed_text"]
                    .as_str()
                    .unwrap_or("{ preserved malformed document")
            );
        }
        review.close();
        if case["name"] == "notes-history-disappeared-keeps-draft" {
            let before = observation.snapshot(&events.borrow(), root, peer.as_ref());
            capture.copy_scratchpad.emit_clicked();
            capture.delete_scratchpad.emit_clicked();
            review.restore_notes();
            settle();
            assert_eq!(
                observation.snapshot(&events.borrow(), root, peer.as_ref()),
                before,
                "Closed review must not resolve, delete or reopen a durable draft"
            );
        }
        let finished = Rc::new(Cell::new(false));
        let flag = finished.clone();
        let shutdown = history.shutdown();
        runtime.spawn(async move {
            shutdown.await.unwrap();
            flag.set(true);
        });
        wait(|| finished.get());
        if let Some(tracker) = &mut tracker {
            tracker.close();
        }
        drop(peer);
        window.destroy();
        settle();
    }
    println!(
        "Matched {} released Command/Notes workflows/{states} GTK/store/target states plus closed-owner recovery protection",
        fixture["cases"].as_array().unwrap().len()
    );
}
