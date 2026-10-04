//! Complete native recording workflows mutate a separate GTK process once and keep recovery honest.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gtk::prelude::*;
use mluva_core::{
    config::AppConfig, diagnostics::DiagnosticsStore, history::HistoryStore,
    personalization::PersonalizationStore,
};
use mluva_gtk::text_target::{DeliveryTargetSnapshot, FocusedTextTargetTracker};
use mluva_providers::{
    Secret, elevenlabs::ElevenLabsClient, rewriting::RewriteClient, speech::SpeechClient,
};
use mluva_workflows::dictation::{Completion, DictationWorkflow};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
#[path = "support/text_transport.rs"]
pub mod transport;
use transport::{Monitor, Peer, settle, write};

fn clipboard(value: Option<&str>) -> String {
    let mut command = Command::new("xclip");
    command.args(["-selection", "clipboard"]);
    if let Some(value) = value {
        let mut child = command.stdin(Stdio::piped()).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(value.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        String::new()
    } else {
        let output = command.arg("-o").output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }
}

#[test]
#[ignore = "requires dev/run-isolated-browser.sh and built text_target_peer"]
fn batch_speech_through_personalization_and_delivery_matches_released_gtk_workflows() {
    let root = PathBuf::from(
        std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private desktop required"),
    )
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
    assert_eq!(std::env::var("ATSPI_DISABLE_P2P").unwrap(), "1");
    assert!(std::env::var_os("MLUVA_WORKFLOW_NO_KEY").is_none());
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-recording-targets.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    assert_eq!(
        fixture["gtk_version"],
        format!(
            "{}.{}.{}",
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        )
    );
    gtk::init().unwrap();
    let application = gtk::Application::new(
        Some("org.example.Mluva.RecordingClient"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    application.register(gio::Cancellable::NONE).unwrap();
    let own = gtk::ApplicationWindow::builder()
        .application(&application)
        .title("Private recording client")
        .build();
    let own_entry = gtk::Entry::new();
    own.set_child(Some(&own_entry));
    own.present();
    own_entry.grab_focus();
    settle(Duration::from_millis(400));
    own.set_visible(false);
    settle(Duration::from_millis(200));
    let mut peer = Peer::new(root.join("native-recording-target"), "text_target_peer");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mode = case["mode"].as_str().unwrap_or("dictation");
        peer.request(json!({"operation":"button"}));
        let mut tracker = FocusedTextTargetTracker::new().unwrap();
        let mut setup = case["setup"].clone();
        setup["operation"] = json!("setup");
        peer.request(setup);
        let monitor = Monitor::new(peer.directory.join(format!("monitor-{name}.log")));
        let (target, selected) = if mode == "command" {
            let text = tracker
                .capture_text_target(2000)
                .unwrap()
                .expect("explicit Command capture");
            let selected = text.selected_text().map(str::to_owned);
            (DeliveryTargetSnapshot::Text(text), selected)
        } else {
            (
                tracker
                    .capture_delivery_target()
                    .expect("content-free recording target"),
                None,
            )
        };
        if case["own_focus"] == true {
            own.present();
            own_entry.grab_focus();
            settle(Duration::from_millis(300));
        }
        let directory = tempfile::tempdir_in(&root).unwrap();
        let data = directory.path().join("data");
        let recordings = data.join("recordings");
        fs::create_dir_all(&recordings).unwrap();
        let audio = recordings.join("capture.wav");
        fs::write(
            &audio,
            STANDARD.decode(fixture["audio"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        let personal = data.join("personalization.json");
        fs::write(&personal, serde_json::to_vec(&json!({"schemaVersion":1,"dictionary":[{"id":"11111111-1111-4111-8111-111111111111","spoken":"cue wen","written":"Qwen3","bundleIdentifier":null,"caseBehavior":"fixed"}]})).unwrap()).unwrap();
        let history = HistoryStore::new(data.join("history.sqlite3"));
        history.initialize().unwrap();
        let diagnostics = DiagnosticsStore::new(data.join("diagnostics.sqlite3"), 5000).unwrap();
        diagnostics.initialize().unwrap();
        let mut http = http::Peer::new(case["responses"].as_array().unwrap());
        let config = AppConfig {
            transcription_provider: "elevenlabs".into(),
            rewrite_provider: "litellm".into(),
            litellm_model: Some("rewrite-fixture".into()),
            litellm_base_url: http.address.clone(),
            litellm_api_key_env: "MLUVA_WORKFLOW_NO_KEY".into(),
            auto_copy_dictation: true,
            auto_paste: true,
            spoken_commands_enabled: true,
            ..Default::default()
        };
        let speech = SpeechClient::ElevenLabs(
            ElevenLabsClient::new(
                Secret::new("synthetic-key"),
                &format!("{}/speech-to-text", http.address),
                Duration::from_secs(2),
            )
            .unwrap(),
        );
        let rewrite = RewriteClient::new(&config, None, Some(Duration::from_secs(2))).unwrap();
        let mut workflow =
            DictationWorkflow::new(config, speech, rewrite, history, directory.path().into());
        workflow.personalization = Some(PersonalizationStore::new(personal));
        workflow.diagnostics = Some(diagnostics);
        clipboard(Some("clipboard canary"));
        let completed = runtime
            .block_on(
                workflow.complete(
                    &audio,
                    Completion {
                        mode: mode.into(),
                        allow_auto_paste: true,
                        incognito: case["incognito"] == true,
                        selected_text: selected,
                        session_identifier: Some("aaaabbbb-cccc-4ddd-8eee-ffffffffffff".into()),
                        application_identifier: target.application_identifier().map(str::to_owned),
                        delivery_target: (mode == "dictation")
                            .then_some(&target as &dyn mluva_workflows::dictation::DeliveryTarget),
                        ..Default::default()
                    },
                ),
            )
            .unwrap();
        settle(Duration::from_millis(200));
        let mut state = peer.observed();
        assert_ne!(
            state["pid"].as_u64().unwrap(),
            u64::from(std::process::id())
        );
        state.as_object_mut().unwrap().remove("serial");
        state.as_object_mut().unwrap().remove("pid");
        let entries = workflow.history.recent(100).unwrap().into_iter().map(|entry| {
            assert_eq!(entry.application_identifier.as_deref(),peer.executable.to_str());
            json!({"raw_text":entry.raw_text,"delivered_text":entry.delivered_text,"mode":entry.mode,"delivery_outcome":entry.delivery_outcome,"recognition_route":entry.recognition_route,"enhancement_provider_id":entry.enhancement_provider_id,"enhancement_model_identifier":entry.enhancement_model_identifier,"enhancement_context_sources":entry.enhancement_context_sources,"enhancement_outcome":entry.enhancement_outcome,"application_identifier":"$TARGET"})
        }).collect::<Vec<_>>();
        let reads = monitor.finish();
        if mode == "command" {
            assert_eq!(
                reads.as_array().unwrap().len(),
                1,
                "positive disclosure control"
            );
        }
        let actual = json!({"output_text":completed.output_text,"delivery":{"copied":completed.delivery.copied,"pasted":completed.delivery.pasted,"guidance":completed.delivery.guidance,"paste_dispatched":completed.delivery.paste_dispatched,"paste_confirmed":completed.delivery.paste_confirmed},"history":entries,"requires_acceptance":completed.requires_acceptance,"audio_exists":audio.exists(),"clipboard":clipboard(None),"target":state,"text_reads":reads,"wire":http.finish(),"diagnostics":workflow.diagnostics.as_ref().unwrap().recent(100).unwrap().into_iter().rev().map(|event| json!({"stage":event.stage,"provider":event.provider,"outcome":event.outcome})).collect::<Vec<_>>()});
        runtime.block_on(workflow.close());
        tracker.close();
        write(
            &peer.directory.join(format!("workflow-{name}.json")),
            &actual,
        );
        assert_eq!(actual, case["observed"], "released joined workflow: {name}");
        println!("accepted GTK workflow: {name}");
        own.set_visible(false);
        settle(Duration::from_millis(100));
    }
    peer.request(json!({"operation":"quit"}));
    peer.await_successful_exit();
    own.close();
}
