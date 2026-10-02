//! Actual native Scribe diarization and archive/retry policy against frozen released transactions.

use chrono::{DateTime, Utc};
use mluva_audio::meeting::MeetingCaptureResult;
use mluva_core::{config::AppConfig, meeting::MeetingStore};
use mluva_providers::{Secret, elevenlabs::ElevenLabsClient};
use mluva_workflows::meeting::{
    MeetingCompletion, MeetingError, MeetingWorkflow, MeetingWorkflowResult,
};
use serde_json::{Value, json};
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
#[path = "support/http.rs"]
mod http;
#[path = "../../mluva-core/tests/support/meeting.rs"]
mod support;

const ID: &str = "11111111-1111-4111-8111-111111111111";
fn result(value: Result<MeetingWorkflowResult, MeetingError>, root: &std::path::Path) -> Value {
    match value {
        Ok(value) => {
            json!({"kind":"completed","meeting":value.meeting.document(),"transcription":value.transcription,"incognito":value.incognito})
        }
        Err(MeetingError::Failed(error)) => {
            json!({"kind":"failed","message":error.message,"stage":error.stage.label(),
            "meeting":error.meeting.map(|value|value.document()),"retained_audio_path":error.retained_audio_path.map(|path|path.to_string_lossy().replace(root.to_str().unwrap(), "$ROOT"))})
        }
        Err(MeetingError::Invalid(error)) => {
            json!({"kind":"invalid","error":support::kind(&error)})
        }
    }
}
#[tokio::test]
async fn released_meeting_diarization_archive_and_explicit_retry_match() {
    let reference = support::fixture();
    let mut states = 0;
    for row in reference["workflows"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = root.join("archive/meetings.json");
        support::setup(
            root,
            &json!({"files":{"capture.wav":"synthetic finalized meeting audio"}}),
        );
        if row["initial"]["malformed"] == true {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "{broken private archive").unwrap();
        }
        if row["initial"]["collision"] == true {
            let audio = root.join(format!("archive/recordings/{ID}.wav"));
            fs::create_dir_all(audio.parent().unwrap()).unwrap();
            fs::write(audio, "keep existing audio").unwrap();
        }
        let mut peer = http::Peer::new(row["responses"].as_array().unwrap());
        let mut config = serde_json::to_value(AppConfig::default()).unwrap();
        for (key, value) in row["settings"].as_object().unwrap() {
            config[key] = value.clone();
        }
        let archive = Arc::new(Mutex::new(MeetingStore::new(path.clone(), None)));
        let workflow = MeetingWorkflow {
            config: serde_json::from_value(config).unwrap(),
            store: archive.clone(),
            client: Arc::new(
                ElevenLabsClient::new(
                    Secret::new("synthetic-meeting-key"),
                    &format!("{}/speech-to-text", peer.address),
                    Duration::from_secs(5),
                )
                .unwrap(),
            ),
        };
        for stage in row["actions"].as_array().unwrap() {
            let action = &stage["action"];
            let value = match action["op"].as_str().unwrap() {
                "complete" => {
                    let sources = action
                        .get("sources")
                        .map(|value| {
                            value
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|value| match value.as_str().unwrap() {
                                    "microphone" => "microphone",
                                    "system" => "system",
                                    "loopback" => "loopback",
                                    other => panic!("unknown fixture audio source {other}"),
                                })
                                .collect()
                        })
                        .unwrap_or_else(|| vec!["microphone", "system"]);
                    let warnings = action.get("warnings").map(|value|value.as_array().unwrap().iter().map(|value| {
                        match value.as_str().unwrap() {
                            "System audio was unavailable; this meeting contains microphone audio only." => "System audio was unavailable; this meeting contains microphone audio only.",
                            other => panic!("unknown fixture warning {other}"),
                        }
                    }).collect()).unwrap_or_default();
                    let capture = MeetingCaptureResult {
                        path: root.join("capture.wav"),
                        audio_sources: sources,
                        warnings,
                        duration_seconds: action["duration"].as_f64().unwrap_or(2.5),
                    };
                    result(
                        workflow
                            .complete(
                                capture,
                                MeetingCompletion {
                                    incognito: action["incognito"] == true,
                                    identifier: Some(action["id"].as_str().unwrap_or(ID).into()),
                                    started_at: Some(
                                        DateTime::parse_from_rfc3339("2026-09-28T12:34:56.123456Z")
                                            .unwrap()
                                            .with_timezone(&Utc),
                                    ),
                                },
                            )
                            .await,
                        root,
                    )
                }
                "retry" => result(workflow.retry(ID).await, root),
                "write_failure" => {
                    support::write_failure(&path);
                    Value::Null
                }
                "remove_audio" => {
                    let archive = archive.lock().unwrap();
                    fs::remove_file(archive.recording_path(archive.find(ID).unwrap()).unwrap())
                        .unwrap();
                    Value::Null
                }
                other => panic!("unknown Meeting action {other}"),
            };
            let label = format!("{} {action}", row["name"]);
            support::assert_value(&value, &stage["result"], &label);
            support::assert_value(
                &support::observe(&archive.lock().unwrap(), root),
                &stage["observed"],
                &label,
            );
            assert_eq!(
                *peer.observed.lock().unwrap(),
                stage["requests"].as_array().unwrap().clone(),
                "{label}"
            );
            states += 1;
        }
        workflow.close();
        peer.finish();
    }
    println!(
        "Matched {} actual HTTP Meeting workflows/{states} archive/result states",
        reference["workflows"].as_array().unwrap().len()
    );
}

