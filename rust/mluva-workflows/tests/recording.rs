//! Independently released observations across real HTTP, SQLite and clipboard boundaries.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mluva_core::{
    config::AppConfig,
    diagnostics::DiagnosticsStore,
    history::{HistoryEntry, HistoryStore},
    personalization::{DictionaryCaseBehavior, PersonalizationStore},
    screenshots::ImageInput,
};
use mluva_providers::{
    Secret, credentials::CredentialStore, elevenlabs::ElevenLabsClient, local_asr::OnnxOptions,
    rewriting::RewriteClient, speech::SpeechClient,
};
use mluva_workflows::{
    dictation::{Completion, DictationWorkflow, WorkflowError, WorkflowResult},
    preparation::reprocess_history_entry,
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

#[path = "support/http.rs"]
mod http;
use http::Peer;

fn completion(mut value: Value) -> Completion<'static> {
    for (collection, fields) in [
        (
            "dictionary_replacements",
            vec![
                ("identifier", "id"),
                ("application_identifier", "bundleIdentifier"),
                ("case_behavior", "caseBehavior"),
            ],
        ),
        (
            "snippets",
            vec![
                ("identifier", "id"),
                ("application_identifier", "bundleIdentifier"),
                ("typed_trigger", "typedTrigger"),
            ],
        ),
    ] {
        if let Some(items) = value
            .get_mut("transcript_preparation")
            .and_then(|preparation| preparation.get_mut(collection))
            .and_then(Value::as_array_mut)
        {
            for item in items {
                let object = item.as_object_mut().unwrap();
                for (source, native) in &fields {
                    let field = object.remove(*source).unwrap();
                    object.insert((*native).into(), field);
                }
            }
        }
    }
    if let Some(pairs) = value["transcript_preparation"]["variables"].as_array() {
        let object = pairs
            .iter()
            .map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].clone()))
            .collect::<serde_json::Map<_, _>>();
        value["transcript_preparation"]["variables"] = Value::Object(object);
    }
    let mut request = Completion::default();
    macro_rules! assign { ($($field:ident),* $(,)?) => { $(if let Some(value) = value.get(stringify!($field)) { request.$field = serde_json::from_value(value.clone()).unwrap(); })* }; }
    assign!(
        mode,
        use_codex_cleanup,
        allow_auto_paste,
        incognito,
        audio_retention_policy,
        selected_text,
        session_identifier,
        application_identifier,
        style_identifier,
        use_saved_style,
        recognized_transcription,
        recognition_duration_seconds,
        recognition_used_batch_fallback,
        recognition_fallback_reason,
        codex_model_identifier,
        transcript_preparation,
        segment_cleanup,
        frozen_style,
        style_is_frozen,
        defer_delivery
    );
    if let Some(images) = value["images"].as_array() {
        request.images = images
            .iter()
            .map(|image| ImageInput {
                data: STANDARD.decode(image["data"].as_str().unwrap()).unwrap(),
                captured_after_seconds: image["captured_after_seconds"].as_f64(),
            })
            .collect();
    }
    request
}
fn entry(entry: Option<&HistoryEntry>) -> Value {
    let Some(entry) = entry else {
        return Value::Null;
    };
    let mut value = serde_json::to_value(entry).unwrap();
    value["identifier"] = json!("generated");
    value["created_at"] = json!("generated");
    if !value["retained_audio_path"].is_null() {
        value["retained_audio_path"] = json!("$AUDIO");
    }
    for key in ["recognition_ms", "enhancement_ms", "delivery_ms"] {
        if !value[key].is_null() {
            value[key] = json!("recorded");
        }
    }
    value
}
fn result(result: WorkflowResult) -> Value {
    json!({
        "kind":"completed", "transcription":result.transcription, "output_text":result.output_text,
        "delivery": {"copied":result.delivery.copied,"pasted":result.delivery.pasted,"guidance":result.delivery.guidance,"paste_dispatched":result.delivery.paste_dispatched,"paste_confirmed":result.delivery.paste_confirmed},
        "history_entry":entry(result.history_entry.as_ref()), "retained_audio_path":result.retained_audio_path.as_ref().map(|_| "$AUDIO"),
        "requires_acceptance":result.requires_acceptance,"incognito":result.incognito,"mode":result.mode,
        "recognition_ms":if result.recognition_route == "scribe-v2-realtime" { json!(result.recognition_ms) } else { json!("recorded") },
        "enhancement_ms":"recorded","delivery_ms":"recorded","session_identifier":result.session_identifier,
        "recognition_fallback":result.recognition_fallback,"recognition_route":result.recognition_route,"recognition_fallback_reason":result.recognition_fallback_reason,
    })
}
fn clipboard(text: Option<&str>) -> String {
    let mut command = Command::new("xclip");
    command.args(["-selection", "clipboard"]);
    if let Some(text) = text {
        let mut child = command.stdin(Stdio::piped()).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
        String::new()
    } else {
        let output = command.arg("-o").output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }
}
fn isolated() {
    let root = fs::canonicalize(
        std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private desktop required"),
    )
    .unwrap();
    for variable in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            fs::canonicalize(std::env::var_os(variable).unwrap())
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
    assert_eq!(std::env::var("XDG_SESSION_TYPE").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    assert!(std::env::var_os("MLUVA_WORKFLOW_NO_KEY").is_none());
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires dev/run-isolated-browser.sh with a private real clipboard and loopback network"]
async fn recording_routes_delivery_privacy_and_recovery_match_the_released_workflow() {
    isolated();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-recording-workflows.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let directory =
            tempfile::tempdir_in(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap()).unwrap();
        let data = directory.path().join("data");
        let recordings = data.join("recordings");
        fs::create_dir_all(&recordings).unwrap();
        let audio = recordings.join("capture.wav");
        fs::write(
            &audio,
            STANDARD.decode(fixture["audio"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        let personal_path = data.join("personalization.json");
        fs::write(
            &personal_path,
            serde_json::to_vec(&case["personalization"]).unwrap(),
        )
        .unwrap();
        let personal =
            (case["no_personalization"] != true).then(|| PersonalizationStore::new(&personal_path));
        let history = HistoryStore::new(data.join("history.sqlite3"));
        if case["history_uninitialized"] != true {
            history.initialize().unwrap();
        }
        let diagnostics = DiagnosticsStore::new(data.join("diagnostics.sqlite3"), 5_000).unwrap();
        if case["diagnostics_uninitialized"] != true {
            diagnostics.initialize().unwrap();
        }
        let mut peer = Peer::new(case["responses"].as_array().unwrap());
        let mut config: AppConfig = serde_json::from_value(case["config"].clone()).unwrap();
        config.litellm_base_url = peer.address.clone();
        config.litellm_api_key_env = "MLUVA_WORKFLOW_NO_KEY".into();
        config.transcription_base_url = peer.address.clone();
        config.transcription_api_key_env = "MLUVA_WORKFLOW_NO_KEY".into();
        let speech = if config.transcription_provider == "elevenlabs" {
            SpeechClient::ElevenLabs(
                ElevenLabsClient::new(
                    Secret::new("synthetic-key"),
                    &format!("{}/speech-to-text", peer.address),
                    Duration::from_secs(2),
                )
                .unwrap(),
            )
        } else {
            SpeechClient::new(
                &config,
                OnnxOptions::new(
                    &data,
                    &config.local_model,
                    directory.path().join("no-worker"),
                ),
                &CredentialStore::new(),
            )
            .await
            .unwrap()
        };
        let rewrite = RewriteClient::new(&config, None, Some(Duration::from_secs(2))).unwrap();
        let mut workflow =
            DictationWorkflow::new(config, speech, rewrite, history, directory.path().into());
        workflow.personalization = personal;
        workflow.diagnostics = Some(diagnostics);
        let mut request = completion(case["request"].clone());
        if case["freeze"] == true {
            let mut captured = workflow
                .freeze_transcript_preparation(
                    case["freeze_mode"].as_str().unwrap_or(&request.mode),
                    request.application_identifier.as_deref(),
                )
                .unwrap();
            let released = request.transcript_preparation.as_ref().unwrap();
            assert_eq!(
                captured.dictionary_replacements,
                released.dictionary_replacements
            );
            assert_eq!(captured.snippets, released.snippets);
            assert_eq!(captured.protected_vocabulary, released.protected_vocabulary);
            if case["variable_snippet"] == true {
                captured.variables = released.variables.clone();
            }
            request.transcript_preparation = Some(captured);
        }
        if case["edit_after_freeze"] == true {
            let store = workflow.personalization.as_mut().unwrap();
            store
                .save_dictionary_replacement(
                    "cue wen",
                    "ChangedQwen",
                    None,
                    DictionaryCaseBehavior::Fixed,
                )
                .unwrap();
            store
                .save_snippet("my signature", "ChangedName", None, None)
                .unwrap();
        }
        clipboard(Some("clipboard canary"));
        let mut observed = match workflow.complete(&audio, request).await {
            Ok(completed) => result(completed),
            Err(WorkflowError::Failure(error)) => {
                json!({"kind":"failure","message":error.message,"stage":error.stage,"output_text":error.output_text,"history_entry":entry(error.history_entry.as_ref()),"retained_audio_path":error.retained_audio_path.as_ref().map(|_| "$AUDIO")})
            }
            Err(error) => json!({"kind":"error","message":error.to_string()}),
        };
        if case["retry"] == true {
            observed["retry"] = entry(Some(
                &workflow
                    .retry_recognition(&workflow.history.recent(1).unwrap()[0].identifier)
                    .await
                    .unwrap(),
            ));
        }
        if case["reprocess"] == true {
            workflow
                .personalization
                .as_mut()
                .unwrap()
                .save_dictionary_replacement(
                    "cue wen",
                    "ChangedQwen",
                    None,
                    DictionaryCaseBehavior::Fixed,
                )
                .unwrap();
            observed["reprocess"] = entry(Some(
                &reprocess_history_entry(
                    &workflow.config,
                    &workflow.history,
                    workflow.personalization.as_ref(),
                    &workflow.history.recent(1).unwrap()[0].identifier,
                )
                .unwrap(),
            ));
        }
        observed["audio_exists"] = json!(audio.exists());
        observed["clipboard"] = json!(clipboard(None));
        observed["history"] = json!(if case["history_uninitialized"] == true {
            vec![]
        } else {
            workflow
                .history
                .recent(100)
                .unwrap()
                .iter()
                .map(|saved| entry(Some(saved)))
                .collect::<Vec<_>>()
        });
        observed["diagnostics"] = json!(if case["diagnostics_uninitialized"] == true {
            vec![]
        } else {
            workflow.diagnostics.as_ref().unwrap().recent(100).unwrap().into_iter().rev().map(|event| json!({"mode":event.mode,"stage":event.stage,"provider":event.provider,"outcome":event.outcome})).collect::<Vec<_>>()
        });
        workflow.close().await;
        observed["wire"] = json!(peer.finish());
        let evidence = std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap();
        fs::write(
            Path::new(&evidence).join(format!("workflow-{name}.json")),
            serde_json::to_vec_pretty(&observed).unwrap(),
        )
        .unwrap();
        assert_eq!(observed, case["observed"], "released workflow case: {name}");
        println!("accepted workflow: {name}");
    }
}
