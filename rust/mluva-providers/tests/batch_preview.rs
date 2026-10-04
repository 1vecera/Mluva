use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_providers::{
    batch_preview::{BatchPreviewClient, BatchPreviewSession},
    compatible::CompatibleClient,
    speech::SpeechClient,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;

fn client(
    address: &str,
    directory: &std::path::Path,
    enabled: bool,
) -> (BatchPreviewClient, Arc<AtomicUsize>) {
    let address = address.to_owned();
    let calls = Arc::new(AtomicUsize::new(0));
    let created = calls.clone();
    (
        BatchPreviewClient {
            factory: Arc::new(move || {
                let address = address.clone();
                created.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    Ok(Arc::new(SpeechClient::Compatible(CompatibleClient::new(
                        &address,
                        "MLUVA_SYNTHETIC_PREVIEW_EMPTY_KEY",
                        Some("synthetic-speech".into()),
                        Duration::from_secs(3),
                    )?)))
                })
            }),
            directory: directory.to_owned(),
            chunk_seconds: 3,
            preview_enabled: enabled,
        },
        calls,
    )
}
fn observe(session: &BatchPreviewSession) -> Value {
    json!({"preview":session.snapshot(),"healthy":session.is_healthy(),"bytes_sent":session.bytes_sent()})
}
async fn until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(4), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("The real preview request did not finish");
}
fn empty(directory: &std::path::Path) -> bool {
    !directory.exists() || std::fs::read_dir(directory).unwrap().next().is_none()
}

#[tokio::test]
async fn preview_and_authoritative_audio_match_unchanged_released_sessions() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-batch-preview.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    for row in fixture["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let staging = directory.path().join("previews");
        let mut peer = http::Peer::new(row["responses"].as_array().unwrap());
        let (client, calls) = client(
            &peer.address,
            &staging,
            row["enabled"].as_bool().unwrap_or(true),
        );
        let session = client.start("auto");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!staging.exists());
        let mut observations = vec![];
        for action in row["actions"].as_array().unwrap() {
            if let Some(size) = action["submit"].as_u64() {
                session.submit_audio(&vec![0; size as usize]);
            }
            if let Some(enabled) = action["enabled"].as_bool() {
                session.set_preview_enabled(enabled);
            }
            if action["cancel"] == true {
                session.cancel();
            }
            if let Some(text) = action["wait"].as_str() {
                until(|| session.snapshot().committed_text == text).await;
            }
            if action["wait_unhealthy"] == true {
                until(|| !session.is_healthy()).await;
            }
            observations.push(observe(&session));
        }
        assert_eq!(json!(observations), row["observations"], "{}", row["name"]);
        let terminal = if row["no_finish"] == true {
            Value::Null
        } else {
            match session.finish().await {
                Ok(result) => json!({"ok":result.transcription}),
                Err(error) => json!({"error":error.to_string()}),
            }
        };
        assert_eq!(terminal, row["terminal"], "{}", row["name"]);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            row["factory_calls"].as_u64().unwrap() as usize
        );
        tokio::time::timeout(Duration::from_secs(3), session.cancel_and_wait())
            .await
            .unwrap();
        assert!(empty(&staging), "{} retained a WAV", row["name"]);
        let mut wire = peer.finish();
        for request in &mut wire {
            let raw = STANDARD
                .decode(
                    request["fields"]
                        .as_object_mut()
                        .unwrap()
                        .remove("file")
                        .unwrap()
                        .as_str()
                        .unwrap(),
                )
                .unwrap();
            request["file_sha256"] = json!(
                Sha256::digest(&raw)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            );
            request["file_bytes"] = json!(raw.len());
        }
        assert_eq!(json!(wire), row["wire"], "{}", row["name"]);
    }
}

#[tokio::test]
async fn dropped_final_request_and_active_preview_cancel_erase_private_audio() {
    for preview in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let staging = directory.path().join("previews");
        let mut peer = http::Peer::new(&[
            json!({"route":"speech","status":200,"delay_ms":350,"allow_disconnect":true,"payload":{"text":"late text","language_code":"eng"}}),
        ]);
        let (client, calls) = client(&peer.address, &staging, true);
        let session = client.start("auto");
        session.submit_audio(&vec![0; if preview { 96000 } else { 32000 }]);
        if preview {
            until(|| !peer.observed.lock().unwrap().is_empty()).await;
        } else {
            tokio::select! {
                () = until(|| !peer.observed.lock().unwrap().is_empty()) => {},
                result = session.finish() => panic!("Delayed request escaped: {result:?}"),
            }
        }
        let temporary = std::fs::read_dir(&staging)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::metadata(&staging).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&temporary).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(temporary.join("audio.wav"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        tokio::time::timeout(Duration::from_secs(3), session.cancel_and_wait())
            .await
            .unwrap();
        assert!(empty(&staging));
        assert!(!session.is_healthy());
        assert!(session.snapshot().committed_text.is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(peer.finish().len(), 1);
    }
}