#[tokio::test]
async fn dropping_inflight_incognito_meeting_erases_owned_audio_without_index_write() {
    let reference = support::fixture();
    let mut response = reference["workflows"][0]["responses"][0].clone();
    response["delay_ms"] = json!(200);
    response["allow_disconnect"] = json!(true);
    let mut peer = http::Peer::new(&[response]);
    let directory = tempfile::tempdir().unwrap();
    let audio = directory.path().join("private.wav");
    fs::write(&audio, b"synthetic in-memory meeting audio").unwrap();
    let store = Arc::new(Mutex::new(MeetingStore::new(
        directory.path().join("archive/meetings.json"),
        None,
    )));
    let workflow = Arc::new(MeetingWorkflow {
        config: AppConfig::default(),
        store: store.clone(),
        client: Arc::new(
            ElevenLabsClient::new(
                Secret::new("synthetic-meeting-key"),
                &format!("{}/speech-to-text", peer.address),
                Duration::from_secs(5),
            )
            .unwrap(),
        ),
    });
    let owner = workflow.clone();
    let capture = MeetingCaptureResult {
        path: audio.clone(),
        audio_sources: vec!["microphone"],
        warnings: vec![],
        duration_seconds: 1.0,
    };
    let task = tokio::spawn(async move {
        owner
            .complete(
                capture,
                MeetingCompletion {
                    incognito: true,
                    identifier: Some(ID.into()),
                    started_at: None,
                },
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        while peer.observed.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    assert!(audio.exists());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(!audio.exists());
    assert!(store.lock().unwrap().meetings().is_empty());
    assert!(!directory.path().join("archive/meetings.json").exists());
    workflow.close();
    peer.finish();
}

fn audio_peer(config: &Value) -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("test-config.json");
    fs::write(&path, serde_json::to_vec(config).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    let executable = root.path().join("pw-record");
    symlink(
        env!("CARGO_BIN_EXE_meeting-audio-fixture-peer"),
        &executable,
    )
    .unwrap();
    (root, executable)
}
async fn capture_pids(root: &std::path::Path) -> Vec<u64> {
    let mut pids = vec![];
    for name in ["microphone", "system"] {
        let path = root.join(format!("{name}.ready.json"));
        let ready: Value = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(bytes) = fs::read(&path)
                    && let Ok(value) = serde_json::from_slice(&bytes)
                {
                    break value;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        pids.push(ready["pid"].as_u64().unwrap());
    }
    pids
}
fn session(
    config: AppConfig,
    store: Arc<Mutex<MeetingStore>>,
    audio_executable: &std::path::Path,
    address: &str,
    incognito: bool,
) -> Arc<mluva_workflows::meeting_session::MeetingSession> {
    use mluva_audio::{capture::CaptureStorage, meeting::PipeWireMeetingRecorder};
    use mluva_workflows::meeting_session::MeetingSession;
    let storage = if incognito {
        CaptureStorage::Incognito {
            cleanup_executable: env!("CARGO_BIN_EXE_meeting-cleanup-fixture-peer").into(),
            memory_root: None,
        }
    } else {
        CaptureStorage::Persistent(store.lock().unwrap().recordings_directory.clone())
    };
    MeetingSession::new(
        MeetingWorkflow {
            config,
            store,
            client: Arc::new(
                ElevenLabsClient::new(
                    Secret::new("synthetic-meeting-key"),
                    &format!("{address}/speech-to-text"),
                    Duration::from_secs(5),
                )
                .unwrap(),
            ),
        },
        PipeWireMeetingRecorder::new(
            audio_executable,
            Some("configured.microphone".into()),
            Some("configured.output".into()),
        ),
        storage,
        MeetingCompletion {
            incognito,
            identifier: Some(ID.into()),
            started_at: Some(
                DateTime::parse_from_rfc3339("2026-09-28T12:34:56.123456Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
        },
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn joined_meeting_recording_recognition_and_archive_match_released_transactions() {
    use mluva_workflows::meeting_session::{MeetingPhase, MeetingSessionError};
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-meeting-session.json")).unwrap();
    for row in fixture["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let path = root.join("archive/meetings.json");
        let store = Arc::new(Mutex::new(MeetingStore::new(path.clone(), None)));
        let (audio_root, executable) = audio_peer(&row["audio"]["config"]);
        let mut peer = http::Peer::new(row["responses"].as_array().unwrap());
        let owner = session(
            AppConfig {
                language_code: "ces".into(),
                ..AppConfig::default()
            },
            store.clone(),
            &executable,
            &peer.address,
            row["incognito"] == true,
        );
        let audio = owner.start().await.unwrap();
        assert_eq!(owner.phase(), MeetingPhase::Recording);
        let pids = capture_pids(audio_root.path()).await;
        if row["index_failure"] == true {
            fs::create_dir(&path).unwrap();
            fs::write(path.join("preserved"), "blocked index").unwrap();
        }
        let observed = if row["cancel"] == true {
            owner.close().await.unwrap();
            json!({"kind":"cancelled"})
        } else {
            match owner.finish().await {
                Ok(value) => result(Ok(value), root),
                Err(MeetingSessionError::Workflow(error)) => result(Err(error), root),
                Err(MeetingSessionError::Audio(error)) => {
                    json!({"kind":"audio-error","message":error.to_string()})
                }
                Err(error) => panic!("unexpected joined Meeting result: {error}"),
            }
        };
        owner.close().await.unwrap();
        owner.close().await.unwrap();
        assert_eq!(owner.phase(), MeetingPhase::Closed);
        support::assert_value(&observed, &row["result"], row["name"].as_str().unwrap());
        support::assert_value(
            &support::observe(&store.lock().unwrap(), root),
            &row["observed"],
            row["name"].as_str().unwrap(),
        );
        assert_eq!(peer.finish(), row["requests"].as_array().unwrap().clone());
        if row["incognito"] == true {
            assert!(
                !audio
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .exists()
            );
        }
        for pid in pids {
            assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        }
    }
}

#[tokio::test]
async fn meeting_shutdown_waits_for_finalization_or_provider_and_prevents_late_publication() {
    use mluva_workflows::meeting_session::{MeetingPhase, MeetingSessionError};
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-meeting-session.json")).unwrap();
    for private in [false, true] {
        for during_http in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let mut audio_config = fixture["cases"][0]["audio"]["config"].clone();
            if !during_http {
                for name in ["microphone", "system"] {
                    audio_config[name]["finalize_delay_ms"] = json!(100);
                }
            }
            let (audio_root, executable) = audio_peer(&audio_config);
            let responses = if during_http {
                let mut response = fixture["cases"][0]["responses"][0].clone();
                response["delay_ms"] = json!(200);
                response["allow_disconnect"] = json!(true);
                vec![response]
            } else {
                vec![]
            };
            let mut peer = http::Peer::new(&responses);
            let store = Arc::new(Mutex::new(MeetingStore::new(
                root.path().join("archive/meetings.json"),
                None,
            )));
            let owner = session(
                AppConfig::default(),
                store.clone(),
                &executable,
                &peer.address,
                private,
            );
            let audio = owner.start().await.unwrap();
            let pids = capture_pids(audio_root.path()).await;
            let task_owner = owner.clone();
            let finish = tokio::spawn(async move { task_owner.finish().await });
            if during_http {
                tokio::time::timeout(Duration::from_secs(3), async {
                    while peer.observed.lock().unwrap().is_empty() {
                        tokio::time::sleep(Duration::from_millis(2)).await;
                    }
                })
                .await
                .unwrap();
            } else {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            owner.close().await.unwrap();
            assert!(matches!(
                finish.await.unwrap(),
                Err(MeetingSessionError::Cancelled)
            ));
            assert_eq!(owner.phase(), MeetingPhase::Closed);
            assert!(!audio.exists());
            assert!(store.lock().unwrap().meetings().is_empty());
            assert!(!root.path().join("archive/meetings.json").exists());
            if private {
                assert!(
                    !audio
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .exists()
                );
            }
            for pid in pids {
                assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
            }
            peer.finish();
            assert_eq!(owner.phase(), MeetingPhase::Closed);
            assert!(store.lock().unwrap().meetings().is_empty());
            assert!(!root.path().join("archive/meetings.json").exists());
        }
    }
}
