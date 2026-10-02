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
