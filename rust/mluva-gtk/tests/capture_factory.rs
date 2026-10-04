//! Released startup and recording states reached through the native application factory.
use adw::prelude::*;
use mluva_core::{
    config::{AppConfig, AppPaths},
    history::HistoryInput,
    personalization::DictionaryCaseBehavior,
};
use mluva_gtk::{
    async_runtime::DesktopRuntime,
    capture_controller::{CaptureController, CaptureControllerCallbacks, CaptureLaunch},
    capture_view::{CaptureCallbacks, CapturePage},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    rewrite_settings::RewriteSettings,
    theme::ThemeController,
};
use mluva_workflows::{
    capture::{CaptureOptions, CapturePhase},
    dictation::WorkflowResult,
    services::{ApplicationServices, NativeBinaries},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
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
    let end = Instant::now() + Duration::from_secs(8);
    while !ready() {
        assert!(Instant::now() < end, "Factory capture did not settle");
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
fn result(value: WorkflowResult) -> Value {
    json!({
        "kind":"completed","transcription":value.transcription,"output_text":value.output_text,
        "delivery":{"copied":value.delivery.copied,"pasted":value.delivery.pasted,"guidance":value.delivery.guidance,"paste_dispatched":value.delivery.paste_dispatched,"paste_confirmed":value.delivery.paste_confirmed},
        "history_entry":capture_ui::entry(value.history_entry.as_ref()),"retained_audio_path":value.retained_audio_path.map(|_|"$AUDIO"),
        "requires_acceptance":value.requires_acceptance,"incognito":value.incognito,"mode":value.mode,
        "recognition_ms":"recorded","enhancement_ms":"recorded","delivery_ms":"recorded","session_identifier":"generated",
        "recognition_fallback":value.recognition_fallback,"recognition_route":value.recognition_route,"recognition_fallback_reason":value.recognition_fallback_reason,
    })
}
fn page(services: &ApplicationServices, resources: &DocumentResources) -> Rc<CapturePage> {
    let workspace = ConversationWorkspace::new(
        services.conversations.clone(),
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
    let config = services.config();
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
            live_draft_edited: Rc::new(|| {}),
            announce: Rc::new(|_| {}),
        },
    )
    .unwrap()
}

#[test]
#[ignore = "requires the isolated GTK/network/device runner and native audio endpoint"]
fn released_application_capture_factory() {
    let root = PathBuf::from(
        std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("isolated verification runner"),
    );
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&root));
    }
    for device in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(device).exists());
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let tools = root.join("factory-tools");
    assert_eq!(
        std::env::split_paths(&std::env::var_os("PATH").unwrap()).next(),
        Some(tools.clone())
    );
    fs::create_dir(&tools).unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    symlink(
        target.join("audio-fixture-peer").canonicalize().unwrap(),
        tools.join("pw-record"),
    )
    .unwrap();
    let binaries = NativeBinaries {
        asr_worker: target.join("mluva-asr-worker"),
        audio_cleanup: target.join("mluva-audio-cleanup"),
        screenshot_editor: target.join("mluva-screenshot-editor"),
    };
    gtk::init().unwrap();
    adw::init().unwrap();
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
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-capture-factory.json")).unwrap();
    assert_eq!(
        fixture["reference"],
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
    let mut observations = 0;
    for row in fixture["cases"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let params = &row["params"];
        let directory = tempfile::tempdir_in(&root).unwrap();
        fs::write(
            tools.join("test-config.json"),
            serde_json::to_vec(&row["pcm"]).unwrap(),
        )
        .unwrap();
        let mut responses = row["responses"].as_array().unwrap().clone();
        for response in &mut responses {
            response["delay_ms"] = json!(350);
        }
        let mut peer = http::Peer::new(&responses);
        let mut document = row["config"].clone();
        document["transcription_base_url"] = json!(format!("{}/v1", peer.address));
        let config: AppConfig = serde_json::from_value(document).unwrap();
        let paths = AppPaths {
            config: directory.path().join("config/mluva"),
            data: directory.path().join("data/mluva"),
            runtime: directory.path().join("runtime/mluva"),
        };
        config.save(&paths.config.join("config.json")).unwrap();
        let services = ApplicationServices::open(paths.clone()).unwrap();
        services
            .personalization
            .borrow_mut()
            .save_dictionary_replacement(
                "cue wen",
                "BeforeCapture",
                None,
                DictionaryCaseBehavior::Fixed,
            )
            .unwrap();
        let retained = services
            .history
            .add(HistoryInput {
                delivery_outcome: "ready".into(),
                ..HistoryInput::dictation("Existing 34 notes.", "Existing working text.")
            })
            .unwrap();
        let ready = Rc::new(RefCell::new(None));
        let prepared = ready.clone();
        let owner = services.clone();
        let programs = binaries.clone();
        runtime.spawn(async move {
            *prepared.borrow_mut() = Some(owner.capture_services(programs).await);
        });
        until(|| ready.borrow().is_some());
        let capture = ready.borrow_mut().take().unwrap();
        let expected_ready = json!({"available":capture.is_ok(),"error":capture.as_ref().err().map(ToString::to_string).unwrap_or_default(),"existing":services.history.find(&retained.identifier).unwrap().raw_text,"default_model":services.config().local_model});
        assert_eq!(expected_ready, row["ready"], "{name}: initialization");
        if params["record"] == true {
            let capture = Rc::new(capture.unwrap());
            let page = page(&services, &resources);
            let last_audio = Rc::new(RefCell::new(None));
            let output = Rc::new(RefCell::new(None));
            let audio = last_audio.clone();
            let data = paths.data.clone();
            let captured = output.clone();
            let saved = services.clone();
            let controller = CaptureController::attach(
                page.clone(),
                runtime.clone(),
                Rc::new(move |_| {
                    let session = capture.launch(CaptureOptions::default())?;
                    *audio.borrow_mut() = Some(
                        data.join("recordings")
                            .join(format!("{}.wav", session.identifier)),
                    );
                    Ok(CaptureLaunch {
                        session,
                        delivery_target: None,
                    })
                }),
                CaptureControllerCallbacks {
                    wait_for_images: Rc::new(|_| Box::pin(async { Ok(()) })),
                    prepare_result: Rc::new(|_, _| {}),
                    refresh_history: Rc::new(|_| {}),
                    queue_title: capture_ui::titles(&page, &runtime, &services.cwd),
                    images: Rc::new(|_| Ok(vec![])),
                    completed: Rc::new(move |completion| {
                        *captured.borrow_mut() = Some(result(completion.result))
                    }),
                    failed: Rc::new(|failure| {
                        panic!("Unexpected capture failure: {}", failure.error)
                    }),
                    cancelled: Rc::new(|_| {}),
                    phase_changed: Rc::new(|_| {}),
                    live_config_changed: Rc::new(move |config| {
                        config.save(&saved.paths.config.join("config.json")).is_ok()
                    }),
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
            let mut stages = vec![];
            let mut stage = |name: &str| {
                stages.push(json!({"stage":name,"ui":capture_ui::observe(&page,last_audio.borrow().as_deref())}))
            };
            page.record_button.emit_clicked();
            stage("preparing");
            until(|| {
                controller.phase() == Some(CapturePhase::Recording)
                    && tools.join("raw.ready.json").exists()
            });
            settle();
            stage("recording");
            if params["edit"] == true {
                services
                    .personalization
                    .borrow_mut()
                    .save_dictionary_replacement(
                        "cue wen",
                        "AfterCapture",
                        None,
                        DictionaryCaseBehavior::Fixed,
                    )
                    .unwrap();
            }
            page.record_button.emit_clicked();
            stage("processing");
            until(|| output.borrow().is_some() && controller.phase().is_none());
            settle();
            stage("terminal");
            let observed_history = services
                .history
                .recent(100)
                .unwrap()
                .iter()
                .filter(|entry| entry.identifier != retained.identifier)
                .map(|entry| capture_ui::entry(Some(entry)))
                .collect::<Vec<_>>();
            // Save a failed comparison outside temporary fixture directories.
            fs::write(
                root.join(format!("{name}.native.json")),
                serde_json::to_vec_pretty(
                    &json!({"stages":stages,"result":*output.borrow(),"history":observed_history}),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(json!(stages), row["stages"], "{name}: actual GTK states");
            assert_eq!(
                output.borrow().as_ref().unwrap(),
                &row["result"],
                "{name}: finalized workflow"
            );
            assert_eq!(
                json!(observed_history),
                row["history"],
                "{name}: durable source"
            );
            observations += stages.len();
            window.close();
            settle();
            fs::remove_file(tools.join("raw.ready.json")).unwrap();
        }
        assert_eq!(
            json!(peer.finish()),
            row["requests"],
            "{name}: actual HTTP requests"
        );
        println!("RELEASED_FACTORY {name} PASS");
    }
    println!(
        "RELEASED_FACTORY COMPLETE {} {observations}",
        fixture["cases"].as_array().unwrap().len()
    );
}
