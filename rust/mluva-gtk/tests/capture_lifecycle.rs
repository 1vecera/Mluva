//! Actual PCM process → asynchronous capture page → HTTP → SQLite transactions.

use adw::prelude::*;
use futures_util::{SinkExt, StreamExt};
use mluva_audio::{capture::CaptureStorage, recorder::PipeWireRecorder};
use mluva_core::{
    config::AppConfig,
    conversation::ConversationStore,
    diagnostics::DiagnosticsStore,
    history::HistoryStore,
    personalization::{DictionaryCaseBehavior, PersonalizationStore},
};
use mluva_gtk::{
    async_runtime::DesktopRuntime,
    capture_controller::{CaptureController, CaptureControllerCallbacks, CaptureLaunch},
    capture_view::{CaptureCallbacks, CapturePage},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    theme::ThemeController,
};
use mluva_providers::{
    Secret,
    batch_preview::{BatchPreviewClient, PreviewSpeechFactory},
    compatible::CompatibleClient,
    elevenlabs::ElevenLabsClient,
    local::LocalSpeechClient,
    local_preview::LocalPreviewClient,
    realtime::{ElevenLabsRealtimeClient, RealtimeOptions},
    speech::SpeechClient,
};
use mluva_workflows::{
    capture::{CaptureOptions, CapturePhase, CaptureRecognitionClient, CaptureSession},
    dictation::{DictationWorkflow, WorkflowError, WorkflowResult},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::{
    Message,
    handshake::server::{ErrorResponse, Request, Response},
};

#[path = "support/capture_ui.rs"]
mod capture_ui;
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
#[path = "../../mluva-providers/tests/support/local_models.rs"]
mod local_models;
use capture_ui::{entry, observe};

struct RealtimePeer {
    address: String,
    events: Arc<Mutex<Option<Vec<Value>>>>,
}

// Tungstenite's header callback fixes this response/error type.
#[allow(clippy::result_large_err)]
fn verify_realtime_headers(
    request: &Request,
    response: Response,
) -> Result<Response, ErrorResponse> {
    assert_eq!(request.headers()["xi-api-key"], "synthetic-key");
    Ok(response)
}

fn realtime_peer(runtime: &Rc<DesktopRuntime>, scenario: &str) -> RealtimePeer {
    let ready = Rc::new(RefCell::new(None));
    let started = ready.clone();
    let scenario = scenario.to_owned();
    runtime.spawn(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("ws://{}/realtime", listener.local_addr().unwrap());
        let events = Arc::new(Mutex::new(None));
        let received = events.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(stream, verify_realtime_headers)
                .await
                .unwrap();
            let initial = if scenario == "startup-failed" {
                json!({"message_type":"auth_error","error":"synthetic private error"})
            } else {
                json!({"message_type":"session_started","session_id":"synthetic-session"})
            };
            socket.send(Message::Text(initial.to_string().into())).await.unwrap();
            let mut wire = vec![];
            while let Some(Ok(message)) = socket.next().await {
                let Message::Text(message) = message else { break; };
                let value: Value = serde_json::from_str(&message).unwrap();
                wire.push(value.clone());
                if !value["audio_base_64"].as_str().unwrap().is_empty() {
                    if scenario.starts_with("segments") {
                        for text in ["Keep 12 files.", "Keep 34 folders."] {
                            socket.send(Message::Text(json!({"message_type":"committed_transcript","text":text,"language_code":"eng"}).to_string().into())).await.unwrap();
                        }
                    }
                    let preview = if scenario == "stream-failed" {
                        json!({"message_type":"auth_error","error":"synthetic private error"})
                    } else {
                        json!({"message_type":"partial_transcript","text":"provisional must stay transient"})
                    };
                    socket.send(Message::Text(preview.to_string().into())).await.unwrap();
                }
                if value["commit"] == true {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    let committed = if scenario == "segments-stream-failed" {
                        json!({"message_type":"auth_error","error":"synthetic private error"})
                    } else {
                        json!({"message_type":"committed_transcript","text":if scenario.starts_with("segments") {"Keep 56 notes."} else {"cue wen new line 12 files."},"language_code":"eng"})
                    };
                    socket.send(Message::Text(committed.to_string().into())).await.unwrap();
                }
            }
            *received.lock().unwrap() = Some(wire);
        });
        *started.borrow_mut() = Some(RealtimePeer { address, events });
    });
    until(|| ready.borrow().is_some());
    ready.borrow_mut().take().unwrap()
}

fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
fn until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "Native capture did not reach its terminal state"
        );
        drain();
        thread::sleep(Duration::from_millis(2));
    }
    drain();
}
fn settle() {
    let end = Instant::now() + Duration::from_millis(40);
    while Instant::now() < end {
        drain();
        thread::sleep(Duration::from_millis(2));
    }
}
fn result(result: WorkflowResult) -> Value {
    json!({"kind":"completed", "transcription":result.transcription,"output_text":result.output_text,
        "delivery":{"copied":result.delivery.copied,"pasted":result.delivery.pasted,"guidance":result.delivery.guidance,"paste_dispatched":result.delivery.paste_dispatched,"paste_confirmed":result.delivery.paste_confirmed},
        "history_entry":entry(result.history_entry.as_ref()),"retained_audio_path":result.retained_audio_path.map(|_|"$AUDIO"),
        "incognito":result.incognito,"mode":result.mode,
        "recognition_ms":"recorded","enhancement_ms":"recorded","delivery_ms":"recorded","session_identifier":"generated",
        "recognition_fallback":result.recognition_fallback,"recognition_route":result.recognition_route,"recognition_fallback_reason":result.recognition_fallback_reason})
}
fn page(
    history: HistoryStore,
    config: AppConfig,
    resources: &DocumentResources,
) -> Rc<CapturePage> {
    let workspace = ConversationWorkspace::new(
        ConversationStore::new(history),
        ConversationCallbacks {
            copy: Rc::new(|_| {}),
            open_archive: Rc::new(|| {}),
            rename: Rc::new(|_, _| true),
            delete: Rc::new(|_| true),
            merge: Rc::new(|_, _| true),
            continue_recording: Rc::new(|_| {}),
            capture_screenshot: Some(Rc::new(|| {})),
            edit_screenshot: Rc::new(|_| {}),
            remove_screenshot: Rc::new(|_| {}),
        },
        resources.clone(),
    )
    .unwrap();
    CapturePage::new(
        workspace,
        config,
        CaptureCallbacks {
            toggle_recording: Rc::new(|| {}),
            toast: Rc::new(|_| {}),
            retry_initialization: Rc::new(|| {}),
            announce: Rc::new(|_| {}),
        },
    )
    .unwrap()
}
fn isolated() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private desktop required"),
    )
    .canonicalize()
    .unwrap();
    for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(name).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists());
    }
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    root
}
fn native_binary(name: &str) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let path = target.join("debug").join(name);
    assert!(
        path.is_file(),
        "Build the native audio/model peers and cleanup binary first"
    );
    path.canonicalize().unwrap()
}
fn memory_entries() -> Vec<PathBuf> {
    let mut entries = fs::read_dir("/dev/shm")
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
fn check(root: &Path, case: &str, stage: &str, actual: Value, expected: &Value) {
    if &actual != expected {
        fs::write(
            root.join("capture-lifecycle-mismatch.json"),
            serde_json::to_vec_pretty(
                &json!({"case":case,"stage":stage,"actual":actual,"expected":expected}),
            )
            .unwrap(),
        )
        .unwrap();
        panic!("Released capture mismatch: {case}, {stage}; private mismatch artifact saved");
    }
}

#[test]
#[ignore = "requires dev/run-isolated-browser.sh, native audio-fixture-peer and mluva-audio-cleanup"]
fn actual_capture_transactions_match_released_states_and_leave_no_audio_children() {
    let root = isolated();
    let mut fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-capture-lifecycle.json")).unwrap();
    let mut previews: Value =
        serde_json::from_str(include_str!("fixtures/released-capture-previews.json")).unwrap();
    capture_ui::current_contract(&mut fixture);
    capture_ui::current_contract(&mut previews);
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    adw::init().unwrap();
    assert_eq!(
        fixture["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    assert_eq!(fixture["pango"], gtk::pango::version_string().as_str());
    assert_eq!(previews["reference_commit"], fixture["reference_commit"]);
    assert_eq!(previews["gtk"], fixture["gtk"]);
    assert_eq!(previews["pango"], fixture["pango"]);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let runtime = DesktopRuntime::new().unwrap();
    let audio_peer = native_binary("audio-fixture-peer");
    let cleanup = native_binary("mluva-audio-cleanup");
    let qwen_peer = native_binary("qwen-fixture-peer");
    let onnx_peer = native_binary("local-asr-fixture-peer");
    let owner = thread::current().id();
    let mut observations = 0;
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(previews["cases"].as_array().unwrap())
    {
        let name = case["name"].as_str().unwrap();
        let capture_began_at = chrono::Utc::now();
        let memory_before = memory_entries();
        let directory = tempfile::tempdir_in(&root).unwrap();
        let endpoint = directory.path().join("endpoint");
        fs::create_dir(&endpoint).unwrap();
        let executable = endpoint.join("pw-record");
        symlink(&audio_peer, &executable).unwrap();
        fs::write(
            endpoint.join("test-config.json"),
            serde_json::to_vec(&case["pcm"]).unwrap(),
        )
        .unwrap();
        let responses = case["responses"]
            .as_array()
            .unwrap()
            .iter()
            .cloned()
            .map(|mut response| {
                response["delay_ms"] = json!(350);
                response
            })
            .collect::<Vec<_>>();
        let mut peer = http::Peer::new(&responses);
        let ws = case["realtime"]
            .as_str()
            .map(|scenario| realtime_peer(&runtime, scenario));
        let realtime = ws.as_ref().map(|peer| {
            Rc::new(
                ElevenLabsRealtimeClient::new(
                    Secret::new("synthetic-key"),
                    &peer.address,
                    RealtimeOptions {
                        session_timeout: Duration::from_secs(1),
                        finalization_timeout: Duration::from_secs(1),
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
        });
        let mut config: AppConfig =
            AppConfig::from_persisted_json(&serde_json::to_vec(&case["config"]).unwrap()).unwrap();
        config.transcription_base_url = peer.address.clone();
        let local_options = case
            .get("local")
            .map(|spec| local_models::setup(directory.path(), spec, &qwen_peer, &onnx_peer));
        let recognition = if let Some(options) = &local_options {
            Some(CaptureRecognitionClient::Local(Rc::new(
                LocalPreviewClient::new(options.clone(), directory.path().join("previews")),
            )))
        } else if config.transcription_provider == "litellm" {
            let creating_config = config.clone();
            let factory: PreviewSpeechFactory = Arc::new(move || {
                let config = creating_config.clone();
                Box::pin(async move {
                    Ok(Arc::new(SpeechClient::Compatible(CompatibleClient::new(
                        &config.transcription_base_url,
                        config.transcription_api_key_env,
                        config.transcription_remote_model,
                        Duration::from_secs(3),
                    )?)))
                })
            });
            Some(CaptureRecognitionClient::Batch(Rc::new(
                BatchPreviewClient {
                    factory,
                    directory: directory.path().join("previews"),
                    chunk_seconds: config.transcription_chunk_seconds.try_into().unwrap(),
                    preview_enabled: true,
                },
            )))
        } else {
            realtime.map(CaptureRecognitionClient::ElevenLabs)
        };
        let data = directory.path().join("data");
        let recordings = data.join("recordings");
        let history = HistoryStore::new(data.join("history.sqlite3"));
        history.initialize().unwrap();
        let diagnostics = DiagnosticsStore::new(data.join("diagnostics.sqlite3"), 5000).unwrap();
        diagnostics.initialize().unwrap();
        let personalization_path = directory.path().join("personalization.json");
        let speech = if let Some(options) = local_options {
            SpeechClient::Local(LocalSpeechClient::new(options).unwrap())
        } else if config.transcription_provider == "litellm" {
            SpeechClient::Compatible(
                CompatibleClient::new(
                    &peer.address,
                    config.transcription_api_key_env.clone(),
                    config.transcription_remote_model.clone(),
                    Duration::from_secs(3),
                )
                .unwrap(),
            )
        } else {
            SpeechClient::ElevenLabs(
                ElevenLabsClient::new(
                    Secret::new("synthetic-key"),
                    &format!("{}/speech-to-text", peer.address),
                    Duration::from_secs(3),
                )
                .unwrap(),
            )
        };
        let mut workflow = DictationWorkflow::new(
            config.clone(),
            speech,
            history.clone(),
            directory.path().into(),
        );
        workflow.personalization = Some(PersonalizationStore::new(&personalization_path));
        workflow.diagnostics = Some(diagnostics);
        let workflow = Rc::new(workflow);
        let page = page(history.clone(), config.clone(), &resources);
        let outcome = Rc::new(RefCell::new(Value::Null));
        let failure = Rc::new(RefCell::new(Value::Null));
        let last_audio = Rc::new(RefCell::new(None::<PathBuf>));
        let (creating_workflow, creating_recordings, creating_config, creating_cleanup) = (
            workflow.clone(),
            recordings.clone(),
            config.clone(),
            cleanup.clone(),
        );
        let completed = outcome.clone();
        let failed = failure.clone();
        let controller = CaptureController::attach(
            page.clone(),
            runtime.clone(),
            Rc::new(move |_| {
                assert_eq!(thread::current().id(), owner);
                let storage = if creating_config.incognito_mode {
                    CaptureStorage::Incognito {
                        cleanup_executable: creating_cleanup.clone(),
                        memory_root: None,
                    }
                } else {
                    CaptureStorage::Persistent(creating_recordings.clone())
                };
                let session = CaptureSession::new(
                    creating_workflow.clone(),
                    PipeWireRecorder::new(&executable, None),
                    storage,
                    recognition.clone(),
                    CaptureOptions {
                        incognito: creating_config.incognito_mode,
                        audio_retention: creating_config.audio_retention_policy,
                        preview_enabled: true,
                        ..Default::default()
                    },
                )?;
                Ok(CaptureLaunch {
                    session,
                    delivery_target: None,
                })
            }),
            CaptureControllerCallbacks {
                wait_for_images: Rc::new(|_| Box::pin(async { Ok(()) })),
                prepare_result: Rc::new(|_, _| {}),
                refresh_history: Rc::new(|_| {}),
                images: Rc::new(|_| Ok(vec![])),
                completed: Rc::new(move |capture| {
                    assert_eq!(thread::current().id(), owner);
                    *completed.borrow_mut() = result(capture.result);
                }),
                failed: Rc::new(move |capture| {
                    assert_eq!(thread::current().id(), owner);
                    *failed.borrow_mut() = match capture.error {
                        WorkflowError::Failure(error) => {
                            json!({"message":error.message,"retained_audio":error.retained_audio_path.is_some(),"entry":entry(error.history_entry.as_ref()),"output":error.output_text})
                        }
                        error => panic!("Unexpected non-workflow failure: {error}"),
                    };
                }),
                cancelled: Rc::new(|_| {}),
                preview_changed: Rc::new(|| {}),
                phase_changed: Rc::new(move |_| assert_eq!(thread::current().id(), owner)),
            },
        );
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .build();
        window.set_content(Some(&page.widget));
        window.present();
        settle();
        let stages = case["stages"].as_array().unwrap();
        let mut index = 0;
        let mut compare = |stage: &str| {
            assert_eq!(stages[index]["stage"], stage);
            if let Ok(files) = fs::read_dir(&recordings)
                && let Some(path) = files
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| path.extension().is_some_and(|extension| extension == "wav"))
            {
                let stamp = path.file_stem().unwrap().to_str().unwrap();
                assert_eq!(stamp.len(), 21, "{name}: released UTC audio filename");
                let captured = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%S%6f")
                    .expect("released recording filenames contain six fractional UTC digits")
                    .and_utc();
                assert!(captured >= capture_began_at && captured <= chrono::Utc::now());
                *last_audio.borrow_mut() = Some(path);
            }
            check(
                &root,
                name,
                stage,
                observe(&page, last_audio.borrow().as_deref()),
                &stages[index]["ui"],
            );
            index += 1;
            observations += 1;
        };
        compare("idle");
        page.record_button.emit_clicked();
        compare("preparing");
        let operation = case["operation"].as_str().unwrap();
        let mut pid = None;
        if operation == "cancel-preparation" {
            page.record_button.emit_clicked();
            until(|| controller.phase().is_none());
            settle();
            assert!(!endpoint.join("raw.ready.json").exists());
        } else {
            until(|| {
                controller.phase() == Some(CapturePhase::Recording)
                    && endpoint.join("raw.ready.json").exists()
            });
            settle();
            if ws.is_some() && case["realtime"] != "startup-failed" {
                let end = Instant::now() + Duration::from_millis(300);
                while Instant::now() < end {
                    drain();
                    thread::sleep(Duration::from_millis(2));
                }
            }
            pid = Some(
                serde_json::from_slice::<Value>(
                    &fs::read(endpoint.join("raw.ready.json")).unwrap(),
                )
                .unwrap()["pid"]
                    .as_u64()
                    .unwrap(),
            );
            compare("recording");
            if case["edit"] == true {
                PersonalizationStore::new(&personalization_path)
                    .save_dictionary_replacement(
                        case["edit_source"].as_str().unwrap_or("cue wen"),
                        case["edit_replacement"]
                            .as_str()
                            .unwrap_or("EditedAfterStart"),
                        None,
                        DictionaryCaseBehavior::Fixed,
                    )
                    .unwrap();
            }
            if operation == "cancel-recording" {
                assert!(
                    Command::new("xdotool")
                        .args(["key", "Escape"])
                        .status()
                        .unwrap()
                        .success()
                );
                until(|| controller.phase().is_none());
                settle();
            } else {
                page.record_button.emit_clicked();
                compare("processing");
                if operation == "duplicate-stop" {
                    page.record_button.emit_clicked();
                    compare("duplicate-stop");
                    assert!(!controller.cancel());
                }
                let pulses = Rc::new(Cell::new(0));
                let timer_pulses = pulses.clone();
                let pulse = glib::timeout_add_local(Duration::from_millis(10), move || {
                    timer_pulses.set(timer_pulses.get() + 1);
                    glib::ControlFlow::Continue
                });
                until(|| controller.phase().is_none());
                pulse.remove();
                assert!(
                    pulses.get() >= 10,
                    "Provider waiting blocked the actual GLib owner"
                );
                settle();
            }
        }
        compare("terminal");
        assert_eq!(index, stages.len());
        check(
            &root,
            name,
            "result",
            outcome.borrow().clone(),
            &case["result"],
        );
        check(
            &root,
            name,
            "failure",
            failure.borrow().clone(),
            &case["failure"],
        );
        check(
            &root,
            name,
            "history",
            json!(
                history
                    .recent(20)
                    .unwrap()
                    .iter()
                    .map(|row| entry(Some(row)))
                    .collect::<Vec<_>>()
            ),
            &case["history"],
        );
        check(&root, name, "wire", json!(peer.finish()), &case["wires"]);
        if case.get("local_processes").is_some() {
            let trace = local_models::trace(directory.path());
            let starts = trace
                .iter()
                .filter(|event| event["kind"] == "start")
                .collect::<Vec<_>>();
            check(
                &root,
                name,
                "local-processes",
                json!(starts.len()),
                &case["local_processes"],
            );
            for event in starts {
                assert!(
                    !Path::new(&format!("/proc/{}", event["pid"])).exists(),
                    "{name}: resident model survived Stop/Cancel"
                );
                if let Some(key) = event["key_path"].as_str() {
                    assert!(
                        !Path::new(key).exists(),
                        "{name}: model credential survived Stop/Cancel"
                    );
                }
            }
            let requests = trace
                .iter()
                .filter(|event| event["kind"] == "request")
                .map(|event| {
                    if config.local_model == "qwen3-1.7b" {
                        event["payload"].clone()
                    } else {
                        json!({"path":"$AUDIO", "language":event["payload"]["language"]})
                    }
                })
                .collect::<Vec<_>>();
            let audio = trace
                .iter()
                .filter(|event| event["kind"] == "wav")
                .map(|event| {
                    let mut event = event.clone();
                    event.as_object_mut().unwrap().remove("kind");
                    event
                })
                .collect::<Vec<_>>();
            check(
                &root,
                name,
                "local-requests",
                json!(requests),
                &case["local_requests"],
            );
            check(
                &root,
                name,
                "local-audio",
                json!(audio),
                &case["local_audio"],
            );
            assert!(
                fs::read_dir(directory.path().join("previews"))
                    .map(|mut entries| entries.next().is_none())
                    .unwrap_or(true),
                "{name}: private preview WAV survived cleanup"
            );
        }
        let realtime_events = if let Some(ws) = ws {
            until(|| ws.events.lock().unwrap().is_some());
            json!(ws.events.lock().unwrap().take().unwrap())
        } else {
            json!([])
        };
        check(
            &root,
            name,
            "realtime-wire",
            realtime_events,
            &case["realtime_events"],
        );
        let retained = fs::read_dir(&recordings)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|entry| entry.path().is_file())
            })
            .unwrap_or(false);
        assert_eq!(
            retained,
            case["audio_exists"].as_bool().unwrap(),
            "{name}: private audio disposition"
        );
        if let Some(pid) = pid {
            assert!(
                !Path::new(&format!("/proc/{pid}")).exists(),
                "{name}: microphone child survived terminal completion"
            );
        }
        if config.incognito_mode {
            assert_eq!(
                memory_entries(),
                memory_before,
                "{name}: memory-backed audio survived terminal cleanup"
            );
            assert!(history.recent(1).unwrap().is_empty());
            assert_eq!(
                workflow
                    .diagnostics
                    .as_ref()
                    .unwrap()
                    .recent(20)
                    .unwrap()
                    .len(),
                0
            );
        }
        window.close();
        drop(controller);
        settle();
        println!("native_capture {name} PASS");
    }
    println!(
        "NATIVE_COMPLETE dictation transactions, {observations} released states, exact PCM upload and native process reaping"
    );
}
