//! Frozen released callbacks compared with the real native editor and Codex children.
use adw::prelude::*;
use futures_util::{SinkExt, StreamExt};
use mluva_audio::{capture::CaptureStorage, recorder::PipeWireRecorder};
use mluva_core::{
    config::AppConfig,
    conversation::ConversationStore,
    history::{HistoryInput, HistoryStore},
    prompts::PromptStore,
};
use mluva_gtk::{
    async_runtime::DesktopRuntime,
    capture_controller::{
        CaptureController, CaptureControllerCallbacks, CaptureLaunch, CaptureOrigin,
    },
    capture_view::{CaptureCallbacks, CapturePage},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    live_controller::{LiveCallbacks, LiveController},
    rewrite_settings::RewriteSettings,
    theme::ThemeController,
};
use mluva_providers::{
    Secret,
    elevenlabs::ElevenLabsClient,
    realtime::{ElevenLabsRealtimeClient, RealtimeOptions},
    rewriting::RewriteClient,
    speech::SpeechClient,
};
use mluva_workflows::{
    capture::{CaptureOptions, CapturePhase, CaptureRecognitionClient, CaptureSession},
    dictation::DictationWorkflow,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::Message;
#[path = "support/capture_ui.rs"]
mod capture_ui;
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;

fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
fn until(mut ready: impl FnMut() -> bool) {
    let limit = Instant::now() + Duration::from_secs(8);
    while !ready() {
        assert!(Instant::now() < limit, "Live did not settle");
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
fn clipboard() -> String {
    let result = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
        .unwrap();
    assert!(result.status.success());
    String::from_utf8(result.stdout).unwrap()
}
fn canary() {
    let mut process = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    process
        .stdin
        .take()
        .unwrap()
        .write_all(b"Live clipboard canary")
        .unwrap();
    assert!(process.wait().unwrap().success());
}
fn records(path: &Path) -> Vec<Value> {
    let raw = fs::read_to_string(path).unwrap_or_default();
    let Some(end) = raw.rfind('\n') else {
        return vec![];
    };
    raw[..end]
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn wire(evidence: &Path) -> Vec<String> {
    records(&evidence.join("requests.jsonl"))
        .iter()
        .filter(|event| event["message"]["method"] == "turn/start")
        .map(|event| {
            event["message"]["params"]["input"][0]["text"]
                .as_str()
                .unwrap()
                .into()
        })
        .collect()
}
fn private_codex() -> PathBuf {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    for variable in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(variable).unwrap())
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
    let tools = root.join("live-codex-tools");
    assert_eq!(
        std::env::split_paths(&std::env::var_os("PATH").unwrap()).next(),
        Some(tools.clone()),
        "Start with OFFSCREEN_SESSION_ROOT/live-codex-tools first on PATH"
    );
    let peer = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join("debug/codex-fixture-peer")
        .canonicalize()
        .unwrap();
    let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    fs::create_dir(&tools).unwrap();
    fs::set_permissions(&tools, fs::Permissions::from_mode(0o700)).unwrap();
    let executable = tools.join("codex");
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nexec {} serve {} \"$@\"\n",
            quote(&peer),
            quote(&root.join("live-codex-fixture.json")),
        ),
    )
    .unwrap();
    fs::set_permissions(executable, fs::Permissions::from_mode(0o700)).unwrap();
    root
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
            rewrite: Rc::new(|_| {}),
            paste: Rc::new(|_| {}),
            open_archive: Rc::new(|| {}),
            save_prompt: Rc::new(|_| {}),
            cancel_rewrite: Rc::new(|| {}),
            rename: Rc::new(|_, _| true),
            delete: Rc::new(|_| true),
            merge: Rc::new(|_, _| true),
            continue_recording: Rc::new(|_| {}),
            capture_screenshot: Some(Rc::new(|| {})),
            edit_screenshot: Rc::new(|_| {}),
            remove_screenshot: Rc::new(|_| {}),
            edit_prompt: Rc::new(|_| {}),
        },
        resources.clone(),
    )
    .unwrap();
    CapturePage::new(
        workspace,
        RewriteSettings::new(config.clone(), Rc::new(|| {}), Rc::new(|_, _, _| {})),
        config,
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
            announce: Rc::new(|_| {}),
        },
    )
    .unwrap()
}
fn snapshot(owner: &LiveController) -> Value {
    let workspace = &owner.workspace;
    json!({
        "draft":workspace.live_draft(),"draft_status":workspace.live_draft_status.label().as_str(),
        "draft_visible":workspace.live_draft_box.get_visible(),"source_visible":workspace.live_source_box.get_visible(),
        "live_visible":workspace.live_box.get_visible(),"notice":workspace.notice.label().as_str(),
        "viewing_live":workspace.is_viewing_live(),"active":owner.active(),"finalizing":owner.finalizing(),
        "entry":workspace.entry().map(|entry|json!({"raw":entry.raw_text,"output":entry.delivered_text})),
        "clipboard":clipboard()
    })
}

#[test]
#[ignore = "requires the isolated Linux desktop and compiled native Codex peer"]
fn released_live_callbacks_keep_edits_reconcile_and_save() {
    let root = private_codex();
    adw::init().unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-live-controller.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    assert_eq!(
        fixture["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    assert_eq!(fixture["pango"], gtk::pango::version_string().as_str());
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(false);
    settings.set_gtk_cursor_blink(false);
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
    let mut observations = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let directory = tempfile::tempdir_in(&root).unwrap();
        let config: AppConfig = serde_json::from_value(case["config"].clone()).unwrap();
        let prompts = PromptStore::new(
            directory.path().join("prompts"),
            &config.live_rewrite_custom_instructions,
            &[],
        )
        .unwrap();
        let history = HistoryStore::new(directory.path().join("history.sqlite3"));
        history.initialize().unwrap();
        let page = page(history.clone(), config.clone(), &resources);
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .build();
        let stack = adw::ViewStack::new();
        stack.add_named(&page.widget, Some("capture"));
        window.set_content(Some(&stack));
        window.present();
        settle();
        canary();
        let reviews = Rc::new(RefCell::new(vec![]));
        let received = reviews.clone();
        let owner = LiveController::attach(
            page.workspace.clone(),
            runtime.clone(),
            config.clone(),
            prompts.clone(),
            directory.path().into(),
            LiveCallbacks {
                images: Rc::new(|_, _| Ok(vec![])),
                review: Rc::new(move |_, phase, message| {
                    received
                        .borrow_mut()
                        .push(json!({"phase":phase,"message":message}))
                }),
            },
        );
        let gate = directory.path().join("preview.release");
        let evidence = directory.path().join("evidence");
        fs::create_dir(&evidence).unwrap();
        let text = case["text"].as_str().unwrap();
        let preview = format!("preview|{text}");
        let final_key = format!("final|{text}");
        let mut spec = json!({
            "scenario":"clean","evidence":evidence,"live_controls":{
                preview.clone():{"gate":gate,"deltas":if case["failed"]==true {json!([42])} else {json!([case["draft"]])}},
                final_key.clone():{"deltas":if case["final_failed"]==true {json!([42])} else {json!([case["final"]])}}
            }
        });
        let initial_entry = if case["continuation"] == true {
            let prefix = case["prefix"].as_str().unwrap();
            let entry = history
                .add(HistoryInput {
                    delivery_outcome: "ready".into(),
                    ..HistoryInput::dictation(prefix, prefix)
                })
                .unwrap();
            page.workspace
                .store
                .append(
                    &entry.identifier,
                    "Earlier rewrite",
                    case["edit"].as_str().unwrap(),
                    "earlier-model",
                )
                .unwrap();
            let controls = spec["live_controls"].as_object_mut().unwrap();
            let preview_control = controls.remove(&preview).unwrap();
            let final_control = controls.remove(&final_key).unwrap();
            controls.insert(format!("preview|{prefix}\n\n{text}"), preview_control);
            controls.insert(format!("final|{prefix}\n\n{text}"), final_control);
            Some(entry)
        } else {
            None
        };
        fs::write(
            root.join("live-codex-fixture.json"),
            serde_json::to_vec(&spec).unwrap(),
        )
        .unwrap();
        owner
            .begin(
                "11111111-1111-4111-8111-111111111111",
                "dictation",
                false,
                initial_entry
                    .as_ref()
                    .map(|entry| entry.identifier.as_str()),
                false,
            )
            .unwrap();
        page.workspace.set_live("00:00", text, true);
        if let Some(seed) = case["seed"].as_str() {
            page.workspace.live_draft_text.buffer().set_text(seed);
        }
        let mut stages = vec![json!({"stage":"started","ui":snapshot(&owner)})];
        if case["prompt_edit"] == true {
            fs::create_dir_all(&prompts.directory).unwrap();
            fs::write(
                prompts.path("live-polish").unwrap(),
                "This later file must not reach the frozen request.",
            )
            .unwrap();
        }
        if case["config_edit"] == true {
            owner.set_config(AppConfig {
                live_rewrite_template: "custom".into(),
                auto_copy_rewrite: false,
                ..config
            });
        }
        owner.offer(text, false);
        until(|| wire(&evidence).len() == 1);
        stages.push(json!({"stage":"updating","ui":snapshot(&owner)}));
        if case["edit_during"] == true {
            page.workspace
                .live_draft_text
                .buffer()
                .set_text(case["edit"].as_str().unwrap());
            stages.push(json!({"stage":"edited","ui":snapshot(&owner)}));
        }
        let mut entry = history
            .add(HistoryInput {
                delivery_outcome: "ready".into(),
                ..HistoryInput::dictation(text, text)
            })
            .unwrap();
        let final_text = if let Some(initial) = initial_entry {
            page.workspace
                .store
                .append_recording(&initial.identifier, &entry)
                .unwrap();
            entry = history.find(&initial.identifier).unwrap();
            page.workspace.store.source_text(&entry, false).unwrap()
        } else {
            text.into()
        };
        if case["final_early"] == true {
            owner.finish_capture(&entry.identifier, &final_text);
            stages.push(json!({"stage":"coalesced-final","ui":snapshot(&owner)}));
        }
        if case["pause"] == true {
            owner.pause();
            stages.push(json!({"stage":"paused","ui":snapshot(&owner)}));
            owner.finish_capture(&entry.identifier, &final_text);
        } else if case["cancel"] == true {
            owner.cancel();
            stages.push(json!({"stage":"cancelled","ui":snapshot(&owner)}));
            if case["new_generation"] == true {
                owner
                    .begin(
                        "22222222-2222-4222-8222-222222222222",
                        "dictation",
                        false,
                        None,
                        false,
                    )
                    .unwrap();
                page.workspace
                    .live_draft_text
                    .buffer()
                    .set_text(case["edit"].as_str().unwrap());
                stages.push(json!({"stage":"new-capture","ui":snapshot(&owner)}));
                fs::write(&gate, "").unwrap();
            }
        } else {
            fs::write(&gate, "").unwrap();
            until(|| {
                !owner.active()
                    || !page
                        .workspace
                        .live_draft_status
                        .label()
                        .starts_with("Updating live draft")
            });
            if case["final_early"] != true {
                stages.push(json!({"stage":"updated","ui":snapshot(&owner)}));
                if case["browse_other"] == true {
                    let other = history
                        .add(HistoryInput {
                            delivery_outcome: "ready".into(),
                            ..HistoryInput::dictation(
                                "Another conversation",
                                "Another conversation",
                            )
                        })
                        .unwrap();
                    page.workspace
                        .show_conversation(Some(other), &[], false)
                        .unwrap();
                    stages.push(json!({"stage":"browsing-other","ui":snapshot(&owner)}));
                }
                if case["delete_entry"] == true {
                    history.delete(&entry.identifier).unwrap();
                }
                let final_gate = directory.path().join("final.release");
                if case["final_edit"] == true {
                    spec["live_controls"][format!("final|{final_text}")]["gate"] =
                        json!(final_gate);
                    fs::write(
                        root.join("live-codex-fixture.json"),
                        serde_json::to_vec(&spec).unwrap(),
                    )
                    .unwrap();
                }
                owner.finish_capture(&entry.identifier, &final_text);
                if case["final_edit"] == true {
                    until(|| wire(&evidence).len() == 2);
                    page.workspace
                        .live_draft_text
                        .buffer()
                        .set_text(case["edit"].as_str().unwrap());
                    stages.push(json!({"stage":"final-edited","ui":snapshot(&owner)}));
                    fs::write(&final_gate, "").unwrap();
                }
            }
        }
        if case["cancel"] != true {
            until(|| !owner.active());
        }
        settle();
        until(|| {
            records(&evidence.join("process.jsonl"))
                .iter()
                .all(|process| !Path::new(&format!("/proc/{}", process["pid"])).exists())
        });
        stages.push(json!({"stage":"terminal","ui":snapshot(&owner)}));
        assert_eq!(
            json!(stages),
            case["stages"],
            "{name}: GTK/editor/clipboard state"
        );
        observations += stages.len();
        let processes = records(&evidence.join("process.jsonl"));
        assert_eq!(
            processes.len(),
            case["processes"].as_u64().unwrap() as usize,
            "{name}: actual children"
        );
        for process in processes {
            assert!(
                !Path::new(process["cwd"].as_str().unwrap()).exists(),
                "{name}: private workspace remains"
            );
        }
        assert_eq!(
            json!(wire(&evidence)),
            case["prompts"],
            "{name}: actual wire prompts"
        );
        let replies = page.workspace.store.replies(&entry.identifier).unwrap().iter().map(|reply|json!({"instruction":reply.instruction,"text":reply.text,"model":reply.model})).collect::<Vec<_>>();
        assert_eq!(json!(replies), case["replies"], "{name}: durable replies");
        assert_eq!(
            json!(*reviews.borrow()),
            case["reviews"],
            "{name}: review event"
        );
        owner.shutdown();
        window.close();
        settle();
        println!("RELEASED_LIVE {name} PASS");
    }
    println!(
        "RELEASED_LIVE COMPLETE {} {observations}",
        fixture["cases"].as_array().unwrap().len()
    );
    released_capture_live_controls(&root, &runtime, &resources);
}

struct RealtimePeer {
    address: String,
    audio: Arc<AtomicBool>,
    events: Arc<Mutex<Option<Vec<Value>>>>,
}
fn realtime_peer(runtime: &Rc<DesktopRuntime>, gate: PathBuf, text: String) -> RealtimePeer {
    let ready = Rc::new(RefCell::new(None));
    let received = ready.clone();
    runtime.spawn(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("ws://{}/realtime",listener.local_addr().unwrap());
        let audio = Arc::new(AtomicBool::new(false));
        let seen = audio.clone();
        let events = Arc::new(Mutex::new(None));
        let wire = events.clone();
        tokio::spawn(async move {
            let (stream,_) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket.send(Message::Text(json!({"message_type":"session_started","session_id":"synthetic-session"}).to_string().into())).await.unwrap();
            let mut observed = vec![];
            while let Some(Ok(Message::Text(message))) = socket.next().await {
                let value: Value = serde_json::from_str(&message).unwrap();
                observed.push(value.clone());
                if !value["audio_base_64"].as_str().unwrap().is_empty() {
                    seen.store(true,Ordering::Release);
                    let deadline = Instant::now()+Duration::from_secs(8);
                    while !gate.exists() {
                        assert!(Instant::now()<deadline, "speech gate was not released");
                        tokio::time::sleep(Duration::from_millis(2)).await;
                    }
                    socket.send(Message::Text(json!({"message_type":"partial_transcript","text":text}).to_string().into())).await.unwrap();
                }
                if value["commit"]==true {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    socket.send(Message::Text(json!({"message_type":"committed_transcript","text":text,"language_code":"eng"}).to_string().into())).await.unwrap();
                }
            }
            *wire.lock().unwrap() = Some(observed);
        });
        *received.borrow_mut() = Some(RealtimePeer {address,audio,events});
    });
    until(|| ready.borrow().is_some());
    ready.borrow_mut().take().unwrap()
}
fn compare(root: &Path, name: &str, key: &str, actual: Value, expected: &Value) {
    if &actual != expected {
        fs::write(
            root.join(format!("{name}.{key}.native.json")),
            serde_json::to_vec_pretty(&actual).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(format!("{name}.{key}.released.json")),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        panic!("{name}: {key} differs; private observations saved");
    }
}
fn capture_snapshot(
    owner: &LiveController,
    page: &CapturePage,
    audio: Option<&Path>,
    path: &Path,
) -> Value {
    let config = page.config();
    json!({"capture":capture_ui::observe(page,audio),"live":snapshot(owner),"enabled":config.live_rewrite_enabled,
        "continuous":config.live_rewrite_continuous,"persisted_enabled":AppConfig::load(path).unwrap().live_rewrite_enabled})
}
fn released_capture_live_controls(
    root: &Path,
    runtime: &Rc<DesktopRuntime>,
    resources: &DocumentResources,
) {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-capture-live.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let peer_executable = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join("debug/audio-fixture-peer")
        .canonicalize()
        .unwrap();
    let mut observations = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let directory = tempfile::tempdir_in(root).unwrap();
        let endpoint = directory.path().join("endpoint");
        fs::create_dir(&endpoint).unwrap();
        let executable = endpoint.join("pw-record");
        symlink(&peer_executable, &executable).unwrap();
        fs::write(
            endpoint.join("test-config.json"),
            serde_json::to_vec(&case["pcm"]).unwrap(),
        )
        .unwrap();
        let mut responses = case["responses"].as_array().unwrap().clone();
        for response in &mut responses {
            response["delay_ms"] = json!(350);
        }
        let mut http = http::Peer::new(&responses);
        let config: AppConfig = serde_json::from_value(case["config"].clone()).unwrap();
        let config_path = directory.path().join("config.json");
        config.save(&config_path).unwrap();
        let text = case["text"].as_str().unwrap();
        let ws = (case["realtime"] == true).then(|| {
            realtime_peer(
                runtime,
                directory.path().join("speech.release"),
                text.into(),
            )
        });
        let recognition = ws.as_ref().map(|ws| {
            CaptureRecognitionClient::ElevenLabs(Rc::new(
                ElevenLabsRealtimeClient::new(
                    Secret::new("synthetic-key"),
                    &ws.address,
                    RealtimeOptions {
                        session_timeout: Duration::from_secs(1),
                        finalization_timeout: Duration::from_secs(1),
                        ..Default::default()
                    },
                )
                .unwrap(),
            ))
        });
        let history = HistoryStore::new(directory.path().join("history.sqlite3"));
        history.initialize().unwrap();
        let workflow = Rc::new(DictationWorkflow::new(
            config.clone(),
            SpeechClient::ElevenLabs(
                ElevenLabsClient::new(
                    Secret::new("synthetic-key"),
                    &format!("{}/speech-to-text", http.address),
                    Duration::from_secs(3),
                )
                .unwrap(),
            ),
            RewriteClient::new(&config, None, None).unwrap(),
            history.clone(),
            directory.path().into(),
        ));
        let page = page(history.clone(), config.clone(), resources);
        let prompts = PromptStore::new(
            directory.path().join("prompts"),
            &config.live_rewrite_custom_instructions,
            &[],
        )
        .unwrap();
        let live = LiveController::attach(
            page.workspace.clone(),
            runtime.clone(),
            config.clone(),
            prompts,
            directory.path().into(),
            LiveCallbacks {
                images: Rc::new(|_, _| Ok(vec![])),
                review: Rc::new(|_, _, _| {}),
            },
        );
        let gate = directory.path().join("preview.release");
        let evidence = directory.path().join("evidence");
        fs::create_dir(&evidence).unwrap();
        let full_text = format!(
            "{}{text}",
            if case["continuation"] == true {
                format!("{}\n\n", case["prefix"].as_str().unwrap())
            } else {
                String::new()
            }
        );
        fs::write(root.join("live-codex-fixture.json"),serde_json::to_vec(&json!({"scenario":"clean","evidence":evidence,"live_controls":{
            format!("preview|{full_text}"):{"gate":gate,"deltas":[case["draft"]]},format!("final|{full_text}"):{"deltas":[case["final"]]}
        }})).unwrap()).unwrap();
        let last_audio = Rc::new(RefCell::new(None));
        let audio_path = last_audio.clone();
        let latest_session = Rc::new(RefCell::new(None::<Rc<CaptureSession>>));
        let prepared_session = latest_session.clone();
        let current_workflow = workflow.clone();
        let recording_root = directory.path().join("recordings");
        let persisted_path = config_path.clone();
        let controller = CaptureController::attach(
            page.clone(),
            runtime.clone(),
            Rc::new(move |_| {
                let session = CaptureSession::new(
                    current_workflow.clone(),
                    PipeWireRecorder::new(&executable, None),
                    CaptureStorage::Persistent(recording_root.clone()),
                    recognition.clone(),
                    CaptureOptions {
                        preview_enabled: current_workflow.config.live_rewrite_enabled,
                        ..Default::default()
                    },
                )?;
                *audio_path.borrow_mut() =
                    Some(recording_root.join(format!("{}.wav", session.identifier)));
                *prepared_session.borrow_mut() = Some(session.clone());
                Ok(CaptureLaunch {
                    session,
                    delivery_target: None,
                })
            }),
            CaptureControllerCallbacks {
                wait_for_images: Rc::new(|_| Box::pin(async { Ok(()) })),
                prepare_result: Rc::new(|_, _| {}),
                refresh_history: Rc::new(|_| {}),
                queue_title: capture_ui::titles(&page, runtime, directory.path()),
                images: Rc::new(|_| Ok(vec![])),
                completed: Rc::new(|_| {}),
                failed: Rc::new(|_| {}),
                cancelled: Rc::new(|_| {}),
                phase_changed: Rc::new(|_| {}),
                live_config_changed: Rc::new(move |config| config.save(&persisted_path).is_ok()),
            },
        );
        controller.bind_live(live.clone());
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .build();
        window.set_content(Some(&page.widget));
        window.present();
        settle();
        canary();
        let mut stages = vec![];
        let mut stage = |label: &str| {
            stages.push(json!({"stage":label,"ui":capture_snapshot(&live,&page,last_audio.borrow().as_deref(),&config_path)}))
        };
        if case["continuation"] == true {
            let prefix = case["prefix"].as_str().unwrap();
            let parent = history
                .add(HistoryInput {
                    delivery_outcome: "ready".into(),
                    ..HistoryInput::dictation(prefix, prefix)
                })
                .unwrap();
            page.workspace
                .store
                .append(
                    &parent.identifier,
                    "Earlier rewrite",
                    case["edit"].as_str().unwrap(),
                    "earlier-model",
                )
                .unwrap();
            controller.toggle(CaptureOrigin::Continuation(parent.identifier));
        } else {
            page.record_button.emit_clicked();
        }
        stage("preparing");
        until(|| {
            controller.phase() == Some(CapturePhase::Recording)
                && endpoint.join("raw.ready.json").exists()
        });
        settle();
        let microphone: Value =
            serde_json::from_slice(&fs::read(endpoint.join("raw.ready.json")).unwrap()).unwrap();
        if let Some(seed) = case["seed"].as_str() {
            page.workspace.live_draft_text.buffer().set_text(seed);
        }
        stage("recording");
        if let Some(ws) = &ws {
            until(|| ws.audio.load(Ordering::Acquire));
            fs::write(directory.path().join("speech.release"), "").unwrap();
            if case["enable"] == true {
                until(|| {
                    latest_session
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .preview()
                        .is_some_and(|preview| preview.display_text() == text)
                });
                let config = AppConfig {
                    live_rewrite_enabled: true,
                    live_rewrite_continuous: false,
                    ..page.config()
                };
                config.save(&config_path).unwrap();
                controller.apply_live_config(config).unwrap();
            }
            until(|| wire(&evidence).len() == 1);
            stage("updating");
            if case["edit_during"] == true {
                page.workspace
                    .live_draft_text
                    .buffer()
                    .set_text(case["edit"].as_str().unwrap());
                stage("edited");
            }
            if case["pause"] == true {
                let config = AppConfig {
                    live_rewrite_enabled: false,
                    ..page.config()
                };
                config.save(&config_path).unwrap();
                controller.apply_live_config(config).unwrap();
                stage("paused");
            } else if case["hold_preview"] != true {
                fs::write(&gate, "").unwrap();
                until(|| {
                    !page
                        .workspace
                        .live_draft_status
                        .label()
                        .starts_with("Updating live draft")
                });
                stage("updated");
            }
        }
        if case["cancel"] == true {
            assert!(controller.cancel());
            until(|| controller.phase().is_none());
        } else {
            page.record_button.emit_clicked();
            stage("processing");
            until(|| controller.phase().is_none());
            if case["hold_preview"] == true && case["pause"] != true {
                until(|| live.finalizing());
                stage("final-pending");
                if case["block_next"] == true {
                    page.record_button.emit_clicked();
                    stage("blocked-next");
                }
                fs::write(&gate, "").unwrap();
            }
            until(|| !live.active());
        }
        until(|| !Path::new(&format!("/proc/{}", microphone["pid"])).exists());
        until(|| {
            records(&evidence.join("process.jsonl"))
                .iter()
                .all(|process| !Path::new(&format!("/proc/{}", process["pid"])).exists())
        });
        settle();
        stage("terminal");
        for (actual, expected) in stages.iter().zip(case["stages"].as_array().unwrap()) {
            compare(
                root,
                name,
                actual["stage"].as_str().unwrap(),
                actual.clone(),
                expected,
            );
        }
        assert_eq!(stages.len(), case["stages"].as_array().unwrap().len());
        observations += stages.len();
        compare(
            root,
            name,
            "prompts",
            json!(wire(&evidence)),
            &case["prompts"],
        );
        let processes = records(&evidence.join("process.jsonl"));
        assert_eq!(
            processes.len(),
            case["processes"].as_u64().unwrap() as usize
        );
        for process in processes {
            assert!(!Path::new(process["cwd"].as_str().unwrap()).exists());
        }
        let entries = history.recent(20).unwrap();
        compare(
            root,
            name,
            "history",
            json!(
                entries
                    .iter()
                    .map(|entry| capture_ui::entry(Some(entry)))
                    .collect::<Vec<_>>()
            ),
            &case["history"],
        );
        let replies=entries.iter().flat_map(|entry|page.workspace.store.replies(&entry.identifier).unwrap()).map(|reply|json!({"instruction":reply.instruction,"text":reply.text,"model":reply.model})).collect::<Vec<_>>();
        compare(root, name, "replies", json!(replies), &case["replies"]);
        assert_eq!(
            last_audio.borrow().as_ref().unwrap().exists(),
            case["audio_exists"].as_bool().unwrap()
        );
        compare(root, name, "http", json!(http.finish()), &case["wires"]);
        if let Some(ws) = ws {
            until(|| ws.events.lock().unwrap().is_some());
            compare(
                root,
                name,
                "realtime",
                json!(ws.events.lock().unwrap().take().unwrap()),
                &case["realtime_events"],
            );
        }
        drop(controller);
        window.close();
        settle();
        println!("RELEASED_CAPTURE_LIVE {name} PASS");
    }
    println!(
        "RELEASED_CAPTURE_LIVE COMPLETE {} {observations}",
        fixture["cases"].as_array().unwrap().len()
    );
}
